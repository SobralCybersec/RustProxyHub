import { createHash, randomUUID } from 'node:crypto'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const localPlaywrightUrl = new URL('./node_modules/playwright/index.mjs', import.meta.url)
const fallbackPlaywrightUrl = new URL('../../proxy/deepsproxy/node_modules/playwright/index.mjs', import.meta.url)
const playwright = await import(
  fs.existsSync(fileURLToPath(localPlaywrightUrl)) ? localPlaywrightUrl : fallbackPlaywrightUrl
)
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
  },
  kimi: {
    context: null,
    page: null,
    currentHeaders: {},
    cachedHeaders: null,
    lastHeadersTime: 0,
  },
  qwen: {
    context: null,
    page: null,
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
  slot.currentHeaders = {}
  slot.cachedHeaders = null
  slot.lastHeadersTime = 0
  slot.cookieCache = null
  slot.userAgent = null
}

async function initDeepSeek({ runtime_dir, headless, browser }) {
  process.chdir(runtime_dir)
  if (state.deepseek.context) return
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

async function initKimi({ runtime_dir, headless, browser }) {
  process.chdir(runtime_dir)
  if (state.kimi.context) return
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
  if (slot.context) return
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
  for (const key of ['deepseek', 'kimi', 'qwen']) {
    if (state[key].context) {
      await state[key].context.close()
      if (key === 'qwen') {
        resetQwenSlot(state.qwen)
      } else {
        state[key].context = null
        state[key].page = null
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
