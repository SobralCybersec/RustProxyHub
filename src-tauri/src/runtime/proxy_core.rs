use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use url::Url;
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FunctionToolSpec {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub parameters: Option<Value>,
    #[serde(default)]
    pub strict: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FunctionToolDefinition {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionToolSpec,
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

pub fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
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

    let tools_json = serde_json::to_string_pretty(tools).unwrap_or_else(|_| "[]".to_owned());
    let mut out = format!(
        "\n\n# TOOLS AVAILABLE\nYou have access to the following tools:\n{tools_json}\n\nThese tools are REAL and executable in this session.\nDo NOT claim you cannot access tools, that tools are only pasted text, or that tool execution is unavailable.\nIf the user asks to test, use, inspect, read, write, search, or run tools, you MUST answer by emitting tool calls.\n\n# TOOL CALLING FORMAT (MANDATORY)\nTo use a tool, you MUST output a JSON object wrapped EXACTLY in these tags:\n<tool_call>\n{{\"name\": \"tool_name\", \"arguments\": {{\"param_name\": \"value\"}}}}\n</tool_call>\n\nCRITICAL RULES:\n1. ONLY use the tags above for tool calling. NEVER output raw JSON without tags.\n2. You can call multiple tools by outputting multiple <tool_call> blocks consecutively.\n3. Do NOT output any other text after your <tool_call> blocks.\n4. The JSON inside the tags MUST be valid and include the \"arguments\" field.\n5. NEVER invent tool names. Only use tools from the list above.\n"
    );

    if let Some(name) = tool_choice
        .and_then(|value| value.get("function"))
        .and_then(|value| value.get("name"))
        .and_then(Value::as_str)
    {
        out.push_str(&format!(
            "\nCRITICAL: You MUST call the tool \"{name}\" in this response.\n"
        ));
    } else if tool_choice.and_then(Value::as_str) == Some("required") {
        out.push_str("\nCRITICAL: You MUST call one of the available tools in this response.\n");
    } else if tool_choice.and_then(Value::as_str) == Some("none") {
        out.push_str("\nCRITICAL: Do NOT call tools in this response.\n");
    }

    out
}

/// Returns `(system_prompt, conversation)` split so providers with a native system channel
/// (e.g. ChatGPT web) can deliver instructions through the correct lane instead of prepending
/// them to the user message where they may be ignored or filtered.
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
    if system_prompt.is_empty() {
        prompt
    } else {
        format!("{system_prompt}\n{prompt}")
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
            tools: Some(vec![FunctionToolDefinition {
                tool_type: "function".to_owned(),
                function: FunctionToolSpec {
                    name: "read_file".to_owned(),
                    description: None,
                    parameters: None,
                    strict: None,
                },
            }]),
            tool_choice: Some(json!("required")),
            stream_options: None,
        });

        assert!(prompt.contains("MUST call one of the available tools"));
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
            tools: Some(vec![FunctionToolDefinition {
                tool_type: "function".to_owned(),
                function: FunctionToolSpec {
                    name: "get_weather".to_owned(),
                    description: None,
                    parameters: None,
                    strict: None,
                },
            }]),
            tool_choice: None,
            stream_options: None,
        };
        let (system, _convo) = split_prompt(&req);
        assert!(system.contains("get_weather"));
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

    // ── estimate_tokens / usage_from_text ──────────────────────────────────

    #[test]
    fn estimate_tokens_rounds_up() {
        // 7 chars / 3.5 = 2.0 exactly
        assert_eq!(estimate_tokens("1234567"), 2);
        // 8 chars / 3.5 = 2.28… → ceil = 3
        assert_eq!(estimate_tokens("12345678"), 3);
        assert_eq!(estimate_tokens(""), 0);
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

    // ── content_to_text ────────────────────────────────────────────────────

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

    // ── robust_parse_json ──────────────────────────────────────────────────

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

    // ── StreamingToolParser flush ──────────────────────────────────────────

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

    // ── split_prompt multi-turn ────────────────────────────────────────────

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
            tools: None,
            tool_choice: None,
            stream_options: None,
        };
        let (sys, _) = split_prompt(&req);
        assert!(sys.contains("Part A."));
        assert!(sys.contains("Part B."));
    }
}
