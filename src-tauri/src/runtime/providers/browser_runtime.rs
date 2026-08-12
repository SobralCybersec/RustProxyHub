use crate::browser_bridge::{
    BridgeStream, BrowserBridge, InitParams, ManualLoginParams, PlaywrightBridge,
};
use crate::proxy_core::{
    build_prompt, constant_time_eq, current_timestamp, extract_tool_calls_from_text,
    preflight_request_to_budget, sse_done, sse_json, usage_from_text, FunctionToolDefinition,
    Message, MessageToolCall, OpenAIRequest, PromptPreflight, PromptPreflightOptions,
    StreamingToolParser, ToolCallFunction, Usage,
};
use anyhow::Result;
use async_stream::stream;
use axum::{
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BrowserProviderKind {
    Chatgpt,
    Gemini,
    Mistral,
    Zai,
    Meta,
}

impl BrowserProviderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chatgpt => "chatgpt",
            Self::Gemini => "gemini",
            Self::Mistral => "mistral",
            Self::Zai => "zai",
            Self::Meta => "meta",
        }
    }

    fn default_model(self) -> &'static str {
        match self {
            Self::Chatgpt => "gpt-5.4-mini",
            Self::Gemini => "gemini-web-session",
            Self::Mistral => "mistral-web-session",
            Self::Zai => "glm-5.2",
            Self::Meta => "meta-ai-web-session",
        }
    }

    fn web_search_supported(self) -> bool {
        matches!(self, Self::Chatgpt)
    }
}

#[derive(Clone)]
pub struct BrowserProviderServerConfig {
    pub kind: BrowserProviderKind,
    pub host: String,
    pub port: u16,
    pub api_key: Option<String>,
    pub headless: bool,
    pub browser: String,
    pub runtime_dir: PathBuf,
    pub helper_dir: PathBuf,
    pub node_path: Option<PathBuf>,
}

#[derive(Clone)]
struct AppState {
    bridge: Arc<PlaywrightBridge>,
    config: BrowserProviderServerConfig,
    models_cache: Arc<tokio::sync::RwLock<Option<(Value, std::time::Instant)>>>,
}

#[derive(Debug, Deserialize)]
struct ManualLoginRequest {
    #[serde(default)]
    browser: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ModelListQuery {
    #[serde(default)]
    chatgpt_mode: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BridgeChatResponse {
    text: String,
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    conversation_id: Option<String>,
    #[serde(default)]
    warning: Option<String>,
    #[serde(default)]
    upstream_usage: Option<Value>,
    #[serde(default)]
    upstream_cache: Option<Value>,
}

#[derive(Debug)]
struct ParsedBrowserOutput {
    text: String,
    tool_calls: Vec<MessageToolCall>,
}

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

fn stream_browser_chat(state: AppState, body: OpenAIRequest, chat: BridgeChatResponse) -> Response {
    let completion_id = format!("chatcmpl-{}", Uuid::new_v4());
    let model = chat.model.clone().unwrap_or_else(|| body.model.clone());
    let prompt = browser_request_preflight(&body)
        .map(|preflight| preflight.flat_prompt)
        .unwrap_or_else(|_| build_prompt(&body));
    let provider_warnings = build_provider_warnings(&state.config.kind, &body, &chat);
    let parsed = parse_browser_output(&body, &chat.text);
    let chunks = split_text_chunks(&parsed.text, 320);
    let tool_calls = parsed.tool_calls.clone();
    let finish_reason = if tool_calls.is_empty() {
        "stop"
    } else {
        "tool_calls"
    };

    let stream = stream! {
        yield Ok::<Bytes, std::convert::Infallible>(sse_json(json!({
            "id": completion_id,
            "object": "chat.completion.chunk",
            "created": current_timestamp(),
            "model": model,
            "choices": [{
                "index": 0,
                "delta": {
                    "role": "assistant",
                    "content": "",
                    "reasoning_content": chat.reasoning_content,
                },
                "logprobs": Value::Null,
                "finish_reason": Value::Null,
            }],
            "provider_warnings": provider_warnings,
        })));

        for chunk in chunks {
            if chunk.is_empty() {
                continue;
            }
            yield Ok(sse_json(json!({
                "id": completion_id,
                "object": "chat.completion.chunk",
                "created": current_timestamp(),
                "model": model,
                "choices": [{
                    "index": 0,
                    "delta": { "content": chunk },
                    "logprobs": Value::Null,
                    "finish_reason": Value::Null,
                }],
            })));
        }

        if !tool_calls.is_empty() {
            yield Ok(sse_json(json!({
                "id": completion_id,
                "object": "chat.completion.chunk",
                "created": current_timestamp(),
                "model": model,
                "choices": [{
                    "index": 0,
                    "delta": { "tool_calls": tool_calls },
                    "logprobs": Value::Null,
                    "finish_reason": Value::Null,
                }],
            })));
        }

        yield Ok(sse_json(json!({
            "id": completion_id,
            "object": "chat.completion.chunk",
            "created": current_timestamp(),
            "model": model,
            "choices": [{
                "index": 0,
                "delta": {},
                "logprobs": Value::Null,
                "finish_reason": finish_reason,
            }],
            "usage": usage_from_text(&prompt, &parsed.text, true),
            "provider_metadata": browser_provider_metadata(&state.config.kind, &chat),
        })));
        yield Ok(sse_done());
    };

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/event-stream"),
            (header::CACHE_CONTROL, "no-cache"),
            (header::CONNECTION, "keep-alive"),
            (header::HeaderName::from_static("x-accel-buffering"), "no"),
        ],
        axum::body::Body::from_stream(stream),
    )
        .into_response()
}

