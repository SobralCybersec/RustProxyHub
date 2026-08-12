pub async fn serve_browser_provider(config: BrowserProviderServerConfig) -> Result<()> {
    crate::proxy_core::enforce_loopback_guard(&config.host, config.api_key.as_deref())?;
    tokio::fs::create_dir_all(&config.runtime_dir).await?;

    let bridge = Arc::new(
        PlaywrightBridge::new_with_node(
            &config.helper_dir,
            config.node_path.clone(),
            config.kind.as_str(),
        )
        .await?,
    );

    let state = AppState {
        bridge,
        config,
        models_cache: Arc::new(tokio::sync::RwLock::new(None)),
    };
    let app = Router::new()
        .route("/health", get(health))
        .route("/admin/manual_login", post(admin_manual_login))
        .route("/admin/close_login", post(admin_close_login))
        .route("/admin/logs", get(admin_logs))
        .route("/v1", get(v1_root))
        .route("/v1/models", get(models))
        .route("/v1/models/{model}", get(model_by_id))
        .route("/v1/chat/completions", post(chat_completions))
        .route("/v1/responses", post(responses))
        .route("/v1/messages", post(anthropic_messages))
        .route("/v1/messages/count_tokens", post(anthropic_count_tokens))
        .layer(DefaultBodyLimit::max(100 * 1024 * 1024))
        .with_state(state.clone());

    let host: IpAddr = state
        .config
        .host
        .parse()
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    let addr = SocketAddr::new(host, state.config.port);
    println!(
        "{} browser provider listening on http://{}",
        state.config.kind.as_str(),
        addr
    );
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "provider": state.config.kind.as_str(),
        "web_search_supported": state.config.kind.web_search_supported(),
    }))
}

async fn v1_root(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref()) {
        return *response;
    }

    Json(json!({
        "object": "api",
        "provider": state.config.kind.as_str(),
        "base_url": format!("http://{}:{}/v1", state.config.host, state.config.port),
        "routes": {
            "models": "/v1/models",
            "chat_completions": "/v1/chat/completions",
            "responses": "/v1/responses",
            "anthropic_messages": "/v1/messages",
            "anthropic_count_tokens": "/v1/messages/count_tokens",
        }
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

    match state
        .bridge
        .manual_login(ManualLoginParams {
            runtime_dir: state.config.runtime_dir.to_string_lossy().to_string(),
            browser: body.browser.unwrap_or_else(|| state.config.browser.clone()),
            account_id: None,
        })
        .await
    {
        Ok(()) => Json(json!({
            "ok": true,
            "provider": state.config.kind.as_str(),
        }))
        .into_response(),
        Err(err) => provider_error(&state, err),
    }
}

async fn admin_close_login(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref()) {
        return *response;
    }

    match state.bridge.shutdown().await {
        Ok(()) => Json(json!({
            "ok": true,
            "provider": state.config.kind.as_str(),
        }))
        .into_response(),
        Err(err) => provider_error(&state, err),
    }
}

async fn admin_logs(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref()) {
        return *response;
    }

    let entries = tokio::fs::read_to_string(state.bridge.log_path())
        .await
        .unwrap_or_default()
        .lines()
        .rev()
        .take(160)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    Json(json!({
        "provider": state.config.kind.as_str(),
        "entries": entries,
    }))
    .into_response()
}

async fn models(
    State(state): State<AppState>,
    Query(query): Query<ModelListQuery>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref()) {
        return *response;
    }

    Json(discover_models(&state, query.chatgpt_mode.as_deref()).await).into_response()
}

async fn model_by_id(
    State(state): State<AppState>,
    Path(model): Path<String>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref()) {
        return *response;
    }

    let payload = discover_models(&state, None).await;
    let found = payload
        .get("data")
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find(|item| item.get("id").and_then(Value::as_str) == Some(model.as_str()))
        })
        .cloned();

    match found {
        Some(item) => Json(item).into_response(),
        None => json_error(StatusCode::NOT_FOUND, "Model not found".to_owned()),
    }
}

