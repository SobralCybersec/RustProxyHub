import { createHash, randomBytes, randomUUID } from 'node:crypto'
import fs from 'node:fs/promises'
import { createServer } from 'node:http'
import os from 'node:os'
import path from 'node:path'

const CODEX_BASE_URL = 'https://chatgpt.com/backend-api/codex'
const CODEX_CLIENT_VERSION = '0.144.1'
const CODEX_REGISTRY_URL = 'https://registry.npmjs.org/@openai/codex/latest'
const CODEX_CLIENT_VERSION_CACHE_MS = 60 * 60 * 1000
const OAUTH_CLIENT_ID = 'app_EMoamEEZ73f0CkXaXp7hrann'
const OAUTH_ISSUER = 'https://auth.openai.com'
const OAUTH_SCOPE = 'openid profile email offline_access'
const REDIRECT_HOST = 'localhost'
const REDIRECT_PORT = 1455
const REDIRECT_URI = `http://${REDIRECT_HOST}:${REDIRECT_PORT}/auth/callback`
const REFRESH_EXPIRY_MARGIN_MS = 5 * 60 * 1000
const REFRESH_INTERVAL_MS = 55 * 60 * 1000
const MODEL_CACHE_MS = 5 * 60 * 1000

let cachedCodexClientVersion = null
let cachedCodexClientVersionExpiresAt = 0
let codexClientVersionPromise = null

function isRecord(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
}

function semanticVersion(value) {
  const match = typeof value === 'string' ? value.trim().match(/\b\d+\.\d+\.\d+\b/) : null
  return match?.[0] || null
}

export async function resolveCodexClientVersion(fetchImpl = globalThis.fetch) {
  const now = Date.now()
  if (cachedCodexClientVersion && cachedCodexClientVersionExpiresAt > now) {
    return cachedCodexClientVersion
  }
  if (codexClientVersionPromise) return codexClientVersionPromise

  codexClientVersionPromise = (async () => {
    try {
      const response = await fetchImpl(CODEX_REGISTRY_URL, { headers: { accept: 'application/json' } })
      if (response.ok) {
        const version = semanticVersion((await response.json())?.version)
        if (version) {
          cachedCodexClientVersion = version
          cachedCodexClientVersionExpiresAt = Date.now() + CODEX_CLIENT_VERSION_CACHE_MS
          return version
        }
      }
    } catch {}

    cachedCodexClientVersion = CODEX_CLIENT_VERSION
    cachedCodexClientVersionExpiresAt = Date.now() + CODEX_CLIENT_VERSION_CACHE_MS
    return CODEX_CLIENT_VERSION
  })().finally(() => {
    codexClientVersionPromise = null
  })
  return codexClientVersionPromise
}

export function resetCodexClientVersionCache() {
  cachedCodexClientVersion = null
  cachedCodexClientVersionExpiresAt = 0
  codexClientVersionPromise = null
}

function base64Url(value) {
  return Buffer.from(value).toString('base64url')
}

function escapeHtml(value) {
  return String(value)
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;')
}

export function parseJwtClaims(token) {
  if (typeof token !== 'string') return null
  const parts = token.split('.')
  if (parts.length !== 3) return null
  try {
    const claims = JSON.parse(Buffer.from(parts[1], 'base64url').toString('utf8'))
    return isRecord(claims) ? claims : null
  } catch {
    return null
  }
}

export function deriveAccountId(token) {
  const claims = parseJwtClaims(token)
  if (!claims) return null
  const auth = claims['https://api.openai.com/auth']
  if (isRecord(auth) && typeof auth.chatgpt_account_id === 'string') {
    return auth.chatgpt_account_id
  }
  if (typeof claims.chatgpt_account_id === 'string') return claims.chatgpt_account_id
  const firstOrganization = Array.isArray(claims.organizations) ? claims.organizations[0] : null
  return isRecord(firstOrganization) && typeof firstOrganization.id === 'string' ? firstOrganization.id : null
}

function isFedRampToken(token) {
  const auth = parseJwtClaims(token)?.['https://api.openai.com/auth']
  return isRecord(auth) && auth.chatgpt_account_is_fedramp === true
}

function isCodexApiModelSlug(slug) {
  const lower = String(slug || '').toLowerCase()
  if (!lower || lower.startsWith('chatgpt')) return false
  if (lower.includes('workspace.model.') || lower.endsWith('.access')) return false
  return !lower.endsWith('link')
}

