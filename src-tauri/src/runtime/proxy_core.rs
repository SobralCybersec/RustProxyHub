use anyhow::{anyhow, Result};
use bytes::Bytes;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use url::Url;
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FunctionToolSpec {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FunctionToolDefinition {
    #[serde(rename = "type")]
    pub tool_type: String,
    /* function tools nest name/params here; custom (freeform, type:"custom") tools omit it */
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function: Option<FunctionToolSpec>,
    /* custom-tool name/description live at the top level, not under `function` */
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl FunctionToolDefinition {
    /* the callable name whether this is a function or a custom tool */
    pub fn tool_name(&self) -> Option<&str> {
        self.function
            .as_ref()
            .map(|spec| spec.name.as_str())
            .or(self.name.as_deref())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MessageToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: ToolCallFunction,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Message {
    pub role: String,
    #[serde(default)]
    pub content: Option<Value>,
    #[serde(default)]
    pub tool_calls: Option<Vec<MessageToolCall>>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub reasoning_content: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct StreamOptions {
    #[serde(default)]
    pub include_usage: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OpenAIRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub stream: Option<bool>,
    #[serde(default)]
    pub web_search: Option<bool>,
    #[serde(default)]
    pub chatgpt_mode: Option<String>,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub tools: Option<Vec<FunctionToolDefinition>>,
    #[serde(default)]
    pub tool_choice: Option<Value>,
    #[serde(default)]
    pub stream_options: Option<StreamOptions>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ToolCallChunk {
    pub index: usize,
    pub id: String,
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: ToolCallFunction,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Usage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_tokens_details: Option<Value>,
}

#[derive(Clone, Debug)]
pub struct ParsedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Clone, Debug, Default)]
pub struct PromptPreflightOptions {
    pub max_prompt_tokens: Option<usize>,
    pub extra_system_instructions: Option<String>,
    pub dedup_system_blocks: bool,
    pub structured_compaction_max_chars: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct PromptPreflight {
    pub request: OpenAIRequest,
    pub system_prompt: String,
    pub conversation: String,
    pub flat_prompt: String,
    pub prompt_tokens: usize,
    pub truncated: bool,
    pub dropped_messages: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructuredPromptCompactionOptions {
    pub max_chars: usize,
    pub preserve_first_block: bool,
}

impl Default for StructuredPromptCompactionOptions {
    fn default() -> Self {
        Self {
            max_chars: 18_000,
            preserve_first_block: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructuredPromptCompaction {
    pub text: String,
    pub truncated: bool,
    pub mode: &'static str,
    pub original_chars: usize,
    pub compacted_chars: usize,
    pub removed_duplicate_blocks: usize,
    pub omitted_blocks: usize,
}

pub fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/* One place owns SSE frame shape for every provider: a JSON payload line and the
terminating [DONE] sentinel, both `data: …\n\n`. */
pub fn sse_json(value: Value) -> Bytes {
    Bytes::from(format!("data: {}\n\n", value))
}

pub fn sse_done() -> Bytes {
    Bytes::from("data: [DONE]\n\n")
}

/// Constant-time string comparison to avoid timing oracles on API keys.
/// Length of inputs leaks; content does not.
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut acc: u8 = 0;
    for (x, y) in a.bytes().zip(b.bytes()) {
        acc |= x ^ y;
    }
    acc == 0
}

/// Validate an account id before it is ever joined to a filesystem path.
/// Allows UUIDs (the format the store generates) and friendly slugs.
pub fn is_safe_account_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

pub fn estimate_tokens(text: &str) -> usize {
    ((text.chars().count() as f64) / 3.5).ceil() as usize
}

pub fn prompt_compaction_enabled() -> bool {
    std::env::var("RUST_PROXY_HUB_PROMPT_COMPACTION")
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "on"
            )
        })
        .unwrap_or(false)
}

const STRUCTURED_OMISSION_MARKER: &str = "[Earlier turns omitted to fit limit]";
const STRUCTURED_BLOCK_TRIM_MARKER: &str = "… [block trimmed] …";
const STRUCTURED_HEAD_TAIL_MARKER: &str = "[Earlier conversation trimmed to fit limit]";
const TOOL_RESPONSE_COMPACTION_MAX_CHARS: usize = 4_000;
const TOOL_RESPONSE_EXCERPT_MARKER: &str = "… [tool response excerpt trimmed] …";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PromptBlockRole {
    User,
    Assistant,
    Tool,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToolResponseContentType {
    Json,
    TestOutput,
    Log,
    Text,
}

fn char_count(text: &str) -> usize {
    text.chars().count()
}

fn take_chars(text: &str, count: usize) -> String {
    text.chars().take(count).collect()
}

fn take_tail_chars(text: &str, count: usize) -> String {
    let chars = text.chars().collect::<Vec<_>>();
    let start = chars.len().saturating_sub(count);
    chars[start..].iter().collect()
}

pub fn compact_prompt(text: &str) -> String {
    let mut compacted = String::with_capacity(text.len());
    let mut blank_line = false;
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            if blank_line {
                continue;
            }
            blank_line = true;
        } else {
            blank_line = false;
        }
        if !compacted.is_empty() {
            compacted.push('\n');
        }
        compacted.push_str(line);
    }
    compacted
}

fn split_prompt_blocks(text: &str) -> Vec<String> {
    compact_prompt(text)
        .split("\n\n")
        .map(str::trim)
        .filter(|block| !block.is_empty())
        .map(str::to_owned)
        .collect()
}

fn block_role(block: &str) -> PromptBlockRole {
    let line = block.trim_start();
    if line.starts_with("User:") {
        PromptBlockRole::User
    } else if line.starts_with("Assistant:") {
        PromptBlockRole::Assistant
    } else if line.starts_with("Tool Response") {
        PromptBlockRole::Tool
    } else {
        PromptBlockRole::Other
    }
}

fn block_is_tool_sensitive(block: &str) -> bool {
    block.contains("<tool_call>")
        || block.contains("</tool_call>")
        || block.trim_start().starts_with("Tool Response")
}

fn compact_long_block(block: &str, max_chars: usize) -> String {
    if char_count(block) <= max_chars {
        return block.to_owned();
    }
    if max_chars <= 48 {
        return take_chars(block, max_chars).trim_end().to_owned();
    }

    let marker_len = char_count(STRUCTURED_BLOCK_TRIM_MARKER) + 2;
    let head_budget = std::cmp::max(24, ((max_chars.saturating_sub(marker_len)) * 35) / 100);
    let tail_budget = std::cmp::max(24, max_chars.saturating_sub(marker_len + head_budget));
    format!(
        "{}\n{}\n{}",
        take_chars(block, head_budget).trim_end(),
        STRUCTURED_BLOCK_TRIM_MARKER,
        take_tail_chars(block, tail_budget).trim_start()
    )
}

fn fallback_head_tail(text: &str, max_chars: usize) -> String {
    if char_count(text) <= max_chars {
        return text.to_owned();
    }
    let marker_len = char_count(STRUCTURED_HEAD_TAIL_MARKER) + 4;
    if max_chars <= marker_len + 64 {
        return take_tail_chars(text, max_chars).trim_start().to_owned();
    }

    let head_budget = std::cmp::min(6_000, ((max_chars.saturating_sub(marker_len)) * 35) / 100);
    let tail_budget = std::cmp::max(1_600, max_chars.saturating_sub(marker_len + head_budget));
    format!(
        "{}\n\n{}\n\n{}",
        take_chars(text, head_budget).trim_end(),
        STRUCTURED_HEAD_TAIL_MARKER,
        take_tail_chars(text, tail_budget).trim_start()
    )
}

fn compact_plain_tool_response(raw: &str, max_chars: usize) -> String {
    let cleaned = raw.replace("\r\n", "\n").replace('\r', "\n");
    if char_count(&cleaned) <= max_chars {
        return cleaned;
    }
    if max_chars <= char_count(TOOL_RESPONSE_EXCERPT_MARKER) + 32 {
        return take_tail_chars(&cleaned, max_chars).trim_start().to_owned();
    }

    let marker_len = char_count(TOOL_RESPONSE_EXCERPT_MARKER) + 2;
    let head_budget = std::cmp::max(32, ((max_chars.saturating_sub(marker_len)) * 35) / 100);
    let tail_budget = std::cmp::max(32, max_chars.saturating_sub(marker_len + head_budget));
    format!(
        "{}\n{}\n{}",
        take_chars(&cleaned, head_budget).trim_end(),
        TOOL_RESPONSE_EXCERPT_MARKER,
        take_tail_chars(&cleaned, tail_budget).trim_start()
    )
}

fn fit_with_header(header: &str, body: &str, max_chars: usize) -> String {
    let combined = if body.trim().is_empty() {
        header.to_owned()
    } else {
        format!("{header}\n{body}")
    };
    if char_count(&combined) <= max_chars {
        return combined;
    }
    if max_chars <= char_count(header) + 1 {
        return take_chars(header, max_chars);
    }
    format!(
        "{}\n{}",
        header,
        compact_plain_tool_response(body, max_chars.saturating_sub(char_count(header) + 1))
    )
}

fn looks_like_media_type(kind: &str) -> bool {
    let lowered = kind.trim().to_ascii_lowercase();
    lowered.contains("image")
        || lowered.contains("audio")
        || lowered.contains("video")
        || lowered.contains("octet-stream")
        || matches!(
            lowered.as_str(),
            "image_url" | "input_image" | "audio_url" | "input_audio" | "file"
        )
}

fn media_payload_summary(value: &Value) -> Option<String> {
    match value {
        Value::String(text)
            if text.trim_start().starts_with("data:image/")
                || text.trim_start().starts_with("data:audio/")
                || text.trim_start().starts_with("data:video/") =>
        {
            Some(format!(
                "inline media payload omitted ({} chars)",
                char_count(text)
            ))
        }
        Value::Object(map) => {
            let kind = map
                .get("mime_type")
                .or_else(|| map.get("media_type"))
                .or_else(|| map.get("content_type"))
                .or_else(|| map.get("type"))
                .and_then(Value::as_str);
            let payload = map
                .get("data")
                .or_else(|| map.get("image"))
                .or_else(|| map.get("image_url"))
                .or_else(|| map.get("url"))
                .or_else(|| map.get("content"))
                .or_else(|| map.get("bytes"));
            let payload_size = payload.map_or_else(
                || char_count(&value.to_string()),
                |payload| char_count(&payload.to_string()),
            );
            if kind.map(looks_like_media_type).unwrap_or(false) {
                return Some(format!(
                    "{} payload omitted ({} chars)",
                    kind.unwrap_or("media"),
                    payload_size
                ));
            }
            let payload_is_data_url = payload
                .and_then(Value::as_str)
                .map(|text| {
                    text.trim_start().starts_with("data:image/")
                        || text.trim_start().starts_with("data:audio/")
                        || text.trim_start().starts_with("data:video/")
                })
                .unwrap_or(false);
            if payload_is_data_url {
                return Some(format!("media payload omitted ({} chars)", payload_size));
            }
            None
        }
        _ => None,
    }
}

fn looks_like_content_parts_array(items: &[Value]) -> bool {
    !items.is_empty()
        && items.iter().all(|item| {
            matches!(item, Value::Object(map) if map.contains_key("type")
                || map.contains_key("text")
                || map.contains_key("image_url")
                || map.contains_key("input_audio")
                || map.contains_key("file"))
        })
}

fn compact_tool_response_part(item: &Value) -> String {
    if let Some(summary) = media_payload_summary(item) {
        return fit_with_header("[Tool Response media payload omitted]", &summary, 320);
    }
    match item {
        Value::Object(map) => map
            .get("text")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| item.to_string()),
        Value::String(text) => text.clone(),
        _ => item.to_string(),
    }
}

fn complete_tool_response_json(content: &Option<Value>, text: &str) -> Option<Value> {
    match content {
        Some(Value::Object(map)) => Some(Value::Object(map.clone())),
        Some(Value::Array(items)) if !looks_like_content_parts_array(items) => {
            Some(Value::Array(items.clone()))
        }
        Some(Value::String(raw)) => {
            let trimmed = raw.trim();
            if trimmed.starts_with('{') || trimmed.starts_with('[') {
                serde_json::from_str::<Value>(trimmed).ok()
            } else {
                None
            }
        }
        _ => {
            let trimmed = text.trim();
            if trimmed.starts_with('{') || trimmed.starts_with('[') {
                serde_json::from_str::<Value>(trimmed).ok()
            } else {
                None
            }
        }
    }
}

fn classify_tool_response_text(content: &Option<Value>, text: &str) -> ToolResponseContentType {
    if complete_tool_response_json(content, text).is_some() {
        return ToolResponseContentType::Json;
    }

    let lines = text.lines().collect::<Vec<_>>();
    let test_hits = lines
        .iter()
        .filter(|line| {
            let line = line.trim();
            line.contains("test result:")
                || line.contains("running ")
                || line.contains("FAILED")
                || line.contains("failures:")
                || line.contains("AssertionError")
                || line.contains("panic")
                || line.contains("error[")
                || line == "Tests"
                || line.starts_with("Test Files")
        })
        .count();
    if test_hits >= 2 {
        return ToolResponseContentType::TestOutput;
    }

    let log_hits = lines
        .iter()
        .filter(|line| {
            let trimmed = line.trim();
            trimmed.contains(" ERROR ")
                || trimmed.contains(" WARN ")
                || trimmed.contains(" INFO ")
                || trimmed.contains(" DEBUG ")
                || trimmed.contains("TRACE")
                || trimmed.contains("FATAL")
                || trimmed.starts_with("ERROR")
                || trimmed.starts_with("WARN")
                || trimmed.starts_with("INFO")
                || trimmed.starts_with("DEBUG")
                || trimmed
                    .chars()
                    .take(10)
                    .filter(|ch| ch.is_ascii_digit() || *ch == '-')
                    .count()
                    >= 8
        })
        .count();
    if log_hits >= 3 && lines.len() >= 6 {
        return ToolResponseContentType::Log;
    }

    ToolResponseContentType::Text
}

fn compact_json_tool_response(raw: &str, json: &Value, max_chars: usize) -> String {
    let minified = serde_json::to_string(json).unwrap_or_else(|_| raw.to_owned());
    if char_count(&minified) <= max_chars {
        return minified;
    }
    let header = format!(
        "[Tool Response JSON compacted; original {} chars]",
        char_count(raw)
    );
    let notice = "Excerpt is not complete JSON.";
    fit_with_header(
        &header,
        &format!(
            "{}\n{}",
            notice,
            compact_plain_tool_response(
                &minified,
                max_chars.saturating_sub(char_count(&header) + char_count(notice) + 2)
            )
        ),
        max_chars,
    )
}

fn compact_selected_lines(raw: &str, selected: &std::collections::BTreeSet<usize>) -> String {
    let lines = raw.lines().collect::<Vec<_>>();
    let mut out = String::new();
    let mut previous = None;
    for &index in selected {
        if index >= lines.len() {
            continue;
        }
        if let Some(prev) = previous {
            if index > prev + 1 {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&format!("… [{} line(s) omitted] …", index - prev - 1));
                out.push('\n');
            } else if !out.is_empty() {
                out.push('\n');
            }
        }
        out.push_str(lines[index]);
        previous = Some(index);
    }
    out
}

fn compact_line_tool_response(
    raw: &str,
    kind: ToolResponseContentType,
    max_chars: usize,
) -> String {
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    if char_count(&normalized) <= max_chars {
        return normalized;
    }

    let lines = normalized.lines().collect::<Vec<_>>();
    let mut selected = std::collections::BTreeSet::new();
    for index in 0..std::cmp::min(8, lines.len()) {
        selected.insert(index);
    }
    let tail_start = lines.len().saturating_sub(10);
    for index in tail_start..lines.len() {
        selected.insert(index);
    }

    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        let important = match kind {
            ToolResponseContentType::TestOutput => {
                trimmed.contains("FAILED")
                    || trimmed.contains("failures:")
                    || trimmed.contains("panic")
                    || trimmed.contains("AssertionError")
                    || trimmed.contains("error[")
                    || trimmed.contains("error:")
                    || trimmed.contains("test result:")
            }
            ToolResponseContentType::Log => {
                trimmed.contains("ERROR")
                    || trimmed.contains("WARN")
                    || trimmed.contains("FATAL")
                    || trimmed.contains("Traceback")
                    || trimmed.contains("Exception")
                    || trimmed.contains("panic")
            }
            _ => false,
        };
        if important {
            selected.insert(index);
        }
    }

    let label = match kind {
        ToolResponseContentType::TestOutput => "test output",
        ToolResponseContentType::Log => "log output",
        _ => "tool output",
    };
    let header = format!(
        "[Tool Response {} compacted; original {} lines]",
        label,
        lines.len()
    );
    fit_with_header(
        &header,
        &compact_selected_lines(&normalized, &selected),
        max_chars,
    )
}

fn compact_tool_response_content_to(content: &Option<Value>, max_chars: usize) -> String {
    let Some(value) = content else {
        return String::new();
    };

    if let Some(summary) = media_payload_summary(value) {
        return fit_with_header("[Tool Response media payload omitted]", &summary, max_chars);
    }

    match value {
        Value::Array(items) if looks_like_content_parts_array(items) => {
            let joined = items
                .iter()
                .map(compact_tool_response_part)
                .collect::<Vec<_>>()
                .join("\n");
            if char_count(&joined) <= max_chars {
                joined
            } else {
                compact_plain_tool_response(&joined, max_chars)
            }
        }
        _ => {
            let raw = content_to_text(content);
            if char_count(&raw) <= max_chars {
                return raw;
            }
            if let Some(json) = complete_tool_response_json(content, &raw) {
                return compact_json_tool_response(&raw, &json, max_chars);
            }
            let kind = classify_tool_response_text(content, &raw);
            match kind {
                ToolResponseContentType::Json => unreachable!(),
                ToolResponseContentType::TestOutput | ToolResponseContentType::Log => {
                    compact_line_tool_response(&raw, kind, max_chars)
                }
                ToolResponseContentType::Text => compact_plain_tool_response(&raw, max_chars),
            }
        }
    }
}

pub fn compact_tool_response_content(content: &Option<Value>) -> String {
    compact_tool_response_content_to(content, TOOL_RESPONSE_COMPACTION_MAX_CHARS)
}

pub fn compact_tool_response_value(value: &Value) -> String {
    compact_tool_response_content_to(&Some(value.clone()), TOOL_RESPONSE_COMPACTION_MAX_CHARS)
}

pub fn compact_structured_prompt(
    prompt: &str,
    options: StructuredPromptCompactionOptions,
) -> StructuredPromptCompaction {
    let baseline = String::from(prompt);
    let cleaned = compact_prompt(prompt).trim().to_owned();
    let original_chars = char_count(&baseline);
    let cleaned_chars = char_count(&cleaned);
    let mut result = StructuredPromptCompaction {
        text: cleaned.clone(),
        truncated: false,
        mode: if cleaned == baseline.trim() {
            "fit"
        } else {
            "trim-format"
        },
        original_chars,
        compacted_chars: cleaned_chars,
        removed_duplicate_blocks: 0,
        omitted_blocks: 0,
    };

    if cleaned_chars <= options.max_chars {
        return result;
    }

    let blocks = split_prompt_blocks(&cleaned);
    let mut duplicate_scan = std::collections::HashSet::new();
    for block in &blocks {
        let signature = normalized_block_signature(block);
        if signature.is_empty() {
            continue;
        }
        if !duplicate_scan.insert(signature) {
            result.removed_duplicate_blocks += 1;
        }
    }

    if blocks.len() <= 1 {
        let text = if blocks
            .first()
            .map(|block| block_is_tool_sensitive(block))
            .unwrap_or(false)
        {
            STRUCTURED_HEAD_TAIL_MARKER
                .chars()
                .take(options.max_chars)
                .collect::<String>()
        } else {
            fallback_head_tail(&cleaned, options.max_chars)
        };
        result.text = text.trim().to_owned();
        result.truncated = true;
        result.mode = "head-tail";
        result.compacted_chars = char_count(&result.text);
        return result;
    }

    let first_block = if options.preserve_first_block {
        blocks.first().cloned().unwrap_or_default()
    } else {
        String::new()
    };
    let first_cost = char_count(&first_block);
    let marker_cost = char_count(STRUCTURED_OMISSION_MARKER) + 4;
    let mut seen = if first_block.is_empty() {
        std::collections::HashSet::new()
    } else {
        std::collections::HashSet::from([normalized_block_signature(&first_block)])
    };
    let mut tail = Vec::<String>::new();
    let mut remaining = options.max_chars.saturating_sub(
        first_cost
            + if first_block.is_empty() {
                0
            } else {
                marker_cost
            },
    );

    for index in (if options.preserve_first_block { 1 } else { 0 }..blocks.len()).rev() {
        let block = &blocks[index];
        let previous_block = if index > 0 { &blocks[index - 1] } else { "" };
        let signature = normalized_block_signature(block);
        if signature.is_empty() {
            continue;
        }
        if seen.contains(&signature) {
            continue;
        }

        let should_keep_pair = tail.is_empty()
            && block_role(block) != PromptBlockRole::User
            && block_role(previous_block) == PromptBlockRole::User
            && (!options.preserve_first_block || index > 0);
        if should_keep_pair {
            let previous_signature = normalized_block_signature(previous_block);
            let pair = format!("{previous_block}\n\n{block}");
            let pair_cost = char_count(&pair);
            if !seen.contains(&previous_signature) && pair_cost <= remaining {
                tail.insert(0, block.clone());
                tail.insert(0, previous_block.to_owned());
                seen.insert(previous_signature);
                seen.insert(signature);
                remaining = remaining.saturating_sub(pair_cost);
                continue;
            }
        }

        let separator_cost = if tail.is_empty() { 0 } else { 2 };
        let block_cost = char_count(block) + separator_cost;
        if block_cost <= remaining {
            tail.insert(0, block.clone());
            seen.insert(signature);
            remaining = remaining.saturating_sub(block_cost);
            continue;
        }

        if tail.is_empty() && remaining > 96 && !block_is_tool_sensitive(block) {
            let trimmed = compact_long_block(block, remaining);
            if !trimmed.is_empty() {
                tail.insert(0, trimmed);
                seen.insert(signature);
                remaining = 0;
            }
        } else {
            result.omitted_blocks += 1;
        }
    }

    if !first_block.is_empty() && tail.is_empty() && blocks.len() > 1 {
        let latest_pair = if blocks.len() >= 2 {
            format!(
                "{}\n\n{}",
                blocks[blocks.len() - 2],
                blocks[blocks.len() - 1]
            )
        } else {
            blocks.last().cloned().unwrap_or_default()
        };
        let latest = if char_count(&latest_pair) <= options.max_chars {
            latest_pair.trim().to_owned()
        } else {
            let latest_block = blocks.last().cloned().unwrap_or_default();
            if block_is_tool_sensitive(&latest_block) {
                STRUCTURED_OMISSION_MARKER
                    .chars()
                    .take(options.max_chars)
                    .collect::<String>()
            } else {
                compact_long_block(&latest_block, options.max_chars)
            }
        };
        result.text = latest.trim().to_owned();
        result.truncated = true;
        result.mode = "latest-tail";
        result.compacted_chars = char_count(&result.text);
        return result;
    }

    let mut parts = Vec::<String>::new();
    if !first_block.is_empty() {
        parts.push(first_block);
    }
    if !parts.is_empty() && !tail.is_empty() {
        parts.push(STRUCTURED_OMISSION_MARKER.to_owned());
    }
    parts.extend(tail);

    let mut text = parts.join("\n\n").trim().to_owned();
    if text.is_empty() || char_count(&text) > options.max_chars {
        text = fallback_head_tail(&cleaned, options.max_chars);
        result.text = text;
        result.truncated = true;
        result.mode = "head-tail";
        result.compacted_chars = char_count(&result.text);
        return result;
    }

    result.text = text;
    result.truncated = true;
    result.mode = "structured";
    result.compacted_chars = char_count(&result.text);
    result
}

fn render_prompt_parts(system_prompt: &str, conversation: &str) -> String {
    if system_prompt.trim().is_empty() {
        conversation.to_owned()
    } else if conversation.trim().is_empty() {
        system_prompt.to_owned()
    } else {
        format!("{system_prompt}\n{conversation}")
    }
}

fn normalized_block_signature(block: &str) -> String {
    block.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn dedup_system_prompt_blocks(text: &str) -> String {
    let mut seen = std::collections::HashSet::new();
    text.split("\n\n")
        .map(str::trim)
        .filter(|block| !block.is_empty())
        .filter(|block| seen.insert(normalized_block_signature(block)))
        .map(str::to_owned)
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn tool_call_grouped_messages(messages: &[Message]) -> Vec<Vec<Message>> {
    let mut groups = Vec::new();
    let mut index = 0usize;
    while index < messages.len() {
        let message = &messages[index];
        if message.role == "assistant"
            && message
                .tool_calls
                .as_ref()
                .map(|calls| !calls.is_empty())
                .unwrap_or(false)
        {
            let tool_call_ids = message
                .tool_calls
                .as_ref()
                .map(|calls| {
                    calls
                        .iter()
                        .map(|call| call.id.clone())
                        .collect::<std::collections::HashSet<_>>()
                })
                .unwrap_or_default();
            let mut group = vec![message.clone()];
            index += 1;
            while index < messages.len() {
                let next = &messages[index];
                let matched_tool_result = matches!(next.role.as_str(), "tool" | "function")
                    && next
                        .tool_call_id
                        .as_deref()
                        .map(|id| tool_call_ids.contains(id))
                        .unwrap_or(false);
                if !matched_tool_result {
                    break;
                }
                group.push(next.clone());
                index += 1;
            }
            groups.push(group);
            continue;
        }
        groups.push(vec![message.clone()]);
        index += 1;
    }
    groups
}

fn request_with_messages(template: &OpenAIRequest, messages: Vec<Message>) -> OpenAIRequest {
    OpenAIRequest {
        model: template.model.clone(),
        messages,
        stream: template.stream,
        web_search: template.web_search,
        chatgpt_mode: template.chatgpt_mode.clone(),
        user: template.user.clone(),
        tools: template.tools.clone(),
        tool_choice: template.tool_choice.clone(),
        stream_options: template.stream_options.clone(),
    }
}

fn preflight_prompt_parts(
    request: &OpenAIRequest,
    options: &PromptPreflightOptions,
) -> (String, String, String) {
    let (mut system_prompt, mut conversation) = split_prompt(request);
    if let Some(extra) = options
        .extra_system_instructions
        .as_deref()
        .map(str::trim)
        .filter(|extra| !extra.is_empty())
    {
        if !system_prompt.trim().is_empty() {
            system_prompt.push_str("\n\n");
        }
        system_prompt.push_str(extra);
    }
    if options.dedup_system_blocks {
        system_prompt = dedup_system_prompt_blocks(&system_prompt);
    }
    if prompt_compaction_enabled() {
        system_prompt = compact_prompt(&system_prompt);
        conversation = compact_prompt(&conversation);
    }
    if let Some(max_chars) = options.structured_compaction_max_chars {
        conversation = compact_structured_prompt(
            &conversation,
            StructuredPromptCompactionOptions {
                max_chars,
                preserve_first_block: true,
            },
        )
        .text;
    }
    let flat_prompt = render_prompt_parts(&system_prompt, &conversation);
    (system_prompt, conversation, flat_prompt)
}

pub fn preflight_request_to_budget<F>(
    request: &OpenAIRequest,
    options: &PromptPreflightOptions,
    estimate: F,
) -> Result<PromptPreflight>
where
    F: Fn(&str) -> usize,
{
    let (system_prompt, conversation, flat_prompt) = preflight_prompt_parts(request, options);
    let prompt_tokens = estimate(&flat_prompt);
    let Some(max_prompt_tokens) = options.max_prompt_tokens else {
        return Ok(PromptPreflight {
            request: request.clone(),
            system_prompt,
            conversation,
            flat_prompt,
            prompt_tokens,
            truncated: false,
            dropped_messages: 0,
        });
    };
    if prompt_tokens <= max_prompt_tokens {
        return Ok(PromptPreflight {
            request: request.clone(),
            system_prompt,
            conversation,
            flat_prompt,
            prompt_tokens,
            truncated: false,
            dropped_messages: 0,
        });
    }

    let system_messages = request
        .messages
        .iter()
        .filter(|message| message.role == "system")
        .cloned()
        .collect::<Vec<_>>();
    let non_system_messages = request
        .messages
        .iter()
        .filter(|message| message.role != "system")
        .cloned()
        .collect::<Vec<_>>();
    let system_only = request_with_messages(request, system_messages.clone());
    let (_, _, system_only_flat) = preflight_prompt_parts(&system_only, options);
    let system_only_tokens = estimate(&system_only_flat);
    if system_only_tokens > max_prompt_tokens {
        return Err(anyhow!(
            "prompt preflight exceeded budget with system/tool instructions only: {system_only_tokens} > {max_prompt_tokens}"
        ));
    }

    let groups = tool_call_grouped_messages(&non_system_messages);
    let mut kept_from_end = Vec::<Vec<Message>>::new();
    let mut dropped_messages = 0usize;

    for (reverse_index, group) in groups.iter().rev().enumerate() {
        let mut candidate_messages = system_messages.clone();
        let mut candidate_groups = kept_from_end.clone();
        candidate_groups.push(group.clone());
        for kept_group in candidate_groups.iter().rev() {
            candidate_messages.extend(kept_group.clone());
        }
        let candidate_request = request_with_messages(request, candidate_messages);
        let (_, _, candidate_flat) = preflight_prompt_parts(&candidate_request, options);
        if estimate(&candidate_flat) <= max_prompt_tokens {
            kept_from_end.push(group.clone());
            continue;
        }

        dropped_messages = groups
            .iter()
            .take(groups.len().saturating_sub(reverse_index))
            .map(Vec::len)
            .sum();
        break;
    }

    let mut final_messages = system_messages;
    for group in kept_from_end.iter().rev() {
        final_messages.extend(group.clone());
    }
    let final_request = request_with_messages(request, final_messages);
    let (system_prompt, conversation, flat_prompt) =
        preflight_prompt_parts(&final_request, options);
    let prompt_tokens = estimate(&flat_prompt);

    Ok(PromptPreflight {
        request: final_request,
        system_prompt,
        conversation,
        flat_prompt,
        prompt_tokens,
        truncated: dropped_messages > 0,
        dropped_messages,
    })
}

pub fn usage_from_text(prompt: &str, output: &str, include_cached_tokens: bool) -> Usage {
    let prompt_tokens = estimate_tokens(prompt);
    let completion_tokens = estimate_tokens(output);
    Usage {
        prompt_tokens,
        completion_tokens,
        total_tokens: prompt_tokens + completion_tokens,
        prompt_tokens_details: include_cached_tokens.then(|| json!({ "cached_tokens": 0 })),
    }
}

/// Truncate a potentially-multibyte string without panicking on char boundaries.
/// `&text[..max_len]` slices by byte and panics if `max_len` lands mid-codepoint;
/// this walks char indices so the cut is always on a boundary.
pub fn truncate_error_payload(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        return text.to_owned();
    }
    let mut end = max_len;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    let mut out = text[..end].to_owned();
    out.push_str("...");
    out
}

/// SSRF guard for user-supplied URLs fetched server-side (e.g. multimodal image_url).
/// Denies non-http(s) schemes, private/loopback/link-local/metadata IP literals, and
/// known cloud-metadata hostnames.
/// ponytail: does not resolve DNS, so a hostname that resolves to a private IP at
/// lookup time still passes. Close the DNS-rebinding gap later by resolving and
/// re-checking the resolved IPs before connect (requires a custom reqwest connector).
pub fn url_is_safe_for_fetch(raw: &str) -> Result<Url> {
    let parsed = Url::parse(raw).map_err(|_| anyhow!("invalid URL"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(anyhow!("unsupported URL scheme: {}", parsed.scheme()));
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow!("URL missing host"))?;
    let blocked_hosts = [
        "169.254.169.254",
        "169.254.169.253",
        "metadata.google.internal",
        "metadata.aws.internal",
        "fdfe:8d69:ac16::1",
    ];
    if blocked_hosts.contains(&host) {
        return Err(anyhow!("blocked metadata host: {host}"));
    }
    if let Some(ip) = parsed.host() {
        let is_unsafe = match ip {
            url::Host::Ipv4(addr) => {
                addr.is_private()
                    || addr.is_loopback()
                    || addr.is_link_local()
                    || addr.is_broadcast()
                    || addr.is_unspecified()
                    || addr.is_documentation()
                    || addr.octets()[0] == 169 && addr.octets()[1] == 254
            }
            url::Host::Ipv6(addr) => {
                addr.is_loopback()
                    || addr.is_unspecified()
                    || addr.is_multicast()
                    || addr.is_unicast_link_local()
            }
            url::Host::Domain(_) => false,
        };
        if is_unsafe {
            return Err(anyhow!("blocked target: {host}"));
        }
    }
    Ok(parsed)
}

/// Refuse to start a proxy bound to a non-loopback host unless an API key is set.
/// Mirrors the hub's guard so standalone/embedded providers can't accidentally
/// expose an unauthenticated endpoint on a public interface.
pub fn enforce_loopback_guard(host: &str, api_key: Option<&str>) -> Result<()> {
    let ip: std::net::IpAddr = host
        .parse()
        .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
    if !ip.is_loopback() && api_key.map(str::is_empty).unwrap_or(true) {
        return Err(anyhow!(
            "refusing to start on non-loopback host {ip} without API_KEY set; set API_KEY or bind to 127.0.0.1/localhost"
        ));
    }
    Ok(())
}

pub fn content_to_text(content: &Option<Value>) -> String {
    let Some(content) = content else {
        return String::new();
    };

    match content {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .map(|item| match item {
                Value::Object(map) => map
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| item.to_string()),
                _ => item.to_string(),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(_) => content.to_string(),
        other => other.to_string(),
    }
}

fn find_tool_name_by_call_id(
    messages: &[Message],
    index: usize,
    tool_call_id: &str,
) -> Option<String> {
    for message in messages[..index].iter().rev() {
        if message.role == "assistant" {
            if let Some(tool_calls) = &message.tool_calls {
                if let Some(found) = tool_calls.iter().find(|call| call.id == tool_call_id) {
                    return Some(found.function.name.clone());
                }
            }
        }
    }
    None
}

fn tool_instructions(tools: &[FunctionToolDefinition], tool_choice: Option<&Value>) -> String {
    if tools.is_empty() {
        return String::new();
    }

    let tools_json = serde_json::to_string(tools).unwrap_or_else(|_| "[]".to_owned());
    let mut out = format!(
        "\n\n# TOOLS AVAILABLE\nYou have access to the following tools:\n{tools_json}\n\nThese tools are REAL and executable in this session.\nDo NOT claim you cannot access tools, that tools are only pasted text, or that tool execution is unavailable.\nIf the user asks to test, use, inspect, read, write, search, or run tools, you MUST answer by emitting tool calls.\n\n# TOOL CALLING FORMAT (MANDATORY)\nTo use a tool, you MUST output a JSON object wrapped EXACTLY in these tags:\n<tool_call>\n{{\"name\": \"tool_name\", \"arguments\": {{\"param_name\": \"value\"}}}}\n</tool_call>\n\nCRITICAL RULES:\n1. ONLY use the tags above for tool calling. NEVER output raw JSON without tags.\n2. You can call multiple tools by outputting multiple <tool_call> blocks consecutively.\n3. Do NOT output any other text after your <tool_call> blocks.\n4. The JSON inside the tags MUST be valid and include the \"arguments\" field.\n5. NEVER invent tool names. Only use tools from the list above.\n"
    );

    /* a forced tool names itself under `function.name` OR at the top level
    ({type:"function"|"custom", name}) */
    let forced_name = tool_choice.and_then(|value| {
        value
            .get("function")
            .and_then(|function| function.get("name"))
            .or_else(|| value.get("name"))
            .and_then(Value::as_str)
    });
    let choice_type = tool_choice
        .and_then(|value| value.get("type"))
        .and_then(Value::as_str);

    if let Some(name) = forced_name {
        out.push_str(&format!(
            "\nCRITICAL: You MUST call the tool \"{name}\" in this response.\n"
        ));
    } else if choice_type == Some("allowed_tools") {
        /* restrict the callable subset without dropping tools from the list above */
        let names: Vec<&str> = tool_choice
            .and_then(|value| value.get("tools"))
            .and_then(Value::as_array)
            .map(|tools| {
                tools
                    .iter()
                    .filter_map(|tool| {
                        tool.get("name")
                            .or_else(|| tool.get("function").and_then(|f| f.get("name")))
                            .and_then(Value::as_str)
                    })
                    .collect()
            })
            .unwrap_or_default();
        if !names.is_empty() {
            let verb = if tool_choice
                .and_then(|v| v.get("mode"))
                .and_then(Value::as_str)
                == Some("required")
            {
                "You MUST call"
            } else {
                "You may only call"
            };
            out.push_str(&format!(
                "\nCRITICAL: {verb} one of these tools: {}.\n",
                names.join(", ")
            ));
        }
    } else if tool_choice.and_then(Value::as_str) == Some("required") {
        out.push_str("\nCRITICAL: You MUST call one of the available tools in this response.\n");
    } else if tool_choice.and_then(Value::as_str) == Some("none") || choice_type == Some("none") {
        out.push_str("\nCRITICAL: Do NOT call tools in this response.\n");
    }

    out
}

/// Returns `(system_prompt, conversation)` split so each provider can place the
/// system instructions the way its channel actually accepts them. NOTE: the
/// chatgpt.com web endpoint has no working system lane — it silently drops inline
/// `role:"system"` turns — so the bridge folds the system prompt into the user
/// message there (see `foldChatGPTSystemPrompt` in the Playwright bridge).
pub fn split_prompt(request: &OpenAIRequest) -> (String, String) {
    let mut system_prompt = String::new();
    let mut prompt = String::new();

    for (index, message) in request.messages.iter().enumerate() {
        let content_str = content_to_text(&message.content);

        match message.role.as_str() {
            "system" => {
                system_prompt.push_str(&content_str);
                system_prompt.push_str("\n\n");
            }
            "user" => {
                prompt.push_str("User: ");
                prompt.push_str(&content_str);
                prompt.push_str("\n\n");
            }
            "assistant" => {
                let mut assistant_content = content_str;
                if let Some(reasoning) = &message.reasoning_content {
                    assistant_content =
                        format!("<think>\n{reasoning}\n</think>\n{assistant_content}");
                }
                if let Some(tool_calls) = &message.tool_calls {
                    for tool_call in tool_calls {
                        let args_value = robust_parse_json(&tool_call.function.arguments)
                            .unwrap_or_else(|| Value::String(tool_call.function.arguments.clone()));
                        let tool_json = json!({
                            "name": tool_call.function.name,
                            "arguments": args_value,
                        });
                        assistant_content.push_str("\n<tool_call>\n");
                        assistant_content.push_str(&tool_json.to_string());
                        assistant_content.push_str("\n</tool_call>");
                    }
                }
                prompt.push_str("Assistant: ");
                prompt.push_str(assistant_content.trim());
                prompt.push_str("\n\n");
            }
            "tool" | "function" => {
                let content_str = compact_tool_response_content(&message.content);
                let tool_name = message.name.clone().or_else(|| {
                    message.tool_call_id.as_deref().and_then(|tool_call_id| {
                        find_tool_name_by_call_id(&request.messages, index, tool_call_id)
                    })
                });
                prompt.push_str("Tool Response (");
                prompt.push_str(tool_name.as_deref().unwrap_or("tool"));
                prompt.push_str("): ");
                prompt.push_str(&content_str);
                prompt.push_str("\n\n");
            }
            other => {
                prompt.push_str(other);
                prompt.push_str(": ");
                prompt.push_str(&content_str);
                prompt.push_str("\n\n");
            }
        }
    }

    if let Some(tools) = &request.tools {
        system_prompt.push_str(&tool_instructions(tools, request.tool_choice.as_ref()));
    }

    (system_prompt.trim_end().to_owned(), prompt)
}

pub fn build_prompt(request: &OpenAIRequest) -> String {
    let (system_prompt, prompt) = split_prompt(request);
    let prompt = render_prompt_parts(&system_prompt, &prompt);
    if prompt_compaction_enabled() {
        compact_prompt(&prompt)
    } else {
        prompt
    }
}

pub fn robust_parse_json(input: &str) -> Option<Value> {
    let mut sanitized = input.trim().to_owned();
    if let Some(stripped) = sanitized.strip_prefix("```json") {
        sanitized = stripped.trim().to_owned();
    }
    if let Some(stripped) = sanitized.strip_suffix("```") {
        sanitized = stripped.trim().to_owned();
    }

    let start = match (sanitized.find('{'), sanitized.find('[')) {
        (Some(object), Some(array)) => object.min(array),
        (Some(object), None) => object,
        (None, Some(array)) => array,
        (None, None) => return None,
    };
    let candidate = sanitized[start..].trim();

    serde_json::from_str(candidate)
        .ok()
        .or_else(|| json5::from_str(candidate).ok())
        .or_else(|| {
            let balanced = balance_json(candidate);
            serde_json::from_str(&balanced)
                .ok()
                .or_else(|| json5::from_str(&balanced).ok())
        })
}

fn balance_json(input: &str) -> String {
    let mut out = String::new();
    let mut in_string = false;
    let mut escaped = false;
    let mut open_braces = 0i32;
    let mut open_brackets = 0i32;

    for ch in input.chars() {
        if escaped {
            out.push(ch);
            escaped = false;
            continue;
        }

        if ch == '\\' {
            out.push(ch);
            escaped = true;
            continue;
        }

        if ch == '"' {
            out.push(ch);
            in_string = !in_string;
            continue;
        }

        if !in_string {
            match ch {
                '{' => open_braces += 1,
                '}' => open_braces -= 1,
                '[' => open_brackets += 1,
                ']' => open_brackets -= 1,
                _ => {}
            }
        }

        out.push(ch);
    }

    if in_string {
        out.push('"');
    }
    if open_brackets > 0 {
        out.push_str(&"]".repeat(open_brackets as usize));
    }
    if open_braces > 0 {
        out.push_str(&"}".repeat(open_braces as usize));
    }

    out.replace(",}", "}").replace(",]", "]")
}

static TOOL_OPEN_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)<tool_call\b[^>]*>").expect("valid tool open regex"));
static TOOL_PARAM_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?is)<parameter\b[^>]*\bname\s*=\s*["']([^"']+)["'][^>]*>(.*?)</parameter>"#)
        .expect("valid tool parameter regex")
});
static TOOL_NAME_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?is)<name>(.*?)</name>"#).expect("valid tool name regex"));
static TOOL_OPEN_NAME_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)<tool_call\b[^>]*\bname\s*=\s*["']([^"']+)["']"#)
        .expect("valid tool open name regex")
});
static TOOL_CLOSE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)</tool_call>").expect("valid tool close regex"));

