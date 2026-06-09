mod runtime;

use anyhow::{anyhow, Context, Result};
use crate::runtime::{
    build_embedded_config,
    serve_browser_provider, BrowserProviderKind, BrowserProviderServerConfig,
    DeepseekServiceConfig, HubServiceConfig, KimiServiceConfig, ProviderConfig,
    serve_deepseek, serve_hub, serve_kimi, serve_qwen,
};
pub use crate::runtime::{browser_bridge, proxy_core};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager, State};
use tokio::sync::Mutex;

const HUB_PORT: u16 = 3100;
const QWEN_PORT: u16 = 3000;
const DEEPSEEK_PORT: u16 = 3001;
const KIMI_PORT: u16 = 3002;
const CHATGPT_PORT: u16 = 3003;
const GEMINI_PORT: u16 = 3004;
const DEFAULT_BROWSER: &str = "msedge";
const LOG_LIMIT: usize = 240;

#[derive(Clone)]
struct ServiceRuntimeStatus {
    running: bool,
    started_at: Option<u64>,
    last_error: Option<String>,
}

impl Default for ServiceRuntimeStatus {
    fn default() -> Self {
        Self {
            running: false,
            started_at: None,
            last_error: None,
        }
    }
}

#[derive(Clone)]
struct ControlState {
    workspace_root: PathBuf,
    app_data_dir: PathBuf,
    helper_dir: PathBuf,
    node_path: Option<PathBuf>,
    client: reqwest::Client,
    hub_api_key: Option<String>,
    statuses: Arc<Mutex<HashMap<String, ServiceRuntimeStatus>>>,
    logs: Arc<Mutex<HashMap<String, VecDeque<String>>>>,
    open_provider_login_sessions: Arc<Mutex<HashSet<String>>>,
    open_qwen_account_login_sessions: Arc<Mutex<HashSet<String>>>,
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
    web_search_supported: bool,
    last_error: Option<String>,
}

#[derive(Debug, Serialize)]
struct DashboardOverview {
    generated_at: u64,
    app_data_dir: String,
    helper_dir: String,
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

#[derive(Debug, Serialize)]
struct QwenAccountSummary {
    id: String,
    email: String,
    has_password: bool,
    created_at: Option<String>,
}

impl ControlState {
    fn new(app: &AppHandle) -> Result<Self> {
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .context("failed to resolve workspace root")?
            .to_path_buf();

        let app_data_dir = app
            .path()
            .app_data_dir()
            .context("failed to resolve app data directory")?;
        fs::create_dir_all(&app_data_dir)?;

        let helper_dir = resolve_helper_dir(app, &workspace_root)?;
        let node_path = resolve_node_path(app, &workspace_root);
        let hub_api_key = std::env::var("RUST_PROXY_HUB_API_KEY")
            .ok()
            .filter(|value| !value.trim().is_empty());

        let mut statuses = HashMap::new();
        let mut logs = HashMap::new();
        for name in service_names() {
            statuses.insert(name.to_owned(), ServiceRuntimeStatus::default());
            logs.insert(name.to_owned(), VecDeque::new());
        }

        Ok(Self {
            workspace_root,
            app_data_dir,
            helper_dir,
            node_path,
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(12))
                .build()?,
            hub_api_key,
            statuses: Arc::new(Mutex::new(statuses)),
            logs: Arc::new(Mutex::new(logs)),
            open_provider_login_sessions: Arc::new(Mutex::new(HashSet::new())),
            open_qwen_account_login_sessions: Arc::new(Mutex::new(HashSet::new())),
        })
    }

