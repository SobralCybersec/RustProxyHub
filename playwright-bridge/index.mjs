import { createHash, randomUUID } from 'node:crypto'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = path.dirname(fileURLToPath(import.meta.url))
async function importPlaywright() {
  const candidateUrls = [
    new URL('./node_modules/playwright/index.mjs', import.meta.url),
    new URL('../node_modules/playwright/index.mjs', import.meta.url),
    new URL('../../node_modules/playwright/index.mjs', import.meta.url),
  ]

  for (const candidate of candidateUrls) {
    if (fs.existsSync(fileURLToPath(candidate))) {
      return import(candidate)
    }
  }

  return import('playwright')
}

const playwright = await importPlaywright()
const { chromium, firefox, webkit } = playwright

function ensureDir(dir) {
  fs.mkdirSync(dir, { recursive: true })
}

function resolveEngine(browser) {
  switch (browser) {
    case 'firefox':
      return { engine: firefox }
    case 'webkit':
      return { engine: webkit }
    case 'chrome':
      return { engine: chromium, channel: 'chrome' }
    case 'edge':
    case 'msedge':
      return { engine: chromium, channel: 'msedge' }
    case 'chromium':
    default:
      return { engine: chromium }
  }
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

function send(id, result = null, error = null) {
  process.stdout.write(`${JSON.stringify({ id, result, error })}\n`)
}

const state = {
  deepseek: {
    context: null,
    page: null,
    headless: null,
  },
  chatgpt: {
    context: null,
    page: null,
    headless: null,
    cachedHeaders: null,
    lastHeadersTime: 0,
  },
  gemini: {
    context: null,
    page: null,
    headless: null,
    cachedHeaders: null,
    lastHeadersTime: 0,
  },
  kimi: {
    context: null,
    page: null,
    headless: null,
    currentHeaders: {},
    cachedHeaders: null,
    lastHeadersTime: 0,
  },
  qwen: {
    context: null,
    page: null,
    headless: null,
    currentHeaders: {},
    cachedHeaders: null,
    lastHeadersTime: 0,
    cookieCache: null,
    userAgent: null,
  },
}

const qwenAccounts = new Map()

function freshQwenSlot() {
  return {
    context: null,
    page: null,
    headless: null,
    currentHeaders: {},
    cachedHeaders: null,
    lastHeadersTime: 0,
    cookieCache: null,
    userAgent: null,
  }
}

function getQwenSlot(accountId = null) {
  if (!accountId) {
    return state.qwen
  }
  if (!qwenAccounts.has(accountId)) {
    qwenAccounts.set(accountId, freshQwenSlot())
  }
  return qwenAccounts.get(accountId)
}

function resetQwenSlot(slot) {
  slot.context = null
  slot.page = null
  slot.headless = null
  slot.currentHeaders = {}
  slot.cachedHeaders = null
  slot.lastHeadersTime = 0
  slot.cookieCache = null
  slot.userAgent = null
}

function ensureSessionText(value, fallback) {
  return typeof value === 'string' && value.trim() ? value : fallback
}

async function initDeepSeek({ runtime_dir, headless, browser }) {
  process.chdir(runtime_dir)
  if (state.deepseek.context && state.deepseek.headless === headless) return
  if (state.deepseek.context) {
    await state.deepseek.context.close()
    state.deepseek.context = null
    state.deepseek.page = null
  }
  ensureDir(path.resolve('deepseek_profile'))
  const { engine, channel } = resolveEngine(browser)
  state.deepseek.context = await engine.launchPersistentContext(path.resolve('deepseek_profile'), {
    headless,
    channel,
    userAgent: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36',
    args: [
      '--disable-blink-features=AutomationControlled',
      '--exclude-switches=enable-automation',
      '--disable-infobars',
      '--no-sandbox',
      '--disable-dev-shm-usage',
      '--disable-gpu',
    ],
  })
  state.deepseek.page = await state.deepseek.context.newPage()
  state.deepseek.headless = headless
}

async function captureDeepSeekHeaders(forceNew = false) {
  const page = state.deepseek.page
  if (!page) throw new Error('DeepSeek Playwright not initialized')

  const currentUrl = page.url()
  const isOnDeepSeek = currentUrl.includes('chat.deepseek.com')
  const isOnSpecificChat = isOnDeepSeek && /\/chat\/\d+/.test(currentUrl)
  if (!isOnDeepSeek || forceNew || isOnSpecificChat) {
    await page.goto('https://chat.deepseek.com/', { waitUntil: 'domcontentloaded' })
  }

  await page.waitForSelector('textarea', { timeout: 30000 }).catch(() => {
    throw new Error('Timeout waiting for DeepSeek chat input. Are you logged in?')
  })

  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error('Timeout waiting for DeepSeek headers')), 30000)
    const routeHandler = async (route, request) => {
      clearTimeout(timeout)
      const reqHeaders = request.headers()
      let chatSessionId = ''
      let parentMessageId = null

      const postData = request.postData()
      if (postData) {
        try {
          const payload = JSON.parse(postData)
          chatSessionId = payload.chat_session_id || ''
          parentMessageId = payload.parent_message_id ?? null
        } catch {}
      }

      const headers = {
        authorization: reqHeaders.authorization || '',
        cookie: reqHeaders.cookie || '',
        'x-ds-pow-response': reqHeaders['x-ds-pow-response'] || '',
        'x-hif-dliq': reqHeaders['x-hif-dliq'] || '',
        'x-hif-leim': reqHeaders['x-hif-leim'] || '',
      }

      await route.abort('aborted')
      await page.unroute('**/api/v0/chat/completion', routeHandler)
      resolve({ headers, chat_session_id: chatSessionId, parent_message_id: parentMessageId })
    }

    page.route('**/api/v0/chat/completion', routeHandler).then(async () => {
      await page.fill('textarea', 'a')
      await page.keyboard.press('Enter')
    })
  })
}

