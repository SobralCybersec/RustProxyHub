mod account_manager;
pub mod accounts;
mod cache;
pub mod config;
mod conversation_registry;
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
    collections::{HashMap, HashSet, VecDeque},
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};
use uuid::Uuid;

const QWEN_WEB_VERSION: &str = "0.2.66";

use self::{
    account_manager::{AccountLease, AccountManager},
    accounts::{global_account, AccountStore, QwenAccount},
    cache::MemoryCache,
    config::{
        ensure_runtime_layout, legacy_accounts_json_candidates, legacy_db_candidates,
        workspace_root, AppConfig,
    },
    conversation_registry::{ConversationRegistry, QwenConversation},
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
    traces: QwenTraceStore,
    watchdog: Watchdog,
}

#[derive(Clone, Default)]
struct QwenTraceStore {
    entries: Arc<tokio::sync::Mutex<VecDeque<Value>>>,
}

impl QwenTraceStore {
    async fn record(&self, completion_id: &str, kind: &str, payload: String) {
        const LIMIT: usize = 64;
        let mut entries = self.entries.lock().await;
        entries.push_back(json!({
            "completion_id": completion_id,
            "kind": kind,
            "payload": payload,
            "timestamp": current_timestamp(),
        }));
        while entries.len() > LIMIT {
            entries.pop_front();
        }
    }

    async fn snapshot(&self) -> Vec<Value> {
        self.entries.lock().await.iter().cloned().collect()
    }
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
    retryable: bool,
}

impl std::fmt::Display for QwenRequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for QwenRequestError {}

fn is_retryable_transport_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    [
        "connection reset",
        "connection closed",
        "broken pipe",
        "connection aborted",
        "econnreset",
        "eof",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

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
    conversations: ConversationRegistry,
    watchdog: Watchdog,
}

include!("qwen_impl_a.rs");
include!("qwen_impl_b.rs");
include!("qwen_impl_c.rs");
#[cfg(test)]
mod tests;