fn stream_browser_chat_live(
    state: AppState,
    body: OpenAIRequest,
    mut bridge_stream: BridgeStream,
) -> Response {
    let completion_id = format!("chatcmpl-{}", Uuid::new_v4());
    let model = browser_request_model(state.config.kind, &body.model);
    let prompt = browser_request_preflight(&body)
        .map(|preflight| preflight.flat_prompt)
        .unwrap_or_else(|_| build_prompt(&body));

    let stream = stream! {
        yield Ok::<Bytes, std::convert::Infallible>(sse_json(json!({
            "id": completion_id,
            "object": "chat.completion.chunk",
            "created": current_timestamp(),
            "model": model,
            "choices": [{
                "index": 0,
                "delta": { "role": "assistant", "content": "" },
                "logprobs": Value::Null,
                "finish_reason": Value::Null,
            }],
        })));

        let mut parser = StreamingToolParser::new();
        let defer_text_until_final = body.tools.is_some();
        let mut emitted_text = String::new();
        let mut emitted_reasoning = String::new();
        while let Some(event) = bridge_stream.next_event().await {
            match event {
                Ok(event) if event.event == "delta" => {
                    if let Some(delta) = event.payload.get("delta").and_then(Value::as_str) {
                        let parsed = parser.feed(delta);
                        if !parsed.text.is_empty() && !defer_text_until_final {
                                emitted_text.push_str(&parsed.text);
                                yield Ok(sse_json(json!({
                                    "id": completion_id,
                                    "object": "chat.completion.chunk",
                                    "created": current_timestamp(),
                                    "model": model,
                                    "choices": [{
                                        "index": 0,
                                        "delta": { "content": parsed.text },
                                        "logprobs": Value::Null,
                                        "finish_reason": Value::Null,
                                    }],
                                })));
                        }
                    }
                }
                Ok(event) if event.event == "reasoning" => {
                    if let Some(delta) = event.payload.get("delta").and_then(Value::as_str) {
                        emitted_reasoning.push_str(delta);
                        yield Ok(sse_json(json!({
                            "id": completion_id,
                            "object": "chat.completion.chunk",
                            "created": current_timestamp(),
                            "model": model,
                            "choices": [{
                                "index": 0,
                                "delta": { "reasoning_content": delta },
                                "logprobs": Value::Null,
                                "finish_reason": Value::Null,
                            }],
                        })));
                    }
                }
                Ok(_) => {}
                Err(err) => {
                    yield Ok(sse_json(json!({ "error": { "message": err.to_string(), "type": "provider_stream_error" } })));
                    yield Ok(sse_done());
                    return;
                }
            }
        }

        let flushed = parser.flush();
        if !flushed.text.is_empty() {
            emitted_text.push_str(&flushed.text);
            yield Ok(sse_json(json!({
                "id": completion_id,
                "object": "chat.completion.chunk",
                "created": current_timestamp(),
                "model": model,
                "choices": [{
                    "index": 0,
                    "delta": { "content": flushed.text },
                    "logprobs": Value::Null,
                    "finish_reason": Value::Null,
                }],
            })));
        }

        let chat = match bridge_stream.finish::<BridgeChatResponse>().await {
            Ok(chat) => chat,
            Err(err) => {
                yield Ok(sse_json(json!({ "error": { "message": err.to_string(), "type": "provider_stream_error" } })));
                yield Ok(sse_done());
                return;
            }
        };
        let parsed = parse_browser_output(&body, &chat.text);
        if let Some(reasoning) = chat.reasoning_content.as_deref() {
            let remaining = reasoning.strip_prefix(&emitted_reasoning).unwrap_or(reasoning);
            if !remaining.is_empty() {
                yield Ok(sse_json(json!({
                    "id": completion_id,
                    "object": "chat.completion.chunk",
                    "created": current_timestamp(),
                    "model": model,
                    "choices": [{
                        "index": 0,
                        "delta": { "reasoning_content": remaining },
                        "logprobs": Value::Null,
                        "finish_reason": Value::Null,
                    }],
                })));
            }
        }
        if let Some(remaining) = parsed.text.strip_prefix(&emitted_text) {
            if !remaining.is_empty() {
                yield Ok(sse_json(json!({
                    "id": completion_id,
                    "object": "chat.completion.chunk",
                    "created": current_timestamp(),
                    "model": model,
                    "choices": [{
                        "index": 0,
                        "delta": { "content": remaining },
                        "logprobs": Value::Null,
                        "finish_reason": Value::Null,
                    }],
                })));
            }
        } else if emitted_text.is_empty() && !parsed.text.is_empty() {
            for chunk in split_text_chunks(&parsed.text, 320) {
                yield Ok(sse_json(json!({
                    "id": completion_id,
                    "object": "chat.completion.chunk",
                    "created": current_timestamp(),
                    "model": model,
                    "choices": [{
                        "index": 0,
                        "delta": { "content": chunk },
                        "logprobs": Value::Null,
                        "finish_reason": Value::Null,
                    }],
                })));
            }
        }

        let finish_reason = if parsed.tool_calls.is_empty() { "stop" } else { "tool_calls" };
        if !parsed.tool_calls.is_empty() {
            yield Ok(sse_json(json!({
                "id": completion_id,
                "object": "chat.completion.chunk",
                "created": current_timestamp(),
                "model": model,
                "choices": [{
                    "index": 0,
                    "delta": { "tool_calls": parsed.tool_calls },
                    "logprobs": Value::Null,
                    "finish_reason": Value::Null,
                }],
            })));
        }
        yield Ok(sse_json(json!({
            "id": completion_id,
            "object": "chat.completion.chunk",
            "created": current_timestamp(),
            "model": model,
            "choices": [{
                "index": 0,
                "delta": {},
                "logprobs": Value::Null,
                "finish_reason": finish_reason,
            }],
            "usage": usage_from_text(&prompt, &parsed.text, true),
            "provider_metadata": browser_provider_metadata(&state.config.kind, &chat),
            "provider_warnings": build_provider_warnings(&state.config.kind, &body, &chat),
        })));
        yield Ok(sse_done());
    };

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/event-stream"),
            (header::CACHE_CONTROL, "no-cache"),
            (header::CONNECTION, "keep-alive"),
            (header::HeaderName::from_static("x-accel-buffering"), "no"),
        ],
        axum::body::Body::from_stream(stream),
    )
        .into_response()
}

