import type {
  DashboardOverview,
  ProviderDetails,
  ProviderLogs,
  ProviderName,
  ProviderOverview,
  QwenAccountSummary,
  ServiceName,
} from '@/lib/types'

/**
 * Mock backend that emulates the original Tauri/Rust `invoke()` commands entirely
 * in the browser. It keeps an in-memory world that drifts over time so the arcade
 * dashboards show living telemetry instead of static numbers.
 */

export const providerOrder: ProviderName[] = ['qwen', 'deepseek', 'kimi', 'chatgpt', 'gemini', 'mistral', 'zai']

const HUB_PORT = 8787
const HUB_BASE = `http://127.0.0.1:${HUB_PORT}`

interface ProviderState {
  name: ProviderName
  running: boolean
  started_at: number | null
  login_state: 'unknown' | 'authenticated' | 'needs_login' | 'expired'
  health_status: 'idle' | 'healthy' | 'degraded' | 'booting'
  models: string[]
  web_search_supported: boolean
  last_error: string | null
  // Live telemetry
  latencyMs: number
  rpm: number
  errorRate: number
  uptimePct: number
  tokens: number
}

const seedModels: Record<ProviderName, string[]> = {
  qwen: ['qwen-max-2025-09', 'qwen-plus-2025-07-28', 'qwen-turbo-latest', 'qwen-vl-max', 'qwen-coder-32b'],
  deepseek: ['deepseek-v4-pro', 'deepseek-v4-pro-thinking', 'deepseek-coder-v3', 'deepseek-math'],
  kimi: ['kimi-k2.6', 'kimi-k2.6-thinking', 'kimi-vision-pro'],
  chatgpt: ['gpt-4o-web', 'gpt-4o-mini-web', 'o3-web-session'],
  gemini: ['gemini-2.5-pro-web', 'gemini-2.5-flash-web', 'gemini-vision-web'],
  mistral: ['mistral-large-web', 'mistral-medium-web', 'codestral-web'],
  zai: ['glm-5.2', 'glm-5.1', 'glm-5-turbo', 'glm-4.6'],
}

function clamp(value: number, min: number, max: number) {
  return Math.max(min, Math.min(max, value))
}

function jitter(value: number, amount: number) {
  return value + (Math.random() - 0.5) * amount
}

class MockWorld {
  hubRunning = false
  hubStartedAt: number | null = null
  startupMode = 'manual'
  startupServices: string[] = ['hub', 'qwen', 'deepseek']
  runnerReady = true
  runtimeIssues: string[] = []

  providers: Record<ProviderName, ProviderState>
  qwenAccounts: QwenAccountSummary[]
  openProviderLogins = new Set<ProviderName>()
  openQwenLogins = new Set<string>()
  logs: Record<ServiceName, string[]>

  // Rolling time-series used by arcade charts (newest last)
  throughputSeries: number[] = []
  latencySeries: number[] = []

  constructor() {
    this.providers = providerOrder.reduce(
      (acc, name, index) => {
        const running = index < 2
        acc[name] = {
          name,
          running,
          started_at: running ? Math.floor(Date.now() / 1000) - (index + 1) * 420 : null,
          login_state: running ? 'authenticated' : 'needs_login',
          health_status: running ? 'healthy' : 'idle',
          models: running ? seedModels[name] : [],
          web_search_supported: name === 'qwen' || name === 'gemini' || name === 'deepseek',
          last_error: null,
          latencyMs: running ? 30 + index * 8 : 0,
          rpm: running ? 120 - index * 18 : 0,
          errorRate: running ? 0.4 + index * 0.2 : 0,
          uptimePct: running ? 99.2 - index * 0.3 : 0,
          tokens: running ? 1_200_000 - index * 180_000 : 0,
        }
        return acc
      },
      {} as Record<ProviderName, ProviderState>,
    )

    this.qwenAccounts = [
      { id: 'qwen-ace-01', email: 'arcade.operator@rustproxy.io', has_password: true, created_at: '2026-05-02' },
      { id: 'qwen-ace-02', email: 'pixel.runner@rustproxy.io', has_password: false, created_at: '2026-05-19' },
    ]

    this.logs = {
      hub: [],
      qwen: [],
      deepseek: [],
      kimi: [],
      chatgpt: [],
      gemini: [],
      mistral: [],
      zai: [],
    }

    // Seed series
    for (let i = 0; i < 32; i += 1) {
      this.throughputSeries.push(clamp(jitter(420, 220), 60, 900))
      this.latencySeries.push(clamp(jitter(42, 22), 18, 92))
    }

    providerOrder.forEach((name) => {
      if (this.providers[name].running) {
        this.pushLog(name, `[boot] ${name} runner online :: pid ${1000 + Math.floor(Math.random() * 9000)}`)
        this.pushLog(name, `[probe] ${name} discovery ok :: ${this.providers[name].models.length} models`)
      }
    })
  }

