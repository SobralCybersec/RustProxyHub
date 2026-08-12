async fn run_server(bridge: Arc<PlaywrightBridge>, config: AppConfig) -> Result<()> {
    crate::proxy_core::enforce_loopback_guard(&config.host, config.api_key.as_deref())?;
    bridge
        .init(InitParams {
            runtime_dir: config.runtime_dir.to_string_lossy().to_string(),
            headless: config.headless,
            browser: config.browser.clone(),
        })
        .await?;

    let state = AppState {
        bridge,
        client: reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(6 * 60 * 60))
            .tcp_nodelay(true)
            .read_timeout(Duration::from_secs(120))
            .tcp_keepalive(Duration::from_secs(30))
            .pool_idle_timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(16)
            .build()?,
        config: config.clone(),
        session_parents: SessionStore::open(config.runtime_dir.join("provider-sessions.db"))?,
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/admin/manual_login", post(admin_manual_login))
        .route("/admin/close_login", post(admin_close_login))
        .route("/admin/logs", get(admin_logs))
        .route("/v1/models", get(models))
        .route("/v1/chat/completions", post(chat_completions))
        .layer(DefaultBodyLimit::max(100 * 1024 * 1024))
        .with_state(state);

    let host: IpAddr = config
        .host
        .parse()
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    let addr = SocketAddr::new(host, config.port);
    println!("proxy-hub deepseek listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

pub async fn serve_embedded(config: DeepseekServiceConfig) -> Result<()> {
    tokio::fs::create_dir_all(&config.runtime_dir).await?;
    let bridge = Arc::new(
        PlaywrightBridge::new_with_node(&config.helper_dir, config.node_path.clone(), "deepseek")
            .await?,
    );
    run_server(
        bridge,
        AppConfig {
            host: config.host,
            port: config.port,
            api_key: config.api_key,
            headless: config.headless,
            browser: config.browser,
            runtime_dir: config.runtime_dir,
        },
    )
    .await
}

async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok" }))
}

async fn ensure_deepseek_ready(state: &AppState) -> Result<()> {
    state
        .bridge
        .init(InitParams {
            runtime_dir: state.config.runtime_dir.to_string_lossy().to_string(),
            headless: state.config.headless,
            browser: state.config.browser.clone(),
        })
        .await
}

async fn admin_manual_login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ManualLoginRequest>,
) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref(), true) {
        return *response;
    }

    match state
        .bridge
        .manual_login(ManualLoginParams {
            runtime_dir: state.config.runtime_dir.to_string_lossy().to_string(),
            browser: body.browser.unwrap_or_else(|| state.config.browser.clone()),
            account_id: None,
        })
        .await
    {
        Ok(()) => Json(json!({ "ok": true, "provider": "deepseek" })).into_response(),
        Err(err) => json_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn admin_close_login(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref(), true) {
        return *response;
    }

    match state.bridge.shutdown().await {
        Ok(()) => Json(json!({ "ok": true, "provider": "deepseek" })).into_response(),
        Err(err) => json_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn admin_logs(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref(), true) {
        return *response;
    }
    let entries = tokio::fs::read_to_string(state.bridge.log_path())
        .await
        .unwrap_or_default()
        .lines()
        .rev()
        .take(160)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    Json(json!({ "provider": "deepseek", "entries": entries })).into_response()
}

async fn models(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref(), true) {
        return *response;
    }

    Json(json!({
        "object": "list",
        "data": [
            { "id": "deepseek-v4-flash", "object": "model", "created": current_timestamp(), "owned_by": "deepseek", "permission": [], "root": "deepseek-v4-flash", "parent": null },
            { "id": "deepseek-v4-flash-thinking", "object": "model", "created": current_timestamp(), "owned_by": "deepseek", "permission": [], "root": "deepseek-v4-flash-thinking", "parent": null },
            { "id": "deepseek-v4-pro", "object": "model", "created": current_timestamp(), "owned_by": "deepseek", "permission": [], "root": "deepseek-v4-pro", "parent": null },
            { "id": "deepseek-v4-pro-thinking", "object": "model", "created": current_timestamp(), "owned_by": "deepseek", "permission": [], "root": "deepseek-v4-pro-thinking", "parent": null },
            { "id": "deepseek-instant", "object": "model", "created": current_timestamp(), "owned_by": "deepseek", "permission": [], "root": "deepseek-v4-flash", "parent": null },
            { "id": "deepseek-instant-deepthink", "object": "model", "created": current_timestamp(), "owned_by": "deepseek", "permission": [], "root": "deepseek-v4-flash-thinking", "parent": null },
            { "id": "deepseek-expert", "object": "model", "created": current_timestamp(), "owned_by": "deepseek", "permission": [], "root": "deepseek-v4-pro", "parent": null },
            { "id": "deepseek-expert-deepthink", "object": "model", "created": current_timestamp(), "owned_by": "deepseek", "permission": [], "root": "deepseek-v4-pro-thinking", "parent": null },
            { "id": "deepseek-v4-vision", "object": "model", "created": current_timestamp(), "owned_by": "deepseek", "permission": [], "root": "deepseek-v4-vision", "parent": null }
        ]
    }))
    .into_response()
}

