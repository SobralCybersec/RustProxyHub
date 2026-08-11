mod account_manager;
pub mod accounts;
mod cache;
mod conversation_registry;
pub mod config;
mod metrics;
mod model_registry;
mod stream_registry;
mod upload;
mod watchdog;

/* was flattened by the old lib.rs wrapper; keep it reachable as qwen::build_embedded_config */
pub use config::build_embedded_config;

use crate::browser_bridge::{
    BrowserBridge, CaptureHeadersParams, CloseAccountParams, InitParams, ManualLoginParams,
    PlaywrightBridge,
};
use crate::proxy_core::{
    constant_time_eq, current_timestamp, preflight_request_to_budget, sse_done, sse_json,
    usage_from_text, MessageToolCall, OpenAIRequest, PromptPreflightOptions, StreamingToolParser,
    ToolCallFunction,
};
use anyhow::{anyhow, Result};
use async_stream::stream;
use axum::{
    body::Body,
    extract::{Multipart, Path, State},
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
    collections::{HashMap, HashSet},
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};
use uuid::Uuid;

const QWEN_WEB_VERSION: &str = "0.2.66";

use self::{
    account_manager::AccountManager,
    accounts::{global_account, AccountStore, QwenAccount},
    cache::MemoryCache,
    conversation_registry::{ConversationRegistry, QwenConversation},
    config::{
        ensure_runtime_layout, legacy_accounts_json_candidates, legacy_db_candidates,
        workspace_root, AppConfig,
    },
    metrics::Metrics,
    model_registry::{normalize_model_id, ModelRegistry, MAX_PAYLOAD_SIZE},
    stream_registry::StreamRegistry,
    upload::{prepare_multimodal_uploads, upload_bytes_to_qwen, MediaUploadInput},
    watchdog::Watchdog,
};

#[derive(Clone)]
struct AppState {
    bridge: Arc<PlaywrightBridge>,
    client: reqwest::Client,
    config: AppConfig,
    accounts: AccountStore,
    account_manager: AccountManager,
    model_registry: ModelRegistry,
    metrics: Metrics,
    cache: MemoryCache,
    conversations: ConversationRegistry,
    stream_registry: StreamRegistry,
    watchdog: Watchdog,
}

#[derive(Default)]
struct QwenParseState {
    target_response_id: Option<String>,
    current_thought_index: usize,
    last_full_content: String,
    reasoning: String,
    prompt_tokens: usize,
    completion_tokens: usize,
}

enum QwenEvent {
    Reasoning(String),
    Text(String),
    ToolCall(MessageToolCall),
}

#[derive(Debug)]
struct QwenRequestError {
    message: String,
    upstream_code: Option<String>,
    upstream_status: Option<u16>,
    retry_after_ms: Option<u64>,
}

impl std::fmt::Display for QwenRequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for QwenRequestError {}

#[derive(Deserialize)]
struct StopRequest {
    #[serde(default)]
    completion_id: Option<String>,
    #[serde(default)]
    chat_id: Option<String>,
    #[serde(default)]
    response_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ManualLoginRequest {
    #[serde(default)]
    browser: Option<String>,
    #[serde(default)]
    account_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CloseLoginRequest {
    #[serde(default)]
    account_id: Option<String>,
}

struct ServerRuntime {
    accounts: AccountStore,
    metrics: Metrics,
    cache: MemoryCache,
    model_registry: ModelRegistry,
    stream_registry: StreamRegistry,
    watchdog: Watchdog,
}

async fn run_server(
    bridge: Arc<PlaywrightBridge>,
    config: AppConfig,
    runtime: ServerRuntime,
) -> Result<()> {
    let state = AppState {
        bridge,
        client: reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(config.chat_timeout)
            .redirect(reqwest::redirect::Policy::none())
            .tcp_nodelay(true)
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(16)
            .build()?,
        config: config.clone(),
        accounts: runtime.accounts,
        account_manager: AccountManager::new(),
        model_registry: runtime.model_registry,
        metrics: runtime.metrics,
        cache: runtime.cache,
        conversations: ConversationRegistry::default(),
        stream_registry: runtime.stream_registry,
        watchdog: runtime.watchdog,
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics_route))
        .route("/admin/status", get(admin_status))
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
            "active_streams": state.stream_registry.snapshots().await,
            "cache": state.cache.stats().await,
            "watchdog": state.watchdog.snapshot().await,
            "metrics": state.metrics.snapshot_json().await,
        }))
        .into_response(),
        Err(err) => json_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
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

