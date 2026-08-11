const DEFAULT_CHATGPT_SESSION_KEY = '__rust_proxy_hub_default_chatgpt_thread__'

export function chatGPTSessionKey(value) {
  const key = typeof value === 'string' ? value.trim() : ''
  return key && key.length <= 256 ? key : DEFAULT_CHATGPT_SESSION_KEY
}

export function chatGPTSessionFromTemplate(template) {
  let payload = null
  try {
    payload = typeof template?.payload === 'string' ? JSON.parse(template.payload) : template?.payload
  } catch {}
  const conversationId = payload?.conversation_id || payload?.conversationId
  const parentMessageId = payload?.parent_message_id || payload?.parentMessageId
  if (typeof conversationId !== 'string' || !conversationId || typeof parentMessageId !== 'string' || !parentMessageId) {
    return null
  }
  return { conversation_id: conversationId, parent_message_id: parentMessageId }
}

export function latestChatGPTAssistantMessageId(payload) {
  const mapping = payload?.mapping && typeof payload.mapping === 'object' ? Object.values(payload.mapping) : []
  const messages = [
    ...(payload?.message?.author?.role === 'assistant' ? [payload.message] : []),
    ...mapping.map(entry => entry?.message),
  ]
    .filter(message => message?.author?.role === 'assistant' && typeof message?.id === 'string' && message.id)
    .sort((left, right) => (left?.create_time || 0) - (right?.create_time || 0))
  return messages.at(-1)?.id || ''
}

export function applyChatGPTConversationSession(payload, session) {
  if (session?.conversation_id && session?.parent_message_id) {
    payload.conversation_id = session.conversation_id
    payload.parent_message_id = session.parent_message_id
  } else {
    payload.parent_message_id = 'client-created-root'
  }
  return payload
}
