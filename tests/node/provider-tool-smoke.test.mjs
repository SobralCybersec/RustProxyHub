import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { mkdtemp, readFile, rm } from 'node:fs/promises'
import http from 'node:http'
import { once } from 'node:events'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import test from 'node:test'
import {
  buildToolRequest,
  discoverProviderLogsEndpoint,
  limitModelsPerProvider,
  parseCliArgs,
  parseSse,
  runProviderToolSmoke,
  selectProviderModels,
  summarizeSse,
} from '../../scripts/provider-tool-smoke.mjs'
import { INTERACTION_TEXT } from '../../scripts/client-interaction-smoke.mjs'

const ALL_PROVIDERS = [
  ['chatgpt', 'gpt-test'],
  ['deepseek', 'deepseek-chat'],
  ['gemini', 'gemini-test'],
  ['kimi', 'kimi-k2'],
  ['meta', 'llama-test'],
  ['mistral', 'mistral-test'],
  ['qwen', 'qwen3'],
  ['zai', 'glm-test'],
]

function startTemporaryHub() {
  const requests = []
  const server = http.createServer(async (request, response) => {
    const url = new URL(request.url, 'http://hub.test')
    const body = await new Promise((resolve, reject) => {
      let text = ''
      request.setEncoding('utf8')
      request.on('data', chunk => {
        text += chunk
      })
      request.on('end', () => resolve(text))
      request.on('error', reject)
    })
    requests.push({ body, headers: request.headers, path: url.pathname })

    if (url.pathname === '/') {
      response.setHeader('content-type', 'application/json')
      response.end(JSON.stringify({ routes: { provider_logs: '/providers/{provider}/logs' } }))
      return
    }
    if (url.pathname === '/openapi.json') {
      response.setHeader('content-type', 'application/json')
      response.end(JSON.stringify({ paths: {} }))
      return
    }
    if (url.pathname === '/v1/models') {
      response.setHeader('content-type', 'application/json')
      response.end(JSON.stringify({ data: ALL_PROVIDERS.map(([provider, id]) => ({ id, provider })) }))
      return
    }
    if (url.pathname === '/v1/chat/completions') {
      const payload = JSON.parse(body)
      const [provider, model] = payload.model.split(':')
      response.writeHead(200, { 'content-type': 'text/event-stream' })
      if (payload.messages.some(message => message.role === 'tool')) {
        response.end(
          [
            `data: ${JSON.stringify({ choices: [{ delta: { content: INTERACTION_TEXT } }] })}`,
            '',
            `data: ${JSON.stringify({ choices: [{ delta: {}, finish_reason: 'stop' }] })}`,
            '',
            'data: [DONE]',
            '',
          ].join('\n')
        )
        return
      }
      response.end(
        [
          `data: ${JSON.stringify({ choices: [{ delta: { tool_calls: [{ index: 0, id: `call_${provider}`, type: 'function', function: { name: 'report_smoke_target', arguments: JSON.stringify({ provider, model }) } }] }, finish_reason: 'tool_calls' }] })}`,
          '',
          'data: [DONE]',
          '',
        ].join('\n')
      )
      return
    }
    if (url.pathname === '/v1/messages') {
      response.writeHead(200, { 'content-type': 'text/event-stream' })
      response.end(
        [
          'event: content_block_delta',
          `data: ${JSON.stringify({ delta: { text: INTERACTION_TEXT } })}`,
          '',
          'event: message_stop',
          'data: {}',
          '',
        ].join('\n')
      )
      return
    }
    if (/^\/providers\/[^/]+\/logs$/.test(url.pathname)) {
      const provider = url.pathname.split('/')[2]
      response.setHeader('content-type', 'application/json')
      response.end(JSON.stringify({ entries: [`${provider} tool-call recorded`] }))
      return
    }
    response.writeHead(404).end()
  })

  return new Promise((resolve, reject) => {
    server.once('error', reject)
    server.listen(0, '127.0.0.1', () => {
      const address = server.address()
      resolve({
        close: () => new Promise(closeResolve => server.close(closeResolve)),
        requests,
        url: `http://127.0.0.1:${address.port}`,
      })
    })
  })
}

