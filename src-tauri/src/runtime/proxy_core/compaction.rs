use super::*;

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

pub fn render_prompt_parts(system_prompt: &str, conversation: &str) -> String {
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

pub fn request_with_messages(template: &OpenAIRequest, messages: Vec<Message>) -> OpenAIRequest {
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