async function openDeepSeekLogin({ runtime_dir, browser }) {
  await initDeepSeek({ runtime_dir, headless: false, browser })
  await state.deepseek.page.goto('https://chat.deepseek.com/', { waitUntil: 'domcontentloaded' })
}

async function initChatGPT({ runtime_dir, headless, browser }) {
  process.chdir(runtime_dir)
  if (state.chatgpt.context && state.chatgpt.headless === headless) return
  if (state.chatgpt.context) {
    await state.chatgpt.context.close()
    state.chatgpt.context = null
    state.chatgpt.page = null
    state.chatgpt.cachedHeaders = null
    state.chatgpt.lastHeadersTime = 0
  }
  ensureDir(path.resolve('chatgpt_profile'))
  const { engine, channel } = resolveEngine(browser)
  state.chatgpt.context = await engine.launchPersistentContext(path.resolve('chatgpt_profile'), {
    headless,
    channel,
    userAgent:
      'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/147.0.0.0 Safari/537.36',
    ignoreDefaultArgs: ['--enable-automation'],
    args: ['--disable-blink-features=AutomationControlled'],
  })
  await state.chatgpt.context.addInitScript(() => {
    Object.defineProperty(navigator, 'webdriver', { get: () => undefined })
  })
  state.chatgpt.page = await state.chatgpt.context.newPage()
  state.chatgpt.headless = headless
}

async function captureChatGPTTemplate(forceNew = false) {
  const page = state.chatgpt.page
  if (!page) throw new Error('ChatGPT Playwright not initialized')

  if (!forceNew && state.chatgpt.cachedHeaders && Date.now() - state.chatgpt.lastHeadersTime < 5 * 60 * 1000) {
    return state.chatgpt.cachedHeaders
  }

  if (!page.url().includes('chatgpt.com') || forceNew) {
    await page.goto('https://chatgpt.com/', { waitUntil: 'domcontentloaded' })
  }

  const inputSelector = 'textarea:visible, #prompt-textarea:visible, div[contenteditable="true"]:visible'
  await page.waitForSelector(inputSelector, { timeout: 30000 }).catch(() => {
    throw new Error('Timeout waiting for ChatGPT input. Are you logged in?')
  })

  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error('Timeout waiting for ChatGPT request template')), 60000)
    const routeHandler = async (route, request) => {
      clearTimeout(timeout)
      const reqHeaders = request.headers()
      const postData = request.postData() || ''
      let payloadModel = 'chatgpt-web-session'

      try {
        payloadModel = JSON.parse(postData).model || payloadModel
      } catch {}

      const headers = {
        authorization: reqHeaders.authorization || '',
        accept: reqHeaders.accept || 'text/event-stream',
        'accept-language': reqHeaders['accept-language'] || 'en-US,en;q=0.9',
        'content-type': reqHeaders['content-type'] || 'application/json',
        origin: reqHeaders.origin || 'https://chatgpt.com',
        referer: reqHeaders.referer || 'https://chatgpt.com/',
        'user-agent': reqHeaders['user-agent'] || '',
        'oai-client-build-number': reqHeaders['oai-client-build-number'] || '',
        'oai-client-version': reqHeaders['oai-client-version'] || '',
        'oai-device-id': reqHeaders['oai-device-id'] || '',
        'oai-language': reqHeaders['oai-language'] || 'en-US',
        'oai-session-id': reqHeaders['oai-session-id'] || '',
        'openai-sentinel-chat-requirements-token': reqHeaders['openai-sentinel-chat-requirements-token'] || '',
        'openai-sentinel-proof-token': reqHeaders['openai-sentinel-proof-token'] || '',
        'openai-sentinel-turnstile-token': reqHeaders['openai-sentinel-turnstile-token'] || '',
        'x-conduit-token': reqHeaders['x-conduit-token'] || '',
        'x-oai-turn-trace-id': reqHeaders['x-oai-turn-trace-id'] || '',
        'x-openai-target-path': reqHeaders['x-openai-target-path'] || '/backend-api/f/conversation',
        'x-openai-target-route': reqHeaders['x-openai-target-route'] || '/backend-api/f/conversation',
      }

      if (!headers.authorization) {
        await route.continue()
        return
      }

      state.chatgpt.cachedHeaders = {
        headers,
        payload: postData,
        model: payloadModel,
        url: request.url(),
      }
      state.chatgpt.lastHeadersTime = Date.now()

      await route.abort('aborted')
      await page.unroute('**/backend-api/f/conversation*', routeHandler)
      resolve(state.chatgpt.cachedHeaders)
    }

    page.route('**/backend-api/f/conversation*', routeHandler).then(async () => {
      await page.focus(inputSelector)
      await page.fill(inputSelector, '')
      await page.type(inputSelector, 'a', { delay: 50 })
      await sleep(1500)
      await page.keyboard.press('Enter')
    })
  })
}

