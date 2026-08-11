import assert from 'node:assert/strict'
import test from 'node:test'
import {
  compactStructuredPrompt,
  summarizePromptCompaction,
  trimPromptFormatting,
} from '../../src-tauri/resources/playwright-bridge/prompt-compaction.mjs'

test('trimPromptFormatting removes trailing whitespace and extra blank lines', () => {
  assert.equal(trimPromptFormatting('User: hi   \n\n\nAssistant: hello\t \n'), 'User: hi\n\nAssistant: hello')
})

test('compactStructuredPrompt preserves first turn and latest turn within limit', () => {
  const prompt = [
    'User: system bootstrap and repo contract',
    'Assistant: acknowledged',
    'User: very old context that can be omitted safely',
    'Assistant: old answer',
    'User: latest request with exact task',
    'Assistant: latest answer',
  ].join('\n\n')

  const compacted = compactStructuredPrompt(prompt, { maxChars: 145 })

  assert.equal(compacted.truncated, true)
  assert.match(compacted.text, /^User: system bootstrap/)
  assert.match(compacted.text, /User: latest request with exact task/)
  assert.doesNotMatch(compacted.text, /very old context/)
})

test('compactStructuredPrompt removes duplicate blocks before omitting unique turns', () => {
  const prompt = [
    'User: repeated turn',
    'Assistant: same output',
    'Assistant: same output',
    'Assistant: same output',
    'User: final turn',
  ].join('\n\n')

  const compacted = compactStructuredPrompt(prompt, { maxChars: 70 })

  assert.equal(compacted.truncated, true)
  assert.ok(compacted.removedDuplicateBlocks >= 1)
  assert.match(compacted.text, /User: final turn/)
})

test('summarizePromptCompaction reports saved chars and structural actions', () => {
  const summary = summarizePromptCompaction({
    truncated: true,
    originalChars: 200,
    compactedChars: 120,
    removedDuplicateBlocks: 2,
    omittedBlocks: 1,
  })

  assert.match(summary, /80 chars saved/)
  assert.match(summary, /2 duplicate block/)
  assert.match(summary, /1 earlier block/)
})
