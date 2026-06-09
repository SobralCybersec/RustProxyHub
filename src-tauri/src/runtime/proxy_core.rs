use anyhow::Result;
use async_trait::async_trait;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
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

#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    async fn list_models(&self) -> Result<Value>;
    async fn start_chat(&self, request: &OpenAIRequest) -> Result<()>;
    async fn continue_chat(&self, _request: &OpenAIRequest) -> Result<()> {
        Ok(())
    }
    async fn parse_chunk(&self, _chunk: &str) -> Result<()> {
        Ok(())
    }
    async fn stop_chat(&self, _payload: &Value) -> Result<()> {
        Ok(())
    }
    async fn upload(&self, _payload: &Value) -> Result<()> {
        Ok(())
    }
}

pub fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
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
        "\n\n# TOOLS AVAILABLE\nYou have access to the following tools:\n{tools_json}\n\n# TOOL CALLING FORMAT (MANDATORY)\nTo use a tool, you MUST output a JSON object wrapped EXACTLY in these tags:\n<tool_call>\n{{\"name\": \"tool_name\", \"arguments\": {{\"param_name\": \"value\"}}}}\n</tool_call>\n\nCRITICAL RULES:\n1. ONLY use the tags above for tool calling. NEVER output raw JSON without tags.\n2. You can call multiple tools by outputting multiple <tool_call> blocks consecutively.\n3. Do NOT output any other text after your <tool_call> blocks.\n4. The JSON inside the tags MUST be valid and include the \"arguments\" field.\n5. NEVER invent tool names. Only use tools from the list above.\n"
    );

    if let Some(name) = tool_choice
        .and_then(|value| value.get("function"))
        .and_then(|value| value.get("name"))
        .and_then(Value::as_str)
    {
        out.push_str(&format!(
            "\nCRITICAL: You MUST call the tool \"{name}\" in this response.\n"
        ));
    }

    out
}

pub fn build_prompt(request: &OpenAIRequest) -> String {
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

    let start = sanitized.find('{').or_else(|| sanitized.find('['))?;
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
                if let Some((start, end, open_tag)) = find_tool_open(&self.buffer) {
                    result.text.push_str(&self.buffer[..start]);
                    self.inside_tool = true;
                    self.current_open_tag = open_tag;
                    self.buffer = self.buffer[end..].to_owned();
                } else {
                    let flush_index =
                        find_partial_tool_open_index(&self.buffer).unwrap_or(self.buffer.len());
                    result.text.push_str(&self.buffer[..flush_index]);
                    self.buffer = self.buffer[flush_index..].to_owned();
                    break;
                }
            } else if let Some(end_idx) = find_tool_close_index(&self.buffer) {
                let content = self.buffer[..end_idx].to_owned();
                self.buffer = self.buffer[end_idx + "</tool_call>".len()..].to_owned();
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
            if let Some(tool_call) = self.try_parse_tool_content(self.buffer.trim()) {
                self.emitted_tool_calls += 1;
                result.tool_calls.push(tool_call);
            } else if !self.buffer.is_empty() {
                result.text.push_str(&self.current_open_tag);
                result.text.push_str(&self.buffer);
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
        if let Some(tool_call) = self.try_parse_tool_content(content.trim()) {
            self.emitted_tool_calls += 1;
            result.tool_calls.push(tool_call);
        } else {
            result.text.push_str(&self.current_open_tag);
            result.text.push_str(content);
            result.text.push_str("</tool_call>");
        }
    }

    fn try_parse_tool_content(&self, content: &str) -> Option<ParsedToolCall> {
        if content.is_empty() {
            return None;
        }

        if content.contains("<parameter") {
            let mut args = Map::new();
            for capture in TOOL_PARAM_RE.captures_iter(content) {
                let name = capture.get(1)?.as_str();
                let raw = capture.get(2)?.as_str().trim();
                let value = robust_parse_json(raw).unwrap_or_else(|| Value::String(raw.to_owned()));
                args.insert(name.to_owned(), value);
            }

            if args.is_empty() {
                return None;
            }

            let tool_name = extract_tool_name_from_markup(&self.current_open_tag, content)?;
            return Some(ParsedToolCall {
                id: format!("call_{}", Uuid::new_v4()),
                name: tool_name,
                arguments: Value::Object(args),
            });
        }

        let parsed = robust_parse_json(content)?;
        match parsed {
            Value::Array(values) => values.into_iter().find_map(parse_tool_call_value),
            other => parse_tool_call_value(other),
        }
    }
}

fn extract_tool_name_from_markup(open_tag: &str, content: &str) -> Option<String> {
    if let Some(captures) = Regex::new(r#"(?i)<tool_call\b[^>]*\bname\s*=\s*["']([^"']+)["']"#)
        .ok()?
        .captures(open_tag)
    {
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
        Value::String(text) => robust_parse_json(&text).unwrap_or(Value::String(text)),
        other => other,
    };

    Some(ParsedToolCall {
        id: format!("call_{}", Uuid::new_v4()),
        name,
        arguments: parsed_args,
    })
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
    buffer.to_lowercase().find("</tool_call>")
}

fn find_partial_tool_open_index(buffer: &str) -> Option<usize> {
    let prefix = "<tool_call";
    let lower = buffer.to_lowercase();
    for idx in (0..lower.len()).rev() {
        if lower.as_bytes()[idx] != b'<' {
            continue;
        }
        let tail = &lower[idx..];
        if prefix.starts_with(tail) {
            return Some(idx);
        }
    }
    None
}
