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
use crate::browser_bridge::{
    BrowserBridge, CaptureHeadersParams, InitParams, ManualLoginParams, PlaywrightBridge,
};
use bytes::Bytes;
use futures_util::StreamExt;
use crate::proxy_core::{
    build_prompt, current_timestamp, usage_from_text, MessageToolCall, OpenAIRequest,
    StreamingToolParser, ToolCallFunction,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::Arc,
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
        client: reqwest::Client::builder().build()?,
        config: config.clone(),
        session_parents: Arc::new(Mutex::new(HashMap::new())),
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/admin/manual_login", post(admin_manual_login))
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
        PlaywrightBridge::new_with_node(
            &config.helper_dir,
            config.node_path.clone(),
            "deepseek",
        )
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
        let captured = state
            .bridge
            .capture_headers(CaptureHeadersParams {
                force_new: is_new_session,
                account_id: None,
            })
            .await?;

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

        let mut payload = serde_json::Map::new();
        if !ui_session_id.is_empty() {
            payload.insert(
                "chat_session_id".to_owned(),
                Value::String(ui_session_id.clone()),
            );
        }
        payload.insert(
            "parent_message_id".to_owned(),
            actual_parent.map(Value::from).unwrap_or(Value::Null),
        );
        payload.insert(
            "model_type".to_owned(),
            if is_pro {
                Value::String("expert".to_owned())
            } else {
                Value::Null
            },
        );
        payload.insert("prompt".to_owned(), Value::String(final_prompt.clone()));
        payload.insert("ref_file_ids".to_owned(), Value::Array(Vec::new()));
        payload.insert("thinking_enabled".to_owned(), Value::Bool(is_thinking));
        payload.insert(
            "search_enabled".to_owned(),
            Value::Bool(body.web_search.unwrap_or(true)),
        );
        payload.insert("preempt".to_owned(), Value::Bool(false));

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
            .json(&Value::Object(payload))
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
            .and_then(|fragments| fragments.first())
            .and_then(Value::as_object)
        {
            if let Some(text) = response.get("content").and_then(Value::as_str) {
                v_str = Some(text.to_owned());
                parse_state.current_append_path =
                    if response.get("type").and_then(Value::as_str) == Some("THINK") {
                        "response/thinking_content".to_owned()
                    } else {
                        "response/content".to_owned()
                    };
                parse_state.current_fragment_type = response
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
            }
        } else if let Some(items) = value.as_array() {
            if let Some(item) = items.first().and_then(Value::as_object) {
                if let Some(text) = item.get("content").and_then(Value::as_str) {
                    v_str = Some(text.to_owned());
                    parse_state.current_append_path =
                        if item.get("type").and_then(Value::as_str) == Some("THINK") {
                            "response/thinking_content".to_owned()
                        } else {
                            "response/content".to_owned()
                        };
                    parse_state.current_fragment_type = item
                        .get("type")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned();
                }
            }
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
