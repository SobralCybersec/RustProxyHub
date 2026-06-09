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
    helper_dir_from, BrowserBridge, CaptureHeadersParams, InitParams, ManualLoginParams,
    PlaywrightBridge,
};
use bytes::Bytes;
use clap::{Parser, Subcommand};
use futures_util::StreamExt;
use once_cell::sync::Lazy;
use crate::proxy_core::{
    build_prompt, current_timestamp, usage_from_text, MessageToolCall, OpenAIRequest,
    StreamingToolParser, ToolCallFunction,
};
use regex::Regex;
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

static PAUSE_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        Regex::new("(?i)maximum number of tool calls").unwrap(),
        Regex::new("(?i)reached the maximum number of tool").unwrap(),
        Regex::new("(?i)type ['\"“”]?continue['\"“”]? to resume").unwrap(),
        Regex::new("(?i)número máximo de chamadas de ferramenta").unwrap(),
        Regex::new("(?i)digite ['\"“”]?continue['\"“”]? para retomar").unwrap(),
        Regex::new("(?i)limite máximo de chamadas").unwrap(),
    ]
});

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

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
pub struct KimiServiceConfig {
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
    session_parents: Arc<Mutex<HashMap<String, Option<String>>>>,
}

#[derive(Default)]
struct ConnectParser {
    buffer: Vec<u8>,
}

impl ConnectParser {
    fn feed(&mut self, chunk: &[u8]) -> Vec<Value> {
        self.buffer.extend_from_slice(chunk);
        let mut messages = Vec::new();

        loop {
            if self.buffer.len() < 5 {
                break;
            }
            let flags = self.buffer[0];
            let length = ((self.buffer[1] as usize) << 24)
                | ((self.buffer[2] as usize) << 16)
                | ((self.buffer[3] as usize) << 8)
                | (self.buffer[4] as usize);
            if self.buffer.len() < 5 + length {
                break;
            }

            let payload = self.buffer[5..5 + length].to_vec();
            self.buffer.drain(..5 + length);

            if flags == 0x00 {
                if let Ok(text) = String::from_utf8(payload) {
                    if let Ok(value) = serde_json::from_str::<Value>(&text) {
                        messages.push(value);
                    }
                }
            }
        }

        messages
    }
}

struct ConsumeResult {
    text: String,
    reasoning: String,
    tool_calls: Vec<MessageToolCall>,
}

#[derive(Debug, Deserialize)]
struct ManualLoginRequest {
    #[serde(default)]
    browser: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = load_config();
    tokio::fs::create_dir_all(&config.runtime_dir).await?;

    let bridge =
        Arc::new(PlaywrightBridge::new(helper_dir_from(env!("CARGO_MANIFEST_DIR")), "kimi").await?);

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
    println!("proxy-hub kimi listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

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
    println!("Kimi browser opened. Login, then press Enter here to close helper.");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    bridge.shutdown().await?;
    Ok(())
}

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
        runtime_dir: root.join("runtime").join("kimi"),
    }
}