function runCli(scriptName, env, args = []) {
  const script = fileURLToPath(new URL(`../../scripts/${scriptName}`, import.meta.url))
  const child = spawn(process.execPath, [script, ...args], { env: { ...process.env, ...env } })
  let stdout = ''
  let stderr = ''
  child.stdout.setEncoding('utf8').on('data', chunk => {
    stdout += chunk
  })
  child.stderr.setEncoding('utf8').on('data', chunk => {
    stderr += chunk
  })
  return once(child, 'close').then(([code]) => ({ code, stderr, stdout }))
}

function runSmokeCli(env) {
  return runCli('provider-tool-smoke.mjs', env)
}

test('sorts tagged provider models and prefixes hub routing ids', () => {
  assert.deepEqual(
    selectProviderModels({
      data: [{ id: 'gpt-test', provider: 'chatgpt' }, { id: 'deepseek-chat', provider: 'deepseek' }, { id: 'ignored' }],
    }),
    [
      { id: 'gpt-test', provider: 'chatgpt' },
      { id: 'deepseek-chat', provider: 'deepseek' },
      { id: 'ignored', provider: 'unknown' },
    ]
  )
  assert.equal(buildToolRequest('deepseek', 'deepseek-chat').model, 'deepseek:deepseek-chat')
})

test('caps scheduled benchmark models per provider without changing discovery', () => {
  const models = selectProviderModels({
    data: [
      { id: 'gpt-a', provider: 'chatgpt' },
      { id: 'gpt-b', provider: 'chatgpt' },
      { id: 'qwen-a', provider: 'qwen' },
      { id: 'qwen-b', provider: 'qwen' },
    ],
  })
  assert.deepEqual(limitModelsPerProvider(models, 1), [
    { id: 'gpt-a', provider: 'chatgpt' },
    { id: 'qwen-a', provider: 'qwen' },
  ])
  assert.throws(() => limitModelsPerProvider(models, 0), /positive integer/)
})

test('parses streamed OpenAI tool-call deltas and done marker', () => {
  const sse = summarizeSse(
    parseSse(
      [
        ': keep-alive',
        'data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"report_smoke_target","arguments":"{\\"provider\\":\\"qwen\\""}}]}}]}',
        '',
        'data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":",\\"model\\":\\"qwen3\\"}"}}]},"finish_reason":"tool_calls"}]}',
        '',
        'data: [DONE]',
        '',
      ].join('\n')
    )
  )

  assert.equal(sse.done, true)
  assert.deepEqual(sse.finish_reasons, ['tool_calls'])
  assert.deepEqual(sse.tool_calls, [
    {
      id: 'call_1',
      type: 'function',
      name: 'report_smoke_target',
      arguments: '{"provider":"qwen","model":"qwen3"}',
      arguments_json: { provider: 'qwen', model: 'qwen3' },
    },
  ])
})