#[derive(Default, Debug)]
pub struct ParserResult {
    pub text: String,
    pub tool_calls: Vec<ParsedToolCall>,
}

#[derive(Debug, Default)]
pub struct StreamingToolParser {
    buffer: String,
    inside_tool: bool,
    current_open_tag: String,
    emitted_tool_calls: usize,
}

impl StreamingToolParser {
    pub fn new() -> Self {
        Self {
            current_open_tag: "<tool_call>".to_owned(),
            ..Self::default()
        }
    }

    pub fn emitted_tool_call_count(&self) -> usize {
        self.emitted_tool_calls
    }

    pub fn feed(&mut self, chunk: &str) -> ParserResult {
        self.buffer.push_str(chunk);
        let mut result = ParserResult::default();

        while !self.buffer.is_empty() {
            if !self.inside_tool {
                let tag_open = find_tool_open(&self.buffer);
                let json_open = find_bare_json_start(&self.buffer);
                /* whichever structure starts first wins; a <tool_call> tag's own '<'
                precedes its inner '{', so tags are still preferred when present */
                let tag_first = match (&tag_open, json_open) {
                    (Some((tag_start, _, _)), Some(js)) => *tag_start <= js,
                    (Some(_), None) => true,
                    _ => false,
                };

                if tag_first {
                    let (start, end, open_tag) =
                        tag_open.expect("tag_first is only true when a tag matched");
                    result.text.push_str(&self.buffer[..start]);
                    self.inside_tool = true;
                    self.current_open_tag = open_tag;
                    // drain the consumed prefix in place instead of reallocating the tail
                    self.buffer.drain(..end);
                } else if let Some(js) = json_open {
                    match find_balanced_json_end(&self.buffer, js) {
                        Some(je) => {
                            let calls = parse_json_tool_calls(&self.buffer[js..je]);
                            if calls.is_empty() {
                                /* balanced but not a tool call → ordinary text */
                                result.text.push_str(&self.buffer[..je]);
                            } else {
                                result.text.push_str(&self.buffer[..js]);
                                self.emitted_tool_calls += calls.len();
                                result.tool_calls.extend(calls);
                            }
                            self.buffer.drain(..je);
                        }
                        None => {
                            /* object still streaming in → hold it until it closes */
                            result.text.push_str(&self.buffer[..js]);
                            self.buffer.drain(..js);
                            break;
                        }
                    }
                } else {
                    let flush_index =
                        find_partial_tool_open_index(&self.buffer).unwrap_or(self.buffer.len());
                    result.text.push_str(&self.buffer[..flush_index]);
                    self.buffer.drain(..flush_index);
                    break;
                }
            } else if let Some(end_idx) = find_tool_close_index(&self.buffer) {
                let content = self.buffer[..end_idx].to_owned();
                self.buffer.drain(..end_idx + "</tool_call>".len());
                self.process_tool_content(&content, &mut result);
                self.inside_tool = false;
                self.current_open_tag = "<tool_call>".to_owned();
            } else {
                break;
            }
        }

        result
    }

