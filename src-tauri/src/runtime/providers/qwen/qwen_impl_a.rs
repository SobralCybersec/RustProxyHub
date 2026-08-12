async fn run_server(
    bridge: Arc<PlaywrightBridge>,
    config: AppConfig,
    runtime: ServerRuntime,
) -> Result<()> {
    let state = AppState {
        bridge,
        client: reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(config.chat_timeout)
            .redirect(reqwest::redirect::Policy::none())
            .tcp_nodelay(true)
            .tcp_keepalive(Duration::from_secs(30))
            .pool_idle_timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(16)
            .build()?,
        config: config.clone(),
        accounts: runtime.accounts,
        account_manager: AccountManager::new(),
        model_registry: runtime.model_registry,
        metrics: runtime.metrics,
        cache: runtime.cache,
        conversations: runtime.conversations,
        stream_registry: runtime.stream_registry,
        traces: QwenTraceStore::default(),
        watchdog: runtime.watchdog,
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics_route))
        .route("/admin/status", get(admin_status))
        .route("/admin/logs", get(admin_logs))
        .route("/admin/manual_login", post(admin_manual_login))
        .route("/admin/close_login", post(admin_close_login))
        .route("/v1/models", get(models))
        .route("/v1/models/{model}", get(model_by_id))
        .route("/v1/chat/completions", post(chat_completions))
        .route("/v1/chat/completions/stop", post(chat_completions_stop))
        .route("/v1/upload", post(upload_file))
        .layer(axum::extract::DefaultBodyLimit::max(100 * 1024 * 1024))
        .with_state(state);

    let host: IpAddr = config
        .host
        .parse()
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    crate::proxy_core::enforce_loopback_guard(&config.host, config.api_key.as_deref())?;
    let addr = SocketAddr::new(host, config.port);
    println!("proxy-hub qwen listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health(State(state): State<AppState>) -> Response {
    let payload = json!({
        "status": "ok",
        "accounts": state.accounts.count().unwrap_or(0),
        "streams_active": state.stream_registry.active_count().await,
        "watchdog": state.watchdog.snapshot().await,
        "cache": state.cache.stats().await,
    });
    Json(payload).into_response()
}

async fn metrics_route(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref()) {
        return *response;
    }
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        state.metrics.format_prometheus().await,
    )
        .into_response()
}

async fn admin_status(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref()) {
        return *response;
    }

    match state.accounts.list_masked_accounts() {
        Ok(accounts) => Json(json!({
            "provider": "qwen",
            "accounts": accounts,
            "cooldowns": state.account_manager.cooldown_status().await,
            "active_accounts": state.account_manager.active_status().await,
            "active_streams": state.stream_registry.snapshots().await,
            "cache": state.cache.stats().await,
            "watchdog": state.watchdog.snapshot().await,
            "metrics": state.metrics.snapshot_json().await,
        }))
        .into_response(),
        Err(err) => json_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn admin_logs(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref()) {
        return *response;
    }

    Json(json!({
        "provider": "qwen",
        "entries": state.traces.snapshot().await,
    }))
    .into_response()
}

async fn admin_manual_login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ManualLoginRequest>,
) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref()) {
        return *response;
    }

    if let Some(ref account_id) = body.account_id {
        if !crate::proxy_core::is_safe_account_id(account_id) {
            return json_error(StatusCode::BAD_REQUEST, "invalid account_id".to_owned());
        }
    }

    match state
        .bridge
        .manual_login(ManualLoginParams {
            runtime_dir: state.config.runtime_dir.to_string_lossy().to_string(),
            browser: body.browser.unwrap_or_else(|| state.config.browser.clone()),
            account_id: body.account_id,
        })
        .await
    {
        Ok(()) => Json(json!({ "ok": true, "provider": "qwen" })).into_response(),
        Err(err) => bad_gateway_error(err),
    }
}

async fn admin_close_login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CloseLoginRequest>,
) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref()) {
        return *response;
    }

    let Some(account_id) = body.account_id else {
        return match state.bridge.shutdown().await {
            Ok(()) => Json(json!({ "ok": true, "provider": "qwen" })).into_response(),
            Err(err) => bad_gateway_error(err),
        };
    };
    if !crate::proxy_core::is_safe_account_id(&account_id) {
        return json_error(StatusCode::BAD_REQUEST, "invalid account_id".to_owned());
    }

    match state
        .bridge
        .close_account(CloseAccountParams { account_id })
        .await
    {
        Ok(()) => Json(json!({ "ok": true, "provider": "qwen" })).into_response(),
        Err(err) => bad_gateway_error(err),
    }
}

