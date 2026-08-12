fn browser_tool_system_instructions(body: &OpenAIRequest) -> Option<String> {
    body.tools.as_ref()?;

    let mut prefix = String::from(
        "BROWSER TOOL MODE\n\
These tools are real and executable by the client in this session.\n\
If user asks to test tools, inspect workspace/files, read code, write code, grep, glob, run commands, or understand repository state, do not answer in prose.\n\
You must respond with one or more <tool_call>...</tool_call> blocks only.\n\
Never print Kilo-style command objects such as {\"command\":\"...\",\"description\":\"...\",\"workdir\":\"...\"}; they are text, not executable calls.\n\
Never say tools are unavailable, pasted text, unsupported, or not accessible from this session.\n",
    );

    if body.tool_choice.as_ref().and_then(Value::as_str) == Some("required") {
        prefix.push_str(
            "tool_choice is required. You must call one or more tools before any normal text.\n",
        );
    }

    if let Some(name) = body
        .tool_choice
        .as_ref()
        .and_then(|value| value.get("function"))
        .and_then(|value| value.get("name"))
        .and_then(Value::as_str)
    {
        prefix.push_str(&format!(
            "tool_choice targets function `{name}`. Call that function unless arguments cannot be inferred.\n"
        ));
    }

    Some(prefix.trim_end().to_owned())
}

fn clean_empty_tool_fence(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed
        .lines()
        .filter(|line| !line.trim().is_empty())
        .all(|line| matches!(line.trim(), "```" | "```json"))
    {
        String::new()
    } else {
        trimmed.to_owned()
    }
}

fn parse_browser_output(body: &OpenAIRequest, text: &str) -> ParsedBrowserOutput {
    let Some(_) = body.tools.as_ref() else {
        return ParsedBrowserOutput {
            text: text.to_owned(),
            tool_calls: Vec::new(),
        };
    };

    let mut parser = StreamingToolParser::new();
    let mut parsed = parser.feed(text);
    let flush = parser.flush();
    parsed.text.push_str(&flush.text);
    parsed.tool_calls.extend(flush.tool_calls);

    if parsed.tool_calls.is_empty() {
        let (cleaned, leaked) = extract_tool_calls_from_text(&parsed.text);
        if !leaked.is_empty() {
            parsed.text = clean_empty_tool_fence(&cleaned);
            parsed.tool_calls = leaked;
        }
    }

    if parsed.tool_calls.is_empty() {
        if let Some((cleaned, leaked)) = extract_command_object_tool_calls(
            &parsed.text,
            body.tools.as_deref().unwrap_or_default(),
        ) {
            parsed.text = cleaned;
            parsed.tool_calls = leaked;
        }
    }

    ParsedBrowserOutput {
        text: clean_empty_tool_fence(&parsed.text),
        tool_calls: parsed
            .tool_calls
            .into_iter()
            .map(tool_call_from_parsed)
            .collect(),
    }
}

fn extract_command_object_tool_calls(
    text: &str,
    tools: &[FunctionToolDefinition],
) -> Option<(String, Vec<crate::proxy_core::ParsedToolCall>)> {
    let tool_name = tools
        .iter()
        .find(|tool| {
            let name = tool.tool_name().unwrap_or_default().to_ascii_lowercase();
            let has_command_parameter = tool
                .function
                .as_ref()
                .and_then(|function| function.parameters.as_ref())
                .and_then(|parameters| parameters.get("properties"))
                .and_then(Value::as_object)
                .is_some_and(|properties| properties.contains_key("command"));
            has_command_parameter
                || name.contains("command")
                || name.contains("shell")
                || name.contains("terminal")
                || name.contains("exec")
                || name == "bash"
        })
        .and_then(FunctionToolDefinition::tool_name)
        .or_else(|| {
            (tools.len() == 1)
                .then(|| tools.first().and_then(FunctionToolDefinition::tool_name))
                .flatten()
        })?
        .to_owned();

    let mut rest = text.trim();
    let mut calls = Vec::new();
    while !rest.is_empty() {
        let mut values = serde_json::Deserializer::from_str(rest).into_iter::<Value>();
        let parsed = values.next()?.ok()?;
        let consumed = values.byte_offset();
        let object = parsed.as_object()?;
        object.get("command").and_then(Value::as_str)?;
        calls.push(crate::proxy_core::ParsedToolCall {
            id: format!("call_{}", Uuid::new_v4()),
            name: tool_name.clone(),
            arguments: parsed,
        });
        rest = rest.get(consumed..)?.trim_start();
    }

    (!calls.is_empty()).then(|| (String::new(), calls))
}

