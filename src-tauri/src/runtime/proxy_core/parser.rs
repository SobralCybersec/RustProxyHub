use super::*;

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
static DSML_BLOCK_OPEN_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)<｜DSML｜(?:tool_calls|function_calls)>").expect("valid DSML block regex")
});
static DSML_BLOCK_CLOSE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)</｜DSML｜(?:tool_calls|function_calls)>")
        .expect("valid DSML block close regex")
});
static DSML_INVOKE_OPEN_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)<｜DSML｜invoke\s+name\s*=\s*["']([^"']+)["']\s*>"#)
        .expect("valid DSML invoke regex")
});
static DSML_INVOKE_CLOSE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)</｜DSML｜invoke>").expect("valid DSML invoke close regex"));
static DSML_PARAMETER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?is)<｜DSML｜parameter\s+name\s*=\s*["']([^"']+)["'](?:\s+string\s*=\s*["'](true|false)["'])?\s*>(.*?)</｜DSML｜parameter>"#,
    )
    .expect("valid DSML parameter regex")
});

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
                let dsml_open = find_dsml_block_open(&self.buffer);
                let json_open = find_bare_json_start(&self.buffer);
                /* whichever structure starts first wins; a <tool_call> tag's own '<'
                precedes its inner '{', so tags are still preferred when present */
                let markup_open = match (tag_open, dsml_open) {
                    (Some(tag), Some(dsml)) if dsml.0 < tag.0 => Some(dsml),
                    (Some(tag), _) => Some(tag),
                    (None, Some(dsml)) => Some(dsml),
                    (None, None) => None,
                };
                let tag_first = match (&markup_open, json_open) {
                    (Some((tag_start, _, _)), Some(js)) => *tag_start <= js,
                    (Some(_), None) => true,
                    _ => false,
                };

                if tag_first {
                    let (start, end, open_tag) =
                        markup_open.expect("tag_first is only true when a tag matched");
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
                        find_partial_markup_open_index(&self.buffer).unwrap_or(self.buffer.len());
                    result.text.push_str(&self.buffer[..flush_index]);
                    self.buffer.drain(..flush_index);
                    break;
                }
            } else if let Some(end_idx) =
                find_markup_close_index(&self.buffer, &self.current_open_tag)
            {
                let content = self.buffer[..end_idx].to_owned();
                let close_len = markup_close_len(&self.current_open_tag);
                self.buffer.drain(..end_idx + close_len);
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
            let tool_calls = self.parse_markup_content(self.buffer.trim());
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
        let tool_calls = self.parse_markup_content(content.trim());
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

    fn parse_markup_content(&self, content: &str) -> Vec<ParsedToolCall> {
        if self.current_open_tag.to_ascii_lowercase().contains("dsml") {
            parse_dsml_tool_calls(content)
        } else {
            self.parse_tool_content(content)
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
be mistaken for a call. Browser adapters separately normalize provider-specific
command objects because their tool name is supplied by the request schema. */
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

fn find_dsml_block_open(buffer: &str) -> Option<(usize, usize, String)> {
    DSML_BLOCK_OPEN_RE
        .find(buffer)
        .map(|matched| (matched.start(), matched.end(), matched.as_str().to_owned()))
}

fn find_markup_close_index(buffer: &str, open_tag: &str) -> Option<usize> {
    if open_tag.to_ascii_lowercase().contains("dsml") {
        DSML_BLOCK_CLOSE_RE
            .find(buffer)
            .map(|matched| matched.start())
    } else {
        find_tool_close_index(buffer)
    }
}

fn markup_close_len(open_tag: &str) -> usize {
    if open_tag.to_ascii_lowercase().contains("dsml") {
        if open_tag.to_ascii_lowercase().contains("function_calls") {
            "</｜DSML｜function_calls>".len()
        } else {
            "</｜DSML｜tool_calls>".len()
        }
    } else {
        "</tool_call>".len()
    }
}

fn parse_dsml_tool_calls(content: &str) -> Vec<ParsedToolCall> {
    let mut calls = Vec::new();
    let mut offset = 0usize;
    while let Some(open_rel) = DSML_INVOKE_OPEN_RE.find(&content[offset..]) {
        let open_start = offset + open_rel.start();
        let body_start = offset + open_rel.end();
        let Some(close_rel) = DSML_INVOKE_CLOSE_RE.find(&content[body_start..]) else {
            break;
        };
        let body_end = body_start + close_rel.start();
        let Some(name) = DSML_INVOKE_OPEN_RE
            .captures(&content[open_start..body_start])
            .and_then(|captures| captures.get(1))
            .map(|value| value.as_str().to_owned())
        else {
            offset = body_start + close_rel.end();
            continue;
        };
        let mut args = Map::new();
        for parameter in DSML_PARAMETER_RE.captures_iter(&content[body_start..body_end]) {
            let Some(key) = parameter.get(1).map(|value| value.as_str()) else {
                continue;
            };
            let Some(raw) = parameter.get(3).map(|value| value.as_str().trim()) else {
                continue;
            };
            let value = if parameter.get(2).map(|value| value.as_str()) == Some("true") {
                Value::String(raw.to_owned())
            } else {
                serde_json::from_str(raw)
                    .or_else(|_| robust_parse_json(raw).ok_or(()))
                    .unwrap_or_else(|_| Value::String(raw.to_owned()))
            };
            args.insert(key.to_owned(), value);
        }
        calls.push(ParsedToolCall {
            id: format!("call_{}", Uuid::new_v4()),
            name,
            arguments: Value::Object(args),
        });
        offset = body_start + close_rel.end();
    }
    calls
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

fn find_partial_markup_open_index(buffer: &str) -> Option<usize> {
    find_partial_tool_open_index(buffer).or_else(|| {
        let prefixes = [
            "<｜DSML｜tool_calls>",
            "<｜DSML｜function_calls>",
            "<｜DSML｜invoke",
        ];
        buffer.char_indices().rev().find_map(|(idx, _)| {
            let tail = &buffer[idx..];
            prefixes
                .iter()
                .any(|prefix| tail.len() < prefix.len() && prefix.starts_with(tail))
                .then_some(idx)
        })
    })
}