    pub fn flush(&mut self) -> ParserResult {
        let mut result = ParserResult::default();
        if self.inside_tool {
            let tool_calls = self.parse_tool_content(self.buffer.trim());
            if tool_calls.is_empty() {
                result.text.push_str(&self.current_open_tag);
                result.text.push_str(&self.buffer);
            } else {
                self.emitted_tool_calls += tool_calls.len();
                result.tool_calls.extend(tool_calls);
            }
        } else {
            result.text.push_str(&self.buffer);
        }

        self.buffer.clear();
        self.inside_tool = false;
        self.current_open_tag = "<tool_call>".to_owned();
        result
    }

    fn process_tool_content(&mut self, content: &str, result: &mut ParserResult) {
        let tool_calls = self.parse_tool_content(content.trim());
        if tool_calls.is_empty() {
            result.text.push_str(&self.current_open_tag);
            result.text.push_str(content);
            result.text.push_str("</tool_call>");
        } else {
            self.emitted_tool_calls += tool_calls.len();
            result.tool_calls.extend(tool_calls);
        }
    }

    fn parse_tool_content(&self, content: &str) -> Vec<ParsedToolCall> {
        if content.is_empty() {
            return Vec::new();
        }

        if content.contains("<parameter") {
            let mut args = Map::new();
            for capture in TOOL_PARAM_RE.captures_iter(content) {
                let Some(name) = capture.get(1).map(|value| value.as_str()) else {
                    continue;
                };
                let Some(raw) = capture.get(2).map(|value| value.as_str().trim()) else {
                    continue;
                };
                let value = robust_parse_json(raw).unwrap_or_else(|| Value::String(raw.to_owned()));
                args.insert(name.to_owned(), value);
            }

            if args.is_empty() {
                return Vec::new();
            }

            return extract_tool_name_from_markup(&self.current_open_tag, content)
                .map(|tool_name| {
                    vec![ParsedToolCall {
                        id: format!("call_{}", Uuid::new_v4()),
                        name: tool_name,
                        arguments: Value::Object(args),
                    }]
                })
                .unwrap_or_default();
        }

        let Some(parsed) = robust_parse_json(content) else {
            return Vec::new();
        };
        match parsed {
            Value::Array(values) => values
                .into_iter()
                .filter_map(parse_tool_call_value)
                .collect(),
            other => parse_tool_call_value(other).into_iter().collect(),
        }
    }
}

