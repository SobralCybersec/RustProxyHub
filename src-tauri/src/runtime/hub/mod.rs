use crate::proxy_core::{constant_time_eq, OpenAIRequest};
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
use futures_util::{future::join_all, TryStreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

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
    mistral: ProviderConfig,
    zai: ProviderConfig,
    meta: ProviderConfig,
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
    pub mistral: ProviderConfig,
    pub zai: ProviderConfig,
    pub meta: ProviderConfig,
}

#[derive(Clone)]
struct AppState {
    client: reqwest::Client,
    config: AppConfig,
    models_cache: Arc<tokio::sync::RwLock<Option<(serde_json::Value, std::time::Instant)>>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum ProviderName {
    Deepseek,
    Kimi,
    Qwen,
    Chatgpt,
    Gemini,
    Mistral,
    Zai,
    Meta,
}

impl ProviderName {
    fn as_str(self) -> &'static str {
        match self {
            Self::Deepseek => "deepseek",
            Self::Kimi => "kimi",
            Self::Qwen => "qwen",
            Self::Chatgpt => "chatgpt",
            Self::Gemini => "gemini",
            Self::Mistral => "mistral",
            Self::Zai => "zai",
            Self::Meta => "meta",
        }
    }
}

const PROVIDER_ORDER: [ProviderName; 8] = [
    ProviderName::Qwen,
    ProviderName::Deepseek,
    ProviderName::Kimi,
    ProviderName::Chatgpt,
    ProviderName::Gemini,
    ProviderName::Mistral,
    ProviderName::Zai,
    ProviderName::Meta,
];

#[derive(Debug, Serialize)]
struct ProviderHealth {
    provider: ProviderName,
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

async fn run_server(config: AppConfig) -> Result<()> {
    let state = AppState {
        client: reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .tcp_nodelay(true)
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(16)
            .timeout(Duration::from_secs(120))
            .build()?,
        config: config.clone(),
        models_cache: Arc::new(tokio::sync::RwLock::new(None)),
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
        .route("/v1/responses", post(responses))
        .route("/v1/messages", post(anthropic_messages))
        .route("/v1/messages/count_tokens", post(anthropic_count_tokens))
        .route("/v1/upload", post(upload))
        .with_state(state);

    let host: IpAddr = config
        .host
        .parse()
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    let is_localhost = host.is_loopback();
    if !is_localhost && config.api_key.as_deref().map(str::is_empty).unwrap_or(true) {
        return Err(anyhow!(
            "refusing to start hub on non-loopback host {host} without API_KEY set; set API_KEY or bind to 127.0.0.1/localhost"
        ));
    }
    let addr = SocketAddr::new(host, config.port);
    println!("proxy-hub hub listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
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
                "responses": "/v1/responses",
                "messages": "/v1/messages",
                "count_tokens": "/v1/messages/count_tokens",
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
        Err(err) => bad_gateway_error(&err),
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
        Err(err) => bad_gateway_error(&err),
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
    // Avoid full clone when model string is unchanged (no prefix was stripped).
    // upstream_body_owned is only initialized in the true branch; ProviderName is Copy.
    let upstream_body_owned;
    let upstream_body: &OpenAIRequest = if routed.model != body.model {
        upstream_body_owned = {
            let mut b = body.clone();
            b.model = routed.model;
            b
        };
        &upstream_body_owned
    } else {
        // ponytail: model string swap; full clone acceptable for large context until OpenAIRequest gets Cow<str> model
        &body
    };

    if body.stream.unwrap_or(false) {
        return match proxy_stream_post(
            &state,
            routed.provider,
            "/v1/chat/completions",
            upstream_body,
        )
        .await
        {
            Ok(response) => response,
            Err(err) => bad_gateway_error(&err),
        };
    }

    match proxy_json_post(
        &state,
        routed.provider,
        "/v1/chat/completions",
        upstream_body,
        Some(body.model.clone()),
    )
    .await
    {
        Ok(response) => response,
        Err(err) => bad_gateway_error(&err),
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
        Err(err) => bad_gateway_error(&err),
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

    proxy_model_json(&state, body, "/v1/responses").await
}

async fn anthropic_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref()) {
        return *response;
    }

    proxy_model_json(&state, body, "/v1/messages").await
}

async fn anthropic_count_tokens(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref()) {
        return *response;
    }

    let routed = normalize_json_model(body);
    match proxy_json_post(
        &state,
        routed.provider,
        "/v1/messages/count_tokens",
        &routed.payload,
        None,
    )
    .await
    {
        Ok(response) => response,
        Err(err) => bad_gateway_error(&err),
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
        Err(err) => bad_gateway_error(&err),
    }
}

async fn proxy_model_json(state: &AppState, body: Value, path: &str) -> Response {
    let routed = normalize_json_model(body);
    if routed
        .payload
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return match proxy_stream_post(state, routed.provider, path, &routed.payload).await {
            Ok(response) => response,
            Err(err) => bad_gateway_error(&err),
        };
    }

