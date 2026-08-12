use crate::browser_bridge::{
    BrowserBridge, CaptureHeadersParams, InitParams, ManualLoginParams, PlaywrightBridge,
};
use crate::proxy_core::{
    build_prompt, constant_time_eq, current_timestamp, sse_done, sse_json, usage_from_text,
    MessageToolCall, OpenAIRequest, StreamingToolParser, ToolCallFunction,
};
use crate::runtime::session_store::SessionStore;
use anyhow::{anyhow, Result};
use async_stream::stream;
use axum::{
    body::Body,
    extract::{DefaultBodyLimit, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use bytes::Bytes;
use futures_util::StreamExt;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::Arc,
};
use uuid::Uuid;

/* static literal patterns: an invalid regex here is a build-time typo, not a runtime condition */
static PAUSE_MESSAGE_RES: Lazy<[Regex; 3]> = Lazy::new(|| {
    [
        Regex::new(r#"(?is)This task paused because Kimi reached.*?resume the task\."#)
            .expect("valid static regex"),
        Regex::new(r#"(?is)Esta tarefa foi pausada porque.*?retomar a tarefa\."#)
            .expect("valid static regex"),
        Regex::new(r#"(?is)This task paused because Kimi reached.*?resume\."#)
            .expect("valid static regex"),
    ]
});
static PAUSE_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        Regex::new("(?i)maximum number of tool calls").expect("valid static regex"),
        Regex::new("(?i)reached the maximum number of tool").expect("valid static regex"),
        Regex::new("(?i)type ['\"“”]?continue['\"“”]? to resume").expect("valid static regex"),
        Regex::new("(?i)número máximo de chamadas de ferramenta").expect("valid static regex"),
        Regex::new("(?i)digite ['\"“”]?continue['\"“”]? para retomar").expect("valid static regex"),
        Regex::new("(?i)limite máximo de chamadas").expect("valid static regex"),
    ]
});

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
    session_parents: SessionStore,
}

#[derive(Default)]
struct ConnectParser {
    buffer: Vec<u8>,
}

impl ConnectParser {
    fn feed(&mut self, chunk: &[u8]) -> Vec<Value> {
        self.buffer.extend_from_slice(chunk);
        let mut messages = Vec::new();
        let mut offset = 0;

        loop {
            if self.buffer.len() - offset < 5 {
                break;
            }
            let flags = self.buffer[offset];
            let length = ((self.buffer[offset + 1] as usize) << 24)
                | ((self.buffer[offset + 2] as usize) << 16)
                | ((self.buffer[offset + 3] as usize) << 8)
                | (self.buffer[offset + 4] as usize);
            if self.buffer.len() - offset < 5 + length {
                break;
            }

            let payload = self.buffer[offset + 5..offset + 5 + length].to_vec();
            offset += 5 + length;

            if flags == 0x00 {
                if let Ok(text) = String::from_utf8(payload) {
                    if let Ok(value) = serde_json::from_str::<Value>(&text) {
                        messages.push(value);
                    }
                }
            }
        }

        if offset > 0 {
            self.buffer.drain(..offset);
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

include!("kimi_impl.rs");
#[cfg(test)]
mod tests;