fn browser_tool_system_instructions(body: &OpenAIRequest) -> Option<String> {
    body.tools.as_ref()?;

    let mut prefix = String::from(
        "BROWSER TOOL MODE\n\
These tools are real and executable by the client in this session.\n\
If user asks to test tools, inspect workspace/files, read code, write code, grep, glob, run commands, or understand repository state, do not answer in prose.\n\
You must respond with one or more <tool_call>...</tool_call> blocks only.\n\
Never print Kilo-style command objects such as {\"command\":\"...\",\"description\":\"...\",\"workdir\":\"...\"}; they are text, not executable calls.\n\
Never say tools are unavailable, pasted text, unsupported, or not accessible from this session.\n",
    );

    if body.tool_choice.as_ref().and_then(Value::as_str) == Some("required") {
        prefix.push_str(
            "tool_choice is required. You must call one or more tools before any normal text.\n",
        );
    }

    if let Some(name) = body
        .tool_choice
        .as_ref()
        .and_then(|value| value.get("function"))
        .and_then(|value| value.get("name"))
        .and_then(Value::as_str)
    {
        prefix.push_str(&format!(
            "tool_choice targets function `{name}`. Call that function unless arguments cannot be inferred.\n"
        ));
    }

    Some(prefix.trim_end().to_owned())
}

fn clean_empty_tool_fence(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed
        .lines()
        .filter(|line| !line.trim().is_empty())
        .all(|line| matches!(line.trim(), "```" | "```json"))
    {
        String::new()
    } else {
        trimmed.to_owned()
    }
}

fn parse_browser_output(body: &OpenAIRequest, text: &str) -> ParsedBrowserOutput {
    let Some(_) = body.tools.as_ref() else {
        return ParsedBrowserOutput {
            text: text.to_owned(),
            tool_calls: Vec::new(),
        };
    };

    let mut parser = StreamingToolParser::new();
    let mut parsed = parser.feed(text);
    let flush = parser.flush();
    parsed.text.push_str(&flush.text);
    parsed.tool_calls.extend(flush.tool_calls);

    if parsed.tool_calls.is_empty() {
        let (cleaned, leaked) = extract_tool_calls_from_text(&parsed.text);
        if !leaked.is_empty() {
            parsed.text = clean_empty_tool_fence(&cleaned);
            parsed.tool_calls = leaked;
        }
    }

    if parsed.tool_calls.is_empty() {
        if let Some((cleaned, leaked)) = extract_command_object_tool_calls(
            &parsed.text,
            body.tools.as_deref().unwrap_or_default(),
        ) {
            parsed.text = cleaned;
            parsed.tool_calls = leaked;
        }
    }

    ParsedBrowserOutput {
        text: clean_empty_tool_fence(&parsed.text),
        tool_calls: parsed
            .tool_calls
            .into_iter()
            .map(tool_call_from_parsed)
            .collect(),
    }
}