async fn discover_models(state: &AppState, chatgpt_mode: Option<&str>) -> Value {
    const TTL: Duration = Duration::from_secs(30);

    // One payload per provider; auto is same default lane as an omitted mode.
    let cacheable = chatgpt_mode.is_none() || chatgpt_mode == Some("auto");

    // serve fresh cache without hitting the bridge
    if cacheable {
        let cached = state.models_cache.read().await;
        if let Some((ref val, ts)) = *cached {
            if ts.elapsed() < TTL {
                return val.clone();
            }
        }
    }

    // hold stale value so a timeout/error doesn't leave callers empty-handed
    let stale = {
        let cached = state.models_cache.read().await;
        cached.as_ref().map(|(v, _)| v.clone())
    };

    let discovered = tokio::time::timeout(model_discovery_timeout(state.config.kind), async {
        ensure_headless_ready(state).await?;
        state
            .bridge
            .request::<_, Value>(
                "list_models",
                json!({
                    "fallback_model": state.config.kind.default_model(),
                    "chatgpt_mode": chatgpt_mode.unwrap_or("auto"),
                }),
            )
            .await
    })
    .await;

    match discovered {
        Ok(Ok(payload)) => {
            let result = normalize_model_payload(state, payload, Vec::new(), chatgpt_mode);
            if cacheable {
                *state.models_cache.write().await =
                    Some((result.clone(), std::time::Instant::now()));
            }
            result
        }
        Ok(Err(err)) => {
            stale.unwrap_or_else(|| fallback_model_payload(state, vec![err.to_string()]))
        }
        Err(_) => stale.unwrap_or_else(|| {
            fallback_model_payload(
                state,
                vec![format!(
                    "{} model discovery timed out; using fallback model",
                    state.config.kind.as_str()
                )],
            )
        }),
    }
}

fn model_discovery_timeout(kind: BrowserProviderKind) -> Duration {
    match kind {
        BrowserProviderKind::Chatgpt => Duration::from_secs(15),
        _ => Duration::from_secs(4),
    }
}

fn normalize_model_payload(
    state: &AppState,
    payload: Value,
    errors: Vec<String>,
    chatgpt_mode: Option<&str>,
) -> Value {
    let mut seen = std::collections::HashSet::new();
    let mut data = Vec::new();
    if let Some(items) = payload.get("data").and_then(Value::as_array) {
        for item in items {
            let id = item
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim();
            if id.is_empty()
                || !model_id_allowed(state.config.kind, id)
                || !seen.insert(id.to_owned())
            {
                continue;
            }
            data.push(model_payload_item(state.config.kind, id, item.clone()));
        }
    }

    if data.is_empty() {
        return fallback_model_payload_for_mode(state.config.kind, chatgpt_mode, errors);
    }

    let discovery = payload
        .get("discovery")
        .cloned()
        .unwrap_or_else(|| model_discovery_metadata(state.config.kind, chatgpt_mode));

    json!({
        "object": "list",
        "data": data,
        "errors": errors,
        "discovery": discovery,
    })
}

fn model_payload_item(kind: BrowserProviderKind, id: &str, source: Value) -> Value {
    let mut object = source.as_object().cloned().unwrap_or_default();
    let metadata = model_metadata(kind, id);
    for (key, value) in metadata.as_object().into_iter().flatten() {
        object.entry(key.clone()).or_insert_with(|| value.clone());
    }
    object.insert("id".to_owned(), Value::String(id.to_owned()));
    object.insert("object".to_owned(), Value::String("model".to_owned()));
    object.insert("created".to_owned(), json!(current_timestamp()));
    object.insert(
        "owned_by".to_owned(),
        Value::String(kind.as_str().to_owned()),
    );
    object
        .entry("permission".to_owned())
        .or_insert_with(|| json!([]));
    object.insert("root".to_owned(), Value::String(id.to_owned()));
    object.entry("parent".to_owned()).or_insert(Value::Null);
    Value::Object(object)
}

fn model_metadata(kind: BrowserProviderKind, id: &str) -> Value {
    let lower = id.to_ascii_lowercase();
    let is_codex = matches!(kind, BrowserProviderKind::Chatgpt) && lower.contains("codex");
    let is_chatgpt = matches!(kind, BrowserProviderKind::Chatgpt);
    let is_gpt_family = is_chatgpt
        && (lower.starts_with("gpt") || lower.starts_with('o') || lower.starts_with("chatgpt"));
    let api = if is_codex {
        "codex_responses"
    } else {
        "chat_completions"
    };
    let billing = if is_codex {
        "Codex billing usage"
    } else if is_chatgpt {
        "ChatGPT subscription/web-session usage"
    } else {
        "Provider session usage"
    };
    let description = if is_codex {
        "Uses Codex OAuth Responses API; usage is billed/limited as Codex usage."
    } else if is_chatgpt {
        "Uses Chat Completions API compatibility through the ChatGPT web session."
    } else {
        "Uses Chat Completions API compatibility through the browser-backed provider session."
    };

    json!({
        "name": model_display_name(id),
        "description": description,
        "api": api,
        "billing": billing,
        "tool_call": is_gpt_family || !is_chatgpt,
        "reasoning": lower.contains("reasoning") || lower.contains("thinking") || lower.contains("codex") || lower.starts_with('o'),
        "temperature": !(lower.contains("codex") || lower.starts_with('o')),
        "attachment": false,
        "limit": {
            "context": 128000,
            "output": 16384
        }
    })
}