async fn handle_chat(state: AppState, body: OpenAIRequest) -> Result<Response> {
    let (normalized, pending_uploads) = normalize_request(&body);
    let prompt_budget = state
        .model_registry
        .context_window(&normalized.model)
        .await
        .saturating_sub(1000);
    let preflight = preflight_request_to_budget(
        &normalized,
        &PromptPreflightOptions {
            max_prompt_tokens: Some(prompt_budget),
            extra_system_instructions: None,
            dedup_system_blocks: false,
            structured_compaction_max_chars: None,
        },
        |text| {
            state
                .model_registry
                .estimate_tokens(text, &normalized.model)
        },
    )?;
    let final_prompt = preflight.flat_prompt;
    let completion_id = format!("chatcmpl-{}", Uuid::new_v4());
    let is_stream = body.stream.unwrap_or(false);
    let include_usage = body
        .stream_options
        .as_ref()
        .and_then(|options| options.include_usage)
        .unwrap_or(false);

    let session_key = qwen_session_key(body.user.as_deref());
    let existing_conversation = state.conversations.get(&session_key).await;
    let all_accounts = effective_accounts(&state.accounts)?;
    let accounts = if let Some(conversation) = &existing_conversation {
        match all_accounts
            .iter()
            .find(|account| account.id == conversation.account_id)
        {
            Some(account) => vec![account.clone()],
            None => {
                state.conversations.remove(&session_key).await;
                all_accounts
            }
        }
    } else {
        all_accounts
    };
    let mut current_account = state.account_manager.select_next(&accounts, false).await;
    let mut tried_accounts = HashSet::new();
    let mut last_error: Option<anyhow::Error> = None;

    while let Some(account) = current_account {
        if tried_accounts.contains(&account.id) {
            current_account = state
                .account_manager
                .select_next_available(&accounts, Some(&account.id))
                .await;
            continue;
        }
        tried_accounts.insert(account.id.clone());

        let bridge_account_id = account_id_for_bridge(&account);
        let header_result = state
            .bridge
            .capture_headers(CaptureHeadersParams {
                force_new: false,
                account_id: bridge_account_id,
            })
            .await;
        let basic_headers = match header_result {
            Ok(headers) => headers.headers,
            Err(err) => {
                last_error = Some(anyhow!(err));
                current_account = state
                    .account_manager
                    .select_next_available(&accounts, Some(&account.id))
                    .await;
                continue;
            }
        };

        let files = if pending_uploads.is_empty() {
            Vec::new()
        } else {
            match prepare_multimodal_uploads(
                &state.client,
                &state.config.qwen_base_url,
                &basic_headers,
                &pending_uploads,
            )
            .await
            {
                Ok(files) => files,
                Err(err) => return Err(err),
            }
        };

        let mut conversation = existing_conversation
            .as_ref()
            .filter(|conversation| conversation.account_id == account.id)
            .cloned();
        let mut retries = 3usize;
        let mut retry_delay_ms = 500u64;

        loop {
            let chat_id = match &conversation {
                Some(conversation) => conversation.chat_id.clone(),
                None => match create_qwen_chat(&state.client, &state.config, &basic_headers).await {
                    Ok(chat_id) => chat_id,
                    Err(err) => {
                        last_error = Some(err);
                        break;
                    }
                },
            };
            let parent_id = conversation
                .as_ref()
                .and_then(|conversation| conversation.parent_id.clone());
            state
                .conversations
                .upsert(
                    session_key.clone(),
                    QwenConversation {
                        chat_id: chat_id.clone(),
                        account_id: account.id.clone(),
                        parent_id: parent_id.clone(),
                    },
                )
                .await;
            conversation = state.conversations.get(&session_key).await;

            match request_qwen_chat(
                &state,
                &body,
                &final_prompt,
                &chat_id,
                parent_id.as_deref(),
                &basic_headers,
                &files,
            )
            .await
            {
                Ok(response) => {
                    let cancel_token = state
                        .stream_registry
                        .register(
                            completion_id.clone(),
                            chat_id.clone(),
                            account.id.clone(),
                            basic_headers.clone(),
                        )
                        .await;
                    state
                        .metrics
                        .gauge(
                            "streams.active",
                            state.stream_registry.active_count().await as f64,
                        )
                        .await;

                    if !is_stream {
                        let result = build_non_stream_response(
                            &state,
                            &body,
                            &final_prompt,
                            QwenConversationRef {
                                chat_id: &chat_id,
                                session_key: &session_key,
                            },
                            &completion_id,
                            response,
                            cancel_token,
                        )
                        .await;
                        state
                            .stream_registry
                            .remove_by_completion_id(&completion_id)
                            .await;
                        state
                            .metrics
                            .gauge(
                                "streams.active",
                                state.stream_registry.active_count().await as f64,
                            )
                            .await;
                        return result;
                    }

                    return Ok(build_stream_response(StreamResponseArgs {
                        state,
                        body: body.clone(),
                        final_prompt,
                        chat_id,
                        session_key,
                        completion_id,
                        response,
                        cancel_token,
                        include_usage,
                    }));
                }
                Err(err) => {
                    retries = retries.saturating_sub(1);
                    if err.upstream_status == Some(429)
                        || err.upstream_code.as_deref() == Some("RateLimited")
                    {
                        state
                            .account_manager
                            .mark_rate_limited(&account.id, err.retry_after_ms, "RateLimited")
                            .await;
                        last_error = Some(anyhow!(err.message));
                        break;
                    }

                    let retryable = err.retry_after_ms.is_some()
                        || err.message.contains("chat is in progress")
                        || err.message.contains("Bad_Request");
                    if retryable && retries > 0 {
                        let delay = err.retry_after_ms.unwrap_or(retry_delay_ms);
                        tokio::time::sleep(Duration::from_millis(delay)).await;
                        retry_delay_ms = (retry_delay_ms * 2).min(5_000);
                        continue;
                    }

                    if retries == 0 && err.upstream_status.unwrap_or(500) >= 500 {
                        state
                            .account_manager
                            .mark_rate_limited(&account.id, None, "ServerError")
                            .await;
                    }
                    last_error = Some(anyhow!(err.message));
                    break;
                }
            }
        }

        current_account = state
            .account_manager
            .select_next_available(&accounts, Some(&account.id))
            .await;
    }

    Err(last_error.unwrap_or_else(|| anyhow!("All accounts failed")))
}

struct QwenConversationRef<'a> {
    chat_id: &'a str,
    session_key: &'a str,
}

