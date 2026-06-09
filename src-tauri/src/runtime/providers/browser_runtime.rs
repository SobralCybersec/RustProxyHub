use crate::browser_bridge::{BrowserBridge, InitParams, ManualLoginParams, PlaywrightBridge};
use crate::proxy_core::{build_prompt, current_timestamp, usage_from_text, OpenAIRequest};
use anyhow::Result;
use async_stream::stream;
use axum::{
    extract::State,
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
};
use tower_http::cors::CorsLayer;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BrowserProviderKind {
    Chatgpt,
    Gemini,
}

impl BrowserProviderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chatgpt => "chatgpt",
            Self::Gemini => "gemini",
        }
    }

    fn default_model(self) -> &'static str {
        match self {
            Self::Chatgpt => "chatgpt-web-session",
            Self::Gemini => "gemini-web-session",
        }
    }

    fn web_search_supported(self) -> bool {
        false
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
}

#[derive(Debug, Deserialize)]
struct ManualLoginRequest {
    #[serde(default)]
    browser: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BridgeChatResponse {
    text: String,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    conversation_id: Option<String>,
    #[serde(default)]
    warning: Option<String>,
}

pub async fn serve_browser_provider(config: BrowserProviderServerConfig) -> Result<()> {
    tokio::fs::create_dir_all(&config.runtime_dir).await?;

    let bridge = Arc::new(
        PlaywrightBridge::new_with_node(
            &config.helper_dir,
            config.node_path.clone(),
            config.kind.as_str(),
        )
        .await?,
    );

    let state = AppState { bridge, config };
    let app = Router::new()
        .route("/health", get(health))
        .route("/admin/manual_login", post(admin_manual_login))
        .route("/admin/close_login", post(admin_close_login))
        .route("/v1/models", get(models))
        .route("/v1/chat/completions", post(chat_completions))
        .layer(CorsLayer::permissive())
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

async fn models(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref()) {
        return *response;
    }

    Json(json!({
        "object": "list",
        "data": [{
            "id": state.config.kind.default_model(),
            "object": "model",
            "created": current_timestamp(),
            "owned_by": state.config.kind.as_str(),
            "permission": [],
            "root": state.config.kind.default_model(),
            "parent": Value::Null,
        }],
        "errors": [],
    }))
    .into_response()
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

    match request_browser_chat(&state, &body).await {
        Ok(chat) if body.stream.unwrap_or(false) => stream_browser_chat(state, body, chat),
        Ok(chat) => json_browser_chat(state, body, chat),
        Err(err) => provider_error(&state, err),
    }
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
    let model = if body.model.trim().is_empty() {
        state.config.kind.default_model().to_owned()
    } else {
        body.model.clone()
    };

    state
        .bridge
        .request(
            "chat",
            json!({
                "model": model,
                "prompt": build_prompt(body),
                "web_search": body.web_search.unwrap_or(false),
            }),
        )
        .await
}

fn json_browser_chat(state: AppState, body: OpenAIRequest, chat: BridgeChatResponse) -> Response {
    let completion_id = format!("chatcmpl-{}", Uuid::new_v4());
    let model = chat.model.clone().unwrap_or_else(|| body.model.clone());
    let usage = usage_from_text(&build_prompt(&body), &chat.text, true);
    let provider_warnings = build_provider_warnings(&state.config.kind, &body, &chat);

    Json(json!({
        "id": completion_id,
        "object": "chat.completion",
        "created": current_timestamp(),
        "model": model,
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": chat.text,
            },
            "logprobs": Value::Null,
            "finish_reason": "stop",
        }],
        "usage": usage,
        "provider_metadata": {
            "provider": state.config.kind.as_str(),
            "conversation_id": chat.conversation_id,
        },
        "provider_warnings": provider_warnings,
    }))
    .into_response()
}

fn stream_browser_chat(state: AppState, body: OpenAIRequest, chat: BridgeChatResponse) -> Response {
    let completion_id = format!("chatcmpl-{}", Uuid::new_v4());
    let model = chat.model.clone().unwrap_or_else(|| body.model.clone());
    let prompt = build_prompt(&body);
    let provider_warnings = build_provider_warnings(&state.config.kind, &body, &chat);
    let chunks = split_text_chunks(&chat.text, 320);

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
            "provider_warnings": provider_warnings,
        })));

        for chunk in chunks {
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

        yield Ok(sse_json(json!({
            "id": completion_id,
            "object": "chat.completion.chunk",
            "created": current_timestamp(),
            "model": model,
            "choices": [{
                "index": 0,
                "delta": {},
                "logprobs": Value::Null,
                "finish_reason": "stop",
            }],
            "usage": usage_from_text(&prompt, &chat.text, true),
            "provider_metadata": {
                "provider": state.config.kind.as_str(),
                "conversation_id": chat.conversation_id,
            },
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

    let authorized = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        == Some(api_key);

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
    let message = err.to_string();
    let status = if is_login_required_error(&message) {
        StatusCode::UNAUTHORIZED
    } else {
        StatusCode::BAD_GATEWAY
    };

    json_error(
        status,
        format!("{} request failed: {}", state.config.kind.as_str(), message),
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

fn sse_json(value: Value) -> Bytes {
    Bytes::from(format!("data: {}\n\n", value))
}

fn sse_done() -> Bytes {
    Bytes::from("data: [DONE]\n\n")
}