fn extract_command_object_tool_calls(
    text: &str,
    tools: &[FunctionToolDefinition],
) -> Option<(String, Vec<crate::proxy_core::ParsedToolCall>)> {
    let tool_name = tools
        .iter()
        .find(|tool| {
            let name = tool.tool_name().unwrap_or_default().to_ascii_lowercase();
            let has_command_parameter = tool
                .function
                .as_ref()
                .and_then(|function| function.parameters.as_ref())
                .and_then(|parameters| parameters.get("properties"))
                .and_then(Value::as_object)
                .is_some_and(|properties| properties.contains_key("command"));
            has_command_parameter
                || name.contains("command")
                || name.contains("shell")
                || name.contains("terminal")
                || name.contains("exec")
                || name == "bash"
        })
        .and_then(FunctionToolDefinition::tool_name)
        .or_else(|| {
            (tools.len() == 1)
                .then(|| tools.first().and_then(FunctionToolDefinition::tool_name))
                .flatten()
        })?
        .to_owned();

    let mut rest = text.trim();
    let mut calls = Vec::new();
    while !rest.is_empty() {
        let mut values = serde_json::Deserializer::from_str(rest).into_iter::<Value>();
        let parsed = values.next()?.ok()?;
        let consumed = values.byte_offset();
        let object = parsed.as_object()?;
        if object.get("command").and_then(Value::as_str).is_none() {
            return None;
        }
        calls.push(crate::proxy_core::ParsedToolCall {
            id: format!("call_{}", Uuid::new_v4()),
            name: tool_name.clone(),
            arguments: parsed,
        });
        rest = rest.get(consumed..)?.trim_start();
    }

    (!calls.is_empty()).then(|| (String::new(), calls))
}

fn tool_call_from_parsed(parsed: crate::proxy_core::ParsedToolCall) -> MessageToolCall {
    MessageToolCall {
        id: parsed.id,
        tool_type: "function".to_owned(),
        function: ToolCallFunction {
            name: parsed.name,
            arguments: parsed.arguments.to_string(),
        },
    }
}

