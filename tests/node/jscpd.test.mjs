import assert from 'node:assert/strict'
import test from 'node:test'
import { buildJscpdArgs, parseCliArgs, summarizeReport } from '../../scripts/jscpd.mjs'

test('jscpd CLI parses report and metric options', () => {
  const options = parseCliArgs([
    'src',
    '--reporters',
    'console,markdown',
    '--output',
    'tmp/jscpd',
    '--metrics',
    'tmp/metrics.json',
    '--min-lines',
    '12',
    '--threshold',
    '3',
  ])

  assert.deepEqual(options.paths, ['src'])
  assert.deepEqual(options.reporters, ['console', 'markdown', 'json'])
  assert.deepEqual(options.ignores, ['src-tauri/resources/node/**', 'README.pt-BR.md'])
  assert.equal(options.minLines, 12)
  assert.equal(options.threshold, 3)
  assert.deepEqual(buildJscpdArgs(options), [
    'src',
    '--reporters',
    'console,markdown,json',
    '--output',
    options.output,
    '--min-lines',
    '12',
    '--threshold',
    '3',
    '--ignore',
    'src-tauri/resources/node/**,README.pt-BR.md',
    '--no-colors',
    '--no-tips',
  ])
})

test('jscpd CLI always includes JSON reporter and summarizes total metrics', () => {
  const options = parseCliArgs([])
  const metrics = summarizeReport(
    {
      statistics: {
        detectionDate: '2026-08-12T00:00:00.000Z',
        total: {
          clones: 2,
          duplicatedLines: 24,
          lines: 1000,
          percentage: 2.4,
          sources: 8,
          tokens: 4000,
        },
      },
    },
    options
  )

  assert.equal(options.reporters.includes('json'), true)
  assert.equal(options.ignores.includes('src-tauri/resources/node/**'), true)
  assert.deepEqual(metrics.summary, {
    clones: 2,
    duplicated_lines: 24,
    duplication_percent: 2.4,
    files: 8,
    lines: 1000,
    tokens: 4000,
  })
})

test('jscpd CLI rejects invalid thresholds', () => {
  assert.throws(() => parseCliArgs(['--threshold', '101']), /between 0 and 100/)
  assert.throws(() => parseCliArgs(['--min-lines', '0']), /positive integer/)
})
