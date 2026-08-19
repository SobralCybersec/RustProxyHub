mod qwen_accounts;
mod runtime;

use crate::browser_bridge::provider_bridge_log_path;
use crate::proxy_core::{current_timestamp, is_safe_account_id};
use crate::runtime::{
    build_embedded_config, serve_browser_provider, serve_deepseek, serve_hub, serve_kimi,
    serve_qwen, BrowserProviderKind, BrowserProviderServerConfig, DeepseekServiceConfig,
    HubServiceConfig, KimiServiceConfig, ProviderConfig,
};
use anyhow::{anyhow, Context, Result};
use futures_util::future::join_all;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    env, fs,
    future::Future,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::{Mutex, Notify};

use self::{
    qwen_accounts::{
        add_qwen_account as add_qwen_account_db, ensure_qwen_db,
        list_qwen_accounts as list_qwen_accounts_db, remove_qwen_account as remove_qwen_account_db,
        QwenAccountSummary,
    },
    runtime::{
        build_runtime_diagnostics, detect_browser_available, require_helper_dir, require_node_path,
        RuntimeDiagnostics,
    },
};

const HUB_PORT: u16 = 3100;
const QWEN_PORT: u16 = 3000;
const DEEPSEEK_PORT: u16 = 3001;
const KIMI_PORT: u16 = 3002;
const CHATGPT_PORT: u16 = 3003;
const GEMINI_PORT: u16 = 3004;
const MISTRAL_PORT: u16 = 3005;
const ZAI_PORT: u16 = 3006;
const META_PORT: u16 = 3007;
const DEFAULT_BROWSER: &str = "chromium";
const LOG_LIMIT: usize = 240;
const PROVIDER_READY_TIMEOUT: Duration = Duration::from_secs(30);
const PROVIDER_READY_POLL_INTERVAL: Duration = Duration::from_millis(100);
const MANUAL_LOGIN_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone, Default)]
struct ServiceRuntimeStatus {
    running: bool,
    started_at: Option<u64>,
    last_error: Option<String>,
}

#[derive(Clone)]
struct ControlState {
    workspace_root: PathBuf,
    app_data_dir: PathBuf,
    qwen_runtime_dir: PathBuf,
    runtime: RuntimeDiagnostics,
    startup_config: StartupConfig,
    client: reqwest::Client,
    hub_api_key: Option<String>,
    statuses: Arc<Mutex<HashMap<String, ServiceRuntimeStatus>>>,
    logs: Arc<Mutex<HashMap<String, VecDeque<String>>>>,
    open_provider_login_sessions: Arc<Mutex<HashSet<String>>>,
    open_qwen_account_login_sessions: Arc<Mutex<HashSet<String>>>,
    tasks: Arc<std::sync::Mutex<HashMap<String, tauri::async_runtime::JoinHandle<()>>>>,
    app_handle: Option<AppHandle>,
    dashboard_notify: Arc<Notify>,
}

#[derive(Debug, Deserialize)]
struct ProviderLoginRequest {
    provider: String,
    browser: Option<String>,
}

#[derive(Debug, Deserialize)]
struct QwenAccountLoginRequest {
    account_id: String,
    browser: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AddAccountRequest {
    email: String,
    password: String,
}

#[derive(Debug, Deserialize)]
struct WorkbenchRequest {
    model: String,
    prompt: String,
    web_search: bool,
    #[serde(default)]
    chatgpt_mode: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StartServicesRequest {
    services: Vec<String>,
}

#[derive(Debug, Serialize, Clone)]
struct StartupConfig {
    mode: String,
    services: Vec<String>,
}

#[derive(Debug, Serialize, Clone)]
struct HubConfigResponse {
    port: u16,
    base_url: String,
    openapi_url: String,
    api_key_enabled: bool,
}

#[derive(Debug, Serialize, Clone)]
struct HubOverview {
    running: bool,
    started_at: Option<u64>,
    health_status: String,
    model_count: usize,
    provider_statuses: Vec<Value>,
    detail: Option<Value>,
    #[serde(flatten)]
    config: HubConfigResponse,
}

#[derive(Debug, Serialize, Clone)]
struct ProviderOverview {
    name: String,
    running: bool,
    started_at: Option<u64>,
    base_url: String,
    health_status: String,
    login_state: String,
    model_count: usize,
    models: Vec<String>,
    model_modes: HashMap<String, String>,
    web_search_supported: bool,
    last_error: Option<String>,
}

#[derive(Debug, Serialize)]
struct DashboardOverview {
    generated_at: u64,
    runtime: RuntimeDiagnostics,
    startup_config: StartupConfig,
    hub: HubOverview,
    providers: Vec<ProviderOverview>,
    qwen_account_count: usize,
    open_provider_login_sessions: Vec<String>,
    open_qwen_account_login_sessions: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ProviderDetails {
    overview: ProviderOverview,
    detail: Option<Value>,
    logs: Vec<String>,
    qwen_accounts: Option<Vec<QwenAccountSummary>>,
}

#[derive(Debug, Serialize)]
struct ProviderLogs {
    provider: String,
    entries: Vec<String>,
}

include!("control_room_impl.rs");
include!("control_room_handlers.rs");
#[cfg(test)]
mod tests;