async fn build_non_stream_response(
    state: &AppState,
    body: &OpenAIRequest,
    final_prompt: &str,
    conversation: QwenConversationRef<'_>,
    completion_id: &str,
    response: reqwest::Response,
    cancel_token: tokio_util::sync::CancellationToken,
) -> Result<Response> {
    let mut parse_state = QwenParseState::default();
    let mut tool_parser = body.tools.as_ref().map(|_| StreamingToolParser::new());
    let mut tool_calls = Vec::new();
    let mut buffer = String::new();
    let mut offset = 0usize;
    let mut bytes_stream = response.bytes_stream();

    loop {
        tokio::select! {
            _ = cancel_token.cancelled() => {
                return Err(anyhow!("stream cancelled"));
            }
            chunk = bytes_stream.next() => {
                let Some(chunk) = chunk else { break; };
                let chunk = chunk?;
                buffer.push_str(&String::from_utf8_lossy(&chunk));
                while let Some(rel) = buffer[offset..].find('\n') {
                    let end = offset + rel;
                    let line = buffer[offset..end].trim().to_owned();
                    offset = end + 1;
                    if let Some(data) = line.strip_prefix("data: ") {
                        for event in collect_qwen_events(
                            data,
                            completion_id,
                            &state.stream_registry,
                            &mut parse_state,
                            &mut tool_parser,
                        )
                        .await?
                        {
                            if let QwenEvent::ToolCall(tool_call) = event {
                                tool_calls.push(tool_call);
                            }
                        }
                    }
                }
                // drain in place: offset is right after '\n' (ASCII), always a char boundary
                buffer.drain(..offset);
                offset = 0;
            }
        }
    }

    if let Some(parser) = &mut tool_parser {
        let flush = parser.flush();
        if !flush.text.is_empty() {
            if parse_state.last_full_content.is_empty() {
                parse_state.last_full_content = flush.text;
            } else {
                parse_state.last_full_content.push_str(&flush.text);
            }
        }
        for parsed_tool in flush.tool_calls {
            tool_calls.push(tool_call_from_parsed(parsed_tool));
        }
    }

    if let Some(parent_id) = parse_state.target_response_id.clone() {
        state
            .conversations
            .update_parent(conversation.session_key, conversation.chat_id, parent_id)
            .await;
    }

    let usage = usage_from_text(
        final_prompt,
        &format!("{}{}", parse_state.reasoning, parse_state.last_full_content),
        true,
    );
    let message = if tool_calls.is_empty() {
        json!({
            "role": "assistant",
            "content": parse_state.last_full_content,
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

    Ok(Json(json!({
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
    .into_response())
}

struct StreamResponseArgs {
    state: AppState,
    body: OpenAIRequest,
    final_prompt: String,
    chat_id: String,
    session_key: String,
    completion_id: String,
    response: reqwest::Response,
    cancel_token: tokio_util::sync::CancellationToken,
    include_usage: bool,
}

fn build_stream_response(args: StreamResponseArgs) -> Response {
    let StreamResponseArgs {
        state,
        body,
        final_prompt,
        chat_id,
        session_key,
        completion_id,
        response,
        cancel_token,
        include_usage,
    } = args;
    let model = body.model.clone();
    let stream_registry = state.stream_registry.clone();
    let conversations = state.conversations.clone();
    let metrics = state.metrics.clone();
    /* frees the registry slot even if the client disconnects before the generator
    reaches its explicit remove below */
    let cleanup_guard = stream_registry.guard(completion_id.clone());

    let stream = stream! {
        let _cleanup_guard = cleanup_guard;
        yield Ok::<Bytes, std::convert::Infallible>(Bytes::from(": heartbeat\n\n"));
        yield Ok(sse_json(json!({
            "id": completion_id,
            "object": "chat.completion.chunk",
            "created": current_timestamp(),
            "model": model,
            "choices": [{ "index": 0, "delta": { "role": "assistant", "content": "" }, "logprobs": Value::Null, "finish_reason": Value::Null }]
        })));

        let mut parse_state = QwenParseState::default();
        let mut tool_parser = body.tools.as_ref().map(|_| StreamingToolParser::new());
        let mut tool_index = 0usize;
        let mut buffer = String::new();
        let mut offset = 0usize;
        let mut bytes_stream = response.bytes_stream();

        loop {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    yield Ok(sse_done());
                    break;
                }
                chunk = bytes_stream.next() => {
                    match chunk {
                        Some(Ok(chunk)) => {
                            buffer.push_str(&String::from_utf8_lossy(&chunk));
                            while let Some(rel) = buffer[offset..].find('\n') {
                                let end = offset + rel;
                                let line = buffer[offset..end].trim().to_owned();
                                offset = end + 1;
                                if let Some(data) = line.strip_prefix("data: ") {
                                    match collect_qwen_events(data, &completion_id, &stream_registry, &mut parse_state, &mut tool_parser).await {
                                        Ok(events) => {
                                            for event in events {
                                                match event {
                                                    QwenEvent::Reasoning(content) => {
                                                        yield Ok(sse_json(json!({
                                                            "id": completion_id,
                                                            "object": "chat.completion.chunk",
                                                            "created": current_timestamp(),
                                                            "model": model,
                                                            "choices": [{ "index": 0, "delta": { "reasoning_content": content }, "logprobs": Value::Null, "finish_reason": Value::Null }]
                                                        })));
                                                    }
                                                    QwenEvent::Text(content) => {
                                                        yield Ok(sse_json(json!({
                                                            "id": completion_id,
                                                            "object": "chat.completion.chunk",
                                                            "created": current_timestamp(),
                                                            "model": model,
                                                            "choices": [{ "index": 0, "delta": { "content": content }, "logprobs": Value::Null, "finish_reason": Value::Null }]
                                                        })));
                                                    }
                                                    QwenEvent::ToolCall(tool_call) => {
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
                                                "choices": [{ "index": 0, "delta": { "content": format!("Qwen parse error: {err}") }, "logprobs": Value::Null, "finish_reason": "stop" }]
                                            })));
                                            yield Ok(sse_done());
                                            break;
                                        }
                                    }
                                }
                            }
                            // drain in place: offset is right after '\n' (ASCII), always a char boundary
                            buffer.drain(..offset);
                            offset = 0;
                        }
                        Some(Err(err)) => {
                            metrics.increment("streams.errors", 1.0).await;
                            yield Ok(sse_json(json!({
                                "id": completion_id,
                                "object": "chat.completion.chunk",
                                "created": current_timestamp(),
                                "model": model,
                                "choices": [{ "index": 0, "delta": { "content": format!("Qwen upstream error: {err}") }, "logprobs": Value::Null, "finish_reason": "stop" }]
                            })));
                            yield Ok(sse_done());
                            break;
                        }
                        None => break,
                    }
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

        let usage = json!({
            "prompt_tokens": parse_state.prompt_tokens.max(state.model_registry.estimate_tokens(&final_prompt, &model)),
            "completion_tokens": parse_state.completion_tokens.max(state.model_registry.estimate_tokens(&format!("{}{}", parse_state.reasoning, parse_state.last_full_content), &model)),
            "total_tokens": parse_state.prompt_tokens.max(state.model_registry.estimate_tokens(&final_prompt, &model)) + parse_state.completion_tokens.max(state.model_registry.estimate_tokens(&format!("{}{}", parse_state.reasoning, parse_state.last_full_content), &model)),
            "prompt_tokens_details": { "cached_tokens": 0 }
        });

        let mut final_chunk = json!({
            "id": completion_id,
            "object": "chat.completion.chunk",
            "created": current_timestamp(),
            "model": model,
            "choices": [{ "index": 0, "delta": {}, "logprobs": Value::Null, "finish_reason": if tool_index == 0 { "stop" } else { "tool_calls" } }],
        });
        if !include_usage {
            final_chunk["usage"] = usage.clone();
        }
        yield Ok(sse_json(final_chunk));

        if include_usage {
            yield Ok(sse_json(json!({
                "id": completion_id,
                "object": "chat.completion.chunk",
                "created": current_timestamp(),
                "model": model,
                "choices": [],
                "usage": usage,
            })));
        }
        yield Ok(sse_done());

        if let Some(parent_id) = parse_state.target_response_id.clone() {
            conversations
                .update_parent(&session_key, &chat_id, parent_id)
                .await;
        }
        stream_registry.remove_by_completion_id(&completion_id).await;
        metrics.gauge("streams.active", stream_registry.active_count().await as f64).await;
    };

    stream_response(stream)
}

async fn create_qwen_chat(
    client: &reqwest::Client,
    config: &AppConfig,
    headers: &HashMap<String, String>,
) -> Result<String> {
    let response = client
        .post(format!("{}/api/v2/chats/new", config.qwen_base_url))
        .header("accept", "application/json, text/plain, */*")
        .header("accept-language", "pt-BR,pt;q=0.9")
        .header("content-type", "application/json")
        .header("cookie", headers.get("cookie").cloned().unwrap_or_default())
        .header("origin", &config.qwen_base_url)
        .header("referer", format!("{}/c/new-chat", config.qwen_base_url))
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
        .json(&json!({
            "title": "Nova Conversa",
            "models": ["qwen3.7-plus"],
            "chat_mode": "normal",
            "chat_type": "t2t",
            "timestamp": current_timestamp(),
            "project_id": ""
        }))
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "Qwen create chat error: {} {}",
            response.status(),
            response.text().await.unwrap_or_default()
        ));
    }

    let body: Value = response.json().await?;
    if let Some(message) = extract_qwen_api_error(&body) {
        return Err(anyhow!("Qwen create chat error: {message}"));
    }
    extract_qwen_chat_id(&body)
        .map(str::to_owned)
        .ok_or_else(|| {
            anyhow!(
                "Qwen chat id missing in response: {}",
                truncate_error_payload(&body.to_string(), 400)
            )
        })
}

async fn request_qwen_chat(
    state: &AppState,
    body: &OpenAIRequest,
    final_prompt: &str,
    chat_id: &str,
    parent_id: Option<&str>,
    headers: &HashMap<String, String>,
    files: &[Value],
) -> std::result::Result<reqwest::Response, QwenRequestError> {
    let model = normalize_model_id(&body.model);
    let parent_id = parent_id.map(Value::from).unwrap_or(Value::Null);
    let payload = json!({
        "stream": true,
        "version": "2.1",
        "incremental_output": true,
        "chat_id": chat_id,
        "chat_mode": "normal",
        "model": model,
        "parent_id": parent_id,
        "messages": [{
            "fid": Uuid::new_v4().to_string(),
            "parentId": parent_id,
            "childrenIds": [],
            "role": "user",
            "content": final_prompt,
            "user_action": "chat",
            "files": files,
            "timestamp": current_timestamp(),
            "models": [model],
            "chat_type": "t2t",
            "feature_config": {
                "thinking_enabled": !body.model.contains("no-thinking"),
                "output_schema": "phase",
                "research_mode": "normal",
                "auto_thinking": false,
                "thinking_mode": "Thinking",
                "thinking_format": "summary",
                "auto_search": body.web_search.unwrap_or(false)
            },
            "extra": { "meta": { "subChatType": "t2t" } },
            "sub_chat_type": "t2t",
            "parent_id": parent_id
        }],
        "timestamp": current_timestamp() + 1
    });

    let payload_json = payload.to_string();
    if payload_json.len() > MAX_PAYLOAD_SIZE {
        return Err(QwenRequestError {
            message: format!(
                "payload too large: {} bytes exceeds limit of {} bytes",
                payload_json.len(),
                MAX_PAYLOAD_SIZE
            ),
            upstream_code: None,
            upstream_status: Some(413),
            retry_after_ms: None,
        });
    }

    let response = state
        .client
        .post(format!(
            "{}/api/v2/chat/completions?chat_id={chat_id}",
            state.config.qwen_base_url
        ))
        .header("accept", "application/json")
        .header("accept-language", "pt-BR,pt;q=0.9")
        .header("content-type", "application/json")
        .header("cookie", headers.get("cookie").cloned().unwrap_or_default())
        .header("origin", &state.config.qwen_base_url)
        .header(
            "referer",
            format!("{}/c/{chat_id}", state.config.qwen_base_url),
        )
        .header("sec-fetch-dest", "empty")
        .header("sec-fetch-mode", "cors")
        .header("sec-fetch-site", "same-origin")
        .header("timezone", "UTC")
        .header(
            "user-agent",
            headers.get("user-agent").cloned().unwrap_or_default(),
        )
        .header("x-accel-buffering", "no")
        .header("x-request-id", Uuid::new_v4().to_string())
        .header("bx-v", headers.get("bx-v").cloned().unwrap_or_default())
        .header("bx-ua", headers.get("bx-ua").cloned().unwrap_or_default())
        .header(
            "bx-umidtoken",
            headers.get("bx-umidtoken").cloned().unwrap_or_default(),
        )
        .header("version", QWEN_WEB_VERSION)
        .header("source", "web")
        .body(payload_json)
        .send()
        .await
        .map_err(|err| QwenRequestError {
            message: err.to_string(),
            upstream_code: None,
            upstream_status: Some(502),
            retry_after_ms: None,
        })?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();

        if let Ok(json) = serde_json::from_str::<Value>(&text) {
            if json.get("success").and_then(Value::as_bool) == Some(false) {
                let code = json
                    .get("data")
                    .and_then(Value::as_object)
                    .and_then(|data| data.get("code"))
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                let details = json
                    .get("data")
                    .and_then(Value::as_object)
                    .and_then(|data| data.get("details"))
                    .and_then(Value::as_str)
                    .or_else(|| json.get("message").and_then(Value::as_str))
                    .unwrap_or("Qwen returned an error");
                let retry_after_ms = if details.contains("chat is in progress") {
                    Some(2_500)
                } else {
                    None
                };
                return Err(QwenRequestError {
                    message: format!(
                        "Qwen upstream error: {}: {}",
                        code.clone().unwrap_or_else(|| "UpstreamError".to_owned()),
                        details
                    ),
                    upstream_code: code,
                    upstream_status: Some(if status == 0 { 502 } else { status }),
                    retry_after_ms,
                });
            }
        }

        return Err(QwenRequestError {
            message: format!("Failed to fetch from Qwen: {status} {text}"),
            upstream_code: None,
            upstream_status: Some(status.max(502)),
            retry_after_ms: None,
        });
    }

    Ok(response)
}

async fn stop_upstream_generation(
    state: &AppState,
    target: &stream_registry::ActiveStreamHandle,
    response_id: &str,
) -> Result<()> {
    let response = state
        .client
        .post(format!(
            "{}/api/v2/chat/completions/stop?chat_id={}",
            state.config.qwen_base_url, target.snapshot.chat_id
        ))
        .header("accept", "application/json, text/plain, */*")
        .header("accept-language", "pt-BR,pt;q=0.9")
        .header("content-type", "application/json")
        .header(
            "cookie",
            target.headers.get("cookie").cloned().unwrap_or_default(),
        )
        .header("origin", &state.config.qwen_base_url)
        .header(
            "referer",
            format!(
                "{}/c/{}",
                state.config.qwen_base_url, target.snapshot.chat_id
            ),
        )
        .header(
            "user-agent",
            target
                .headers
                .get("user-agent")
                .cloned()
                .unwrap_or_default(),
        )
        .header("x-request-id", Uuid::new_v4().to_string())
        .header(
            "bx-ua",
            target.headers.get("bx-ua").cloned().unwrap_or_default(),
        )
        .header(
            "bx-umidtoken",
            target
                .headers
                .get("bx-umidtoken")
                .cloned()
                .unwrap_or_default(),
        )
        .header(
            "bx-v",
            target.headers.get("bx-v").cloned().unwrap_or_default(),
        )
        .json(&json!({
            "chat_id": target.snapshot.chat_id,
            "response_id": response_id,
        }))
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "stop generation failed: {} {}",
            response.status(),
            response.text().await.unwrap_or_default()
        ));
    }

    Ok(())
}

