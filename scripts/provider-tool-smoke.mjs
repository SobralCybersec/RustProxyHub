#!/usr/bin/env node
import { fileURLToPath } from 'node:url'
import { performance } from 'node:perf_hooks'

const TOOL_NAME = 'report_smoke_target'
const DEFAULT_HUB_URL = 'http://127.0.0.1:3100'

function trimTrailingSlash(value) {
  return String(value).replace(/\/+$/, '')
}

export function parseCliArgs(argv, env = process.env) {
  const options = {
    apiKey: env.RUST_PROXY_HUB_API_KEY || '',
    hubUrl: env.RUST_PROXY_HUB_URL || DEFAULT_HUB_URL,
  }

  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index]
    if (argument === '--' && index === 0) continue
    if (argument === '--help' || argument === '-h') return { help: true }
    if (argument === '--hub' || argument === '--api-key') {
      const value = argv[index + 1]
      if (!value) throw new Error(`${argument} requires a value`)
      options[argument === '--hub' ? 'hubUrl' : 'apiKey'] = value
      index += 1
      continue
    }
    throw new Error(`Unknown argument: ${argument}`)
  }

  const url = new URL(options.hubUrl)
  if (!['http:', 'https:'].includes(url.protocol)) {
    throw new Error('--hub must use http or https')
  }
  options.hubUrl = trimTrailingSlash(url.toString())
  return options
}

export function buildAuthHeaders(apiKey, headers = {}) {
  return {
    accept: 'application/json, text/event-stream',
    ...headers,
    ...(apiKey ? { authorization: `Bearer ${apiKey}` } : {}),
  }
}

function captureHeaders(headers) {
  return Object.fromEntries(new Headers(headers).entries())
}

export function captureRequest(url, init = {}) {
  const headers = captureHeaders(init.headers)
  if (headers.authorization) headers.authorization = '[redacted]'
  return {
    body: typeof init.body === 'string' ? init.body : null,
    headers,
    method: init.method || 'GET',
    url: String(url),
  }
}

export function captureResponse(response, body) {
  return {
    body,
    headers: captureHeaders(response.headers),
    status: response.status,
  }
}

export function selectProviderModels(payload) {
  if (!Array.isArray(payload?.data)) throw new Error('GET /v1/models returned no data array')

  return payload.data
    .filter(item => typeof item?.id === 'string' && item.id.trim())
    .map(item => ({
      id: item.id.trim(),
      provider: typeof item.provider === 'string' && item.provider.trim() ? item.provider.trim() : 'unknown',
    }))
    .sort((left, right) => left.provider.localeCompare(right.provider) || left.id.localeCompare(right.id))
}

export function limitModelsPerProvider(models, maxModelsPerProvider = Infinity) {
  if (maxModelsPerProvider !== Infinity && (!Number.isSafeInteger(maxModelsPerProvider) || maxModelsPerProvider < 1)) {
    throw new Error('maxModelsPerProvider must be a positive integer or Infinity')
  }
  const selected = []
  const counts = new Map()
  for (const model of models) {
    const count = counts.get(model.provider) || 0
    if (count >= maxModelsPerProvider) continue
    counts.set(model.provider, count + 1)
    selected.push(model)
  }
  return selected
}

export function routedModelId(provider, model) {
  return provider !== 'unknown' && !model.includes(':') ? `${provider}:${model}` : model
}

export function buildToolRequest(provider, model) {
  return {
    model: routedModelId(provider, model),
    stream: true,
    messages: [
      {
        role: 'user',
        content: `Call ${TOOL_NAME} once with provider ${JSON.stringify(provider)} and model ${JSON.stringify(model)}. Do not add text.`,
      },
    ],
    tools: [
      {
        type: 'function',
        function: {
          name: TOOL_NAME,
          description: 'Report which provider model handled this deterministic smoke request.',
          parameters: {
            type: 'object',
            properties: {
              provider: { type: 'string' },
              model: { type: 'string' },
            },
            required: ['provider', 'model'],
            additionalProperties: false,
          },
        },
      },
    ],
    tool_choice: { type: 'function', function: { name: TOOL_NAME } },
  }
}