fn tool_call_from_parsed(parsed: crate::proxy_core::ParsedToolCall) -> MessageToolCall {
    MessageToolCall {
        id: parsed.id,
        tool_type: "function".to_owned(),
        function: ToolCallFunction {
            name: parsed.name,
            arguments: parsed.arguments.to_string(),
        },
    }
}

fn coerce_agent_model(kind: BrowserProviderKind, requested: &str) -> String {
    let trimmed = requested.trim();
    if trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("claude")
        || trimmed.to_ascii_lowercase().starts_with("claude-")
        || trimmed.to_ascii_lowercase().starts_with("anthropic.")
    {
        kind.default_model().to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn openai_responses_to_request(state: &AppState, body: &Value) -> Result<OpenAIRequest> {
    let model = coerce_agent_model(
        state.config.kind,
        body.get("model")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    let mut messages = Vec::new();
    if let Some(instructions) = body.get("instructions") {
        let text = value_to_text(instructions);
        if !text.is_empty() {
            messages.push(simple_message("system", text));
        }
    }
    messages.extend(response_input_to_messages(body.get("input")));

    Ok(OpenAIRequest {
        model,
        messages,
        stream: body.get("stream").and_then(Value::as_bool),
        web_search: body.get("web_search").and_then(Value::as_bool),
        chatgpt_mode: body
            .get("chatgpt_mode")
            .and_then(Value::as_str)
            .map(str::to_owned),
        user: body.get("user").and_then(Value::as_str).map(str::to_owned),
        tools: openai_tools_from_value(body.get("tools")),
        tool_choice: body.get("tool_choice").cloned(),
        stream_options: None,
    })
}

fn anthropic_to_openai_request(state: &AppState, body: &Value) -> Result<OpenAIRequest> {
    let model = coerce_agent_model(
        state.config.kind,
        body.get("model")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    let mut messages = Vec::new();

    if let Some(system) = body.get("system") {
        let text = value_to_text(system);
        if !text.is_empty() {
            messages.push(simple_message("system", text));
        }
    }

    let anthropic_messages = body
        .get("messages")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("Anthropic messages array is required"))?;
    for message in anthropic_messages {
        messages.extend(anthropic_message_to_openai_messages(message));
    }

    let tool_choice = anthropic_tool_choice_to_openai(body.get("tool_choice"));

    Ok(OpenAIRequest {
        model,
        messages,
        stream: body.get("stream").and_then(Value::as_bool),
        web_search: body.get("web_search").and_then(Value::as_bool),
        chatgpt_mode: body
            .get("chatgpt_mode")
            .and_then(Value::as_str)
            .map(str::to_owned),
        user: body.get("user").and_then(Value::as_str).map(str::to_owned),
        tools: anthropic_tools_from_value(body.get("tools")),
        tool_choice,
        stream_options: None,
    })
}

fn anthropic_tool_choice_to_openai(value: Option<&Value>) -> Option<Value> {
    match value {
        Some(Value::Object(map)) if map.get("type").and_then(Value::as_str) == Some("tool") => {
            map.get("name").and_then(Value::as_str).map(|name| {
                json!({
                    "type": "function",
                    "function": { "name": name }
                })
            })
        }
        Some(Value::Object(map)) if map.get("type").and_then(Value::as_str) == Some("any") => {
            Some(json!("required"))
        }
        Some(Value::Object(map)) if map.get("type").and_then(Value::as_str) == Some("auto") => {
            Some(json!("auto"))
        }
        Some(Value::String(value)) if value == "any" => Some(json!("required")),
        Some(Value::String(value)) if value == "auto" || value == "none" => Some(json!(value)),
        Some(other) => Some(other.clone()),
        None => None,
    }
}

fn anthropic_message_to_openai_messages(message: &Value) -> Vec<Message> {
    let role = message
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or("user");
    let content = message.get("content").unwrap_or(&Value::Null);
    let Some(blocks) = content.as_array() else {
        return vec![simple_message(role, value_to_text(content))];
    };

    let mut text_parts = Vec::new();
    let mut tool_calls = Vec::new();
    let mut tool_results = Vec::new();

    for block in blocks {
        let block_type = block
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match block_type {
            "text" => {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    text_parts.push(text.to_owned());
                }
            }
            "tool_use" if role == "assistant" => {
                let Some(name) = block.get("name").and_then(Value::as_str) else {
                    continue;
                };
                let id = block
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("call_{}", Uuid::new_v4()));
                let input = block.get("input").cloned().unwrap_or_else(|| json!({}));
                tool_calls.push(MessageToolCall {
                    id,
                    tool_type: "function".to_owned(),
                    function: ToolCallFunction {
                        name: name.to_owned(),
                        arguments: input.to_string(),
                    },
                });
            }
            "tool_result" => {
                let Some(tool_call_id) = block
                    .get("tool_use_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                else {
                    continue;
                };
                tool_results.push(Message {
                    role: "tool".to_owned(),
                    content: block.get("content").cloned(),
                    tool_calls: None,
                    tool_call_id: Some(tool_call_id),
                    name: None,
                    reasoning_content: None,
                });
            }
            _ => {
                let text = value_to_text(block);
                if !text.is_empty() {
                    text_parts.push(text);
                }
            }
        }
    }

    let mut out = Vec::new();
    let text = text_parts.join("\n");
    if role == "assistant" && (!text.is_empty() || !tool_calls.is_empty()) {
        out.push(Message {
            role: "assistant".to_owned(),
            content: (!text.is_empty()).then_some(Value::String(text)),
            tool_calls: (!tool_calls.is_empty()).then_some(tool_calls),
            tool_call_id: None,
            name: None,
            reasoning_content: None,
        });
    } else if !text.is_empty() {
        out.push(simple_message(role, text));
    }
    out.extend(tool_results);

    if out.is_empty() {
        out.push(simple_message(role, value_to_text(content)));
    }
    out
}

fn response_input_to_messages(input: Option<&Value>) -> Vec<Message> {
    match input {
        None => Vec::new(),
        Some(Value::String(text)) => vec![simple_message("user", text.clone())],
        Some(Value::Array(items)) => items
            .iter()
            .flat_map(response_input_item_to_messages)
            .collect(),
        Some(value) => vec![simple_message("user", value_to_text(value))],
    }
}

fn response_input_item_to_messages(item: &Value) -> Vec<Message> {
    match item.get("type").and_then(Value::as_str) {
        Some("function_call") => {
            let Some(name) = item.get("name").and_then(Value::as_str) else {
                return Vec::new();
            };
            let id = item
                .get("call_id")
                .or_else(|| item.get("id"))
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| format!("call_{}", Uuid::new_v4()));
            let arguments = item
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}))
                .to_string();
            vec![Message {
                role: "assistant".to_owned(),
                content: None,
                tool_calls: Some(vec![MessageToolCall {
                    id,
                    tool_type: "function".to_owned(),
                    function: ToolCallFunction {
                        name: name.to_owned(),
                        arguments,
                    },
                }]),
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            }]
        }
        Some("function_call_output") => {
            let Some(tool_call_id) = item
                .get("call_id")
                .and_then(Value::as_str)
                .map(str::to_owned)
            else {
                return Vec::new();
            };
            vec![Message {
                role: "tool".to_owned(),
                content: item.get("output").cloned(),
                tool_calls: None,
                tool_call_id: Some(tool_call_id),
                name: None,
                reasoning_content: None,
            }]
        }
        _ => {
            let role = item.get("role").and_then(Value::as_str).unwrap_or("user");
            let text = item
                .get("content")
                .map(value_to_text)
                .unwrap_or_else(|| value_to_text(item));
            vec![simple_message(role, text)]
        }
    }
}