async function getChatGPTBasicHeaders() {
  const page = state.chatgpt.page
  if (!page) throw new Error('ChatGPT Playwright not initialized')

  const cookies = await page.context().cookies()
  const cookie = cookies.map((item) => `${item.name}=${item.value}`).join('; ')
  const userAgent = await page.evaluate(() => navigator.userAgent)
  const template = state.chatgpt.cachedHeaders

  return {
    headers: {
      cookie,
      authorization: template?.headers?.authorization || '',
      'user-agent': userAgent,
      origin: 'https://chatgpt.com',
      referer: 'https://chatgpt.com/',
    },
  }
}

function buildChatGPTPayload(prompt, model, webSearch) {
  return {
    action: 'next',
    messages: [
      {
        id: randomUUID(),
        author: { role: 'user' },
        create_time: Date.now() / 1000,
        content: {
          content_type: 'text',
          parts: [prompt],
        },
        metadata: {
          developer_mode_connector_ids: [],
          selected_sources: webSearch ? ['web'] : [],
          selected_github_repos: [],
          selected_all_github_repos: false,
          serialization_metadata: {
            custom_symbol_offsets: [],
          },
        },
      },
    ],
    parent_message_id: 'client-created-root',
    model,
    client_prepare_state: 'success',
    timezone_offset_min: -new Date().getTimezoneOffset(),
    timezone: Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC',
    conversation_mode: { kind: 'primary_assistant' },
    enable_message_followups: true,
    system_hints: [],
    supports_buffering: true,
    supported_encodings: ['v1'],
    client_contextual_info: {
      app_name: 'chatgpt.com',
    },
    paragen_cot_summary_display_override: 'allow',
    force_parallel_switch: 'auto',
    thinking_effort: model.includes('thinking') ? 'extended' : 'auto',
  }
}

function extractChatGPTAssistantText(payload) {
  if (!payload || typeof payload !== 'object') return ''
  const mapping = payload.mapping && typeof payload.mapping === 'object' ? Object.values(payload.mapping) : []
  const messages = mapping
    .map((entry) => entry?.message)
    .filter((message) => message?.author?.role === 'assistant')
    .sort((left, right) => (left?.create_time || 0) - (right?.create_time || 0))

  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const content = messages[index]?.content
    if (!content) continue
    const parts = Array.isArray(content.parts) ? content.parts : []
    const text = parts
      .filter((part) => typeof part === 'string')
      .join('\n')
      .trim()
    if (text) return text
  }

  return ''
}

async function listChatGPTModels() {
  const template = await captureChatGPTTemplate(false)
  return {
    data: [
      {
        id: ensureSessionText(template.model, 'chatgpt-web-session'),
        provider: 'chatgpt',
      },
    ],
  }
}

async function chatChatGPT({ model, prompt, web_search = false }) {
  const page = state.chatgpt.page
  if (!page) throw new Error('ChatGPT Playwright not initialized')

  const template = await captureChatGPTTemplate(true)
  const requestHeaders = { ...template.headers }
  delete requestHeaders.cookie

  const payload = buildChatGPTPayload(prompt, ensureSessionText(model, template.model || 'chatgpt-web-session'), web_search)
  const requestResult = await page.evaluate(async ({ headers, payload }) => {
    const response = await fetch('https://chatgpt.com/backend-api/f/conversation', {
      method: 'POST',
      credentials: 'include',
      headers,
      body: JSON.stringify(payload),
    })
    const reader = response.body?.getReader()
    const decoder = new TextDecoder()
    let raw = ''
    let conversationId = ''

    if (reader) {
      while (true) {
        const { done, value } = await reader.read()
        if (done) break
        raw += decoder.decode(value, { stream: true })
        const lines = raw.split('\n')
        raw = lines.pop() || ''
        for (const line of lines) {
          const trimmed = line.trim()
          if (!trimmed.startsWith('data:')) continue
          const chunk = trimmed.slice(5).trim()
          if (!chunk || chunk === '[DONE]') continue
          try {
            const parsed = JSON.parse(chunk)
            conversationId =
              parsed.conversation_id ||
              parsed.token?.conversation_id ||
              parsed.options?.[0]?.conversation_id ||
              conversationId
          } catch {}
        }
      }
    }

    return {
      ok: response.ok,
      status: response.status,
      conversationId,
    }
  }, { headers: requestHeaders, payload })

  if (!requestResult.ok || !requestResult.conversationId) {
    throw new Error(`ChatGPT upstream request failed with status ${requestResult.status}`)
  }

  const conversationJson = await page.evaluate(async ({ headers, conversationId }) => {
    for (let attempt = 0; attempt < 60; attempt += 1) {
      const response = await fetch(`https://chatgpt.com/backend-api/conversation/${conversationId}`, {
        method: 'GET',
        credentials: 'include',
        headers,
      })

      if (response.ok) {
        const text = await response.text()
        if (text && text !== 'null') {
          return text
        }
      }

      await new Promise((resolve) => setTimeout(resolve, 1000))
    }

    return ''
  }, { headers: requestHeaders, conversationId: requestResult.conversationId })

  const text = extractChatGPTAssistantText(conversationJson ? JSON.parse(conversationJson) : null)
  if (!text) {
    throw new Error('ChatGPT response was empty. Confirm session is active, then retry.')
  }

  return {
    text,
    model: payload.model,
    conversation_id: requestResult.conversationId,
    warning: web_search
      ? 'ChatGPT web search toggle not mapped yet. Current web-session defaults were used.'
      : null,
  }
}