    async fn bootstrap(&self) -> Result<()> {
        fs::create_dir_all(self.providers_root())?;
        fs::create_dir_all(self.qwen_runtime_dir())?;
        fs::create_dir_all(self.deepseek_runtime_dir())?;
        fs::create_dir_all(self.kimi_runtime_dir())?;
        fs::create_dir_all(self.chatgpt_runtime_dir())?;
        fs::create_dir_all(self.gemini_runtime_dir())?;
        fs::create_dir_all(self.hub_runtime_dir())?;
        let _ = ensure_qwen_db(self)?;

        self.spawn_service("qwen", {
            let state = self.clone();
            async move {
                serve_qwen(
                    build_embedded_config(
                        state.qwen_runtime_dir(),
                        QWEN_PORT,
                        None,
                        DEFAULT_BROWSER.to_owned(),
                        true,
                    ),
                    state.helper_dir.clone(),
                    state.node_path.clone(),
                )
                .await
            }
        });

        self.spawn_service("deepseek", {
            let state = self.clone();
            async move {
                serve_deepseek(DeepseekServiceConfig {
                    host: "127.0.0.1".to_owned(),
                    port: DEEPSEEK_PORT,
                    api_key: None,
                    headless: true,
                    browser: DEFAULT_BROWSER.to_owned(),
                    runtime_dir: state.deepseek_runtime_dir(),
                    helper_dir: state.helper_dir.clone(),
                    node_path: state.node_path.clone(),
                })
                .await
            }
        });

        self.spawn_service("kimi", {
            let state = self.clone();
            async move {
                serve_kimi(KimiServiceConfig {
                    host: "127.0.0.1".to_owned(),
                    port: KIMI_PORT,
                    api_key: None,
                    headless: true,
                    browser: DEFAULT_BROWSER.to_owned(),
                    runtime_dir: state.kimi_runtime_dir(),
                    helper_dir: state.helper_dir.clone(),
                    node_path: state.node_path.clone(),
                })
                .await
            }
        });

        self.spawn_service("chatgpt", {
            let state = self.clone();
            async move {
                serve_browser_provider(BrowserProviderServerConfig {
                    kind: BrowserProviderKind::Chatgpt,
                    host: "127.0.0.1".to_owned(),
                    port: CHATGPT_PORT,
                    api_key: None,
                    headless: true,
                    browser: DEFAULT_BROWSER.to_owned(),
                    runtime_dir: state.chatgpt_runtime_dir(),
                    helper_dir: state.helper_dir.clone(),
                    node_path: state.node_path.clone(),
                })
                .await
            }
        });

        self.spawn_service("gemini", {
            let state = self.clone();
            async move {
                serve_browser_provider(BrowserProviderServerConfig {
                    kind: BrowserProviderKind::Gemini,
                    host: "127.0.0.1".to_owned(),
                    port: GEMINI_PORT,
                    api_key: None,
                    headless: true,
                    browser: DEFAULT_BROWSER.to_owned(),
                    runtime_dir: state.gemini_runtime_dir(),
                    helper_dir: state.helper_dir.clone(),
                    node_path: state.node_path.clone(),
                })
                .await
            }
        });

        self.spawn_service("hub", {
            let hub_api_key = self.hub_api_key.clone();
            async move {
                serve_hub(HubServiceConfig {
                    host: "127.0.0.1".to_owned(),
                    port: HUB_PORT,
                    api_key: hub_api_key,
                    qwen: ProviderConfig::new(provider_base_url("qwen"), None::<String>),
                    deepseek: ProviderConfig::new(provider_base_url("deepseek"), None::<String>),
                    kimi: ProviderConfig::new(provider_base_url("kimi"), None::<String>),
                    chatgpt: ProviderConfig::new(provider_base_url("chatgpt"), None::<String>),
                    gemini: ProviderConfig::new(provider_base_url("gemini"), None::<String>),
                })
                .await
            }
        });

        Ok(())
    }