async fn models(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref()) {
        return *response;
    }

    match fetch_models(&state).await {
        Ok(models) => Json(models).into_response(),
        Err(err) => json_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn model_by_id(
    State(state): State<AppState>,
    Path(model): Path<String>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref()) {
        return *response;
    }

    match fetch_models(&state).await {
        Ok(payload) => {
            if let Some(found) = payload
                .get("data")
                .and_then(Value::as_array)
                .and_then(|items| {
                    items
                        .iter()
                        .find(|item| item.get("id").and_then(Value::as_str) == Some(model.as_str()))
                })
            {
                Json(found.clone()).into_response()
            } else {
                json_error(StatusCode::NOT_FOUND, "Model not found".to_owned())
            }
        }
        Err(err) => json_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn upload_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref()) {
        return *response;
    }

    let mut upload_name: Option<String> = None;
    let mut upload_type: Option<String> = None;
    let mut upload_bytes: Option<Vec<u8>> = None;
    loop {
        match multipart.next_field().await {
            Ok(Some(field)) => {
                if field.name() == Some("file") {
                    upload_name = field
                        .file_name()
                        .map(str::to_owned)
                        .filter(|value| !value.trim().is_empty());
                    upload_type = field.content_type().map(str::to_owned);
                    upload_bytes = match field.bytes().await {
                        Ok(bytes) => Some(bytes.to_vec()),
                        Err(err) => {
                            return json_error(StatusCode::BAD_REQUEST, err.to_string());
                        }
                    };
                    break;
                }
            }
            Ok(None) => break,
            Err(err) => return json_error(StatusCode::BAD_REQUEST, err.to_string()),
        }
    }

    let Some(bytes) = upload_bytes else {
        return json_error(StatusCode::BAD_REQUEST, "No file provided".to_owned());
    };

    let filename = upload_name.unwrap_or_else(|| format!("upload-{}.bin", Uuid::new_v4()));
    let content_type = upload_type;

    match pick_capture_headers_for_aux_request(&state).await {
        Ok((_, qwen_headers)) => match upload_bytes_to_qwen(
            &state.client,
            &state.config.qwen_base_url,
            &qwen_headers,
            filename,
            content_type.as_deref(),
            bytes,
        )
        .await
        {
            Ok(payload) => Json(json!({
                "url": payload.url,
                "file_id": payload.file_id,
                "filename": payload.filename,
                "type": payload.media_type,
                "qwen_file": payload.qwen_file,
            }))
            .into_response(),
            Err(err) => json_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        },
        Err(err) => json_error(StatusCode::SERVICE_UNAVAILABLE, err.to_string()),
    }
}

async fn chat_completions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<OpenAIRequest>,
) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref()) {
        return *response;
    }

    state.metrics.increment("requests.total", 1.0).await;
    let started = std::time::Instant::now();
    if let Err(err) = ensure_headless_ready(&state).await {
        state.metrics.increment("requests.errors", 1.0).await;
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
    }

    let response = match handle_chat(state.clone(), body).await {
        Ok(response) => response,
        Err(err) => {
            state.metrics.increment("requests.errors", 1.0).await;
            // ponytail: other error sites (admin/models/upload) still call json_error
            // with err.to_string(); migrate them to bad_gateway_error in the DRY pass.
            bad_gateway_error(err)
        }
    };

    state
        .metrics
        .histogram("latency.request", started.elapsed().as_millis() as f64)
        .await;
    response
}

async fn chat_completions_stop(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<StopRequest>,
) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref()) {
        return *response;
    }

    let target = if let Some(completion_id) = body.completion_id.as_deref() {
        state
            .stream_registry
            .get_by_completion_id(completion_id)
            .await
    } else if let Some(chat_id) = body.chat_id.as_deref() {
        state.stream_registry.get_by_chat_id(chat_id).await
    } else {
        None
    };

    let Some(target) = target else {
        return json_error(StatusCode::NOT_FOUND, "Stream not found".to_owned());
    };

    let response_id = body
        .response_id
        .or_else(|| target.snapshot.response_id.clone());
    let Some(response_id) = response_id else {
        return json_error(
            StatusCode::BAD_REQUEST,
            "response_id is required".to_owned(),
        );
    };

    if let Some(expected) = target.snapshot.response_id.as_deref() {
        if expected != response_id {
            return json_error(StatusCode::BAD_REQUEST, "response_id mismatch".to_owned());
        }
    }

    match stop_upstream_generation(&state, &target, &response_id).await {
        Ok(()) => {
            target.cancel.cancel();
            state
                .stream_registry
                .remove_by_completion_id(&target.snapshot.completion_id)
                .await;
            state
                .metrics
                .gauge(
                    "streams.active",
                    state.stream_registry.active_count().await as f64,
                )
                .await;
            Json(json!({ "success": true })).into_response()
        }
        Err(err) => json_error(StatusCode::BAD_GATEWAY, err.to_string()),
    }
}

