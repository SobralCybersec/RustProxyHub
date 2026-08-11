import assert from 'node:assert/strict'
import test from 'node:test'
import {
  cleanChatGPTUiAssistantText,
  extractChatGPTAssistantText,
  extractChatGPTAssistantTextFromSse,
} from '../../src-tauri/resources/playwright-bridge/chatgpt-web-response.mjs'

test('extracts latest ChatGPT assistant response from structured parts', () => {
  const response = extractChatGPTAssistantText({
    mapping: {
      first: {
        message: {
          author: { role: 'assistant' },
          create_time: 1,
          content: { parts: ['old response'] },
        },
      },
      latest: {
        message: {
          author: { role: 'assistant' },
          create_time: 2,
          content: {
            content_type: 'multimodal_text',
            parts: [{ content: { text: 'fresh response' } }],
          },
        },
      },
    },
  })

  assert.equal(response, 'fresh response')
})

test('ignores non-assistant messages and unknown scalar metadata', () => {
  const response = extractChatGPTAssistantText({
    mapping: {
      user: {
        message: {
          author: { role: 'user' },
          content: { parts: ['user content'] },
        },
      },
      assistant: {
        message: {
          author: { role: 'assistant' },
          content: { content_type: 'text', parts: ['assistant content'], model_slug: 'gpt-test' },
        },
      },
    },
  })

  assert.equal(response, 'assistant content')
})

test('removes ChatGPT work-status text and echoed submitted prompt', () => {
  assert.equal(
    cleanChatGPTUiAssistantText(
      'Test your tools, dive deep into our workspace.\nWorked for 12s',
      'Test your tools, dive deep into our workspace.'
    ),
    ''
  )
  assert.equal(cleanChatGPTUiAssistantText('Modified 1 file ▥'), '')
  assert.equal(
    cleanChatGPTUiAssistantText(
      'Test your tools, dive deep into our workspace.\nActual answer',
      'Test your tools, dive deep into our workspace.'
    ),
    'Actual answer'
  )
})

test('extracts final assistant text directly from ChatGPT conversation SSE', () => {
  const raw = [
    'data: {"message":{"author":{"role":"assistant"},"create_time":1,"content":{"parts":["partial"]}}}',
    'data: {"message":{"author":{"role":"assistant"},"create_time":2,"content":{"parts":["final answer"]}}}',
    'data: [DONE]',
  ].join('\n')

  assert.equal(extractChatGPTAssistantTextFromSse(raw), 'final answer')
  assert.equal(
    extractChatGPTAssistantTextFromSse(
      'data: {"message":{"author":{"role":"assistant"},"content":{"parts":["still working"]}}}'
    ),
    ''
  )
})

test('removes echoed prompt from structured SSE assistant content', () => {
  const prompt = 'Use your tools. Dive deep into our workspace.'
  const raw = [
    'data: {"message":{"author":{"role":"assistant"},"content":{"parts":["Use your tools. Dive deep into our workspace.","Actual answer"]}}}',
    'data: [DONE]',
  ].join('\n')

  assert.equal(extractChatGPTAssistantTextFromSse(raw, prompt), 'Actual answer')
})