test('runs every discovered model and includes advertised provider logs', async () => {
  const requests = []
  const fetchImpl = async (url, init = {}) => {
    const requestUrl = new URL(url)
    requests.push({ path: requestUrl.pathname, headers: new Headers(init.headers), body: init.body })
    if (requestUrl.pathname === '/') {
      return Response.json({ routes: { provider_logs: '/providers/{provider}/logs' } })
    }
    if (requestUrl.pathname === '/openapi.json') return Response.json({ paths: {} })
    if (requestUrl.pathname === '/v1/models') {
      return Response.json({
        data: [
          { id: 'gpt-test', provider: 'chatgpt' },
          { id: 'qwen3', provider: 'qwen' },
        ],
      })
    }
    if (requestUrl.pathname === '/v1/chat/completions') {
      const payload = JSON.parse(init.body)
      const [provider, model] = payload.model.split(':')
      return new Response(
        [
          `data: ${JSON.stringify({ choices: [{ delta: { tool_calls: [{ index: 0, id: `call_${provider}`, type: 'function', function: { name: 'report_smoke_target', arguments: JSON.stringify({ provider, model }) } }] }, finish_reason: 'tool_calls' }] })}`,
          '',
          'data: [DONE]',
          '',
        ].join('\n'),
        { headers: { 'content-type': 'text/event-stream' } }
      )
    }
    if (requestUrl.pathname.endsWith('/logs')) {
      return Response.json({ entries: [`${requestUrl.pathname} log`] })
    }
    throw new Error(`Unexpected request: ${url}`)
  }

  const report = await runProviderToolSmoke({ apiKey: 'test-key', fetchImpl, hubUrl: 'http://hub.test' })

  assert.deepEqual(report.summary, {
    failed: 0,
    fetched_models: 2,
    fetched_providers: 2,
    models: 2,
    passed: 2,
    providers: 2,
    scheduled_models: 2,
    scheduled_providers: 2,
    worked_models: 2,
    worked_providers: 2,
  })
  assert.deepEqual(report.provider_logs.chatgpt.entries, ['/providers/chatgpt/logs log'])
  assert.equal(report.results[0].routed_model, 'chatgpt:gpt-test')
  assert.equal(report.results[1].routed_model, 'qwen:qwen3')
  assert.equal(report.results[1].request.headers.authorization, '[redacted]')
  assert.equal(JSON.parse(report.results[1].request.body).model, 'qwen:qwen3')
  assert.match(report.results[1].response.body, /call_qwen/)
  assert.equal(requests.find(request => request.path === '/v1/models').headers.get('authorization'), 'Bearer test-key')
  assert.equal(requests.filter(request => request.path === '/v1/chat/completions').length, 2)
})

test('uses an OpenAPI provider-log endpoint and validates CLI values', () => {
  assert.deepEqual(discoverProviderLogsEndpoint({}, { paths: { '/providers/{provider}/logs': {} } }), {
    path: '/providers/{provider}/logs',
    source: 'openapi',
  })
  assert.deepEqual(parseCliArgs(['--hub', 'http://localhost:3100/', '--api-key', 'key'], {}), {
    apiKey: 'key',
    hubUrl: 'http://localhost:3100',
  })
  assert.deepEqual(parseCliArgs(['--', '--help'], {}), { help: true })
  assert.throws(() => parseCliArgs(['--hub', 'file:///tmp/hub'], {}), /http or https/)
})

test('CLI creates an isolated provider test environment and verifies every provider response and log', async () => {
  const hub = await startTemporaryHub()
  try {
    const result = await runSmokeCli({
      RUST_PROXY_HUB_API_KEY: 'temporary-test-key',
      RUST_PROXY_HUB_URL: hub.url,
    })
    assert.equal(result.code, 0, result.stderr)

    const report = JSON.parse(result.stdout)
    assert.deepEqual(report.summary, {
      failed: 0,
      fetched_models: 8,
      fetched_providers: 8,
      models: 8,
      passed: 8,
      providers: 8,
      scheduled_models: 8,
      scheduled_providers: 8,
      worked_models: 8,
      worked_providers: 8,
    })
    assert.deepEqual(Object.keys(report.provider_logs).sort(), ALL_PROVIDERS.map(([provider]) => provider).sort())
    for (const [provider] of ALL_PROVIDERS) {
      assert.deepEqual(report.provider_logs[provider].entries, [`${provider} tool-call recorded`])
    }

    const chats = hub.requests.filter(request => request.path === '/v1/chat/completions')
    assert.equal(chats.length, ALL_PROVIDERS.length)
    for (const request of chats) {
      const payload = JSON.parse(request.body)
      assert.equal(request.headers.authorization, 'Bearer temporary-test-key')
      assert.equal(payload.stream, true)
      assert.equal(payload.tools[0].function.name, 'report_smoke_target')
      assert.equal(payload.tool_choice.function.name, 'report_smoke_target')
    }
  } finally {
    await hub.close()
  }
})

