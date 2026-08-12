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

include!("browser_runtime_impl_a.rs");
include!("browser_runtime_impl_b.rs");
include!("browser_runtime_impl_c.rs");
#[cfg(test)]
#[path = "browser_runtime/tests.rs"]
mod tests;