async fn collect_qwen_events(
    data: &str,
    completion_id: &str,
    stream_registry: &StreamRegistry,
    parse_state: &mut QwenParseState,
    tool_parser: &mut Option<StreamingToolParser>,
) -> Result<Vec<QwenEvent>> {
    if data == "[DONE]" {
        return Ok(Vec::new());
    }

    let chunk: Value = match serde_json::from_str(data) {
        Ok(value) => value,
        Err(_) => return Ok(Vec::new()),
    };
    if let Some(error) = extract_qwen_api_error(&chunk) {
        return Err(anyhow!("Qwen upstream stream error: {error}"));
    }

    if let Some(response_id) = chunk
        .get("response.created")
        .and_then(Value::as_object)
        .and_then(|created| created.get("response_id"))
        .and_then(Value::as_str)
        .or_else(|| chunk.get("response_id").and_then(Value::as_str))
    {
        parse_state
            .target_response_id
            .get_or_insert_with(|| response_id.to_owned());
        stream_registry
            .update_response_id(completion_id, response_id.to_owned())
            .await;
    }

    if let Some(usage) = chunk.get("usage").and_then(Value::as_object) {
        if let Some(input) = usage.get("input_tokens").and_then(Value::as_u64) {
            parse_state.prompt_tokens = input as usize;
        }
        if let Some(output) = usage.get("output_tokens").and_then(Value::as_u64) {
            parse_state.completion_tokens = output as usize;
        }
    }

    let delta = chunk
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(Value::as_object)
        .and_then(|choice| choice.get("delta"))
        .and_then(Value::as_object);

    let Some(delta) = delta else {
        return Ok(Vec::new());
    };

    if parse_state.target_response_id.is_some()
        && chunk.get("response_id").and_then(Value::as_str)
            != parse_state.target_response_id.as_deref()
        && chunk.get("response.created").is_none()
    {
        return Ok(Vec::new());
    }

    let phase = delta
        .get("phase")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if phase == "thinking_summary" {
        if let Some(content) = delta
            .get("extra")
            .and_then(Value::as_object)
            .and_then(|extra| extra.get("summary_thought"))
            .and_then(Value::as_object)
            .and_then(|summary| summary.get("content"))
            .and_then(Value::as_array)
        {
            if content.len() > parse_state.current_thought_index {
                let append = content[parse_state.current_thought_index..]
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join("\n");
                parse_state.current_thought_index = content.len();
                if !append.is_empty() {
                    parse_state.reasoning.push_str(&append);
                    return Ok(vec![QwenEvent::Reasoning(append)]);
                }
            }
        }
        return Ok(Vec::new());
    }

    let Some(content) = extract_qwen_delta_text(delta.get("content")) else {
        return Ok(Vec::new());
    };

    let incremental = if content.starts_with(&parse_state.last_full_content) {
        content[parse_state.last_full_content.len()..].to_owned()
    } else {
        content.to_owned()
    };
    parse_state.last_full_content = content.to_owned();

    if incremental.is_empty() {
        return Ok(Vec::new());
    }

    if let Some(parser) = tool_parser {
        let parsed = parser.feed(&incremental);
        let mut events = Vec::new();
        if !parsed.text.is_empty() {
            events.push(QwenEvent::Text(parsed.text));
        }
        for parsed_tool in parsed.tool_calls {
            events.push(QwenEvent::ToolCall(tool_call_from_parsed(parsed_tool)));
        }
        return Ok(events);
    }

    Ok(vec![QwenEvent::Text(incremental)])
}