async function openChatGPTLogin({ runtime_dir, browser }) {
  await initChatGPT({ runtime_dir, headless: false, browser })
  await state.chatgpt.page.goto('https://chatgpt.com/', { waitUntil: 'domcontentloaded' })
}

async function initGemini({ runtime_dir, headless, browser }) {
  process.chdir(runtime_dir)
  if (state.gemini.context && state.gemini.headless === headless) return
  if (state.gemini.context) {
    await state.gemini.context.close()
    state.gemini.context = null
    state.gemini.page = null
    state.gemini.cachedHeaders = null
    state.gemini.lastHeadersTime = 0
  }
  ensureDir(path.resolve('gemini_profile'))
  const { engine, channel } = resolveEngine(browser)
  state.gemini.context = await engine.launchPersistentContext(path.resolve('gemini_profile'), {
    headless,
    channel,
    userAgent:
      'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/147.0.0.0 Safari/537.36',
    ignoreDefaultArgs: ['--enable-automation'],
    args: ['--disable-blink-features=AutomationControlled'],
  })
  await state.gemini.context.addInitScript(() => {
    Object.defineProperty(navigator, 'webdriver', { get: () => undefined })
  })
  state.gemini.page = await state.gemini.context.newPage()
  state.gemini.headless = headless
}

async function captureGeminiTemplate(forceNew = false) {
  const page = state.gemini.page
  if (!page) throw new Error('Gemini Playwright not initialized')

  if (!forceNew && state.gemini.cachedHeaders && Date.now() - state.gemini.lastHeadersTime < 5 * 60 * 1000) {
    return state.gemini.cachedHeaders
  }

  if (!page.url().includes('gemini.google.com') || forceNew) {
    await page.goto('https://gemini.google.com/app', { waitUntil: 'domcontentloaded' })
  }

  const inputSelector = 'rich-textarea textarea, textarea:visible, div[contenteditable="true"]:visible'
  await page.waitForSelector(inputSelector, { timeout: 30000 }).catch(() => {
    throw new Error('Timeout waiting for Gemini input. Are you logged in?')
  })

  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error('Timeout waiting for Gemini request template')), 60000)
    const routeHandler = async (route, request) => {
      clearTimeout(timeout)
      const reqHeaders = request.headers()
      const headers = {
        accept: reqHeaders.accept || '*/*',
        'accept-language': reqHeaders['accept-language'] || 'en-US,en;q=0.9',
        'content-type': reqHeaders['content-type'] || 'application/x-www-form-urlencoded;charset=UTF-8',
        origin: reqHeaders.origin || 'https://gemini.google.com',
        referer: reqHeaders.referer || 'https://gemini.google.com/app',
        'user-agent': reqHeaders['user-agent'] || '',
        'x-goog-ext-525001261-jspb': reqHeaders['x-goog-ext-525001261-jspb'] || '',
        'x-goog-ext-525005358-jspb': reqHeaders['x-goog-ext-525005358-jspb'] || '',
        'x-same-domain': reqHeaders['x-same-domain'] || '1',
      }

      state.gemini.cachedHeaders = {
        headers,
        payload: request.postData() || '',
        url: request.url(),
      }
      state.gemini.lastHeadersTime = Date.now()

      await route.abort('aborted')
      await page.unroute('**/StreamGenerate*', routeHandler)
      resolve(state.gemini.cachedHeaders)
    }

    page.route('**/StreamGenerate*', routeHandler).then(async () => {
      await page.focus(inputSelector)
      await page.fill(inputSelector, '')
      await page.type(inputSelector, 'a', { delay: 50 })
      await sleep(1500)
      await page.keyboard.press('Enter')
    })
  })
}

