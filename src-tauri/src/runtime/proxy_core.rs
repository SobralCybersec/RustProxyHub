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

mod compaction;
pub use compaction::*;

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

    let flat_tools: Vec<Value> = tools
        .iter()
        .map(|tool| {
            let mut obj = Map::new();
            if let Some(ref spec) = tool.function {
                if !spec.name.is_empty() {
                    obj.insert("name".to_owned(), Value::String(spec.name.clone()));
                }
                if let Some(ref desc) = spec.description {
                    obj.insert("description".to_owned(), Value::String(desc.clone()));
                }
                if let Some(ref params) = spec.parameters {
                    obj.insert("parameters".to_owned(), params.clone());
                }
                if let Some(strict) = spec.strict {
                    obj.insert("strict".to_owned(), Value::Bool(strict));
                }
            } else if let Some(ref name) = tool.name {
                obj.insert("name".to_owned(), Value::String(name.clone()));
                if let Some(ref desc) = tool.description {
                    obj.insert("description".to_owned(), Value::String(desc.clone()));
                }
            }
            Value::Object(obj)
        })
        .collect();
    let tools_json = serde_json::to_string(&flat_tools).unwrap_or_else(|_| "[]".to_owned());
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

mod parser;
pub use parser::*;

#[cfg(test)]
#[path = "proxy_core/tests.rs"]
mod tests;
