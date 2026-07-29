import { createHash, randomUUID } from 'node:crypto'
import dns from 'node:dns'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

// Fix IPv6/IPv4 resolution issue in Node 17+ (localhost resolves to ::1 instead of 127.0.0.1)
// See: https://github.com/microsoft/playwright/issues/20784
dns.setDefaultResultOrder('ipv4first')

const __dirname = path.dirname(fileURLToPath(import.meta.url))
async function importPlaywright() {
  const candidateUrls = [
    new URL('./node_modules/playwright/index.mjs', import.meta.url),
    new URL('../node_modules/playwright/index.mjs', import.meta.url),
    new URL('../../node_modules/playwright/index.mjs', import.meta.url),
    new URL('../../../node_modules/playwright/index.mjs', import.meta.url),
  ]

  for (const candidate of candidateUrls) {
    if (fs.existsSync(fileURLToPath(candidate))) {
      return import(candidate)
    }
  }

  const pnpmRoots = [
    path.resolve(__dirname, 'node_modules', '.pnpm'),
    path.resolve(__dirname, '..', 'node_modules', '.pnpm'),
    path.resolve(__dirname, '..', '..', 'node_modules', '.pnpm'),
    path.resolve(__dirname, '..', '..', '..', 'node_modules', '.pnpm'),
  ]

  for (const root of pnpmRoots) {
    if (!fs.existsSync(root)) {
      continue
    }

    const playwrightDir = fs
      .readdirSync(root, { withFileTypes: true })
      .find((entry) => entry.isDirectory() && entry.name.startsWith('playwright@'))

    if (!playwrightDir) {
      continue
    }

    const candidate = path.join(root, playwrightDir.name, 'node_modules', 'playwright', 'index.mjs')
    if (fs.existsSync(candidate)) {
      return import(pathToFileURL(candidate).href)
    }
  }

  return import('playwright')
}

const playwright = await importPlaywright()
const { chromium, firefox, webkit } = playwright

function ensureDir(dir) {
  fs.mkdirSync(dir, { recursive: true })
}

// Defense-in-depth: account_id is joined to a filesystem profile path.
// Reject anything outside [A-Za-z0-9_-]{1,64} before path.resolve sees it.
const SAFE_ACCOUNT_ID = /^[A-Za-z0-9_-]{1,64}$/
function assertSafeAccountId(accountId) {
  if (accountId != null && accountId !== '' && !SAFE_ACCOUNT_ID.test(accountId)) {
    throw new Error(`unsafe account_id rejected: ${accountId}`)
  }
}

// Known install locations per Chromium-family browser, across platforms. Used
// to fall back to an installed browser when the requested channel's own
// distribution is missing (e.g. 'msedge' requested on a Linux box that only has
// Chromium) instead of hard-failing the launch.
const BROWSER_PATHS = {
  msedge: [
    '/opt/microsoft/msedge/msedge',
    '/usr/bin/microsoft-edge',
    '/usr/bin/microsoft-edge-stable',
    '/usr/bin/microsoft-edge-beta',
    '/usr/bin/microsoft-edge-dev',
    'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe',
    'C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe',
    '/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge',
  ],
  chrome: [
    '/opt/google/chrome/chrome',
    '/usr/bin/google-chrome',
    '/usr/bin/google-chrome-stable',
    'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe',
    'C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe',
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  ],
  chromium: [
    '/usr/bin/chromium',
    '/usr/bin/chromium-browser',
    '/snap/bin/chromium',
    '/Applications/Chromium.app/Contents/MacOS/Chromium',
  ],
}

function firstExisting(paths) {
  return paths.find((candidate) => fs.existsSync(candidate))
}

// Resolve a Chromium launch config. If the requested channel's real
// distribution is installed, drive it via `channel` (Playwright applies the
// right profile flags). Otherwise point `executablePath` at whatever
// Chromium-family browser IS installed — preferring the requested family, then
// Edge → Chrome → Chromium. Last resort is Playwright's bundled chromium.
function resolveChromium(preferredChannel) {
  if (preferredChannel && firstExisting(BROWSER_PATHS[preferredChannel] ?? [])) {
    return { engine: chromium, channel: preferredChannel }
  }
  const order = preferredChannel
    ? [preferredChannel, 'msedge', 'chrome', 'chromium']
    : ['chromium', 'chrome', 'msedge']
  for (const key of order) {
    const executablePath = firstExisting(BROWSER_PATHS[key] ?? [])
    if (executablePath) {
      return { engine: chromium, executablePath }
    }
  }
  return { engine: chromium }
}

