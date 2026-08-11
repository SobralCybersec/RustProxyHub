#!/usr/bin/env node
import { performance } from 'node:perf_hooks'
import { parseSse, summarizeSse } from './provider-tool-smoke.mjs'

const iterations = Number.parseInt(process.env.BENCH_ITERATIONS || '10000', 10)
if (!Number.isSafeInteger(iterations) || iterations < 1) throw new Error('BENCH_ITERATIONS must be a positive integer')

const fixture = [
  `data: ${JSON.stringify({ choices: [{ delta: { tool_calls: [{ index: 0, id: 'call_1', type: 'function', function: { name: 'report_smoke_target', arguments: '{"provider":"qwen"' } }] } }] })}`,
  '',
  `data: ${JSON.stringify({ choices: [{ delta: { tool_calls: [{ index: 0, function: { arguments: ',"model":"qwen3"}' } }] }, finish_reason: 'tool_calls' }] })}`,
  '',
  'data: [DONE]',
  '',
].join('\n')

for (let index = 0; index < 1_000; index += 1) summarizeSse(parseSse(fixture))

const started = performance.now()
let toolCalls = 0
for (let index = 0; index < iterations; index += 1) {
  toolCalls += summarizeSse(parseSse(fixture)).tool_calls.length
}
const elapsedMs = performance.now() - started

console.log(JSON.stringify({
  benchmark: 'provider_tool_smoke_sse_parse',
  elapsed_ms: Number(elapsedMs.toFixed(3)),
  iterations,
  operations_per_second: Number((iterations / (elapsedMs / 1_000)).toFixed(2)),
  tool_calls: toolCalls,
}, null, 2))