async function getGeminiBasicHeaders() {
  const page = state.gemini.page
  if (!page) throw new Error('Gemini Playwright not initialized')

  const cookies = await page.context().cookies()
  const cookie = cookies.map((item) => `${item.name}=${item.value}`).join('; ')
  const userAgent = await page.evaluate(() => navigator.userAgent)

  return {
    headers: {
      cookie,
      'user-agent': userAgent,
      origin: 'https://gemini.google.com',
      referer: 'https://gemini.google.com/app',
    },
  }
}

function extractGeminiText(body) {
  for (const rawLine of body.split('\n')) {
    const line = rawLine.replace(/^\)\]\}'/, '').trim()
    if (!line.startsWith('[')) continue
    try {
      const envelope = JSON.parse(line)
      const payload = envelope?.[0]?.[2]
      if (typeof payload !== 'string') continue
      const decoded = JSON.parse(payload)
      const text =
        decoded?.[4]?.[0]?.[1]?.[0] ||
        decoded?.[4]?.[0]?.[0] ||
        decoded?.[0]?.[0] ||
        ''
      if (typeof text === 'string' && text.trim()) {
        return text.trim()
      }
    } catch {}
  }

  return ''
}

async function listGeminiModels() {
  await captureGeminiTemplate(false)
  return {
    data: [
      {
        id: 'gemini-web-session',
        provider: 'gemini',
      },
    ],
  }
}

async function chatGemini({ prompt, web_search = false }) {
  const page = state.gemini.page
  if (!page) throw new Error('Gemini Playwright not initialized')

  const template = await captureGeminiTemplate(true)
  const form = new URLSearchParams(template.payload)
  const rawFReq = form.get('f.req')
  if (!rawFReq) {
    throw new Error('Gemini request template missing f.req payload')
  }

  const outer = JSON.parse(rawFReq)
  const inner = JSON.parse(outer[1])
  if (Array.isArray(inner[0])) {
    inner[0][0] = prompt
  } else {
    inner[0] = [prompt, 0, null, null, null, null, 0]
  }
  outer[1] = JSON.stringify(inner)
  form.set('f.req', JSON.stringify(outer))

  const requestHeaders = { ...template.headers }
  const response = await page.evaluate(async ({ url, headers, body }) => {
    const result = await fetch(url, {
      method: 'POST',
      credentials: 'include',
      headers,
      body,
    })
    return {
      ok: result.ok,
      status: result.status,
      body: await result.text(),
    }
  }, { url: template.url, headers: requestHeaders, body: form.toString() })

  if (!response.ok) {
    throw new Error(`Gemini upstream request failed with status ${response.status}`)
  }

  const text = extractGeminiText(response.body)
  if (!text) {
    throw new Error('Gemini response was empty. Confirm session is active, then retry.')
  }

  return {
    text,
    model: 'gemini-web-session',
    warning: web_search
      ? 'Gemini web search toggle not mapped yet. Current web-session defaults were used.'
      : null,
  }
}

async function openGeminiLogin({ runtime_dir, browser }) {
  await initGemini({ runtime_dir, headless: false, browser })
  await state.gemini.page.goto('https://gemini.google.com/app', { waitUntil: 'domcontentloaded' })
}

async function initKimi({ runtime_dir, headless, browser }) {
  process.chdir(runtime_dir)
  if (state.kimi.context && state.kimi.headless === headless) return
  if (state.kimi.context) {
    await state.kimi.context.close()
    state.kimi.context = null
    state.kimi.page = null
    state.kimi.currentHeaders = {}
    state.kimi.cachedHeaders = null
    state.kimi.lastHeadersTime = 0
  }
  ensureDir(path.resolve('kimi_profile'))
  const { engine, channel } = resolveEngine(browser)
  state.kimi.context = await engine.launchPersistentContext(path.resolve('kimi_profile'), {
    headless,
    channel,
    userAgent: 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36',
    ignoreDefaultArgs: ['--enable-automation'],
    args: ['--disable-blink-features=AutomationControlled'],
  })
  await state.kimi.context.addInitScript(() => {
    Object.defineProperty(navigator, 'webdriver', { get: () => undefined })
  })
  state.kimi.page = await state.kimi.context.newPage()
  state.kimi.headless = headless
}

async function getKimiBasicHeaders() {
  const page = state.kimi.page
  if (!page) throw new Error('Kimi Playwright not initialized')
  const cookies = await page.context().cookies()
  const cookie = cookies.map((item) => `${item.name}=${item.value}`).join('; ')
  const userAgent = await page.evaluate(() => navigator.userAgent)
  return {
    headers: {
      cookie,
      authorization: state.kimi.currentHeaders.authorization || '',
      'user-agent': userAgent,
    },
  }
}