function resolveEngine(browser) {
  switch (browser) {
    case 'firefox':
      return { engine: firefox }
    case 'webkit':
      return { engine: webkit }
    case 'chrome':
      return resolveChromium('chrome')
    case 'edge':
    case 'msedge':
      return resolveChromium('msedge')
    case 'chromium':
    default:
      return resolveChromium(null)
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
  mistral: {
    context: null,
    page: null,
    headless: null,
    cachedHeaders: null,
    lastHeadersTime: 0,
  },
  zai: {
    context: null,
    page: null,
    headless: null,
    cachedHeaders: null,
    lastHeadersTime: 0,
  },
  meta: {
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

function modelPattern(provider) {
  switch (provider) {
    case 'chatgpt':
      return /^(?:gpt|o[0-9]|chatgpt)[a-z0-9_.:-]*$/i
    case 'gemini':
      return /^(?:gemini|learnlm)[a-z0-9_.:-]*$/i
    case 'mistral':
      return /^(?:mistral|magistral|codestral|vibe|le-chat)[a-z0-9_.:-]*$/i
    case 'zai':
      return /^(?:glm|autoglm|zai)[a-z0-9_.:-]*$/i
    case 'meta':
      return /^(?:meta(?:-ai)?|llama)[a-z0-9_.:-]*$/i
    default:
      return /^[a-z0-9][a-z0-9_.:-]{1,80}$/i
  }
}

function addModelCandidate(target, provider, value) {
  if (typeof value !== 'string') return
  const clean = value.trim().replace(/^model:/i, '').replace(/^models\//i, '')
  if (!clean || clean.length > 96 || /\s/.test(clean)) return
  if (modelPattern(provider).test(clean)) target.add(clean)
}

function collectModelIds(value, provider, target, depth = 0) {
  if (depth > 8 || value == null) return

  if (typeof value === 'string') {
    addModelCandidate(target, provider, value)
    const trimmed = value.trim()
    if ((trimmed.startsWith('{') || trimmed.startsWith('[')) && trimmed.length < 500000) {
      try {
        collectModelIds(JSON.parse(trimmed), provider, target, depth + 1)
      } catch {}
    }

    const modelKeyRe = /["'](?:model|model_slug|slug|id|name)["']\s*:\s*["']([a-zA-Z0-9][\w.:-]{1,95})["']/g
    for (const match of trimmed.matchAll(modelKeyRe)) addModelCandidate(target, provider, match[1])

    const directPatterns = {
      chatgpt: /\b(?:gpt|o[0-9]|chatgpt)[a-zA-Z0-9_.:-]{1,80}\b/g,
      gemini: /\b(?:gemini|learnlm)[a-zA-Z0-9_.:-]{1,80}\b/g,
      mistral: /\b(?:mistral|magistral|codestral|vibe|le-chat)[a-zA-Z0-9_.:-]{1,80}\b/g,
      zai: /\b(?:glm|autoglm|zai)[a-zA-Z0-9_.:-]{1,80}\b/g,
      meta: /\b(?:meta(?:-ai)?|llama)[a-zA-Z0-9_.:-]{1,80}\b/g,
    }
    for (const match of trimmed.matchAll(directPatterns[provider] || /\b[a-z][a-z0-9_.:-]{1,80}\b/g)) {
      addModelCandidate(target, provider, match[0])
    }
    return
  }

  if (Array.isArray(value)) {
    for (const item of value) collectModelIds(item, provider, target, depth + 1)
    return
  }

  if (typeof value === 'object') {
    for (const [key, child] of Object.entries(value)) {
      if (/^(?:model|model_slug|slug|id|name)$/i.test(key)) {
        addModelCandidate(target, provider, child)
      }
      collectModelIds(child, provider, target, depth + 1)
    }
  }
}

function modelListResponse(ids, provider, fallbackModel) {
  const data = [...ids].length ? [...ids] : [fallbackModel]
  return {
    data: data.map(id => ({ id, provider })),
  }
}

function addKnownChatGPTModels(target) {
  for (const id of [
    'gpt-5-3',
    'gpt-5.5',
    'gpt-5.5-thinking',
    'gpt-5',
    'gpt-4.1',
    'o3',
    'o4-mini',
    'chatgpt-web-session',
  ]) {
    addModelCandidate(target, 'chatgpt', id)
  }
}

function addKnownZaiModels(target) {
  for (const id of [
    'glm-5.2',
    'glm-5.1',
    'glm-5',
    'glm-5-turbo',
    'glm-4.7',
    'glm-4.6',
    'glm-4.5',
    'glm-4.6v',
    'glm-4.5v',
    'autoglm',
  ]) {
    addModelCandidate(target, 'zai', id)
  }
}

function addKnownMetaModels(target) {
  for (const id of ['meta-ai-web-session']) {
    addModelCandidate(target, 'meta', id)
  }
}

async function scanPageModelHints(page, provider, endpointPaths = []) {
  const bodies = await page.evaluate(async ({ endpointPaths }) => {
    const out = []
    const add = value => {
      if (typeof value === 'string' && value.trim()) out.push(value.slice(0, 500000))
    }

    for (const endpoint of endpointPaths) {
      try {
        const response = await fetch(endpoint, { credentials: 'include' })
        if (response.ok) add(await response.text())
      } catch {}
    }

    try {
      add(JSON.stringify(window.__NEXT_DATA__ || window.__NUXT__ || {}))
    } catch {}

    for (const script of Array.from(document.scripts).slice(0, 80)) {
      const text = script.textContent || ''
      if (/model|gemini|gpt|mistral|codestral|magistral|glm|autoglm|zai|meta|llama/i.test(text)) add(text)
    }

    for (const storage of [window.localStorage, window.sessionStorage]) {
      try {
        for (let index = 0; index < storage.length; index += 1) {
          const key = storage.key(index) || ''
          const value = storage.getItem(key) || ''
          if (/model|gemini|gpt|mistral|codestral|magistral|glm|autoglm|zai|meta|llama/i.test(`${key} ${value}`)) {
            add(`${key} ${value}`)
          }
        }
      } catch {}
    }

    for (const resource of performance.getEntriesByType('resource').map(entry => entry.name)) {
      if (/batchexecute|model|init|template|status/i.test(resource)) add(resource)
    }

    return out
  }, { endpointPaths })

  const ids = new Set()
  for (const body of bodies) collectModelIds(body, provider, ids)
  return ids
}

async function waitForInteractiveSelector(page, selectors, timeout = 30000) {
  for (const selector of selectors) {
    try {
      await page.waitForSelector(selector, { timeout })
      return selector
    } catch {}
  }

  throw new Error(`Timeout waiting for interactive selector: ${selectors.join(', ')}`)
}

async function scanPageModelHintsWithRetries(page, provider, endpointPaths = [], attempts = 3) {
  let ids = new Set()
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    ids = await scanPageModelHints(page, provider, endpointPaths)
    if (ids.size > 0) {
      return ids
    }
    await sleep(1200)
  }
  return ids
}

async function closeContext(context) {
  if (!context) return
  const browser = typeof context.browser === 'function' ? context.browser() : null
  await context.close().catch(() => {})
  if (browser) {
    await browser.close().catch(() => {})
  }
}

async function initDeepSeek({ runtime_dir, headless, browser }) {
  process.chdir(runtime_dir)
  if (state.deepseek.context && state.deepseek.headless === headless) {
    try {
      if (!state.deepseek.page || state.deepseek.page.isClosed()) {
        state.deepseek.page =
          state.deepseek.context.pages().find((page) => !page.isClosed()) ||
          (await state.deepseek.context.newPage())
      }
      return
    } catch {
      await closeContext(state.deepseek.context).catch(() => {})
      state.deepseek.context = null
      state.deepseek.page = null
    }
  }
  if (state.deepseek.context) {
    await closeContext(state.deepseek.context)
    state.deepseek.context = null
    state.deepseek.page = null
  }
  ensureDir(path.resolve('deepseek_profile'))
  const { engine, channel, executablePath } = resolveEngine(browser)
  state.deepseek.context = await engine.launchPersistentContext(path.resolve('deepseek_profile'), {
    headless,
    channel,
    executablePath,
    userAgent: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36',
    args: [
      '--disable-blink-features=AutomationControlled',
      '--exclude-switches=enable-automation',
      '--disable-infobars',
      '--no-sandbox',
      '--disable-dev-shm-usage',
      '--disable-gpu',
      // Fix Chrome 136+ DevTools debugging restrictions
      // See: https://github.com/microsoft/playwright/issues/35836
      '--disable-features=DevToolsDebuggingRestrictions',
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
    try {
      await page.goto('https://chat.deepseek.com/', { waitUntil: 'domcontentloaded' })
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      if (!message.includes('net::ERR_ABORTED')) throw error
    }
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
      let requestPayload = null

      const postData = request.postData()
      if (postData) {
        try {
          const payload = JSON.parse(postData)
          requestPayload = payload && typeof payload === 'object' ? payload : null
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
      resolve({
        headers,
        chat_session_id: chatSessionId,
        parent_message_id: parentMessageId,
        request_payload: requestPayload,
      })
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
    await closeContext(state.chatgpt.context)
    state.chatgpt.context = null
    state.chatgpt.page = null
    state.chatgpt.cachedHeaders = null
    state.chatgpt.lastHeadersTime = 0
  }
  ensureDir(path.resolve('chatgpt_profile'))
  const { engine, channel, executablePath } = resolveEngine(browser)
  state.chatgpt.context = await engine.launchPersistentContext(path.resolve('chatgpt_profile'), {
    headless,
    channel,
    executablePath,
    userAgent:
      'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/147.0.0.0 Safari/537.36',
    ignoreDefaultArgs: ['--enable-automation'],
    args: [
      '--disable-blink-features=AutomationControlled',
      // Fix Chrome 136+ DevTools debugging restrictions
      '--disable-features=DevToolsDebuggingRestrictions',
    ],
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

function buildChatGPTMessages(prompt, webSearch, systemPrompt) {
  const messages = []
  if (systemPrompt && systemPrompt.trim()) {
    messages.push({
      id: randomUUID(),
      author: { role: 'system' },
      create_time: Date.now() / 1000,
      content: { content_type: 'text', parts: [systemPrompt.trim()] },
      metadata: {},
    })
  }
  messages.push({
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
      serialization_metadata: { custom_symbol_offsets: [] },
    },
  })
  return messages
}

function buildChatGPTPayload(prompt, model, webSearch, systemPrompt) {
  return {
    action: 'next',
    messages: buildChatGPTMessages(prompt, webSearch, systemPrompt),
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

function cloneJson(value) {
  return value == null ? value : JSON.parse(JSON.stringify(value))
}

function replaceChatGPTMessageContent(content, prompt) {
  if (!content || typeof content !== 'object') {
    return {
      content_type: 'text',
      parts: [prompt],
    }
  }

  if (Array.isArray(content.parts)) {
    return {
      ...content,
      parts: [prompt],
    }
  }

  return {
    ...content,
    text: prompt,
  }
}

function compactChatGPTPrompt(prompt, maxChars = 18000) {
  if (typeof prompt !== 'string') return { text: '', truncated: false }
  const clean = prompt.trim()
  if (clean.length <= maxChars) return { text: clean, truncated: false }

  const marker = '\n\n[Earlier conversation trimmed to fit ChatGPT limit]\n\n'
  const headBudget = Math.min(6000, Math.floor((maxChars - marker.length) * 0.4))
  const tailBudget = Math.max(2000, maxChars - marker.length - headBudget)
  return {
    text: `${clean.slice(0, headBudget)}${marker}${clean.slice(-tailBudget)}`,
    truncated: true,
  }
}

function buildChatGPTPayloadFromTemplate(template, prompt, model, webSearch, systemPrompt) {
  let payload = null
  try {
    payload = template?.payload ? JSON.parse(template.payload) : null
  } catch {}

  if (!payload || typeof payload !== 'object') {
    return buildChatGPTPayload(prompt, model, webSearch, systemPrompt)
  }

  const nextPayload = cloneJson(payload)
  const messages = Array.isArray(nextPayload.messages) ? nextPayload.messages : []
  const templateMessage = messages.find((message) => message?.author?.role === 'user') || messages[0] || {}
  const templateMetadata =
    templateMessage?.metadata && typeof templateMessage.metadata === 'object'
      ? templateMessage.metadata
      : {}

  nextPayload.model = model
  delete nextPayload.conversation_id
  delete nextPayload.conversationId
  delete nextPayload.current_node
  delete nextPayload.currentNode
  delete nextPayload.parent_id
  delete nextPayload.parentId
  delete nextPayload.response_id
  delete nextPayload.responseId
  delete nextPayload.suggestions
  delete nextPayload.history_and_training_disabled

  const builtMessages = []
  if (systemPrompt && systemPrompt.trim()) {
    builtMessages.push({
      id: randomUUID(),
      author: { role: 'system' },
      create_time: Date.now() / 1000,
      content: { content_type: 'text', parts: [systemPrompt.trim()] },
      metadata: {},
    })
  }
  builtMessages.push({
    ...templateMessage,
    id: randomUUID(),
    create_time: Date.now() / 1000,
    author: { ...(templateMessage.author || {}), role: 'user' },
    content: replaceChatGPTMessageContent(templateMessage.content, prompt),
    metadata: {
      ...templateMetadata,
      selected_sources: webSearch ? ['web'] : [],
    },
  })
  nextPayload.messages = builtMessages

  nextPayload.parent_message_id = 'client-created-root'
  if (!nextPayload.action || typeof nextPayload.action !== 'string') {
    nextPayload.action = 'next'
  }

  return nextPayload
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
  const page = state.chatgpt.page
  if (!page) throw new Error('ChatGPT Playwright not initialized')
  if (!page.url().includes('chatgpt.com')) {
    await page.goto('https://chatgpt.com/', { waitUntil: 'domcontentloaded' })
  }

  await waitForInteractiveSelector(page, [
    'textarea:visible',
    '#prompt-textarea:visible',
    'div[contenteditable="true"]:visible',
  ])

  const ids = await scanPageModelHintsWithRetries(page, 'chatgpt', [
    '/backend-api/models',
    '/backend-api/f/models',
    '/backend-api/model_slug_availability',
  ])
  addKnownChatGPTModels(ids)
  if (state.chatgpt.cachedHeaders?.model) addModelCandidate(ids, 'chatgpt', state.chatgpt.cachedHeaders.model)
  return modelListResponse(ids, 'chatgpt', 'chatgpt-web-session')
}

async function chatChatGPT({ model, prompt, system_prompt, web_search = false }) {
  const page = state.chatgpt.page
  if (!page) throw new Error('ChatGPT Playwright not initialized')

  const template = await captureChatGPTTemplate(true)
  const requestHeaders = { ...template.headers }
  delete requestHeaders.cookie

  const sendConversation = async (preparedPrompt) => {
    const payload = buildChatGPTPayloadFromTemplate(
      template,
      preparedPrompt.text,
      ensureSessionText(model, template.model || 'chatgpt-web-session'),
      web_search,
      system_prompt || null,
    )
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
        body: raw,
      }
    }, { headers: requestHeaders, payload })
    return { payload, requestResult, preparedPrompt }
  }

  let sent = await sendConversation(compactChatGPTPrompt(prompt, 18000))
  if (!sent.requestResult.ok && sent.requestResult.status === 413) {
    sent = await sendConversation(compactChatGPTPrompt(prompt, 9000))
  }

  const conversationId = sent.requestResult.conversationId || sent.payload.conversation_id || ''
  if (!sent.requestResult.ok || !conversationId) {
    const detail = sent.requestResult.body?.trim()
    throw new Error(
      detail
        ? `ChatGPT upstream request failed with status ${sent.requestResult.status}: ${detail.slice(0, 400)}`
        : `ChatGPT upstream request failed with status ${sent.requestResult.status}`,
    )
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
  }, { headers: requestHeaders, conversationId })

  const text = extractChatGPTAssistantText(conversationJson ? JSON.parse(conversationJson) : null)
  if (!text) {
    throw new Error('ChatGPT response was empty. Confirm session is active, then retry.')
  }

  return {
    text,
    model: sent.payload.model,
    conversation_id: conversationId,
    warning: [
      web_search
        ? 'ChatGPT web search toggle not mapped yet. Current web-session defaults were used.'
        : null,
      sent.preparedPrompt.truncated
        ? 'Prompt was compacted before ChatGPT send to avoid message_length_exceeds_limit.'
        : null,
    ]
      .filter(Boolean)
      .join(' | ') || null,
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
    await closeContext(state.gemini.context)
    state.gemini.context = null
    state.gemini.page = null
    state.gemini.cachedHeaders = null
    state.gemini.lastHeadersTime = 0
  }
  ensureDir(path.resolve('gemini_profile'))
  const { engine, channel, executablePath } = resolveEngine(browser)
  state.gemini.context = await engine.launchPersistentContext(path.resolve('gemini_profile'), {
    headless,
    channel,
    executablePath,
    userAgent:
      'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/147.0.0.0 Safari/537.36',
    ignoreDefaultArgs: ['--enable-automation'],
    args: [
      '--disable-blink-features=AutomationControlled',
      // Fix Chrome 136+ DevTools debugging restrictions
      '--disable-features=DevToolsDebuggingRestrictions',
    ],
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
  const payloads = []
  for (const rawLine of body.split('\n')) {
    const line = rawLine.replace(/^\)\]\}'/, '').trim()
    if (!line.startsWith('[')) continue
    try {
      const envelope = JSON.parse(line)
      const payload = envelope?.[0]?.[2]
      if (typeof payload !== 'string') continue
      const decoded = JSON.parse(payload)
      payloads.push(decoded)
    } catch {}
  }

  for (let index = payloads.length - 1; index >= 0; index -= 1) {
    const text = extractGeminiTextFromPayload(payloads[index])
    if (text) return text
  }

  return ''
}

function extractGeminiTextFromPayload(decoded) {
  const candidates = [
    decoded?.[4]?.[0]?.[1]?.[0],
    decoded?.[4]?.[0]?.[0],
    decoded?.[4]?.[0]?.[1],
    decoded?.[0]?.[0],
    extractFirstGeminiText(decoded?.[4]),
  ]

  for (const candidate of candidates) {
    const text = normalizeGeminiText(candidate)
    if (text) return text
  }

  return ''
}

function extractFirstGeminiText(value) {
  if (typeof value === 'string') return value
  if (Array.isArray(value)) {
    for (const item of value) {
      const text = extractFirstGeminiText(item)
      if (text) return text
    }
    return ''
  }
  if (value && typeof value === 'object') {
    for (const key of ['text', 'content', 'value']) {
      const text = extractFirstGeminiText(value[key])
      if (text) return text
    }
    for (const child of Object.values(value)) {
      const text = extractFirstGeminiText(child)
      if (text) return text
    }
  }
  return ''
}

function normalizeGeminiText(value) {
  if (typeof value === 'string') {
    return value.trim()
  }
  if (Array.isArray(value)) {
    const text = value
      .map(item => normalizeGeminiText(item))
      .filter(Boolean)
      .join('\n')
      .trim()
    return text
  }
  return ''
}

async function listGeminiModels() {
  const page = state.gemini.page
  if (!page) throw new Error('Gemini Playwright not initialized')
  if (!page.url().includes('gemini.google.com')) {
    await page.goto('https://gemini.google.com/app', { waitUntil: 'domcontentloaded' })
  }

  await waitForInteractiveSelector(page, [
    'rich-textarea textarea',
    'textarea:visible',
    'div[contenteditable="true"]:visible',
  ])

  const resourceUrls = await page.evaluate(() =>
    performance
      .getEntriesByType('resource')
      .map(entry => entry.name)
      .filter(name => name.includes('batchexecute'))
      .slice(0, 8)
  )
  const ids = await scanPageModelHintsWithRetries(page, 'gemini', resourceUrls)
  return modelListResponse(ids, 'gemini', 'gemini-web-session')
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

async function initMistral({ runtime_dir, headless, browser }) {
  process.chdir(runtime_dir)
  if (state.mistral.context && state.mistral.headless === headless) return
  if (state.mistral.context) {
    await closeContext(state.mistral.context)
    state.mistral.context = null
    state.mistral.page = null
    state.mistral.cachedHeaders = null
    state.mistral.lastHeadersTime = 0
  }
  ensureDir(path.resolve('mistral_profile'))
  const { engine, channel, executablePath } = resolveEngine(browser)
  state.mistral.context = await engine.launchPersistentContext(path.resolve('mistral_profile'), {
    headless,
    channel,
    executablePath,
    userAgent:
      'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/147.0.0.0 Safari/537.36',
    ignoreDefaultArgs: ['--enable-automation'],
    args: [
      '--disable-blink-features=AutomationControlled',
      '--disable-features=DevToolsDebuggingRestrictions',
    ],
  })
  await state.mistral.context.addInitScript(() => {
    Object.defineProperty(navigator, 'webdriver', { get: () => undefined })
  })
  state.mistral.page = await state.mistral.context.newPage()
  state.mistral.headless = headless
}

async function captureMistralTemplate(forceNew = false) {
  const page = state.mistral.page
  if (!page) throw new Error('Mistral Playwright not initialized')

  if (!forceNew && state.mistral.cachedHeaders && Date.now() - state.mistral.lastHeadersTime < 5 * 60 * 1000) {
    return state.mistral.cachedHeaders
  }

  if (!page.url().includes('chat.mistral.ai') || forceNew) {
    await page.goto('https://chat.mistral.ai/chat', { waitUntil: 'domcontentloaded' })
  }

  const inputSelector = 'textarea:visible, div[contenteditable="true"]:visible, [data-testid*="composer"] textarea'
  await page.waitForSelector(inputSelector, { timeout: 30000 }).catch(() => {
    throw new Error('Timeout waiting for Mistral chat input. Are you logged in?')
  })

  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error('Timeout waiting for Mistral request template')), 60000)
    const routeHandler = async (route, request) => {
      const url = request.url()
      const method = request.method()
      if (method !== 'POST' || !url.includes('chat.mistral.ai')) {
        await route.continue()
        return
      }

      const postData = request.postData() || ''
      if (!postData || !/(prompt|message|content|text|query|input|conversation)/i.test(postData + url)) {
        await route.continue()
        return
      }

      clearTimeout(timeout)
      const reqHeaders = request.headers()
      const headers = {
        accept: reqHeaders.accept || '*/*',
        'accept-language': reqHeaders['accept-language'] || 'en-US,en;q=0.9',
        'content-type': reqHeaders['content-type'] || 'application/json',
        origin: reqHeaders.origin || 'https://chat.mistral.ai',
        referer: reqHeaders.referer || 'https://chat.mistral.ai/chat',
        'user-agent': reqHeaders['user-agent'] || '',
        authorization: reqHeaders.authorization || '',
      }

      state.mistral.cachedHeaders = {
        headers,
        payload: postData,
        url,
      }
      state.mistral.lastHeadersTime = Date.now()

      await route.abort('aborted')
      await page.unroute('**/*', routeHandler)
      resolve(state.mistral.cachedHeaders)
    }

    page.route('**/*', routeHandler).then(async () => {
      await page.focus(inputSelector)
      await page.fill(inputSelector, '')
      await page.type(inputSelector, 'a', { delay: 50 })
      await sleep(1500)
      await page.keyboard.press('Enter')
    })
  })
}

function replacePromptAndModel(value, prompt, model) {
  let changedPrompt = false
  let changedModel = false
  const promptKeys = new Set(['prompt', 'message', 'content', 'text', 'query', 'input'])

  function visit(node, key = '') {
    if (Array.isArray(node)) {
      return node.map(item => visit(item))
    }
    if (!node || typeof node !== 'object') {
      if (!changedPrompt && typeof node === 'string' && node.trim() === 'a') {
        changedPrompt = true
        return prompt
      }
      return node
    }

    const out = {}
    for (const [childKey, childValue] of Object.entries(node)) {
      if (!changedModel && model && /^model$/i.test(childKey) && typeof childValue === 'string') {
        changedModel = true
        out[childKey] = model
        continue
      }
      if (!changedPrompt && promptKeys.has(childKey.toLowerCase()) && typeof childValue === 'string') {
        changedPrompt = true
        out[childKey] = prompt
        continue
      }
      out[childKey] = visit(childValue, childKey)
    }
    return out
  }

  return { value: visit(value), changedPrompt, changedModel }
}

function buildTemplateBody(template, prompt, model) {
  const contentType = template.headers?.['content-type'] || ''
  if (contentType.includes('application/x-www-form-urlencoded')) {
    const form = new URLSearchParams(template.payload)
    let changed = false
    let changedModel = false
    for (const [key, value] of [...form.entries()]) {
      if (!changed && /prompt|message|content|text|query|input/i.test(key)) {
        form.set(key, prompt)
        changed = true
      } else if (model && /^model$/i.test(key)) {
        form.set(key, model)
        changedModel = true
      } else if (!changed && (value.trim().startsWith('{') || value.trim().startsWith('['))) {
        try {
          const replaced = replacePromptAndModel(JSON.parse(value), prompt, model)
          if (replaced.changedPrompt) {
            form.set(key, JSON.stringify(replaced.value))
            changed = true
            changedModel = changedModel || replaced.changedModel
          }
        } catch {}
      }
    }
    if (!changed) throw new Error('Mistral request template did not expose a usable prompt field')
    return { body: form.toString(), changedModel }
  }

  let parsed
  try {
    parsed = JSON.parse(template.payload)
  } catch {
    throw new Error('Mistral request template payload is not JSON or form data')
  }
  const replaced = replacePromptAndModel(parsed, prompt, model)
  if (!replaced.changedPrompt) throw new Error('Mistral request template did not expose a usable prompt field')
  return { body: JSON.stringify(replaced.value), changedModel: replaced.changedModel }
}

function collectResponseText(value, out = [], depth = 0, key = '') {
  if (depth > 10 || value == null) return out
  if (typeof value === 'string') {
    if (/^(content|text|answer|message|delta)$/i.test(key) && value.trim()) out.push(value.trim())
    return out
  }
  if (Array.isArray(value)) {
    for (const item of value) collectResponseText(item, out, depth + 1, key)
    return out
  }
  if (typeof value === 'object') {
    for (const [childKey, childValue] of Object.entries(value)) {
      collectResponseText(childValue, out, depth + 1, childKey)
    }
  }
  return out
}

function extractMistralText(body) {
  const texts = []
  for (const rawLine of body.split('\n')) {
    const line = rawLine.trim()
    if (!line) continue
    const payload = line.startsWith('data:') ? line.slice(5).trim() : line
    if (!payload || payload === '[DONE]') continue
    try {
      collectResponseText(JSON.parse(payload), texts)
    } catch {}
  }

  if (!texts.length) {
    try {
      collectResponseText(JSON.parse(body), texts)
    } catch {}
  }

  return texts.filter(Boolean).at(-1) || ''
}

async function listMistralModels() {
  const page = state.mistral.page
  if (!page) throw new Error('Mistral Playwright not initialized')
  if (!page.url().includes('chat.mistral.ai')) {
    await page.goto('https://chat.mistral.ai/chat', { waitUntil: 'domcontentloaded' })
  }

  const ids = await scanPageModelHints(page, 'mistral', ['/api/models', '/api/model'])
  return modelListResponse(ids, 'mistral', 'mistral-web-session')
}

async function chatMistral({ model, prompt, web_search = false }) {
  const page = state.mistral.page
  if (!page) throw new Error('Mistral Playwright not initialized')

  const template = await captureMistralTemplate(true)
  const headers = { ...template.headers }
  const prepared = buildTemplateBody(template, prompt, model)

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
  }, { url: template.url, headers, body: prepared.body })

  if (!response.ok) {
    throw new Error(`Mistral upstream request failed with status ${response.status}`)
  }

  const text = extractMistralText(response.body)
  if (!text) {
    throw new Error('Mistral response was empty. Confirm session is active and the captured request template is still valid.')
  }

  return {
    text,
    model: ensureSessionText(model, 'mistral-web-session'),
    warning: web_search
      ? 'Mistral web search toggle is not mapped yet. Current web-session defaults were used.'
      : null,
  }
}

async function openMistralLogin({ runtime_dir, browser }) {
  await initMistral({ runtime_dir, headless: false, browser })
  await state.mistral.page.goto('https://chat.mistral.ai/chat', { waitUntil: 'domcontentloaded' })
}

async function initZai({ runtime_dir, headless, browser }) {
  process.chdir(runtime_dir)
  if (state.zai.context && state.zai.headless === headless) return
  if (state.zai.context) {
    await closeContext(state.zai.context)
    state.zai.context = null
    state.zai.page = null
    state.zai.cachedHeaders = null
    state.zai.lastHeadersTime = 0
  }
  ensureDir(path.resolve('zai_profile'))
  const { engine, channel, executablePath } = resolveEngine(browser)
  state.zai.context = await engine.launchPersistentContext(path.resolve('zai_profile'), {
    headless,
    channel,
    executablePath,
    userAgent:
      'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/147.0.0.0 Safari/537.36',
    ignoreDefaultArgs: ['--enable-automation'],
    args: [
      '--disable-blink-features=AutomationControlled',
      '--disable-features=DevToolsDebuggingRestrictions',
    ],
  })
  await state.zai.context.addInitScript(() => {
    Object.defineProperty(navigator, 'webdriver', { get: () => undefined })
  })
  state.zai.page = await state.zai.context.newPage()
  state.zai.headless = headless
}

async function captureZaiTemplate(forceNew = false) {
  const page = state.zai.page
  if (!page) throw new Error('Z.AI Playwright not initialized')

  if (!forceNew && state.zai.cachedHeaders && Date.now() - state.zai.lastHeadersTime < 5 * 60 * 1000) {
    return state.zai.cachedHeaders
  }

  if (!page.url().includes('chat.z.ai') || forceNew) {
    await page.goto('https://chat.z.ai/', { waitUntil: 'domcontentloaded' })
  }

  const inputSelector =
    'textarea:visible, div[contenteditable="true"]:visible, [data-testid*="composer"] textarea, [role="textbox"]:visible'
  await page.waitForSelector(inputSelector, { timeout: 30000 }).catch(() => {
    throw new Error('Timeout waiting for Z.AI chat input. Are you logged in?')
  })

  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error('Timeout waiting for Z.AI request template')), 60000)
    const routeHandler = async (route, request) => {
      const url = request.url()
      const method = request.method()
      if (method !== 'POST' || !/(?:chat|api)\.z\.ai/i.test(url)) {
        await route.continue()
        return
      }

      const postData = request.postData() || ''
      const signature = `${url}\n${postData}`
      if (!/(prompt|message|content|text|query|input|conversation|chat\/completions|responses|glm)/i.test(signature)) {
        await route.continue()
        return
      }

      clearTimeout(timeout)
      const reqHeaders = request.headers()
      const headers = {
        accept: reqHeaders.accept || 'text/event-stream',
        'accept-language': reqHeaders['accept-language'] || 'en-US,en;q=0.9',
        'content-type': reqHeaders['content-type'] || 'application/json',
        origin: reqHeaders.origin || 'https://chat.z.ai',
        referer: reqHeaders.referer || 'https://chat.z.ai/',
        'user-agent': reqHeaders['user-agent'] || '',
        authorization: reqHeaders.authorization || '',
      }

      state.zai.cachedHeaders = {
        headers,
        payload: postData,
        url,
      }
      state.zai.lastHeadersTime = Date.now()

      await route.abort('aborted')
      await page.unroute('**/*', routeHandler)
      resolve(state.zai.cachedHeaders)
    }

    page.route('**/*', routeHandler).then(async () => {
      await page.focus(inputSelector)
      await page.fill(inputSelector, '')
      await page.type(inputSelector, 'a', { delay: 50 })
      await sleep(1500)
      let clicked = false
      for (const selector of [
        'button[type="submit"]',
        'button:has(svg)',
        '[data-testid*="send"]',
        '[aria-label*="Send"]',
        '[aria-label*="send"]',
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

function extractOpenAIStyleResponse(body) {
  let text = ''
  let reasoning = ''
  let model = ''

  for (const rawLine of body.split('\n')) {
    const line = rawLine.trim()
    if (!line.startsWith('data:')) continue
    const payload = line.slice(5).trim()
    if (!payload || payload === '[DONE]') continue

    try {
      const parsed = JSON.parse(payload)
      model = parsed.model || model
      const choice = parsed.choices?.[0]
      const delta = choice?.delta || {}
      if (typeof delta.reasoning_content === 'string') reasoning += delta.reasoning_content
      if (typeof delta.content === 'string') text += delta.content
      if (!text && typeof choice?.message?.content === 'string') text = choice.message.content
      if (!reasoning && typeof choice?.message?.reasoning_content === 'string') {
        reasoning = choice.message.reasoning_content
      }
    } catch {}
  }

  if (text || reasoning || model) {
    return { text: text.trim(), reasoning_content: reasoning.trim() || null, model: model || null }
  }

  try {
    const parsed = JSON.parse(body)
    const choice = parsed.choices?.[0]
    return {
      text:
        choice?.message?.content ||
        parsed.output_text ||
        parsed.text ||
        '',
      reasoning_content: choice?.message?.reasoning_content || null,
      model: parsed.model || null,
    }
  } catch {}

  return { text: body.trim(), reasoning_content: null, model: null }
}

async function listZaiModels() {
  const page = state.zai.page
  if (!page) throw new Error('Z.AI Playwright not initialized')
  if (!page.url().includes('chat.z.ai')) {
    await page.goto('https://chat.z.ai/', { waitUntil: 'domcontentloaded' })
  }

  await waitForInteractiveSelector(page, [
    'textarea:visible',
    'div[contenteditable="true"]:visible',
    '[role="textbox"]:visible',
  ])

  const ids = await scanPageModelHintsWithRetries(page, 'zai', [
    '/api/models',
    '/api/model',
    '/api/paas/v4/models',
  ])
  addKnownZaiModels(ids)
  return modelListResponse(ids, 'zai', 'glm-5.2')
}

async function chatZai({ model, prompt, web_search = false }) {
  const page = state.zai.page
  if (!page) throw new Error('Z.AI Playwright not initialized')

  const template = await captureZaiTemplate(true)
  const headers = { ...template.headers }
  const prepared = buildTemplateBody(template, prompt, ensureSessionText(model, 'glm-5.2'))

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
  }, { url: template.url, headers, body: prepared.body })

  if (!response.ok) {
    throw new Error(`Z.AI upstream request failed with status ${response.status}`)
  }

  const parsed = extractOpenAIStyleResponse(response.body)
  if (!parsed.text) {
    throw new Error('Z.AI response was empty. Confirm session is active and the captured request template is still valid.')
  }

  const warnings = []
  if (web_search) {
    warnings.push('Z.AI web search toggle is not mapped yet. Current web-session defaults were used.')
  }
  if (model && !prepared.changedModel) {
    warnings.push('Z.AI live model switching is best-effort. Captured request template did not expose a writable model field.')
  }

  return {
    text: parsed.text,
    reasoning_content: parsed.reasoning_content,
    model: ensureSessionText(parsed.model, ensureSessionText(model, 'glm-5.2')),
    warning: warnings.join(' | ') || null,
  }
}

async function openZaiLogin({ runtime_dir, browser }) {
  await initZai({ runtime_dir, headless: false, browser })
  await state.zai.page.goto('https://chat.z.ai/', { waitUntil: 'domcontentloaded' })
}

async function initMeta({ runtime_dir, headless, browser }) {
  process.chdir(runtime_dir)
  if (state.meta.context && state.meta.headless === headless) return
  if (state.meta.context) {
    await closeContext(state.meta.context)
    state.meta.context = null
    state.meta.page = null
    state.meta.cachedHeaders = null
    state.meta.lastHeadersTime = 0
  }
  ensureDir(path.resolve('meta_profile'))
  const { engine, channel, executablePath } = resolveEngine(browser)
  state.meta.context = await engine.launchPersistentContext(path.resolve('meta_profile'), {
    headless,
    channel,
    executablePath,
    userAgent:
      'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/147.0.0.0 Safari/537.36',
    ignoreDefaultArgs: ['--enable-automation'],
    args: [
      '--disable-blink-features=AutomationControlled',
      '--disable-features=DevToolsDebuggingRestrictions',
    ],
  })
  await state.meta.context.addInitScript(() => {
    Object.defineProperty(navigator, 'webdriver', { get: () => undefined })
  })
  state.meta.page = await state.meta.context.newPage()
  state.meta.headless = headless
}

async function captureMetaTemplate(forceNew = false) {
  const page = state.meta.page
  if (!page) throw new Error('Meta AI Playwright not initialized')

  if (!forceNew && state.meta.cachedHeaders && Date.now() - state.meta.lastHeadersTime < 5 * 60 * 1000) {
    return state.meta.cachedHeaders
  }

  if (!page.url().includes('meta.ai') || forceNew) {
    await page.goto('https://www.meta.ai/', { waitUntil: 'domcontentloaded' })
  }

  await waitForInteractiveSelector(page, [
    'textarea:visible',
    '[role="textbox"]:visible',
    'div[contenteditable="true"]:visible',
    '[aria-label*="Message"]:visible',
    '[aria-label*="message"]:visible',
  ]).catch(() => {
    throw new Error('Timeout waiting for Meta AI chat input. Are you logged in?')
  })

  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error('Timeout waiting for Meta AI request template')), 60000)
    const routeHandler = async (route, request) => {
      const url = request.url()
      const method = request.method()
      if (method !== 'POST' || !/(?:meta\.ai|graph\.facebook\.com|graph\.meta\.com)/i.test(url)) {
        await route.continue()
        return
      }

      const postData = request.postData() || ''
      const signature = `${url}\n${postData}`
      if (!/(prompt|message|content|text|query|input|conversation|chat|llama|meta)/i.test(signature)) {
        await route.continue()
        return
      }

      clearTimeout(timeout)
      const reqHeaders = request.headers()
      const headers = {
        accept: reqHeaders.accept || '*/*',
        'accept-language': reqHeaders['accept-language'] || 'en-US,en;q=0.9',
        'content-type': reqHeaders['content-type'] || 'application/json',
        origin: reqHeaders.origin || 'https://www.meta.ai',
        referer: reqHeaders.referer || 'https://www.meta.ai/',
        'user-agent': reqHeaders['user-agent'] || '',
        authorization: reqHeaders.authorization || '',
      }

      state.meta.cachedHeaders = {
        headers,
        payload: postData,
        url,
      }
      state.meta.lastHeadersTime = Date.now()

      await route.abort('aborted')
      await page.unroute('**/*', routeHandler)
      resolve(state.meta.cachedHeaders)
    }

    page.route('**/*', routeHandler).then(async () => {
      const selector = await waitForInteractiveSelector(page, [
        'textarea:visible',
        '[role="textbox"]:visible',
        'div[contenteditable="true"]:visible',
        '[aria-label*="Message"]:visible',
        '[aria-label*="message"]:visible',
      ])
      await page.focus(selector)
      if (selector.includes('textarea')) {
        await page.fill(selector, '')
      }
      await page.type(selector, 'a', { delay: 50 })
      await sleep(1500)
      let clicked = false
      for (const buttonSelector of [
        'button[type="submit"]',
        '[aria-label*="Send"]',
        '[aria-label*="send"]',
        '[data-testid*="send"]',
        'button:has(svg)',
      ]) {
        try {
          const button = await page.$(buttonSelector)
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

function extractFlexibleResponse(body) {
  const openAIStyle = extractOpenAIStyleResponse(body)
  if (openAIStyle.text || openAIStyle.reasoning_content || openAIStyle.model) {
    return openAIStyle
  }

  const texts = []
  for (const rawLine of body.split('\n')) {
    const line = rawLine.trim()
    if (!line) continue
    const payload = line.startsWith('data:') ? line.slice(5).trim() : line
    if (!payload || payload === '[DONE]') continue
    try {
      collectResponseText(JSON.parse(payload), texts)
    } catch {}
  }

  if (!texts.length) {
    try {
      collectResponseText(JSON.parse(body), texts)
    } catch {}
  }

  return {
    text: texts.filter(Boolean).at(-1) || body.trim(),
    reasoning_content: null,
    model: null,
  }
}

async function listMetaModels() {
  const page = state.meta.page
  if (!page) throw new Error('Meta AI Playwright not initialized')
  if (!page.url().includes('meta.ai')) {
    await page.goto('https://www.meta.ai/', { waitUntil: 'domcontentloaded' })
  }

  await waitForInteractiveSelector(page, [
    'textarea:visible',
    '[role="textbox"]:visible',
    'div[contenteditable="true"]:visible',
  ])

  const ids = await scanPageModelHintsWithRetries(page, 'meta', [
    '/api/models',
    '/api/model',
  ])
  addKnownMetaModels(ids)
  return modelListResponse(ids, 'meta', 'meta-ai-web-session')
}

async function chatMeta({ model, prompt, web_search = false }) {
  const page = state.meta.page
  if (!page) throw new Error('Meta AI Playwright not initialized')

  const template = await captureMetaTemplate(true)
  const headers = { ...template.headers }
  const prepared = buildTemplateBody(template, prompt, ensureSessionText(model, 'meta-ai-web-session'))

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
  }, { url: template.url, headers, body: prepared.body })

  if (!response.ok) {
    throw new Error(`Meta AI upstream request failed with status ${response.status}`)
  }

  const parsed = extractFlexibleResponse(response.body)
  if (!parsed.text) {
    throw new Error('Meta AI response was empty. Confirm session is active and the captured request template is still valid.')
  }

  const warnings = []
  if (web_search) {
    warnings.push('Meta AI web search toggle is not mapped yet. Current web-session defaults were used.')
  }
  if (model && !prepared.changedModel) {
    warnings.push('Meta AI live model switching is best-effort. Captured request template did not expose a writable model field.')
  }

  return {
    text: parsed.text,
    reasoning_content: parsed.reasoning_content,
    model: ensureSessionText(parsed.model, ensureSessionText(model, 'meta-ai-web-session')),
    warning: warnings.join(' | ') || null,
  }
}

async function openMetaLogin({ runtime_dir, browser }) {
  await initMeta({ runtime_dir, headless: false, browser })
  await state.meta.page.goto('https://www.meta.ai/', { waitUntil: 'domcontentloaded' })
}

async function initKimi({ runtime_dir, headless, browser }) {
  process.chdir(runtime_dir)
  if (state.kimi.context && state.kimi.headless === headless) return
  if (state.kimi.context) {
    await closeContext(state.kimi.context)
    state.kimi.context = null
    state.kimi.page = null
    state.kimi.currentHeaders = {}
    state.kimi.cachedHeaders = null
    state.kimi.lastHeadersTime = 0
  }
  ensureDir(path.resolve('kimi_profile'))
  const { engine, channel, executablePath } = resolveEngine(browser)
  state.kimi.context = await engine.launchPersistentContext(path.resolve('kimi_profile'), {
    headless,
    channel,
    executablePath,
    userAgent: 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36',
    ignoreDefaultArgs: ['--enable-automation'],
    args: [
      '--disable-blink-features=AutomationControlled',
      // Fix Chrome 136+ DevTools debugging restrictions
      '--disable-features=DevToolsDebuggingRestrictions',
    ],
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
  assertSafeAccountId(account_id)
  process.chdir(runtime_dir)
  const slot = getQwenSlot(account_id)
  if (slot.context && slot.headless === headless) return
  if (slot.context) {
    await closeContext(slot.context)
    resetQwenSlot(slot)
  }
  const profileId = account_id || '_default'
  ensureDir(path.resolve('qwen_profiles', profileId))
  const { engine, channel, executablePath } = resolveEngine(browser)
  slot.context = await engine.launchPersistentContext(path.resolve('qwen_profiles', profileId), {
    headless,
    channel,
    executablePath,
    userAgent: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36',
    ignoreDefaultArgs: ['--enable-automation'],
    args: [
      '--disable-blink-features=AutomationControlled',
      // Fix Chrome 136+ DevTools debugging restrictions
      '--disable-features=DevToolsDebuggingRestrictions',
    ],
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
    await closeContext(slot.context)
  }
  resetQwenSlot(slot)
  if (account_id) {
    qwenAccounts.delete(account_id)
  }
  return { ok: true }
}

async function closeAll() {
  for (const key of ['deepseek', 'chatgpt', 'gemini', 'mistral', 'zai', 'meta', 'kimi', 'qwen']) {
    if (state[key].context) {
      await closeContext(state[key].context)
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
      await closeContext(slot.context)
    }
    qwenAccounts.delete(accountId)
  }
}

async function shutdownAndExit(code = 0) {
  await closeAll().catch((error) => {
    process.stderr.write(`shutdown cleanup failed: ${error instanceof Error ? error.message : String(error)}\n`)
  })
  process.exit(code)
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
    case 'mistral:init':
      return initMistral(params)
    case 'mistral:capture_headers':
      return captureMistralTemplate(!!params.force_new)
    case 'mistral:manual_login':
      return openMistralLogin(params)
    case 'mistral:list_models':
      return listMistralModels()
    case 'mistral:chat':
      return chatMistral(params)
    case 'zai:init':
      return initZai(params)
    case 'zai:capture_headers':
      return captureZaiTemplate(!!params.force_new)
    case 'zai:manual_login':
      return openZaiLogin(params)
    case 'zai:list_models':
      return listZaiModels()
    case 'zai:chat':
      return chatZai(params)
    case 'meta:init':
      return initMeta(params)
    case 'meta:capture_headers':
      return captureMetaTemplate(!!params.force_new)
    case 'meta:manual_login':
      return openMetaLogin(params)
    case 'meta:list_models':
      return listMetaModels()
    case 'meta:chat':
      return chatMeta(params)
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
    case 'mistral:shutdown':
    case 'zai:shutdown':
    case 'meta:shutdown':
    case 'kimi:shutdown':
    case 'qwen:shutdown':
      await closeAll()
      setImmediate(() => process.exit(0))
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
      let requestId = null
      try {
        const request = JSON.parse(line)
        requestId = request?.id ?? null
        const result = await handle(request.method, request.provider, request.params || {})
        send(request.id, result, null)
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error)
        if (requestId != null) {
          send(requestId, null, message)
        } else {
          process.stderr.write(`bridge request parse failed: ${message}\n`)
        }
      }
    }
    newlineIndex = buffer.indexOf('\n')
  }
})

process.stdin.on('end', () => {
  void shutdownAndExit(0)
})

process.on('SIGTERM', () => {
  void shutdownAndExit(0)
})

process.on('SIGINT', () => {
  void shutdownAndExit(0)
})
