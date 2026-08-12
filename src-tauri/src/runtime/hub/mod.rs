use crate::proxy_core::{constant_time_eq, OpenAIRequest};
use anyhow::{anyhow, Result};
use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Path, Query, State},
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

#[derive(Debug, Default, Deserialize)]
struct ModelListQuery {
    #[serde(default)]
    chatgpt_mode: Option<String>,
}

include!("hub_impl_a.rs");
include!("hub_impl_b.rs");
include!("hub_openapi.rs");
#[cfg(test)]
mod tests;
