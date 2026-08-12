async fn handle_chat(state: AppState, body: OpenAIRequest) -> Result<Response> {
    let (normalized, pending_uploads) = normalize_request(&body);
    let prompt_budget = state
        .model_registry
        .context_window(&normalized.model)
        .await
        .saturating_sub(1000);
    let preflight = preflight_request_to_budget(
        &normalized,
        &PromptPreflightOptions {
            max_prompt_tokens: Some(prompt_budget),
            extra_system_instructions: None,
            dedup_system_blocks: false,
            structured_compaction_max_chars: None,
        },
        |text| {
            state
                .model_registry
                .estimate_tokens(text, &normalized.model)
        },
    )?;
    let final_prompt = preflight.flat_prompt;
    let completion_id = format!("chatcmpl-{}", Uuid::new_v4());
    let is_stream = body.stream.unwrap_or(false);
    let include_usage = body
        .stream_options
        .as_ref()
        .and_then(|options| options.include_usage)
        .unwrap_or(false);

    let session_key = qwen_session_key(body.user.as_deref());
    let existing_conversation = state.conversations.get(&session_key).await;
    let all_accounts = effective_accounts(&state.accounts)?;
    let mut accounts = if let Some(conversation) = &existing_conversation {
        match all_accounts
            .iter()
            .find(|account| account.id == conversation.account_id)
        {
            Some(account) => vec![account.clone()],
            None => {
                state.conversations.remove(&session_key).await;
                all_accounts.clone()
            }
        }
    } else {
        all_accounts.clone()
    };
    let mut current_account = state.account_manager.select_next(&accounts, false).await;
    let mut tried_accounts = HashSet::new();
    let mut last_error: Option<anyhow::Error> = None;

    while let Some(account) = current_account {
        if tried_accounts.contains(&account.id) {
            current_account = state
                .account_manager
                .select_next_available(&accounts, Some(&account.id))
                .await;
            continue;
        }
        tried_accounts.insert(account.id.clone());
        let account_lease = state.account_manager.lease(account.id.clone()).await;

        let bridge_account_id = account_id_for_bridge(&account);
        let header_result = state
            .bridge
            .capture_headers(CaptureHeadersParams {
                force_new: false,
                account_id: bridge_account_id,
            })
            .await;
        let basic_headers = match header_result {
            Ok(headers) => headers.headers,
            Err(err) => {
                last_error = Some(anyhow!(err));
                current_account = state
                    .account_manager
                    .select_next_available(&accounts, Some(&account.id))
                    .await;
                continue;
            }
        };

        let files = if pending_uploads.is_empty() {
            Vec::new()
        } else {
            match prepare_multimodal_uploads(
                &state.client,
                &state.config.qwen_base_url,
                &basic_headers,
                &pending_uploads,
            )
            .await
            {
                Ok(files) => files,
                Err(err) => return Err(err),
            }
        };

        let mut conversation = existing_conversation
            .as_ref()
            .filter(|conversation| conversation.account_id == account.id)
            .cloned();
        let mut retries = 3usize;
        let mut retry_delay_ms = 500u64;

        loop {
            let chat_id = match &conversation {
                Some(conversation) => conversation.chat_id.clone(),
                None => {
                    match create_qwen_chat(&state.client, &state.config, &basic_headers).await {
                        Ok(chat_id) => chat_id,
                        Err(err) => {
                            last_error = Some(err);
                            break;
                        }
                    }
                }
            };
            let parent_id = conversation
                .as_ref()
                .and_then(|conversation| conversation.parent_id.clone());
            state
                .conversations
                .upsert(
                    session_key.clone(),
                    QwenConversation {
                        chat_id: chat_id.clone(),
                        account_id: account.id.clone(),
                        parent_id: parent_id.clone(),
                    },
                )
                .await;
            conversation = state.conversations.get(&session_key).await;

            match request_qwen_chat(
                &state,
                QwenChatRequest {
                    body: &body,
                    final_prompt: &final_prompt,
                    completion_id: &completion_id,
                    chat_id: &chat_id,
                    parent_id: parent_id.as_deref(),
                    headers: &basic_headers,
                    files: &files,
                },
            )
            .await
            {
                Ok(response) => {
                    let cancel_token = state
                        .stream_registry
                        .register(
                            completion_id.clone(),
                            chat_id.clone(),
                            account.id.clone(),
                            basic_headers.clone(),
                        )
                        .await;
                    state
                        .metrics
                        .gauge(
                            "streams.active",
                            state.stream_registry.active_count().await as f64,
                        )
                        .await;

                    if !is_stream {
                        let result = build_non_stream_response(
                            &state,
                            &body,
                            &final_prompt,
                            QwenConversationRef {
                                chat_id: &chat_id,
                                session_key: &session_key,
                            },
                            &completion_id,
                            response,
                            cancel_token,
                        )
                        .await;
                        state
                            .stream_registry
                            .remove_by_completion_id(&completion_id)
                            .await;
                        state
                            .metrics
                            .gauge(
                                "streams.active",
                                state.stream_registry.active_count().await as f64,
                            )
                            .await;
                        return result;
                    }

                    return Ok(build_stream_response(StreamResponseArgs {
                        state,
                        body: body.clone(),
                        final_prompt,
                        chat_id,
                        session_key,
                        completion_id,
                        response,
                        cancel_token,
                        include_usage,
                        account_lease,
                    }));
                }
                Err(err) => {
                    retries = retries.saturating_sub(1);
                    if err.upstream_status == Some(429)
                        || err.upstream_code.as_deref() == Some("RateLimited")
                    {
                        state
                            .account_manager
                            .mark_rate_limited(&account.id, err.retry_after_ms, "RateLimited")
                            .await;
                        if accounts.len() != all_accounts.len() {
                            state.conversations.remove(&session_key).await;
                            accounts = all_accounts.clone();
                        }
                        last_error = Some(anyhow!(err.message));
                        break;
                    }

                    let retryable = err.retryable
                        || err.retry_after_ms.is_some()
                        || err.message.contains("chat is in progress")
                        || err.message.contains("Bad_Request");
                    if retryable && retries > 0 {
                        let delay = err.retry_after_ms.unwrap_or(retry_delay_ms);
                        tokio::time::sleep(Duration::from_millis(delay)).await;
                        retry_delay_ms = (retry_delay_ms * 2).min(5_000);
                        continue;
                    }

                    if retries == 0 && err.upstream_status.unwrap_or(500) >= 500 {
                        state
                            .account_manager
                            .mark_rate_limited(&account.id, None, "ServerError")
                            .await;
                    }
                    last_error = Some(anyhow!(err.message));
                    break;
                }
            }
        }

        current_account = state
            .account_manager
            .select_next_available(&accounts, Some(&account.id))
            .await;
    }

    Err(last_error.unwrap_or_else(|| anyhow!("All accounts failed")))
}

