use anyhow::{anyhow, Result};
use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use bytes::Bytes;
use futures_util::TryStreamExt;
use crate::proxy_core::OpenAIRequest;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};
use tower_http::cors::CorsLayer;

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
}

#[derive(Clone)]
pub struct ProviderConfig {
    base_url: String,
    api_key: Option<String>,
}

impl ProviderConfig {
    pub fn new(base_url: impl Into<String>, api_key: Option<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_key,
        }
    }
}

#[derive(Clone)]
struct AppConfig {
    host: String,
    port: u16,
    api_key: Option<String>,
    deepseek: ProviderConfig,
    kimi: ProviderConfig,
    qwen: ProviderConfig,
    chatgpt: ProviderConfig,
    gemini: ProviderConfig,
}

#[derive(Clone)]
pub struct HubServiceConfig {
    pub host: String,
    pub port: u16,
    pub api_key: Option<String>,
    pub qwen: ProviderConfig,
    pub deepseek: ProviderConfig,
    pub kimi: ProviderConfig,
    pub chatgpt: ProviderConfig,
    pub gemini: ProviderConfig,
}

#[derive(Clone)]
struct AppState {
    client: reqwest::Client,
    config: AppConfig,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum ProviderName {
    Deepseek,
    Kimi,
    Qwen,
    Chatgpt,
    Gemini,
}

impl ProviderName {
    fn as_str(self) -> &'static str {
        match self {
            Self::Deepseek => "deepseek",
            Self::Kimi => "kimi",
            Self::Qwen => "qwen",
            Self::Chatgpt => "chatgpt",
            Self::Gemini => "gemini",
        }
    }
}

const PROVIDER_ORDER: [ProviderName; 5] = [
    ProviderName::Qwen,
    ProviderName::Deepseek,
    ProviderName::Kimi,
    ProviderName::Chatgpt,
    ProviderName::Gemini,
];

#[derive(Debug, Serialize)]
struct ProviderHealth {
    provider: ProviderName,
    base_url: String,
    healthy: bool,
    status_code: Option<u16>,
    detail: Option<Value>,
}

#[derive(Debug, Deserialize, Serialize)]
struct StopRequest {
    #[serde(default)]
    completion_id: Option<String>,
    #[serde(default)]
    chat_id: Option<String>,
    #[serde(default)]
    response_id: Option<String>,
}

#[cfg(feature = "standalone-provider-cli")]
#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = load_config();

    match cli.command {
        Commands::Server => run_server(config).await,
    }
}