async function captureKimiHeaders(forceNew = false) {
  const page = state.kimi.page
  if (!page) throw new Error('Kimi Playwright not initialized')

  if (!forceNew && state.kimi.cachedHeaders && Date.now() - state.kimi.lastHeadersTime < 10 * 60 * 1000) {
    return state.kimi.cachedHeaders
  }

  const currentUrl = page.url()
  if (!currentUrl.includes('kimi.com') || forceNew) {
    await page.goto('https://www.kimi.com/', { waitUntil: 'domcontentloaded' })
  }

  const inputSelector = 'textarea:visible, [contenteditable="true"]:visible, div[contenteditable="true"]'
  await page.waitForSelector(inputSelector, { timeout: 30000 }).catch(() => {
    throw new Error('Timeout waiting for Kimi chat input. Are you logged in?')
  })

  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error('Timeout waiting for Kimi headers')), 60000)
    const routeHandler = async (route, request) => {
      clearTimeout(timeout)

      const reqHeaders = request.headers()
      let chatSessionId = ''
      let parentMessageId = null
      const postData = request.postData()

      if (postData) {
        try {
          const jsonStart = postData.indexOf('{')
          if (jsonStart !== -1) {
            const payload = JSON.parse(postData.slice(jsonStart))
            chatSessionId = payload.chat_id || ''
            parentMessageId = payload.message?.parent_id || null
          }
        } catch {}
      }

      const headers = {
        cookie: reqHeaders.cookie || '',
        authorization: reqHeaders.authorization || '',
        'connect-protocol-version': reqHeaders['connect-protocol-version'] || '1',
        'x-msh-device-id': reqHeaders['x-msh-device-id'] || '',
        'x-msh-platform': reqHeaders['x-msh-platform'] || 'web',
        'x-msh-session-id': reqHeaders['x-msh-session-id'] || '',
        'x-msh-version': reqHeaders['x-msh-version'] || '1.0.0',
        'x-traffic-id': reqHeaders['x-traffic-id'] || '',
        'r-timezone': reqHeaders['r-timezone'] || 'America/Sao_Paulo',
        'user-agent': reqHeaders['user-agent'] || '',
      }

      if (!headers.cookie || !headers.authorization) {
        await route.continue()
        return
      }

      state.kimi.currentHeaders = headers
      state.kimi.cachedHeaders = { headers, chat_session_id: chatSessionId, parent_message_id: parentMessageId }
      state.kimi.lastHeadersTime = Date.now()

      await route.abort('aborted')
      await page.unroute('**/apiv2/kimi.gateway.chat.v1.ChatService/Chat*', routeHandler)
      resolve(state.kimi.cachedHeaders)
    }

    page.route('**/apiv2/kimi.gateway.chat.v1.ChatService/Chat*', routeHandler).then(async () => {
      await page.focus(inputSelector)
      await page.fill(inputSelector, '')
      await page.type(inputSelector, 'a', { delay: 100 })
      await sleep(2000)
      let clicked = false
      for (const selector of [
        'button[type="submit"]',
        'button.send-button',
        '.chat-input-send-button',
        'svg.send-icon',
        'button:has(svg)',
      ]) {
        try {
          const button = await page.$(selector)
          if (button && await button.isVisible()) {
            await button.click({ force: true, delay: 50 }).catch(() => {})
            clicked = true
            break
          }
        } catch {}
      }
      if (!clicked) {
        await page.keyboard.press('Enter')
      }
    })
  })
}

async function openKimiLogin({ runtime_dir, browser }) {
  await initKimi({ runtime_dir, headless: false, browser })
  await state.kimi.page.goto('https://www.kimi.com/', { waitUntil: 'domcontentloaded' })
}

async function initQwen({ runtime_dir, headless, browser, account_id = null }) {
  process.chdir(runtime_dir)
  const slot = getQwenSlot(account_id)
  if (slot.context && slot.headless === headless) return
  if (slot.context) {
    await slot.context.close()
    resetQwenSlot(slot)
  }
  const profileId = account_id || '_default'
  ensureDir(path.resolve('qwen_profiles', profileId))
  const { engine, channel } = resolveEngine(browser)
  slot.context = await engine.launchPersistentContext(path.resolve('qwen_profiles', profileId), {
    headless,
    channel,
    userAgent: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36',
    ignoreDefaultArgs: ['--enable-automation'],
    args: ['--disable-blink-features=AutomationControlled'],
  })
  await slot.context.addInitScript(() => {
    Object.defineProperty(navigator, 'webdriver', { get: () => undefined })
  })
  slot.page = await slot.context.newPage()
  slot.headless = headless
  if (!account_id) {
    await attemptQwenAutoLogin(null)
  }
}

async function attemptQwenAutoLogin(accountId = null, email = process.env.QWEN_EMAIL, password = process.env.QWEN_PASSWORD) {
  const slot = getQwenSlot(accountId)
  if (!email || !password || !slot.page) return false
  const page = slot.page
  await page.goto('https://chat.qwen.ai/auth', { waitUntil: 'domcontentloaded' })
  const hashedPassword = createHash('sha256').update(password).digest('hex')

  const result = await page.evaluate(async ({ email, password }) => {
    try {
      const response = await fetch('https://chat.qwen.ai/api/v2/auths/signin', {
        method: 'POST',
        headers: {
          accept: 'application/json, text/plain, */*',
          'content-type': 'application/json',
          source: 'web',
          timezone: new Date().toString().split(' (')[0],
          'x-request-id': crypto.randomUUID(),
        },
        body: JSON.stringify({ email, password, login_type: 'email' }),
      })
      const data = await response.json().catch(() => null)
      return { ok: response.ok, data }
    } catch (error) {
      return { ok: false, error: String(error) }
    }
  }, { email, password: hashedPassword })

  if (result.ok) {
    await page.goto('https://chat.qwen.ai/', { waitUntil: 'domcontentloaded' })
    return true
  }
  return false
}

