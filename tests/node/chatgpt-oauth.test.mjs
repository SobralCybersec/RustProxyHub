import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import test from 'node:test'
import {
  buildCodexResponsesRequest,
  ChatGptOAuthClient,
  collectCodexResponse,
  deriveAccountId,
  resetCodexClientVersionCache,
  resolveCodexClientVersion,
  shouldRefreshAccessToken,
} from '../../src-tauri/resources/playwright-bridge/chatgpt-oauth.mjs'

function jwt(claims) {
  return `${Buffer.from('{}').toString('base64url')}.${Buffer.from(JSON.stringify(claims)).toString('base64url')}.sig`
}

test('derives ChatGPT account id and refreshes near token expiry', () => {
  const now = Date.parse('2026-08-06T12:00:00Z')
  const token = jwt({
    exp: Math.floor((now + 60_000) / 1000),
    'https://api.openai.com/auth': { chatgpt_account_id: 'acct_test' },
  })

  assert.equal(deriveAccountId(token), 'acct_test')
  assert.equal(shouldRefreshAccessToken(token, '2026-08-06T11:59:00Z', now), true)
})

test('resolves and caches the current Codex client version for model discovery', async () => {
  resetCodexClientVersionCache()
  let requests = 0
  const fetchVersion = async (url, init) => {
    requests += 1
    assert.equal(url, 'https://registry.npmjs.org/@openai/codex/latest')
    assert.equal(new Headers(init.headers).get('accept'), 'application/json')
    return Response.json({ version: '0.200.1' })
  }

  assert.equal(await resolveCodexClientVersion(fetchVersion), '0.200.1')
  assert.equal(await resolveCodexClientVersion(fetchVersion), '0.200.1')
  assert.equal(requests, 1)
})

test('normalizes Codex responses request and keeps instructions trusted', () => {
  const prepared = buildCodexResponsesRequest({
    model: 'gpt-test',
    prompt: 'User request',
    instructions: 'System contract',
    webSearch: true,
    modelInfo: {
      use_responses_lite: true,
      default_reasoning_level: 'medium',
      support_verbosity: true,
      default_verbosity: 'low',
    },
  })

  assert.equal(prepared.useResponsesLite, true)
  assert.equal(prepared.body.store, false)
  assert.equal(prepared.body.stream, true)
  assert.equal(prepared.body.instructions, '')
  assert.deepEqual(prepared.body.include, ['reasoning.encrypted_content'])
  assert.equal(prepared.body.force_use_tool, 'web')
  assert.equal(prepared.body.input[0].role, 'developer')
  assert.equal(prepared.body.input[0].content[0].text, 'System contract')
  assert.equal(prepared.body.input[1].role, 'user')
  assert.equal(prepared.body.input[1].content[0].text, 'User request')
  assert.deepEqual(prepared.body.reasoning, { effort: 'medium', context: 'all_turns' })
  assert.deepEqual(prepared.body.text, { verbosity: 'low' })
})

test('collects fragmented Codex SSE text and reasoning', async () => {
  const encoded = new TextEncoder().encode(
    [
      'event: response.output_text.delta\ndata: {"type":"response.output_text.delta","delta":"hel"}\n\n',
      'event: response.reasoning_summary_text.delta\ndata: {"type":"response.reasoning_summary_text.delta","delta":"why"}\n\n',
      'event: response.output_text.delta\ndata: {"type":"response.output_text.delta","delta":"lo"}\n\n',
      'event: response.completed\ndata: {"type":"response.completed","response":{"id":"resp_test","status":"completed"}}\n\n',
    ].join('')
  )
  const stream = new ReadableStream({
    start(controller) {
      controller.enqueue(encoded.slice(0, 37))
      controller.enqueue(encoded.slice(37, 121))
      controller.enqueue(encoded.slice(121))
      controller.close()
    },
  })

  assert.deepEqual(await collectCodexResponse(stream), {
    text: 'hello',
    reasoningContent: 'why',
    responseId: 'resp_test',
    upstreamUsage: null,
    upstreamCache: null,
  })
})