    fn spawn_service<F>(&self, name: &'static str, future: F)
    where
        F: Future<Output = Result<()>> + Send + 'static,
    {
        let state = self.clone();
        let service_name = name.to_owned();
        tauri::async_runtime::spawn(async move {
            state
                .mark_service_started(&service_name, format!("starting embedded {service_name}"))
                .await;

            match future.await {
                Ok(()) => {
                    state
                        .mark_service_stopped(
                            &service_name,
                            Some(format!("{service_name} server exited")),
                            None,
                        )
                        .await;
                }
                Err(err) => {
                    state
                        .mark_service_stopped(
                            &service_name,
                            Some(format!("{service_name} server failed: {err}")),
                            Some(err.to_string()),
                        )
                        .await;
                }
            }
        });
    }

    async fn mark_service_started(&self, name: &str, message: String) {
        {
            let mut guard = self.statuses.lock().await;
            let entry = guard.entry(name.to_owned()).or_default();
            entry.running = true;
            entry.started_at = Some(now_ts());
            entry.last_error = None;
        }
        self.push_log(name, message).await;
    }

    async fn mark_service_stopped(
        &self,
        name: &str,
        log_message: Option<String>,
        last_error: Option<String>,
    ) {
        {
            let mut guard = self.statuses.lock().await;
            let entry = guard.entry(name.to_owned()).or_default();
            entry.running = false;
            if last_error.is_some() {
                entry.last_error = last_error;
            }
        }
        if let Some(message) = log_message {
            self.push_log(name, message).await;
        }
    }

    async fn push_log(&self, name: &str, message: impl Into<String>) {
        let mut guard = self.logs.lock().await;
        let entry = guard.entry(name.to_owned()).or_default();
        entry.push_back(format!("[{}] {}", now_ts(), message.into()));
        while entry.len() > LOG_LIMIT {
            entry.pop_front();
        }
    }

    async fn service_status(&self, name: &str) -> ServiceRuntimeStatus {
        self.statuses
            .lock()
            .await
            .get(name)
            .cloned()
            .unwrap_or_default()
    }

    async fn service_logs(&self, name: &str) -> Vec<String> {
        self.logs
            .lock()
            .await
            .get(name)
            .map(|items| items.iter().cloned().collect())
            .unwrap_or_default()
    }

    fn hub_config(&self) -> HubConfigResponse {
        HubConfigResponse {
            port: HUB_PORT,
            base_url: hub_base_url(),
            openapi_url: format!("{}/openapi.json", hub_base_url()),
            api_key_enabled: self.hub_api_key.is_some(),
        }
    }

    fn providers_root(&self) -> PathBuf {
        self.app_data_dir.join("providers")
    }

    fn qwen_runtime_dir(&self) -> PathBuf {
        self.providers_root().join("qwen")
    }

    fn deepseek_runtime_dir(&self) -> PathBuf {
        self.providers_root().join("deepseek")
    }

    fn kimi_runtime_dir(&self) -> PathBuf {
        self.providers_root().join("kimi")
    }

    fn chatgpt_runtime_dir(&self) -> PathBuf {
        self.providers_root().join("chatgpt")
    }

    fn gemini_runtime_dir(&self) -> PathBuf {
        self.providers_root().join("gemini")
    }

    fn hub_runtime_dir(&self) -> PathBuf {
        self.providers_root().join("hub")
    }

    async fn call_json_get(
        &self,
        base_url: &str,
        path: &str,
        api_key: Option<&str>,
    ) -> Result<(u16, Value)> {
        let url = format!("{base_url}{path}");
        let mut request = self.client.get(url);
        if let Some(api_key) = api_key.filter(|value| !value.trim().is_empty()) {
            request = request.header(AUTHORIZATION, format!("Bearer {api_key}"));
        }
        let response = request.send().await?;
        let status = response.status().as_u16();
        let value = response.json::<Value>().await.unwrap_or_else(|_| json!({}));
        Ok((status, value))
    }

    async fn call_json_post(
        &self,
        base_url: &str,
        path: &str,
        api_key: Option<&str>,
        payload: &Value,
    ) -> Result<(u16, Value)> {
        let url = format!("{base_url}{path}");
        let mut request = self
            .client
            .post(url)
            .header(CONTENT_TYPE, "application/json")
            .json(payload);
        if let Some(api_key) = api_key.filter(|value| !value.trim().is_empty()) {
            request = request.header(AUTHORIZATION, format!("Bearer {api_key}"));
        }
        let response = request.send().await?;
        let status = response.status().as_u16();
        let value = response.json::<Value>().await.unwrap_or_else(|_| json!({}));
        Ok((status, value))
    }

    async fn hub_overview(&self) -> HubOverview {
        let status = self.service_status("hub").await;
        let health = self
            .call_json_get(&hub_base_url(), "/health", self.hub_api_key.as_deref())
            .await
            .ok();
        let models = self
            .call_json_get(&hub_base_url(), "/v1/models", self.hub_api_key.as_deref())
            .await
            .ok();

        let detail = health.as_ref().map(|(_, payload)| payload.clone());
        let health_status = health
            .as_ref()
            .and_then(|(_, payload)| payload.get("status"))
            .and_then(Value::as_str)
            .unwrap_or(if status.running { "starting" } else { "offline" })
            .to_owned();
        let provider_statuses = detail
            .as_ref()
            .and_then(|payload| payload.get("providers"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let model_count = models
            .as_ref()
            .and_then(|(_, payload)| payload.get("data"))
            .and_then(Value::as_array)
            .map(|items| items.len())
            .unwrap_or_default();

        HubOverview {
            running: status.running,
            started_at: status.started_at,
            health_status,
            model_count,
            provider_statuses,
            detail,
            config: self.hub_config(),
        }
    }

    async fn provider_overview(&self, name: &'static str) -> ProviderOverview {
        let status = self.service_status(name).await;
        let base_url = provider_base_url(name);
        let health = self.call_json_get(&base_url, "/health", None).await.ok();
        let open_provider_sessions = self.open_provider_login_sessions.lock().await.clone();
        let open_qwen_account_sessions =
            self.open_qwen_account_login_sessions.lock().await.clone();
        let login_open = open_provider_sessions.contains(name);

        let models = if login_open {
            None
        } else {
            self.call_json_get(&base_url, "/v1/models", None).await.ok()
        };

        let model_ids = models
            .as_ref()
            .and_then(|(_, payload)| payload.get("data"))
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.get("id").and_then(Value::as_str).map(str::to_owned))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let model_count = model_ids.len();

        let health_status = health
            .as_ref()
            .and_then(|(_, payload)| payload.get("status"))
            .and_then(Value::as_str)
            .unwrap_or(if status.running { "starting" } else { "offline" })
            .to_owned();

        let login_state = if login_open {
            "login_open".to_owned()
        } else if name == "chatgpt" || name == "gemini" {
            if model_count > 0 {
                "authenticated".to_owned()
            } else if status.running {
                "login_required".to_owned()
            } else {
                "offline".to_owned()
            }
        } else if !open_qwen_account_sessions.is_empty() && name == "qwen" {
            "account_login_open".to_owned()
        } else if status.running {
            "ready".to_owned()
        } else {
            "offline".to_owned()
        };

        ProviderOverview {
            name: name.to_owned(),
            running: status.running,
            started_at: status.started_at,
            base_url,
            health_status,
            login_state,
            model_count,
            models: model_ids,
            web_search_supported: provider_web_search_supported(name),
            last_error: status.last_error,
        }
    }
}

#[tauri::command]
async fn dashboard_overview(state: State<'_, ControlState>) -> Result<DashboardOverview, String> {
    let providers = {
        let mut items = Vec::new();
        for provider in provider_names() {
            items.push(state.provider_overview(provider).await);
        }
        items
    };

    Ok(DashboardOverview {
        generated_at: now_ts(),
        app_data_dir: state.app_data_dir.display().to_string(),
        helper_dir: state.helper_dir.display().to_string(),
        hub: state.hub_overview().await,
        providers,
        qwen_account_count: list_qwen_accounts_internal(&state)
            .map(|items| items.len())
            .map_err(|err| err.to_string())?,
        open_provider_login_sessions: state
            .open_provider_login_sessions
            .lock()
            .await
            .iter()
            .cloned()
            .collect(),
        open_qwen_account_login_sessions: state
            .open_qwen_account_login_sessions
            .lock()
            .await
            .iter()
            .cloned()
            .collect(),
    })
}

#[tauri::command]
async fn provider_details(
    state: State<'_, ControlState>,
    provider: String,
) -> Result<ProviderDetails, String> {
    let provider = normalize_provider(&provider).map_err(|err| err.to_string())?;
    let overview = state.provider_overview(provider).await;
    let detail = state
        .call_json_get(&provider_base_url(provider), "/health", None)
        .await
        .ok()
        .map(|(_, payload)| payload);
    let logs = state.service_logs(provider).await;
    let qwen_accounts = if provider == "qwen" {
        Some(list_qwen_accounts_internal(&state).map_err(|err| err.to_string())?)
    } else {
        None
    };

    Ok(ProviderDetails {
        overview,
        detail,
        logs,
        qwen_accounts,
    })
}

#[tauri::command]
async fn provider_logs(
    state: State<'_, ControlState>,
    provider: String,
) -> Result<ProviderLogs, String> {
    let provider = normalize_service_name(&provider).map_err(|err| err.to_string())?;
    Ok(ProviderLogs {
        provider: provider.to_owned(),
        entries: state.service_logs(provider).await,
    })
}

#[tauri::command]
async fn hub_config(state: State<'_, ControlState>) -> Result<HubConfigResponse, String> {
    Ok(state.hub_config())
}

#[tauri::command]
async fn run_workbench_request(
    state: State<'_, ControlState>,
    request: WorkbenchRequest,
) -> Result<Value, String> {
    let model = request.model.trim();
    let prompt = request.prompt.trim();
    if model.is_empty() {
        return Err("model is required".to_owned());
    }
    if prompt.is_empty() {
        return Err("prompt is required".to_owned());
    }

    let payload = json!({
        "model": model,
        "messages": [{ "role": "user", "content": prompt }],
        "stream": false,
        "web_search": request.web_search,
    });

    let (status, value) = state
        .call_json_post(
            &hub_base_url(),
            "/v1/chat/completions",
            state.hub_api_key.as_deref(),
            &payload,
        )
        .await
        .map_err(|err| err.to_string())?;

    if status >= 400 {
        Err(value
            .get("error")
            .and_then(|item| item.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("hub request failed")
            .to_owned())
    } else {
        Ok(value)
    }
}

#[tauri::command]
async fn list_qwen_accounts(state: State<'_, ControlState>) -> Result<Vec<QwenAccountSummary>, String> {
    list_qwen_accounts_internal(&state).map_err(|err| err.to_string())
}

#[tauri::command]
async fn add_qwen_account(
    state: State<'_, ControlState>,
    request: AddAccountRequest,
) -> Result<Vec<QwenAccountSummary>, String> {
    add_qwen_account_internal(&state, &request.email, &request.password)
        .and_then(|_| list_qwen_accounts_internal(&state))
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn remove_qwen_account(
    state: State<'_, ControlState>,
    account_id: String,
) -> Result<Vec<QwenAccountSummary>, String> {
    remove_qwen_account_internal(&state, &account_id)
        .and_then(|_| list_qwen_accounts_internal(&state))
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn start_provider_login_session(
    state: State<'_, ControlState>,
    request: ProviderLoginRequest,
) -> Result<Vec<String>, String> {
    let provider = normalize_provider(&request.provider).map_err(|err| err.to_string())?;
    let payload = json!({
        "browser": request.browser.unwrap_or_else(|| DEFAULT_BROWSER.to_owned()),
    });

    let (_, response) = state
        .call_json_post(&provider_base_url(provider), "/admin/manual_login", None, &payload)
        .await
        .map_err(|err| err.to_string())?;

    if let Some(message) = response
        .get("error")
        .and_then(|item| item.get("message"))
        .and_then(Value::as_str)
    {
        return Err(message.to_owned());
    }

    let mut guard = state.open_provider_login_sessions.lock().await;
    guard.insert(provider.to_owned());
    Ok(guard.iter().cloned().collect())
}

#[tauri::command]
async fn stop_provider_login_session(
    state: State<'_, ControlState>,
    provider: String,
) -> Result<Vec<String>, String> {
    let provider = normalize_provider(&provider).map_err(|err| err.to_string())?;
    let mut guard = state.open_provider_login_sessions.lock().await;
    guard.remove(provider);
    Ok(guard.iter().cloned().collect())
}

#[tauri::command]
async fn start_qwen_account_login_session(
    state: State<'_, ControlState>,
    request: QwenAccountLoginRequest,
) -> Result<Vec<String>, String> {
    if request.account_id.trim().is_empty() {
        return Err("account_id is required".to_owned());
    }

    let payload = json!({
        "browser": request.browser.unwrap_or_else(|| DEFAULT_BROWSER.to_owned()),
        "account_id": request.account_id,
    });

    let (_, response) = state
        .call_json_post(&provider_base_url("qwen"), "/admin/manual_login", None, &payload)
        .await
        .map_err(|err| err.to_string())?;

    if let Some(message) = response
        .get("error")
        .and_then(|item| item.get("message"))
        .and_then(Value::as_str)
    {
        return Err(message.to_owned());
    }

    let mut guard = state.open_qwen_account_login_sessions.lock().await;
    guard.insert(payload["account_id"].as_str().unwrap_or_default().to_owned());
    Ok(guard.iter().cloned().collect())
}

#[tauri::command]
async fn stop_qwen_account_login_session(
    state: State<'_, ControlState>,
    account_id: String,
) -> Result<Vec<String>, String> {
    if account_id.trim().is_empty() {
        return Err("account_id is required".to_owned());
    }

    let _ = state
        .call_json_post(
            &provider_base_url("qwen"),
            "/admin/close_login",
            None,
            &json!({ "account_id": account_id }),
        )
        .await;

    let mut guard = state.open_qwen_account_login_sessions.lock().await;
    guard.remove(&account_id);
    Ok(guard.iter().cloned().collect())
}

fn ensure_qwen_db(state: &ControlState) -> Result<PathBuf> {
    let runtime_dir = state.qwen_runtime_dir().join("data");
    fs::create_dir_all(&runtime_dir)?;
    let db_path = runtime_dir.join("qwenproxy.db");

    if !db_path.exists() {
        for legacy_db in [
            state
                .workspace_root
                .join("runtime")
                .join("qwen")
                .join("data")
                .join("qwenproxy.db"),
            state
                .workspace_root
                .join("proxy")
                .join("qwenproxy")
                .join("data")
                .join("qwenproxy.db"),
        ] {
            if legacy_db.exists() {
                fs::copy(&legacy_db, &db_path)?;
                break;
            }
        }
    }

    let connection = Connection::open(&db_path)?;
    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS accounts (
          id TEXT PRIMARY KEY,
          email TEXT UNIQUE NOT NULL,
          password TEXT NOT NULL DEFAULT '',
          created_at TEXT NOT NULL DEFAULT (datetime('now')),
          updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_accounts_email ON accounts(email);
        "#,
    )?;

    let legacy_json = state
        .workspace_root
        .join("proxy")
        .join("qwenproxy")
        .join("accounts.json");
    if legacy_json.exists() {
        migrate_legacy_accounts_json(&connection, &legacy_json)?;
    }

    Ok(db_path)
}

fn migrate_legacy_accounts_json(connection: &Connection, json_path: &Path) -> Result<()> {
    #[derive(Deserialize)]
    struct LegacyAccount {
        id: Option<String>,
        email: String,
        #[serde(default)]
        password: String,
    }

    let raw = fs::read_to_string(json_path)?;
    let parsed: Vec<LegacyAccount> = serde_json::from_str(&raw).unwrap_or_default();
    let mut statement =
        connection.prepare("INSERT OR IGNORE INTO accounts (id, email, password) VALUES (?1, ?2, ?3)")?;
    for account in parsed {
        if account.email.trim().is_empty() {
            continue;
        }
        statement.execute(params![
            account.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            account.email.trim(),
            account.password
        ])?;
    }
    let backup = json_path.with_extension("json.bak");
    let _ = fs::rename(json_path, backup);
    Ok(())
}

fn list_qwen_accounts_internal(state: &ControlState) -> Result<Vec<QwenAccountSummary>> {
    let db_path = ensure_qwen_db(state)?;
    let connection = Connection::open(db_path)?;
    let mut statement = connection.prepare(
        "SELECT id, email, password, created_at FROM accounts ORDER BY created_at ASC",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(QwenAccountSummary {
            id: row.get(0)?,
            email: row.get(1)?,
            has_password: !row.get::<_, String>(2)?.is_empty(),
            created_at: row.get(3).ok(),
        })
    })?;

    let mut accounts = Vec::new();
    for row in rows {
        accounts.push(row?);
    }
    Ok(accounts)
}

fn add_qwen_account_internal(state: &ControlState, email: &str, password: &str) -> Result<()> {
    let email = email.trim();
    if email.is_empty() {
        return Err(anyhow!("email is required"));
    }
    let db_path = ensure_qwen_db(state)?;
    let connection = Connection::open(db_path)?;
    let existing: Option<String> = connection
        .query_row(
            "SELECT id FROM accounts WHERE email = ?1",
            params![email],
            |row| row.get(0),
        )
        .optional()?;
    if existing.is_some() {
        return Err(anyhow!("account with email {email} already exists"));
    }
    connection.execute(
        "INSERT INTO accounts (id, email, password) VALUES (?1, ?2, ?3)",
        params![uuid::Uuid::new_v4().to_string(), email, password],
    )?;
    Ok(())
}

fn remove_qwen_account_internal(state: &ControlState, account_id: &str) -> Result<()> {
    let db_path = ensure_qwen_db(state)?;
    let connection = Connection::open(db_path)?;
    connection.execute("DELETE FROM accounts WHERE id = ?1", params![account_id])?;
    Ok(())
}

fn normalize_provider(value: &str) -> Result<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "qwen" => Ok("qwen"),
        "deepseek" => Ok("deepseek"),
        "kimi" => Ok("kimi"),
        "chatgpt" => Ok("chatgpt"),
        "gemini" => Ok("gemini"),
        other => Err(anyhow!("unsupported provider: {other}")),
    }
}

fn normalize_service_name(value: &str) -> Result<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "hub" => Ok("hub"),
        other => normalize_provider(other),
    }
}

