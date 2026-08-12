
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
    let overview = state.provider_overview(provider, None).await;
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
async fn provider_models(
    state: State<'_, ControlState>,
    provider: String,
    chatgpt_mode: Option<String>,
) -> Result<ProviderOverview, String> {
    let provider = normalize_provider(&provider).map_err(|err| err.to_string())?;
    Ok(state
        .provider_overview(provider, chatgpt_mode.as_deref())
        .await)
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
        "chatgpt_mode": request.chatgpt_mode.as_deref().unwrap_or("auto"),
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

    state
        .ensure_provider_ready(provider)
        .await
        .map_err(|err| err.to_string())?;

    let (status, response) = state
        .call_json_post_with_timeout(
            &provider_base_url(provider),
            "/admin/manual_login",
            state.hub_api_key.as_deref(),
            &payload,
            Some(MANUAL_LOGIN_TIMEOUT),
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
        .call_json_post_with_timeout(
            &provider_base_url(provider),
            "/admin/close_login",
            state.hub_api_key.as_deref(),
            &json!({}),
            Some(MANUAL_LOGIN_TIMEOUT),
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

    state
        .ensure_provider_ready("qwen")
        .await
        .map_err(|err| err.to_string())?;

    let (status, response) = state
        .call_json_post_with_timeout(
            &provider_base_url("qwen"),
            "/admin/manual_login",
            state.hub_api_key.as_deref(),
            &payload,
            Some(MANUAL_LOGIN_TIMEOUT),
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
        .call_json_post_with_timeout(
            &provider_base_url("qwen"),
            "/admin/close_login",
            state.hub_api_key.as_deref(),
            &json!({ "account_id": account_id.clone() }),
            Some(MANUAL_LOGIN_TIMEOUT),
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
    matches!(provider, "qwen" | "deepseek" | "chatgpt")
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
            provider_models,
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