fn model_display_name(id: &str) -> String {
    id.split(['-', '_', '.'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(_) if part.len() <= 3 => part.to_ascii_uppercase(),
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn fallback_model_payload(state: &AppState, errors: Vec<String>) -> Value {
    fallback_model_payload_for_mode(state.config.kind, None, errors)
}

#[cfg(test)]
fn fallback_model_payload_for(kind: BrowserProviderKind, errors: Vec<String>) -> Value {
    fallback_model_payload_for_mode(kind, None, errors)
}

fn fallback_model_payload_for_mode(
    kind: BrowserProviderKind,
    chatgpt_mode: Option<&str>,
    errors: Vec<String>,
) -> Value {
    let fallback = kind.default_model();
    json!({
        "object": "list",
        "data": [model_payload_item(kind, fallback, json!({}))],
        "errors": errors,
        "discovery": model_discovery_metadata(kind, chatgpt_mode),
    })
}

fn model_id_allowed(kind: BrowserProviderKind, id: &str) -> bool {
    !(kind == BrowserProviderKind::Chatgpt
        && id
            .to_ascii_lowercase()
            .starts_with("chatgpt.workspace.model."))
}

fn model_discovery_metadata(kind: BrowserProviderKind, chatgpt_mode: Option<&str>) -> Value {
    if kind == BrowserProviderKind::Chatgpt && chatgpt_mode == Some("codex") {
        return json!({
            "provider": "codex",
            "source": "oauth",
            "api": "codex_responses",
            "endpoint": "/backend-api/codex/models",
            "request_endpoint": "/v1/models?chatgpt_mode=codex",
        });
    }

    if kind == BrowserProviderKind::Chatgpt {
        return json!({
            "provider": "chatgpt",
            "source": "playwright",
            "api": "chat_completions",
            "endpoints": [
                "/backend-api/models",
                "/backend-api/f/models",
                "/backend-api/model_slug_availability",
            ],
            "request_endpoint": "/v1/models?chatgpt_mode=auto",
        });
    }

    json!({
        "provider": kind.as_str(),
        "source": "fallback",
        "request_endpoint": "/v1/models",
    })
}

async fn chat_completions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<OpenAIRequest>,
) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref()) {
        return *response;
    }

    if let Err(err) = ensure_headless_ready(&state).await {
        return provider_error(&state, err);
    }

    if live_chatgpt_web_stream_requested(state.config.kind, &body) {
        return match request_browser_chat_stream(&state, &body).await {
            Ok(stream) => stream_browser_chat_live(state, body, stream),
            Err(err) => provider_error(&state, err),
        };
    }

    match request_browser_chat(&state, &body).await {
        Ok(chat) if body.stream.unwrap_or(false) => stream_browser_chat(state, body, chat),
        Ok(chat) => json_browser_chat(state, body, chat),
        Err(err) => provider_error(&state, err),
    }
}

async fn responses(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref()) {
        return *response;
    }

    let request = match openai_responses_to_request(&state, &body) {
        Ok(request) => request,
        Err(err) => return json_error(StatusCode::BAD_REQUEST, err.to_string()),
    };

    if let Err(err) = ensure_headless_ready(&state).await {
        return provider_error(&state, err);
    }

    match request_browser_chat(&state, &request).await {
        Ok(chat) if request.stream.unwrap_or(false) => stream_openai_response(state, request, chat),
        Ok(chat) => json_openai_response(state, request, chat),
        Err(err) => provider_error(&state, err),
    }
}

async fn anthropic_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref()) {
        return *response;
    }

    let request = match anthropic_to_openai_request(&state, &body) {
        Ok(request) => request,
        Err(err) => return json_error(StatusCode::BAD_REQUEST, err.to_string()),
    };

    if let Err(err) = ensure_headless_ready(&state).await {
        return provider_error(&state, err);
    }

    match request_browser_chat(&state, &request).await {
        Ok(chat) if request.stream.unwrap_or(false) => {
            stream_anthropic_response(state, request, chat)
        }
        Ok(chat) => json_anthropic_response(state, request, chat),
        Err(err) => provider_error(&state, err),
    }
}