async function ensureQwenReady({ runtime_dir = process.cwd(), headless = true, browser = process.env.BROWSER || 'chromium', account_id = null } = {}) {
  const slot = getQwenSlot(account_id)
  if (!slot.context) {
    await initQwen({ runtime_dir, headless, browser, account_id })
  }
  return getQwenSlot(account_id)
}

async function getQwenCookie(accountId = null) {
  const slot = await ensureQwenReady({ account_id: accountId })
  const page = slot.page
  if (!page) throw new Error('Qwen Playwright not initialized')
  if (slot.cookieCache && Date.now() - slot.cookieCache.timestamp < 5 * 60 * 1000) {
    return slot.cookieCache.cookie
  }
  const cookies = await page.context().cookies()
  const cookie = cookies.map((item) => `${item.name}=${item.value}`).join('; ')
  slot.cookieCache = { cookie, timestamp: Date.now() }
  return cookie
}

async function getQwenBasicHeaders(params = {}) {
  const accountId = params.account_id || null
  const slot = await ensureQwenReady({ account_id: accountId })
  const page = slot.page
  if (!page) throw new Error('Qwen Playwright not initialized')
  if (!slot.userAgent) {
    slot.userAgent = await page.evaluate(() => navigator.userAgent)
  }
  return {
    headers: {
      cookie: await getQwenCookie(accountId),
      'user-agent': slot.userAgent,
      'bx-v': slot.currentHeaders['bx-v'] || '2.5.36',
      'bx-ua': slot.currentHeaders['bx-ua'] || '',
      'bx-umidtoken': slot.currentHeaders['bx-umidtoken'] || '',
    },
  }
}

async function captureQwenHeaders(forceNew = false, accountId = null) {
  const slot = await ensureQwenReady({ account_id: accountId })
  const page = slot.page
  if (!page) throw new Error('Qwen Playwright not initialized')
  if (!forceNew && slot.cachedHeaders && Date.now() - slot.lastHeadersTime < 60 * 60 * 1000) {
    return slot.cachedHeaders
  }

  const currentUrl = page.url()
  const isOnQwen = currentUrl.includes('chat.qwen.ai')
  const isOnSpecificChat = isOnQwen && /\/c\//.test(currentUrl)
  if (!isOnQwen || forceNew || isOnSpecificChat) {
    await page.goto('https://chat.qwen.ai/', { waitUntil: 'domcontentloaded' })
  }

  const isLoginPage = page.url().includes('login') || page.url().includes('auth')
  if (isLoginPage && !accountId) {
    await attemptQwenAutoLogin(accountId)
    await page.goto('https://chat.qwen.ai/', { waitUntil: 'domcontentloaded' })
  }

  const inputSelector = 'textarea:visible, [contenteditable="true"]:visible'
  await page.waitForSelector(inputSelector, { timeout: 30000 }).catch(() => {
    throw new Error('Timeout waiting for Qwen chat input. Are you logged in?')
  })

  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error('Timeout waiting for Qwen headers')), 60000)
    const routeHandler = async (route, request) => {
      clearTimeout(timeout)
      const reqHeaders = request.headers()
      let chatSessionId = ''
      let parentMessageId = null
      const postData = request.postData()
      if (postData) {
        try {
          const payload = JSON.parse(postData)
          chatSessionId = payload.chat_id || ''
          parentMessageId = payload.parent_id ?? null
        } catch {}
      }

      const headers = {
        cookie: reqHeaders.cookie || '',
        'user-agent': reqHeaders['user-agent'] || '',
        'bx-ua': reqHeaders['bx-ua'] || '',
        'bx-umidtoken': reqHeaders['bx-umidtoken'] || '',
        'bx-v': reqHeaders['bx-v'] || '',
      }

      if (!headers.cookie || !headers['bx-ua']) {
        await route.continue()
        return
      }

      slot.currentHeaders = headers
      slot.cachedHeaders = { headers, chat_session_id: chatSessionId, parent_message_id: parentMessageId }
      slot.lastHeadersTime = Date.now()
      slot.userAgent = headers['user-agent']

      await route.abort('aborted')
      await page.unroute('**/api/v2/chat/completions*', routeHandler)
      resolve(slot.cachedHeaders)
    }

    page.route('**/api/v2/chat/completions*', routeHandler).then(async () => {
      await page.focus(inputSelector)
      await page.fill(inputSelector, '')
      await page.type(inputSelector, 'a', { delay: 100 })
      await sleep(2000)
      let clicked = false
      for (const selector of [
        '.message-input-right-button-send .send-button',
        '.chat-prompt-send-button',
        'button.send-button',
      ]) {
        try {
          const button = await page.$(selector)
          if (button && await button.isVisible()) {
            await button.click({ force: true, delay: 50 }).catch(() => {})
            clicked = true
            break
          }
        } catch {}
      }
      if (!clicked) {
        await page.keyboard.press('Enter')
      }
    })
  })
}