fn simple_message(role: &str, text: String) -> Message {
    Message {
        role: role.to_owned(),
        content: Some(Value::String(text)),
        tool_calls: None,
        tool_call_id: None,
        name: None,
        reasoning_content: None,
    }
}

fn openai_tools_from_value(value: Option<&Value>) -> Option<Vec<FunctionToolDefinition>> {
    let items = value?.as_array()?;
    let tools = items
        .iter()
        .filter_map(|item| {
            if item.get("function").is_some() {
                return serde_json::from_value(item.clone()).ok();
            }

            let tool_type = item
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("function");
            let name = item.get("name").and_then(Value::as_str)?.to_owned();
            let description = item
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if tool_type == "custom" {
                /* freeform custom tool: no JSON parameters, name at the top level */
                return Some(FunctionToolDefinition {
                    tool_type: "custom".to_owned(),
                    function: None,
                    name: Some(name),
                    description,
                });
            }
            Some(FunctionToolDefinition {
                tool_type: "function".to_owned(),
                function: Some(crate::proxy_core::FunctionToolSpec {
                    name,
                    description,
                    parameters: item.get("parameters").cloned(),
                    strict: item.get("strict").and_then(Value::as_bool),
                }),
                name: None,
                description: None,
            })
        })
        .collect::<Vec<_>>();
    Some(tools)
}