    match proxy_json_post(
        state,
        routed.provider,
        path,
        &routed.payload,
        routed.original_model,
    )
    .await
    {
        Ok(response) => response,
        Err(err) => bad_gateway_error(&err),
    }
}

async fn provider_health_checks(state: &AppState) -> Vec<ProviderHealth> {
    join_all(PROVIDER_ORDER.into_iter().map(|provider| async move {
        let config = state.provider_config(provider);
        let response = provider_request(&state.client, config, Method::GET, "/health")
            .send()
            .await;

        match response {
            Ok(response) => {
                let status = response.status();
                let detail = response.json::<Value>().await.ok();
                ProviderHealth {
                    provider,
                    healthy: status.is_success(),
                    status_code: Some(status.as_u16()),
                    detail,
                }
            }
            Err(_err) => ProviderHealth {
                provider,
                healthy: false,
                status_code: None,
                detail: Some(json!({ "error": "upstream health check failed" })),
            },
        }
    }))
    .await
}

async fn fetch_merged_models(state: &AppState) -> Result<Value> {
    const TTL: Duration = Duration::from_secs(30);

    // read lock — concurrent callers share the fast path
    {
        let cached = state.models_cache.read().await;
        if let Some((ref val, ts)) = *cached {
            if ts.elapsed() < TTL {
                return Ok(val.clone());
            }
        }
    }

    // cache miss — fan out to all providers
    let mut data = Vec::new();
    let mut errors = Vec::new();

    for result in
        join_all(PROVIDER_ORDER.into_iter().map(|provider| async move {
            (provider, fetch_provider_models(state, provider).await)
        }))
        .await
    {
        match result {
            (_provider, Ok(mut items)) => data.append(&mut items),
            (provider, Err(err)) => errors.push(json!({
                "provider": provider.as_str(),
                "message": err.to_string(),
            })),
        }
    }

    if data.is_empty() && !errors.is_empty() {
        return Err(anyhow!("all upstream model lists failed"));
    }

    let result = json!({ "object": "list", "data": data, "errors": errors });
    *state.models_cache.write().await = Some((result.clone(), std::time::Instant::now()));
    Ok(result)
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

    Ok(tag_provider_models(items, provider))
}

fn tag_provider_models(items: Vec<Value>, provider: ProviderName) -> Vec<Value> {
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
    output
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
    let mut value = serde_json::from_slice::<Value>(&bytes).map_err(|_| {
        // Don't echo raw upstream body to the client; log server-side only.
        anyhow::anyhow!(
            "upstream returned non-JSON body ({} bytes): {:?}",
            bytes.len(),
            String::from_utf8_lossy(&bytes)
        )
    })?;

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
    match provided {
        Some(provided) if constant_time_eq(provided, api_key) => Ok(()),
        _ => Err(Box::new(json_error(
            StatusCode::UNAUTHORIZED,
            "Missing or invalid Authorization header".to_owned(),
        ))),
    }
}

