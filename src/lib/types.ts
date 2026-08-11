export type ProviderName = 'qwen' | 'deepseek' | 'kimi' | 'chatgpt' | 'gemini' | 'mistral' | 'zai' | 'meta'
export type ServiceName = 'hub' | ProviderName

export interface HubConfigResponse {
  port: number
  base_url: string
  openapi_url: string
  api_key_enabled: boolean
}

export interface HubOverview extends HubConfigResponse {
  running: boolean
  started_at: number | null
  health_status: string
  model_count: number
  provider_statuses: Record<string, unknown>[]
  detail: Record<string, unknown> | null
}

export interface ProviderOverview {
  name: ProviderName
  running: boolean
  started_at: number | null
  base_url: string
  health_status: string
  login_state: string
  model_count: number
  models: string[]
  model_modes?: Record<string, string>
  web_search_supported: boolean
  last_error: string | null
  // Live telemetry (provided by the mock backend)
  latency_ms?: number
  rpm?: number
  error_rate?: number
  uptime_pct?: number
  tokens?: number
}

export interface RuntimeDiagnostics {
  browser_available: boolean
  single_runner_ready: boolean
  issues: string[]
}

export interface StartupConfig {
  mode: string
  services: string[]
}

export interface DashboardOverview {
  generated_at: number
  runtime: RuntimeDiagnostics
  startup_config: StartupConfig
  hub: HubOverview
  providers: ProviderOverview[]
  qwen_account_count?: number
  open_provider_login_sessions: ProviderName[]
  open_qwen_account_login_sessions?: string[]
  // Live telemetry series (provided by the mock backend)
  throughput_series?: number[]
  latency_series?: number[]
}

export interface ProviderDetails {
  overview: ProviderOverview
  detail: Record<string, unknown> | null
  logs: string[]
  qwen_accounts?: QwenAccountSummary[]
}

export interface QwenAccountSummary {
  id: string
  email: string
  has_password: boolean
  created_at: string | null
}

export interface ProviderLogs {
  provider: ServiceName
  entries: string[]
}

export interface BrowserPrefs {
  qwen: string
  deepseek: string
  kimi: string
  chatgpt: string
  gemini: string
  mistral: string
  zai: string
  meta: string
}
