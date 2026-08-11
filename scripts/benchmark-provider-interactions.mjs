#!/usr/bin/env node
import { appendFile, mkdir, readFile, writeFile } from 'node:fs/promises'
import { performance } from 'node:perf_hooks'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { runClientInteractionSmoke } from './client-interaction-smoke.mjs'
import { runProviderToolSmoke } from './provider-tool-smoke.mjs'

const DEFAULT_HUB_URL = 'http://127.0.0.1:3100'
const DEFAULT_HISTORY_DIR = 'benchmark-history'
const DEFAULT_MAX_MODELS_PER_PROVIDER = 8

function positiveInteger(value, name) {
  const parsed = Number.parseInt(value, 10)
  if (!Number.isSafeInteger(parsed) || parsed < 1) throw new Error(`${name} must be a positive integer`)
  return parsed
}

function modelLimit(value, name) {
  const parsed = Number.parseInt(value, 10)
  if (!Number.isSafeInteger(parsed) || parsed < 0) throw new Error(`${name} must be a non-negative integer`)
  return parsed === 0 ? Infinity : parsed
}

export function parseCliArgs(argv, env = process.env) {
  const options = {
    apiKey: env.RUST_PROXY_HUB_API_KEY || '',
    historyDir: env.RUST_PROXY_HUB_BENCHMARK_HISTORY_DIR || DEFAULT_HISTORY_DIR,
    hubUrl: env.RUST_PROXY_HUB_URL || DEFAULT_HUB_URL,
    iterations: positiveInteger(env.BENCH_ITERATIONS || '1', 'BENCH_ITERATIONS'),
    maxModelsPerProvider: modelLimit(env.BENCH_MAX_MODELS_PER_PROVIDER || String(DEFAULT_MAX_MODELS_PER_PROVIDER), 'BENCH_MAX_MODELS_PER_PROVIDER'),
  }
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index]
    if (argument === '--' && index === 0) continue
    if (argument === '--help' || argument === '-h') return { help: true }
    if (!['--hub', '--api-key', '--history-dir', '--iterations', '--max-models-per-provider'].includes(argument)) {
      throw new Error(`Unknown argument: ${argument}`)
    }
    const value = argv[index + 1]
    if (!value) throw new Error(`${argument} requires a value`)
    if (argument === '--hub') options.hubUrl = value
    else if (argument === '--api-key') options.apiKey = value
    else if (argument === '--history-dir') options.historyDir = value
    else if (argument === '--iterations') options.iterations = positiveInteger(value, '--iterations')
    else options.maxModelsPerProvider = modelLimit(value, '--max-models-per-provider')
    index += 1
  }
  const url = new URL(options.hubUrl)
  if (!['http:', 'https:'].includes(url.protocol)) throw new Error('--hub must use http or https')
  options.hubUrl = url.toString().replace(/\/+$/, '')
  return options
}

function compactToolResult(result, providerLogs) {
  const log = providerLogs[result.provider] || { available: false }
  const entries = Array.isArray(log.entries) ? log.entries.slice(-10) : []
  return {
    kind: 'tool_call',
    latency_ms: result.latency_ms,
    model: result.routed_model,
    observed: {
      finish_reasons: result.sse?.finish_reasons || [],
      provider_log: {
        available: log.available === true,
        entries,
        entry_count: entries.length,
      },
      tool_call_detected: result.tool_call_detected,
    },
    provider: result.provider,
    request: result.request,
    result: result.result,
    response: result.response,
    status: result.status || null,
  }
}

function compactInteractionResult(protocol, result) {
  return {
    kind: 'prompt_tool_result_interaction',
    latency_ms: result.latency_ms,
    model: result.model.includes(':') ? result.model : `${result.provider}:${result.model}`,
    observed: { interaction_confirmation: result.result === 'passed' },
    protocol,
    provider: result.provider,
    request: result.request,
    result: result.result,
    response: result.response,
    response_text: result.response_text || '',
    status: result.status || null,
  }
}