async fn run_server(config: AppConfig) -> Result<()> {
    let state = AppState {
        client: reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()?,
        config: config.clone(),
    };

    let app = Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .route("/providers", get(providers))
        .route("/openapi.json", get(openapi))
        .route("/v1/models", get(models))
        .route("/v1/models/{model}", get(model_by_id))
        .route("/v1/chat/completions", post(chat_completions))
        .route("/v1/chat/completions/stop", post(chat_completions_stop))
        .route("/v1/upload", post(upload))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let host: IpAddr = config
        .host
        .parse()
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    let addr = SocketAddr::new(host, config.port);
    println!("proxy-hub hub listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(feature = "standalone-provider-cli")]
fn load_config() -> AppConfig {
    AppConfig {
        host: std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_owned()),
        port: std::env::var("PORT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(3100),
        api_key: std::env::var("API_KEY")
            .ok()
            .filter(|value| !value.trim().is_empty()),
        qwen: ProviderConfig {
            base_url: std::env::var("QWEN_BASE_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:3000".to_owned()),
            api_key: std::env::var("QWEN_API_KEY")
                .ok()
                .filter(|value| !value.trim().is_empty()),
        },
        chatgpt: ProviderConfig {
            base_url: std::env::var("CHATGPT_BASE_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:3003".to_owned()),
            api_key: std::env::var("CHATGPT_API_KEY")
                .ok()
                .filter(|value| !value.trim().is_empty()),
        },
        gemini: ProviderConfig {
            base_url: std::env::var("GEMINI_BASE_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:3004".to_owned()),
            api_key: std::env::var("GEMINI_API_KEY")
                .ok()
                .filter(|value| !value.trim().is_empty()),
        },
        deepseek: ProviderConfig {
            base_url: std::env::var("DEEPSEEK_BASE_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:3001".to_owned()),
            api_key: std::env::var("DEEPSEEK_API_KEY")
                .ok()
                .filter(|value| !value.trim().is_empty()),
        },
        kimi: ProviderConfig {
            base_url: std::env::var("KIMI_BASE_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:3002".to_owned()),
            api_key: std::env::var("KIMI_API_KEY")
                .ok()
                .filter(|value| !value.trim().is_empty()),
        },
    }
}

async fn root(State(state): State<AppState>) -> impl IntoResponse {
    Json(json!({
        "name": "RustProxyHub",
        "status": "ok",
        "openapi": format!("http://127.0.0.1:{}/openapi.json", state.config.port),
        "routes": {
            "health": "/health",
            "providers": "/providers",
            "models": "/v1/models",
            "chat": "/v1/chat/completions",
            "stop": "/v1/chat/completions/stop",
            "upload": "/v1/upload",
        }
    }))
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let providers = provider_health_checks(&state).await;
    let healthy_count = providers.iter().filter(|provider| provider.healthy).count();
    Json(json!({
        "status": if healthy_count == PROVIDER_ORDER.len() { "ok" } else { "degraded" },
        "healthy_providers": healthy_count,
        "providers": providers,
    }))
}

async fn providers(State(state): State<AppState>) -> impl IntoResponse {
    Json(provider_health_checks(&state).await)
}

async fn openapi(State(state): State<AppState>) -> impl IntoResponse {
    Json(openapi_document(&state.config))
}

async fn models(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref()) {
        return *response;
    }

    match fetch_merged_models(&state).await {
        Ok(payload) => Json(payload).into_response(),
        Err(err) => json_error(StatusCode::BAD_GATEWAY, err.to_string()),
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

    match fetch_merged_models(&state).await {
        Ok(payload) => {
            let requested = normalize_prefixed_model(&model);
            let found = payload
                .get("data")
                .and_then(Value::as_array)
                .and_then(|items| {
                    items.iter().find(|item| {
                        item.get("id").and_then(Value::as_str) == Some(requested.model.as_str())
                    })
                })
                .cloned();

            match found {
                Some(item) => Json(item).into_response(),
                None => json_error(StatusCode::NOT_FOUND, "Model not found".to_owned()),
            }
        }
        Err(err) => json_error(StatusCode::BAD_GATEWAY, err.to_string()),
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

    let routed = normalize_prefixed_model(&body.model);
    let mut upstream_body = body.clone();
    upstream_body.model = routed.model.clone();

    if body.stream.unwrap_or(false) {
        return match proxy_stream_post(
            &state,
            routed.provider,
            "/v1/chat/completions",
            &upstream_body,
        )
        .await
        {
            Ok(response) => response,
            Err(err) => json_error(StatusCode::BAD_GATEWAY, err.to_string()),
        };
    }

    match proxy_json_post(
        &state,
        routed.provider,
        "/v1/chat/completions",
        &upstream_body,
        Some(body.model.clone()),
    )
    .await
    {
        Ok(response) => response,
        Err(err) => json_error(StatusCode::BAD_GATEWAY, err.to_string()),
    }
}

async fn chat_completions_stop(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<StopRequest>,
) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref()) {
        return *response;
    }

    let payload = serde_json::to_value(body).unwrap_or_else(|_| json!({}));
    match proxy_json_post(
        &state,
        ProviderName::Qwen,
        "/v1/chat/completions/stop",
        &payload,
        None,
    )
    .await
    {
        Ok(response) => response,
        Err(err) => json_error(StatusCode::BAD_GATEWAY, err.to_string()),
    }
}

async fn upload(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref()) {
        return *response;
    }

    let Some(content_type) = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
    else {
        return json_error(
            StatusCode::BAD_REQUEST,
            "multipart content-type is required".to_owned(),
        );
    };

    match proxy_bytes_post(&state, ProviderName::Qwen, "/v1/upload", content_type, body).await {
        Ok(response) => response,
        Err(err) => json_error(StatusCode::BAD_GATEWAY, err.to_string()),
    }
}