fn anthropic_tools_from_value(value: Option<&Value>) -> Option<Vec<FunctionToolDefinition>> {
    let items = value?.as_array()?;
    let tools = items
        .iter()
        .filter_map(|item| {
            let name = item.get("name").and_then(Value::as_str)?.to_owned();
            Some(FunctionToolDefinition {
                tool_type: "function".to_owned(),
                function: Some(crate::proxy_core::FunctionToolSpec {
                    name,
                    description: item
                        .get("description")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    parameters: item.get("input_schema").cloned(),
                    strict: None,
                }),
                name: None,
                description: None,
            })
        })
        .collect::<Vec<_>>();
    Some(tools)
}

fn value_to_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .map(|item| match item {
                Value::Object(map) => {
                    if let Some(text) = map.get("text").and_then(Value::as_str) {
                        text.to_owned()
                    } else if let Some(content) = map.get("content") {
                        value_to_text(content)
                    } else {
                        item.to_string()
                    }
                }
                _ => value_to_text(item),
            })
            .filter(|item| !item.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(map) => {
            if let Some(text) = map.get("text").and_then(Value::as_str) {
                text.to_owned()
            } else if let Some(content) = map.get("content") {
                value_to_text(content)
            } else {
                value.to_string()
            }
        }
        other => other.to_string(),
    }
}

fn browser_provider_metadata(kind: &BrowserProviderKind, chat: &BridgeChatResponse) -> Value {
    let mut metadata = serde_json::Map::new();
    metadata.insert("provider".to_owned(), json!(kind.as_str()));
    if let Some(conversation_id) = &chat.conversation_id {
        metadata.insert("conversation_id".to_owned(), json!(conversation_id));
    }
    if let Some(usage) = &chat.upstream_usage {
        metadata.insert("upstream_usage".to_owned(), usage.clone());
    }
    if let Some(cache) = &chat.upstream_cache {
        metadata.insert("upstream_cache".to_owned(), cache.clone());
    }
    Value::Object(metadata)
}