fn json_error(status: StatusCode, message: String) -> Response {
    (status, Json(json!({ "error": { "message": message } }))).into_response()
}

/// Map an upstream error to a generic 502 with an opaque id; log the real cause server-side
/// so internal base URLs / IPs / path fragments never reach the client.
fn bad_gateway_error(err: impl std::fmt::Display) -> Response {
    let id = uuid::Uuid::new_v4();
    eprintln!("[hub] upstream error {id}: {err}");
    json_error(
        StatusCode::BAD_GATEWAY,
        format!("upstream provider error (id={id})"),
    )
}

#[derive(Clone)]
struct RoutedModel {
    provider: ProviderName,
    model: String,
}

struct RoutedJson {
    provider: ProviderName,
    payload: Value,
    original_model: Option<String>,
}

fn normalize_json_model(mut payload: Value) -> RoutedJson {
    let original_model = payload
        .get("model")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let routed = original_model
        .as_deref()
        .map(normalize_prefixed_model)
        .unwrap_or_else(|| RoutedModel {
            provider: ProviderName::Chatgpt,
            model: String::new(),
        });

    if !routed.model.is_empty() {
        if let Some(object) = payload.as_object_mut() {
            object.insert("model".to_owned(), Value::String(routed.model));
        }
    }

    RoutedJson {
        provider: routed.provider,
        payload,
        original_model,
    }
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
            "mistral" => ProviderName::Mistral,
            "zai" => ProviderName::Zai,
            "meta" => ProviderName::Meta,
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
    } else if lower.starts_with("k2") || lower.starts_with("k3") || lower.starts_with("kimi") {
        ProviderName::Kimi
    } else if lower.starts_with("gemini") {
        ProviderName::Gemini
    } else if lower.starts_with("mistral")
        || lower.starts_with("magistral")
        || lower.starts_with("codestral")
    {
        ProviderName::Mistral
    } else if lower.starts_with("glm") || lower.starts_with("autoglm") || lower.starts_with("zai") {
        ProviderName::Zai
    } else if lower.starts_with("meta") || lower.starts_with("llama") {
        ProviderName::Meta
    } else if lower.starts_with("gpt")
        || lower.starts_with("o1")
        || lower.starts_with("o3")
        || lower.starts_with("o4")
        || lower.starts_with("chatgpt")
        || lower.starts_with("claude")
        || lower.starts_with("anthropic.")
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
            ProviderName::Mistral => &self.config.mistral,
            ProviderName::Zai => &self.config.zai,
            ProviderName::Meta => &self.config.meta,
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
        mistral: config.mistral,
        zai: config.zai,
        meta: config.meta,
    })
    .await
}