test('collects Codex refusal text when assistant text is absent', async () => {
  const encoded = new TextEncoder().encode(
    [
      'event: response.refusal.delta\ndata: {"type":"response.refusal.delta","delta":"Nope"}\n\n',
      'event: response.completed\ndata: {"type":"response.completed","response":{"id":"resp_refusal","status":"completed","output":[{"type":"message","content":[{"type":"refusal","refusal":"Nope"}]}]}}\n\n',
    ].join('')
  )
  const stream = new ReadableStream({
    start(controller) {
      controller.enqueue(encoded)
      controller.close()
    },
  })

  assert.deepEqual(await collectCodexResponse(stream), {
    text: 'Nope',
    reasoningContent: null,
    responseId: 'resp_refusal',
    upstreamUsage: null,
    upstreamCache: null,
  })
})

test('collects Codex completed response output_text when delta is absent', async () => {
  const stream = new ReadableStream({
    start(controller) {
      controller.enqueue(
        new TextEncoder().encode(
          'data: {"type":"response.completed","response":{"id":"resp_done","output_text":"done text"}}\n\n'
        )
      )
      controller.close()
    },
  })

  const result = await collectCodexResponse(stream)

  assert.equal(result.text, 'done text')
  assert.equal(result.responseId, 'resp_done')
  assert.equal(result.upstreamUsage, null)
  assert.equal(result.upstreamCache, null)
})

test('passes through completed Codex usage and cache metadata unchanged', async () => {
  const usage = { input_tokens: 7, output_tokens: 3, input_tokens_details: { cached_tokens: 2 } }
  const cache = { status: 'HIT' }
  const stream = new ReadableStream({
    start(controller) {
      controller.enqueue(
        new TextEncoder().encode(
          `data: ${JSON.stringify({ type: 'response.completed', response: { id: 'resp_usage', output_text: 'done', usage, cache } })}\n\n`
        )
      )
      controller.close()
    },
  })

  const result = await collectCodexResponse(stream)
  assert.deepEqual(result.upstreamUsage, usage)
  assert.deepEqual(result.upstreamCache, cache)
})

test('loads local credentials and applies Codex auth headers', async t => {
  const runtimeDir = await fs.mkdtemp(path.join(os.tmpdir(), 'rustproxyhub-oauth-'))
  t.after(() => fs.rm(runtimeDir, { recursive: true, force: true }))
  const accessToken = jwt({ exp: Math.floor(Date.now() / 1000) + 3600 })
  await fs.writeFile(
    path.join(runtimeDir, 'chatgpt_oauth.json'),
    JSON.stringify({
      auth_mode: 'chatgpt',
      tokens: { access_token: accessToken, account_id: 'acct_local' },
      last_refresh: new Date().toISOString(),
    })
  )
  let captured
  const client = new ChatGptOAuthClient(runtimeDir, async (url, init) => {
    captured = { url, headers: new Headers(init.headers) }
    return new Response('{}')
  })

  await client.authenticatedFetch('/probe')

  assert.equal(captured.url, 'https://chatgpt.com/backend-api/codex/probe')
  assert.equal(captured.headers.get('authorization'), `Bearer ${accessToken}`)
  assert.equal(captured.headers.get('chatgpt-account-id'), 'acct_local')
})

test('refreshes OAuth session after a Codex 401 before retrying', async t => {
  const runtimeDir = await fs.mkdtemp(path.join(os.tmpdir(), 'rustproxyhub-oauth-retry-'))
  t.after(() => fs.rm(runtimeDir, { recursive: true, force: true }))
  const oldToken = jwt({ exp: Math.floor(Date.now() / 1000) + 3600 })
  const newToken = jwt({ exp: Math.floor(Date.now() / 1000) + 7200 })
  await fs.writeFile(
    path.join(runtimeDir, 'chatgpt_oauth.json'),
    JSON.stringify({
      auth_mode: 'chatgpt',
      tokens: {
        access_token: oldToken,
        refresh_token: 'refresh-token',
        account_id: 'acct_retry',
      },
      last_refresh: new Date().toISOString(),
    })
  )

  const requests = []
  const client = new ChatGptOAuthClient(runtimeDir, async (url, init = {}) => {
    requests.push({ url, authorization: new Headers(init.headers).get('authorization') })
    if (url.endsWith('/oauth/token')) {
      return Response.json({ access_token: newToken, refresh_token: 'refresh-token' })
    }
    if (requests.length === 1) return new Response('expired', { status: 401 })
    return Response.json({ ok: true })
  })

  const response = await client.authenticatedFetch('/probe')

  assert.equal(response.status, 200)
  assert.equal(requests.length, 3)
  assert.equal(requests[0].authorization, `Bearer ${oldToken}`)
  assert.equal(requests[1].url, 'https://auth.openai.com/oauth/token')
  assert.equal(requests[2].authorization, `Bearer ${newToken}`)
})