fn response_usage(usage: &Usage) -> Value {
    json!({
        "input_tokens": usage.prompt_tokens,
        "output_tokens": usage.completion_tokens,
        "total_tokens": usage.total_tokens,
    })
}

fn json_openai_response(
    state: AppState,
    body: OpenAIRequest,
    chat: BridgeChatResponse,
) -> Response {
    let response_id = format!("resp_{}", Uuid::new_v4().simple());
    let model = chat.model.clone().unwrap_or_else(|| body.model.clone());
    let parsed = parse_browser_output(&body, &chat.text);
    let usage = usage_from_text(&build_prompt(&body), &parsed.text, true);
    let mut output = Vec::new();
    if !parsed.text.is_empty() {
        output.push(json!({
            "id": format!("msg_{}", Uuid::new_v4().simple()),
            "type": "message",
            "role": "assistant",
            "content": [{ "type": "output_text", "text": parsed.text.clone() }]
        }));
    }
    for tool_call in &parsed.tool_calls {
        output.push(json!({
            "id": tool_call.id,
            "type": "function_call",
            "name": tool_call.function.name,
            "arguments": tool_call.function.arguments,
            "call_id": tool_call.id,
        }));
    }

    Json(json!({
        "id": response_id,
        "object": "response",
        "created_at": current_timestamp(),
        "status": "completed",
        "model": model,
        "output": output,
        "output_text": parsed.text,
        "usage": response_usage(&usage),
        "provider_metadata": browser_provider_metadata(&state.config.kind, &chat)
    }))
    .into_response()
}

fn stream_openai_response(
    state: AppState,
    body: OpenAIRequest,
    chat: BridgeChatResponse,
) -> Response {
    let response_id = format!("resp_{}", Uuid::new_v4().simple());
    let model = chat.model.clone().unwrap_or_else(|| body.model.clone());
    let parsed = parse_browser_output(&body, &chat.text);
    let usage = usage_from_text(&build_prompt(&body), &parsed.text, true);
    let text = parsed.text.clone();
    let chunks = split_text_chunks(&text, 320);
    let tool_calls = parsed.tool_calls.clone();

    let stream = stream! {
        yield Ok::<Bytes, std::convert::Infallible>(Bytes::from(format!("event: response.created\ndata: {}\n\n", json!({
            "id": response_id,
            "object": "response",
            "created_at": current_timestamp(),
            "status": "in_progress",
            "model": model,
        }))));

        for chunk in chunks {
            if chunk.is_empty() {
                continue;
            }
            yield Ok(Bytes::from(format!("event: response.output_text.delta\ndata: {}\n\n", json!({
                "id": response_id,
                "delta": chunk,
            }))));
        }

        for tool_call in tool_calls {
            yield Ok(Bytes::from(format!("event: response.output_item.added\ndata: {}\n\n", json!({
                "id": response_id,
                "item": {
                    "id": tool_call.id,
                    "type": "function_call",
                    "name": tool_call.function.name,
                    "arguments": tool_call.function.arguments,
                    "call_id": tool_call.id,
                }
            }))));
        }

        yield Ok(Bytes::from(format!("event: response.completed\ndata: {}\n\n", json!({
            "id": response_id,
            "object": "response",
            "created_at": current_timestamp(),
            "status": "completed",
            "model": model,
            "output_text": text,
            "usage": response_usage(&usage),
            "provider_metadata": browser_provider_metadata(&state.config.kind, &chat)
        }))));
    };

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/event-stream"),
            (header::CACHE_CONTROL, "no-cache"),
            (header::CONNECTION, "keep-alive"),
            (header::HeaderName::from_static("x-accel-buffering"), "no"),
        ],
        axum::body::Body::from_stream(stream),
    )
        .into_response()
}