function codexModelItems(payload) {
  const raw = [
    ...(Array.isArray(payload?.models) ? payload.models : []),
    ...(Array.isArray(payload?.data) ? payload.data : []),
  ]
  const seen = new Set()
  return raw
    .map(item => {
      if (typeof item === 'string') return { slug: item }
      if (!isRecord(item)) return null
      const slug = typeof item.slug === 'string' ? item.slug : item.id
      return typeof slug === 'string' && slug.trim() ? { ...item, slug: slug.trim() } : null
    })
    .filter(Boolean)
    .filter(item => {
      if (seen.has(item.slug)) return false
      seen.add(item.slug)
      return true
    })
}

export function shouldRefreshAccessToken(accessToken, lastRefresh, now = Date.now()) {
  if (!accessToken) return true
  const exp = parseJwtClaims(accessToken)?.exp
  if (typeof exp === 'number' && exp * 1000 <= now + REFRESH_EXPIRY_MARGIN_MS) return true
  const refreshedAt = typeof lastRefresh === 'string' ? Date.parse(lastRefresh) : Number.NaN
  return Number.isFinite(refreshedAt) && refreshedAt <= now - REFRESH_INTERVAL_MS
}

export function buildCodexResponsesRequest({ model, prompt, instructions, modelInfo = null, webSearch = false }) {
  const userInput = {
    role: 'user',
    content: [{ type: 'input_text', text: prompt }],
  }
  const body = {
    model,
    instructions: instructions || '',
    input: [userInput],
    store: false,
    stream: true,
    include: ['reasoning.encrypted_content'],
  }

  const reasoning = {}
  if (typeof modelInfo?.default_reasoning_level === 'string') {
    reasoning.effort = modelInfo.default_reasoning_level
  }
  if (modelInfo?.use_responses_lite === true) reasoning.context = 'all_turns'
  if (Object.keys(reasoning).length > 0) body.reasoning = reasoning

  if (modelInfo?.support_verbosity === true && typeof modelInfo.default_verbosity === 'string') {
    body.text = { verbosity: modelInfo.default_verbosity }
  }

  if (webSearch) body.force_use_tool = 'web'

  if (modelInfo?.use_responses_lite === true) {
    if (body.instructions) {
      body.input.unshift({
        role: 'developer',
        content: [{ type: 'input_text', text: body.instructions }],
      })
    }
    body.instructions = ''
    body.parallel_tool_calls = false
  }

  return { body, useResponsesLite: modelInfo?.use_responses_lite === true }
}

function parseEventBlock(block) {
  const event = { data: '' }
  const data = []
  for (const line of block.split(/\r?\n/)) {
    if (line.startsWith('event:')) event.event = line.slice(6).trim()
    if (line.startsWith('data:')) data.push(line.slice(5).trimStart())
  }
  event.data = data.join('\n')
  return event
}

async function* iterateSse(stream) {
  const reader = stream.getReader()
  const decoder = new TextDecoder()
  let buffer = ''
  try {
    while (true) {
      const { done, value } = await reader.read()
      if (done) break
      buffer += decoder.decode(value, { stream: true })
      const blocks = buffer.split(/\r?\n\r?\n/)
      buffer = blocks.pop() || ''
      for (const block of blocks) if (block.trim()) yield parseEventBlock(block)
    }
    buffer += decoder.decode()
    if (buffer.trim()) yield parseEventBlock(buffer)
  } finally {
    reader.releaseLock()
  }
}

function collectTextFields(value, out = [], depth = 0, key = '') {
  if (depth > 10 || value == null) return out
  if (typeof value === 'string') {
    if (/^(?:text|output_text|content|refusal|message)$/i.test(key) && value.trim()) out.push(value)
    return out
  }
  if (Array.isArray(value)) {
    for (const item of value) collectTextFields(item, out, depth + 1, key)
    return out
  }
  if (isRecord(value)) {
    for (const [childKey, childValue] of Object.entries(value)) collectTextFields(childValue, out, depth + 1, childKey)
  }
  return out
}