pub async fn serve_embedded(config: KimiServiceConfig) -> Result<()> {
    tokio::fs::create_dir_all(&config.runtime_dir).await?;
    let bridge = Arc::new(
        PlaywrightBridge::new_with_node(&config.helper_dir, config.node_path.clone(), "kimi")
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
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref()) {
        return response;
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
        Ok(()) => Json(json!({ "ok": true, "provider": "kimi" })).into_response(),
        Err(err) => json_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn models(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref()) {
        return response;
    }

    Json(json!({
        "object": "list",
        "data": [
            { "id": "k2d6", "object": "model", "created": current_timestamp(), "owned_by": "kimi" },
            { "id": "k2d6-thinking", "object": "model", "created": current_timestamp(), "owned_by": "kimi" }
        ]
    }))
    .into_response()
}

async fn chat_completions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<OpenAIRequest>,
) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref()) {
        return response;
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
    let is_new_session = !body
        .messages
        .iter()
        .any(|message| message.role == "assistant");
    let completion_id = format!("chatcmpl-{}", Uuid::new_v4());
    let provider_warnings = if body.web_search == Some(true) {
        vec![Value::String(
            "Kimi web search toggle is not supported yet. Chat continued without forcing search."
                .to_owned(),
        )]
    } else {
        Vec::new()
    };

    let (response, ui_session_id) = request_kimi_stream(
        &state,
        &final_prompt,
        &body.model,
        is_thinking,
        is_new_session,
    )
    .await?;

    if !is_stream {
        let mut tool_calls = Vec::new();
        let mut text = String::new();
        let mut reasoning = String::new();
        let mut current_response = response;
        let mut current_session = ui_session_id;

        for _ in 0..5 {
            let result = consume_kimi_stream(
                current_response,
                &current_session,
                Arc::clone(&state.session_parents),
                body.tools.as_ref().map(|_| StreamingToolParser::new()),
            )
            .await?;

            text.push_str(&result.text);
            reasoning.push_str(&result.reasoning);
            tool_calls.extend(result.tool_calls);

            if is_paused_message(&text) {
                text = clean_pause_message(&text);
                let (next_response, next_session) =
                    request_kimi_stream(&state, "continue", &body.model, is_thinking, false)
                        .await?;
                current_response = next_response;
                current_session = next_session;
                continue;
            }
            break;
        }

        let usage = usage_from_text(&final_prompt, &format!("{reasoning}{text}"), true);
        let message = if tool_calls.is_empty() {
            json!({
                "role": "assistant",
                "content": text,
                "reasoning_content": reasoning,
            })
        } else {
            json!({
                "role": "assistant",
                "content": Value::Null,
                "reasoning_content": reasoning,
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
            "usage": usage,
            "provider_warnings": provider_warnings,
        }))
        .into_response());
    }

    let state_for_stream = state.clone();
    let model = body.model.clone();
    let stream = stream! {
        yield Ok::<Bytes, std::convert::Infallible>(sse_json(json!({
            "id": completion_id,
            "object": "chat.completion.chunk",
            "created": current_timestamp(),
            "model": model,
            "choices": [{ "index": 0, "delta": { "role": "assistant", "content": "" }, "logprobs": Value::Null, "finish_reason": Value::Null }],
            "provider_warnings": provider_warnings,
        })));

        let mut current_response = response;
        let mut current_session = ui_session_id;
        let mut reasoning = String::new();
        let mut text = String::new();
        let mut text_stream_buffer = String::new();
        let mut total_tool_calls = 0usize;

        for _ in 0..5 {
            let mut parser = ConnectParser::default();
            let mut tool_parser = body.tools.as_ref().map(|_| StreamingToolParser::new());
            let mut local_paused = false;
            let mut bytes_stream = current_response.bytes_stream();

            while let Some(chunk) = bytes_stream.next().await {
                match chunk {
                    Ok(chunk) => {
                        for message in parser.feed(&chunk) {
                            if let Some(chat_id) = message
                                .get("chat")
                                .and_then(Value::as_object)
                                .and_then(|chat| chat.get("id"))
                                .and_then(Value::as_str)
                            {
                                current_session = chat_id.to_owned();
                            }

                            if let Some(message_id) = message
                                .get("message")
                                .and_then(Value::as_object)
                                .and_then(|message| message.get("id"))
                                .and_then(Value::as_str)
                            {
                                if message
                                    .get("message")
                                    .and_then(Value::as_object)
                                    .and_then(|message| message.get("role"))
                                    .and_then(Value::as_str)
                                    == Some("assistant")
                                {
                                    state_for_stream
                                        .session_parents
                                        .lock()
                                        .await
                                        .insert(current_session.clone(), Some(message_id.to_owned()));
                                }
                            }

                            if let Some(think) = message
                                .get("block")
                                .and_then(Value::as_object)
                                .and_then(|block| block.get("think"))
                                .and_then(Value::as_object)
                                .and_then(|think| think.get("content"))
                                .and_then(Value::as_str)
                            {
                                reasoning.push_str(think);
                                yield Ok(sse_json(json!({
                                    "id": completion_id,
                                    "object": "chat.completion.chunk",
                                    "created": current_timestamp(),
                                    "model": model,
                                    "choices": [{ "index": 0, "delta": { "reasoning_content": think }, "logprobs": Value::Null, "finish_reason": Value::Null }]
                                })));
                            }

                            if let Some(text_chunk) = message
                                .get("block")
                                .and_then(Value::as_object)
                                .and_then(|block| block.get("text"))
                                .and_then(Value::as_object)
                                .and_then(|text| text.get("content"))
                                .and_then(Value::as_str)
                            {
                                if let Some(parser) = &mut tool_parser {
                                    let parsed = parser.feed(text_chunk);
                                    if !parsed.text.is_empty() {
                                        text_stream_buffer.push_str(&parsed.text);
                                        if text_stream_buffer.len() > 200 {
                                            let split_index = text_stream_buffer.len() - 200;
                                            let emit = text_stream_buffer[..split_index].to_owned();
                                            text_stream_buffer = text_stream_buffer[split_index..].to_owned();
                                            text.push_str(&emit);
                                            yield Ok(sse_json(json!({
                                                "id": completion_id,
                                                "object": "chat.completion.chunk",
                                                "created": current_timestamp(),
                                                "model": model,
                                                "choices": [{ "index": 0, "delta": { "content": emit }, "logprobs": Value::Null, "finish_reason": Value::Null }]
                                            })));
                                        }
                                    }
                                    for parsed_tool in parsed.tool_calls {
                                        let tool_call = tool_call_from_parsed(parsed_tool);
                                        yield Ok(sse_json(json!({
                                            "id": completion_id,
                                            "object": "chat.completion.chunk",
                                            "created": current_timestamp(),
                                            "model": model,
                                            "choices": [{
                                                "index": 0,
                                                "delta": {
                                                    "tool_calls": [{
                                                        "index": total_tool_calls,
                                                        "id": tool_call.id,
                                                        "type": "function",
                                                        "function": tool_call.function,
                                                    }]
                                                },
                                                "logprobs": Value::Null,
                                                "finish_reason": Value::Null
                                            }]
                                        })));
                                        total_tool_calls += 1;
                                    }
                                } else {
                                    text_stream_buffer.push_str(text_chunk);
                                    if text_stream_buffer.len() > 200 {
                                        let split_index = text_stream_buffer.len() - 200;
                                        let emit = text_stream_buffer[..split_index].to_owned();
                                        text_stream_buffer = text_stream_buffer[split_index..].to_owned();
                                        text.push_str(&emit);
                                        yield Ok(sse_json(json!({
                                            "id": completion_id,
                                            "object": "chat.completion.chunk",
                                            "created": current_timestamp(),
                                            "model": model,
                                            "choices": [{ "index": 0, "delta": { "content": emit }, "logprobs": Value::Null, "finish_reason": Value::Null }]
                                        })));
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
                            "choices": [{ "index": 0, "delta": { "content": format!("Kimi upstream error: {err}") }, "logprobs": Value::Null, "finish_reason": "stop" }]
                        })));
                        yield Ok(sse_done());
                        return;
                    }
                }
            }

            if let Some(parser) = &mut tool_parser {
                let flush = parser.flush();
                text_stream_buffer.push_str(&flush.text);
                for parsed_tool in flush.tool_calls {
                    let tool_call = tool_call_from_parsed(parsed_tool);
                    yield Ok(sse_json(json!({
                        "id": completion_id,
                        "object": "chat.completion.chunk",
                        "created": current_timestamp(),
                        "model": model,
                        "choices": [{
                            "index": 0,
                            "delta": {
                                "tool_calls": [{
                                    "index": total_tool_calls,
                                    "id": tool_call.id,
                                    "type": "function",
                                    "function": tool_call.function,
                                }]
                            },
                            "logprobs": Value::Null,
                            "finish_reason": Value::Null
                        }]
                    })));
                    total_tool_calls += 1;
                }
            }

            if is_paused_message(&text_stream_buffer) {
                text_stream_buffer = clean_pause_message(&text_stream_buffer);
                local_paused = true;
            }

            if local_paused {
                match request_kimi_stream(&state_for_stream, "continue", &body.model, is_thinking, false).await {
                    Ok((next_response, next_session)) => {
                        current_response = next_response;
                        current_session = next_session;
                        continue;
                    }
                    Err(err) => {
                        yield Ok(sse_json(json!({
                            "id": completion_id,
                            "object": "chat.completion.chunk",
                            "created": current_timestamp(),
                            "model": model,
                            "choices": [{ "index": 0, "delta": { "content": format!("Kimi auto-continue failed: {err}") }, "logprobs": Value::Null, "finish_reason": "stop" }]
                        })));
                        yield Ok(sse_done());
                        return;
                    }
                }
            }

            if !text_stream_buffer.is_empty() {
                text.push_str(&text_stream_buffer);
                yield Ok(sse_json(json!({
                    "id": completion_id,
                    "object": "chat.completion.chunk",
                    "created": current_timestamp(),
                    "model": model,
                    "choices": [{ "index": 0, "delta": { "content": text_stream_buffer }, "logprobs": Value::Null, "finish_reason": Value::Null }]
                })));
            }
            break;
        }

        let usage = usage_from_text(&final_prompt, &format!("{reasoning}{text}"), true);
        yield Ok(sse_json(json!({
            "id": completion_id,
            "object": "chat.completion.chunk",
            "created": current_timestamp(),
            "model": model,
            "choices": [{ "index": 0, "delta": {}, "logprobs": Value::Null, "finish_reason": if total_tool_calls == 0 { "stop" } else { "tool_calls" } }],
            "usage": usage,
        })));
        yield Ok(sse_done());
    };

    Ok(stream_response(stream))
}