function percentile(values, fraction) {
  if (values.length === 0) return null
  const sorted = [...values].sort((left, right) => left - right)
  return sorted[Math.min(sorted.length - 1, Math.ceil(sorted.length * fraction) - 1)]
}

export async function runProviderInteractionBenchmark({ apiKey = '', fetchImpl = fetch, hubUrl = DEFAULT_HUB_URL, iterations = 1, maxModelsPerProvider = DEFAULT_MAX_MODELS_PER_PROVIDER } = {}) {
  const runs = []
  for (let iteration = 1; iteration <= iterations; iteration += 1) {
    const started = performance.now()
    const tools = await runProviderToolSmoke({ apiKey, fetchImpl, hubUrl, maxModelsPerProvider })
    const interactions = await runClientInteractionSmoke({ apiKey, fetchImpl, hubUrl, maxModelsPerProvider })
    runs.push({
      elapsed_ms: Number((performance.now() - started).toFixed(3)),
      interactions,
      iteration,
      tools,
    })
  }
  const conversationResults = runs.flatMap(run => {
    const clients = Object.values(run.interactions.clients)
    const anthropic = clients.flatMap(client => client.configuration.protocol === 'anthropic-messages'
      ? client.results.map(result => compactInteractionResult('anthropic-messages', result))
      : [])
    const openai = clients.some(client => client.configuration.provider_api === 'OpenAI Compatible')
      ? (run.interactions.clients.kilo?.results || []).map(result => compactInteractionResult('openai-chat-completions', result))
      : []
    return [...run.tools.results.map(result => compactToolResult(result, run.tools.provider_logs)), ...anthropic, ...openai]
  })
  const latencies = conversationResults.map(result => result.latency_ms).filter(Number.isFinite)
  const elapsedMs = Number(runs.reduce((total, run) => total + run.elapsed_ms, 0).toFixed(3))
  const scheduledModels = [...new Set(runs.flatMap(run => run.tools.results.map(result => result.routed_model)))].sort()
  const scheduledProviders = [...new Set(runs.flatMap(run => run.tools.results.map(result => result.provider)))].sort()
  const modelResults = new Map(scheduledModels.map(model => [model, conversationResults.filter(result => result.model === model)]))
  const providerResults = new Map(scheduledProviders.map(provider => [provider, conversationResults.filter(result => result.provider === provider)]))
  const workedModels = [...modelResults.values()].filter(results => results.length > 0 && results.every(result => result.result === 'passed')).length
  const workedProviders = [...providerResults.values()].filter(results => results.length > 0 && results.every(result => result.result === 'passed')).length
  const logEntries = conversationResults
    .filter(result => result.kind === 'tool_call')
    .reduce((total, result) => total + (result.observed.provider_log.entry_count || 0), 0)
  const logsFetched = conversationResults.filter(result => result.kind === 'tool_call' && result.observed.provider_log.available).length
  return {
    benchmark: 'provider_model_tool_and_interaction',
    conversation_results: conversationResults,
    generated_at: new Date().toISOString(),
    harness: {
      clients: ['kilo', 'claude', 'pi', 'opencode'],
      max_models_per_provider: maxModelsPerProvider === Infinity ? 'all' : maxModelsPerProvider,
      task_contract: 'forced_tool_call + prompt_tool_result_interaction:v1',
    },
    hub: hubUrl.replace(/\/+$/, ''),
    iterations,
    provider_logs: runs.map(run => ({
      iteration: run.iteration,
      providers: run.tools.provider_logs,
    })),
    summary: {
      failed: conversationResults.filter(result => result.result !== 'passed').length,
      latency_ms: {
        p50: percentile(latencies, 0.5),
        p95: percentile(latencies, 0.95),
        total: elapsedMs,
      },
      log_entries: logEntries,
      logs_fetched: logsFetched,
      models: scheduledModels.length,
      models_fetched: Math.max(0, ...runs.map(run => run.tools.summary.fetched_models)),
      models_scheduled: scheduledModels.length,
      models_worked: workedModels,
      passed: conversationResults.filter(result => result.result === 'passed').length,
      providers: scheduledProviders.length,
      providers_fetched: Math.max(0, ...runs.map(run => run.tools.summary.fetched_providers)),
      providers_scheduled: scheduledProviders.length,
      providers_worked: workedProviders,
      requests: conversationResults.length,
    },
  }
}

