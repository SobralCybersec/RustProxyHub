use crate::browser_bridge::{
    BrowserBridge, CaptureHeadersParams, InitParams, ManualLoginParams, PlaywrightBridge,
};
use crate::ids::{ParentMessageId, SessionId};
use crate::proxy_core::{
    build_prompt, constant_time_eq, current_timestamp, extract_tool_calls_from_text, sse_done,
    sse_json, usage_from_text, MessageToolCall, OpenAIRequest, StreamingToolParser,
    ToolCallFunction,
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
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use uuid::Uuid;

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
    session_parents: SessionStore,
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

fn deepseek_model_type(is_pro: bool, is_vision: bool) -> &'static str {
    // Vision mode is chat-only and mutually exclusive with expert/default on
    // DeepSeek web, so it wins the model_type slot when the id asks for it.
    if is_vision {
        "vision"
    } else if is_pro {
        "expert"
    } else {
        "default"
    }
}

fn deepseek_mode_flags(model_id: &str) -> (bool, bool, bool) {
    let lower = model_id.trim().to_ascii_lowercase();
    let is_vision = lower.contains("vision");
    // Vision runs neither the expert nor the thinking pipeline on the web app.
    let is_pro = !is_vision && (lower.contains("expert") || lower.contains("pro"));
    let is_thinking = !is_vision && (lower.contains("thinking") || lower.contains("deepthink"));
    (is_pro, is_thinking, is_vision)
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

fn sync_deepseek_events(
    payload: &mut serde_json::Map<String, Value>,
    is_pro: bool,
    is_thinking: bool,
    is_vision: bool,
) {
    let sync_events = |events: &mut Vec<Value>| {
        upsert_deepseek_event(
            events,
            "switchModelType",
            Value::String(deepseek_model_type(is_pro, is_vision).to_owned()),
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
            if let Some(group_events) = group_object.get_mut("events").and_then(Value::as_array_mut)
            {
                sync_events(group_events);
                return;
            }
        }
    }

    let mut events = Vec::new();
    sync_events(&mut events);
    payload.insert("events".to_owned(), Value::Array(events));
}

/* Builds the DeepSeek chat payload. Mode flags read as named toggles at the call
site (.pro/.thinking/.vision/.search) instead of a row of positional bools the
compiler and the reader can't tell apart. */
struct DeepseekPayload<'a> {
    template: Option<Value>,
    prompt: &'a str,
    session_id: &'a SessionId,
    parent: Option<ParentMessageId>,
    is_pro: bool,
    is_thinking: bool,
    is_vision: bool,
    search: bool,
}

impl<'a> DeepseekPayload<'a> {
    fn new(prompt: &'a str, session_id: &'a SessionId) -> Self {
        Self {
            template: None,
            prompt,
            session_id,
            parent: None,
            is_pro: false,
            is_thinking: false,
            is_vision: false,
            search: false,
        }
    }

    fn template(mut self, template: Option<Value>) -> Self {
        self.template = template;
        self
    }
    fn parent(mut self, parent: Option<ParentMessageId>) -> Self {
        self.parent = parent;
        self
    }
    fn pro(mut self, yes: bool) -> Self {
        self.is_pro = yes;
        self
    }
    fn thinking(mut self, yes: bool) -> Self {
        self.is_thinking = yes;
        self
    }
    fn vision(mut self, yes: bool) -> Self {
        self.is_vision = yes;
        self
    }
    fn search(mut self, yes: bool) -> Self {
        self.search = yes;
        self
    }

    fn build(self) -> Value {
        let mut payload = self
            .template
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default();

        if !self.session_id.is_empty() {
            payload.insert(
                "chat_session_id".to_owned(),
                Value::String(self.session_id.to_string()),
            );
        }
        payload.insert(
            "parent_message_id".to_owned(),
            self.parent
                .map(|parent| Value::from(parent.get()))
                .unwrap_or(Value::Null),
        );
        payload.insert(
            "model_type".to_owned(),
            Value::String(deepseek_model_type(self.is_pro, self.is_vision).to_owned()),
        );
        payload.insert("prompt".to_owned(), Value::String(self.prompt.to_owned()));
        payload.insert("ref_file_ids".to_owned(), Value::Array(Vec::new()));
        payload.insert("thinking_enabled".to_owned(), Value::Bool(self.is_thinking));
        payload.insert("search_enabled".to_owned(), Value::Bool(self.search));
        payload.insert("preempt".to_owned(), Value::Bool(false));
        sync_deepseek_events(&mut payload, self.is_pro, self.is_thinking, self.is_vision);

        Value::Object(payload)
    }
}

#[derive(Debug, Deserialize)]
struct ManualLoginRequest {
    #[serde(default)]
    browser: Option<String>,
}

include!("deepseek_impl.rs");
#[cfg(test)]
mod tests;
