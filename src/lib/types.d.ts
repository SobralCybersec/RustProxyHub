export type ProviderName = 'qwen' | 'deepseek' | 'kimi' | 'chatgpt' | 'gemini';
export type ServiceName = 'hub' | ProviderName;
export interface HubConfigResponse {
    port: number;
    base_url: string;
    openapi_url: string;
    api_key_enabled: boolean;
}
export interface HubOverview extends HubConfigResponse {
    running: boolean;
    started_at: number | null;
    health_status: string;
    model_count: number;
    provider_statuses: Record<string, unknown>[];
    detail: Record<string, unknown> | null;
}
export interface ProviderOverview {
    name: ProviderName;
    running: boolean;
    started_at: number | null;
    base_url: string;
    health_status: string;
    login_state: string;
    model_count: number;
    models: string[];
    web_search_supported: boolean;
    last_error: string | null;
}
export interface DashboardOverview {
    generated_at: number;
    app_data_dir: string;
    helper_dir: string;
    hub: HubOverview;
    providers: ProviderOverview[];
    qwen_account_count: number;
    open_provider_login_sessions: ProviderName[];
    open_qwen_account_login_sessions: string[];
}
export interface QwenAccountSummary {
    id: string;
    email: string;
    has_password: boolean;
    created_at: string | null;
}
export interface ProviderDetails {
    overview: ProviderOverview;
    detail: Record<string, unknown> | null;
    logs: string[];
    qwen_accounts: QwenAccountSummary[] | null;
}
export interface ProviderLogs {
    provider: ServiceName;
    entries: string[];
}
export interface BrowserPrefs {
    qwen: string;
    deepseek: string;
    kimi: string;
    chatgpt: string;
    gemini: string;
}