fn service_names() -> [&'static str; 6] {
    ["hub", "qwen", "deepseek", "kimi", "chatgpt", "gemini"]
}

fn provider_names() -> [&'static str; 5] {
    ["qwen", "deepseek", "kimi", "chatgpt", "gemini"]
}

fn provider_base_url(provider: &str) -> String {
    let port = match provider {
        "qwen" => QWEN_PORT,
        "deepseek" => DEEPSEEK_PORT,
        "kimi" => KIMI_PORT,
        "chatgpt" => CHATGPT_PORT,
        "gemini" => GEMINI_PORT,
        _ => HUB_PORT,
    };
    format!("http://127.0.0.1:{port}")
}

fn hub_base_url() -> String {
    format!("http://127.0.0.1:{HUB_PORT}")
}

fn provider_web_search_supported(provider: &str) -> bool {
    matches!(provider, "qwen" | "deepseek")
}

fn resolve_helper_dir(app: &AppHandle, workspace_root: &Path) -> Result<PathBuf> {
    let resource_dir = app.path().resource_dir().ok();
    if let Some(resource_dir) = resource_dir {
        let bundled = resource_dir.join("playwright-bridge");
        if bundled.exists() {
            return Ok(bundled);
        }
    }

    let dev = workspace_root
        .join("src-tauri")
        .join("resources")
        .join("playwright-bridge");
    if dev.exists() {
        return Ok(dev);
    }

    Err(anyhow!("failed to resolve playwright-bridge helper directory"))
}