test('discovers Codex models from data and excludes web-only catalog entries', async t => {
  const runtimeDir = await fs.mkdtemp(path.join(os.tmpdir(), 'rustproxyhub-oauth-models-'))
  t.after(() => fs.rm(runtimeDir, { recursive: true, force: true }))
  const accessToken = jwt({ exp: Math.floor(Date.now() / 1000) + 3600 })
  await fs.writeFile(
    path.join(runtimeDir, 'chatgpt_oauth.json'),
    JSON.stringify({
      auth_mode: 'chatgpt',
      tokens: { access_token: accessToken, account_id: 'acct_models' },
      last_refresh: new Date().toISOString(),
    })
  )

  const client = new ChatGptOAuthClient(runtimeDir, async url => {
    assert.match(url, /\/models\?client_version=/)
    return Response.json({
      data: [
        { id: 'gpt-codex-ok', supported_in_api: true },
        { id: 'chatgpt-web-session', supported_in_api: true },
        { id: 'chatgpt.workspace.model.GPT-4.1.access', supported_in_api: true },
        { id: 'gpt-codex-disabled', supported_in_api: false },
        { id: 'gpt-codex-hidden', supported_in_api: true, visibility: 'hidden' },
      ],
    })
  })

  const result = await client.listModels()

  assert.deepEqual(
    result.data.map(item => item.id),
    ['gpt-codex-ok']
  )
  assert.equal(result.data[0].owned_by, 'codex')
  assert.equal(result.discovery.catalogue_fields[0], 'data')
  assert.equal(result.discovery.accepted_count, 1)
})

test('coalesces concurrent Codex model discovery', async t => {
  const runtimeDir = await fs.mkdtemp(path.join(os.tmpdir(), 'rustproxyhub-oauth-models-single-flight-'))
  t.after(() => fs.rm(runtimeDir, { recursive: true, force: true }))
  const accessToken = jwt({ exp: Math.floor(Date.now() / 1000) + 3600 })
  await fs.writeFile(
    path.join(runtimeDir, 'chatgpt_oauth.json'),
    JSON.stringify({
      auth_mode: 'chatgpt',
      tokens: { access_token: accessToken, account_id: 'acct_single_flight' },
      last_refresh: new Date().toISOString(),
    })
  )

  let modelRequests = 0
  const client = new ChatGptOAuthClient(runtimeDir, async url => {
    if (url.includes('/models?')) {
      modelRequests += 1
      await new Promise(resolve => setTimeout(resolve, 10))
      return Response.json({ models: [{ slug: 'gpt-single-flight', supported_in_api: true }] })
    }
    return Response.json({})
  })

  const results = await Promise.all([client.listModels(), client.listModels(), client.listModels()])

  assert.equal(modelRequests, 1)
  assert.deepEqual(
    results.map(result => result.data[0].id),
    ['gpt-single-flight', 'gpt-single-flight', 'gpt-single-flight']
  )
})

test('discovers model, sends normalized Codex request, and collects reply', async t => {
  const runtimeDir = await fs.mkdtemp(path.join(os.tmpdir(), 'rustproxyhub-chat-'))
  t.after(() => fs.rm(runtimeDir, { recursive: true, force: true }))
  const accessToken = jwt({ exp: Math.floor(Date.now() / 1000) + 3600 })
  await fs.writeFile(
    path.join(runtimeDir, 'chatgpt_oauth.json'),
    JSON.stringify({
      auth_mode: 'chatgpt',
      tokens: { access_token: accessToken, account_id: 'acct_chat' },
      last_refresh: new Date().toISOString(),
    })
  )
  let requestBody
  let requestHeaders
  const client = new ChatGptOAuthClient(runtimeDir, async (url, init = {}) => {
    if (url.includes('/models?')) {
      return Response.json({
        models: [
          {
            slug: 'gpt-test',
            visibility: 'list',
            supported_in_api: true,
            use_responses_lite: true,
            default_reasoning_level: 'medium',
          },
        ],
      })
    }
    requestBody = JSON.parse(init.body)
    requestHeaders = new Headers(init.headers)
    const stream = new ReadableStream({
      start(controller) {
        controller.enqueue(
          new TextEncoder().encode(
            'event: response.output_text.delta\ndata: {"type":"response.output_text.delta","delta":"working"}\n\n' +
              'event: response.completed\ndata: {"type":"response.completed","response":{"id":"resp_live","status":"completed"}}\n\n'
          )
        )
        controller.close()
      },
    })
    return new Response(stream, { headers: { 'content-type': 'text/event-stream' } })
  })

  const result = await client.chat({
    model: 'gpt-test',
    prompt: 'Run task',
    systemPrompt: 'Follow contract',
    webSearch: true,
  })

  assert.equal(requestHeaders.get('authorization'), `Bearer ${accessToken}`)
  assert.equal(requestHeaders.get('chatgpt-account-id'), 'acct_chat')
  assert.equal(requestHeaders.get('x-openai-internal-codex-responses-lite'), 'true')
  assert.equal(requestBody.store, false)
  assert.equal(requestBody.force_use_tool, 'web')
  assert.equal(requestBody.input[0].role, 'developer')
  assert.equal(requestBody.input[0].content[0].text, 'Follow contract')
  assert.equal(requestBody.input[1].content[0].text, 'Run task')
  assert.equal(result.text, 'working')
  assert.equal(result.conversation_id, 'resp_live')
})