function textFromResponse(response) {
  if (!response) return ''
  if (typeof response.output_text === 'string') return response.output_text
  if (Array.isArray(response.output)) {
    const text = response.output
      .flatMap(item => {
        if (!Array.isArray(item?.content)) return []
        return item.content
      })
      .map(content => {
        if (typeof content?.text === 'string') return content.text
        if (typeof content?.output_text === 'string') return content.output_text
        if (typeof content?.refusal === 'string') return content.refusal
        return ''
      })
      .join('')
    if (text) return text
  }
  return collectTextFields(response).join('')
}

export function upstreamCacheMetadata(headers) {
  const cache = {}
  for (const name of ['cache-control', 'age', 'cf-cache-status']) {
    const value = headers?.get?.(name)
    if (value) cache[name] = value
  }
  return Object.keys(cache).length ? cache : null
}

function errorFromEvent(event) {
  const error = event?.error || event?.response?.error
  if (typeof error === 'string') return error
  if (isRecord(error) && typeof error.message === 'string') return error.message
  if (typeof event?.message === 'string') return event.message
  return null
}

export async function collectCodexResponse(stream) {
  if (!stream) throw new Error('Codex response did not include an SSE body')
  let text = ''
  let reasoning = ''
  let refusal = ''
  let latestResponse = null

  for await (const event of iterateSse(stream)) {
    if (!event.data || event.data === '[DONE]') continue
    let parsed
    try {
      parsed = JSON.parse(event.data)
    } catch {
      continue
    }
    const type = typeof parsed.type === 'string' ? parsed.type : event.event
    if ((type === 'response.output_text.delta' || type === 'response.text.delta') && typeof parsed.delta === 'string') text += parsed.delta
    if (typeof parsed.output_text === 'string') text += parsed.output_text
    if (type === 'response.reasoning_summary_text.delta' && typeof parsed.delta === 'string') {
      reasoning += parsed.delta
    }
    if (type === 'response.refusal.delta' && typeof parsed.delta === 'string') refusal += parsed.delta
    if (isRecord(parsed.response)) latestResponse = parsed.response
    if (type === 'error' || type === 'response.failed' || type === 'response.incomplete') {
      throw new Error(errorFromEvent(parsed) || `Codex response ended with ${type}`)
    }
  }

  if (!text) text = textFromResponse(latestResponse)
  if (!text && refusal) text = refusal
  if (!text) throw new Error('Codex response completed without assistant text')
  return {
    text,
    reasoningContent: reasoning || null,
    responseId: typeof latestResponse?.id === 'string' ? latestResponse.id : null,
    upstreamUsage: isRecord(latestResponse?.usage) ? latestResponse.usage : null,
    upstreamCache: isRecord(latestResponse?.cache) ? latestResponse.cache : null,
  }
}

function tokenRequestError(status, body) {
  let detail = ''
  try {
    const parsed = JSON.parse(body)
    detail = parsed.error_description || parsed.message || parsed.detail || ''
  } catch {}
  return new Error(`OpenAI OAuth token request failed with HTTP ${status}${detail ? `: ${detail}` : ''}`)
}

function requestIdFrom(response) {
  return response?.headers?.get?.('x-request-id') || response?.headers?.get?.('openai-request-id') || ''
}

function upstreamError(status, body, response = null) {
  let detail = body
  try {
    const parsed = JSON.parse(body)
    detail = parsed.detail || parsed.error?.message || parsed.message || body
  } catch {}
  const requestId = requestIdFrom(response)
  return new Error(
    `Codex upstream request failed with HTTP ${status}${detail ? `: ${String(detail).slice(0, 400)}` : ''}${requestId ? ` (x-request-id: ${requestId})` : ''}`
  )
}

function unexpectedContentTypeError(operation, response, body) {
  const contentType = response.headers.get('content-type') || 'missing'
  const requestId = requestIdFrom(response)
  const challenge = /<html|security verification|cloudflare|captcha|sign in/i.test(body)
    ? ' ChatGPT returned a sign-in or security-verification page; complete verification in the visible login browser, then retry.'
    : ''
  return new Error(
    `Codex ${operation} returned invalid content type ${contentType}.${challenge}${requestId ? ` (x-request-id: ${requestId})` : ''}`
  )
}