async fn provider_health_checks(state: &AppState) -> Vec<ProviderHealth> {
    let mut results = Vec::new();
    for provider in PROVIDER_ORDER {
        let config = state.provider_config(provider);
        let response = provider_request(&state.client, config, Method::GET, "/health")
            .send()
            .await;

        match response {
            Ok(response) => {
                let status = response.status();
                let detail = response.json::<Value>().await.ok();
                results.push(ProviderHealth {
                    provider,
                    base_url: config.base_url.clone(),
                    healthy: status.is_success(),
                    status_code: Some(status.as_u16()),
                    detail,
                });
            }
            Err(err) => results.push(ProviderHealth {
                provider,
                base_url: config.base_url.clone(),
                healthy: false,
                status_code: None,
                detail: Some(json!({ "error": err.to_string() })),
            }),
        }
    }
    results
}

async fn fetch_merged_models(state: &AppState) -> Result<Value> {
    let mut data = Vec::new();
    let mut errors = Vec::new();

    for provider in PROVIDER_ORDER {
        match fetch_provider_models(state, provider).await {
            Ok(mut items) => data.append(&mut items),
            Err(err) => errors.push(json!({
                "provider": provider.as_str(),
                "message": err.to_string(),
            })),
        }
    }

    if data.is_empty() && !errors.is_empty() {
        return Err(anyhow!("all upstream model lists failed"));
    }

    Ok(json!({
        "object": "list",
        "data": data,
        "errors": errors,
    }))
}

async fn fetch_provider_models(state: &AppState, provider: ProviderName) -> Result<Vec<Value>> {
    let response = provider_request(
        &state.client,
        state.provider_config(provider),
        Method::GET,
        "/v1/models",
    )
    .send()
    .await?;
    let status = response.status();
    if !status.is_success() {
        return Err(anyhow!(
            "{} models failed with status {}",
            provider.as_str(),
            status
        ));
    }

    let payload: Value = response.json().await?;
    let items = payload
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut output = Vec::new();
    for item in items {
        let mut object = match item {
            Value::Object(map) => map,
            _ => continue,
        };
        object.insert(
            "provider".to_owned(),
            Value::String(provider.as_str().to_owned()),
        );
        output.push(Value::Object(object));
    }
    Ok(output)
}

async fn proxy_json_post<T: serde::Serialize>(
    state: &AppState,
    provider: ProviderName,
    path: &str,
    payload: &T,
    override_model: Option<String>,
) -> Result<Response> {
    let response = provider_request(
        &state.client,
        state.provider_config(provider),
        Method::POST,
        path,
    )
    .json(payload)
    .send()
    .await?;

    let status = response.status();
    let bytes = response.bytes().await?;
    let mut value = serde_json::from_slice::<Value>(&bytes).unwrap_or_else(|_| {
        json!({
            "error": {
                "message": String::from_utf8_lossy(&bytes).to_string()
            }
        })
    });

    if let Some(model) = override_model.as_deref() {
        if let Some(object) = value.as_object_mut() {
            object.insert("model".to_owned(), Value::String(model.to_owned()));
        }
    }

    Ok((status, Json(value)).into_response())
}

async fn proxy_stream_post<T: serde::Serialize>(
    state: &AppState,
    provider: ProviderName,
    path: &str,
    payload: &T,
) -> Result<Response> {
    let response = provider_request(
        &state.client,
        state.provider_config(provider),
        Method::POST,
        path,
    )
    .json(payload)
    .send()
    .await?;

    let status = response.status();
    let headers = response.headers().clone();
    let stream = response
        .bytes_stream()
        .map_err(|err| std::io::Error::other(err.to_string()));

    let mut builder = Response::builder().status(status);
    for name in [
        header::CONTENT_TYPE,
        header::CACHE_CONTROL,
        header::CONNECTION,
    ] {
        if let Some(value) = headers.get(&name) {
            builder = builder.header(name.clone(), value);
        }
    }
    if let Some(value) = headers.get("x-accel-buffering") {
        builder = builder.header("x-accel-buffering", value);
    }

    builder
        .body(Body::from_stream(stream))
        .map_err(|err| anyhow!(err.to_string()))
}

