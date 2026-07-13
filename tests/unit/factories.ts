import type { DashboardOverview, ProviderName, ProviderOverview, RuntimeDiagnostics } from '@/lib/types'

const providerOrder: ProviderName[] = ['qwen', 'deepseek', 'kimi', 'chatgpt', 'gemini', 'mistral', 'zai', 'meta']

function makeRuntime(overrides: Partial<RuntimeDiagnostics> = {}): RuntimeDiagnostics {
  return {
    edge_available: true,
    single_runner_ready: true,
    issues: [],
    ...overrides,
  }
}

function makeProvider(name: ProviderName, overrides: Partial<ProviderOverview> = {}): ProviderOverview {
  const portByProvider: Record<ProviderName, number> = {
    qwen: 3000,
    deepseek: 3001,
    kimi: 3002,
    chatgpt: 3003,
    gemini: 3004,
    mistral: 3005,
    zai: 3006,
    meta: 3007,
  }
  return {
    name,
    running: true,
    started_at: 1_717_171_717,
    base_url: `http://127.0.0.1:${portByProvider[name]}`,
    health_status: 'healthy',
    login_state: 'ready',
    model_count: 1,
    models: [`${name}-model`],
    web_search_supported: true,
    last_error: null,
    ...overrides,
  }
}

export function makeOverview(overrides: Partial<DashboardOverview> = {}): DashboardOverview {
  const runtime = makeRuntime(overrides.runtime)
  const providers = overrides.providers ?? providerOrder.map((name) => makeProvider(name))

  return {
    generated_at: 1_717_171_717,
    runtime,
    startup_config: {
      mode: 'manual',
      services: [],
    },
    hub: {
      port: 3100,
      base_url: 'http://127.0.0.1:3100',
      openapi_url: 'http://127.0.0.1:3100/openapi.json',
      api_key_enabled: false,
      running: true,
      started_at: 1_717_171_717,
      health_status: 'healthy',
      model_count: providers.length,
      provider_statuses: [],
      detail: null,
    },
    providers,
    qwen_account_count: 0,
    open_provider_login_sessions: [],
    open_qwen_account_login_sessions: [],
    ...overrides,
  }
}