fn extract_qwen_delta_text(value: Option<&Value>) -> Option<String> {
    extract_qwen_delta_text_depth(value, 0)
}

fn extract_qwen_delta_text_depth(value: Option<&Value>, depth: usize) -> Option<String> {
    // ponytail: cap recursion at 64 to block stack-overflow via pathological upstream JSON.
    const MAX_DEPTH: usize = 64;
    let value = value?;
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => {
            let text = items
                .iter()
                .filter_map(|item| extract_qwen_delta_text_depth(Some(item), depth + 1))
                .collect::<Vec<_>>()
                .join("");
            (!text.is_empty()).then_some(text)
        }
        Value::Object(map) => {
            if depth >= MAX_DEPTH {
                return None;
            }
            if let Some(text) = map.get("text").and_then(Value::as_str) {
                return Some(text.to_owned());
            }
            if let Some(text) = map.get("content").and_then(Value::as_str) {
                return Some(text.to_owned());
            }
            if let Some(text) = map.get("value").and_then(Value::as_str) {
                return Some(text.to_owned());
            }
            if let Some(text) = map
                .get("answer")
                .and_then(|answer| extract_qwen_delta_text_depth(Some(answer), depth + 1))
            {
                return Some(text);
            }
            if let Some(text) = map
                .get("parts")
                .and_then(|parts| extract_qwen_delta_text_depth(Some(parts), depth + 1))
            {
                return Some(text);
            }
            None
        }
        _ => None,
    }
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

