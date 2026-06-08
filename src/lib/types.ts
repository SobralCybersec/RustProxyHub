export type ProviderName = 'deepseek' | 'kimi' | 'qwen'
export type ServiceName = 'hub' | ProviderName

export interface ServiceConfig {
  port: number
  apiKey: string
  browser: string
  headless: boolean
}

export interface ServiceModelSummary {
  id: string
  provider: ProviderName | null
}

export interface ServiceEndpoints {
  base_url: string | null
  health_url: string | null
  models_url: string | null
  chat_url: string | null
  openapi_url: string | null
  stop_url: string | null
  upload_url: string | null
}

export interface ServiceSnapshot {
  provider: ServiceName
  running: boolean
  port: number | null
  pid: number | null
  started_at: number | null
  launch_preview: string | null
  logs: string[]
  health: Record<string, unknown> | null
  model_count: number
  models: ServiceModelSummary[]
  admin_status: Record<string, unknown> | null
  endpoints: ServiceEndpoints
}

export interface QwenAccountSummary {
  id: string
  email: string
  has_password: boolean
  created_at: string | null
}

export interface DashboardSnapshot {
  tools_root: string
  rust_proxy_hub: string
  services: ServiceSnapshot[]
  qwen_accounts: QwenAccountSummary[]
  open_login_sessions: string[]
  provider_login_sessions: ProviderName[]
}