  private pushLog(service: ServiceName, line: string) {
    const stamp = new Date().toLocaleTimeString('en-US', { hour12: false })
    this.logs[service].push(`${stamp} ${line}`)
    if (this.logs[service].length > 60) this.logs[service].shift()
  }

  /** Advance the simulation by one tick — called on an interval. */
  tick() {
    const liveProviders = providerOrder.filter((name) => this.providers[name].running)
    let totalRpm = 0
    let latAccum = 0
    let latCount = 0

    liveProviders.forEach((name) => {
      const p = this.providers[name]
      p.latencyMs = clamp(jitter(p.latencyMs, 6), 14, 140)
      p.rpm = clamp(jitter(p.rpm, 14), 12, 320)
      p.errorRate = clamp(jitter(p.errorRate, 0.3), 0, 6)
      p.tokens += Math.floor(p.rpm * 110)
      p.uptimePct = clamp(p.uptimePct + (Math.random() - 0.45) * 0.05, 95, 99.99)
      p.health_status = p.errorRate > 3.4 ? 'degraded' : 'healthy'
      totalRpm += p.rpm
      latAccum += p.latencyMs
      latCount += 1
      if (Math.random() < 0.08) {
        this.pushLog(name, `[req] 200 OK :: ${p.models[0] ?? 'model'} :: ${p.latencyMs.toFixed(0)}ms`)
      }
    })

    const throughput = this.hubRunning ? clamp(totalRpm * (3 + Math.random()), 40, 1400) : clamp(jitter(60, 40), 0, 120)
    const latency = latCount ? latAccum / latCount : clamp(jitter(46, 16), 20, 90)
    this.throughputSeries.push(throughput)
    this.latencySeries.push(latency)
    if (this.throughputSeries.length > 48) this.throughputSeries.shift()
    if (this.latencySeries.length > 48) this.latencySeries.shift()

    if (this.hubRunning && Math.random() < 0.12) {
      this.pushLog('hub', `[hub] routed batch :: ${Math.floor(throughput)} req/min :: avg ${latency.toFixed(0)}ms`)
    }
  }

  toOverview(): DashboardOverview {
    const providers: ProviderOverview[] = providerOrder.map((name) => {
      const p = this.providers[name]
      return {
        name,
        running: p.running,
        started_at: p.started_at,
        base_url: p.running ? `${HUB_BASE}/${name}` : '',
        health_status: p.health_status,
        login_state: p.login_state,
        model_count: p.models.length,
        models: [...p.models],
        web_search_supported: p.web_search_supported,
        last_error: p.last_error,
        // extended telemetry (consumed by arcade dashboards)
        latency_ms: Math.round(p.latencyMs),
        rpm: Math.round(p.rpm),
        error_rate: Number(p.errorRate.toFixed(2)),
        uptime_pct: Number(p.uptimePct.toFixed(2)),
        tokens: p.tokens,
      } as ProviderOverview
    })

    const modelCount = providers.reduce((sum, p) => sum + p.model_count, 0)

    return {
      generated_at: Math.floor(Date.now() / 1000),
      runtime: {
        edge_available: true,
        single_runner_ready: this.runnerReady,
        issues: this.runtimeIssues,
      },
      startup_config: {
        mode: this.startupMode,
        services: this.startupServices,
      },
      hub: {
        port: HUB_PORT,
        base_url: HUB_BASE,
        openapi_url: `${HUB_BASE}/docs`,
        api_key_enabled: true,
        running: this.hubRunning,
        started_at: this.hubStartedAt,
        health_status: this.hubRunning ? 'healthy' : 'offline',
        model_count: modelCount,
        provider_statuses: [],
        detail: this.hubRunning
          ? { uptime_s: this.hubStartedAt ? Math.floor(Date.now() / 1000) - this.hubStartedAt : 0 }
          : null,
      },
      providers,
      qwen_account_count: this.qwenAccounts.length,
      open_provider_login_sessions: [...this.openProviderLogins],
      open_qwen_account_login_sessions: [...this.openQwenLogins],
      // extended telemetry series
      throughput_series: [...this.throughputSeries],
      latency_series: [...this.latencySeries],
    } as DashboardOverview
  }

  startService(name: ServiceName) {
    if (name === 'hub') {
      this.hubRunning = true
      this.hubStartedAt = Math.floor(Date.now() / 1000)
      this.pushLog('hub', '[hub] RustProxyHub gateway online :: TLS 1.3 :: ready for routes')
      return
    }
    const p = this.providers[name]
    p.running = true
    p.started_at = Math.floor(Date.now() / 1000)
    p.health_status = 'booting'
    p.login_state = p.login_state === 'authenticated' ? 'authenticated' : 'needs_login'
    p.models = seedModels[name]
    p.latencyMs = 38
    p.rpm = 80
    p.errorRate = 0.6
    p.uptimePct = 99
    p.tokens = 50_000
    this.pushLog(name, `[boot] ${name} runner spawned :: warming buffers`)
    window.setTimeout(() => {
      p.health_status = 'healthy'
      this.pushLog(name, `[probe] ${name} discovery ok :: ${p.models.length} models`)
    }, 900)
  }
}