export class ChatGptOAuthClient {
  constructor(runtimeDir, fetchImpl = globalThis.fetch, versionFetchImpl = globalThis.fetch) {
    this.runtimeDir = runtimeDir
    this.fetch = fetchImpl
    this.versionFetch = versionFetchImpl
    this.authPath = path.join(runtimeDir, 'chatgpt_oauth.json')
    this.loginServer = null
    this.loginTimer = null
    this.modelCache = null
    this.modelsPromise = null
    this.sessionPromise = null
    this.refreshPromise = null
  }

  authCandidates() {
    const codexHome = process.env.CODEX_HOME
      ? path.join(process.env.CODEX_HOME, 'auth.json')
      : path.join(os.homedir(), '.codex', 'auth.json')
    return [...new Set([this.authPath, codexHome])]
  }

  async readAuthFile() {
    for (const candidate of this.authCandidates()) {
      try {
        const value = JSON.parse(await fs.readFile(candidate, 'utf8'))
        if (isRecord(value)) return { value, sourcePath: candidate }
      } catch (error) {
        if (error?.code !== 'ENOENT') throw error
      }
    }
    throw new Error('ChatGPT OAuth credentials not found. Open Login Studio and sign in.')
  }

  async writeAuthFile(targetPath, value) {
    await fs.mkdir(path.dirname(targetPath), { recursive: true })
    await fs.writeFile(targetPath, `${JSON.stringify(value, null, 2)}\n`, { mode: 0o600 })
    await fs.chmod(targetPath, 0o600)
  }

  async requestTokens(body, encoding) {
    const isForm = encoding === 'form'
    const response = await this.fetch(`${OAUTH_ISSUER}/oauth/token`, {
      method: 'POST',
      headers: {
        'content-type': isForm ? 'application/x-www-form-urlencoded' : 'application/json',
      },
      body: isForm ? new URLSearchParams(body) : JSON.stringify(body),
    })
    const raw = await response.text()
    if (!response.ok) throw tokenRequestError(response.status, raw)
    const parsed = JSON.parse(raw)
    if (typeof parsed.access_token !== 'string') {
      throw new Error('OpenAI OAuth token response did not include access_token')
    }
    return parsed
  }

  async saveTokens(tokens, sourcePath = this.authPath, existing = {}) {
    const accountId = tokens.account_id || deriveAccountId(tokens.id_token) || deriveAccountId(tokens.access_token)
    if (!accountId) throw new Error('ChatGPT account id was missing from OAuth token response')
    const value = {
      ...existing,
      auth_mode: 'chatgpt',
      tokens: {
        id_token: tokens.id_token,
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        account_id: accountId,
      },
      last_refresh: new Date().toISOString(),
    }
    await this.writeAuthFile(sourcePath, value)
    return value
  }

  async loadSession(forceRefresh = false) {
    const activePromise = forceRefresh ? this.refreshPromise : this.sessionPromise
    if (activePromise) return activePromise

    const promise = this.loadSessionOnce(forceRefresh)
    if (forceRefresh) this.refreshPromise = promise
    else this.sessionPromise = promise
    try {
      return await promise
    } finally {
      if (forceRefresh && this.refreshPromise === promise) this.refreshPromise = null
      if (!forceRefresh && this.sessionPromise === promise) this.sessionPromise = null
    }
  }

  async loadSessionOnce(forceRefresh = false) {
    const loaded = await this.readAuthFile()
    let auth = loaded.value
    let tokens = isRecord(auth.tokens) ? auth.tokens : {}
    if (
      tokens.refresh_token &&
      (forceRefresh || shouldRefreshAccessToken(tokens.access_token, auth.last_refresh))
    ) {
      const refreshed = await this.requestTokens(
        {
          grant_type: 'refresh_token',
          refresh_token: tokens.refresh_token,
          client_id: OAUTH_CLIENT_ID,
        },
        'json'
      )
      auth = await this.saveTokens(
        {
          ...refreshed,
          id_token: refreshed.id_token || tokens.id_token,
          refresh_token: refreshed.refresh_token || tokens.refresh_token,
          account_id: refreshed.account_id || tokens.account_id,
        },
        loaded.sourcePath,
        auth
      )
      tokens = auth.tokens
    }

    const accessToken = tokens.access_token
    const accountId = tokens.account_id || deriveAccountId(tokens.id_token) || deriveAccountId(accessToken)
    if (!accessToken || !accountId) {
      throw new Error('ChatGPT OAuth credentials are incomplete. Open Login Studio and sign in again.')
    }
    return {
      accessToken,
      accountId,
      idToken: tokens.id_token,
      isFedRamp: isFedRampToken(tokens.id_token) || isFedRampToken(accessToken),
      canRefresh: Boolean(tokens.refresh_token),
    }
  }

