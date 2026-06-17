use crate::browser_bridge::{
    BrowserBridge, CaptureHeadersParams, InitParams, ManualLoginParams, PlaywrightBridge,
};
use crate::proxy_core::{
    build_prompt, current_timestamp, usage_from_text, MessageToolCall, OpenAIRequest,
    StreamingToolParser, ToolCallFunction,
};
use anyhow::{anyhow, Result};
use async_stream::stream;
use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use bytes::Bytes;
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;
use uuid::Uuid;

#[cfg(feature = "standalone-provider-cli")]
use crate::browser_bridge::helper_dir_from;
#[cfg(feature = "standalone-provider-cli")]
use clap::{Parser, Subcommand};

#[cfg(feature = "standalone-provider-cli")]
#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[cfg(feature = "standalone-provider-cli")]
#[derive(Subcommand)]
enum Commands {
    Server,
    Login {
        #[arg(long, default_value = "chromium")]
        browser: String,
    },
}

#[derive(Clone)]
struct AppConfig {
    host: String,
    port: u16,
    api_key: Option<String>,
    headless: bool,
    browser: String,
    runtime_dir: PathBuf,
}

#[derive(Clone)]
pub struct DeepseekServiceConfig {
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
    client: reqwest::Client,
    config: AppConfig,
    session_parents: Arc<Mutex<HashMap<String, Option<i64>>>>,
}

#[derive(Default)]
struct DeepSeekParseState {
    current_append_path: String,
    current_fragment_type: String,
    reasoning: String,
    text: String,
    completion_tokens: usize,
}

enum ParsedEvent {
    Reasoning(String),
    Text(String),
    ToolCall(MessageToolCall),
}

fn collect_fragment_events(
    items: &[Value],
    parse_state: &mut DeepSeekParseState,
    tool_parser: &mut Option<StreamingToolParser>,
) -> Vec<ParsedEvent> {
    let mut events = Vec::new();

    for item in items {
        let Some(object) = item.as_object() else {
            continue;
        };
        let Some(text) = object.get("content").and_then(Value::as_str) else {
            continue;
        };
        if text.is_empty() || text == "FINISHED" {
            continue;
        }

        let fragment_type = object
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        parse_state.current_fragment_type = fragment_type.clone();

        if fragment_type == "THINK" {
            parse_state.current_append_path = "response/thinking_content".to_owned();
            parse_state.reasoning.push_str(text);
            events.push(ParsedEvent::Reasoning(text.to_owned()));
            continue;
        }

        parse_state.current_append_path = "response/content".to_owned();
        if let Some(parser) = tool_parser {
            let parsed = parser.feed(text);
            if !parsed.text.is_empty() {
                events.push(ParsedEvent::Text(parsed.text));
            }
            for tool_call in parsed.tool_calls {
                events.push(ParsedEvent::ToolCall(tool_call_to_message(
                    parser.emitted_tool_call_count(),
                    tool_call,
                )));
            }
        } else {
            events.push(ParsedEvent::Text(text.to_owned()));
        }
    }

    events
}

fn deepseek_model_type(is_pro: bool) -> &'static str {
    if is_pro { "expert" } else { "default" }
}

fn upsert_deepseek_event(events: &mut Vec<Value>, name: &str, params: Value) {
    for event in events.iter_mut() {
        let Some(object) = event.as_object_mut() else {
            continue;
        };
        if object.get("event").and_then(Value::as_str) == Some(name) {
            object.insert("params".to_owned(), params);
            return;
        }
    }

    events.push(json!({
        "event": name,
        "params": params,
    }));
}

fn sync_deepseek_events(payload: &mut serde_json::Map<String, Value>, is_pro: bool, is_thinking: bool) {
    let sync_events = |events: &mut Vec<Value>| {
        upsert_deepseek_event(
            events,
            "switchModelType",
            Value::String(deepseek_model_type(is_pro).to_owned()),
        );
        upsert_deepseek_event(events, "thinkingSwitchToggled", Value::Bool(is_thinking));
    };

    if let Some(Value::Array(events)) = payload.get_mut("events") {
        let is_direct_event_list = events.iter().any(|item| {
            item.as_object()
                .and_then(|object| object.get("event"))
                .and_then(Value::as_str)
                .is_some()
        });
        if is_direct_event_list {
            sync_events(events);
            return;
        }

        for group in events.iter_mut() {
            let Some(group_object) = group.as_object_mut() else {
                continue;
            };
            if let Some(group_events) = group_object.get_mut("events").and_then(Value::as_array_mut) {
                sync_events(group_events);
                return;
            }
        }
    }

    let mut events = Vec::new();
    sync_events(&mut events);
    payload.insert("events".to_owned(), Value::Array(events));
}