fn extract_tool_name_from_markup(open_tag: &str, content: &str) -> Option<String> {
    if let Some(captures) = TOOL_OPEN_NAME_RE.captures(open_tag) {
        return captures.get(1).map(|value| value.as_str().to_owned());
    }
    TOOL_NAME_RE
        .captures(content)
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str().trim().to_owned())
}

fn parse_tool_call_value(value: Value) -> Option<ParsedToolCall> {
    let object = value.as_object()?;
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .or_else(|| object.get("tool_name").and_then(Value::as_str))
        .or_else(|| object.get("tool").and_then(Value::as_str))
        .or_else(|| {
            object
                .get("function")
                .and_then(Value::as_object)
                .and_then(|function| function.get("name"))
                .and_then(Value::as_str)
        })?
        .to_owned();

    let arguments = object
        .get("arguments")
        .cloned()
        .or_else(|| object.get("args").cloned())
        .or_else(|| object.get("parameters").cloned())
        .or_else(|| object.get("input").cloned())
        .or_else(|| {
            object
                .get("function")
                .and_then(Value::as_object)
                .and_then(|function| function.get("arguments"))
                .cloned()
        })
        .unwrap_or_else(|| {
            let mut rest = Map::new();
            for (key, value) in object {
                if key != "name" && key != "tool_name" && key != "tool" && key != "function" {
                    rest.insert(key.clone(), value.clone());
                }
            }
            Value::Object(rest)
        });

    let parsed_args = match arguments {
        Value::Null => Value::Object(Map::new()),
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                Value::Object(Map::new())
            } else {
                robust_parse_json(trimmed).unwrap_or(Value::String(text))
            }
        }
        other => other,
    };

    Some(ParsedToolCall {
        id: format!("call_{}", Uuid::new_v4()),
        name,
        arguments: parsed_args,
    })
}