  async authenticatedFetch(endpoint, init = {}) {
    const send = async session => {
      const headers = new Headers(init.headers)
      headers.set('authorization', `Bearer ${session.accessToken}`)
      headers.set('chatgpt-account-id', session.accountId)
      if (!headers.has('accept')) headers.set('accept', 'application/json')
      if (!headers.has('x-client-request-id')) headers.set('x-client-request-id', randomUUID())
      if (session.isFedRamp) headers.set('x-openai-fedramp', 'true')
      return this.fetch(`${CODEX_BASE_URL}${endpoint}`, { ...init, headers })
    }

    const session = await this.loadSession()
    const response = await send(session)
    if (response.status !== 401 || !session.canRefresh) return response
    return send(await this.loadSession(true))
  }

  async models() {
    if (this.modelCache && this.modelCache.expiresAt > Date.now()) return this.modelCache.models
    if (this.modelsPromise) return this.modelsPromise

    const promise = (async () => {
      const clientVersion = await resolveCodexClientVersion(this.versionFetch)
      const endpoint = `/models?client_version=${encodeURIComponent(clientVersion)}`
      const response = await this.authenticatedFetch(endpoint)
      const raw = await response.text()
      if (!response.ok) throw upstreamError(response.status, raw, response)
      if (!response.headers.get('content-type')?.toLowerCase().includes('application/json')) {
        throw unexpectedContentTypeError('model discovery', response, raw)
      }
      const payload = JSON.parse(raw)
      const rawModels = codexModelItems(payload)
      const catalogueFields = ['models', 'data'].filter(field => Array.isArray(payload?.[field]))
      const models = rawModels.filter(
        item =>
          item.supported_in_api !== false &&
          (item.visibility === undefined || item.visibility === 'list') &&
          isCodexApiModelSlug(item.slug)
      )
      if (models.length === 0) throw new Error('Codex returned an empty models list')
      this.modelCache = {
        models,
        discovery: {
          provider: 'codex',
          source: 'oauth',
          api: 'codex_responses',
          endpoint: `/backend-api/codex${endpoint}`,
          catalogue_fields: catalogueFields,
          raw_count: rawModels.length,
          accepted_count: models.length,
        },
        expiresAt: Date.now() + MODEL_CACHE_MS,
      }
      return models
    })()
    this.modelsPromise = promise
    try {
      return await promise
    } finally {
      if (this.modelsPromise === promise) this.modelsPromise = null
    }
  }

  async listModels() {
    const models = await this.models()
    return {
      object: 'list',
      data: models.map(item => ({
        id: item.slug,
        object: 'model',
        owned_by: 'codex',
        api: 'codex_responses',
      })),
      discovery: this.modelCache?.discovery || {
        provider: 'codex',
        source: 'oauth',
        api: 'codex_responses',
        endpoint: `/backend-api/codex/models?client_version=${CODEX_CLIENT_VERSION}`,
      },
    }
  }

  async chat({ model, prompt, systemPrompt, webSearch }) {
    const models = await this.models()
    const requested = typeof model === 'string' ? model.trim() : ''
    const exact = requested ? models.find(item => item.slug === requested) : null
    const selected = !requested || requested === 'chatgpt-web-session' ? models[0] : exact || models[0]
    const warning =
      requested && requested !== 'chatgpt-web-session' && !exact
        ? `ChatGPT Codex model ${requested} is unavailable; using ${selected.slug}.`
        : null
    const prepared = buildCodexResponsesRequest({
      model: selected.slug,
      prompt,
      instructions: systemPrompt,
      modelInfo: selected,
      webSearch,
    })
    const headers = { 'content-type': 'application/json', accept: 'text/event-stream' }
    if (prepared.useResponsesLite) headers['x-openai-internal-codex-responses-lite'] = 'true'
    const response = await this.authenticatedFetch('/responses', {
      method: 'POST',
      headers,
      body: JSON.stringify(prepared.body),
    })
    if (!response.ok) throw upstreamError(response.status, await response.text(), response)
    if (!response.headers.get('content-type')?.toLowerCase().includes('text/event-stream')) {
      throw unexpectedContentTypeError('response stream', response, await response.text())
    }
    const result = await collectCodexResponse(response.body)
    return {
      text: result.text,
      reasoning_content: result.reasoningContent,
      model: selected.slug,
      conversation_id: result.responseId,
      upstream_usage: result.upstreamUsage,
      upstream_cache: result.upstreamCache || upstreamCacheMetadata(response.headers),
      warning,
    }
  }