fn resolve_node_path(app: &AppHandle, workspace_root: &Path) -> Option<PathBuf> {
    let resource_dir = app.path().resource_dir().ok();
    let candidates = resolve_node_candidates_for_tests(resource_dir.as_deref(), workspace_root);
    candidates.into_iter().find(|path| path.exists())
}

fn resolve_node_candidates_for_tests(
    resource_dir: Option<&Path>,
    workspace_root: &Path,
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(resource_dir) = resource_dir {
        candidates.push(resource_dir.join("node").join("node.exe"));
        candidates.push(resource_dir.join("node.exe"));
    }
    candidates.push(
        workspace_root
            .join("src-tauri")
            .join("resources")
            .join("node")
            .join("node.exe"),
    );
    candidates.push(workspace_root.join("src-tauri").join("resources").join("node.exe"));
    candidates.push(workspace_root.join("node.exe"));
    candidates
}

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let state = ControlState::new(&app.handle())?;
            let bootstrap = state.clone();

            #[cfg(debug_assertions)]
            {
                if let Some(window) = tauri::Manager::get_webview_window(app, "main") {
                    window.open_devtools();
                }
            }

            app.manage(state);

            tauri::async_runtime::spawn(async move {
                if let Err(err) = bootstrap.bootstrap().await {
                    bootstrap
                        .push_log("hub", format!("bootstrap failed: {err}"))
                        .await;
                }
            });

            Ok(())
        })
        .plugin(tauri_plugin_prevent_default::init())
        .invoke_handler(tauri::generate_handler![
            dashboard_overview,
            provider_details,
            provider_logs,
            hub_config,
            run_workbench_request,
            list_qwen_accounts,
            add_qwen_account,
            remove_qwen_account,
            start_provider_login_session,
            stop_provider_login_session,
            start_qwen_account_login_session,
            stop_qwen_account_login_session
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::{provider_base_url, resolve_node_candidates_for_tests};
    use std::path::PathBuf;

    #[test]
    fn provider_base_urls_keep_embedded_ports() {
        assert_eq!(provider_base_url("qwen"), "http://127.0.0.1:3000");
        assert_eq!(provider_base_url("deepseek"), "http://127.0.0.1:3001");
        assert_eq!(provider_base_url("kimi"), "http://127.0.0.1:3002");
        assert_eq!(provider_base_url("chatgpt"), "http://127.0.0.1:3003");
        assert_eq!(provider_base_url("gemini"), "http://127.0.0.1:3004");
    }

    #[test]
    fn node_candidates_prefer_src_tauri_resources_before_repo_root() {
        let root = PathBuf::from("G:/repo");
        let candidates = resolve_node_candidates_for_tests(None, &root);
        assert_eq!(
            candidates,
            vec![
                root.join("src-tauri").join("resources").join("node").join("node.exe"),
                root.join("src-tauri").join("resources").join("node.exe"),
                root.join("node.exe"),
            ]
        );
    }
}