test('falls back to discovered Codex model when web model is requested in Codex lane', async t => {
  const runtimeDir = await fs.mkdtemp(path.join(os.tmpdir(), 'rustproxyhub-codex-fallback-'))
  t.after(() => fs.rm(runtimeDir, { recursive: true, force: true }))
  const accessToken = jwt({ exp: Math.floor(Date.now() / 1000) + 3600 })
  await fs.writeFile(
    path.join(runtimeDir, 'chatgpt_oauth.json'),
    JSON.stringify({
      auth_mode: 'chatgpt',
      tokens: { access_token: accessToken, account_id: 'acct_chat' },
      last_refresh: new Date().toISOString(),
    })
  )

  let requestBody
  const client = new ChatGptOAuthClient(runtimeDir, async (url, init = {}) => {
    if (url.includes('/models?')) {
      return Response.json({
        models: [{ slug: 'gpt-codex-ok', visibility: 'list', supported_in_api: true }],
      })
    }
    requestBody = JSON.parse(init.body)
    const stream = new ReadableStream({
      start(controller) {
        controller.enqueue(
          new TextEncoder().encode(
            'event: response.output_text.delta\ndata: {"type":"response.output_text.delta","delta":"ok"}\n\n' +
              'event: response.completed\ndata: {"type":"response.completed","response":{"id":"resp_fallback","status":"completed"}}\n\n'
          )
        )
        controller.close()
      },
    })
    return new Response(stream, { headers: { 'content-type': 'text/event-stream' } })
  })

  const result = await client.chat({
    model: 'gpt-5-3',
    prompt: 'Run task',
    systemPrompt: '',
    webSearch: false,
  })

  assert.equal(requestBody.model, 'gpt-codex-ok')
  assert.equal(result.model, 'gpt-codex-ok')
  assert.match(result.warning, /gpt-5-3.*gpt-codex-ok/)
})

test('reports an HTML security-verification page during Codex model discovery', async t => {
  const runtimeDir = await fs.mkdtemp(path.join(os.tmpdir(), 'rustproxyhub-oauth-html-models-'))
  t.after(() => fs.rm(runtimeDir, { recursive: true, force: true }))
  const accessToken = jwt({ exp: Math.floor(Date.now() / 1000) + 3600 })
  await fs.writeFile(
    path.join(runtimeDir, 'chatgpt_oauth.json'),
    JSON.stringify({
      auth_mode: 'chatgpt',
      tokens: { access_token: accessToken, account_id: 'acct_html' },
      last_refresh: new Date().toISOString(),
    })
  )

  const client = new ChatGptOAuthClient(runtimeDir, async (_url, init = {}) => {
    const headers = new Headers(init.headers)
    assert.equal(headers.get('accept'), 'application/json')
    assert.ok(headers.get('x-client-request-id'))
    return new Response('<html>Performing security verification</html>', {
      headers: { 'content-type': 'text/html; charset=UTF-8', 'x-request-id': 'req_html' },
    })
  })

  await assert.rejects(
    () => client.listModels(),
    /invalid content type text\/html; charset=UTF-8.*security-verification.*req_html/i
  )
})