  async startLogin(page) {
    await this.closeLoginServer()
    const state = base64Url(randomBytes(24))
    const codeVerifier = base64Url(randomBytes(48))
    const codeChallenge = base64Url(createHash('sha256').update(codeVerifier).digest())
    const authorizationUrl = new URL(`${OAUTH_ISSUER}/oauth/authorize`)
    authorizationUrl.searchParams.set('response_type', 'code')
    authorizationUrl.searchParams.set('client_id', OAUTH_CLIENT_ID)
    authorizationUrl.searchParams.set('redirect_uri', REDIRECT_URI)
    authorizationUrl.searchParams.set('scope', OAUTH_SCOPE)
    authorizationUrl.searchParams.set('state', state)
    authorizationUrl.searchParams.set('code_challenge', codeChallenge)
    authorizationUrl.searchParams.set('code_challenge_method', 'S256')
    authorizationUrl.searchParams.set('id_token_add_organizations', 'true')
    authorizationUrl.searchParams.set('codex_cli_simplified_flow', 'true')
    authorizationUrl.searchParams.set('originator', 'codex_cli_rs')

    const server = createServer(async (request, response) => {
      const url = new URL(request.url || '/', REDIRECT_URI)
      if (url.pathname !== '/auth/callback') {
        response.writeHead(404, { 'content-type': 'text/plain; charset=utf-8' })
        response.end('Not found')
        return
      }
      const error = url.searchParams.get('error')
      const code = url.searchParams.get('code')
      if (error || !code || url.searchParams.get('state') !== state) {
        response.writeHead(400, { 'content-type': 'text/html; charset=utf-8' })
        response.end('<h1>ChatGPT sign-in failed</h1><p>Return to RustProxyHub and try again.</p>')
        this.finishLoginServer(server)
        return
      }
      try {
        const tokens = await this.requestTokens(
          {
            grant_type: 'authorization_code',
            code,
            redirect_uri: REDIRECT_URI,
            client_id: OAUTH_CLIENT_ID,
            code_verifier: codeVerifier,
          },
          'form'
        )
        await this.saveTokens(tokens)
        this.modelCache = null
        response.writeHead(200, { 'content-type': 'text/html; charset=utf-8' })
        response.end('<h1>ChatGPT sign-in complete</h1><p>Credentials saved locally. Return to RustProxyHub.</p>')
      } catch (exchangeError) {
        response.writeHead(500, { 'content-type': 'text/html; charset=utf-8' })
        response.end(`<h1>ChatGPT sign-in failed</h1><p>${escapeHtml(exchangeError.message || exchangeError)}</p>`)
      }
      this.finishLoginServer(server)
    })

    await new Promise((resolve, reject) => {
      server.once('error', reject)
      server.listen(REDIRECT_PORT, '127.0.0.1', resolve)
    })
    this.loginServer = server
    this.loginTimer = setTimeout(() => this.finishLoginServer(server), 5 * 60 * 1000)
    this.loginTimer.unref()
    try {
      await page.goto(authorizationUrl.toString(), { waitUntil: 'domcontentloaded' })
    } catch (error) {
      await this.closeLoginServer()
      throw error
    }
    return { authorization_url: authorizationUrl.toString() }
  }

  finishLoginServer(server) {
    if (this.loginServer !== server) return
    if (this.loginTimer) clearTimeout(this.loginTimer)
    this.loginTimer = null
    this.loginServer = null
    server.close()
  }

  async closeLoginServer() {
    if (this.loginTimer) clearTimeout(this.loginTimer)
    this.loginTimer = null
    const server = this.loginServer
    this.loginServer = null
    if (!server) return
    await new Promise(resolve => server.close(resolve))
  }

  async close() {
    await this.closeLoginServer()
  }
}
