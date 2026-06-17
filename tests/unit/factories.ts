import type { DashboardOverview, ProviderName, ProviderOverview, RuntimeDiagnostics } from '@/lib/types'

const providerOrder: ProviderName[] = ['qwen', 'deepseek', 'kimi', 'chatgpt', 'gemini', 'mistral']

function makeRuntime(overrides: Partial<RuntimeDiagnostics> = {}): RuntimeDiagnostics {
  return {
    node_path: 'C:/bundle/resources/node/node.exe',
    node_source: 'bundled-resource',
    helper_dir: 'C:/bundle/resources/playwright-bridge',
    edge_available: true,
    single_runner_ready: true,
    issues: [],
    ...overrides,
  }
}

function makeProvider(name: ProviderName, overrides: Partial<ProviderOverview> = {}): ProviderOverview {
  return {
    name,
    running: true,
    started_at: 1_717_171_717,
    base_url: `http://127.0.0.1:${name === 'qwen' ? '3000' : '3001'}`,
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
    app_data_dir: 'C:/Users/test/AppData/Local/RustProxyHub',
    helper_dir: runtime.helper_dir ?? 'C:/bundle/resources/playwright-bridge',
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