async fn proxy_bytes_post(
    state: &AppState,
    provider: ProviderName,
    path: &str,
    content_type: &str,
    body: Bytes,
) -> Result<Response> {
    let response = provider_request(
        &state.client,
        state.provider_config(provider),
        Method::POST,
        path,
    )
    .header(header::CONTENT_TYPE, content_type)
    .body(body)
    .send()
    .await?;

    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .cloned()
        .unwrap_or_else(|| HeaderValue::from_static("application/json"));
    let bytes = response.bytes().await?;

    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type)
        .body(Body::from(bytes))
        .map_err(|err| anyhow!(err.to_string()))
}

fn provider_request(
    client: &reqwest::Client,
    config: &ProviderConfig,
    method: Method,
    path: &str,
) -> reqwest::RequestBuilder {
    let url = format!("{}{}", config.base_url.trim_end_matches('/'), path);
    let mut request = client.request(method, url);
    if let Some(api_key) = config.api_key.as_deref() {
        request = request.bearer_auth(api_key);
        request = request.header("x-api-key", api_key);
    }
    request
}

fn require_api_key(headers: &HeaderMap, api_key: Option<&str>) -> Result<(), Box<Response>> {
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
        Err(Box::new(json_error(
            StatusCode::UNAUTHORIZED,
            "Missing or invalid Authorization header".to_owned(),
        )))
    }
}

fn json_error(status: StatusCode, message: String) -> Response {
    (status, Json(json!({ "error": { "message": message } }))).into_response()
}

#[derive(Clone)]
struct RoutedModel {
    provider: ProviderName,
    model: String,
}

fn normalize_prefixed_model(model: &str) -> RoutedModel {
    let trimmed = model.trim();
    if let Some((prefix, actual)) = trimmed.split_once(':') {
        let provider = match prefix.to_ascii_lowercase().as_str() {
            "deepseek" => ProviderName::Deepseek,
            "kimi" => ProviderName::Kimi,
            "qwen" => ProviderName::Qwen,
            "chatgpt" => ProviderName::Chatgpt,
            "gemini" => ProviderName::Gemini,
            _ => infer_provider(trimmed),
        };
        return RoutedModel {
            provider,
            model: actual.to_owned(),
        };
    }

    RoutedModel {
        provider: infer_provider(trimmed),
        model: trimmed.to_owned(),
    }
}

fn infer_provider(model: &str) -> ProviderName {
    let lower = model.to_ascii_lowercase();
    if lower.starts_with("deepseek") {
        ProviderName::Deepseek
    } else if lower.starts_with("k2") || lower.starts_with("kimi") {
        ProviderName::Kimi
    } else if lower.starts_with("gemini") {
        ProviderName::Gemini
    } else if lower.starts_with("gpt")
        || lower.starts_with("o1")
        || lower.starts_with("o3")
        || lower.starts_with("o4")
        || lower.starts_with("chatgpt")
    {
        ProviderName::Chatgpt
    } else {
        ProviderName::Qwen
    }
}

impl AppState {
    fn provider_config(&self, provider: ProviderName) -> &ProviderConfig {
        match provider {
            ProviderName::Deepseek => &self.config.deepseek,
            ProviderName::Kimi => &self.config.kimi,
            ProviderName::Qwen => &self.config.qwen,
            ProviderName::Chatgpt => &self.config.chatgpt,
            ProviderName::Gemini => &self.config.gemini,
        }
    }
}

pub async fn serve_embedded(config: HubServiceConfig) -> Result<()> {
    run_server(AppConfig {
        host: config.host,
        port: config.port,
        api_key: config.api_key,
        qwen: config.qwen,
        deepseek: config.deepseek,
        kimi: config.kimi,
        chatgpt: config.chatgpt,
        gemini: config.gemini,
    })
    .await
}

