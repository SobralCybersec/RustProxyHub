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

test('normalizes Codex responses request and keeps instructions trusted', () => {
  const prepared = buildCodexResponsesRequest({
    model: 'gpt-test',
    prompt: 'User request',
    instructions: 'System contract',
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
  })
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
    webSearch: false,
  })

  assert.equal(requestHeaders.get('authorization'), `Bearer ${accessToken}`)
  assert.equal(requestHeaders.get('chatgpt-account-id'), 'acct_chat')
  assert.equal(requestHeaders.get('x-openai-internal-codex-responses-lite'), 'true')
  assert.equal(requestBody.store, false)
  assert.equal(requestBody.input[0].role, 'developer')
  assert.equal(requestBody.input[0].content[0].text, 'Follow contract')
  assert.equal(requestBody.input[1].content[0].text, 'Run task')
  assert.equal(result.text, 'working')
  assert.equal(result.conversation_id, 'resp_live')
})