const world = new MockWorld()
let ticking = false

function ensureTicking() {
  if (ticking) return
  ticking = true
  window.setInterval(() => world.tick(), 1500)
}

function delay(ms: number) {
  return new Promise((resolve) => window.setTimeout(resolve, ms))
}

/** Drop-in replacement for the Tauri `invoke` function. */
export async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  ensureTicking()
  await delay(120 + Math.random() * 220)

  switch (command) {
    case 'dashboard_overview':
      return world.toOverview() as T

    case 'provider_details': {
      const provider = args?.provider as ProviderName
      const overview = world.toOverview().providers.find((p) => p.name === provider)!
      const details: ProviderDetails = {
        overview,
        detail: {
          latency_ms: (overview as ProviderOverview & { latency_ms?: number }).latency_ms ?? 0,
          requests_per_min: (overview as ProviderOverview & { rpm?: number }).rpm ?? 0,
          error_rate_pct: (overview as ProviderOverview & { error_rate?: number }).error_rate ?? 0,
          uptime_pct: (overview as ProviderOverview & { uptime_pct?: number }).uptime_pct ?? 0,
          tokens_served: (overview as ProviderOverview & { tokens?: number }).tokens ?? 0,
          web_search: overview.web_search_supported,
        },
        logs: [...world.logs[provider]],
        qwen_accounts: provider === 'qwen' ? [...world.qwenAccounts] : null,
      }
      return details as T
    }

    case 'provider_logs': {
      const provider = args?.provider as ServiceName
      const response: ProviderLogs = { provider, entries: [...world.logs[provider]] }
      return response as T
    }

    case 'list_qwen_accounts':
      return [...world.qwenAccounts] as T

    case 'add_qwen_account': {
      const request = args?.request as { email: string; password?: string }
      world.qwenAccounts = [
        ...world.qwenAccounts,
        {
          id: `qwen-ace-${String(world.qwenAccounts.length + 1).padStart(2, '0')}`,
          email: request.email,
          has_password: Boolean(request.password),
          created_at: new Date().toISOString().slice(0, 10),
        },
      ]
      return [...world.qwenAccounts] as T
    }

    case 'remove_qwen_account': {
      const accountId = args?.accountId as string
      world.qwenAccounts = world.qwenAccounts.filter((a) => a.id !== accountId)
      world.openQwenLogins.delete(accountId)
      return [...world.qwenAccounts] as T
    }

    case 'start_provider_login_session': {
      const request = args?.request as { provider: ProviderName }
      world.openProviderLogins.add(request.provider)
      return [] as unknown as T
    }

    case 'stop_provider_login_session': {
      const provider = args?.provider as ProviderName
      world.openProviderLogins.delete(provider)
      world.providers[provider].login_state = 'authenticated'
      return [] as unknown as T
    }

    case 'start_qwen_account_login_session': {
      const request = args?.request as { account_id: string }
      world.openQwenLogins.add(request.account_id)
      return [] as unknown as T
    }

    case 'stop_qwen_account_login_session': {
      const accountId = args?.account_id as string
      world.openQwenLogins.delete(accountId)
      return [] as unknown as T
    }

    case 'start_service': {
      world.startService(args?.name as ServiceName)
      return undefined as T
    }

    case 'run_workbench_request': {
      const request = args?.request as { model: string; prompt: string; web_search: boolean }
      await delay(700)
      const provider = request.model.split(':')[0]
      return {
        id: `cmpl-${Math.random().toString(36).slice(2, 10)}`,
        model: request.model,
        provider,
        web_search: request.web_search,
        created: Math.floor(Date.now() / 1000),
        choices: [
          {
            index: 0,
            finish_reason: 'stop',
            message: {
              role: 'assistant',
              content: `Hello from RustProxyHub! This response was routed through the ${provider} proxy. Your prompt "${request.prompt.slice(0, 64)}${request.prompt.length > 64 ? '…' : ''}" completed in ${(0.4 + Math.random()).toFixed(2)}s.`,
            },
          },
        ],
        usage: (() => {
          const prompt_tokens = 18 + Math.floor(Math.random() * 40)
          const completion_tokens = 60 + Math.floor(Math.random() * 120)
          return {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
          }
        })(),
      } as T
    }

    default:
      throw new Error(`mock invoke: unknown command "${command}"`)
  }
}