fn openapi_document(config: &AppConfig) -> Value {
    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "RustProxyHub Unified API",
            "version": "0.1.0",
            "description": "Unified OpenAI-compatible gateway for the embedded Qwen, DeepSeek, Kimi, ChatGPT, and Gemini proxy services."
        },
        "servers": [
            { "url": format!("http://127.0.0.1:{}", config.port) }
        ],
        "components": {
            "securitySchemes": {
                "BearerAuth": {
                    "type": "http",
                    "scheme": "bearer"
                }
            },
            "schemas": {
                "ChatMessage": {
                    "type": "object",
                    "required": ["role"],
                    "properties": {
                        "role": { "type": "string" },
                        "content": {
                            "oneOf": [
                                { "type": "string" },
                                { "type": "array" },
                                { "type": "object" },
                                { "type": "null" }
                            ]
                        },
                        "tool_calls": { "type": "array" },
                        "tool_call_id": { "type": "string" },
                        "name": { "type": "string" },
                        "reasoning_content": { "type": "string" }
                    }
                },
                "ChatCompletionRequest": {
                    "type": "object",
                    "required": ["model", "messages"],
                    "properties": {
                        "model": {
                            "type": "string",
                            "description": "Model id. Raw ids are auto-routed; optional provider prefixes like qwen:model-id are also accepted."
                        },
                        "messages": {
                            "type": "array",
                            "items": { "$ref": "#/components/schemas/ChatMessage" }
                        },
                        "stream": { "type": "boolean" },
                        "tools": { "type": "array" },
                        "tool_choice": {},
                        "stream_options": {
                            "type": "object",
                            "properties": {
                                "include_usage": { "type": "boolean" }
                            }
                        }
                    }
                },
                "StopRequest": {
                    "type": "object",
                    "properties": {
                        "completion_id": { "type": "string" },
                        "chat_id": { "type": "string" },
                        "response_id": { "type": "string" }
                    }
                }
            }
        },
        "paths": {
            "/health": {
                "get": {
                    "summary": "Hub health and provider reachability",
                    "responses": {
                        "200": {
                            "description": "Hub health payload"
                        }
                    }
                }
            },
            "/providers": {
                "get": {
                    "summary": "List upstream provider status snapshots",
                    "responses": {
                        "200": { "description": "Provider status list" }
                    }
                }
            },
            "/openapi.json": {
                "get": {
                    "summary": "OpenAPI specification for the unified hub",
                    "responses": {
                        "200": { "description": "OpenAPI document" }
                    }
                }
            },
            "/v1/models": {
                "get": {
                    "summary": "Merged model list across Qwen, DeepSeek, and Kimi",
                    "security": [{ "BearerAuth": [] }],
                    "responses": {
                        "200": { "description": "OpenAI-style model list" },
                        "401": { "description": "Unauthorized" }
                    }
                }
            },
            "/v1/models/{model}": {
                "get": {
                    "summary": "Look up one merged model by id",
                    "security": [{ "BearerAuth": [] }],
                    "parameters": [
                        {
                            "name": "model",
                            "in": "path",
                            "required": true,
                            "schema": { "type": "string" }
                        }
                    ],
                    "responses": {
                        "200": { "description": "Model payload" },
                        "404": { "description": "Model not found" }
                    }
                }
            },
            "/v1/chat/completions": {
                "post": {
                    "summary": "Route one OpenAI chat request to the matching upstream provider",
                    "security": [{ "BearerAuth": [] }],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/ChatCompletionRequest" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Chat completion or SSE stream" },
                        "401": { "description": "Unauthorized" },
                        "502": { "description": "Upstream proxy error" }
                    }
                }
            },
            "/v1/chat/completions/stop": {
                "post": {
                    "summary": "Forward a stop request to the Qwen proxy",
                    "security": [{ "BearerAuth": [] }],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/StopRequest" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Stop response" }
                    }
                }
            },
            "/v1/upload": {
                "post": {
                    "summary": "Forward multipart uploads to the Qwen proxy",
                    "security": [{ "BearerAuth": [] }],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "multipart/form-data": {
                                "schema": {
                                    "type": "object",
                                    "properties": {
                                        "file": {
                                            "type": "string",
                                            "format": "binary"
                                        }
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Upload response" }
                    }
                }
            }
        }
    })
}