/* A JSON value is only treated as a leaked tool call when it carries BOTH a
name-key and an arguments-key. Without this, plain data like {"name":"Jo"} would
be mistaken for a call. */
fn looks_like_tool_call(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    let has_name = ["name", "tool_name", "tool"]
        .iter()
        .any(|key| object.get(*key).and_then(Value::as_str).is_some())
        || object
            .get("function")
            .and_then(|function| function.get("name"))
            .and_then(Value::as_str)
            .is_some();
    let has_args = ["arguments", "args", "parameters", "input"]
        .iter()
        .any(|key| object.contains_key(*key))
        || object
            .get("function")
            .and_then(|function| function.get("arguments"))
            .is_some();
    has_name && has_args
}

fn parse_json_tool_calls(candidate: &str) -> Vec<ParsedToolCall> {
    robust_parse_json(candidate)
        .map(json_value_tool_calls)
        .unwrap_or_default()
}

fn json_value_tool_calls(parsed: Value) -> Vec<ParsedToolCall> {
    let values = match parsed {
        Value::Array(values) => values,
        other => vec![other],
    };
    values
        .into_iter()
        .filter(looks_like_tool_call)
        .filter_map(parse_tool_call_value)
        .collect()
}

/* Catch tool calls a model emitted as bare or ```-fenced JSON instead of the
<tool_call> tags it was told to use. Otherwise they reach the client as plain
text and agents like Kilo/Pi see no tool call at all. Returns the text with any
lifted calls removed. Only whole-message JSON or fenced blocks are scanned —
inline JSON amid prose is left alone to avoid false positives. */
pub fn extract_tool_calls_from_text(text: &str) -> (String, Vec<ParsedToolCall>) {
    let trimmed = text.trim();
    /* whole message is exactly the JSON call — strict parse so prose around JSON
    isn't scraped out from under itself */
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
            let calls = json_value_tool_calls(parsed);
            if !calls.is_empty() {
                return (String::new(), calls);
            }
        }
    }

    let mut cleaned = String::with_capacity(text.len());
    let mut tool_calls = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find("```") {
        let after_open = &rest[open + 3..];
        let Some(close_rel) = after_open.find("```") else {
            break;
        };
        let block = &after_open[..close_rel];
        /* drop the optional language tag on the fence's first line */
        let inner = block
            .split_once('\n')
            .map(|(_, rest)| rest)
            .unwrap_or("")
            .trim();
        let calls = robust_parse_json(inner)
            .map(json_value_tool_calls)
            .unwrap_or_default();
        if calls.is_empty() {
            cleaned.push_str(&rest[..open + 3 + close_rel + 3]);
        } else {
            cleaned.push_str(&rest[..open]);
            tool_calls.extend(calls);
        }
        rest = &after_open[close_rel + 3..];
    }
    cleaned.push_str(rest);
    (cleaned.trim().to_owned(), tool_calls)
}

/* First byte index of a '{' or '[' — a possible bare-JSON tool call the model
emitted without the <tool_call> tags. Structural JSON chars are ASCII, so a byte
index is always a valid char boundary for slicing. */
fn find_bare_json_start(buffer: &str) -> Option<usize> {
    buffer.bytes().position(|b| b == b'{' || b == b'[')
}

/* End index (exclusive) of the balanced JSON value that begins at `start`, or None
if it hasn't fully arrived yet (mid-stream). String-aware so braces inside string
literals don't throw off the depth count. */
fn find_balanced_json_end(buffer: &str, start: usize) -> Option<usize> {
    let bytes = buffer.as_bytes();
    let open = bytes[start];
    let close = match open {
        b'{' => b'}',
        b'[' => b']',
        _ => return None,
    };
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, &byte) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
        } else if byte == b'"' {
            in_string = true;
        } else if byte == open {
            depth += 1;
        } else if byte == close {
            depth -= 1;
            if depth == 0 {
                return Some(offset + 1);
            }
        }
    }
    None
}

fn find_tool_open(buffer: &str) -> Option<(usize, usize, String)> {
    let captures = TOOL_OPEN_RE.find(buffer)?;
    Some((
        captures.start(),
        captures.end(),
        buffer[captures.start()..captures.end()].to_owned(),
    ))
}

fn find_tool_close_index(buffer: &str) -> Option<usize> {
    TOOL_CLOSE_RE.find(buffer).map(|m| m.start())
}

