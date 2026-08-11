#!/usr/bin/env node
import { fileURLToPath } from 'node:url'
import { performance } from 'node:perf_hooks'
import {
  buildAuthHeaders,
  captureRequest,
  captureResponse,
  limitModelsPerProvider,
  parseSse,
  selectProviderModels,
  summarizeSse,
} from './provider-tool-smoke.mjs'

const CLIENTS = new Set(['kilo', 'claude', 'pi', 'opencode'])
const DEFAULT_HUB_URL = 'http://127.0.0.1:3100'
export const INTERACTION_TEXT = 'RUST_PROXY_HUB_INTERACTION_CONFIRMED'
const TOOL_NAME = 'report_interaction_target'

function trimTrailingSlash(value) {
  return String(value).replace(/\/+$/, '')
}

function routedModelId(provider, model) {
  return model.includes(':') ? model : `${provider}:${model}`
}

function responseText(response) {
  return response.text().then(text => ({ response, text }))
}

function tryJson(value) {
  try {
    return JSON.parse(value)
  } catch {
    return null
  }
}

export function parseCliArgs(argv, env = process.env) {
  const options = {
    apiKey: env.RUST_PROXY_HUB_API_KEY || '',
    clients: [...CLIENTS],
    hubUrl: env.RUST_PROXY_HUB_URL || DEFAULT_HUB_URL,
  }

  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index]
    if (argument === '--' && index === 0) continue
    if (argument === '--help' || argument === '-h') return { help: true }
    if (argument === '--hub' || argument === '--api-key' || argument === '--client') {
      const value = argv[index + 1]
      if (!value) throw new Error(`${argument} requires a value`)
      if (argument === '--hub') options.hubUrl = value
      else if (argument === '--api-key') options.apiKey = value
      else options.clients = value.split(',').map(item => item.trim()).filter(Boolean)
      index += 1
      continue
    }
    throw new Error(`Unknown argument: ${argument}`)
  }

  const url = new URL(options.hubUrl)
  if (!['http:', 'https:'].includes(url.protocol)) throw new Error('--hub must use http or https')
  options.hubUrl = trimTrailingSlash(url.toString())
  for (const client of options.clients) {
    if (!CLIENTS.has(client)) throw new Error(`Unknown client: ${client}`)
  }
  return options
}

export function clientConfiguration(client, { apiKeyEnv = 'RUST_PROXY_HUB_API_KEY', hubUrl, model }) {
  const baseUrl = `${trimTrailingSlash(hubUrl)}/v1`
  if (client === 'kilo') {
    return {
      provider_api: 'OpenAI Compatible',
      base_url: baseUrl,
      api_key_environment: apiKeyEnv,
      model,
    }
  }
  if (client === 'claude') {
    return {
      environment: {
        ANTHROPIC_AUTH_TOKEN: `$${apiKeyEnv}`,
        ANTHROPIC_BASE_URL: trimTrailingSlash(hubUrl),
      },
      model,
      protocol: 'anthropic-messages',
    }
  }
  if (client === 'pi') {
    return {
      models_json: {
        providers: {
          rust_proxy_hub: {
            api: 'openai-completions',
            apiKey: apiKeyEnv,
            authHeader: true,
            baseUrl,
            models: [{ id: model }],
          },
        },
      },
    }
  }
  return {
    opencode_json: {
      $schema: 'https://opencode.ai/config.json',
      provider: {
        rust_proxy_hub: {
          models: { [model]: { name: model } },
          name: 'RustProxyHub',
          npm: '@ai-sdk/openai-compatible',
          options: { apiKey: `{env:${apiKeyEnv}}`, baseURL: baseUrl },
        },
      },
    },
  }
}

export function buildOpenAiInteractionRequest(provider, model) {
  const routedModel = routedModelId(provider, model)
  return {
    model: routedModel,
    stream: true,
    messages: [
      { role: 'system', content: 'You are validating a deterministic tool-result interaction.' },
      { role: 'user', content: `A prior ${TOOL_NAME} call completed for ${provider}/${model}.` },
      {
        role: 'assistant',
        content: null,
        tool_calls: [{
          id: 'interaction_call_1',
          type: 'function',
          function: { name: TOOL_NAME, arguments: JSON.stringify({ model, provider }) },
        }],
      },
      {
        role: 'tool',
        tool_call_id: 'interaction_call_1',
        name: TOOL_NAME,
        content: JSON.stringify({ model, provider, status: 'completed' }),
      },
      { role: 'user', content: `Use that result and reply with exactly ${INTERACTION_TEXT}.` },
    ],
    tools: [{
      type: 'function',
      function: {
        name: TOOL_NAME,
        description: 'Reports deterministic interaction metadata.',
        parameters: { type: 'object', properties: {}, additionalProperties: false },
      },
    }],
  }
}

export function buildAnthropicInteractionRequest(provider, model) {
  const routedModel = routedModelId(provider, model)
  return {
    model: routedModel,
    stream: true,
    max_tokens: 64,
    system: 'You are validating a deterministic tool-result interaction.',
    messages: [
      { role: 'user', content: `Call ${TOOL_NAME} for ${provider}/${model}.` },
      {
        role: 'assistant',
        content: [{
          type: 'tool_use',
          id: 'interaction_call_1',
          name: TOOL_NAME,
          input: { model, provider },
        }],
      },
      {
        role: 'user',
        content: [
          {
            type: 'tool_result',
            tool_use_id: 'interaction_call_1',
            content: JSON.stringify({ model, provider, status: 'completed' }),
          },
          { type: 'text', text: `Use that result and reply with exactly ${INTERACTION_TEXT}.` },
        ],
      },
    ],
    tools: [{
      name: TOOL_NAME,
      description: 'Reports deterministic interaction metadata.',
      input_schema: { type: 'object', properties: {}, additionalProperties: false },
    }],
  }
}