export function parseSse(text) {
  const events = []
  let event = 'message'
  let data = []

  const flush = () => {
    if (data.length === 0) return
    const body = data.join('\n')
    const parsed = body === '[DONE]' ? { value: null, error: null } : tryParseJson(body)
    events.push({ event, data: body, json: parsed.value, parseError: parsed.error })
    event = 'message'
    data = []
  }

  for (const line of String(text).replace(/\r\n?/g, '\n').split('\n')) {
    if (line === '') {
      flush()
    } else if (line.startsWith('event:')) {
      event = line.slice(6).trim() || 'message'
    } else if (line.startsWith('data:')) {
      data.push(line.slice(5).replace(/^ /, ''))
    }
  }
  flush()
  return events
}

function tryParseJson(value) {
  try {
    return { value: JSON.parse(value), error: null }
  } catch {
    return { value: null, error: value ? 'invalid_json' : null }
  }
}

export function summarizeSse(events) {
  const calls = new Map()
  const finishReasons = []
  let contentChars = 0
  let done = false
  let parsedEvents = 0
  let malformedEvents = 0

  for (const event of events) {
    if (event.data === '[DONE]') {
      done = true
      continue
    }
    if (event.parseError) {
      malformedEvents += 1
      continue
    }
    if (!event.json) continue
    parsedEvents += 1

    for (const choice of event.json.choices || []) {
      const delta = choice.delta || {}
      if (typeof delta.content === 'string') contentChars += delta.content.length
      if (choice.finish_reason) finishReasons.push(choice.finish_reason)
      for (const toolCall of delta.tool_calls || []) {
        const key = toolCall.index ?? toolCall.id ?? calls.size
        const current = calls.get(key) || { id: null, type: null, name: null, arguments: '' }
        current.id ||= toolCall.id || null
        current.type ||= toolCall.type || null
        current.name ||= toolCall.function?.name || null
        current.arguments += toolCall.function?.arguments || ''
        calls.set(key, current)
      }
    }
  }

  const toolCalls = [...calls.values()].map(toolCall => ({
    ...toolCall,
    arguments_json: tryParseJson(toolCall.arguments).value,
  }))
  return {
    content_chars: contentChars,
    done,
    events: events.length,
    finish_reasons: finishReasons,
    malformed_events: malformedEvents,
    parsed_events: parsedEvents,
    tool_calls: toolCalls,
  }
}

export function discoverProviderLogsEndpoint(root, openApi) {
  const routes = root?.routes && typeof root.routes === 'object' ? root.routes : {}
  for (const [name, path] of Object.entries(routes)) {
    if (/logs?/i.test(name) && typeof path === 'string' && path.startsWith('/')) {
      return { path, source: 'root.routes' }
    }
  }

  for (const path of Object.keys(openApi?.paths || {})) {
    if (/provider.*logs|logs.*provider/i.test(path)) return { path, source: 'openapi' }
  }
  return null
}

export function providerLogsUrl(hubUrl, endpoint, provider) {
  const path = endpoint.path.includes('{provider}')
    ? endpoint.path.replaceAll('{provider}', encodeURIComponent(provider))
    : `${endpoint.path}${endpoint.path.includes('?') ? '&' : '?'}provider=${encodeURIComponent(provider)}`
  return new URL(path, `${trimTrailingSlash(hubUrl)}/`).toString()
}

async function fetchText(fetchImpl, url, init) {
  const response = await fetchImpl(url, init)
  return { response, text: await response.text() }
}

async function fetchJson(fetchImpl, url, init) {
  const { response, text } = await fetchText(fetchImpl, url, init)
  const parsed = tryParseJson(text)
  return { ok: response.ok, status: response.status, value: parsed.value, error: parsed.error }
}

async function optionalJson(fetchImpl, url, init) {
  try {
    return await fetchJson(fetchImpl, url, init)
  } catch (error) {
    return { ok: false, status: null, value: null, error: error.message }
  }
}