test('CLI checks prompt and tool-result interactions for Kilo, Claude, Pi, and OpenCode', async () => {
  const hub = await startTemporaryHub()
  try {
    const result = await runCli('client-interaction-smoke.mjs', {
      RUST_PROXY_HUB_API_KEY: 'temporary-test-key',
      RUST_PROXY_HUB_URL: hub.url,
    })
    assert.equal(result.code, 0, result.stderr)

    const report = JSON.parse(result.stdout)
    assert.deepEqual(report.summary, {
      failed: 0,
      fetched_models: 8,
      models: 8,
      passed: 16,
      protocols: 2,
      scheduled_models: 8,
    })
    assert.deepEqual(Object.keys(report.clients).sort(), ['claude', 'kilo', 'opencode', 'pi'])
    assert.equal(report.clients.kilo.configuration.provider_api, 'OpenAI Compatible')
    assert.equal(report.clients.pi.configuration.models_json.providers.rust_proxy_hub.api, 'openai-completions')
    assert.equal(
      report.clients.opencode.configuration.opencode_json.provider.rust_proxy_hub.npm,
      '@ai-sdk/openai-compatible'
    )
    assert.equal(report.clients.claude.configuration.protocol, 'anthropic-messages')

    const chatInteractions = hub.requests.filter(
      request =>
        request.path === '/v1/chat/completions' &&
        JSON.parse(request.body).messages.some(message => message.role === 'tool')
    )
    const anthropicInteractions = hub.requests.filter(request => request.path === '/v1/messages')
    assert.equal(chatInteractions.length, ALL_PROVIDERS.length)
    assert.equal(anthropicInteractions.length, ALL_PROVIDERS.length)
    assert.ok(
      anthropicInteractions.every(request => JSON.parse(request.body).messages.at(-1).content[0].type === 'tool_result')
    )
  } finally {
    await hub.close()
  }
})

test('benchmark records every provider/model conversation in append-only JSONL and Markdown history', async () => {
  const hub = await startTemporaryHub()
  const historyDir = await mkdtemp(join(tmpdir(), 'rust-proxy-hub-benchmark-'))
  try {
    const result = await runCli(
      'benchmark-provider-interactions.mjs',
      {
        RUST_PROXY_HUB_API_KEY: 'temporary-test-key',
        RUST_PROXY_HUB_URL: hub.url,
      },
      ['--history-dir', historyDir]
    )
    assert.equal(result.code, 0, result.stderr)

    const report = JSON.parse(result.stdout)
    assert.equal(report.summary.failed, 0)
    assert.equal(report.summary.models_fetched, 8)
    assert.equal(report.summary.models_scheduled, 8)
    assert.equal(report.summary.models_worked, 8)
    assert.equal(report.summary.providers_fetched, 8)
    assert.equal(report.summary.providers_scheduled, 8)
    assert.equal(report.summary.providers_worked, 8)
    assert.equal(report.summary.requests, 24)
    assert.equal(report.summary.passed, 24)
    assert.equal(report.summary.logs_fetched, 8)
    assert.ok(report.summary.log_entries >= 8)
    assert.ok(report.summary.latency_ms.total > 0)
    assert.equal(report.history.runs, 1)
    const repeat = await runCli(
      'benchmark-provider-interactions.mjs',
      {
        RUST_PROXY_HUB_API_KEY: 'temporary-test-key',
        RUST_PROXY_HUB_URL: hub.url,
      },
      ['--history-dir', historyDir]
    )
    assert.equal(repeat.code, 0, repeat.stderr)
    assert.equal(JSON.parse(repeat.stdout).history.runs, 2)
    const jsonl = await readFile(join(historyDir, 'provider-model-history.jsonl'), 'utf8')
    const markdown = await readFile(join(historyDir, 'provider-model-history.md'), 'utf8')
    assert.equal(jsonl.trim().split('\n').length, 2)
    assert.match(jsonl, /forced_tool_call \+ prompt_tool_result_interaction:v1/)
    assert.match(jsonl, /\"request\"/)
    assert.match(jsonl, /\"response\"/)
    assert.match(jsonl, /\[redacted\]/)
    assert.match(markdown, /Latest conversation results/)
    assert.match(markdown, /qwen:qwen3/)
    assert.match(markdown, /anthropic-messages/)
    assert.match(markdown, /RUST_PROXY_HUB_INTERACTION_CONFIRMED/)
  } finally {
    await hub.close()
    await rm(historyDir, { force: true, recursive: true })
  }
})