struct QwenConversationRef<'a> {
    chat_id: &'a str,
    session_key: &'a str,
}

async fn build_non_stream_response(
    state: &AppState,
    body: &OpenAIRequest,
    final_prompt: &str,
    conversation: QwenConversationRef<'_>,
    completion_id: &str,
    response: reqwest::Response,
    cancel_token: tokio_util::sync::CancellationToken,
) -> Result<Response> {
    let mut parse_state = QwenParseState::default();
    let mut tool_parser = body.tools.as_ref().map(|_| StreamingToolParser::new());
    let mut tool_calls = Vec::new();
    let mut buffer = String::new();
    let mut raw_response = String::new();
    let mut offset = 0usize;
    let mut bytes_stream = response.bytes_stream();

    loop {
        tokio::select! {
            _ = cancel_token.cancelled() => {
                return Err(anyhow!("stream cancelled"));
            }
            chunk = bytes_stream.next() => {
                let Some(chunk) = chunk else { break; };
                let chunk = chunk?;
                let chunk = String::from_utf8_lossy(&chunk);
                raw_response.push_str(&chunk);
                buffer.push_str(&chunk);
                while let Some(rel) = buffer[offset..].find('\n') {
                    let end = offset + rel;
                    let line = buffer[offset..end].trim().to_owned();
                    offset = end + 1;
                    if let Some(data) = qwen_sse_data(&line) {
                        for event in collect_qwen_events(
                            data,
                            completion_id,
                            &state.stream_registry,
                            &mut parse_state,
                            &mut tool_parser,
                        )
                        .await?
                        {
                            if let QwenEvent::ToolCall(tool_call) = event {
                                tool_calls.push(tool_call);
                            }
                        }
                    }
                }
                // drain in place: offset is right after '\n' (ASCII), always a char boundary
                buffer.drain(..offset);
                offset = 0;
            }
        }
    }

    state
        .traces
        .record(completion_id, "upstream_response", raw_response)
        .await;

    if let Some(parser) = &mut tool_parser {
        let flush = parser.flush();
        if !flush.text.is_empty() {
            if parse_state.last_full_content.is_empty() {
                parse_state.last_full_content = flush.text;
            } else {
                parse_state.last_full_content.push_str(&flush.text);
            }
        }
        for parsed_tool in flush.tool_calls {
            tool_calls.push(tool_call_from_parsed(parsed_tool));
        }
    }

    if let Some(parent_id) = parse_state.target_response_id.clone() {
        state
            .conversations
            .update_parent(conversation.session_key, conversation.chat_id, parent_id)
            .await;
    }

    let usage = usage_from_text(
        final_prompt,
        &format!("{}{}", parse_state.reasoning, parse_state.last_full_content),
        true,
    );
    let message = if tool_calls.is_empty() {
        json!({
            "role": "assistant",
            "content": parse_state.last_full_content,
            "reasoning_content": parse_state.reasoning,
        })
    } else {
        json!({
            "role": "assistant",
            "content": Value::Null,
            "reasoning_content": parse_state.reasoning,
            "tool_calls": tool_calls,
        })
    };

    Ok(Json(json!({
        "id": completion_id,
        "object": "chat.completion",
        "created": current_timestamp(),
        "model": body.model,
        "choices": [{
            "index": 0,
            "message": message,
            "logprobs": Value::Null,
            "finish_reason": if tool_calls.is_empty() { "stop" } else { "tool_calls" }
        }],
        "usage": usage
    }))
    .into_response())
}