async fn anthropic_count_tokens(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref()) {
        return *response;
    }

    let request = match anthropic_to_openai_request(&state, &body) {
        Ok(request) => request,
        Err(err) => return json_error(StatusCode::BAD_REQUEST, err.to_string()),
    };
    let usage = usage_from_text(&build_prompt(&request), "", false);

    Json(json!({
        "input_tokens": usage.prompt_tokens
    }))
    .into_response()
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

async fn request_browser_chat(
    state: &AppState,
    body: &OpenAIRequest,
) -> Result<BridgeChatResponse> {
    let model = browser_request_model(state.config.kind, &body.model);

    let preflight = browser_request_preflight(body)?;
    state
        .bridge
        .request(
            "chat",
            json!({
                "model": model,
                "prompt": preflight.conversation,
                "system_prompt": if preflight.system_prompt.is_empty() { serde_json::Value::Null } else { serde_json::json!(preflight.system_prompt) },
                "web_search": body.web_search.unwrap_or(false),
                "chatgpt_mode": body.chatgpt_mode.as_deref().unwrap_or("auto"),
                "session_id": body.user.as_deref(),
                "stream": body.stream.unwrap_or(false),
            }),
        )
        .await
}

async fn request_browser_chat_stream(
    state: &AppState,
    body: &OpenAIRequest,
) -> Result<BridgeStream> {
    let model = browser_request_model(state.config.kind, &body.model);
    let preflight = browser_request_preflight(body)?;
    state
        .bridge
        .request_stream(
            "chat",
            json!({
                "model": model,
                "prompt": preflight.conversation,
                "system_prompt": if preflight.system_prompt.is_empty() { serde_json::Value::Null } else { serde_json::json!(preflight.system_prompt) },
                "web_search": body.web_search.unwrap_or(false),
                "chatgpt_mode": body.chatgpt_mode.as_deref().unwrap_or("auto"),
                "session_id": body.user.as_deref(),
                "stream": true,
            }),
        )
        .await
}

fn live_chatgpt_web_stream_requested(kind: BrowserProviderKind, body: &OpenAIRequest) -> bool {
    if kind != BrowserProviderKind::Chatgpt || !body.stream.unwrap_or(false) {
        return false;
    }
    match body.chatgpt_mode.as_deref() {
        Some("web") => true,
        Some("codex") => false,
        _ => {
            let model = body.model.to_ascii_lowercase();
            model == "chatgpt-web-session" || model.contains("web-session")
        }
    }
}

fn browser_request_model(kind: BrowserProviderKind, requested: &str) -> String {
    if requested.trim().is_empty() {
        kind.default_model().to_owned()
    } else {
        requested.to_owned()
    }
}

fn browser_request_preflight(body: &OpenAIRequest) -> Result<PromptPreflight> {
    preflight_request_to_budget(
        body,
        &PromptPreflightOptions {
            max_prompt_tokens: None,
            extra_system_instructions: browser_tool_system_instructions(body),
            dedup_system_blocks: false,
            structured_compaction_max_chars: Some(18_000),
        },
        crate::proxy_core::estimate_tokens,
    )
}

fn json_browser_chat(state: AppState, body: OpenAIRequest, chat: BridgeChatResponse) -> Response {
    let completion_id = format!("chatcmpl-{}", Uuid::new_v4());
    let model = chat.model.clone().unwrap_or_else(|| body.model.clone());
    let parsed = parse_browser_output(&body, &chat.text);
    let rendered_prompt = browser_request_preflight(&body)
        .map(|preflight| preflight.flat_prompt)
        .unwrap_or_else(|_| build_prompt(&body));
    let usage = usage_from_text(&rendered_prompt, &parsed.text, true);
    let provider_warnings = build_provider_warnings(&state.config.kind, &body, &chat);
    let finish_reason = if parsed.tool_calls.is_empty() {
        "stop"
    } else {
        "tool_calls"
    };

    Json(json!({
        "id": completion_id,
        "object": "chat.completion",
        "created": current_timestamp(),
        "model": model,
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": if parsed.tool_calls.is_empty() {
                    Value::String(parsed.text.clone())
                } else if parsed.text.trim().is_empty() {
                    Value::Null
                } else {
                    Value::String(parsed.text.clone())
                },
                "reasoning_content": chat.reasoning_content,
                "tool_calls": if parsed.tool_calls.is_empty() {
                    Value::Null
                } else {
                    json!(parsed.tool_calls)
                },
            },
            "logprobs": Value::Null,
            "finish_reason": finish_reason,
        }],
        "usage": usage,
        "provider_metadata": browser_provider_metadata(&state.config.kind, &chat),
        "provider_warnings": provider_warnings,
    }))
    .into_response()
}