fn coerce_agent_model(kind: BrowserProviderKind, requested: &str) -> String {
    let trimmed = requested.trim();
    if trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("claude")
        || trimmed.to_ascii_lowercase().starts_with("claude-")
        || trimmed.to_ascii_lowercase().starts_with("anthropic.")
    {
        kind.default_model().to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn openai_responses_to_request(state: &AppState, body: &Value) -> Result<OpenAIRequest> {
    let model = coerce_agent_model(
        state.config.kind,
        body.get("model")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    let mut messages = Vec::new();
    if let Some(instructions) = body.get("instructions") {
        let text = value_to_text(instructions);
        if !text.is_empty() {
            messages.push(simple_message("system", text));
        }
    }
    messages.extend(response_input_to_messages(body.get("input")));

    Ok(OpenAIRequest {
        model,
        messages,
        stream: body.get("stream").and_then(Value::as_bool),
        web_search: body.get("web_search").and_then(Value::as_bool),
        chatgpt_mode: body
            .get("chatgpt_mode")
            .and_then(Value::as_str)
            .map(str::to_owned),
        user: body.get("user").and_then(Value::as_str).map(str::to_owned),
        tools: openai_tools_from_value(body.get("tools")),
        tool_choice: body.get("tool_choice").cloned(),
        stream_options: None,
    })
}

fn anthropic_to_openai_request(state: &AppState, body: &Value) -> Result<OpenAIRequest> {
    let model = coerce_agent_model(
        state.config.kind,
        body.get("model")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    let mut messages = Vec::new();

    if let Some(system) = body.get("system") {
        let text = value_to_text(system);
        if !text.is_empty() {
            messages.push(simple_message("system", text));
        }
    }

    let anthropic_messages = body
        .get("messages")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("Anthropic messages array is required"))?;
    for message in anthropic_messages {
        messages.extend(anthropic_message_to_openai_messages(message));
    }

    let tool_choice = anthropic_tool_choice_to_openai(body.get("tool_choice"));

    Ok(OpenAIRequest {
        model,
        messages,
        stream: body.get("stream").and_then(Value::as_bool),
        web_search: body.get("web_search").and_then(Value::as_bool),
        chatgpt_mode: body
            .get("chatgpt_mode")
            .and_then(Value::as_str)
            .map(str::to_owned),
        user: body.get("user").and_then(Value::as_str).map(str::to_owned),
        tools: anthropic_tools_from_value(body.get("tools")),
        tool_choice,
        stream_options: None,
    })
}

fn anthropic_tool_choice_to_openai(value: Option<&Value>) -> Option<Value> {
    match value {
        Some(Value::Object(map)) if map.get("type").and_then(Value::as_str) == Some("tool") => {
            map.get("name").and_then(Value::as_str).map(|name| {
                json!({
                    "type": "function",
                    "function": { "name": name }
                })
            })
        }
        Some(Value::Object(map)) if map.get("type").and_then(Value::as_str) == Some("any") => {
            Some(json!("required"))
        }
        Some(Value::Object(map)) if map.get("type").and_then(Value::as_str) == Some("auto") => {
            Some(json!("auto"))
        }
        Some(Value::String(value)) if value == "any" => Some(json!("required")),
        Some(Value::String(value)) if value == "auto" || value == "none" => Some(json!(value)),
        Some(other) => Some(other.clone()),
        None => None,
    }
}

fn anthropic_message_to_openai_messages(message: &Value) -> Vec<Message> {
    let role = message
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or("user");
    let content = message.get("content").unwrap_or(&Value::Null);
    let Some(blocks) = content.as_array() else {
        return vec![simple_message(role, value_to_text(content))];
    };

    let mut text_parts = Vec::new();
    let mut tool_calls = Vec::new();
    let mut tool_results = Vec::new();

    for block in blocks {
        let block_type = block
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match block_type {
            "text" => {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    text_parts.push(text.to_owned());
                }
            }
            "tool_use" if role == "assistant" => {
                let Some(name) = block.get("name").and_then(Value::as_str) else {
                    continue;
                };
                let id = block
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("call_{}", Uuid::new_v4()));
                let input = block.get("input").cloned().unwrap_or_else(|| json!({}));
                tool_calls.push(MessageToolCall {
                    id,
                    tool_type: "function".to_owned(),
                    function: ToolCallFunction {
                        name: name.to_owned(),
                        arguments: input.to_string(),
                    },
                });
            }
            "tool_result" => {
                let Some(tool_call_id) = block
                    .get("tool_use_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                else {
                    continue;
                };
                tool_results.push(Message {
                    role: "tool".to_owned(),
                    content: block.get("content").cloned(),
                    tool_calls: None,
                    tool_call_id: Some(tool_call_id),
                    name: None,
                    reasoning_content: None,
                });
            }
            _ => {
                let text = value_to_text(block);
                if !text.is_empty() {
                    text_parts.push(text);
                }
            }
        }
    }

    let mut out = Vec::new();
    let text = text_parts.join("\n");
    if role == "assistant" && (!text.is_empty() || !tool_calls.is_empty()) {
        out.push(Message {
            role: "assistant".to_owned(),
            content: (!text.is_empty()).then_some(Value::String(text)),
            tool_calls: (!tool_calls.is_empty()).then_some(tool_calls),
            tool_call_id: None,
            name: None,
            reasoning_content: None,
        });
    } else if !text.is_empty() {
        out.push(simple_message(role, text));
    }
    out.extend(tool_results);

    if out.is_empty() {
        out.push(simple_message(role, value_to_text(content)));
    }
    out
}

fn response_input_to_messages(input: Option<&Value>) -> Vec<Message> {
    match input {
        None => Vec::new(),
        Some(Value::String(text)) => vec![simple_message("user", text.clone())],
        Some(Value::Array(items)) => items
            .iter()
            .flat_map(response_input_item_to_messages)
            .collect(),
        Some(value) => vec![simple_message("user", value_to_text(value))],
    }
}

fn response_input_item_to_messages(item: &Value) -> Vec<Message> {
    match item.get("type").and_then(Value::as_str) {
        Some("function_call") => {
            let Some(name) = item.get("name").and_then(Value::as_str) else {
                return Vec::new();
            };
            let id = item
                .get("call_id")
                .or_else(|| item.get("id"))
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| format!("call_{}", Uuid::new_v4()));
            let arguments = item
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}))
                .to_string();
            vec![Message {
                role: "assistant".to_owned(),
                content: None,
                tool_calls: Some(vec![MessageToolCall {
                    id,
                    tool_type: "function".to_owned(),
                    function: ToolCallFunction {
                        name: name.to_owned(),
                        arguments,
                    },
                }]),
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            }]
        }
        Some("function_call_output") => {
            let Some(tool_call_id) = item
                .get("call_id")
                .and_then(Value::as_str)
                .map(str::to_owned)
            else {
                return Vec::new();
            };
            vec![Message {
                role: "tool".to_owned(),
                content: item.get("output").cloned(),
                tool_calls: None,
                tool_call_id: Some(tool_call_id),
                name: None,
                reasoning_content: None,
            }]
        }
        _ => {
            let role = item.get("role").and_then(Value::as_str).unwrap_or("user");
            let text = item
                .get("content")
                .map(value_to_text)
                .unwrap_or_else(|| value_to_text(item));
            vec![simple_message(role, text)]
        }
    }
}