async fn request_kimi_stream(
    state: &AppState,
    prompt: &str,
    model_id: &str,
    enable_thinking: bool,
    force_new: bool,
) -> Result<(reqwest::Response, String)> {
    let capture = state
        .bridge
        .capture_headers(CaptureHeadersParams {
            force_new,
            account_id: None,
        })
        .await?;

    let ui_session_id = capture.chat_session_id.unwrap_or_default();
    let actual_parent = if force_new {
        None
    } else {
        state
            .session_parents
            .lock()
            .await
            .get(&ui_session_id)
            .cloned()
            .flatten()
            .or_else(|| {
                capture
                    .parent_message_id
                    .as_ref()
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
    };

    let model_config = model_scenario(model_id);
    let mut payload = serde_json::Map::new();
    payload.insert(
        "scenario".to_owned(),
        Value::String(model_config.scenario.clone()),
    );
    payload.insert(
        "message".to_owned(),
        json!({
            "parent_id": actual_parent.clone().unwrap_or_default(),
            "role": "user",
            "blocks": [{ "message_id": "", "text": { "content": prompt } }],
            "scenario": model_config.scenario,
        }),
    );
    payload.insert(
        "options".to_owned(),
        json!({
            "thinking": enable_thinking || model_config.thinking,
            "agent_mode": model_config.agent_mode,
        }),
    );
    if !ui_session_id.is_empty() {
        payload.insert("chat_id".to_owned(), Value::String(ui_session_id.clone()));
    }
    if let Some(kimi_plus_id) = model_config.kimi_plus_id {
        payload.insert("kimi_plus_id".to_owned(), Value::String(kimi_plus_id));
    }

    let response = state
        .client
        .post("https://www.kimi.com/apiv2/kimi.gateway.chat.v1.ChatService/Chat")
        .header("accept", "*/*")
        .header("accept-language", "pt-BR,pt;q=0.9,en-US;q=0.8,en;q=0.7")
        .header(
            "authorization",
            capture
                .headers
                .get("authorization")
                .cloned()
                .unwrap_or_default(),
        )
        .header("connect-protocol-version", "1")
        .header("content-type", "application/connect+json")
        .header(
            "cookie",
            capture.headers.get("cookie").cloned().unwrap_or_default(),
        )
        .header("origin", "https://www.kimi.com")
        .header(
            "referer",
            if ui_session_id.is_empty() {
                "https://www.kimi.com/".to_owned()
            } else {
                format!("https://www.kimi.com/chat/{ui_session_id}")
            },
        )
        .header(
            "user-agent",
            capture
                .headers
                .get("user-agent")
                .cloned()
                .unwrap_or_default(),
        )
        .header(
            "x-msh-device-id",
            capture
                .headers
                .get("x-msh-device-id")
                .cloned()
                .unwrap_or_default(),
        )
        .header("x-msh-platform", "web")
        .header(
            "x-msh-session-id",
            capture
                .headers
                .get("x-msh-session-id")
                .cloned()
                .unwrap_or_default(),
        )
        .header("x-msh-version", "1.0.0")
        .header(
            "x-traffic-id",
            capture
                .headers
                .get("x-traffic-id")
                .cloned()
                .unwrap_or_default(),
        )
        .header(
            "r-timezone",
            capture
                .headers
                .get("r-timezone")
                .cloned()
                .unwrap_or_else(|| "America/Sao_Paulo".to_owned()),
        )
        .body(encode_connect_request(&Value::Object(payload))?)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "Kimi upstream error: {} {}",
            response.status(),
            response.text().await.unwrap_or_default()
        ));
    }

    Ok((response, ui_session_id))
}

