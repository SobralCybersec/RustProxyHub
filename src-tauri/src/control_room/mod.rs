mod qwen_accounts;
mod runtime;

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
const DEFAULT_BROWSER: &str = "msedge";
const LOG_LIMIT: usize = 240;

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

        let runtime = build_runtime_diagnostics(
            app.path().resource_dir().ok().as_deref(),
            &workspace_root,
            &app_data_dir,
            detect_browser_available(),
        );
        let startup_config = startup_config_from_env();
        let hub_api_key = std::env::var("RUST_PROXY_HUB_API_KEY")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let qwen_runtime_dir = app_data_dir.join("providers").join("qwen");

        let mut statuses = HashMap::new();
        let mut logs = HashMap::new();
        for name in service_names() {
            statuses.insert(name.to_owned(), ServiceRuntimeStatus::default());
            logs.insert(name.to_owned(), VecDeque::new());
        }

        Ok(Self {
            workspace_root,
            app_data_dir,
            qwen_runtime_dir,
            runtime,
            startup_config,
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .tcp_nodelay(true)
                .pool_idle_timeout(Duration::from_secs(90))
                .pool_max_idle_per_host(16)
                .timeout(Duration::from_secs(12))
                .build()?,
            hub_api_key,
            statuses: Arc::new(Mutex::new(statuses)),
            logs: Arc::new(Mutex::new(logs)),
            open_provider_login_sessions: Arc::new(Mutex::new(HashSet::new())),
            open_qwen_account_login_sessions: Arc::new(Mutex::new(HashSet::new())),
            tasks: Arc::new(std::sync::Mutex::new(HashMap::new())),
            app_handle: Some(app.clone()),
            dashboard_notify: Arc::new(Notify::new()),
        })
    }

    fn emit_dashboard_update(&self) {
        // Poke the coalescer; it will debounce bursts into one build+emit.
        self.dashboard_notify.notify_one();
    }

    fn start_dashboard_coalescer(&self) {
        let state = self.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                state.dashboard_notify.notified().await;
                // ponytail: 250ms coalesce window; tune if UI feels laggy
                tokio::time::sleep(Duration::from_millis(250)).await;
                // Any notify_one() calls that arrived during the sleep are
                // already collapsed into one permit — we consume them by doing
                // the build now; the loop will block on the next notified().
                if let (Some(handle), Ok(overview)) =
                    (&state.app_handle, state.build_dashboard_overview().await)
                {
                    let _ = handle.emit("dashboard:update", &overview);
                }
            }
        });
    }

    async fn bootstrap(&self) -> Result<()> {
        if !self.runtime.single_runner_ready {
            self.mark_runtime_blocked().await;
            return Ok(());
        }

        fs::create_dir_all(self.providers_root())?;
        fs::create_dir_all(self.qwen_runtime_dir())?;
        let _ = ensure_qwen_db(&self.qwen_runtime_dir(), &self.workspace_root)?;
        let selected = self.selected_startup_services();

        for name in &selected {
            self.ensure_service_dir(name);
            self.spawn_service_by_name(name)
                .map_err(|err| anyhow!("failed to start {name}: {err}"))?;
        }

        Ok(())
    }

    fn selected_startup_services(&self) -> Vec<&'static str> {
        let configured = self
            .startup_config
            .services
            .iter()
            .filter_map(|name| normalize_service_name(name).ok())
            .collect::<HashSet<_>>();

        service_names()
            .into_iter()
            .filter(|name| configured.contains(name))
            .collect()
    }

    fn ensure_service_dir(&self, name: &str) {
        let _ = match name {
            "qwen" => fs::create_dir_all(self.qwen_runtime_dir()),
            "deepseek" => fs::create_dir_all(self.deepseek_runtime_dir()),
            "kimi" => fs::create_dir_all(self.kimi_runtime_dir()),
            "chatgpt" => fs::create_dir_all(self.chatgpt_runtime_dir()),
            "gemini" => fs::create_dir_all(self.gemini_runtime_dir()),
            "mistral" => fs::create_dir_all(self.mistral_runtime_dir()),
            "zai" => fs::create_dir_all(self.zai_runtime_dir()),
            "meta" => fs::create_dir_all(self.meta_runtime_dir()),
            "hub" => fs::create_dir_all(self.hub_runtime_dir()),
            _ => Ok(()),
        };
    }

    fn spawn_service_by_name(&self, name: &str) -> Result<()> {
        let helper_dir = require_helper_dir(&self.runtime)?;
        let node_path = Some(require_node_path(&self.runtime)?);
        // Gate every embedded provider behind the same key the hub uses, so a local
        // process can't bypass the hub by hitting 127.0.0.1:3000-3006 directly.
        let provider_api_key = self.hub_api_key.clone();

        match name {
            "qwen" => {
                let runtime_dir = self.qwen_runtime_dir();
                let helper_dir = helper_dir.clone();
                let node_path = node_path.clone();
                self.spawn_service("qwen".to_owned(), async move {
                    serve_qwen(
                        build_embedded_config(
                            runtime_dir,
                            QWEN_PORT,
                            provider_api_key,
                            DEFAULT_BROWSER.to_owned(),
                            true,
                        ),
                        helper_dir,
                        node_path,
                    )
                    .await
                });
            }
            "deepseek" => {
                let runtime_dir = self.deepseek_runtime_dir();
                let helper_dir = helper_dir.clone();
                let node_path = node_path.clone();
                let api_key = provider_api_key.clone();
                self.spawn_service("deepseek".to_owned(), async move {
                    serve_deepseek(DeepseekServiceConfig {
                        host: "127.0.0.1".to_owned(),
                        port: DEEPSEEK_PORT,
                        api_key,
                        headless: true,
                        browser: DEFAULT_BROWSER.to_owned(),
                        runtime_dir,
                        helper_dir,
                        node_path,
                    })
                    .await
                });
            }
            "kimi" => {
                let runtime_dir = self.kimi_runtime_dir();
                let helper_dir = helper_dir.clone();
                let node_path = node_path.clone();
                let api_key = provider_api_key.clone();
                self.spawn_service("kimi".to_owned(), async move {
                    serve_kimi(KimiServiceConfig {
                        host: "127.0.0.1".to_owned(),
                        port: KIMI_PORT,
                        api_key,
                        headless: true,
                        browser: DEFAULT_BROWSER.to_owned(),
                        runtime_dir,
                        helper_dir,
                        node_path,
                    })
                    .await
                });
            }
            "chatgpt" => {
                let runtime_dir = self.chatgpt_runtime_dir();
                let helper_dir = helper_dir.clone();
                let node_path = node_path.clone();
                let api_key = provider_api_key.clone();
                self.spawn_service("chatgpt".to_owned(), async move {
                    serve_browser_provider(BrowserProviderServerConfig {
                        kind: BrowserProviderKind::Chatgpt,
                        host: "127.0.0.1".to_owned(),
                        port: CHATGPT_PORT,
                        api_key,
                        headless: true,
                        browser: DEFAULT_BROWSER.to_owned(),
                        runtime_dir,
                        helper_dir,
                        node_path,
                    })
                    .await
                });
            }
            "gemini" => {
                let runtime_dir = self.gemini_runtime_dir();
                let helper_dir = helper_dir.clone();
                let node_path = node_path.clone();
                let api_key = provider_api_key.clone();
                self.spawn_service("gemini".to_owned(), async move {
                    serve_browser_provider(BrowserProviderServerConfig {
                        kind: BrowserProviderKind::Gemini,
                        host: "127.0.0.1".to_owned(),
                        port: GEMINI_PORT,
                        api_key,
                        headless: true,
                        browser: DEFAULT_BROWSER.to_owned(),
                        runtime_dir,
                        helper_dir,
                        node_path,
                    })
                    .await
                });
            }
            "mistral" => {
                let runtime_dir = self.mistral_runtime_dir();
                let helper_dir = helper_dir.clone();
                let node_path = node_path.clone();
                let api_key = provider_api_key.clone();
                self.spawn_service("mistral".to_owned(), async move {
                    serve_browser_provider(BrowserProviderServerConfig {
                        kind: BrowserProviderKind::Mistral,
                        host: "127.0.0.1".to_owned(),
                        port: MISTRAL_PORT,
                        api_key,
                        headless: true,
                        browser: DEFAULT_BROWSER.to_owned(),
                        runtime_dir,
                        helper_dir,
                        node_path,
                    })
                    .await
                });
            }
            "zai" => {
                let runtime_dir = self.zai_runtime_dir();
                let helper_dir = helper_dir.clone();
                let node_path = node_path.clone();
                let api_key = provider_api_key.clone();
                self.spawn_service("zai".to_owned(), async move {
                    serve_browser_provider(BrowserProviderServerConfig {
                        kind: BrowserProviderKind::Zai,
                        host: "127.0.0.1".to_owned(),
                        port: ZAI_PORT,
                        api_key,
                        headless: true,
                        browser: DEFAULT_BROWSER.to_owned(),
                        runtime_dir,
                        helper_dir,
                        node_path,
                    })
                    .await
                });
            }
            "meta" => {
                let runtime_dir = self.meta_runtime_dir();
                let helper_dir = helper_dir.clone();
                let node_path = node_path.clone();
                let api_key = provider_api_key.clone();
                self.spawn_service("meta".to_owned(), async move {
                    serve_browser_provider(BrowserProviderServerConfig {
                        kind: BrowserProviderKind::Meta,
                        host: "127.0.0.1".to_owned(),
                        port: META_PORT,
                        api_key,
                        headless: true,
                        browser: DEFAULT_BROWSER.to_owned(),
                        runtime_dir,
                        helper_dir,
                        node_path,
                    })
                    .await
                });
            }
            "hub" => {
                let hub_api_key = self.hub_api_key.clone();
                self.spawn_service("hub".to_owned(), async move {
                    serve_hub(HubServiceConfig {
                        host: "127.0.0.1".to_owned(),
                        port: HUB_PORT,
                        api_key: hub_api_key.clone(),
                        qwen: ProviderConfig::new(provider_base_url("qwen"), hub_api_key.clone()),
                        deepseek: ProviderConfig::new(
                            provider_base_url("deepseek"),
                            hub_api_key.clone(),
                        ),
                        kimi: ProviderConfig::new(provider_base_url("kimi"), hub_api_key.clone()),
                        chatgpt: ProviderConfig::new(
                            provider_base_url("chatgpt"),
                            hub_api_key.clone(),
                        ),
                        gemini: ProviderConfig::new(
                            provider_base_url("gemini"),
                            hub_api_key.clone(),
                        ),
                        mistral: ProviderConfig::new(
                            provider_base_url("mistral"),
                            hub_api_key.clone(),
                        ),
                        zai: ProviderConfig::new(provider_base_url("zai"), hub_api_key.clone()),
                        meta: ProviderConfig::new(provider_base_url("meta"), hub_api_key),
                    })
                    .await
                });
            }
            _ => return Err(anyhow!("unsupported service: {name}")),
        }

        Ok(())
    }

    async fn start_services(&self, requested: Vec<String>) -> Result<Vec<String>> {
        if !self.runtime.single_runner_ready {
            return Err(anyhow!("runtime preflight is blocking startup"));
        }

        let mut started = Vec::new();
        for raw in requested {
            let name = normalize_service_name(&raw)?;
            self.ensure_service_dir(name);
            self.spawn_service_by_name(name)
                .map_err(|err| anyhow!("failed to start {name}: {err}"))?;
            started.push(name.to_owned());
        }

        Ok(started)
    }

    async fn mark_runtime_blocked(&self) {
        let summary = self.runtime.issues.join(" | ");
        {
            let mut guard = self.statuses.lock().await;
            for name in service_names() {
                let entry = guard.entry(name.to_owned()).or_default();
                entry.running = false;
                entry.last_error = Some(summary.clone());
            }
        }

        for issue in &self.runtime.issues {
            self.push_log("hub", format!("runtime preflight blocked startup: {issue}"))
                .await;
        }
    }

    fn spawn_service<F>(&self, name: String, future: F)
    where
        F: Future<Output = Result<()>> + Send + 'static,
    {
        let state = self.clone();
        let service_name = name;
        let service_name_for_task = service_name.clone();

        let handle = tauri::async_runtime::spawn(async move {
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

        // ponytail: one lock hold so check+insert are atomic; abort duplicate to keep live handle tracked
        let mut guard = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        use std::collections::hash_map::Entry;
        match guard.entry(service_name_for_task) {
            Entry::Occupied(_) => handle.abort(),
            Entry::Vacant(e) => {
                e.insert(handle);
            }
        }
    }

    async fn mark_service_started(&self, name: &str, message: String) {
        {
            let mut guard = self.statuses.lock().await;
            let entry = guard.entry(name.to_owned()).or_default();
            entry.running = true;
            entry.started_at = Some(current_timestamp());
            entry.last_error = None;
        }
        self.push_log(name, message).await;
        self.emit_dashboard_update();
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
        self.emit_dashboard_update();
    }

    async fn push_log(&self, name: &str, message: impl Into<String>) {
        let mut guard = self.logs.lock().await;
        let entry = guard.entry(name.to_owned()).or_default();
        entry.push_back(format!("[{}] {}", current_timestamp(), message.into()));
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
        self.qwen_runtime_dir.clone()
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

    fn mistral_runtime_dir(&self) -> PathBuf {
        self.providers_root().join("mistral")
    }

    fn zai_runtime_dir(&self) -> PathBuf {
        self.providers_root().join("zai")
    }

    fn meta_runtime_dir(&self) -> PathBuf {
        self.providers_root().join("meta")
    }

    fn hub_runtime_dir(&self) -> PathBuf {
        self.providers_root().join("hub")
    }

    async fn build_dashboard_overview(&self) -> Result<DashboardOverview> {
        let providers_future = join_all(
            provider_names()
                .into_iter()
                .map(|provider| self.provider_overview(provider)),
        );
        let hub_future = self.hub_overview();
        let (providers, hub) = tokio::join!(providers_future, hub_future);

        Ok(DashboardOverview {
            generated_at: current_timestamp(),
            runtime: self.runtime.clone(),
            startup_config: self.startup_config.clone(),
            hub,
            providers,
            qwen_account_count: list_qwen_accounts_db(
                &self.qwen_runtime_dir(),
                &self.workspace_root,
            )?
            .len(),
            open_provider_login_sessions: self
                .open_provider_login_sessions
                .lock()
                .await
                .iter()
                .cloned()
                .collect(),
            open_qwen_account_login_sessions: self
                .open_qwen_account_login_sessions
                .lock()
                .await
                .iter()
                .cloned()
                .collect(),
        })
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
        if !status.running {
            return HubOverview {
                running: status.running,
                started_at: status.started_at,
                health_status: "offline".to_owned(),
                model_count: 0,
                provider_statuses: Vec::new(),
                detail: None,
                config: self.hub_config(),
            };
        }

        let base_url = hub_base_url();
        let (health, models) = tokio::join!(
            self.call_json_get(&base_url, "/health", self.hub_api_key.as_deref()),
            self.call_json_get(&base_url, "/v1/models", self.hub_api_key.as_deref()),
        );
        let health = health.ok();
        let models = models.ok();

        let detail = health.as_ref().map(|(_, payload)| payload.clone());
        let health_status = health
            .as_ref()
            .and_then(|(_, payload)| payload.get("status"))
            .and_then(Value::as_str)
            .unwrap_or(if status.running {
                "starting"
            } else {
                "offline"
            })
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
        let open_provider_sessions = self.open_provider_login_sessions.lock().await.clone();
        let open_qwen_account_sessions = self.open_qwen_account_login_sessions.lock().await.clone();
        let login_open = open_provider_sessions.contains(name);
        if !status.running {
            return ProviderOverview {
                name: name.to_owned(),
                running: false,
                started_at: status.started_at,
                base_url,
                health_status: "offline".to_owned(),
                login_state: "offline".to_owned(),
                model_count: 0,
                models: Vec::new(),
                web_search_supported: provider_web_search_supported(name),
                last_error: status.last_error,
            };
        }

        let (health, models) = if login_open {
            (
                self.call_json_get(&base_url, "/health", None).await.ok(),
                None,
            )
        } else {
            let (health, models) = tokio::join!(
                self.call_json_get(&base_url, "/health", None),
                self.call_json_get(&base_url, "/v1/models", None),
            );
            (health.ok(), models.ok())
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
            .unwrap_or(if status.running {
                "starting"
            } else {
                "offline"
            })
            .to_owned();

        let login_state = if login_open {
            "login_open".to_owned()
        } else if name == "chatgpt"
            || name == "gemini"
            || name == "mistral"
            || name == "zai"
            || name == "meta"
        {
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
async fn start_service(state: State<'_, ControlState>, name: String) -> Result<(), String> {
    let name = normalize_service_name(&name).map_err(|err| err.to_string())?;
    if state.service_status(name).await.running {
        return Err(format!("service {} is already running", name));
    }
    state.ensure_service_dir(name);
    state
        .spawn_service_by_name(name)
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn stop_service(state: State<'_, ControlState>, name: String) -> Result<(), String> {
    let handle = state
        .tasks
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&name);
    if let Some(handle) = handle {
        handle.abort();
        state
            .mark_service_stopped(&name, Some(format!("service {name} stopped by user")), None)
            .await;
        Ok(())
    } else {
        Err(format!("service {} is not running", name))
    }
}

#[tauri::command]
async fn dashboard_overview(state: State<'_, ControlState>) -> Result<DashboardOverview, String> {
    state
        .build_dashboard_overview()
        .await
        .map_err(|err| err.to_string())
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
        Some(
            list_qwen_accounts_db(&state.qwen_runtime_dir(), &state.workspace_root)
                .map_err(|err| err.to_string())?,
        )
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
async fn start_services(
    state: State<'_, ControlState>,
    request: StartServicesRequest,
) -> Result<Vec<String>, String> {
    if request.services.is_empty() {
        return Err("at least one service is required".to_owned());
    }

    state
        .start_services(request.services)
        .await
        .map_err(|err| err.to_string())
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

    let (status, mut value) = state
        .call_json_post(
            &hub_base_url(),
            "/v1/chat/completions",
            state.hub_api_key.as_deref(),
            &payload,
        )
        .await
        .map_err(|err| err.to_string())?;

    if status >= 400 {
        return Err(value
            .get("error")
            .and_then(|item| item.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("hub request failed")
            .to_owned());
    }

    // Keep visible content clean. Do not surface reasoning/thinking as user text here.
    let provider_warnings = value.get("provider_warnings").cloned();
    if let Some(choices) = value.get_mut("choices").and_then(Value::as_array_mut) {
        for choice in choices.iter_mut() {
            if let Some(message) = choice.get_mut("message").and_then(Value::as_object_mut) {
                let content_empty = match message.get("content") {
                    Some(Value::String(text)) => text.trim().is_empty(),
                    Some(Value::Null) | None => true,
                    Some(other) => other
                        .as_array()
                        .map(|items| items.is_empty())
                        .unwrap_or(false),
                };

                if content_empty {
                    if let Some(tool_calls) = message.get("tool_calls").cloned() {
                        message.insert("content".to_owned(), Value::String(tool_calls.to_string()));
                    } else if let Some(warnings) = provider_warnings.clone() {
                        message.insert("content".to_owned(), Value::String(warnings.to_string()));
                    }
                }
            }
        }
    }

    Ok(value)
}

#[tauri::command]
async fn list_qwen_accounts(
    state: State<'_, ControlState>,
) -> Result<Vec<QwenAccountSummary>, String> {
    list_qwen_accounts_db(&state.qwen_runtime_dir(), &state.workspace_root)
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn add_qwen_account(
    state: State<'_, ControlState>,
    request: AddAccountRequest,
) -> Result<Vec<QwenAccountSummary>, String> {
    add_qwen_account_db(
        &state.qwen_runtime_dir(),
        &state.workspace_root,
        &request.email,
        &request.password,
    )
    .and_then(|_| list_qwen_accounts_db(&state.qwen_runtime_dir(), &state.workspace_root))
    .map_err(|err| err.to_string())
}

#[tauri::command]
async fn remove_qwen_account(
    state: State<'_, ControlState>,
    account_id: String,
) -> Result<Vec<QwenAccountSummary>, String> {
    remove_qwen_account_db(
        &state.qwen_runtime_dir(),
        &state.workspace_root,
        &account_id,
    )
    .and_then(|_| list_qwen_accounts_db(&state.qwen_runtime_dir(), &state.workspace_root))
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

    let (status, response) = state
        .call_json_post(
            &provider_base_url(provider),
            "/admin/manual_login",
            None,
            &payload,
        )
        .await
        .map_err(|err| err.to_string())?;

    if status >= 400 {
        return Err(response
            .get("error")
            .and_then(|item| item.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("manual login failed")
            .to_owned());
    }

    let mut guard = state.open_provider_login_sessions.lock().await;
    guard.insert(provider.to_owned());
    let result = guard.iter().cloned().collect();
    drop(guard);
    state.emit_dashboard_update();
    Ok(result)
}

#[tauri::command]
async fn stop_provider_login_session(
    state: State<'_, ControlState>,
    provider: String,
) -> Result<Vec<String>, String> {
    let provider = normalize_provider(&provider).map_err(|err| err.to_string())?;
    let _ = state
        .call_json_post(
            &provider_base_url(provider),
            "/admin/close_login",
            None,
            &json!({}),
        )
        .await;

    let mut guard = state.open_provider_login_sessions.lock().await;
    guard.remove(provider);
    let result = guard.iter().cloned().collect();
    drop(guard);
    state.emit_dashboard_update();
    Ok(result)
}

#[tauri::command]
async fn start_qwen_account_login_session(
    state: State<'_, ControlState>,
    request: QwenAccountLoginRequest,
) -> Result<Vec<String>, String> {
    let account_id = request.account_id.trim().to_owned();
    if account_id.is_empty() {
        return Err("account_id is required".to_owned());
    }
    if !is_safe_account_id(&account_id) {
        return Err("account_id must be 1-64 chars of [A-Za-z0-9_-]".to_owned());
    }

    let payload = json!({
        "browser": request.browser.unwrap_or_else(|| DEFAULT_BROWSER.to_owned()),
        "account_id": account_id.clone(),
    });

    let (status, response) = state
        .call_json_post(
            &provider_base_url("qwen"),
            "/admin/manual_login",
            None,
            &payload,
        )
        .await
        .map_err(|err| err.to_string())?;

    if status >= 400 {
        return Err(response
            .get("error")
            .and_then(|item| item.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("manual account login failed")
            .to_owned());
    }

    let mut guard = state.open_qwen_account_login_sessions.lock().await;
    guard.insert(account_id);
    let result = guard.iter().cloned().collect();
    drop(guard);
    state.emit_dashboard_update();
    Ok(result)
}

#[tauri::command]
async fn stop_qwen_account_login_session(
    state: State<'_, ControlState>,
    account_id: String,
) -> Result<Vec<String>, String> {
    let account_id = account_id.trim().to_owned();
    if account_id.is_empty() {
        return Err("account_id is required".to_owned());
    }
    if !is_safe_account_id(&account_id) {
        return Err("account_id must be 1-64 chars of [A-Za-z0-9_-]".to_owned());
    }

    let _ = state
        .call_json_post(
            &provider_base_url("qwen"),
            "/admin/close_login",
            None,
            &json!({ "account_id": account_id.clone() }),
        )
        .await;

    let mut guard = state.open_qwen_account_login_sessions.lock().await;
    guard.remove(&account_id);
    let result = guard.iter().cloned().collect();
    drop(guard);
    state.emit_dashboard_update();
    Ok(result)
}

fn normalize_provider(value: &str) -> Result<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "qwen" => Ok("qwen"),
        "deepseek" => Ok("deepseek"),
        "kimi" => Ok("kimi"),
        "chatgpt" => Ok("chatgpt"),
        "gemini" => Ok("gemini"),
        "mistral" => Ok("mistral"),
        "zai" => Ok("zai"),
        "meta" => Ok("meta"),
        other => Err(anyhow!("unsupported provider: {other}")),
    }
}

fn normalize_service_name(value: &str) -> Result<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "hub" => Ok("hub"),
        other => normalize_provider(other),
    }
}

fn startup_config_from_env() -> StartupConfig {
    let configured = env::var("RUST_PROXY_START_SERVICES")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());

    let Some(configured) = configured else {
        // Default when RUST_PROXY_START_SERVICES is unset: start nothing automatically.
        // All providers must be started manually from the dashboard.
        // Set RUST_PROXY_START_SERVICES=all for every service, or a comma-separated
        // list (e.g. "qwen,kimi") to auto-start specific ones.
        return StartupConfig {
            mode: "manual".to_owned(),
            services: Vec::new(),
        };
    };

    if configured.eq_ignore_ascii_case("manual") || configured.eq_ignore_ascii_case("none") {
        return StartupConfig {
            mode: "manual".to_owned(),
            services: Vec::new(),
        };
    }

    if configured.eq_ignore_ascii_case("all") {
        return StartupConfig {
            mode: "all".to_owned(),
            services: service_names()
                .iter()
                .map(|name| name.to_string())
                .collect(),
        };
    }

    let mut seen = HashSet::new();
    let services = configured
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .filter_map(|name| normalize_service_name(name).ok())
        .filter(|name| seen.insert(*name))
        .map(str::to_owned)
        .collect::<Vec<_>>();

    if services.is_empty() {
        StartupConfig {
            mode: "manual".to_owned(),
            services,
        }
    } else {
        StartupConfig {
            mode: "selected".to_owned(),
            services,
        }
    }
}

fn service_names() -> [&'static str; 9] {
    [
        "hub", "qwen", "deepseek", "kimi", "chatgpt", "gemini", "mistral", "zai", "meta",
    ]
}

fn provider_names() -> [&'static str; 8] {
    [
        "qwen", "deepseek", "kimi", "chatgpt", "gemini", "mistral", "zai", "meta",
    ]
}

fn provider_base_url(provider: &str) -> String {
    let port = match provider {
        "qwen" => QWEN_PORT,
        "deepseek" => DEEPSEEK_PORT,
        "kimi" => KIMI_PORT,
        "chatgpt" => CHATGPT_PORT,
        "gemini" => GEMINI_PORT,
        "mistral" => MISTRAL_PORT,
        "zai" => ZAI_PORT,
        "meta" => META_PORT,
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let state = ControlState::new(app.handle())?;
            state.start_dashboard_coalescer();
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
            start_services,
            run_workbench_request,
            list_qwen_accounts,
            add_qwen_account,
            remove_qwen_account,
            start_provider_login_session,
            stop_provider_login_session,
            start_qwen_account_login_session,
            stop_qwen_account_login_session,
            start_service,
            stop_service
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::{
        add_qwen_account_db, ensure_qwen_db, normalize_provider, provider_base_url, service_names,
        ControlState, RuntimeDiagnostics, ServiceRuntimeStatus, StartupConfig,
    };
    use std::{
        collections::{HashMap, HashSet, VecDeque},
        fs,
        path::PathBuf,
        sync::Arc,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };
    use tokio::sync::Mutex;

    fn temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("rustproxyhub-control-room-{name}-{unique}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn provider_base_urls_keep_embedded_ports() {
        assert_eq!(provider_base_url("qwen"), "http://127.0.0.1:3000");
        assert_eq!(provider_base_url("deepseek"), "http://127.0.0.1:3001");
        assert_eq!(provider_base_url("kimi"), "http://127.0.0.1:3002");
        assert_eq!(provider_base_url("chatgpt"), "http://127.0.0.1:3003");
        assert_eq!(provider_base_url("gemini"), "http://127.0.0.1:3004");
        assert_eq!(provider_base_url("mistral"), "http://127.0.0.1:3005");
        assert_eq!(provider_base_url("zai"), "http://127.0.0.1:3006");
        assert_eq!(provider_base_url("meta"), "http://127.0.0.1:3007");
    }

    #[test]
    fn provider_normalization_accepts_mistral_zai_and_meta() {
        assert_eq!(normalize_provider(" MISTRAL ").unwrap(), "mistral");
        assert_eq!(normalize_provider(" ZAI ").unwrap(), "zai");
        assert_eq!(normalize_provider(" META ").unwrap(), "meta");
        assert!(normalize_provider("claude").is_err());
    }

    #[tokio::test]
    async fn runtime_block_marks_all_services_with_last_error() {
        let mut statuses = HashMap::new();
        let mut logs = HashMap::new();
        for name in service_names() {
            statuses.insert(name.to_owned(), ServiceRuntimeStatus::default());
            logs.insert(name.to_owned(), VecDeque::new());
        }

        let state = ControlState {
            workspace_root: PathBuf::from("G:/repo"),
            app_data_dir: PathBuf::from("G:/repo/runtime"),
            qwen_runtime_dir: PathBuf::from("G:/repo/runtime/providers/qwen"),
            runtime: RuntimeDiagnostics {
                node_path: None,
                node_source: None,
                helper_dir: None,
                browser_available: false,
                single_runner_ready: false,
                issues: vec!["Bundled node.exe not found in Tauri resources.".to_owned()],
            },
            startup_config: StartupConfig {
                mode: "manual".to_owned(),
                services: Vec::new(),
            },
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .tcp_nodelay(true)
                .pool_idle_timeout(Duration::from_secs(90))
                .pool_max_idle_per_host(16)
                .build()
                .unwrap(),
            hub_api_key: None,
            statuses: Arc::new(Mutex::new(statuses)),
            logs: Arc::new(Mutex::new(logs)),
            open_provider_login_sessions: Arc::new(Mutex::new(HashSet::new())),
            open_qwen_account_login_sessions: Arc::new(Mutex::new(HashSet::new())),
            tasks: Arc::new(std::sync::Mutex::new(HashMap::new())),
            app_handle: None,
            dashboard_notify: Arc::new(tokio::sync::Notify::new()),
        };

        state.mark_runtime_blocked().await;

        let statuses = state.statuses.lock().await;
        for name in service_names() {
            assert_eq!(
                statuses
                    .get(name)
                    .and_then(|entry| entry.last_error.as_deref()),
                Some("Bundled node.exe not found in Tauri resources.")
            );
        }
    }

    #[tokio::test]
    async fn dashboard_overview_reports_runtime_and_qwen_counts() {
        let workspace_root = temp_dir("overview-workspace");
        let app_data_dir = workspace_root.join("app-data");
        let qwen_runtime_dir = app_data_dir.join("providers").join("qwen");

        ensure_qwen_db(&qwen_runtime_dir, &workspace_root).unwrap();
        add_qwen_account_db(
            &qwen_runtime_dir,
            &workspace_root,
            "pilot@example.com",
            "secret",
        )
        .unwrap();

        let mut statuses = HashMap::new();
        let mut logs = HashMap::new();
        for name in service_names() {
            statuses.insert(name.to_owned(), ServiceRuntimeStatus::default());
            logs.insert(name.to_owned(), VecDeque::new());
        }

        let state = ControlState {
            workspace_root: workspace_root.clone(),
            app_data_dir: app_data_dir.clone(),
            qwen_runtime_dir: qwen_runtime_dir.clone(),
            runtime: RuntimeDiagnostics {
                node_path: Some("C:/bundle/resources/node/node.exe".to_owned()),
                node_source: Some("bundled-resource".to_owned()),
                helper_dir: Some("C:/bundle/resources/playwright-bridge".to_owned()),
                browser_available: true,
                single_runner_ready: true,
                issues: Vec::new(),
            },
            startup_config: StartupConfig {
                mode: "manual".to_owned(),
                services: Vec::new(),
            },
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .tcp_nodelay(true)
                .pool_idle_timeout(Duration::from_secs(90))
                .pool_max_idle_per_host(16)
                .build()
                .unwrap(),
            hub_api_key: None,
            statuses: Arc::new(Mutex::new(statuses)),
            logs: Arc::new(Mutex::new(logs)),
            open_provider_login_sessions: Arc::new(Mutex::new(HashSet::from([String::from(
                "qwen",
            )]))),
            open_qwen_account_login_sessions: Arc::new(Mutex::new(HashSet::from([String::from(
                "acct-1",
            )]))),
            tasks: Arc::new(std::sync::Mutex::new(HashMap::new())),
            app_handle: None,
            dashboard_notify: Arc::new(tokio::sync::Notify::new()),
        };

        let overview = state.build_dashboard_overview().await.unwrap();

        assert!(overview.runtime.single_runner_ready);
        assert_eq!(
            overview.runtime.node_path.as_deref(),
            Some("C:/bundle/resources/node/node.exe")
        );
        assert_eq!(overview.qwen_account_count, 1);
        assert_eq!(
            overview.open_provider_login_sessions,
            vec![String::from("qwen")]
        );
        assert_eq!(
            overview.open_qwen_account_login_sessions,
            vec![String::from("acct-1")]
        );
    }
}