fn json_anthropic_response(
    state: AppState,
    body: OpenAIRequest,
    chat: BridgeChatResponse,
) -> Response {
    let message_id = format!("msg_{}", Uuid::new_v4().simple());
    let model = chat.model.clone().unwrap_or_else(|| body.model.clone());
    let parsed = parse_browser_output(&body, &chat.text);
    let usage = usage_from_text(&build_prompt(&body), &parsed.text, true);
    let mut content = Vec::new();
    if !parsed.text.is_empty() {
        content.push(json!({ "type": "text", "text": parsed.text.clone() }));
    }
    for tool_call in parsed.tool_calls {
        content.push(json!({
            "type": "tool_use",
            "id": tool_call.id,
            "name": tool_call.function.name,
            "input": serde_json::from_str::<Value>(&tool_call.function.arguments).unwrap_or_else(|_| json!({ "raw": tool_call.function.arguments })),
        }));
    }

    Json(json!({
        "id": message_id,
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": content,
        "stop_reason": if content.iter().any(|item| item.get("type").and_then(Value::as_str) == Some("tool_use")) { "tool_use" } else { "end_turn" },
        "stop_sequence": Value::Null,
        "usage": {
            "input_tokens": usage.prompt_tokens,
            "output_tokens": usage.completion_tokens,
        },
        "provider_metadata": browser_provider_metadata(&state.config.kind, &chat)
    }))
    .into_response()
}

fn stream_anthropic_response(
    _state: AppState,
    body: OpenAIRequest,
    chat: BridgeChatResponse,
) -> Response {
    let message_id = format!("msg_{}", Uuid::new_v4().simple());
    let model = chat.model.clone().unwrap_or_else(|| body.model.clone());
    let parsed = parse_browser_output(&body, &chat.text);
    let usage = usage_from_text(&build_prompt(&body), &parsed.text, true);
    let chunks = split_text_chunks(&parsed.text, 320);
    let tool_calls = parsed.tool_calls.clone();
    let stop_reason = if tool_calls.is_empty() {
        "end_turn"
    } else {
        "tool_use"
    };

    // text block occupies index 0 when present; tool blocks follow immediately after
    let tool_block_start = if parsed.text.is_empty() { 0usize } else { 1 };

    let stream = stream! {
        yield Ok::<Bytes, std::convert::Infallible>(Bytes::from(format!("event: message_start\ndata: {}\n\n", json!({
            "type": "message_start",
            "message": {
                "id": message_id,
                "type": "message",
                "role": "assistant",
                "model": model,
                "content": [],
                "stop_reason": Value::Null,
                "stop_sequence": Value::Null,
                "usage": { "input_tokens": usage.prompt_tokens, "output_tokens": 0 }
            }
        }))));

        if !parsed.text.is_empty() {
            yield Ok(Bytes::from(format!("event: content_block_start\ndata: {}\n\n", json!({
                "type": "content_block_start",
                "index": 0,
                "content_block": { "type": "text", "text": "" }
            }))));
            for chunk in chunks {
                if chunk.is_empty() {
                    continue;
                }
                yield Ok(Bytes::from(format!("event: content_block_delta\ndata: {}\n\n", json!({
                    "type": "content_block_delta",
                    "index": 0,
                    "delta": { "type": "text_delta", "text": chunk }
                }))));
            }
            yield Ok(Bytes::from("event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n"));
        }

        for (i, tool_call) in tool_calls.iter().enumerate() {
            let block_index = tool_block_start + i;
            // spec: content_block_start carries empty input; SDK accumulates from input_json_delta
            yield Ok(Bytes::from(format!("event: content_block_start\ndata: {}\n\n", json!({
                "type": "content_block_start",
                "index": block_index,
                "content_block": {
                    "type": "tool_use",
                    "id": tool_call.id,
                    "name": tool_call.function.name,
                    "input": {},
                }
            }))));
            yield Ok(Bytes::from(format!("event: content_block_delta\ndata: {}\n\n", json!({
                "type": "content_block_delta",
                "index": block_index,
                "delta": { "type": "input_json_delta", "partial_json": tool_call.function.arguments }
            }))));
            yield Ok(Bytes::from(format!("event: content_block_stop\ndata: {}\n\n", json!({
                "type": "content_block_stop",
                "index": block_index
            }))));
        }

        yield Ok(Bytes::from(format!("event: message_delta\ndata: {}\n\n", json!({
            "type": "message_delta",
            "delta": { "stop_reason": stop_reason, "stop_sequence": Value::Null },
            "usage": { "output_tokens": usage.completion_tokens }
        }))));
        yield Ok(Bytes::from("event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"));
    };

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/event-stream"),
            (header::CACHE_CONTROL, "no-cache"),
            (header::CONNECTION, "keep-alive"),
            (header::HeaderName::from_static("x-accel-buffering"), "no"),
        ],
        axum::body::Body::from_stream(stream),
    )
        .into_response()
}