fn build_deepseek_payload(
    template: Option<Value>,
    final_prompt: &str,
    ui_session_id: &str,
    actual_parent: Option<i64>,
    is_pro: bool,
    is_thinking: bool,
    search_enabled: bool,
) -> Value {
    let mut payload = template
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();

    if !ui_session_id.is_empty() {
        payload.insert(
            "chat_session_id".to_owned(),
            Value::String(ui_session_id.to_owned()),
        );
    }
    payload.insert(
        "parent_message_id".to_owned(),
        actual_parent.map(Value::from).unwrap_or(Value::Null),
    );
    payload.insert(
        "model_type".to_owned(),
        Value::String(deepseek_model_type(is_pro).to_owned()),
    );
    payload.insert("prompt".to_owned(), Value::String(final_prompt.to_owned()));
    payload.insert("ref_file_ids".to_owned(), Value::Array(Vec::new()));
    payload.insert("thinking_enabled".to_owned(), Value::Bool(is_thinking));
    payload.insert("search_enabled".to_owned(), Value::Bool(search_enabled));
    payload.insert("preempt".to_owned(), Value::Bool(false));
    sync_deepseek_events(&mut payload, is_pro, is_thinking);

    Value::Object(payload)
}

#[derive(Debug, Deserialize)]
struct ManualLoginRequest {
    #[serde(default)]
    browser: Option<String>,
}

#[cfg(feature = "standalone-provider-cli")]
#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = load_config();
    tokio::fs::create_dir_all(&config.runtime_dir).await?;

    let bridge = Arc::new(
        PlaywrightBridge::new(helper_dir_from(env!("CARGO_MANIFEST_DIR")), "deepseek").await?,
    );

    match cli.command {
        Commands::Server => run_server(bridge, config).await,
        Commands::Login { browser } => run_login(bridge, config, browser).await,
    }
}