fn simple_message(role: &str, text: String) -> Message {
    Message {
        role: role.to_owned(),
        content: Some(Value::String(text)),
        tool_calls: None,
        tool_call_id: None,
        name: None,
        reasoning_content: None,
    }
}

fn openai_tools_from_value(value: Option<&Value>) -> Option<Vec<FunctionToolDefinition>> {
    let items = value?.as_array()?;
    let tools = items
        .iter()
        .filter_map(|item| {
            if item.get("function").is_some() {
                return serde_json::from_value(item.clone()).ok();
            }

            let tool_type = item
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("function");
            let name = item.get("name").and_then(Value::as_str)?.to_owned();
            let description = item
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if tool_type == "custom" {
                /* freeform custom tool: no JSON parameters, name at the top level */
                return Some(FunctionToolDefinition {
                    tool_type: "custom".to_owned(),
                    function: None,
                    name: Some(name),
                    description,
                });
            }
            Some(FunctionToolDefinition {
                tool_type: "function".to_owned(),
                function: Some(crate::proxy_core::FunctionToolSpec {
                    name,
                    description,
                    parameters: item.get("parameters").cloned(),
                    strict: item.get("strict").and_then(Value::as_bool),
                }),
                name: None,
                description: None,
            })
        })
        .collect::<Vec<_>>();
    Some(tools)
}

fn anthropic_tools_from_value(value: Option<&Value>) -> Option<Vec<FunctionToolDefinition>> {
    let items = value?.as_array()?;
    let tools = items
        .iter()
        .filter_map(|item| {
            let name = item.get("name").and_then(Value::as_str)?.to_owned();
            Some(FunctionToolDefinition {
                tool_type: "function".to_owned(),
                function: Some(crate::proxy_core::FunctionToolSpec {
                    name,
                    description: item
                        .get("description")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    parameters: item.get("input_schema").cloned(),
                    strict: None,
                }),
                name: None,
                description: None,
            })
        })
        .collect::<Vec<_>>();
    Some(tools)
}

fn value_to_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .map(|item| match item {
                Value::Object(map) => {
                    if let Some(text) = map.get("text").and_then(Value::as_str) {
                        text.to_owned()
                    } else if let Some(content) = map.get("content") {
                        value_to_text(content)
                    } else {
                        item.to_string()
                    }
                }
                _ => value_to_text(item),
            })
            .filter(|item| !item.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(map) => {
            if let Some(text) = map.get("text").and_then(Value::as_str) {
                text.to_owned()
            } else if let Some(content) = map.get("content") {
                value_to_text(content)
            } else {
                value.to_string()
            }
        }
        other => other.to_string(),
    }
}

fn browser_provider_metadata(kind: &BrowserProviderKind, chat: &BridgeChatResponse) -> Value {
    let mut metadata = serde_json::Map::new();
    metadata.insert("provider".to_owned(), json!(kind.as_str()));
    if let Some(conversation_id) = &chat.conversation_id {
        metadata.insert("conversation_id".to_owned(), json!(conversation_id));
    }
    if let Some(usage) = &chat.upstream_usage {
        metadata.insert("upstream_usage".to_owned(), usage.clone());
    }
    if let Some(cache) = &chat.upstream_cache {
        metadata.insert("upstream_cache".to_owned(), cache.clone());
    }
    Value::Object(metadata)
}

fn response_usage(usage: &Usage) -> Value {
    json!({
        "input_tokens": usage.prompt_tokens,
        "output_tokens": usage.completion_tokens,
        "total_tokens": usage.total_tokens,
    })
}

fn json_openai_response(
    state: AppState,
    body: OpenAIRequest,
    chat: BridgeChatResponse,
) -> Response {
    let response_id = format!("resp_{}", Uuid::new_v4().simple());
    let model = chat.model.clone().unwrap_or_else(|| body.model.clone());
    let parsed = parse_browser_output(&body, &chat.text);
    let usage = usage_from_text(&build_prompt(&body), &parsed.text, true);
    let mut output = Vec::new();
    if !parsed.text.is_empty() {
        output.push(json!({
            "id": format!("msg_{}", Uuid::new_v4().simple()),
            "type": "message",
            "role": "assistant",
            "content": [{ "type": "output_text", "text": parsed.text.clone() }]
        }));
    }
    for tool_call in &parsed.tool_calls {
        output.push(json!({
            "id": tool_call.id,
            "type": "function_call",
            "name": tool_call.function.name,
            "arguments": tool_call.function.arguments,
            "call_id": tool_call.id,
        }));
    }

    Json(json!({
        "id": response_id,
        "object": "response",
        "created_at": current_timestamp(),
        "status": "completed",
        "model": model,
        "output": output,
        "output_text": parsed.text,
        "usage": response_usage(&usage),
        "provider_metadata": browser_provider_metadata(&state.config.kind, &chat)
    }))
    .into_response()
}