fn build_provider_warnings(
    kind: &BrowserProviderKind,
    body: &OpenAIRequest,
    chat: &BridgeChatResponse,
) -> Vec<String> {
    let mut warnings = Vec::new();
    if body.web_search == Some(true) && !kind.web_search_supported() {
        warnings.push(format!(
            "{} web search toggle is not mapped yet. Chat continued with normal browser session behavior.",
            kind.as_str()
        ));
    }
    if let Some(warning) = &chat.warning {
        if !warning.trim().is_empty() {
            warnings.push(warning.clone());
        }
    }
    warnings
}

fn split_text_chunks(text: &str, max_chars: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }

    let mut chunks = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        if current.is_empty() {
            current.push_str(word);
            continue;
        }

        if current.len() + 1 + word.len() > max_chars {
            chunks.push(current);
            current = word.to_owned();
        } else {
            current.push(' ');
            current.push_str(word);
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

fn require_api_key(
    headers: &HeaderMap,
    api_key: Option<&str>,
) -> std::result::Result<(), Box<Response>> {
    let Some(api_key) = api_key.filter(|value| !value.trim().is_empty()) else {
        return Ok(());
    };

    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim);
    let xkey = headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim);
    let authorized = matches!(bearer, Some(provided) if constant_time_eq(provided, api_key))
        || matches!(xkey, Some(provided) if constant_time_eq(provided, api_key));

    if authorized {
        Ok(())
    } else {
        Err(Box::new(json_error(
            StatusCode::UNAUTHORIZED,
            "Missing or invalid bearer token".to_owned(),
        )))
    }
}

fn provider_error(state: &AppState, err: anyhow::Error) -> Response {
    // ponytail: log full cause server-side; return opaque id to client so upstream
    // bodies / header fragments never leak. Login-required detection runs on the
    // internal message before it's dropped.
    let message = err.to_string();
    let id = uuid::Uuid::new_v4();
    eprintln!(
        "[{}] upstream error {id}: {message}",
        state.config.kind.as_str()
    );
    let status = if is_login_required_error(&message) {
        StatusCode::UNAUTHORIZED
    } else {
        StatusCode::BAD_GATEWAY
    };

    json_error(
        status,
        format!("{} upstream error (id={id})", state.config.kind.as_str()),
    )
}

fn is_login_required_error(message: &str) -> bool {
    let lowered = message.to_ascii_lowercase();
    lowered.contains("logged in")
        || lowered.contains("session is active")
        || lowered.contains("timeout waiting for")
        || lowered.contains("request template")
}

fn json_error(status: StatusCode, message: String) -> Response {
    (
        status,
        Json(json!({
            "error": {
                "message": message,
                "type": "provider_error",
            }
        })),
    )
        .into_response()
}