fn effective_accounts(store: &AccountStore) -> Result<Vec<QwenAccount>> {
    let accounts = store.list_accounts()?;
    if accounts.is_empty() {
        Ok(vec![global_account()])
    } else {
        Ok(accounts)
    }
}

fn account_id_for_bridge(account: &QwenAccount) -> Option<String> {
    (account.id != "global").then(|| account.id.clone())
}

fn qwen_session_key(user: Option<&str>) -> String {
    user.filter(|value| !value.trim().is_empty())
        .map(str::trim)
        .unwrap_or("default")
        .to_owned()
}

async fn pick_capture_headers_for_aux_request(
    state: &AppState,
) -> Result<(QwenAccount, HashMap<String, String>)> {
    let accounts = effective_accounts(&state.accounts)?;
    let account = state
        .account_manager
        .select_next(&accounts, false)
        .await
        .unwrap_or_else(global_account);
    let headers = state
        .bridge
        .capture_headers(CaptureHeadersParams {
            force_new: false,
            account_id: account_id_for_bridge(&account),
        })
        .await?
        .headers;
    Ok((account, headers))
}

fn extract_qwen_chat_id(body: &Value) -> Option<&str> {
    body.get("chat_id")
        .and_then(Value::as_str)
        .or_else(|| body.get("id").and_then(Value::as_str))
        .or_else(|| body.pointer("/data/chat_id").and_then(Value::as_str))
        .or_else(|| body.pointer("/data/id").and_then(Value::as_str))
        .or_else(|| body.pointer("/chat/chat_id").and_then(Value::as_str))
        .or_else(|| body.pointer("/chat/id").and_then(Value::as_str))
        .or_else(|| body.pointer("/data/chat/chat_id").and_then(Value::as_str))
        .or_else(|| body.pointer("/data/chat/id").and_then(Value::as_str))
}