fn find_partial_tool_open_index(buffer: &str) -> Option<usize> {
    let prefix = "<tool_call";
    let bytes = buffer.as_bytes();
    for idx in (0..bytes.len()).rev() {
        if bytes[idx] != b'<' {
            continue;
        }
        let tail = &buffer[idx..];
        if tail.len() < prefix.len() {
            /* still mid-name: hold if the tail is a prefix of "<tool_call" */
            if prefix[..tail.len()].eq_ignore_ascii_case(tail) {
                return Some(idx);
            }
        } else if tail[..prefix.len()].eq_ignore_ascii_case(prefix) && !tail.contains('>') {
            /* full "<tool_call" but no closing '>' yet — attributes or the '>' may
            still be streaming in; hold it so the tag doesn't leak as text */
            return Some(idx);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_parser_extracts_multiple_json_array_tool_calls() {
        let mut parser = StreamingToolParser::new();
        let parsed = parser.feed(
            r#"<tool_call>[
{"name":"read_file","arguments":{"path":"Cargo.toml"}},
{"name":"run_tests","arguments":{"filter":"proxy_core"}}
]</tool_call>"#,
        );

        assert_eq!(parsed.text, "");
        assert_eq!(parsed.tool_calls.len(), 2);
        assert_eq!(parsed.tool_calls[0].name, "read_file");
        assert_eq!(parsed.tool_calls[1].name, "run_tests");
        assert_eq!(parser.emitted_tool_call_count(), 2);
    }

    #[test]
    fn extract_lifts_whole_message_bare_json_tool_call() {
        let (text, calls) =
            extract_tool_calls_from_text(r#"{"name":"read_file","arguments":{"path":"a.rs"}}"#);
        assert_eq!(text, "");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "read_file");
    }

    #[test]
    fn extract_lifts_fenced_json_tool_call_and_keeps_prose() {
        let (text, calls) = extract_tool_calls_from_text(
            "Sure, running it:\n```json\n{\"name\":\"run_tests\",\"arguments\":{\"filter\":\"x\"}}\n```",
        );
        assert_eq!(text, "Sure, running it:");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "run_tests");
    }

    #[test]
    fn extract_ignores_plain_json_without_arguments_key() {
        /* {"name":..} with no args-key is data, not a tool call — must stay text */
        let (text, calls) = extract_tool_calls_from_text(r#"{"name":"John","age":30}"#);
        assert!(calls.is_empty());
        assert_eq!(text, r#"{"name":"John","age":30}"#);
    }

    #[test]
    fn extract_leaves_ordinary_code_fence_alone() {
        let input = "Here:\n```rust\nfn main() {}\n```";
        let (text, calls) = extract_tool_calls_from_text(input);
        assert!(calls.is_empty());
        assert_eq!(text, input);
    }

    #[test]
    fn custom_tool_deserializes_without_400_and_names_itself() {
        /* a type:"custom" tool has no `function` object — it must not fail the request */
        let req: OpenAIRequest = serde_json::from_value(json!({
            "model": "m",
            "messages": [],
            "tools": [
                { "type": "function", "function": { "name": "read_file" } },
                { "type": "custom", "name": "code_exec", "description": "run code" }
            ]
        }))
        .expect("custom tool must not break deserialization");
        let tools = req.tools.expect("tools present");
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].tool_name(), Some("read_file"));
        assert_eq!(tools[1].tool_type, "custom");
        assert_eq!(tools[1].tool_name(), Some("code_exec"));
    }

    #[test]
    fn tool_instructions_restricts_to_allowed_tools() {
        let tools = vec![FunctionToolDefinition {
            tool_type: "function".to_owned(),
            function: Some(FunctionToolSpec {
                name: "alpha".to_owned(),
                description: None,
                parameters: None,
                strict: None,
            }),
            name: None,
            description: None,
        }];
        let choice = json!({
            "type": "allowed_tools",
            "mode": "required",
            "tools": [{ "type": "function", "name": "alpha" }]
        });
        let out = tool_instructions(&tools, Some(&choice));
        assert!(out.contains("You MUST call one of these tools: alpha"));
    }

    #[test]
    fn tool_instructions_forces_top_level_named_tool() {
        let tools = vec![FunctionToolDefinition {
            tool_type: "custom".to_owned(),
            function: None,
            name: Some("code_exec".to_owned()),
            description: None,
        }];
        /* Responses/custom style: name at the top level, not under `function` */
        let choice = json!({ "type": "custom", "name": "code_exec" });
        let out = tool_instructions(&tools, Some(&choice));
        assert!(out.contains("You MUST call the tool \"code_exec\""));
    }

    #[test]
    fn streaming_parser_lifts_bare_json_tool_call_without_tags() {
        let mut parser = StreamingToolParser::new();
        let parsed = parser.feed(r#"{"name":"bash","arguments":{"command":"ls"}}"#);
        assert_eq!(parsed.text, "");
        assert_eq!(parsed.tool_calls.len(), 1);
        assert_eq!(parsed.tool_calls[0].name, "bash");
    }

    #[test]
    fn streaming_parser_lifts_multiple_space_separated_bare_json_calls() {
        /* the exact Kilo failure: {..} {..} {..} bare, space-separated, no tags */
        let mut parser = StreamingToolParser::new();
        let parsed = parser.feed(
            r#"{"name":"bash","arguments":{"command":"ls"}} {"name":"glob","arguments":{"pattern":"*.md"}} {"name":"read","arguments":{"filePath":"a"}}"#,
        );
        let names: Vec<_> = parsed.tool_calls.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["bash", "glob", "read"]);
    }

    #[test]
    fn streaming_parser_buffers_bare_json_split_across_chunks() {
        let mut parser = StreamingToolParser::new();
        let first = parser.feed(r#"{"name":"read","argu"#);
        assert!(first.tool_calls.is_empty());
        let second = parser.feed(r#"ments":{"filePath":"a.rs"}}"#);
        assert_eq!(second.tool_calls.len(), 1);
        assert_eq!(second.tool_calls[0].name, "read");
    }

    #[test]
    fn streaming_parser_does_not_leak_tool_call_tag_split_at_bracket() {
        /* the exact live streaming bug: a chunk lands on "<tool_call" (no '>' yet);
        it must be held, not leaked as content, and the tags must never appear in text */
        let mut parser = StreamingToolParser::new();
        let first = parser.feed("<tool_call");
        assert_eq!(first.text, "");
        let second =
            parser.feed(">\n{\"name\":\"bash\",\"arguments\":{\"command\":\"ls\"}}\n</tool_call>");
        assert_eq!(second.text, "");
        assert_eq!(second.tool_calls.len(), 1);
        assert_eq!(second.tool_calls[0].name, "bash");
    }

    #[test]
    fn streaming_parser_leaves_non_tool_json_as_text() {
        let mut parser = StreamingToolParser::new();
        let parsed = parser.feed(r#"the config is {"name":"John","age":30} today"#);
        assert!(parsed.tool_calls.is_empty());
        assert_eq!(
            parsed.text,
            r#"the config is {"name":"John","age":30} today"#
        );
    }

    #[test]
    fn streaming_parser_handles_split_tool_call_tags() {
        let mut parser = StreamingToolParser::new();
        let first = parser.feed("before <tool");
        let second =
            parser.feed(r#"_call>{"name":"lookup","arguments":{"id":7}}</tool_call> after"#);

        assert_eq!(first.text, "before ");
        assert_eq!(second.text, " after");
        assert_eq!(second.tool_calls.len(), 1);
        assert_eq!(second.tool_calls[0].name, "lookup");
    }

    #[test]
    fn build_prompt_honors_required_tool_choice() {
        let prompt = build_prompt(&OpenAIRequest {
            model: "test".to_owned(),
            messages: vec![Message {
                role: "user".to_owned(),
                content: Some(Value::String("inspect".to_owned())),
                tool_calls: None,
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            }],
            stream: None,
            web_search: None,
            chatgpt_mode: None,
            user: None,
            tools: Some(vec![FunctionToolDefinition {
                tool_type: "function".to_owned(),
                function: Some(FunctionToolSpec {
                    name: "read_file".to_owned(),
                    description: None,
                    parameters: None,
                    strict: None,
                }),
                name: None,
                description: None,
            }]),
            tool_choice: Some(json!("required")),
            stream_options: None,
        });

        assert!(prompt.contains("MUST call one of the available tools"));
    }

    #[test]
    fn tool_instructions_minify_schema_and_omit_nulls() {
        let request = OpenAIRequest {
            model: "test".to_owned(),
            messages: vec![Message {
                role: "user".to_owned(),
                content: Some(Value::String("inspect".to_owned())),
                tool_calls: None,
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            }],
            stream: None,
            web_search: None,
            chatgpt_mode: None,
            user: None,
            tools: Some(vec![FunctionToolDefinition {
                tool_type: "function".to_owned(),
                function: Some(FunctionToolSpec {
                    name: "read_file".to_owned(),
                    description: None,
                    parameters: Some(
                        json!({"type":"object","properties":{"path":{"type":"string"}}}),
                    ),
                    strict: None,
                }),
                name: None,
                description: None,
            }]),
            tool_choice: None,
            stream_options: None,
        };
        let (system_prompt, _conversation) = split_prompt(&request);

        assert!(system_prompt.contains(r#""name":"read_file""#));
        assert!(system_prompt.contains(r#"[{"type":"function""#));
        assert!(!system_prompt.contains(r#""description":null"#));
        assert!(!system_prompt.contains(r#""strict":null"#));
    }

    #[test]
    fn constant_time_eq_rejects_mismatched_inputs() {
        assert!(constant_time_eq("secret", "secret"));
        assert!(!constant_time_eq("secret", "secre"));
        assert!(!constant_time_eq("secret", "secrex"));
        assert!(!constant_time_eq("", "x"));
        assert!(constant_time_eq("", ""));
    }

    #[test]
    fn safe_account_id_rejects_path_traversal() {
        assert!(is_safe_account_id("a1b2c3d4-e5f6-7890-abcd-ef1234567890"));
        assert!(is_safe_account_id("work-account"));
        assert!(is_safe_account_id("_default"));
        assert!(!is_safe_account_id(""));
        assert!(!is_safe_account_id("../escape"));
        assert!(!is_safe_account_id("a/b"));
        assert!(!is_safe_account_id("a\\b"));
        assert!(!is_safe_account_id("a:b"));
        assert!(!is_safe_account_id("."));
        assert!(!is_safe_account_id(&"a".repeat(65)));
    }

    #[test]
    fn truncate_error_payload_handles_multibyte_boundary() {
        // 4-byte emoji repeated; byte 400 lands mid-codepoint.
        let text = "🦀".repeat(200);
        let truncated = truncate_error_payload(&text, 400);
        assert!(truncated.ends_with("..."));
        // Must not panic and must be valid UTF-8.
        let _ = truncated.chars().count();
    }

    #[test]
    fn truncate_error_payload_preserves_short_input() {
        assert_eq!(truncate_error_payload("short", 400), "short");
    }

    #[test]
    fn url_safety_denies_private_and_metadata() {
        assert!(url_is_safe_for_fetch("http://127.0.0.1/").is_err());
        assert!(url_is_safe_for_fetch("http://10.0.0.1/").is_err());
        assert!(url_is_safe_for_fetch("http://169.254.169.254/").is_err());
        assert!(url_is_safe_for_fetch("http://192.168.1.1/").is_err());
        assert!(url_is_safe_for_fetch("http://[::1]/").is_err());
        assert!(url_is_safe_for_fetch("ftp://example.com/").is_err());
        assert!(url_is_safe_for_fetch("javascript:alert(1)").is_err());
        assert!(url_is_safe_for_fetch("http://metadata.google.internal/").is_err());
    }

    #[test]
    fn url_safety_allows_public() {
        assert!(url_is_safe_for_fetch("https://example.com/img.png").is_ok());
        assert!(url_is_safe_for_fetch("https://chat.qwen.ai/img.png").is_ok());
    }

    #[test]
    fn split_prompt_extracts_system_from_conversation() {
        let req = OpenAIRequest {
            model: "test".to_owned(),
            messages: vec![
                Message {
                    role: "system".to_owned(),
                    content: Some(Value::String("You are a pirate.".to_owned())),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                    reasoning_content: None,
                },
                Message {
                    role: "user".to_owned(),
                    content: Some(Value::String("Hello.".to_owned())),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                    reasoning_content: None,
                },
            ],
            stream: None,
            web_search: None,
            chatgpt_mode: None,
            user: None,
            tools: None,
            tool_choice: None,
            stream_options: None,
        };
        let (system, convo) = split_prompt(&req);
        assert_eq!(system.trim(), "You are a pirate.");
        assert!(convo.contains("Hello."));
        assert!(!convo.contains("You are a pirate."));
    }

    #[test]
    fn split_prompt_empty_system_when_no_system_message() {
        let req = OpenAIRequest {
            model: "test".to_owned(),
            messages: vec![Message {
                role: "user".to_owned(),
                content: Some(Value::String("Hi.".to_owned())),
                tool_calls: None,
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            }],
            stream: None,
            web_search: None,
            chatgpt_mode: None,
            user: None,
            tools: None,
            tool_choice: None,
            stream_options: None,
        };
        let (system, convo) = split_prompt(&req);
        assert!(system.is_empty());
        assert!(convo.contains("Hi."));
    }

    #[test]
    fn split_prompt_appends_tool_definitions_to_system() {
        let req = OpenAIRequest {
            model: "test".to_owned(),
            messages: vec![Message {
                role: "user".to_owned(),
                content: Some(Value::String("go".to_owned())),
                tool_calls: None,
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            }],
            stream: None,
            web_search: None,
            chatgpt_mode: None,
            user: None,
            tools: Some(vec![FunctionToolDefinition {
                tool_type: "function".to_owned(),
                function: Some(FunctionToolSpec {
                    name: "get_weather".to_owned(),
                    description: None,
                    parameters: None,
                    strict: None,
                }),
                name: None,
                description: None,
            }]),
            tool_choice: None,
            stream_options: None,
        };
        let (system, _convo) = split_prompt(&req);
        assert!(system.contains("get_weather"));
    }

    #[test]
    fn tool_response_small_content_stays_exact_and_resolves_call_name() {
        let req = OpenAIRequest {
            model: "test".to_owned(),
            messages: vec![
                Message {
                    role: "assistant".to_owned(),
                    content: Some(Value::String("calling tool".to_owned())),
                    tool_calls: Some(vec![MessageToolCall {
                        id: "call_1".to_owned(),
                        tool_type: "function".to_owned(),
                        function: ToolCallFunction {
                            name: "read_file".to_owned(),
                            arguments: "{\"path\":\"Cargo.toml\"}".to_owned(),
                        },
                    }]),
                    tool_call_id: None,
                    name: None,
                    reasoning_content: None,
                },
                Message {
                    role: "tool".to_owned(),
                    content: Some(Value::String("ok".to_owned())),
                    tool_calls: None,
                    tool_call_id: Some("call_1".to_owned()),
                    name: None,
                    reasoning_content: None,
                },
            ],
            stream: None,
            web_search: None,
            chatgpt_mode: None,
            user: None,
            tools: None,
            tool_choice: None,
            stream_options: None,
        };

        let (_system, convo) = split_prompt(&req);
        assert!(convo.contains("Tool Response (read_file): ok"));
    }

    #[test]
    fn tool_response_json_compaction_labels_excerpt_and_keeps_order() {
        let raw_json = json!({
            "items": (0..600)
                .map(|index| json!({"id": index, "value": format!("entry-{index}-{}", "x".repeat(12))}))
                .collect::<Vec<_>>()
        });
        let req = OpenAIRequest {
            model: "test".to_owned(),
            messages: vec![
                Message {
                    role: "user".to_owned(),
                    content: Some(Value::String("inspect".to_owned())),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                    reasoning_content: None,
                },
                Message {
                    role: "assistant".to_owned(),
                    content: Some(Value::String("running tool".to_owned())),
                    tool_calls: Some(vec![MessageToolCall {
                        id: "call_2".to_owned(),
                        tool_type: "function".to_owned(),
                        function: ToolCallFunction {
                            name: "query".to_owned(),
                            arguments: "{}".to_owned(),
                        },
                    }]),
                    tool_call_id: None,
                    name: None,
                    reasoning_content: None,
                },
                Message {
                    role: "tool".to_owned(),
                    content: Some(raw_json.clone()),
                    tool_calls: None,
                    tool_call_id: Some("call_2".to_owned()),
                    name: Some("query".to_owned()),
                    reasoning_content: None,
                },
            ],
            stream: None,
            web_search: None,
            chatgpt_mode: None,
            user: None,
            tools: None,
            tool_choice: None,
            stream_options: None,
        };

        let prompt = build_prompt(&req);
        assert!(
            prompt.find("User: inspect").unwrap() < prompt.find("Assistant: running tool").unwrap()
        );
        assert!(
            prompt.find("Assistant: running tool").unwrap()
                < prompt.find("Tool Response (query):").unwrap()
        );
        assert!(prompt.contains("[Tool Response JSON compacted; original"));
        assert!(prompt.contains("Excerpt is not complete JSON."));
        assert!(prompt.len() < raw_json.to_string().len() + 256);
    }

    #[test]
    fn tool_response_test_output_compaction_keeps_failure_lines() {
        let mut output = String::from("running 120 tests\n");
        for _ in 0..220 {
            output
                .push_str("test module::passing_case_with_verbose_name_and_fixture_setup ... ok\n");
        }
        output.push_str("thread 'main' panicked at src/lib.rs:42: boom\n");
        output.push_str("failures:\n");
        output.push_str("test result: FAILED. 119 passed; 1 failed;\n");

        let compacted = compact_tool_response_content(&Some(Value::String(output)));
        assert!(compacted.contains("[Tool Response test output compacted; original"));
        assert!(compacted.contains("panicked"));
        assert!(compacted.contains("FAILED"));
        assert!(!compacted.contains("passing_case ... ok\ntest module::passing_case ... ok\ntest module::passing_case ... ok\ntest module::passing_case ... ok\ntest module::passing_case ... ok\ntest module::passing_case ... ok\ntest module::passing_case ... ok\ntest module::passing_case ... ok\ntest module::passing_case ... ok\ntest module::passing_case ... ok"));
    }

    #[test]
    fn tool_response_log_compaction_keeps_error_lines_in_order() {
        let output = [
            "2026-08-11T09:00:00Z INFO booting",
            "2026-08-11T09:00:01Z INFO warming cache",
            "2026-08-11T09:00:02Z WARN slow response",
            "2026-08-11T09:00:03Z INFO heartbeat",
            "2026-08-11T09:00:04Z ERROR upstream timeout",
            "2026-08-11T09:00:05Z INFO retrying",
            "2026-08-11T09:00:06Z ERROR retry failed",
            "2026-08-11T09:00:07Z INFO done",
        ]
        .join("\n")
        .repeat(120);

        let compacted = compact_tool_response_content(&Some(Value::String(output)));
        let warn_index = compacted.find("WARN slow response").unwrap();
        let error_one_index = compacted.find("ERROR upstream timeout").unwrap();
        let error_two_index = compacted.rfind("ERROR retry failed").unwrap();
        assert!(compacted.contains("[Tool Response log output compacted; original"));
        assert!(warn_index < error_one_index);
        assert!(error_one_index < error_two_index);
    }

    #[test]
    fn tool_response_media_payload_is_omitted() {
        let compacted = compact_tool_response_content(&Some(json!({
            "type": "image_url",
            "image_url": "data:image/png;base64,AAAAABBBBBCCCCCDDDD"
        })));
        assert!(compacted.contains("[Tool Response media payload omitted]"));
        assert!(compacted.contains("payload omitted"));
    }

    #[test]
    fn compact_structured_prompt_preserves_first_and_latest_blocks() {
        let prompt = [
            "User: system bootstrap and repo contract",
            "Assistant: acknowledged",
            "User: very old context that can be omitted safely",
            "Assistant: old answer",
            "User: latest request with exact task",
            "Assistant: latest answer",
        ]
        .join("\n\n");

        let compacted = compact_structured_prompt(
            &prompt,
            StructuredPromptCompactionOptions {
                max_chars: 145,
                preserve_first_block: true,
            },
        );

        assert!(compacted.truncated);
        assert!(compacted.text.starts_with("User: system bootstrap"));
        assert!(compacted
            .text
            .contains("User: latest request with exact task"));
        assert!(!compacted.text.contains("very old context"));
    }

    #[test]
    fn compact_structured_prompt_removes_duplicate_blocks_before_omitting_unique() {
        let prompt = [
            "User: repeated turn",
            "Assistant: same output",
            "Assistant: same output",
            "Assistant: same output",
            "User: final turn",
        ]
        .join("\n\n");

        let compacted = compact_structured_prompt(
            &prompt,
            StructuredPromptCompactionOptions {
                max_chars: 70,
                preserve_first_block: true,
            },
        );

        assert!(compacted.truncated);
        assert!(compacted.removed_duplicate_blocks >= 1);
        assert!(compacted.text.contains("User: final turn"));
    }

    #[test]
    fn compact_structured_prompt_uses_head_tail_for_single_long_block() {
        let prompt = format!("User: {}", "A".repeat(300));
        let compacted = compact_structured_prompt(
            &prompt,
            StructuredPromptCompactionOptions {
                max_chars: 80,
                preserve_first_block: true,
            },
        );

        assert!(compacted.truncated);
        assert_eq!(compacted.mode, "head-tail");
        assert!(compacted.compacted_chars <= 80);
        assert!(!compacted.text.is_empty());
    }

    #[test]
    fn compact_structured_prompt_does_not_middle_trim_tool_call_block() {
        let prompt = [
            "User: bootstrap",
            "Assistant: latest tool run\n<tool_call>\n{\"name\":\"read_file\",\"arguments\":{\"path\":\"Cargo.toml\"}}\n</tool_call>",
        ]
        .join("\n\n");

        let compacted = compact_structured_prompt(
            &prompt,
            StructuredPromptCompactionOptions {
                max_chars: 60,
                preserve_first_block: true,
            },
        );

        assert!(compacted.truncated);
        let has_open = compacted.text.contains("<tool_call>");
        let has_close = compacted.text.contains("</tool_call>");
        assert_eq!(has_open, has_close);
    }

    #[test]
    fn preflight_preserves_assistant_tool_call_and_tool_result_group() {
        let request = OpenAIRequest {
            model: "test".to_owned(),
            messages: vec![
                Message {
                    role: "system".to_owned(),
                    content: Some(Value::String("Keep exact tools.".to_owned())),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                    reasoning_content: None,
                },
                Message {
                    role: "user".to_owned(),
                    content: Some(Value::String("older".repeat(30))),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                    reasoning_content: None,
                },
                Message {
                    role: "assistant".to_owned(),
                    content: Some(Value::String("calling tool".to_owned())),
                    tool_calls: Some(vec![MessageToolCall {
                        id: "call_1".to_owned(),
                        tool_type: "function".to_owned(),
                        function: ToolCallFunction {
                            name: "read_file".to_owned(),
                            arguments: "{\"path\":\"Cargo.toml\"}".to_owned(),
                        },
                    }]),
                    tool_call_id: None,
                    name: None,
                    reasoning_content: None,
                },
                Message {
                    role: "tool".to_owned(),
                    content: Some(Value::String("ok".to_owned())),
                    tool_calls: None,
                    tool_call_id: Some("call_1".to_owned()),
                    name: Some("read_file".to_owned()),
                    reasoning_content: None,
                },
            ],
            stream: None,
            web_search: None,
            chatgpt_mode: None,
            user: None,
            tools: None,
            tool_choice: None,
            stream_options: None,
        };
        let expected_kept = request_with_messages(
            &request,
            vec![
                request.messages[0].clone(),
                request.messages[2].clone(),
                request.messages[3].clone(),
            ],
        );
        let expected_budget = estimate_tokens(&build_prompt(&expected_kept));

        let preflight = preflight_request_to_budget(
            &request,
            &PromptPreflightOptions {
                max_prompt_tokens: Some(expected_budget),
                extra_system_instructions: None,
                dedup_system_blocks: false,
                structured_compaction_max_chars: None,
            },
            estimate_tokens,
        )
        .expect("preflight");

        assert_eq!(preflight.request.messages.len(), 3);
        assert_eq!(preflight.request.messages[1].role, "assistant");
        assert_eq!(preflight.request.messages[2].role, "tool");
        assert!(preflight.truncated);
    }

    #[test]
    fn preflight_rejects_over_budget_system_and_tools() {
        let request = OpenAIRequest {
            model: "test".to_owned(),
            messages: vec![Message {
                role: "system".to_owned(),
                content: Some(Value::String("A".repeat(400))),
                tool_calls: None,
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            }],
            stream: None,
            web_search: None,
            chatgpt_mode: None,
            user: None,
            tools: Some(vec![FunctionToolDefinition {
                tool_type: "function".to_owned(),
                function: Some(FunctionToolSpec {
                    name: "read_file".to_owned(),
                    description: Some("B".repeat(400)),
                    parameters: Some(
                        json!({"type":"object","properties":{"path":{"type":"string"}}}),
                    ),
                    strict: None,
                }),
                name: None,
                description: None,
            }]),
            tool_choice: None,
            stream_options: None,
        };

        let error = preflight_request_to_budget(
            &request,
            &PromptPreflightOptions {
                max_prompt_tokens: Some(20),
                extra_system_instructions: None,
                dedup_system_blocks: false,
                structured_compaction_max_chars: None,
            },
            estimate_tokens,
        )
        .expect_err("budget failure");

        assert!(error
            .to_string()
            .contains("prompt preflight exceeded budget with system/tool instructions only"));
    }

    #[test]
    fn preflight_dedups_system_blocks_only_when_enabled() {
        let request = OpenAIRequest {
            model: "test".to_owned(),
            messages: vec![
                Message {
                    role: "system".to_owned(),
                    content: Some(Value::String("Keep it terse.".to_owned())),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                    reasoning_content: None,
                },
                Message {
                    role: "system".to_owned(),
                    content: Some(Value::String("Keep   it terse.".to_owned())),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                    reasoning_content: None,
                },
            ],
            stream: None,
            web_search: None,
            chatgpt_mode: None,
            user: None,
            tools: None,
            tool_choice: None,
            stream_options: None,
        };

        let disabled = preflight_request_to_budget(
            &request,
            &PromptPreflightOptions::default(),
            estimate_tokens,
        )
        .expect("disabled");
        let enabled = preflight_request_to_budget(
            &request,
            &PromptPreflightOptions {
                max_prompt_tokens: None,
                extra_system_instructions: None,
                dedup_system_blocks: true,
                structured_compaction_max_chars: None,
            },
            estimate_tokens,
        )
        .expect("enabled");

        assert!(disabled.system_prompt.matches("Keep").count() >= 2);
        assert_eq!(enabled.system_prompt.matches("Keep").count(), 1);
    }

    #[test]
    fn preflight_structured_compaction_compacts_conversation_not_system() {
        let request = OpenAIRequest {
            model: "test".to_owned(),
            messages: vec![
                Message {
                    role: "system".to_owned(),
                    content: Some(Value::String("System line stays intact.".to_owned())),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                    reasoning_content: None,
                },
                Message {
                    role: "user".to_owned(),
                    content: Some(Value::String("old context ".repeat(12))),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                    reasoning_content: None,
                },
                Message {
                    role: "assistant".to_owned(),
                    content: Some(Value::String("older answer ".repeat(8))),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                    reasoning_content: None,
                },
                Message {
                    role: "user".to_owned(),
                    content: Some(Value::String("latest exact request".to_owned())),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                    reasoning_content: None,
                },
            ],
            stream: None,
            web_search: None,
            chatgpt_mode: None,
            user: None,
            tools: Some(vec![FunctionToolDefinition {
                tool_type: "function".to_owned(),
                function: Some(FunctionToolSpec {
                    name: "read_file".to_owned(),
                    description: Some("Reads file contents".to_owned()),
                    parameters: Some(
                        json!({"type":"object","properties":{"path":{"type":"string"}}}),
                    ),
                    strict: None,
                }),
                name: None,
                description: None,
            }]),
            tool_choice: None,
            stream_options: None,
        };

        let preflight = preflight_request_to_budget(
            &request,
            &PromptPreflightOptions {
                max_prompt_tokens: None,
                extra_system_instructions: None,
                dedup_system_blocks: false,
                structured_compaction_max_chars: Some(120),
            },
            estimate_tokens,
        )
        .expect("preflight");

        assert!(preflight
            .system_prompt
            .contains("System line stays intact."));
        assert!(preflight.system_prompt.contains("\"name\":\"read_file\""));
        assert!(preflight.conversation.chars().count() <= 120);
        assert!(preflight.flat_prompt.contains("System line stays intact."));
        assert!(preflight.flat_prompt.contains(&preflight.conversation));
    }

    #[test]
    fn build_prompt_concatenates_system_and_convo() {
        let req = OpenAIRequest {
            model: "test".to_owned(),
            messages: vec![
                Message {
                    role: "system".to_owned(),
                    content: Some(Value::String("Be terse.".to_owned())),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                    reasoning_content: None,
                },
                Message {
                    role: "user".to_owned(),
                    content: Some(Value::String("Summarize.".to_owned())),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                    reasoning_content: None,
                },
            ],
            stream: None,
            web_search: None,
            chatgpt_mode: None,
            user: None,
            tools: None,
            tool_choice: None,
            stream_options: None,
        };
        let flat = build_prompt(&req);
        assert!(flat.contains("Be terse."));
        assert!(flat.contains("Summarize."));
    }

    #[test]
    fn build_prompt_returns_just_convo_when_no_system() {
        let req = OpenAIRequest {
            model: "test".to_owned(),
            messages: vec![Message {
                role: "user".to_owned(),
                content: Some(Value::String("Do it.".to_owned())),
                tool_calls: None,
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            }],
            stream: None,
            web_search: None,
            chatgpt_mode: None,
            user: None,
            tools: None,
            tool_choice: None,
            stream_options: None,
        };
        let flat = build_prompt(&req);
        // No "system\n\n" prefix bloating the prompt
        assert!(!flat.starts_with('\n'));
        assert!(flat.contains("Do it."));
    }

    #[test]
    fn loopback_guard_rejects_non_loopback_without_key() {
        assert!(enforce_loopback_guard("127.0.0.1", None).is_ok());
        assert!(enforce_loopback_guard("0.0.0.0", None).is_err());
        assert!(enforce_loopback_guard("0.0.0.0", Some("")).is_err());
        assert!(enforce_loopback_guard("0.0.0.0", Some("secret")).is_ok());
    }

    #[test]
    fn estimate_tokens_rounds_up() {
        // 7 chars / 3.5 = 2.0 exactly
        assert_eq!(estimate_tokens("1234567"), 2);
        // 8 chars / 3.5 = 2.28… → ceil = 3
        assert_eq!(estimate_tokens("12345678"), 3);
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn compact_prompt_removes_formatting_only() {
        let input = "System: keep exact text  \n\n\nUser: hello\n\n\n\nAssistant: ok  ";
        assert_eq!(
            compact_prompt(input),
            "System: keep exact text\n\nUser: hello\n\nAssistant: ok"
        );
    }

    #[test]
    fn usage_from_text_sums_correctly() {
        let u = usage_from_text("1234567", "12345678", false);
        assert_eq!(u.prompt_tokens, 2);
        assert_eq!(u.completion_tokens, 3);
        assert_eq!(u.total_tokens, 5);
        assert!(u.prompt_tokens_details.is_none());
    }

    #[test]
    fn usage_from_text_includes_cached_tokens_detail_when_requested() {
        let u = usage_from_text("hello", "world", true);
        let detail = u.prompt_tokens_details.expect("detail present");
        assert_eq!(detail["cached_tokens"], 0);
    }

    #[test]
    fn content_to_text_handles_none() {
        assert_eq!(content_to_text(&None), "");
    }

    #[test]
    fn content_to_text_handles_null_value() {
        assert_eq!(content_to_text(&Some(Value::Null)), "");
    }

    #[test]
    fn content_to_text_handles_plain_string() {
        let s = content_to_text(&Some(Value::String("hello".to_owned())));
        assert_eq!(s, "hello");
    }

    #[test]
    fn content_to_text_joins_text_parts_array() {
        let parts = Value::Array(vec![
            json!({ "type": "text", "text": "first" }),
            json!({ "type": "text", "text": "second" }),
        ]);
        let result = content_to_text(&Some(parts));
        assert!(result.contains("first"));
        assert!(result.contains("second"));
    }

    #[test]
    fn content_to_text_handles_mixed_array_items() {
        // Non-object items fall back to JSON serialisation
        let parts = Value::Array(vec![
            json!({ "type": "text", "text": "readable" }),
            json!(42),
        ]);
        let result = content_to_text(&Some(parts));
        assert!(result.contains("readable"));
        assert!(result.contains("42"));
    }

    #[test]
    fn robust_parse_json_strips_markdown_fence() {
        let raw = "```json\n{\"key\": \"value\"}\n```";
        let val = robust_parse_json(raw).expect("parsed");
        assert_eq!(val["key"], "value");
    }

    #[test]
    fn robust_parse_json_handles_prefix_junk() {
        let val = robust_parse_json("Here is the JSON: {\"x\": 1}").expect("parsed");
        assert_eq!(val["x"], 1);
    }

    #[test]
    fn robust_parse_json_returns_none_for_no_json() {
        assert!(robust_parse_json("no json here at all").is_none());
        assert!(robust_parse_json("").is_none());
    }

    #[test]
    fn robust_parse_json_balances_truncated_object() {
        // Missing closing brace — balance_json should close it
        let val = robust_parse_json("{\"a\": 1, \"b\": 2").expect("parsed truncated");
        assert_eq!(val["a"], 1);
        assert_eq!(val["b"], 2);
    }

    #[test]
    fn robust_parse_json_parses_array() {
        let val = robust_parse_json("[1, 2, 3]").expect("parsed array");
        assert_eq!(val.as_array().unwrap().len(), 3);
    }

    #[test]
    fn flush_recovers_partial_open_tag_as_text() {
        let mut parser = StreamingToolParser::new();
        // Chunk ends mid-tag — not yet recognised as a tool call
        let _ = parser.feed("text before <tool");
        let flushed = parser.flush();
        // Should surface the buffered content rather than swallow it
        assert!(flushed.text.contains("tool") || flushed.text.contains("before"));
    }

    #[test]
    fn flush_emits_remaining_plain_text() {
        let mut parser = StreamingToolParser::new();
        let first = parser.feed("hello ");
        assert_eq!(first.text, "hello ");
        let flushed = parser.flush();
        // Buffer was already drained by feed; flush produces nothing new
        assert!(flushed.tool_calls.is_empty());
    }

    #[test]
    fn parser_handles_xml_parameter_style_tool_call() {
        let mut parser = StreamingToolParser::new();
        let result = parser.feed(
            r#"<tool_call name="read_file"><name>read_file</name><parameter name="path">Cargo.toml</parameter></tool_call>"#,
        );
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].name, "read_file");
        assert_eq!(
            result.tool_calls[0].arguments["path"],
            Value::String("Cargo.toml".to_owned())
        );
    }

    fn make_msg(role: &str, text: &str) -> Message {
        Message {
            role: role.to_owned(),
            content: Some(Value::String(text.to_owned())),
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning_content: None,
        }
    }

    #[test]
    fn split_prompt_multi_turn_conversation_shape() {
        let req = OpenAIRequest {
            model: "t".to_owned(),
            messages: vec![
                make_msg("system", "You are helpful."),
                make_msg("user", "Q1"),
                make_msg("assistant", "A1"),
                make_msg("user", "Q2"),
            ],
            stream: None,
            web_search: None,
            chatgpt_mode: None,
            user: None,
            tools: None,
            tool_choice: None,
            stream_options: None,
        };
        let (sys, convo) = split_prompt(&req);
        assert!(sys.contains("You are helpful."));
        assert!(!convo.contains("You are helpful."));
        assert!(convo.contains("User: Q1"));
        assert!(convo.contains("Assistant: A1"));
        assert!(convo.contains("User: Q2"));
    }

    #[test]
    fn split_prompt_tool_response_uses_name_field() {
        let req = OpenAIRequest {
            model: "t".to_owned(),
            messages: vec![
                make_msg("user", "use tool"),
                Message {
                    role: "tool".to_owned(),
                    content: Some(Value::String("42".to_owned())),
                    tool_calls: None,
                    tool_call_id: Some("call_1".to_owned()),
                    name: Some("calculator".to_owned()),
                    reasoning_content: None,
                },
            ],
            stream: None,
            web_search: None,
            chatgpt_mode: None,
            user: None,
            tools: None,
            tool_choice: None,
            stream_options: None,
        };
        let (_, convo) = split_prompt(&req);
        assert!(convo.contains("Tool Response (calculator)"));
        assert!(convo.contains("42"));
    }

    #[test]
    fn split_prompt_assistant_reasoning_content_wrapped_in_think_tags() {
        let req = OpenAIRequest {
            model: "t".to_owned(),
            messages: vec![Message {
                role: "assistant".to_owned(),
                content: Some(Value::String("final answer".to_owned())),
                tool_calls: None,
                tool_call_id: None,
                name: None,
                reasoning_content: Some("step by step".to_owned()),
            }],
            stream: None,
            web_search: None,
            chatgpt_mode: None,
            user: None,
            tools: None,
            tool_choice: None,
            stream_options: None,
        };
        let (_, convo) = split_prompt(&req);
        assert!(convo.contains("<think>"));
        assert!(convo.contains("step by step"));
        assert!(convo.contains("</think>"));
        assert!(convo.contains("final answer"));
    }

    #[test]
    fn split_prompt_assistant_with_tool_calls_serialises_them() {
        let req = OpenAIRequest {
            model: "t".to_owned(),
            messages: vec![Message {
                role: "assistant".to_owned(),
                content: Some(Value::String(String::new())),
                tool_calls: Some(vec![MessageToolCall {
                    id: "call_1".to_owned(),
                    tool_type: "function".to_owned(),
                    function: ToolCallFunction {
                        name: "lookup".to_owned(),
                        arguments: r#"{"id":7}"#.to_owned(),
                    },
                }]),
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            }],
            stream: None,
            web_search: None,
            chatgpt_mode: None,
            user: None,
            tools: None,
            tool_choice: None,
            stream_options: None,
        };
        let (_, convo) = split_prompt(&req);
        assert!(convo.contains("<tool_call>"));
        assert!(convo.contains("lookup"));
        assert!(convo.contains("</tool_call>"));
    }

    #[test]
    fn split_prompt_multiple_system_messages_concatenated() {
        let req = OpenAIRequest {
            model: "t".to_owned(),
            messages: vec![
                make_msg("system", "Part A."),
                make_msg("system", "Part B."),
                make_msg("user", "Hi"),
            ],
            stream: None,
            web_search: None,
            chatgpt_mode: None,
            user: None,
            tools: None,
            tool_choice: None,
            stream_options: None,
        };
        let (sys, _) = split_prompt(&req);
        assert!(sys.contains("Part A."));
        assert!(sys.contains("Part B."));
    }
}