fn openapi_document(config: &AppConfig) -> Value {
    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "RustProxyHub Unified API",
            "version": "0.1.0",
            "description": "Unified OpenAI-compatible gateway for the embedded Qwen, DeepSeek, Kimi, ChatGPT, Gemini, Mistral, Z.AI, and Meta AI proxy services."
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
            "/v1/responses": {
                "post": {
                    "summary": "Route one OpenAI Responses request to the matching upstream provider",
                    "security": [{ "BearerAuth": [] }],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "type": "object" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Responses API payload or SSE stream" },
                        "401": { "description": "Unauthorized" },
                        "502": { "description": "Upstream proxy error" }
                    }
                }
            },
            "/v1/messages": {
                "post": {
                    "summary": "Route one Anthropic Messages request to the matching upstream provider",
                    "security": [{ "BearerAuth": [] }],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "type": "object" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Anthropic message payload or SSE stream" },
                        "401": { "description": "Unauthorized" },
                        "502": { "description": "Upstream proxy error" }
                    }
                }
            },
            "/v1/messages/count_tokens": {
                "post": {
                    "summary": "Route one Anthropic token-count request to the matching upstream provider",
                    "security": [{ "BearerAuth": [] }],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "type": "object" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Anthropic token count payload" },
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

#[cfg(test)]
mod tests {
    use super::{
        infer_provider, normalize_json_model, normalize_prefixed_model, tag_provider_models,
        ProviderName,
    };
    use serde_json::{json, Value};

    #[test]
    fn routes_prefixed_mistral_models_to_mistral() {
        let routed = normalize_prefixed_model("mistral:mistral-large-latest");
        assert_eq!(routed.provider, ProviderName::Mistral);
        assert_eq!(routed.model, "mistral-large-latest");
    }

    #[test]
    fn routes_prefixed_zai_models_to_zai() {
        let routed = normalize_prefixed_model("zai:glm-5.2");
        assert_eq!(routed.provider, ProviderName::Zai);
        assert_eq!(routed.model, "glm-5.2");
    }

    #[test]
    fn routes_prefixed_meta_models_to_meta() {
        let routed = normalize_prefixed_model("meta:meta-ai-web-session");
        assert_eq!(routed.provider, ProviderName::Meta);
        assert_eq!(routed.model, "meta-ai-web-session");
    }

    #[test]
    fn infers_mistral_family_models() {
        assert_eq!(infer_provider("mistral-medium"), ProviderName::Mistral);
        assert_eq!(infer_provider("magistral-medium"), ProviderName::Mistral);
        assert_eq!(infer_provider("codestral-latest"), ProviderName::Mistral);
    }

    #[test]
    fn infers_glm_family_models_to_zai() {
        assert_eq!(infer_provider("glm-5.2"), ProviderName::Zai);
        assert_eq!(infer_provider("autoglm-agent"), ProviderName::Zai);
    }

    #[test]
    fn infers_meta_family_models() {
        assert_eq!(infer_provider("meta-ai-web-session"), ProviderName::Meta);
        assert_eq!(infer_provider("llama-4"), ProviderName::Meta);
    }

    #[test]
    fn infers_claude_family_models_to_browser_provider() {
        assert_eq!(infer_provider("claude-sonnet-4-5"), ProviderName::Chatgpt);
        assert_eq!(
            infer_provider("anthropic.claude-sonnet-4"),
            ProviderName::Chatgpt
        );
    }

    #[test]
    fn normalizes_prefixed_json_model_payloads() {
        let routed = normalize_json_model(json!({
            "model": "mistral:mistral-large-latest",
            "input": "hello"
        }));

        assert_eq!(routed.provider, ProviderName::Mistral);
        assert_eq!(
            routed.original_model.as_deref(),
            Some("mistral:mistral-large-latest")
        );
        assert_eq!(
            routed.payload.get("model").and_then(Value::as_str),
            Some("mistral-large-latest")
        );
    }

    #[test]
    fn infers_kimi_k2_family_models() {
        assert_eq!(infer_provider("k2d6"), ProviderName::Kimi);
        assert_eq!(infer_provider("k2d6-thinking"), ProviderName::Kimi);
        assert_eq!(infer_provider("kimi-k2.6"), ProviderName::Kimi);
        assert_eq!(infer_provider("kimi-k2.6-thinking"), ProviderName::Kimi);
    }

    #[test]
    fn infers_kimi_k3_family_models() {
        assert_eq!(infer_provider("k3"), ProviderName::Kimi);
        assert_eq!(infer_provider("kimi-k3"), ProviderName::Kimi);
        assert_eq!(infer_provider("kimi-k3-thinking"), ProviderName::Kimi);
        assert_eq!(infer_provider("kimi-latest"), ProviderName::Kimi);
    }

    #[test]
    fn routes_prefixed_kimi_models() {
        let routed = normalize_prefixed_model("kimi:kimi-k3");
        assert_eq!(routed.provider, ProviderName::Kimi);
        assert_eq!(routed.model, "kimi-k3");
    }

    #[test]
    fn model_merge_tags_provider_and_skips_invalid_items() {
        let items = vec![
            json!({ "id": "mistral-web-session" }),
            Value::String("bad".to_owned()),
        ];

        let tagged = tag_provider_models(items, ProviderName::Mistral);

        assert_eq!(tagged.len(), 1);
        assert_eq!(
            tagged[0].get("provider").and_then(Value::as_str),
            Some("mistral")
        );
        assert_eq!(
            tagged[0].get("id").and_then(Value::as_str),
            Some("mistral-web-session")
        );
    }

    #[test]
    fn infers_deepseek_family_models() {
        assert_eq!(infer_provider("deepseek-v4-flash"), ProviderName::Deepseek);
        assert_eq!(infer_provider("deepseek-v4-pro"), ProviderName::Deepseek);
        assert_eq!(
            infer_provider("deepseek-v4-flash-thinking"),
            ProviderName::Deepseek
        );
        assert_eq!(infer_provider("deepseek-instant"), ProviderName::Deepseek);
    }

    #[test]
    fn infers_gemini_family_models() {
        assert_eq!(infer_provider("gemini-2.5-pro"), ProviderName::Gemini);
        assert_eq!(infer_provider("gemini-flash"), ProviderName::Gemini);
        assert_eq!(infer_provider("gemini-exp-1206"), ProviderName::Gemini);
    }

    #[test]
    fn infers_chatgpt_gpt_series() {
        assert_eq!(infer_provider("gpt-4o"), ProviderName::Chatgpt);
        assert_eq!(infer_provider("gpt-4o-mini"), ProviderName::Chatgpt);
        assert_eq!(infer_provider("o3"), ProviderName::Chatgpt);
        assert_eq!(infer_provider("o4-mini"), ProviderName::Chatgpt);
        assert_eq!(infer_provider("o1-preview"), ProviderName::Chatgpt);
        assert_eq!(infer_provider("chatgpt-web-session"), ProviderName::Chatgpt);
    }

    #[test]
    fn infers_qwen_as_default_fallback() {
        assert_eq!(infer_provider("qwen3-30b-a3b"), ProviderName::Qwen);
        assert_eq!(infer_provider("qwq-32b-preview"), ProviderName::Qwen);
        // Anything unrecognised lands on Qwen
        assert_eq!(
            infer_provider("unknown-future-model-xyz"),
            ProviderName::Qwen
        );
    }

    #[test]
    fn routes_prefixed_deepseek_model() {
        let routed = normalize_prefixed_model("deepseek:deepseek-v4-flash");
        assert_eq!(routed.provider, ProviderName::Deepseek);
        assert_eq!(routed.model, "deepseek-v4-flash");
    }

    #[test]
    fn routes_prefixed_gemini_model() {
        let routed = normalize_prefixed_model("gemini:gemini-2.5-pro");
        assert_eq!(routed.provider, ProviderName::Gemini);
        assert_eq!(routed.model, "gemini-2.5-pro");
    }

    #[test]
    fn routes_prefixed_qwen_model() {
        let routed = normalize_prefixed_model("qwen:qwen3-30b-a3b");
        assert_eq!(routed.provider, ProviderName::Qwen);
        assert_eq!(routed.model, "qwen3-30b-a3b");
    }

    #[test]
    fn model_without_prefix_infers_provider() {
        // bare model names route by infer_provider (no colon)
        let routed = normalize_prefixed_model("deepseek-v4-flash");
        assert_eq!(routed.provider, ProviderName::Deepseek);
        assert_eq!(routed.model, "deepseek-v4-flash");
    }

    #[test]
    fn tag_provider_models_empty_input() {
        let tagged = tag_provider_models(vec![], ProviderName::Kimi);
        assert!(tagged.is_empty());
    }

    #[test]
    fn tag_provider_models_all_invalid() {
        let items = vec![Value::Bool(true), json!(null)];
        let tagged = tag_provider_models(items, ProviderName::Kimi);
        assert!(tagged.is_empty());
    }
}