async function openQwenLogin({ runtime_dir, browser, account_id = null }) {
  const slot = await ensureQwenReady({ runtime_dir, headless: false, browser, account_id })
  await slot.page.goto('https://chat.qwen.ai/auth', { waitUntil: 'domcontentloaded' })
}

async function loginQwenAccount({ account_id, email, password, headless = true, browser = 'chromium' }) {
  const slot = await ensureQwenReady({ runtime_dir: process.cwd(), headless, browser, account_id })
  const cookies = await slot.context.cookies()
  const hasAuthCookie = cookies.some((cookie) => cookie.name.toLowerCase().includes('token') || cookie.name.toLowerCase().includes('session'))
  if (!hasAuthCookie) {
    const ok = await attemptQwenAutoLogin(account_id, email, password)
    if (!ok) {
      throw new Error(`Qwen login failed for ${email}`)
    }
  }
  return { ok: true }
}

async function closeQwenAccount({ account_id = null }) {
  const slot = getQwenSlot(account_id)
  if (slot.context) {
    await slot.context.close()
  }
  resetQwenSlot(slot)
  if (account_id) {
    qwenAccounts.delete(account_id)
  }
  return { ok: true }
}

async function closeAll() {
  for (const key of ['deepseek', 'chatgpt', 'gemini', 'kimi', 'qwen']) {
    if (state[key].context) {
      await state[key].context.close()
      if (key === 'qwen') {
        resetQwenSlot(state.qwen)
      } else {
        state[key].context = null
        state[key].page = null
        state[key].headless = null
        state[key].cachedHeaders = null
        state[key].lastHeadersTime = 0
      }
    }
  }
  for (const [accountId, slot] of qwenAccounts.entries()) {
    if (slot.context) {
      await slot.context.close()
    }
    qwenAccounts.delete(accountId)
  }
}

async function handle(method, provider, params) {
  switch (`${provider}:${method}`) {
    case 'deepseek:init':
      return initDeepSeek(params)
    case 'deepseek:capture_headers':
      return captureDeepSeekHeaders(!!params.force_new)
    case 'deepseek:manual_login':
      return openDeepSeekLogin(params)
    case 'chatgpt:init':
      return initChatGPT(params)
    case 'chatgpt:capture_headers':
      return captureChatGPTTemplate(!!params.force_new)
    case 'chatgpt:basic_headers':
      return getChatGPTBasicHeaders()
    case 'chatgpt:manual_login':
      return openChatGPTLogin(params)
    case 'chatgpt:list_models':
      return listChatGPTModels()
    case 'chatgpt:chat':
      return chatChatGPT(params)
    case 'gemini:init':
      return initGemini(params)
    case 'gemini:capture_headers':
      return captureGeminiTemplate(!!params.force_new)
    case 'gemini:basic_headers':
      return getGeminiBasicHeaders()
    case 'gemini:manual_login':
      return openGeminiLogin(params)
    case 'gemini:list_models':
      return listGeminiModels()
    case 'gemini:chat':
      return chatGemini(params)
    case 'kimi:init':
      return initKimi(params)
    case 'kimi:capture_headers':
      return captureKimiHeaders(!!params.force_new)
    case 'kimi:basic_headers':
      return getKimiBasicHeaders()
    case 'kimi:manual_login':
      return openKimiLogin(params)
    case 'qwen:init':
      return initQwen(params)
    case 'qwen:capture_headers':
      return captureQwenHeaders(!!params.force_new, params.account_id || null)
    case 'qwen:basic_headers':
      return getQwenBasicHeaders(params)
    case 'qwen:manual_login':
      return openQwenLogin(params)
    case 'qwen:login_account':
      return loginQwenAccount(params)
    case 'qwen:close_account':
      return closeQwenAccount(params)
    case 'deepseek:shutdown':
    case 'chatgpt:shutdown':
    case 'gemini:shutdown':
    case 'kimi:shutdown':
    case 'qwen:shutdown':
      await closeAll()
      return { ok: true }
    default:
      throw new Error(`Unsupported helper call: ${provider}:${method}`)
  }
}

let buffer = ''
process.stdin.setEncoding('utf8')
process.stdin.on('data', async (chunk) => {
  buffer += chunk
  let newlineIndex = buffer.indexOf('\n')
  while (newlineIndex !== -1) {
    const line = buffer.slice(0, newlineIndex).trim()
    buffer = buffer.slice(newlineIndex + 1)
    if (line) {
      try {
        const request = JSON.parse(line)
        const result = await handle(request.method, request.provider, request.params || {})
        send(request.id, result, null)
      } catch (error) {
        send(JSON.parse(line).id, null, error instanceof Error ? error.message : String(error))
      }
    }
    newlineIndex = buffer.indexOf('\n')
  }
})
