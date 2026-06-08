use anyhow::{anyhow, Result};
use reqwest::header::AUTHORIZATION;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashMap, VecDeque},
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::State;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    sync::Mutex,
};
use uuid::Uuid;

const LOG_LIMIT: usize = 500;
const LOG_TAIL: usize = 48;

#[derive(Clone)]
struct ManagedProcess {
    child: Arc<Mutex<Child>>,
    logs: Arc<Mutex<VecDeque<String>>>,
    port: Option<u16>,
    started_at: u64,
    api_key: Option<String>,
    launch_preview: String,
}

#[derive(Clone)]
struct ControlState {
    tools_root: PathBuf,
    rust_proxy_hub: PathBuf,
    client: reqwest::Client,
    services: Arc<Mutex<HashMap<String, ManagedProcess>>>,
    qwen_login_sessions: Arc<Mutex<HashMap<String, ManagedProcess>>>,
    provider_login_sessions: Arc<Mutex<HashMap<String, ManagedProcess>>>,
}

#[derive(Debug, Deserialize)]
struct HubUpstreamRequest {
    provider: String,
    port: u16,
    api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StartServiceRequest {
    provider: String,
    port: u16,
    api_key: Option<String>,
    browser: Option<String>,
    headless: Option<bool>,
    upstreams: Option<Vec<HubUpstreamRequest>>,
}

#[derive(Debug, Serialize)]
struct DashboardSnapshot {
    tools_root: String,
    rust_proxy_hub: String,
    services: Vec<ServiceSnapshot>,
    qwen_accounts: Vec<QwenAccountSummary>,
    open_login_sessions: Vec<String>,
    provider_login_sessions: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ServiceSnapshot {
    provider: String,
    running: bool,
    port: Option<u16>,
    pid: Option<u32>,
    started_at: Option<u64>,
    launch_preview: Option<String>,
    logs: Vec<String>,
    health: Option<Value>,
    model_count: usize,
    models: Vec<ServiceModelSummary>,
    admin_status: Option<Value>,
    endpoints: ServiceEndpoints,
}

#[derive(Debug, Serialize)]
struct ServiceModelSummary {
    id: String,
    provider: Option<String>,
}

#[derive(Debug, Serialize)]
struct ServiceEndpoints {
    base_url: Option<String>,
    health_url: Option<String>,
    models_url: Option<String>,
    chat_url: Option<String>,
    openapi_url: Option<String>,
    stop_url: Option<String>,
    upload_url: Option<String>,
}

#[derive(Debug, Serialize)]
struct QwenAccountSummary {
    id: String,
    email: String,
    has_password: bool,
    created_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AddAccountRequest {
    email: String,
    password: String,
}

#[derive(Debug, Deserialize)]
struct LoginSessionRequest {
    account_id: String,
    browser: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProviderLoginRequest {
    provider: String,
    browser: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorkbenchRequest {
    service: String,
    model: String,
    prompt: String,
}

impl ControlState {
    fn new() -> Result<Self> {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let tools_root = manifest_dir
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| anyhow!("failed to resolve tools root"))?
            .to_path_buf();
        let rust_proxy_hub = tools_root.join("RustProxyHub");
        Ok(Self {
            tools_root,
            rust_proxy_hub,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(3))
                .build()?,
            services: Arc::new(Mutex::new(HashMap::new())),
            qwen_login_sessions: Arc::new(Mutex::new(HashMap::new())),
            provider_login_sessions: Arc::new(Mutex::new(HashMap::new())),
        })
    }
}

#[tauri::command]
async fn app_snapshot(state: State<'_, ControlState>) -> Result<DashboardSnapshot, String> {
    let service_entries = {
        let guard = state.services.lock().await;
        guard
            .iter()
            .map(|(provider, process)| (provider.clone(), process.clone()))
            .collect::<Vec<_>>()
    };

    let mut process_map = HashMap::new();
    for (provider, process) in service_entries {
        process_map.insert(provider, process);
    }

    let mut services = Vec::new();
    for &provider in service_order() {
        if let Some(process) = process_map.get(provider).cloned() {
            services.push(snapshot_service(&state.client, provider.to_owned(), process).await);
        } else {
            services.push(empty_service_snapshot(provider));
        }
    }

    let login_sessions = collect_live_session_keys(&state.qwen_login_sessions).await;
    let provider_login_sessions = collect_live_session_keys(&state.provider_login_sessions).await;

    Ok(DashboardSnapshot {
        tools_root: state.tools_root.display().to_string(),
        rust_proxy_hub: state.rust_proxy_hub.display().to_string(),
        services,
        qwen_accounts: list_qwen_accounts_internal(&state).map_err(|err| err.to_string())?,
        open_login_sessions: login_sessions,
        provider_login_sessions,
    })
}

#[tauri::command]
async fn start_service(
    state: State<'_, ControlState>,
    request: StartServiceRequest,
) -> Result<ServiceSnapshot, String> {
    let provider = normalize_provider(&request.provider)?;
    {
        let mut guard = state.services.lock().await;
        if let Some(existing) = guard.get(&provider).cloned() {
            if is_running(&existing).await {
                drop(guard);
                return Ok(snapshot_service(&state.client, provider, existing).await);
            }
            guard.remove(&provider);
        }
    }

    let process = spawn_service_process(&state.rust_proxy_hub, &provider, &request)
        .await
        .map_err(|err| err.to_string())?;

    state
        .services
        .lock()
        .await
        .insert(provider.clone(), process.clone());
    Ok(snapshot_service(&state.client, provider, process).await)
}

#[tauri::command]
async fn stop_service(
    state: State<'_, ControlState>,
    provider: String,
) -> Result<ServiceSnapshot, String> {
    let provider = normalize_provider(&provider)?;
    let process = state.services.lock().await.remove(&provider);
    if let Some(process) = process {
        kill_process(&process).await;
        Ok(snapshot_service(&state.client, provider, process).await)
    } else {
        Ok(empty_service_snapshot(&provider))
    }
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
async fn start_qwen_login_session(
    state: State<'_, ControlState>,
    request: LoginSessionRequest,
) -> Result<Vec<String>, String> {
    let account_id = request.account_id.trim().to_owned();
    if account_id.is_empty() {
        return Err("account_id is required".to_owned());
    }

    {
        let mut guard = state.qwen_login_sessions.lock().await;
        if let Some(existing) = guard.get(&account_id).cloned() {
            if is_running(&existing).await {
                return Ok(guard.keys().cloned().collect());
            }
            guard.remove(&account_id);
        }
    }

    let process = spawn_login_process(
        &state.rust_proxy_hub,
        &account_id,
        request.browser.clone().unwrap_or_else(|| "chromium".to_owned()),
    )
    .await
    .map_err(|err| err.to_string())?;

    let mut guard = state.qwen_login_sessions.lock().await;
    guard.insert(account_id, process);
    Ok(guard.keys().cloned().collect())
}

#[tauri::command]
async fn stop_qwen_login_session(
    state: State<'_, ControlState>,
    account_id: String,
) -> Result<Vec<String>, String> {
    let process = state.qwen_login_sessions.lock().await.remove(&account_id);
    if let Some(process) = process {
        kill_process(&process).await;
    }
    Ok(state
        .qwen_login_sessions
        .lock()
        .await
        .keys()
        .cloned()
        .collect())
}

#[tauri::command]
async fn start_provider_login_session(
    state: State<'_, ControlState>,
    request: ProviderLoginRequest,
) -> Result<Vec<String>, String> {
    let provider = normalize_provider(&request.provider)?;
    if provider == "hub" {
        return Err("hub does not support browser login sessions".to_owned());
    }

    {
        let mut guard = state.provider_login_sessions.lock().await;
        if let Some(existing) = guard.get(&provider).cloned() {
            if is_running(&existing).await {
                return Ok(guard.keys().cloned().collect());
            }
            guard.remove(&provider);
        }
    }

    let process = spawn_provider_login_process(
        &state.rust_proxy_hub,
        &provider,
        request.browser.clone().unwrap_or_else(|| "chromium".to_owned()),
    )
    .await
    .map_err(|err| err.to_string())?;

    let mut guard = state.provider_login_sessions.lock().await;
    guard.insert(provider, process);
    Ok(guard.keys().cloned().collect())
}

#[tauri::command]
async fn stop_provider_login_session(
    state: State<'_, ControlState>,
    provider: String,
) -> Result<Vec<String>, String> {
    let provider = normalize_provider(&provider)?;
    let process = state.provider_login_sessions.lock().await.remove(&provider);
    if let Some(process) = process {
        kill_process(&process).await;
    }
    Ok(state
        .provider_login_sessions
        .lock()
        .await
        .keys()
        .cloned()
        .collect())
}

#[tauri::command]
async fn fetch_service_models(
    state: State<'_, ControlState>,
    provider: String,
) -> Result<Vec<ServiceModelSummary>, String> {
    let provider = normalize_provider(&provider)?;
    let process = state
        .services
        .lock()
        .await
        .get(&provider)
        .cloned()
        .ok_or_else(|| format!("{provider} service is not running"))?;
    let Some(port) = process.port else {
        return Err(format!("{provider} service does not expose an HTTP port"));
    };

    service_model_summaries(&state.client, port, process.api_key.as_deref())
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn run_workbench_request(
    state: State<'_, ControlState>,
    request: WorkbenchRequest,
) -> Result<Value, String> {
    let provider = normalize_provider(&request.service)?;
    let process = state
        .services
        .lock()
        .await
        .get(&provider)
        .cloned()
        .ok_or_else(|| format!("{provider} service is not running"))?;
    let Some(port) = process.port else {
        return Err(format!("{provider} service does not expose an HTTP port"));
    };

    let url = format!("http://127.0.0.1:{port}/v1/chat/completions");
    let payload = serde_json::json!({
        "model": request.model,
        "messages": [{ "role": "user", "content": request.prompt }],
        "stream": false,
    });

    let mut request_builder = state.client.post(url).json(&payload);
    if let Some(api_key) = process.api_key.as_deref().filter(|value| !value.trim().is_empty()) {
        request_builder = request_builder.header(AUTHORIZATION, format!("Bearer {api_key}"));
    }

    let response = request_builder.send().await.map_err(|err| err.to_string())?;
    let status = response.status();
    let bytes = response.bytes().await.map_err(|err| err.to_string())?;
    let value = serde_json::from_slice::<Value>(&bytes).unwrap_or_else(|_| {
        serde_json::json!({
            "status": status.as_u16(),
            "raw": String::from_utf8_lossy(&bytes).to_string(),
        })
    });

    if status.is_success() {
        Ok(value)
    } else {
        Err(value.to_string())
    }
}

fn normalize_provider(provider: &str) -> Result<String, String> {
    let normalized = provider.trim().to_lowercase();
    match normalized.as_str() {
        "hub" | "qwen" | "kimi" | "deepseek" => Ok(normalized),
        _ => Err("provider must be one of: hub, qwen, kimi, deepseek".to_owned()),
    }
}

fn provider_package(provider: &str) -> (&'static str, &'static str) {
    match provider {
        "hub" => ("hub-proxy-rs", "hub-proxy-rs"),
        "deepseek" => ("deepseek-proxy-rs", "deepseek-proxy-rs"),
        "kimi" => ("kimi-proxy-rs", "kimi-proxy-rs"),
        _ => ("qwen-proxy-rs", "qwen-proxy-rs"),
    }
}

fn service_order() -> &'static [&'static str] {
    &["hub", "qwen", "deepseek", "kimi"]
}

fn binary_path(workspace: &Path, provider: &str) -> PathBuf {
    let (_, binary) = provider_package(provider);
    let file = if cfg!(windows) {
        format!("{binary}.exe")
    } else {
        binary.to_owned()
    };
    workspace.join("target").join("debug").join(file)
}

async fn spawn_service_process(
    workspace: &Path,
    provider: &str,
    request: &StartServiceRequest,
) -> Result<ManagedProcess> {
    if provider == "hub" {
        return spawn_hub_service_process(
            workspace,
            request.port,
            request.api_key.clone(),
            request.upstreams.as_deref().unwrap_or(&[]),
        )
        .await;
    }

    let (package, _) = provider_package(provider);
    let binary = binary_path(workspace, provider);
    let mut command;
    let launch_preview;

    if binary.exists() {
        launch_preview = format!("{} server", binary.display());
        command = Command::new(binary);
        command.arg("server");
    } else {
        launch_preview = format!("cargo run -p {package} -- server");
        command = Command::new("cargo");
        command.args(["run", "-p", package, "--", "server"]);
    }

    command
        .current_dir(workspace)
        .env("HOST", "127.0.0.1")
        .env("PORT", request.port.to_string())
        .env(
            "BROWSER",
            request
                .browser
                .clone()
                .unwrap_or_else(|| "chromium".to_owned()),
        )
        .env(
            "HEADLESS",
            if request.headless.unwrap_or(true) {
                "true"
            } else {
                "false"
            },
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    match request
        .api_key
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        Some(value) => {
            command.env("API_KEY", value);
        }
        None => {
            command.env_remove("API_KEY");
        }
    }

    spawn_managed_process(command, Some(request.port), request.api_key.clone(), launch_preview).await
}

async fn spawn_hub_service_process(
    workspace: &Path,
    port: u16,
    api_key: Option<String>,
    upstreams: &[HubUpstreamRequest],
) -> Result<ManagedProcess> {
    let (package, _) = provider_package("hub");
    let binary = binary_path(workspace, "hub");
    let mut command;
    let launch_preview;

    if binary.exists() {
        launch_preview = format!("{} server", binary.display());
        command = Command::new(binary);
        command.arg("server");
    } else {
        launch_preview = format!("cargo run -p {package} -- server");
        command = Command::new("cargo");
        command.args(["run", "-p", package, "--", "server"]);
    }

    command
        .current_dir(workspace)
        .env("HOST", "127.0.0.1")
        .env("PORT", port.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    match api_key.as_deref().filter(|value| !value.trim().is_empty()) {
        Some(value) => {
            command.env("API_KEY", value);
        }
        None => {
            command.env_remove("API_KEY");
        }
    }

    for upstream in upstreams {
        let provider = normalize_provider(&upstream.provider).map_err(anyhow::Error::msg)?;
        if provider == "hub" {
            continue;
        }
        let upper = provider.to_uppercase();
        command.env(
            format!("{upper}_BASE_URL"),
            format!("http://127.0.0.1:{}", upstream.port),
        );
        match upstream
            .api_key
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            Some(value) => {
                command.env(format!("{upper}_API_KEY"), value);
            }
            None => {
                command.env_remove(format!("{upper}_API_KEY"));
            }
        }
    }

    spawn_managed_process(command, Some(port), api_key, launch_preview).await
}

async fn spawn_login_process(
    workspace: &Path,
    account_id: &str,
    browser: String,
) -> Result<ManagedProcess> {
    let binary = binary_path(workspace, "qwen");
    let mut command;
    let launch_preview;

    if binary.exists() {
        launch_preview = format!(
            "{} login --browser {} --account-id {}",
            binary.display(),
            browser,
            account_id
        );
        command = Command::new(binary);
        command.args(["login", "--browser", browser.as_str(), "--account-id", account_id]);
    } else {
        launch_preview = format!(
            "cargo run -p qwen-proxy-rs -- login --browser {} --account-id {}",
            browser, account_id
        );
        command = Command::new("cargo");
        command.args([
            "run",
            "-p",
            "qwen-proxy-rs",
            "--",
            "login",
            "--browser",
            browser.as_str(),
            "--account-id",
            account_id,
        ]);
    }

    command
        .current_dir(workspace)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    spawn_managed_process(command, None, None, launch_preview).await
}

async fn spawn_provider_login_process(
    workspace: &Path,
    provider: &str,
    browser: String,
) -> Result<ManagedProcess> {
    if provider == "hub" {
        return Err(anyhow!("hub does not support login"));
    }

    let (package, _) = provider_package(provider);
    let binary = binary_path(workspace, provider);
    let mut command;
    let launch_preview;

    if binary.exists() {
        launch_preview = format!("{} login --browser {}", binary.display(), browser);
        command = Command::new(binary);
        command.args(["login", "--browser", browser.as_str()]);
    } else {
        launch_preview = format!("cargo run -p {package} -- login --browser {browser}");
        command = Command::new("cargo");
        command.args(["run", "-p", package, "--", "login", "--browser", browser.as_str()]);
    }

    command
        .current_dir(workspace)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    spawn_managed_process(command, None, None, launch_preview).await
}

async fn spawn_managed_process(
    mut command: Command,
    port: Option<u16>,
    api_key: Option<String>,
    launch_preview: String,
) -> Result<ManagedProcess> {
    let mut child = command.spawn()?;
    let logs = Arc::new(Mutex::new(VecDeque::with_capacity(LOG_LIMIT)));

    if let Some(stdout) = child.stdout.take() {
        spawn_log_reader(stdout, Arc::clone(&logs), "out");
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_log_reader(stderr, Arc::clone(&logs), "err");
    }

    Ok(ManagedProcess {
        child: Arc::new(Mutex::new(child)),
        logs,
        port,
        started_at: now_ts(),
        api_key,
        launch_preview,
    })
}

fn spawn_log_reader<R>(reader: R, logs: Arc<Mutex<VecDeque<String>>>, label: &'static str)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tauri::async_runtime::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let mut guard = logs.lock().await;
            if guard.len() >= LOG_LIMIT {
                guard.pop_front();
            }
            guard.push_back(format!("[{label}] {line}"));
        }
    });
}

async fn kill_process(process: &ManagedProcess) {
    let pid = {
        let child = process.child.lock().await;
        child.id()
    };

    if let Some(pid) = pid {
        #[cfg(windows)]
        {
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await;
            return;
        }
    }

    let mut child = process.child.lock().await;
    let _ = child.kill().await;
}

async fn is_running(process: &ManagedProcess) -> bool {
    let mut child = process.child.lock().await;
    matches!(child.try_wait(), Ok(None))
}

async fn collect_live_session_keys(
    sessions: &Mutex<HashMap<String, ManagedProcess>>,
) -> Vec<String> {
    let entries = {
        let guard = sessions.lock().await;
        guard.iter()
            .map(|(key, process)| (key.clone(), process.clone()))
            .collect::<Vec<_>>()
    };

    let mut live = Vec::new();
    let mut stale = Vec::new();
    for (key, process) in entries {
        if is_running(&process).await {
            live.push(key);
        } else {
            stale.push(key);
        }
    }

    if !stale.is_empty() {
        let mut guard = sessions.lock().await;
        for key in stale {
            guard.remove(&key);
        }
    }

    live.sort();
    live
}

async fn snapshot_service(
    client: &reqwest::Client,
    provider: String,
    process: ManagedProcess,
) -> ServiceSnapshot {
    let running = is_running(&process).await;
    let pid = {
        let child = process.child.lock().await;
        child.id()
    };
    let logs = {
        let guard = process.logs.lock().await;
        let start = guard.len().saturating_sub(LOG_TAIL);
        guard.iter().skip(start).cloned().collect::<Vec<_>>()
    };

    let mut snapshot = ServiceSnapshot {
        provider: provider.clone(),
        running,
        port: process.port,
        pid,
        started_at: Some(process.started_at),
        launch_preview: Some(process.launch_preview.clone()),
        logs,
        health: None,
        model_count: 0,
        models: Vec::new(),
        admin_status: None,
        endpoints: service_endpoints(&provider, process.port),
    };

    if running {
        if let Some(port) = process.port {
            snapshot.health = fetch_json(client, port, "/health", None).await;
            if let Some(models) =
                fetch_json(client, port, "/v1/models", process.api_key.as_deref()).await
            {
                snapshot.models = extract_model_summaries(&models);
                snapshot.model_count = snapshot.models.len();
            }
            if provider == "qwen" {
                snapshot.admin_status =
                    fetch_json(client, port, "/admin/status", process.api_key.as_deref()).await;
            } else if provider == "hub" {
                snapshot.admin_status =
                    fetch_json(client, port, "/providers", process.api_key.as_deref()).await;
            }
        }
    }

    snapshot
}

fn empty_service_snapshot(provider: &str) -> ServiceSnapshot {
    ServiceSnapshot {
        provider: provider.to_owned(),
        running: false,
        port: None,
        pid: None,
        started_at: None,
        launch_preview: None,
        logs: Vec::new(),
        health: None,
        model_count: 0,
        models: Vec::new(),
        admin_status: None,
        endpoints: service_endpoints(provider, None),
    }
}

fn service_endpoints(provider: &str, port: Option<u16>) -> ServiceEndpoints {
    let base_url = port.map(|port| format!("http://127.0.0.1:{port}"));
    let health_url = base_url.as_ref().map(|base| format!("{base}/health"));
    let models_url = base_url.as_ref().map(|base| format!("{base}/v1/models"));
    let chat_url = base_url
        .as_ref()
        .map(|base| format!("{base}/v1/chat/completions"));
    let openapi_url = (provider == "hub")
        .then(|| base_url.as_ref().map(|base| format!("{base}/openapi.json")))
        .flatten();
    let stop_url = ((provider == "hub") || (provider == "qwen"))
        .then(|| {
            base_url
                .as_ref()
                .map(|base| format!("{base}/v1/chat/completions/stop"))
        })
        .flatten();
    let upload_url = ((provider == "hub") || (provider == "qwen"))
        .then(|| base_url.as_ref().map(|base| format!("{base}/v1/upload")))
        .flatten();

    ServiceEndpoints {
        base_url,
        health_url,
        models_url,
        chat_url,
        openapi_url,
        stop_url,
        upload_url,
    }
}

fn extract_model_summaries(payload: &Value) -> Vec<ServiceModelSummary> {
    payload
        .get("data")
        .and_then(Value::as_array)
        .map(|items| {
            items.iter()
                .filter_map(|item| {
                    let id = item.get("id").and_then(Value::as_str)?;
                    Some(ServiceModelSummary {
                        id: id.to_owned(),
                        provider: item
                            .get("provider")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

async fn service_model_summaries(
    client: &reqwest::Client,
    port: u16,
    api_key: Option<&str>,
) -> Result<Vec<ServiceModelSummary>> {
    Ok(fetch_json(client, port, "/v1/models", api_key)
        .await
        .map(|payload| extract_model_summaries(&payload))
        .unwrap_or_default())
}

async fn fetch_json(
    client: &reqwest::Client,
    port: u16,
    path: &str,
    api_key: Option<&str>,
) -> Option<Value> {
    let url = format!("http://127.0.0.1:{port}{path}");
    let mut request = client.get(url);
    if let Some(api_key) = api_key.filter(|value| !value.trim().is_empty()) {
        request = request.header(AUTHORIZATION, format!("Bearer {api_key}"));
    }
    let response = request.send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.json::<Value>().await.ok()
}

fn ensure_qwen_db(state: &ControlState) -> Result<PathBuf> {
    let runtime_dir = state.rust_proxy_hub.join("runtime").join("qwen").join("data");
    fs::create_dir_all(&runtime_dir)?;
    let db_path = runtime_dir.join("qwenproxy.db");

    if !db_path.exists() {
        let legacy_db = state
            .tools_root
            .join("proxy")
            .join("qwenproxy")
            .join("data")
            .join("qwenproxy.db");
        if legacy_db.exists() {
            fs::copy(&legacy_db, &db_path)?;
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

    let legacy_json = state.tools_root.join("proxy").join("qwenproxy").join("accounts.json");
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
            account.id.unwrap_or_else(|| Uuid::new_v4().to_string()),
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
        params![Uuid::new_v4().to_string(), email, password],
    )?;
    Ok(())
}

fn remove_qwen_account_internal(state: &ControlState, account_id: &str) -> Result<()> {
    let db_path = ensure_qwen_db(state)?;
    let connection = Connection::open(db_path)?;
    connection.execute("DELETE FROM accounts WHERE id = ?1", params![account_id])?;
    Ok(())
}

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = ControlState::new().expect("valid control state");
    tauri::Builder::default()
        .setup(|app| {
            #[cfg(debug_assertions)]
            {
                let window = tauri::Manager::get_webview_window(app, "main").unwrap();
                window.open_devtools();
            }
            Ok(())
        })
        .manage(state)
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_prevent_default::init())
        .invoke_handler(tauri::generate_handler![
            app_snapshot,
            start_service,
            stop_service,
            add_qwen_account,
            remove_qwen_account,
            start_qwen_login_session,
            stop_qwen_login_session,
            start_provider_login_session,
            stop_provider_login_session,
            fetch_service_models,
            run_workbench_request
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