fn stream_openai_response(
    state: AppState,
    body: OpenAIRequest,
    chat: BridgeChatResponse,
) -> Response {
    let response_id = format!("resp_{}", Uuid::new_v4().simple());
    let model = chat.model.clone().unwrap_or_else(|| body.model.clone());
    let parsed = parse_browser_output(&body, &chat.text);
    let usage = usage_from_text(&build_prompt(&body), &parsed.text, true);
    let text = parsed.text.clone();
    let chunks = split_text_chunks(&text, 320);
    let tool_calls = parsed.tool_calls.clone();

    let stream = stream! {
        yield Ok::<Bytes, std::convert::Infallible>(Bytes::from(format!("event: response.created\ndata: {}\n\n", json!({
            "id": response_id,
            "object": "response",
            "created_at": current_timestamp(),
            "status": "in_progress",
            "model": model,
        }))));

        for chunk in chunks {
            if chunk.is_empty() {
                continue;
            }
            yield Ok(Bytes::from(format!("event: response.output_text.delta\ndata: {}\n\n", json!({
                "id": response_id,
                "delta": chunk,
            }))));
        }

        for tool_call in tool_calls {
            yield Ok(Bytes::from(format!("event: response.output_item.added\ndata: {}\n\n", json!({
                "id": response_id,
                "item": {
                    "id": tool_call.id,
                    "type": "function_call",
                    "name": tool_call.function.name,
                    "arguments": tool_call.function.arguments,
                    "call_id": tool_call.id,
                }
            }))));
        }

        yield Ok(Bytes::from(format!("event: response.completed\ndata: {}\n\n", json!({
            "id": response_id,
            "object": "response",
            "created_at": current_timestamp(),
            "status": "completed",
            "model": model,
            "output_text": text,
            "usage": response_usage(&usage),
            "provider_metadata": browser_provider_metadata(&state.config.kind, &chat)
        }))));
    };

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/event-stream"),
            (header::CACHE_CONTROL, "no-cache"),
            (header::CONNECTION, "keep-alive"),
            (header::HeaderName::from_static("x-accel-buffering"), "no"),
        ],
        axum::body::Body::from_stream(stream),
    )
        .into_response()
}

fn json_anthropic_response(
    state: AppState,
    body: OpenAIRequest,
    chat: BridgeChatResponse,
) -> Response {
    let message_id = format!("msg_{}", Uuid::new_v4().simple());
    let model = chat.model.clone().unwrap_or_else(|| body.model.clone());
    let parsed = parse_browser_output(&body, &chat.text);
    let usage = usage_from_text(&build_prompt(&body), &parsed.text, true);
    let mut content = Vec::new();
    if !parsed.text.is_empty() {
        content.push(json!({ "type": "text", "text": parsed.text.clone() }));
    }
    for tool_call in parsed.tool_calls {
        content.push(json!({
            "type": "tool_use",
            "id": tool_call.id,
            "name": tool_call.function.name,
            "input": serde_json::from_str::<Value>(&tool_call.function.arguments).unwrap_or_else(|_| json!({ "raw": tool_call.function.arguments })),
        }));
    }

    Json(json!({
        "id": message_id,
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": content,
        "stop_reason": if content.iter().any(|item| item.get("type").and_then(Value::as_str) == Some("tool_use")) { "tool_use" } else { "end_turn" },
        "stop_sequence": Value::Null,
        "usage": {
            "input_tokens": usage.prompt_tokens,
            "output_tokens": usage.completion_tokens,
        },
        "provider_metadata": browser_provider_metadata(&state.config.kind, &chat)
    }))
    .into_response()
}

