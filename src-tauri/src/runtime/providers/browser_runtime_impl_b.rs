fn stream_browser_chat(state: AppState, body: OpenAIRequest, chat: BridgeChatResponse) -> Response {
    let completion_id = format!("chatcmpl-{}", Uuid::new_v4());
    let model = chat.model.clone().unwrap_or_else(|| body.model.clone());
    let prompt = browser_request_preflight(&body)
        .map(|preflight| preflight.flat_prompt)
        .unwrap_or_else(|_| build_prompt(&body));
    let provider_warnings = build_provider_warnings(&state.config.kind, &body, &chat);
    let parsed = parse_browser_output(&body, &chat.text);
    let chunks = split_text_chunks(&parsed.text, 320);
    let tool_calls = parsed.tool_calls.clone();
    let finish_reason = if tool_calls.is_empty() {
        "stop"
    } else {
        "tool_calls"
    };

    let stream = stream! {
        yield Ok::<Bytes, std::convert::Infallible>(sse_json(json!({
            "id": completion_id,
            "object": "chat.completion.chunk",
            "created": current_timestamp(),
            "model": model,
            "choices": [{
                "index": 0,
                "delta": {
                    "role": "assistant",
                    "content": "",
                    "reasoning_content": chat.reasoning_content,
                },
                "logprobs": Value::Null,
                "finish_reason": Value::Null,
            }],
            "provider_warnings": provider_warnings,
        })));

        for chunk in chunks {
            if chunk.is_empty() {
                continue;
            }
            yield Ok(sse_json(json!({
                "id": completion_id,
                "object": "chat.completion.chunk",
                "created": current_timestamp(),
                "model": model,
                "choices": [{
                    "index": 0,
                    "delta": { "content": chunk },
                    "logprobs": Value::Null,
                    "finish_reason": Value::Null,
                }],
            })));
        }

        if !tool_calls.is_empty() {
            yield Ok(sse_json(json!({
                "id": completion_id,
                "object": "chat.completion.chunk",
                "created": current_timestamp(),
                "model": model,
                "choices": [{
                    "index": 0,
                    "delta": { "tool_calls": tool_calls },
                    "logprobs": Value::Null,
                    "finish_reason": Value::Null,
                }],
            })));
        }

        yield Ok(sse_json(json!({
            "id": completion_id,
            "object": "chat.completion.chunk",
            "created": current_timestamp(),
            "model": model,
            "choices": [{
                "index": 0,
                "delta": {},
                "logprobs": Value::Null,
                "finish_reason": finish_reason,
            }],
            "usage": usage_from_text(&prompt, &parsed.text, true),
            "provider_metadata": browser_provider_metadata(&state.config.kind, &chat),
        })));
        yield Ok(sse_done());
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

fn stream_browser_chat_live(
    state: AppState,
    body: OpenAIRequest,
    mut bridge_stream: BridgeStream,
) -> Response {
    let completion_id = format!("chatcmpl-{}", Uuid::new_v4());
    let model = browser_request_model(state.config.kind, &body.model);
    let prompt = browser_request_preflight(&body)
        .map(|preflight| preflight.flat_prompt)
        .unwrap_or_else(|_| build_prompt(&body));

    let stream = stream! {
        yield Ok::<Bytes, std::convert::Infallible>(sse_json(json!({
            "id": completion_id,
            "object": "chat.completion.chunk",
            "created": current_timestamp(),
            "model": model,
            "choices": [{
                "index": 0,
                "delta": { "role": "assistant", "content": "" },
                "logprobs": Value::Null,
                "finish_reason": Value::Null,
            }],
        })));

        let mut parser = StreamingToolParser::new();
        let defer_text_until_final = body.tools.is_some();
        let mut emitted_text = String::new();
        let mut emitted_reasoning = String::new();
        while let Some(event) = bridge_stream.next_event().await {
            match event {
                Ok(event) if event.event == "delta" => {
                    if let Some(delta) = event.payload.get("delta").and_then(Value::as_str) {
                        let parsed = parser.feed(delta);
                        if !parsed.text.is_empty() && !defer_text_until_final {
                                emitted_text.push_str(&parsed.text);
                                yield Ok(sse_json(json!({
                                    "id": completion_id,
                                    "object": "chat.completion.chunk",
                                    "created": current_timestamp(),
                                    "model": model,
                                    "choices": [{
                                        "index": 0,
                                        "delta": { "content": parsed.text },
                                        "logprobs": Value::Null,
                                        "finish_reason": Value::Null,
                                    }],
                                })));
                        }
                    }
                }
                Ok(event) if event.event == "reasoning" => {
                    if let Some(delta) = event.payload.get("delta").and_then(Value::as_str) {
                        emitted_reasoning.push_str(delta);
                        yield Ok(sse_json(json!({
                            "id": completion_id,
                            "object": "chat.completion.chunk",
                            "created": current_timestamp(),
                            "model": model,
                            "choices": [{
                                "index": 0,
                                "delta": { "reasoning_content": delta },
                                "logprobs": Value::Null,
                                "finish_reason": Value::Null,
                            }],
                        })));
                    }
                }
                Ok(_) => {}
                Err(err) => {
                    yield Ok(sse_json(json!({ "error": { "message": err.to_string(), "type": "provider_stream_error" } })));
                    yield Ok(sse_done());
                    return;
                }
            }
        }

        let flushed = parser.flush();
        if !flushed.text.is_empty() {
            emitted_text.push_str(&flushed.text);
            yield Ok(sse_json(json!({
                "id": completion_id,
                "object": "chat.completion.chunk",
                "created": current_timestamp(),
                "model": model,
                "choices": [{
                    "index": 0,
                    "delta": { "content": flushed.text },
                    "logprobs": Value::Null,
                    "finish_reason": Value::Null,
                }],
            })));
        }

        let chat = match bridge_stream.finish::<BridgeChatResponse>().await {
            Ok(chat) => chat,
            Err(err) => {
                yield Ok(sse_json(json!({ "error": { "message": err.to_string(), "type": "provider_stream_error" } })));
                yield Ok(sse_done());
                return;
            }
        };
        let parsed = parse_browser_output(&body, &chat.text);
        if let Some(reasoning) = chat.reasoning_content.as_deref() {
            let remaining = reasoning.strip_prefix(&emitted_reasoning).unwrap_or(reasoning);
            if !remaining.is_empty() {
                yield Ok(sse_json(json!({
                    "id": completion_id,
                    "object": "chat.completion.chunk",
                    "created": current_timestamp(),
                    "model": model,
                    "choices": [{
                        "index": 0,
                        "delta": { "reasoning_content": remaining },
                        "logprobs": Value::Null,
                        "finish_reason": Value::Null,
                    }],
                })));
            }
        }
        if let Some(remaining) = parsed.text.strip_prefix(&emitted_text) {
            if !remaining.is_empty() {
                yield Ok(sse_json(json!({
                    "id": completion_id,
                    "object": "chat.completion.chunk",
                    "created": current_timestamp(),
                    "model": model,
                    "choices": [{
                        "index": 0,
                        "delta": { "content": remaining },
                        "logprobs": Value::Null,
                        "finish_reason": Value::Null,
                    }],
                })));
            }
        } else if emitted_text.is_empty() && !parsed.text.is_empty() {
            for chunk in split_text_chunks(&parsed.text, 320) {
                yield Ok(sse_json(json!({
                    "id": completion_id,
                    "object": "chat.completion.chunk",
                    "created": current_timestamp(),
                    "model": model,
                    "choices": [{
                        "index": 0,
                        "delta": { "content": chunk },
                        "logprobs": Value::Null,
                        "finish_reason": Value::Null,
                    }],
                })));
            }
        }

        let finish_reason = if parsed.tool_calls.is_empty() { "stop" } else { "tool_calls" };
        if !parsed.tool_calls.is_empty() {
            yield Ok(sse_json(json!({
                "id": completion_id,
                "object": "chat.completion.chunk",
                "created": current_timestamp(),
                "model": model,
                "choices": [{
                    "index": 0,
                    "delta": { "tool_calls": parsed.tool_calls },
                    "logprobs": Value::Null,
                    "finish_reason": Value::Null,
                }],
            })));
        }
        yield Ok(sse_json(json!({
            "id": completion_id,
            "object": "chat.completion.chunk",
            "created": current_timestamp(),
            "model": model,
            "choices": [{
                "index": 0,
                "delta": {},
                "logprobs": Value::Null,
                "finish_reason": finish_reason,
            }],
            "usage": usage_from_text(&prompt, &parsed.text, true),
            "provider_metadata": browser_provider_metadata(&state.config.kind, &chat),
            "provider_warnings": build_provider_warnings(&state.config.kind, &body, &chat),
        })));
        yield Ok(sse_done());
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

