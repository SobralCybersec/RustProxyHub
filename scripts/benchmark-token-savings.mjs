#!/usr/bin/env node
import fs from 'node:fs'
import { compactStructuredPrompt, trimPromptFormatting } from '../src-tauri/resources/playwright-bridge/prompt-compaction.mjs'

function estimateTokens(text) {
  return Math.ceil([...String(text)].length / 3.5)
}

const sample = `System: keep every semantic instruction.   


User: summarize this repo.      



Assistant: inspecting files.   

User: keep whitespace cleanup only.      `

const inputPath = process.argv[2]
const input = inputPath ? fs.readFileSync(inputPath, 'utf8') : sample
const maxChars = Number(process.argv[3] || 18000)
const trimmed = trimPromptFormatting(input)
const structured = compactStructuredPrompt(input, { maxChars })
const baselineTokens = estimateTokens(input)
const trimmedTokens = estimateTokens(trimmed)
const structuredTokens = estimateTokens(structured.text)

function percent(saved, baseline) {
  return baseline === 0 ? 0 : Number(((saved / baseline) * 100).toFixed(2))
}

console.log(JSON.stringify({
  input: inputPath || '<built-in sample>',
  max_chars: maxChars,
  baseline_chars: [...input].length,
  trimmed_chars: [...trimmed].length,
  structured_chars: [...structured.text].length,
  baseline_tokens: baselineTokens,
  trimmed_tokens: trimmedTokens,
  structured_tokens: structuredTokens,
  trimmed_tokens_saved: Math.max(0, baselineTokens - trimmedTokens),
  structured_tokens_saved: Math.max(0, baselineTokens - structuredTokens),
  trimmed_savings_percent: percent(Math.max(0, baselineTokens - trimmedTokens), baselineTokens),
  structured_savings_percent: percent(Math.max(0, baselineTokens - structuredTokens), baselineTokens),
  structured_mode: structured.mode,
  structured_truncated: structured.truncated,
  duplicate_blocks_removed: structured.removedDuplicateBlocks,
  omitted_blocks: structured.omittedBlocks,
}, null, 2))