fn stream_anthropic_response(
    _state: AppState,
    body: OpenAIRequest,
    chat: BridgeChatResponse,
) -> Response {
    let message_id = format!("msg_{}", Uuid::new_v4().simple());
    let model = chat.model.clone().unwrap_or_else(|| body.model.clone());
    let parsed = parse_browser_output(&body, &chat.text);
    let usage = usage_from_text(&build_prompt(&body), &parsed.text, true);
    let chunks = split_text_chunks(&parsed.text, 320);
    let tool_calls = parsed.tool_calls.clone();
    let stop_reason = if tool_calls.is_empty() {
        "end_turn"
    } else {
        "tool_use"
    };

    // text block occupies index 0 when present; tool blocks follow immediately after
    let tool_block_start = if parsed.text.is_empty() { 0usize } else { 1 };

    let stream = stream! {
        yield Ok::<Bytes, std::convert::Infallible>(Bytes::from(format!("event: message_start\ndata: {}\n\n", json!({
            "type": "message_start",
            "message": {
                "id": message_id,
                "type": "message",
                "role": "assistant",
                "model": model,
                "content": [],
                "stop_reason": Value::Null,
                "stop_sequence": Value::Null,
                "usage": { "input_tokens": usage.prompt_tokens, "output_tokens": 0 }
            }
        }))));

        if !parsed.text.is_empty() {
            yield Ok(Bytes::from(format!("event: content_block_start\ndata: {}\n\n", json!({
                "type": "content_block_start",
                "index": 0,
                "content_block": { "type": "text", "text": "" }
            }))));
            for chunk in chunks {
                if chunk.is_empty() {
                    continue;
                }
                yield Ok(Bytes::from(format!("event: content_block_delta\ndata: {}\n\n", json!({
                    "type": "content_block_delta",
                    "index": 0,
                    "delta": { "type": "text_delta", "text": chunk }
                }))));
            }
            yield Ok(Bytes::from("event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n"));
        }

        for (i, tool_call) in tool_calls.iter().enumerate() {
            let block_index = tool_block_start + i;
            // spec: content_block_start carries empty input; SDK accumulates from input_json_delta
            yield Ok(Bytes::from(format!("event: content_block_start\ndata: {}\n\n", json!({
                "type": "content_block_start",
                "index": block_index,
                "content_block": {
                    "type": "tool_use",
                    "id": tool_call.id,
                    "name": tool_call.function.name,
                    "input": {},
                }
            }))));
            yield Ok(Bytes::from(format!("event: content_block_delta\ndata: {}\n\n", json!({
                "type": "content_block_delta",
                "index": block_index,
                "delta": { "type": "input_json_delta", "partial_json": tool_call.function.arguments }
            }))));
            yield Ok(Bytes::from(format!("event: content_block_stop\ndata: {}\n\n", json!({
                "type": "content_block_stop",
                "index": block_index
            }))));
        }

        yield Ok(Bytes::from(format!("event: message_delta\ndata: {}\n\n", json!({
            "type": "message_delta",
            "delta": { "stop_reason": stop_reason, "stop_sequence": Value::Null },
            "usage": { "output_tokens": usage.completion_tokens }
        }))));
        yield Ok(Bytes::from("event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"));
    };

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/event-stream"),
            (header::CACHE_CONTROL, "no-cache"),
            (header::CONNECTION, "keep-alive"),
            (header::HeaderName::from_static("x-accel-buffering"), "no"),
        ],
        axum::body::Body::from_stream(stream),
    )
        .into_response()
}

fn build_provider_warnings(
    kind: &BrowserProviderKind,
    body: &OpenAIRequest,
    chat: &BridgeChatResponse,
) -> Vec<String> {
    let mut warnings = Vec::new();
    if body.web_search == Some(true) && !kind.web_search_supported() {
        warnings.push(format!(
            "{} web search toggle is not mapped yet. Chat continued with normal browser session behavior.",
            kind.as_str()
        ));
    }
    if let Some(warning) = &chat.warning {
        if !warning.trim().is_empty() {
            warnings.push(warning.clone());
        }
    }
    warnings
}

fn split_text_chunks(text: &str, max_chars: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }

    let mut chunks = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        if current.is_empty() {
            current.push_str(word);
            continue;
        }

        if current.len() + 1 + word.len() > max_chars {
            chunks.push(current);
            current = word.to_owned();
        } else {
            current.push(' ');
            current.push_str(word);
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

fn require_api_key(
    headers: &HeaderMap,
    api_key: Option<&str>,
) -> std::result::Result<(), Box<Response>> {
    let Some(api_key) = api_key.filter(|value| !value.trim().is_empty()) else {
        return Ok(());
    };

    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim);
    let xkey = headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim);
    let authorized = matches!(bearer, Some(provided) if constant_time_eq(provided, api_key))
        || matches!(xkey, Some(provided) if constant_time_eq(provided, api_key));

    if authorized {
        Ok(())
    } else {
        Err(Box::new(json_error(
            StatusCode::UNAUTHORIZED,
            "Missing or invalid bearer token".to_owned(),
        )))
    }
}

fn provider_error(state: &AppState, err: anyhow::Error) -> Response {
    // ponytail: log full cause server-side; return opaque id to client so upstream
    // bodies / header fragments never leak. Login-required detection runs on the
    // internal message before it's dropped.
    let message = err.to_string();
    let id = uuid::Uuid::new_v4();
    eprintln!(
        "[{}] upstream error {id}: {message}",
        state.config.kind.as_str()
    );
    let status = if is_login_required_error(&message) {
        StatusCode::UNAUTHORIZED
    } else {
        StatusCode::BAD_GATEWAY
    };

    json_error(
        status,
        format!("{} upstream error (id={id})", state.config.kind.as_str()),
    )
}

fn is_login_required_error(message: &str) -> bool {
    let lowered = message.to_ascii_lowercase();
    lowered.contains("logged in")
        || lowered.contains("session is active")
        || lowered.contains("timeout waiting for")
        || lowered.contains("request template")
}

fn json_error(status: StatusCode, message: String) -> Response {
    (
        status,
        Json(json!({
            "error": {
                "message": message,
                "type": "provider_error",
            }
        })),
    )
        .into_response()
}

#[cfg(test)]
#[path = "browser_runtime/tests.rs"]
mod tests;