export async function runProviderToolSmoke({ apiKey = '', fetchImpl = fetch, hubUrl = DEFAULT_HUB_URL, maxModelsPerProvider = Infinity } = {}) {
  const headers = buildAuthHeaders(apiKey)
  const rootUrl = new URL('/', `${trimTrailingSlash(hubUrl)}/`).toString()
  const openApiUrl = new URL('/openapi.json', `${trimTrailingSlash(hubUrl)}/`).toString()
  const modelsUrl = new URL('/v1/models', `${trimTrailingSlash(hubUrl)}/`).toString()
  const root = await optionalJson(fetchImpl, rootUrl, { headers })
  const openApi = await optionalJson(fetchImpl, openApiUrl, { headers })
  const modelsResponse = await fetchJson(fetchImpl, modelsUrl, { headers })
  if (!modelsResponse.ok) {
    throw new Error(`GET /v1/models failed with status ${modelsResponse.status}`)
  }

  const endpoint = discoverProviderLogsEndpoint(root.value, openApi.value)
  const fetchedModels = selectProviderModels(modelsResponse.value)
  const models = limitModelsPerProvider(fetchedModels, maxModelsPerProvider)
  const results = []

  for (const { provider, id } of models) {
    const request = buildToolRequest(provider, id)
    const init = {
      method: 'POST',
      headers: buildAuthHeaders(apiKey, { 'content-type': 'application/json' }),
      body: JSON.stringify(request),
    }
    const requestCapture = captureRequest(new URL('/v1/chat/completions', `${trimTrailingSlash(hubUrl)}/`).toString(), init)
    const started = performance.now()
    try {
      const { response, text } = await fetchText(fetchImpl, requestCapture.url, init)
      const sse = summarizeSse(parseSse(text))
      const toolCallDetected = sse.tool_calls.some(toolCall => toolCall.name === TOOL_NAME)
      results.push({
        provider,
        model: id,
        routed_model: request.model,
        status: response.status,
        latency_ms: Number((performance.now() - started).toFixed(3)),
        content_type: response.headers.get('content-type'),
        result: response.ok && sse.done && toolCallDetected ? 'passed' : 'failed',
        request: requestCapture,
        response: captureResponse(response, text),
        tool_call_detected: toolCallDetected,
        sse,
      })
    } catch (error) {
      results.push({
        provider,
        model: id,
        routed_model: request.model,
        request: requestCapture,
        response: null,
        result: 'failed',
        latency_ms: Number((performance.now() - started).toFixed(3)),
        error: error.message,
      })
    }
  }

  const fetchedProviders = [...new Set(fetchedModels.map(model => model.provider))]
  const providers = [...new Set(models.map(model => model.provider))]
  const providerLogs = {}
  for (const provider of providers) {
    if (!endpoint) {
      providerLogs[provider] = { available: false }
      continue
    }
    const logs = await optionalJson(fetchImpl, providerLogsUrl(hubUrl, endpoint, provider), { headers })
    providerLogs[provider] = logs.ok
      ? { available: true, endpoint: endpoint.path, status: logs.status, entries: logs.value?.entries ?? logs.value }
      : { available: true, endpoint: endpoint.path, status: logs.status, error: logs.error }
  }

  return {
    hub: trimTrailingSlash(hubUrl),
    logs_endpoint: endpoint,
    provider_logs: providerLogs,
    results,
    summary: {
      failed: results.filter(result => result.result !== 'passed').length,
      fetched_models: fetchedModels.length,
      fetched_providers: fetchedProviders.length,
      models: results.length,
      passed: results.filter(result => result.result === 'passed').length,
      providers: providers.length,
      scheduled_models: models.length,
      scheduled_providers: providers.length,
      worked_models: results.filter(result => result.result === 'passed').length,
      worked_providers: new Set(results.filter(result => result.result === 'passed').map(result => result.provider)).size,
    },
  }
}

export function helpText() {
  return `Usage: node scripts/provider-tool-smoke.mjs [--hub URL] [--api-key KEY]\n\nEnvironment: RUST_PROXY_HUB_URL, RUST_PROXY_HUB_API_KEY`
}

export async function main(argv = process.argv.slice(2), env = process.env) {
  const options = parseCliArgs(argv, env)
  if (options.help) {
    console.log(helpText())
    return 0
  }
  const report = await runProviderToolSmoke(options)
  console.log(JSON.stringify(report, null, 2))
  return report.summary.failed === 0 ? 0 : 1
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  main().then(code => process.exitCode = code).catch(error => {
    console.error(error.message)
    process.exitCode = 1
  })
}
