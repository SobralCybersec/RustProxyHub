import assert from 'node:assert/strict'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import test from 'node:test'

import {
  loadChatGPTWebSessions,
  saveChatGPTWebSessions,
} from '../../src-tauri/resources/playwright-bridge/chatgpt-web-session.mjs'

test('ChatGPT web sessions persist valid cursor pairs only', () => {
  const runtimeDir = fs.mkdtempSync(path.join(os.tmpdir(), 'rust-proxy-hub-chatgpt-session-'))
  const sessions = new Map([
    ['client', { conversation_id: 'chat-1', parent_message_id: 'message-1', updated_at: 1 }],
    ['bad', { conversation_id: 'chat-2' }],
  ])
  saveChatGPTWebSessions(runtimeDir, sessions)

  assert.deepEqual(
    loadChatGPTWebSessions(runtimeDir),
    new Map([['client', { conversation_id: 'chat-1', parent_message_id: 'message-1', updated_at: 1 }]])
  )
  fs.rmSync(runtimeDir, { recursive: true, force: true })
})