fn extract_qwen_api_error(body: &Value) -> Option<String> {
    let success = body.get("success").and_then(Value::as_bool);
    let code = body
        .pointer("/data/code")
        .or_else(|| body.get("code"))
        .and_then(Value::as_str);
    let details = body
        .pointer("/data/details")
        .or_else(|| body.get("message"))
        .and_then(Value::as_str);

    if success == Some(false) || code.is_some() || details.is_some() {
        Some(match (code, details) {
            (Some(code), Some(details)) => format!("{code}: {details}"),
            (Some(code), None) => code.to_owned(),
            (None, Some(details)) => details.to_owned(),
            (None, None) => "unknown upstream error".to_owned(),
        })
    } else {
        None
    }
}

fn truncate_error_payload(text: &str, max_len: usize) -> String {
    crate::proxy_core::truncate_error_payload(text, max_len)
}

fn normalize_request(request: &OpenAIRequest) -> (OpenAIRequest, Vec<MediaUploadInput>) {
    let mut normalized = request.clone();
    let mut uploads = Vec::new();

    for message in &mut normalized.messages {
        let Some(Value::Array(items)) = message.content.clone() else {
            continue;
        };

        let mut text_parts = Vec::new();
        for item in items {
            if let Some(text) = item.as_str() {
                text_parts.push(text.to_owned());
                continue;
            }
            let Some(object) = item.as_object() else {
                continue;
            };
            match object.get("type").and_then(Value::as_str) {
                Some("text") => {
                    if let Some(text) = object.get("text").and_then(Value::as_str) {
                        text_parts.push(text.to_owned());
                    }
                }
                Some("image_url") => collect_upload(&mut uploads, "image_url", object, "image_url"),
                Some("video_url") => collect_upload(&mut uploads, "video_url", object, "video_url"),
                Some("audio_url") => collect_upload(&mut uploads, "audio_url", object, "audio_url"),
                Some("file_url") => collect_upload(&mut uploads, "file_url", object, "file_url"),
                _ => {}
            }
        }

        message.content = Some(Value::String(text_parts.join("\n")));
    }

    (normalized, uploads)
}