struct StreamResponseArgs {
    state: AppState,
    body: OpenAIRequest,
    final_prompt: String,
    chat_id: String,
    session_key: String,
    completion_id: String,
    response: reqwest::Response,
    cancel_token: tokio_util::sync::CancellationToken,
    include_usage: bool,
    account_lease: AccountLease,
}

fn build_stream_response(args: StreamResponseArgs) -> Response {
    let StreamResponseArgs {
        state,
        body,
        final_prompt,
        chat_id,
        session_key,
        completion_id,
        response,
        cancel_token,
        include_usage,
        account_lease,
    } = args;
    let model = body.model.clone();
    let stream_registry = state.stream_registry.clone();
    let conversations = state.conversations.clone();
    let metrics = state.metrics.clone();
    /* frees the registry slot even if the client disconnects before the generator
    reaches its explicit remove below */
    let cleanup_guard = stream_registry.guard(completion_id.clone());

    let stream = stream! {
        let _account_lease = account_lease;
        let _cleanup_guard = cleanup_guard;
        yield Ok::<Bytes, std::convert::Infallible>(Bytes::from(": heartbeat\n\n"));
        yield Ok(sse_json(json!({
            "id": completion_id,
            "object": "chat.completion.chunk",
            "created": current_timestamp(),
            "model": model,
            "choices": [{ "index": 0, "delta": { "role": "assistant", "content": "" }, "logprobs": Value::Null, "finish_reason": Value::Null }]
        })));

        let mut parse_state = QwenParseState::default();
        let mut tool_parser = body.tools.as_ref().map(|_| StreamingToolParser::new());
        let mut tool_index = 0usize;
        let mut buffer = String::new();
        let mut raw_response = String::new();
        let mut offset = 0usize;
        let mut bytes_stream = response.bytes_stream();

        loop {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    yield Ok(sse_done());
                    break;
                }
                chunk = bytes_stream.next() => {
                    match chunk {
                        Some(Ok(chunk)) => {
                            let chunk = String::from_utf8_lossy(&chunk);
                            raw_response.push_str(&chunk);
                            buffer.push_str(&chunk);
                            while let Some(rel) = buffer[offset..].find('\n') {
                                let end = offset + rel;
                                let line = buffer[offset..end].trim().to_owned();
                                offset = end + 1;
                                if let Some(data) = qwen_sse_data(&line) {
                                    match collect_qwen_events(data, &completion_id, &stream_registry, &mut parse_state, &mut tool_parser).await {
                                        Ok(events) => {
                                            for event in events {
                                                match event {
                                                    QwenEvent::Reasoning(content) => {
                                                        yield Ok(sse_json(json!({
                                                            "id": completion_id,
                                                            "object": "chat.completion.chunk",
                                                            "created": current_timestamp(),
                                                            "model": model,
                                                            "choices": [{ "index": 0, "delta": { "reasoning_content": content }, "logprobs": Value::Null, "finish_reason": Value::Null }]
                                                        })));
                                                    }
                                                    QwenEvent::Text(content) => {
                                                        yield Ok(sse_json(json!({
                                                            "id": completion_id,
                                                            "object": "chat.completion.chunk",
                                                            "created": current_timestamp(),
                                                            "model": model,
                                                            "choices": [{ "index": 0, "delta": { "content": content }, "logprobs": Value::Null, "finish_reason": Value::Null }]
                                                        })));
                                                    }
                                                    QwenEvent::ToolCall(tool_call) => {
                                                        yield Ok(sse_json(json!({
                                                            "id": completion_id,
                                                            "object": "chat.completion.chunk",
                                                            "created": current_timestamp(),
                                                            "model": model,
                                                            "choices": [{
                                                                "index": 0,
                                                                "delta": {
                                                                    "tool_calls": [{
                                                                        "index": tool_index,
                                                                        "id": tool_call.id,
                                                                        "type": "function",
                                                                        "function": tool_call.function,
                                                                    }]
                                                                },
                                                                "logprobs": Value::Null,
                                                                "finish_reason": Value::Null
                                                            }]
                                                        })));
                                                        tool_index += 1;
                                                    }
                                                }
                                            }
                                        }
                                        Err(err) => {
                                            yield Ok(sse_json(json!({
                                                "id": completion_id,
                                                "object": "chat.completion.chunk",
                                                "created": current_timestamp(),
                                                "model": model,
                                                "choices": [{ "index": 0, "delta": { "content": format!("Qwen parse error: {err}") }, "logprobs": Value::Null, "finish_reason": "stop" }]
                                            })));
                                            yield Ok(sse_done());
                                            break;
                                        }
                                    }
                                }
                            }
                            // drain in place: offset is right after '\n' (ASCII), always a char boundary
                            buffer.drain(..offset);
                            offset = 0;
                        }
                        Some(Err(err)) => {
                            metrics.increment("streams.errors", 1.0).await;
                            yield Ok(sse_json(json!({
                                "id": completion_id,
                                "object": "chat.completion.chunk",
                                "created": current_timestamp(),
                                "model": model,
                                "choices": [{ "index": 0, "delta": { "content": format!("Qwen upstream error: {err}") }, "logprobs": Value::Null, "finish_reason": "stop" }]
                            })));
                            yield Ok(sse_done());
                            break;
                        }
                        None => break,
                    }
                }
            }
        }

        state
            .traces
            .record(&completion_id, "upstream_response", raw_response)
            .await;

        if let Some(parser) = &mut tool_parser {
            let flush = parser.flush();
            if !flush.text.is_empty() {
                yield Ok(sse_json(json!({
                    "id": completion_id,
                    "object": "chat.completion.chunk",
                    "created": current_timestamp(),
                    "model": model,
                    "choices": [{ "index": 0, "delta": { "content": flush.text }, "logprobs": Value::Null, "finish_reason": Value::Null }]
                })));
            }
            for parsed_tool in flush.tool_calls {
                let tool_call = tool_call_from_parsed(parsed_tool);
                yield Ok(sse_json(json!({
                    "id": completion_id,
                    "object": "chat.completion.chunk",
                    "created": current_timestamp(),
                    "model": model,
                    "choices": [{
                        "index": 0,
                        "delta": {
                            "tool_calls": [{
                                "index": tool_index,
                                "id": tool_call.id,
                                "type": "function",
                                "function": tool_call.function,
                            }]
                        },
                        "logprobs": Value::Null,
                        "finish_reason": Value::Null
                    }]
                })));
                tool_index += 1;
            }
        }

        let usage = json!({
            "prompt_tokens": parse_state.prompt_tokens.max(state.model_registry.estimate_tokens(&final_prompt, &model)),
            "completion_tokens": parse_state.completion_tokens.max(state.model_registry.estimate_tokens(&format!("{}{}", parse_state.reasoning, parse_state.last_full_content), &model)),
            "total_tokens": parse_state.prompt_tokens.max(state.model_registry.estimate_tokens(&final_prompt, &model)) + parse_state.completion_tokens.max(state.model_registry.estimate_tokens(&format!("{}{}", parse_state.reasoning, parse_state.last_full_content), &model)),
            "prompt_tokens_details": { "cached_tokens": 0 }
        });

        let mut final_chunk = json!({
            "id": completion_id,
            "object": "chat.completion.chunk",
            "created": current_timestamp(),
            "model": model,
            "choices": [{ "index": 0, "delta": {}, "logprobs": Value::Null, "finish_reason": if tool_index == 0 { "stop" } else { "tool_calls" } }],
        });
        if !include_usage {
            final_chunk["usage"] = usage.clone();
        }
        yield Ok(sse_json(final_chunk));

        if include_usage {
            yield Ok(sse_json(json!({
                "id": completion_id,
                "object": "chat.completion.chunk",
                "created": current_timestamp(),
                "model": model,
                "choices": [],
                "usage": usage,
            })));
        }
        yield Ok(sse_done());

        if let Some(parent_id) = parse_state.target_response_id.clone() {
            conversations
                .update_parent(&session_key, &chat_id, parent_id)
                .await;
        }
        stream_registry.remove_by_completion_id(&completion_id).await;
        metrics.gauge("streams.active", stream_registry.active_count().await as f64).await;
    };

    stream_response(stream)
}