async fn consume_kimi_stream(
    response: reqwest::Response,
    initial_ui_session_id: &str,
    session_parents: Arc<Mutex<HashMap<String, Option<String>>>>,
    mut tool_parser: Option<StreamingToolParser>,
) -> Result<ConsumeResult> {
    let mut parser = ConnectParser::default();
    let mut bytes_stream = response.bytes_stream();
    let mut reasoning = String::new();
    let mut text = String::new();
    let mut tool_calls = Vec::new();
    let mut ui_session_id = initial_ui_session_id.to_owned();

    while let Some(chunk) = bytes_stream.next().await {
        let chunk = chunk?;
        for message in parser.feed(&chunk) {
            if let Some(chat_id) = message
                .get("chat")
                .and_then(Value::as_object)
                .and_then(|chat| chat.get("id"))
                .and_then(Value::as_str)
            {
                ui_session_id = chat_id.to_owned();
            }

            if let Some(message_id) = message
                .get("message")
                .and_then(Value::as_object)
                .and_then(|message| message.get("id"))
                .and_then(Value::as_str)
            {
                if message
                    .get("message")
                    .and_then(Value::as_object)
                    .and_then(|message| message.get("role"))
                    .and_then(Value::as_str)
                    == Some("assistant")
                {
                    session_parents
                        .lock()
                        .await
                        .insert(ui_session_id.clone(), Some(message_id.to_owned()));
                }
            }

            if let Some(think) = message
                .get("block")
                .and_then(Value::as_object)
                .and_then(|block| block.get("think"))
                .and_then(Value::as_object)
                .and_then(|think| think.get("content"))
                .and_then(Value::as_str)
            {
                reasoning.push_str(think);
            }

            if let Some(text_chunk) = message
                .get("block")
                .and_then(Value::as_object)
                .and_then(|block| block.get("text"))
                .and_then(Value::as_object)
                .and_then(|text| text.get("content"))
                .and_then(Value::as_str)
            {
                if let Some(parser) = &mut tool_parser {
                    let parsed = parser.feed(text_chunk);
                    text.push_str(&parsed.text);
                    for parsed_tool in parsed.tool_calls {
                        tool_calls.push(tool_call_from_parsed(parsed_tool));
                    }
                } else {
                    text.push_str(text_chunk);
                }
            }
        }
    }

    if let Some(parser) = &mut tool_parser {
        let flush = parser.flush();
        text.push_str(&flush.text);
        for parsed_tool in flush.tool_calls {
            tool_calls.push(tool_call_from_parsed(parsed_tool));
        }
    }

    Ok(ConsumeResult {
        text,
        reasoning,
        tool_calls,
    })
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

fn model_scenario(model_id: &str) -> KimiModelConfig {
    let clean_model = model_id.replace("-no-thinking", "");
    match clean_model.as_str() {
        "k2d6" => KimiModelConfig {
            scenario: "SCENARIO_K2D5".to_owned(),
            thinking: false,
            kimi_plus_id: None,
            agent_mode: None,
        },
        "k2d6-thinking" => KimiModelConfig {
            scenario: "SCENARIO_K2D5".to_owned(),
            thinking: true,
            kimi_plus_id: None,
            agent_mode: None,
        },
        "k2d6-agent" => KimiModelConfig {
            scenario: "SCENARIO_OK_COMPUTER".to_owned(),
            thinking: false,
            kimi_plus_id: Some("ok-computer".to_owned()),
            agent_mode: Some("TYPE_NORMAL".to_owned()),
        },
        "k2d6-agent-ultra" => KimiModelConfig {
            scenario: "SCENARIO_OK_COMPUTER".to_owned(),
            thinking: false,
            kimi_plus_id: Some("ok-computer".to_owned()),
            agent_mode: Some("TYPE_ULTRA".to_owned()),
        },
        _ => KimiModelConfig {
            scenario: "SCENARIO_K2D5".to_owned(),
            thinking: model_id.contains("thinking"),
            kimi_plus_id: None,
            agent_mode: None,
        },
    }
}

struct KimiModelConfig {
    scenario: String,
    thinking: bool,
    kimi_plus_id: Option<String>,
    agent_mode: Option<String>,
}

fn encode_connect_request(payload: &Value) -> Result<Vec<u8>> {
    let json = serde_json::to_vec(payload)?;
    let length = json.len() as u32;
    let mut framed = Vec::with_capacity(5 + json.len());
    framed.push(0x00);
    framed.push(((length >> 24) & 0xff) as u8);
    framed.push(((length >> 16) & 0xff) as u8);
    framed.push(((length >> 8) & 0xff) as u8);
    framed.push((length & 0xff) as u8);
    framed.extend_from_slice(&json);
    Ok(framed)
}

fn is_paused_message(text: &str) -> bool {
    PAUSE_PATTERNS.iter().any(|pattern| pattern.is_match(text))
}

fn clean_pause_message(text: &str) -> String {
    let replacements = [
        r#"(?is)This task paused because Kimi reached.*?resume the task\."#,
        r#"(?is)Esta tarefa foi pausada porque.*?retomar a tarefa\."#,
        r#"(?is)This task paused because Kimi reached.*?resume\."#,
    ];

    let mut output = text.to_owned();
    for replacement in replacements {
        output = Regex::new(replacement)
            .unwrap()
            .replace_all(&output, "")
            .to_string();
    }
    output.trim().to_owned()
}

fn require_api_key(headers: &HeaderMap, api_key: Option<&str>) -> Result<(), Response> {
    let Some(api_key) = api_key else {
        return Ok(());
    };
    let provided = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    if provided == Some(api_key) {
        Ok(())
    } else {
        Err(json_error(
            StatusCode::UNAUTHORIZED,
            "Missing or invalid Authorization header".to_owned(),
        ))
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