export function summarizeAnthropicSse(text) {
  let done = false
  const fragments = []
  for (const event of parseSse(text)) {
    const payload = event.json
    if (event.event === 'message_stop' || event.data === '[DONE]') done = true
    if (typeof payload?.delta?.text === 'string') fragments.push(payload.delta.text)
    if (typeof payload?.content_block?.text === 'string') fragments.push(payload.content_block.text)
    if (typeof payload?.content?.[0]?.text === 'string') fragments.push(payload.content[0].text)
  }
  return { done, text: fragments.join('') }
}

function openAiResponseText(text) {
  return parseSse(text)
    .flatMap(event => event.json?.choices || [])
    .map(choice => choice.delta?.content || '')
    .join('')
}

async function runInteraction({ apiKey, fetchImpl, hubUrl, models, protocol }) {
  const path = protocol === 'anthropic' ? '/v1/messages' : '/v1/chat/completions'
  const results = []
  for (const { provider, id } of models) {
    const request = protocol === 'anthropic'
      ? buildAnthropicInteractionRequest(provider, id)
      : buildOpenAiInteractionRequest(provider, id)
    const started = performance.now()
    const url = new URL(path, `${hubUrl}/`).toString()
    const init = {
      method: 'POST',
      headers: buildAuthHeaders(apiKey, { 'content-type': 'application/json', 'anthropic-version': '2023-06-01' }),
      body: JSON.stringify(request),
    }
    const requestCapture = captureRequest(url, init)
    try {
      const { response, text } = await responseText(await fetchImpl(url, init))
      const summary = protocol === 'anthropic'
        ? summarizeAnthropicSse(text)
        : summarizeSse(parseSse(text))
      const output = protocol === 'anthropic' ? summary.text : openAiResponseText(text)
      results.push({
        model: id,
        provider,
        result: response.ok && summary.done && output.includes(INTERACTION_TEXT) ? 'passed' : 'failed',
        response_text: output.slice(0, 1_000),
        request: requestCapture,
        response: captureResponse(response, text),
        status: response.status,
        latency_ms: Number((performance.now() - started).toFixed(3)),
      })
    } catch (error) {
      results.push({ model: id, provider, request: requestCapture, response: null, result: 'failed', error: error.message, latency_ms: Number((performance.now() - started).toFixed(3)) })
    }
  }
  return results
}

export async function runClientInteractionSmoke({ apiKey = '', clients = [...CLIENTS], fetchImpl = fetch, hubUrl = DEFAULT_HUB_URL, maxModelsPerProvider = Infinity } = {}) {
  const normalizedHubUrl = trimTrailingSlash(hubUrl)
  const { response, text } = await responseText(await fetchImpl(new URL('/v1/models', `${normalizedHubUrl}/`).toString(), {
    headers: buildAuthHeaders(apiKey),
  }))
  if (!response.ok) throw new Error(`GET /v1/models failed with status ${response.status}`)
  const fetchedModels = selectProviderModels(tryJson(text))
  const models = limitModelsPerProvider(fetchedModels, maxModelsPerProvider)
  const openAiClients = clients.filter(client => client !== 'claude')
  const openai = openAiClients.length === 0
    ? []
    : await runInteraction({ apiKey, fetchImpl, hubUrl: normalizedHubUrl, models, protocol: 'openai' })
  const anthropic = clients.includes('claude')
    ? await runInteraction({ apiKey, fetchImpl, hubUrl: normalizedHubUrl, models, protocol: 'anthropic' })
    : []
  const configurationModel = models[0] ? routedModelId(models[0].provider, models[0].id) : 'provider:model'
  const clientResults = Object.fromEntries(clients.map(client => [client, {
    configuration: clientConfiguration(client, { hubUrl: normalizedHubUrl, model: configurationModel }),
    results: client === 'claude' ? anthropic : openai,
  }]))
  const allResults = [...openai, ...anthropic]
  return {
    clients: clientResults,
    hub: normalizedHubUrl,
    summary: {
      failed: allResults.filter(result => result.result !== 'passed').length,
      fetched_models: fetchedModels.length,
      models: models.length,
      passed: allResults.filter(result => result.result === 'passed').length,
      protocols: Number(openai.length > 0) + Number(anthropic.length > 0),
      scheduled_models: models.length,
    },
  }
}

export function helpText() {
  return 'Usage: node scripts/client-interaction-smoke.mjs [--hub URL] [--api-key KEY] [--client kilo,claude,pi,opencode]'
}

export async function main(argv = process.argv.slice(2), env = process.env) {
  const options = parseCliArgs(argv, env)
  if (options.help) {
    console.log(helpText())
    return 0
  }
  const report = await runClientInteractionSmoke(options)
  console.log(JSON.stringify(report, null, 2))
  return report.summary.failed === 0 ? 0 : 1
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  main().then(code => { process.exitCode = code }).catch(error => {
    console.error(error.message)
    process.exitCode = 1
  })
}