function readHistory(text) {
  return text.split('\n').filter(Boolean).map(line => JSON.parse(line))
}

export function renderHistoryMarkdown(history) {
  const latest = history.at(-1)
  const rows = history.map(run => `| ${run.generated_at} | ${run.summary.providers_worked}/${run.summary.providers_fetched} | ${run.summary.models_worked}/${run.summary.models_fetched} | ${run.summary.logs_fetched} | ${run.summary.requests} | ${run.summary.passed} | ${run.summary.failed} | ${run.summary.latency_ms.total ?? 'n/a'} | ${run.summary.latency_ms.p50 ?? 'n/a'} | ${run.summary.latency_ms.p95 ?? 'n/a'} |`)
  const resultRows = (latest?.conversation_results || []).map(result => `| ${result.kind} | ${result.provider} | ${result.model} | ${result.protocol || 'openai-chat-completions'} | ${result.status ?? 'n/a'} | ${result.latency_ms ?? 'n/a'} | ${String(result.response_text || '').replace(/[|\n]/g, ' ').slice(0, 160) || 'n/a'} | ${result.result} |`)
  return [
    '# Provider/model benchmark history',
    '',
    `Harness: ${latest?.harness?.task_contract || 'unknown'}; clients: ${(latest?.harness?.clients || []).join(', ') || 'unknown'}.`,
    '',
    'Each run records deterministic tool calls plus prompt → tool-result interactions. Latency is local observed wall time; it includes provider/browser latency and is not a proxy-throughput claim.',
    '',
    '| Generated | Providers worked/fetched | Models worked/fetched | Logs fetched | Requests | Passed | Failed | Total ms | p50 ms | p95 ms |',
    '|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|',
    ...rows,
    '',
    '## Latest conversation results',
    '',
    '| Kind | Provider | Model | Protocol | HTTP | Latency ms | Output preview | Result |',
    '|---|---|---|---|---:|---:|---|---|',
    ...resultRows,
    '',
  ].join('\n')
}

export async function writeBenchmarkHistory(historyDir, report) {
  const directory = resolve(historyDir)
  const jsonPath = resolve(directory, 'provider-model-history.jsonl')
  const markdownPath = resolve(directory, 'provider-model-history.md')
  await mkdir(directory, { recursive: true })
  await appendFile(jsonPath, `${JSON.stringify(report)}\n`)
  const history = readHistory(await readFile(jsonPath, 'utf8'))
  await writeFile(markdownPath, renderHistoryMarkdown(history))
  return { json_path: jsonPath, markdown_path: markdownPath, runs: history.length }
}

export function helpText() {
  return 'Usage: node scripts/benchmark-provider-interactions.mjs [--hub URL] [--api-key KEY] [--history-dir PATH] [--iterations N] [--max-models-per-provider N] (0 = all)'
}

export async function main(argv = process.argv.slice(2), env = process.env) {
  const options = parseCliArgs(argv, env)
  if (options.help) {
    console.log(helpText())
    return 0
  }
  const report = await runProviderInteractionBenchmark(options)
  const history = await writeBenchmarkHistory(options.historyDir, report)
  console.log(JSON.stringify({ ...report, history }, null, 2))
  return report.summary.failed === 0 ? 0 : 1
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  main().then(code => { process.exitCode = code }).catch(error => {
    console.error(error.message)
    process.exitCode = 1
  })
}