async fn fetch_models(state: &AppState) -> Result<Value> {
    // Version cache key so newly registered models are not hidden by an older
    // process-local catalogue after the registry changes.
    const CACHE_KEY: &str = "models:qwen:v3";
    if let Some(cached) = state.cache.get_json(CACHE_KEY).await {
        return Ok(cached);
    }

    let payload = match pick_capture_headers_for_aux_request(state).await {
        Ok((_, headers)) => match fetch_live_models_payload(state, &headers).await {
            Ok(payload) => payload,
            Err(err) => {
                fallback_models_payload(
                    state,
                    format!("Qwen live model catalog unavailable: {err}"),
                )
                .await
            }
        },
        Err(err) => {
            fallback_models_payload(state, format!("Qwen live model catalog unavailable: {err}"))
                .await
        }
    };
    state
        .cache
        .set_json(
            CACHE_KEY,
            payload.clone(),
            Some(state.config.cache.response_ttl),
        )
        .await;
    Ok(payload)
}

async fn fetch_live_models_payload(
    state: &AppState,
    headers: &HashMap<String, String>,
) -> Result<Value> {
    let response = state
        .client
        .get(format!("{}/api/models", state.config.qwen_base_url))
        .header("accept", "application/json, text/plain, */*")
        .header("accept-language", "pt-BR,pt;q=0.9")
        .header("cookie", headers.get("cookie").cloned().unwrap_or_default())
        .header("referer", format!("{}/", state.config.qwen_base_url))
        .header(
            "user-agent",
            headers.get("user-agent").cloned().unwrap_or_default(),
        )
        .header("x-request-id", Uuid::new_v4().to_string())
        .header("bx-v", headers.get("bx-v").cloned().unwrap_or_default())
        .header("bx-ua", headers.get("bx-ua").cloned().unwrap_or_default())
        .header(
            "bx-umidtoken",
            headers.get("bx-umidtoken").cloned().unwrap_or_default(),
        )
        .header("timezone", "UTC")
        .header("version", QWEN_WEB_VERSION)
        .header("source", "web")
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(anyhow!(
            "Qwen live model request failed: {} {}",
            response.status(),
            response.text().await.unwrap_or_default()
        ));
    }
    live_qwen_model_data(&response.json::<Value>().await?)
}

fn live_qwen_model_data(payload: &Value) -> Result<Value> {
    if let Some(error) = extract_qwen_api_error(payload) {
        return Err(anyhow!("Qwen live model request failed: {error}"));
    }
    let source_models = payload
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("Qwen live model response contained no data array"))?;
    let mut ids = HashSet::new();
    let mut models = Vec::new();
    for source in source_models {
        let Some(id) = source
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
        else {
            continue;
        };
        let mut base = source.clone();
        if !base.is_object() {
            base = json!({});
        }
        base["id"] = Value::String(id.to_owned());
        base["object"] = Value::String("model".to_owned());
        base["owned_by"] = Value::String("qwen".to_owned());
        base["created"] = Value::Number(current_timestamp().into());
        for variant in [
            id.to_owned(),
            format!("{id}-thinking"),
            format!("{id}-no-thinking"),
        ] {
            if ids.insert(variant.clone()) {
                let mut model = base.clone();
                model["id"] = Value::String(variant);
                models.push(model);
            }
        }
    }
    if models.is_empty() {
        return Err(anyhow!("Qwen live model response contained no model ids"));
    }
    Ok(json!({
        "object": "list",
        "data": models,
        "source": "upstream",
        "discovery": {
            "provider": "qwen",
            "source": "upstream",
            "api": "chat.qwen.ai/api/models",
            "endpoint": "/v1/models",
            "live": true,
            "catalogue_fields": ["data"],
        },
    }))
}

async fn ensure_headless_ready(state: &AppState) -> Result<()> {
    state
        .bridge
        .init(InitParams {
            runtime_dir: state.config.runtime_dir.to_string_lossy().to_string(),
            headless: state.config.headless,
            browser: state.config.browser.clone(),
        })
        .await
}

async fn fallback_models_payload(state: &AppState, warning: String) -> Value {
    let data = state
        .model_registry
        .fallback_catalog()
        .await
        .into_iter()
        .map(|(id, context_window)| {
            let name = if let Some(base) = id.strip_suffix("-no-thinking") {
                format!("{base} (No Thinking)")
            } else {
                id.clone()
            };
            json!({
                "id": id,
                "name": name,
                "object": "model",
                "owned_by": "qwen",
                "created": current_timestamp(),
                "context_window": context_window,
                "capabilities": Value::Null,
            })
        })
        .collect::<Vec<_>>();

    json!({
        "object": "list",
        "data": data,
        "source": "registry",
        "discovery": {
            "provider": "qwen",
            "source": "registry",
            "api": "chat_completions",
            "endpoint": "/v1/models",
            "live": false,
            "catalogue_fields": ["data"],
        },
        "warning": warning,
    })
}