async fn run_server(bridge: Arc<PlaywrightBridge>, config: AppConfig) -> Result<()> {
    bridge
        .init(InitParams {
            runtime_dir: config.runtime_dir.to_string_lossy().to_string(),
            headless: config.headless,
            browser: config.browser.clone(),
        })
        .await?;

    let state = AppState {
        bridge,
        client: reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .tcp_nodelay(true)
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(16)
            .timeout(Duration::from_secs(120))
            .build()?,
        config: config.clone(),
        session_parents: Arc::new(Mutex::new(HashMap::new())),
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/admin/manual_login", post(admin_manual_login))
        .route("/admin/close_login", post(admin_close_login))
        .route("/v1/models", get(models))
        .route("/v1/chat/completions", post(chat_completions))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let host: IpAddr = config
        .host
        .parse()
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    let addr = SocketAddr::new(host, config.port);
    println!("proxy-hub deepseek listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(feature = "standalone-provider-cli")]
async fn run_login(
    bridge: Arc<PlaywrightBridge>,
    config: AppConfig,
    browser: String,
) -> Result<()> {
    bridge
        .manual_login(ManualLoginParams {
            runtime_dir: config.runtime_dir.to_string_lossy().to_string(),
            browser,
            account_id: None,
        })
        .await?;
    println!("DeepSeek browser opened. Login, then press Enter here to close helper.");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    bridge.shutdown().await?;
    Ok(())
}

#[cfg(feature = "standalone-provider-cli")]
fn load_config() -> AppConfig {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf();

    AppConfig {
        host: std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_owned()),
        port: std::env::var("PORT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(3000),
        api_key: std::env::var("API_KEY")
            .ok()
            .filter(|value| !value.is_empty()),
        headless: std::env::var("HEADLESS")
            .map(|value| value != "false")
            .unwrap_or(true),
        browser: std::env::var("BROWSER").unwrap_or_else(|_| "chromium".to_owned()),
        runtime_dir: root.join("runtime").join("deepseek"),
    }
}

pub async fn serve_embedded(config: DeepseekServiceConfig) -> Result<()> {
    tokio::fs::create_dir_all(&config.runtime_dir).await?;
    let bridge = Arc::new(
        PlaywrightBridge::new_with_node(&config.helper_dir, config.node_path.clone(), "deepseek")
            .await?,
    );
    run_server(
        bridge,
        AppConfig {
            host: config.host,
            port: config.port,
            api_key: config.api_key,
            headless: config.headless,
            browser: config.browser,
            runtime_dir: config.runtime_dir,
        },
    )
    .await
}

async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok" }))
}

async fn admin_manual_login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ManualLoginRequest>,
) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref(), true) {
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
        Ok(()) => Json(json!({ "ok": true, "provider": "deepseek" })).into_response(),
        Err(err) => json_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn admin_close_login(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref(), true) {
        return *response;
    }

    match state.bridge.shutdown().await {
        Ok(()) => Json(json!({ "ok": true, "provider": "deepseek" })).into_response(),
        Err(err) => json_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn models(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref(), true) {
        return *response;
    }

    Json(json!({
        "object": "list",
        "data": [
            { "id": "deepseek-v4-flash", "object": "model", "created": current_timestamp(), "owned_by": "deepseek", "permission": [], "root": "deepseek-v4-flash", "parent": null },
            { "id": "deepseek-v4-flash-thinking", "object": "model", "created": current_timestamp(), "owned_by": "deepseek", "permission": [], "root": "deepseek-v4-flash-thinking", "parent": null },
            { "id": "deepseek-v4-pro", "object": "model", "created": current_timestamp(), "owned_by": "deepseek", "permission": [], "root": "deepseek-v4-pro", "parent": null },
            { "id": "deepseek-v4-pro-thinking", "object": "model", "created": current_timestamp(), "owned_by": "deepseek", "permission": [], "root": "deepseek-v4-pro-thinking", "parent": null }
        ]
    }))
    .into_response()
}

async fn chat_completions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<OpenAIRequest>,
) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref(), true) {
        return *response;
    }

    match handle_chat(state, body).await {
        Ok(response) => response,
        Err(err) => json_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn handle_chat(state: AppState, body: OpenAIRequest) -> Result<Response> {
    let final_prompt = build_prompt(&body);
    let is_stream = body.stream.unwrap_or(false);
    let is_thinking = body.model.contains("thinking");
    let is_pro = body.model.contains("pro");
    let is_new_session = !body
        .messages
        .iter()
        .any(|message| message.role == "assistant");

    let mut last_error = None;
    let mut response = None;
    let mut ui_session_id = String::new();

    for _ in 0..3 {
        let captured = match state
            .bridge
            .capture_headers(CaptureHeadersParams {
                force_new: is_new_session,
                account_id: None,
            })
            .await
        {
            Ok(captured) => captured,
            Err(err) => {
                last_error = Some(anyhow!(err));
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }
        };

        ui_session_id = captured.chat_session_id.unwrap_or_default();
        let browser_parent = captured.parent_message_id.as_ref().and_then(Value::as_i64);
        let actual_parent = if is_new_session {
            None
        } else {
            state
                .session_parents
                .lock()
                .await
                .get(&ui_session_id)
                .copied()
                .flatten()
                .or(browser_parent)
        };

        let payload = build_deepseek_payload(
            captured.request_payload.clone(),
            &final_prompt,
            &ui_session_id,
            actual_parent,
            is_pro,
            is_thinking,
            body.web_search.unwrap_or(true),
        );

        let request = state
            .client
            .post("https://chat.deepseek.com/api/v0/chat/completion")
            .header("accept", "*/*")
            .header("accept-language", "pt-BR,pt;q=0.9,en-US;q=0.8,en;q=0.7")
            .header(
                "authorization",
                captured
                    .headers
                    .get("authorization")
                    .cloned()
                    .unwrap_or_default(),
            )
            .header("content-type", "application/json")
            .header("origin", "https://chat.deepseek.com")
            .header(
                "x-ds-pow-response",
                captured
                    .headers
                    .get("x-ds-pow-response")
                    .cloned()
                    .unwrap_or_default(),
            )
            .header(
                "x-hif-dliq",
                captured
                    .headers
                    .get("x-hif-dliq")
                    .cloned()
                    .unwrap_or_default(),
            )
            .header(
                "x-hif-leim",
                captured
                    .headers
                    .get("x-hif-leim")
                    .cloned()
                    .unwrap_or_default(),
            )
            .header("x-app-version", "2.0.0")
            .header("x-client-locale", "pt_BR")
            .header("x-client-platform", "web")
            .header("x-client-version", "2.0.0")
            .json(&payload)
            .send()
            .await;

        match request {
            Ok(upstream) if upstream.status().is_success() => {
                response = Some(upstream);
                break;
            }
            Ok(upstream) => {
                last_error = Some(anyhow!(upstream.text().await.unwrap_or_default()));
            }
            Err(err) => {
                last_error = Some(anyhow!(err));
            }
        }
    }

    let response = response
        .ok_or_else(|| last_error.unwrap_or_else(|| anyhow!("DeepSeek upstream request failed")))?;
    let completion_id = format!("chatcmpl-{}", Uuid::new_v4());
    if !is_stream {
        let mut parse_state = DeepSeekParseState::default();
        let mut tool_parser = body.tools.as_ref().map(|_| StreamingToolParser::new());
        let mut tool_calls = Vec::new();
        let mut buffer = String::new();
        let mut bytes_stream = response.bytes_stream();

        while let Some(chunk) = bytes_stream.next().await {
            let chunk = chunk?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(idx) = buffer.find('\n') {
                let line = buffer[..idx].trim().to_owned();
                buffer = buffer[idx + 1..].to_owned();
                if let Some(data) = line.strip_prefix("data: ") {
                    process_deepseek_line(
                        data,
                        &ui_session_id,
                        &state.session_parents,
                        &mut parse_state,
                        &mut tool_parser,
                        &mut tool_calls,
                    )
                    .await?;
                }
            }
        }

        if let Some(parser) = &mut tool_parser {
            let flush = parser.flush();
            parse_state.text.push_str(&flush.text);
            for parsed in flush.tool_calls {
                tool_calls.push(tool_call_to_message(tool_calls.len(), parsed));
            }
        }

        let output = format!("{}{}", parse_state.reasoning, parse_state.text);
        let usage = usage_from_text(&final_prompt, &output, true);
        let message = if tool_calls.is_empty() {
            json!({
                "role": "assistant",
                "content": parse_state.text,
                "reasoning_content": parse_state.reasoning,
            })
        } else {
            json!({
                "role": "assistant",
                "content": Value::Null,
                "reasoning_content": parse_state.reasoning,
                "tool_calls": tool_calls,
            })
        };

        return Ok(Json(json!({
            "id": completion_id,
            "object": "chat.completion",
            "created": current_timestamp(),
            "model": body.model,
            "choices": [{
                "index": 0,
                "message": message,
                "logprobs": Value::Null,
                "finish_reason": if tool_calls.is_empty() { "stop" } else { "tool_calls" }
            }],
            "usage": usage
        }))
        .into_response());
    }

    let model = body.model.clone();
    let session_parents = Arc::clone(&state.session_parents);
    let stream = stream! {
        yield Ok::<Bytes, std::convert::Infallible>(sse_json(json!({
            "id": completion_id,
            "object": "chat.completion.chunk",
            "created": current_timestamp(),
            "model": model,
            "choices": [{ "index": 0, "delta": { "role": "assistant", "content": "" }, "logprobs": Value::Null, "finish_reason": Value::Null }]
        })));

        let mut parse_state = DeepSeekParseState::default();
        let mut tool_parser = body.tools.as_ref().map(|_| StreamingToolParser::new());
        let mut tool_index = 0usize;
        let mut buffer = String::new();
        let mut bytes_stream = response.bytes_stream();

        while let Some(chunk) = bytes_stream.next().await {
            match chunk {
                Ok(chunk) => {
                    buffer.push_str(&String::from_utf8_lossy(&chunk));
                    while let Some(idx) = buffer.find('\n') {
                        let line = buffer[..idx].trim().to_owned();
                        buffer = buffer[idx + 1..].to_owned();
                        if let Some(data) = line.strip_prefix("data: ") {
                            match collect_deepseek_events(data, &ui_session_id, &session_parents, &mut parse_state, &mut tool_parser).await {
                                Ok(events) => {
                                    for event in events {
                                        match event {
                                            ParsedEvent::Reasoning(content) => {
                                                yield Ok(sse_json(json!({
                                                    "id": completion_id,
                                                    "object": "chat.completion.chunk",
                                                    "created": current_timestamp(),
                                                    "model": model,
                                                    "choices": [{ "index": 0, "delta": { "reasoning_content": content }, "logprobs": Value::Null, "finish_reason": Value::Null }]
                                                })));
                                            }
                                            ParsedEvent::Text(content) => {
                                                yield Ok(sse_json(json!({
                                                    "id": completion_id,
                                                    "object": "chat.completion.chunk",
                                                    "created": current_timestamp(),
                                                    "model": model,
                                                    "choices": [{ "index": 0, "delta": { "content": content }, "logprobs": Value::Null, "finish_reason": Value::Null }]
                                                })));
                                            }
                                            ParsedEvent::ToolCall(tool_call) => {
                                                yield Ok(sse_json(json!({
                                                    "id": completion_id,
                                                    "object": "chat.completion.chunk",
                                                    "created": current_timestamp(),
                                                    "model": model,
                                                    "choices": [{
                                                        "index": 0,
                                                        "delta": {
                                                            "tool_calls": [{
                                                                "index": tool_index,
                                                                "id": tool_call.id,
                                                                "type": "function",
                                                                "function": tool_call.function,
                                                            }]
                                                        },
                                                        "logprobs": Value::Null,
                                                        "finish_reason": Value::Null
                                                    }]
                                                })));
                                                tool_index += 1;
                                            }
                                        }
                                    }
                                }
                                Err(err) => {
                                    yield Ok(sse_json(json!({
                                        "id": completion_id,
                                        "object": "chat.completion.chunk",
                                        "created": current_timestamp(),
                                        "model": model,
                                        "choices": [{ "index": 0, "delta": { "content": format!("DeepSeek parse error: {err}") }, "logprobs": Value::Null, "finish_reason": "stop" }]
                                    })));
                                    yield Ok(sse_done());
                                    return;
                                }
                            }
                        }
                    }
                }
                Err(err) => {
                    yield Ok(sse_json(json!({
                        "id": completion_id,
                        "object": "chat.completion.chunk",
                        "created": current_timestamp(),
                        "model": model,
                        "choices": [{ "index": 0, "delta": { "content": format!("DeepSeek upstream error: {err}") }, "logprobs": Value::Null, "finish_reason": "stop" }]
                    })));
                    yield Ok(sse_done());
                    return;
                }
            }
        }

        if let Some(parser) = &mut tool_parser {
            let flush = parser.flush();
            if !flush.text.is_empty() {
                yield Ok(sse_json(json!({
                    "id": completion_id,
                    "object": "chat.completion.chunk",
                    "created": current_timestamp(),
                    "model": model,
                    "choices": [{ "index": 0, "delta": { "content": flush.text }, "logprobs": Value::Null, "finish_reason": Value::Null }]
                })));
            }
            for parsed in flush.tool_calls {
                let tool_call = tool_call_to_message(tool_index, parsed);
                yield Ok(sse_json(json!({
                    "id": completion_id,
                    "object": "chat.completion.chunk",
                    "created": current_timestamp(),
                    "model": model,
                    "choices": [{
                        "index": 0,
                        "delta": {
                            "tool_calls": [{
                                "index": tool_index,
                                "id": tool_call.id,
                                "type": "function",
                                "function": tool_call.function,
                            }]
                        },
                        "logprobs": Value::Null,
                        "finish_reason": Value::Null
                    }]
                })));
                tool_index += 1;
            }
        }

        let usage = usage_from_text(&final_prompt, &format!("{}{}", parse_state.reasoning, parse_state.text), true);
        yield Ok(sse_json(json!({
            "id": completion_id,
            "object": "chat.completion.chunk",
            "created": current_timestamp(),
            "model": model,
            "choices": [{ "index": 0, "delta": {}, "logprobs": Value::Null, "finish_reason": if tool_index == 0 { "stop" } else { "tool_calls" } }],
            "usage": usage,
        })));
        yield Ok(sse_done());
    };

    Ok(stream_response(stream))
}

async fn process_deepseek_line(
    data: &str,
    ui_session_id: &str,
    session_parents: &Arc<Mutex<HashMap<String, Option<i64>>>>,
    parse_state: &mut DeepSeekParseState,
    tool_parser: &mut Option<StreamingToolParser>,
    tool_calls: &mut Vec<MessageToolCall>,
) -> Result<()> {
    let events = collect_deepseek_events(
        data,
        ui_session_id,
        session_parents,
        parse_state,
        tool_parser,
    )
    .await?;
    for event in events {
        match event {
            ParsedEvent::Reasoning(_) => {}
            ParsedEvent::Text(text) => parse_state.text.push_str(&text),
            ParsedEvent::ToolCall(tool_call) => tool_calls.push(tool_call),
        }
    }
    Ok(())
}

async fn collect_deepseek_events(
    data: &str,
    ui_session_id: &str,
    session_parents: &Arc<Mutex<HashMap<String, Option<i64>>>>,
    parse_state: &mut DeepSeekParseState,
    tool_parser: &mut Option<StreamingToolParser>,
) -> Result<Vec<ParsedEvent>> {
    if data == "[DONE]" {
        return Ok(Vec::new());
    }

    let chunk: Value = match serde_json::from_str(data) {
        Ok(value) => value,
        Err(_) => return Ok(Vec::new()),
    };

    if let Some(message_id) = chunk
        .get("response_message_id")
        .and_then(Value::as_i64)
        .or_else(|| chunk.get("message_id").and_then(Value::as_i64))
        .or_else(|| {
            chunk
                .get("v")
                .and_then(Value::as_object)
                .and_then(|value| value.get("response"))
                .and_then(Value::as_object)
                .and_then(|response| response.get("message_id"))
                .and_then(Value::as_i64)
        })
        .or_else(|| {
            chunk
                .get("v")
                .and_then(Value::as_object)
                .and_then(|value| value.get("message_id"))
                .and_then(Value::as_i64)
        })
    {
        session_parents
            .lock()
            .await
            .insert(ui_session_id.to_owned(), Some(message_id));
    }

    if let Some(path) = chunk.get("p").and_then(Value::as_str) {
        parse_state.current_append_path = path.to_owned();
        if path == "response/accumulated_token_usage" {
            if let Some(tokens) = chunk.get("v").and_then(Value::as_u64) {
                parse_state.completion_tokens = tokens as usize;
            }
        }
    }

    let mut v_str = None;
    if let Some(value) = chunk.get("v") {
        if let Some(text) = value.as_str() {
            v_str = Some(text.to_owned());
        } else if let Some(response) = value
            .get("response")
            .and_then(Value::as_object)
            .and_then(|response| response.get("fragments"))
            .and_then(Value::as_array)
        {
            return Ok(collect_fragment_events(response, parse_state, tool_parser));
        } else if let Some(items) = value.as_array() {
            return Ok(collect_fragment_events(items, parse_state, tool_parser));
        }
    }

    let Some(v_str) = v_str else {
        return Ok(Vec::new());
    };
    if v_str.is_empty() || v_str == "FINISHED" {
        return Ok(Vec::new());
    }

    let is_thinking = parse_state.current_append_path.contains("thinking_content")
        || parse_state.current_append_path.contains("THINK")
        || (parse_state
            .current_append_path
            .contains("fragments/-1/content")
            && parse_state.current_fragment_type == "THINK");

    if is_thinking {
        parse_state.reasoning.push_str(&v_str);
        return Ok(vec![ParsedEvent::Reasoning(v_str)]);
    }

    if let Some(parser) = tool_parser {
        let parsed = parser.feed(&v_str);
        let mut events = Vec::new();
        if !parsed.text.is_empty() {
            events.push(ParsedEvent::Text(parsed.text));
        }
        for tool_call in parsed.tool_calls {
            events.push(ParsedEvent::ToolCall(tool_call_to_message(
                parser.emitted_tool_call_count(),
                tool_call,
            )));
        }
        return Ok(events);
    }

    Ok(vec![ParsedEvent::Text(v_str)])
}

fn tool_call_to_message(
    _index: usize,
    parsed: crate::proxy_core::ParsedToolCall,
) -> MessageToolCall {
    MessageToolCall {
        id: parsed.id,
        tool_type: "function".to_owned(),
        function: ToolCallFunction {
            name: parsed.name,
            arguments: parsed.arguments.to_string(),
        },
    }
}

fn require_api_key(
    headers: &HeaderMap,
    api_key: Option<&str>,
    allow_x_api_key: bool,
) -> Result<(), Box<Response>> {
    let Some(api_key) = api_key else {
        return Ok(());
    };

    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    let provided = bearer.or_else(|| {
        allow_x_api_key
            .then(|| {
                headers
                    .get("x-api-key")
                    .and_then(|value| value.to_str().ok())
            })
            .flatten()
    });

    if provided == Some(api_key) {
        Ok(())
    } else {
        Err(Box::new(json_error(
            StatusCode::UNAUTHORIZED,
            "Unauthorized".to_owned(),
        )))
    }
}

fn json_error(status: StatusCode, message: String) -> Response {
    (status, Json(json!({ "error": { "message": message } }))).into_response()
}

fn sse_json(value: Value) -> Bytes {
    Bytes::from(format!("data: {}\n\n", value))
}

fn sse_done() -> Bytes {
    Bytes::from("data: [DONE]\n\n")
}

fn stream_response<S>(stream: S) -> Response
where
    S: futures_util::Stream<Item = Result<Bytes, std::convert::Infallible>> + Send + 'static,
{
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header(header::CONNECTION, "keep-alive")
        .body(Body::from_stream(stream))
        .expect("valid streaming response")
}

#[cfg(test)]
mod tests {
    use super::{
        build_deepseek_payload, collect_fragment_events, DeepSeekParseState, ParsedEvent,
    };
    use crate::proxy_core::StreamingToolParser;
    use serde_json::json;

    #[test]
    fn fragment_arrays_keep_reasoning_and_visible_text_separate() {
        let mut state = DeepSeekParseState::default();
        let mut parser = None::<StreamingToolParser>;
        let events = collect_fragment_events(
            &[
                json!({ "type": "THINK", "content": "internal" }),
                json!({ "type": "TEXT", "content": "visible" }),
            ],
            &mut state,
            &mut parser,
        );

        assert_eq!(state.reasoning, "internal");
        assert_eq!(
            events
                .into_iter()
                .filter_map(|event| match event {
                    ParsedEvent::Text(text) => Some(text),
                    _ => None,
                })
                .collect::<String>(),
            "visible"
        );
    }

    #[test]
    fn deepseek_payload_reuses_template_and_syncs_direct_events() {
        let payload = build_deepseek_payload(
            Some(json!({
                "events": [
                    { "event": "switchModelType", "params": "default" },
                    { "event": "thinkingSwitchToggled", "params": false }
                ],
                "unused": true
            })),
            "READY",
            "session-1",
            Some(42),
            true,
            true,
            false,
        );

        assert_eq!(payload["chat_session_id"], "session-1");
        assert_eq!(payload["parent_message_id"], 42);
        assert_eq!(payload["model_type"], "expert");
        assert_eq!(payload["prompt"], "READY");
        assert_eq!(payload["thinking_enabled"], true);
        assert_eq!(payload["search_enabled"], false);
        assert_eq!(payload["unused"], true);
        assert_eq!(payload["events"][0]["params"], "expert");
        assert_eq!(payload["events"][1]["params"], true);
    }

    #[test]
    fn deepseek_payload_syncs_nested_event_groups() {
        let payload = build_deepseek_payload(
            Some(json!({
                "events": [
                    {
                        "events": [
                            { "event": "switchModelType", "params": "expert" }
                        ]
                    }
                ]
            })),
            "READY",
            "session-2",
            None,
            false,
            false,
            true,
        );

        assert_eq!(payload["model_type"], "default");
        assert_eq!(payload["events"][0]["events"][0]["params"], "default");
        assert_eq!(payload["events"][0]["events"][1]["event"], "thinkingSwitchToggled");
        assert_eq!(payload["events"][0]["events"][1]["params"], false);
    }
}