fn collect_upload(
    uploads: &mut Vec<MediaUploadInput>,
    kind: &str,
    object: &serde_json::Map<String, Value>,
    field: &str,
) {
    if let Some(url) = object
        .get(field)
        .and_then(Value::as_object)
        .and_then(|value| value.get("url"))
        .and_then(Value::as_str)
    {
        uploads.push(MediaUploadInput {
            kind: kind.to_owned(),
            url: url.to_owned(),
        });
    }
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

/// Map an upstream/internal error to a generic 502 with an opaque id; log the real
/// cause server-side so upstream bodies / header fragments never reach the client.
fn bad_gateway_error(err: impl std::fmt::Display) -> Response {
    let id = Uuid::new_v4();
    eprintln!("[qwen] upstream error {id}: {err}");
    json_error(
        StatusCode::BAD_GATEWAY,
        format!("upstream provider error (id={id})"),
    )
}

fn stream_response<S>(stream: S) -> Response
where
    S: futures_util::Stream<Item = Result<Bytes, std::convert::Infallible>> + Send + 'static,
{
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache, no-transform")
        .header(header::CONNECTION, "keep-alive")
        .header("x-accel-buffering", "no")
        .body(Body::from_stream(stream))
        .expect("valid streaming response")
}

pub async fn serve_embedded(
    config: AppConfig,
    helper_dir: std::path::PathBuf,
    node_path: Option<std::path::PathBuf>,
) -> Result<()> {
    ensure_runtime_layout(&config)?;

    let workspace_root = workspace_root();
    let accounts = AccountStore::new(
        config.db_path.clone(),
        &legacy_db_candidates(&workspace_root),
        &legacy_accounts_json_candidates(&workspace_root),
    )?;
    let metrics = Metrics::new().await;
    let cache = MemoryCache::new(config.cache.default_ttl, 10_000, metrics.clone());
    let model_registry = ModelRegistry::new().await;
    let stream_registry = StreamRegistry::new();
    let watchdog = Watchdog::start(
        config.watchdog.clone(),
        metrics.clone(),
        stream_registry.clone(),
        cache.clone(),
        config.chat_timeout,
    );

    let bridge = Arc::new(PlaywrightBridge::new_with_node(&helper_dir, node_path, "qwen").await?);

    run_server(
        bridge,
        config,
        ServerRuntime {
            accounts,
            metrics,
            cache,
            model_registry,
            stream_registry,
            watchdog,
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::{
        collect_qwen_events, extract_qwen_api_error, extract_qwen_chat_id, live_qwen_model_data,
        qwen_session_key, QwenEvent, QwenParseState, StreamRegistry,
    };
    use serde_json::json;

    #[test]
    fn session_key_defaults_and_trims_client_identity() {
        assert_eq!(qwen_session_key(None), "default");
        assert_eq!(qwen_session_key(Some("  ")), "default");
        assert_eq!(qwen_session_key(Some("  client-1 ")), "client-1");
    }

    #[test]
    fn extracts_nested_chat_id_variants() {
        assert_eq!(
            extract_qwen_chat_id(&json!({ "data": { "id": "chat-123" } })),
            Some("chat-123")
        );
        assert_eq!(
            extract_qwen_chat_id(&json!({ "data": { "chat_id": "chat-456" } })),
            Some("chat-456")
        );
        assert_eq!(
            extract_qwen_chat_id(&json!({ "id": "chat-789" })),
            Some("chat-789")
        );
        assert_eq!(
            extract_qwen_chat_id(&json!({ "data": { "chat": { "id": "chat-987" } } })),
            Some("chat-987")
        );
    }

    #[test]
    fn extracts_qwen_api_error_message() {
        assert_eq!(
            extract_qwen_api_error(&json!({
                "success": false,
                "data": {
                    "code": "Unauthorized",
                    "details": "login required"
                }
            })),
            Some("Unauthorized: login required".to_owned())
        );
    }

    #[test]
    fn expands_live_qwen_catalog_with_thinking_variants() {
        let payload = live_qwen_model_data(&json!({
            "data": [{ "id": "qwen-live", "info": { "created_at": 1 } }]
        }))
        .unwrap();
        let ids = payload["data"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|model| model["id"].as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            ["qwen-live", "qwen-live-thinking", "qwen-live-no-thinking"]
        );
        assert_eq!(payload["discovery"]["live"], true);
    }

    #[tokio::test]
    async fn surfaces_qwen_stream_error_payloads() {
        let registry = StreamRegistry::new();
        let mut state = QwenParseState::default();
        let mut parser = None;
        let result = collect_qwen_events(
            &json!({
                "success": false,
                "data": { "code": "Bad_Request", "details": "model unavailable" }
            })
            .to_string(),
            "chatcmpl-test",
            &registry,
            &mut state,
            &mut parser,
        )
        .await;
        assert!(result.is_err());
        let error = result.err().unwrap();
        assert!(error.to_string().contains("Bad_Request: model unavailable"));
    }

    #[tokio::test]
    async fn parses_answer_content_without_phase() {
        let registry = StreamRegistry::new();
        let mut state = QwenParseState::default();
        let mut parser = None;
        let data = json!({
            "response_id": "resp-1",
            "choices": [{
                "delta": {
                    "content": "visible answer"
                }
            }]
        })
        .to_string();

        let events =
            collect_qwen_events(&data, "chatcmpl-test", &registry, &mut state, &mut parser)
                .await
                .unwrap();

        assert!(matches!(
            events.as_slice(),
            [QwenEvent::Text(text)] if text == "visible answer"
        ));
        assert_eq!(state.last_full_content, "visible answer");
    }

    #[tokio::test]
    async fn maps_thinking_summary_to_reasoning() {
        let registry = StreamRegistry::new();
        let mut state = QwenParseState::default();
        let mut parser = None;
        let data = json!({
            "response_id": "resp-1",
            "choices": [{
                "delta": {
                    "phase": "thinking_summary",
                    "extra": {
                        "summary_thought": {
                            "content": ["first thought"]
                        }
                    }
                }
            }]
        })
        .to_string();

        let events =
            collect_qwen_events(&data, "chatcmpl-test", &registry, &mut state, &mut parser)
                .await
                .unwrap();

        assert!(matches!(
            events.as_slice(),
            [QwenEvent::Reasoning(text)] if text == "first thought"
        ));
        assert_eq!(state.reasoning, "first thought");
        assert!(state.last_full_content.is_empty());
    }

    #[tokio::test]
    async fn parses_answer_content_array_shape() {
        let registry = StreamRegistry::new();
        let mut state = QwenParseState::default();
        let mut parser = None;
        let data = json!({
            "response_id": "resp-1",
            "choices": [{
                "delta": {
                    "content": [
                        { "type": "text", "text": "visible " },
                        { "type": "text", "content": "answer" }
                    ]
                }
            }]
        })
        .to_string();

        let events =
            collect_qwen_events(&data, "chatcmpl-test", &registry, &mut state, &mut parser)
                .await
                .unwrap();

        assert!(matches!(
            events.as_slice(),
            [QwenEvent::Text(text)] if text == "visible answer"
        ));
        assert_eq!(state.last_full_content, "visible answer");
    }

    #[tokio::test]
    async fn parses_nested_answer_content_object() {
        let registry = StreamRegistry::new();
        let mut state = QwenParseState::default();
        let mut parser = None;
        let data = json!({
            "response_id": "resp-1",
            "choices": [{
                "delta": {
                    "content": {
                        "answer": {
                            "parts": [
                                { "text": "nested " },
                                { "value": "answer" }
                            ]
                        }
                    }
                }
            }]
        })
        .to_string();

        let events =
            collect_qwen_events(&data, "chatcmpl-test", &registry, &mut state, &mut parser)
                .await
                .unwrap();

        assert!(matches!(
            events.as_slice(),
            [QwenEvent::Text(text)] if text == "nested answer"
        ));
        assert_eq!(state.last_full_content, "nested answer");
    }
}