async fn chat_completions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<OpenAIRequest>,
) -> Response {
    if let Err(response) = require_api_key(&headers, state.config.api_key.as_deref(), true) {
        return *response;
    }

    match handle_chat(state, body).await {
        Ok(response) => response,
        Err(err) => bad_gateway_error(err),
    }
}

async fn handle_chat(state: AppState, body: OpenAIRequest) -> Result<Response> {
    ensure_deepseek_ready(&state).await?;
    let final_prompt = deepen_tool_prompt(build_prompt(&body), &body);
    let is_stream = body.stream.unwrap_or(false);
    let (is_pro, is_thinking, is_vision) = deepseek_mode_flags(&body.model);
    let is_new_session = !body
        .messages
        .iter()
        .any(|message| message.role == "assistant");

    let mut last_error = None;
    let mut response = None;
    let mut ui_session_id = SessionId::default();

    for attempt in 0..3u32 {
        let captured = match state
            .bridge
            .capture_headers(CaptureHeadersParams {
                force_new: is_new_session,
                account_id: None,
            })
            .await
        {
            Ok(captured) => captured,
            Err(err) => {
                last_error = Some(anyhow!(err));
                // exponential backoff + jitter: 250ms, 500ms, 1000ms ± 0-100ms.
                let base = 250u64 * 2u64.pow(attempt);
                let jitter = ((attempt * 37) % 100) as u64;
                tokio::time::sleep(Duration::from_millis(base + jitter)).await;
                continue;
            }
        };

        ui_session_id = SessionId::new(captured.chat_session_id.unwrap_or_default());
        let browser_parent = captured
            .parent_message_id
            .as_ref()
            .and_then(Value::as_i64)
            .map(ParentMessageId::from);
        let actual_parent = if is_new_session {
            None
        } else {
            state
                .session_parents
                .get::<ParentMessageId>("deepseek", ui_session_id.as_str())?
                .or(browser_parent)
        };

        let payload = DeepseekPayload::new(&final_prompt, &ui_session_id)
            .template(captured.request_payload.clone())
            .parent(actual_parent)
            .pro(is_pro)
            .thinking(is_thinking)
            .vision(is_vision)
            .search(body.web_search.unwrap_or(true))
            .build();

        let request = state
            .client
            .post("https://chat.deepseek.com/api/v0/chat/completion")
            .header("accept", "*/*")
            .header("accept-language", "pt-BR,pt;q=0.9,en-US;q=0.8,en;q=0.7")
            .header(
                "authorization",
                captured
                    .headers
                    .get("authorization")
                    .cloned()
                    .unwrap_or_default(),
            )
            .header("content-type", "application/json")
            .header("origin", "https://chat.deepseek.com")
            .header(
                "x-ds-pow-response",
                captured
                    .headers
                    .get("x-ds-pow-response")
                    .cloned()
                    .unwrap_or_default(),
            )
            .header(
                "x-hif-dliq",
                captured
                    .headers
                    .get("x-hif-dliq")
                    .cloned()
                    .unwrap_or_default(),
            )
            .header(
                "x-hif-leim",
                captured
                    .headers
                    .get("x-hif-leim")
                    .cloned()
                    .unwrap_or_default(),
            )
            .header("x-app-version", "2.0.0")
            .header("x-client-locale", "pt_BR")
            .header("x-client-platform", "web")
            .header("x-client-version", "2.0.0")
            .json(&payload)
            .send()
            .await;

        match request {
            Ok(upstream) if upstream.status().is_success() => {
                response = Some(upstream);
                break;
            }
            Ok(upstream) => {
                // Don't dump full upstream body into the error; log server-side only.
                let status = upstream.status();
                let body = upstream.text().await.unwrap_or_default();
                eprintln!(
                    "[deepseek] upstream http {status}: {}",
                    body.chars().take(400).collect::<String>()
                );
                last_error = Some(anyhow!("deepseek upstream returned {status}"));
            }
            Err(err) => {
                last_error = Some(anyhow!(err));
            }
        }
    }

    let response = response
        .ok_or_else(|| last_error.unwrap_or_else(|| anyhow!("DeepSeek upstream request failed")))?;
    let completion_id = format!("chatcmpl-{}", Uuid::new_v4());
    if !is_stream {
        let mut parse_state = DeepSeekParseState::default();
        let mut tool_parser = body.tools.as_ref().map(|_| StreamingToolParser::new());
        let mut tool_calls = Vec::new();
        let mut buffer = String::new();
        let mut offset = 0usize;
        let mut bytes_stream = response.bytes_stream();

        while let Some(chunk) = bytes_stream.next().await {
            let chunk = chunk?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(rel) = buffer[offset..].find('\n') {
                let end = offset + rel;
                let line = buffer[offset..end].trim().to_owned();
                offset = end + 1;
                if let Some(data) = line.strip_prefix("data: ") {
                    process_deepseek_line(
                        data,
                        &ui_session_id,
                        &state.session_parents,
                        &mut parse_state,
                        &mut tool_parser,
                        &mut tool_calls,
                    )
                    .await?;
                }
            }
            buffer.drain(..offset);
            offset = 0;
        }

        if let Some(parser) = &mut tool_parser {
            let flush = parser.flush();
            parse_state.text.push_str(&flush.text);
            for parsed in flush.tool_calls {
                tool_calls.push(tool_call_to_message(tool_calls.len(), parsed));
            }
        }

        /* model may emit the call as bare or fenced JSON instead of <tool_call> tags;
        lift those out so the agent receives real tool_calls, not leaked text */
        if tool_calls.is_empty() && body.tools.is_some() {
            let (cleaned, leaked) = extract_tool_calls_from_text(&parse_state.text);
            if !leaked.is_empty() {
                parse_state.text = cleaned;
                for parsed in leaked {
                    tool_calls.push(tool_call_to_message(tool_calls.len(), parsed));
                }
            }
        }

        let output = format!("{}{}", parse_state.reasoning, parse_state.text);
        let usage = usage_from_text(&final_prompt, &output, true);
        let message = if tool_calls.is_empty() {
            json!({
                "role": "assistant",
                "content": parse_state.text,
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

        return Ok(Json(json!({
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
        .into_response());
    }

    let model = body.model.clone();
    let session_parents = state.session_parents.clone();
    let stream = stream! {
        yield Ok::<Bytes, std::convert::Infallible>(sse_json(json!({
            "id": completion_id,
            "object": "chat.completion.chunk",
            "created": current_timestamp(),
            "model": model,
            "choices": [{ "index": 0, "delta": { "role": "assistant", "content": "" }, "logprobs": Value::Null, "finish_reason": Value::Null }]
        })));

        let mut parse_state = DeepSeekParseState::default();
        let mut tool_parser = body.tools.as_ref().map(|_| StreamingToolParser::new());
        let mut tool_index = 0usize;
        let mut buffer = String::new();
        let mut offset = 0usize;
        let mut bytes_stream = response.bytes_stream();

        while let Some(chunk) = bytes_stream.next().await {
            match chunk {
                Ok(chunk) => {
                    buffer.push_str(&String::from_utf8_lossy(&chunk));
                    while let Some(rel) = buffer[offset..].find('\n') {
                        let end = offset + rel;
                        let line = buffer[offset..end].trim().to_owned();
                        offset = end + 1;
                        if let Some(data) = line.strip_prefix("data: ") {
                            match collect_deepseek_events(data, &ui_session_id, &session_parents, &mut parse_state, &mut tool_parser).await {
                                Ok(events) => {
                                    for event in events {
                                        match event {
                                            ParsedEvent::Reasoning(content) => {
                                                yield Ok(sse_json(json!({
                                                    "id": completion_id,
                                                    "object": "chat.completion.chunk",
                                                    "created": current_timestamp(),
                                                    "model": model,
                                                    "choices": [{ "index": 0, "delta": { "reasoning_content": content }, "logprobs": Value::Null, "finish_reason": Value::Null }]
                                                })));
                                            }
                                            ParsedEvent::Text(content) => {
                                                yield Ok(sse_json(json!({
                                                    "id": completion_id,
                                                    "object": "chat.completion.chunk",
                                                    "created": current_timestamp(),
                                                    "model": model,
                                                    "choices": [{ "index": 0, "delta": { "content": content }, "logprobs": Value::Null, "finish_reason": Value::Null }]
                                                })));
                                            }
                                            ParsedEvent::ToolCall(tool_call) => {
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
                                        "choices": [{ "index": 0, "delta": { "content": format!("DeepSeek parse error: {err}") }, "logprobs": Value::Null, "finish_reason": "stop" }]
                                    })));
                                    yield Ok(sse_done());
                                    return;
                                }
                            }
                        }
                    }
                    buffer.drain(..offset);
                    offset = 0;
                }
                Err(err) => {
                    yield Ok(sse_json(json!({
                        "id": completion_id,
                        "object": "chat.completion.chunk",
                        "created": current_timestamp(),
                        "model": model,
                        "choices": [{ "index": 0, "delta": { "content": format!("DeepSeek upstream error: {err}") }, "logprobs": Value::Null, "finish_reason": "stop" }]
                    })));
                    yield Ok(sse_done());
                    return;
                }
            }
        }

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
            for parsed in flush.tool_calls {
                let tool_call = tool_call_to_message(tool_index, parsed);
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

        let usage = usage_from_text(&final_prompt, &format!("{}{}", parse_state.reasoning, parse_state.text), true);
        yield Ok(sse_json(json!({
            "id": completion_id,
            "object": "chat.completion.chunk",
            "created": current_timestamp(),
            "model": model,
            "choices": [{ "index": 0, "delta": {}, "logprobs": Value::Null, "finish_reason": if tool_index == 0 { "stop" } else { "tool_calls" } }],
            "usage": usage,
        })));
        yield Ok(sse_done());
    };

    Ok(stream_response(stream))
}

fn deepen_tool_prompt(base_prompt: String, body: &OpenAIRequest) -> String {
    if body.tools.is_none() {
        return base_prompt;
    }

    let mut prefix = String::from(
        "DEEPSEEK TOOL MODE\n\
These tools are real and executable in this session.\n\
If user asks to test tools, inspect workspace/files, read code, write code, grep, glob, run commands, or understand repository state, do not answer in prose.\n\
You must respond with one or more DSML tool-call blocks only.\n\
Use this exact format:\n\
<｜DSML｜tool_calls>\n\
<｜DSML｜invoke name=\"tool_name\">\n\
<｜DSML｜parameter name=\"param\" string=\"true\">value</｜DSML｜parameter>\n\
</｜DSML｜invoke>\n\
</｜DSML｜tool_calls>\n\
Use string=\"true\" for string arguments and string=\"false\" for JSON numbers, booleans, arrays, or objects.\n\
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
            "You must call tool \"{name}\" in your next response.\n"
        ));
    }

    format!("{prefix}\n{base_prompt}")
}

async fn process_deepseek_line(
    data: &str,
    ui_session_id: &SessionId,
    session_parents: &SessionStore,
    parse_state: &mut DeepSeekParseState,
    tool_parser: &mut Option<StreamingToolParser>,
    tool_calls: &mut Vec<MessageToolCall>,
) -> Result<()> {
    let events = collect_deepseek_events(
        data,
        ui_session_id,
        session_parents,
        parse_state,
        tool_parser,
    )
    .await?;
    for event in events {
        match event {
            ParsedEvent::Reasoning(_) => {}
            ParsedEvent::Text(text) => parse_state.text.push_str(&text),
            ParsedEvent::ToolCall(tool_call) => tool_calls.push(tool_call),
        }
    }
    Ok(())
}

async fn collect_deepseek_events(
    data: &str,
    ui_session_id: &SessionId,
    session_parents: &SessionStore,
    parse_state: &mut DeepSeekParseState,
    tool_parser: &mut Option<StreamingToolParser>,
) -> Result<Vec<ParsedEvent>> {
    if data == "[DONE]" {
        return Ok(Vec::new());
    }

    let chunk: Value = match serde_json::from_str(data) {
        Ok(value) => value,
        Err(_) => return Ok(Vec::new()),
    };

    if let Some(message_id) = chunk
        .get("response_message_id")
        .and_then(Value::as_i64)
        .or_else(|| chunk.get("message_id").and_then(Value::as_i64))
        .or_else(|| {
            chunk
                .get("v")
                .and_then(Value::as_object)
                .and_then(|value| value.get("response"))
                .and_then(Value::as_object)
                .and_then(|response| response.get("message_id"))
                .and_then(Value::as_i64)
        })
        .or_else(|| {
            chunk
                .get("v")
                .and_then(Value::as_object)
                .and_then(|value| value.get("message_id"))
                .and_then(Value::as_i64)
        })
    {
        session_parents.set(
            "deepseek",
            ui_session_id.as_str(),
            &ParentMessageId::from(message_id),
        )?;
    }

    if let Some(path) = chunk.get("p").and_then(Value::as_str) {
        parse_state.current_append_path = path.to_owned();
        if path == "response/accumulated_token_usage" {
            if let Some(tokens) = chunk.get("v").and_then(Value::as_u64) {
                parse_state.completion_tokens = tokens as usize;
            }
        }
    }

    let mut v_str = None;
    if let Some(value) = chunk.get("v") {
        if let Some(text) = value.as_str() {
            v_str = Some(text.to_owned());
        } else if let Some(response) = value
            .get("response")
            .and_then(Value::as_object)
            .and_then(|response| response.get("fragments"))
            .and_then(Value::as_array)
        {
            return Ok(collect_fragment_events(response, parse_state, tool_parser));
        } else if let Some(items) = value.as_array() {
            return Ok(collect_fragment_events(items, parse_state, tool_parser));
        }
    }

    let Some(v_str) = v_str else {
        return Ok(Vec::new());
    };
    if v_str.is_empty() || v_str == "FINISHED" {
        return Ok(Vec::new());
    }

    let is_thinking = parse_state.current_append_path.contains("thinking_content")
        || parse_state.current_append_path.contains("THINK")
        || (parse_state
            .current_append_path
            .contains("fragments/-1/content")
            && parse_state.current_fragment_type == "THINK");

    if is_thinking {
        parse_state.reasoning.push_str(&v_str);
        return Ok(vec![ParsedEvent::Reasoning(v_str)]);
    }

    if let Some(parser) = tool_parser {
        let parsed = parser.feed(&v_str);
        let mut events = Vec::new();
        if !parsed.text.is_empty() {
            events.push(ParsedEvent::Text(parsed.text));
        }
        for tool_call in parsed.tool_calls {
            events.push(ParsedEvent::ToolCall(tool_call_to_message(
                parser.emitted_tool_call_count(),
                tool_call,
            )));
        }
        return Ok(events);
    }

    Ok(vec![ParsedEvent::Text(v_str)])
}

fn tool_call_to_message(
    _index: usize,
    parsed: crate::proxy_core::ParsedToolCall,
) -> MessageToolCall {
    MessageToolCall {
        id: parsed.id,
        tool_type: "function".to_owned(),
        function: ToolCallFunction {
            name: parsed.name,
            arguments: parsed.arguments.to_string(),
        },
    }
}

fn require_api_key(
    headers: &HeaderMap,
    api_key: Option<&str>,
    allow_x_api_key: bool,
) -> Result<(), Box<Response>> {
    let Some(api_key) = api_key else {
        return Ok(());
    };

    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    let provided = bearer.or_else(|| {
        allow_x_api_key
            .then(|| {
                headers
                    .get("x-api-key")
                    .and_then(|value| value.to_str().ok())
            })
            .flatten()
    });

    match provided {
        Some(provided) if constant_time_eq(provided, api_key) => Ok(()),
        _ => Err(Box::new(json_error(
            StatusCode::UNAUTHORIZED,
            "Unauthorized".to_owned(),
        ))),
    }
}

fn json_error(status: StatusCode, message: String) -> Response {
    (status, Json(json!({ "error": { "message": message } }))).into_response()
}

/// Map an upstream/internal error to a generic 502 with an opaque id; log the real
/// cause server-side so upstream bodies / header fragments never reach the client.
fn bad_gateway_error(err: impl std::fmt::Display) -> Response {
    let id = Uuid::new_v4();
    eprintln!("[deepseek] upstream error {id}: {err}");
    json_error(
        StatusCode::BAD_GATEWAY,
        format!("upstream provider error (id={id})"),
    )
}

fn stream_response<S>(stream: S) -> Response
where
    S: futures_util::Stream<Item = Result<Bytes, std::convert::Infallible>> + Send + 'static,
{
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header(header::CONNECTION, "keep-alive")
        .body(Body::from_stream(stream))
        .expect("valid streaming response")
}

