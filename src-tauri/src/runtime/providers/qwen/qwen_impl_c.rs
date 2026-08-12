async fn create_qwen_chat(
    client: &reqwest::Client,
    config: &AppConfig,
    headers: &HashMap<String, String>,
) -> Result<String> {
    let response = client
        .post(format!("{}/api/v2/chats/new", config.qwen_base_url))
        .header("accept", "application/json, text/plain, */*")
        .header("accept-language", "pt-BR,pt;q=0.9")
        .header("content-type", "application/json")
        .header("cookie", headers.get("cookie").cloned().unwrap_or_default())
        .header("origin", &config.qwen_base_url)
        .header("referer", format!("{}/c/new-chat", config.qwen_base_url))
        .header(
            "user-agent",
            headers.get("user-agent").cloned().unwrap_or_default(),
        )
        .header("x-request-id", Uuid::new_v4().to_string())
        .header("bx-v", headers.get("bx-v").cloned().unwrap_or_default())
        .header("bx-ua", headers.get("bx-ua").cloned().unwrap_or_default())
        .header(
            "bx-umidtoken",
            headers.get("bx-umidtoken").cloned().unwrap_or_default(),
        )
        .header("timezone", "UTC")
        .header("version", QWEN_WEB_VERSION)
        .header("source", "web")
        .json(&json!({
            "title": "Nova Conversa",
            "models": ["qwen3.7-plus"],
            "chat_mode": "normal",
            "chat_type": "t2t",
            "timestamp": current_timestamp(),
            "project_id": ""
        }))
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "Qwen create chat error: {} {}",
            response.status(),
            response.text().await.unwrap_or_default()
        ));
    }

    let body: Value = response.json().await?;
    if let Some(message) = extract_qwen_api_error(&body) {
        return Err(anyhow!("Qwen create chat error: {message}"));
    }
    extract_qwen_chat_id(&body)
        .map(str::to_owned)
        .ok_or_else(|| {
            anyhow!(
                "Qwen chat id missing in response: {}",
                truncate_error_payload(&body.to_string(), 400)
            )
        })
}

struct QwenChatRequest<'a> {
    body: &'a OpenAIRequest,
    final_prompt: &'a str,
    completion_id: &'a str,
    chat_id: &'a str,
    parent_id: Option<&'a str>,
    headers: &'a HashMap<String, String>,
    files: &'a [Value],
}

async fn request_qwen_chat(
    state: &AppState,
    request: QwenChatRequest<'_>,
) -> std::result::Result<reqwest::Response, QwenRequestError> {
    let QwenChatRequest {
        body,
        final_prompt,
        completion_id,
        chat_id,
        parent_id,
        headers,
        files,
    } = request;
    let model = normalize_model_id(&body.model);
    let parent_id = parent_id.map(Value::from).unwrap_or(Value::Null);
    let payload = json!({
        "stream": true,
        "version": "2.1",
        "incremental_output": true,
        "chat_id": chat_id,
        "chat_mode": "normal",
        "model": model,
        "parent_id": parent_id,
        "messages": [{
            "fid": Uuid::new_v4().to_string(),
            "parentId": parent_id,
            "childrenIds": [],
            "role": "user",
            "content": final_prompt,
            "user_action": "chat",
            "files": files,
            "timestamp": current_timestamp(),
            "models": [model],
            "chat_type": "t2t",
            "feature_config": {
                "thinking_enabled": !body.model.contains("no-thinking"),
                "output_schema": "phase",
                "research_mode": "normal",
                "auto_thinking": false,
                "thinking_mode": "Thinking",
                "thinking_format": "summary",
                "auto_search": body.web_search.unwrap_or(false)
            },
            "extra": { "meta": { "subChatType": "t2t" } },
            "sub_chat_type": "t2t",
            "parent_id": parent_id
        }],
        "timestamp": current_timestamp() + 1
    });

    let payload_json = payload.to_string();
    if payload_json.len() > MAX_PAYLOAD_SIZE {
        return Err(QwenRequestError {
            message: format!(
                "payload too large: {} bytes exceeds limit of {} bytes",
                payload_json.len(),
                MAX_PAYLOAD_SIZE
            ),
            upstream_code: None,
            upstream_status: Some(413),
            retry_after_ms: None,
            retryable: false,
        });
    }

    state
        .traces
        .record(completion_id, "upstream_request", payload_json.clone())
        .await;

    let response = state
        .client
        .post(format!(
            "{}/api/v2/chat/completions?chat_id={chat_id}",
            state.config.qwen_base_url
        ))
        .header("accept", "application/json")
        .header("accept-language", "pt-BR,pt;q=0.9")
        .header("content-type", "application/json")
        .header("cookie", headers.get("cookie").cloned().unwrap_or_default())
        .header("origin", &state.config.qwen_base_url)
        .header(
            "referer",
            format!("{}/c/{chat_id}", state.config.qwen_base_url),
        )
        .header("sec-fetch-dest", "empty")
        .header("sec-fetch-mode", "cors")
        .header("sec-fetch-site", "same-origin")
        .header("timezone", "UTC")
        .header(
            "user-agent",
            headers.get("user-agent").cloned().unwrap_or_default(),
        )
        .header("x-accel-buffering", "no")
        .header("x-request-id", Uuid::new_v4().to_string())
        .header("bx-v", headers.get("bx-v").cloned().unwrap_or_default())
        .header("bx-ua", headers.get("bx-ua").cloned().unwrap_or_default())
        .header(
            "bx-umidtoken",
            headers.get("bx-umidtoken").cloned().unwrap_or_default(),
        )
        .header("version", QWEN_WEB_VERSION)
        .header("source", "web")
        .body(payload_json)
        .send()
        .await
        .map_err(|err| {
            let message = err.to_string();
            QwenRequestError {
                retryable: err.is_connect()
                    || err.is_timeout()
                    || is_retryable_transport_message(&message),
                message,
                upstream_code: None,
                upstream_status: Some(502),
                retry_after_ms: None,
            }
        })?;

    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if !response.status().is_success() || !content_type.contains("text/event-stream") {
        let status = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();
        state
            .traces
            .record(completion_id, "upstream_response", text.clone())
            .await;

        if let Ok(json) = serde_json::from_str::<Value>(&text) {
            if json.get("success").and_then(Value::as_bool) == Some(false) {
                let code = json
                    .get("data")
                    .and_then(Value::as_object)
                    .and_then(|data| data.get("code"))
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                let details = json
                    .get("data")
                    .and_then(Value::as_object)
                    .and_then(|data| data.get("details"))
                    .and_then(Value::as_str)
                    .or_else(|| json.get("message").and_then(Value::as_str))
                    .unwrap_or("Qwen returned an error");
                let retry_after_ms = if code.as_deref() == Some("RateLimited") {
                    json.pointer("/data/num")
                        .and_then(Value::as_u64)
                        .and_then(|hours| hours.checked_mul(60 * 60 * 1_000))
                } else if details.contains("chat is in progress") {
                    Some(2_500)
                } else {
                    None
                };
                return Err(QwenRequestError {
                    message: format!(
                        "Qwen upstream error: {}: {}",
                        code.clone().unwrap_or_else(|| "UpstreamError".to_owned()),
                        details
                    ),
                    upstream_code: code,
                    upstream_status: Some(if status == 0 { 502 } else { status }),
                    retry_after_ms,
                    retryable: false,
                });
            }
        }

        return Err(QwenRequestError {
            message: format!("Qwen returned non-stream response: {status} {text}"),
            upstream_code: None,
            upstream_status: Some(status.max(502)),
            retry_after_ms: None,
            retryable: false,
        });
    }

    Ok(response)
}

async fn stop_upstream_generation(
    state: &AppState,
    target: &stream_registry::ActiveStreamHandle,
    response_id: &str,
) -> Result<()> {
    let response = state
        .client
        .post(format!(
            "{}/api/v2/chat/completions/stop?chat_id={}",
            state.config.qwen_base_url, target.snapshot.chat_id
        ))
        .header("accept", "application/json, text/plain, */*")
        .header("accept-language", "pt-BR,pt;q=0.9")
        .header("content-type", "application/json")
        .header(
            "cookie",
            target.headers.get("cookie").cloned().unwrap_or_default(),
        )
        .header("origin", &state.config.qwen_base_url)
        .header(
            "referer",
            format!(
                "{}/c/{}",
                state.config.qwen_base_url, target.snapshot.chat_id
            ),
        )
        .header(
            "user-agent",
            target
                .headers
                .get("user-agent")
                .cloned()
                .unwrap_or_default(),
        )
        .header("x-request-id", Uuid::new_v4().to_string())
        .header(
            "bx-ua",
            target.headers.get("bx-ua").cloned().unwrap_or_default(),
        )
        .header(
            "bx-umidtoken",
            target
                .headers
                .get("bx-umidtoken")
                .cloned()
                .unwrap_or_default(),
        )
        .header(
            "bx-v",
            target.headers.get("bx-v").cloned().unwrap_or_default(),
        )
        .json(&json!({
            "chat_id": target.snapshot.chat_id,
            "response_id": response_id,
        }))
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "stop generation failed: {} {}",
            response.status(),
            response.text().await.unwrap_or_default()
        ));
    }

    Ok(())
}

async fn collect_qwen_events(
    data: &str,
    completion_id: &str,
    stream_registry: &StreamRegistry,
    parse_state: &mut QwenParseState,
    tool_parser: &mut Option<StreamingToolParser>,
) -> Result<Vec<QwenEvent>> {
    if data == "[DONE]" {
        return Ok(Vec::new());
    }

    let chunk: Value = match serde_json::from_str(data) {
        Ok(value) => value,
        Err(_) => return Ok(Vec::new()),
    };
    if let Some(error) = extract_qwen_api_error(&chunk) {
        return Err(anyhow!("Qwen upstream stream error: {error}"));
    }

    if let Some(response_id) = chunk
        .get("response.created")
        .and_then(Value::as_object)
        .and_then(|created| created.get("response_id"))
        .and_then(Value::as_str)
        .or_else(|| chunk.get("response_id").and_then(Value::as_str))
    {
        parse_state
            .target_response_id
            .get_or_insert_with(|| response_id.to_owned());
        stream_registry
            .update_response_id(completion_id, response_id.to_owned())
            .await;
    }

    if let Some(usage) = chunk.get("usage").and_then(Value::as_object) {
        if let Some(input) = usage.get("input_tokens").and_then(Value::as_u64) {
            parse_state.prompt_tokens = input as usize;
        }
        if let Some(output) = usage.get("output_tokens").and_then(Value::as_u64) {
            parse_state.completion_tokens = output as usize;
        }
    }

    let delta = chunk
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(Value::as_object)
        .and_then(|choice| choice.get("delta"))
        .and_then(Value::as_object);

    let Some(delta) = delta else {
        return Ok(Vec::new());
    };

    if parse_state.target_response_id.is_some()
        && chunk.get("response_id").and_then(Value::as_str)
            != parse_state.target_response_id.as_deref()
        && chunk.get("response.created").is_none()
    {
        return Ok(Vec::new());
    }

    let phase = delta
        .get("phase")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if phase == "thinking_summary" {
        if let Some(content) = delta
            .get("extra")
            .and_then(Value::as_object)
            .and_then(|extra| extra.get("summary_thought"))
            .and_then(Value::as_object)
            .and_then(|summary| summary.get("content"))
            .and_then(Value::as_array)
        {
            if content.len() > parse_state.current_thought_index {
                let append = content[parse_state.current_thought_index..]
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join("\n");
                parse_state.current_thought_index = content.len();
                if !append.is_empty() {
                    parse_state.reasoning.push_str(&append);
                    return Ok(vec![QwenEvent::Reasoning(append)]);
                }
            }
        }
        return Ok(Vec::new());
    }

    let Some(content) = extract_qwen_delta_text(delta.get("content")) else {
        return Ok(Vec::new());
    };

    let incremental = if content.starts_with(&parse_state.last_full_content) {
        content[parse_state.last_full_content.len()..].to_owned()
    } else {
        content.to_owned()
    };
    parse_state.last_full_content = content.to_owned();

    if incremental.is_empty() {
        return Ok(Vec::new());
    }

    if let Some(parser) = tool_parser {
        let parsed = parser.feed(&incremental);
        let mut events = Vec::new();
        if !parsed.text.is_empty() {
            events.push(QwenEvent::Text(parsed.text));
        }
        for parsed_tool in parsed.tool_calls {
            events.push(QwenEvent::ToolCall(tool_call_from_parsed(parsed_tool)));
        }
        return Ok(events);
    }

    Ok(vec![QwenEvent::Text(incremental)])
}

fn qwen_sse_data(line: &str) -> Option<&str> {
    let data = line.trim().strip_prefix("data:")?.trim_start();
    (!data.is_empty()).then_some(data)
}

fn extract_qwen_delta_text(value: Option<&Value>) -> Option<String> {
    extract_qwen_delta_text_depth(value, 0)
}

fn extract_qwen_delta_text_depth(value: Option<&Value>, depth: usize) -> Option<String> {
    // ponytail: cap recursion at 64 to block stack-overflow via pathological upstream JSON.
    const MAX_DEPTH: usize = 64;
    let value = value?;
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => {
            let text = items
                .iter()
                .filter_map(|item| extract_qwen_delta_text_depth(Some(item), depth + 1))
                .collect::<Vec<_>>()
                .join("");
            (!text.is_empty()).then_some(text)
        }
        Value::Object(map) => {
            if depth >= MAX_DEPTH {
                return None;
            }
            if let Some(text) = map.get("text").and_then(Value::as_str) {
                return Some(text.to_owned());
            }
            if let Some(text) = map.get("content").and_then(Value::as_str) {
                return Some(text.to_owned());
            }
            if let Some(text) = map.get("value").and_then(Value::as_str) {
                return Some(text.to_owned());
            }
            if let Some(text) = map
                .get("answer")
                .and_then(|answer| extract_qwen_delta_text_depth(Some(answer), depth + 1))
            {
                return Some(text);
            }
            if let Some(text) = map
                .get("parts")
                .and_then(|parts| extract_qwen_delta_text_depth(Some(parts), depth + 1))
            {
                return Some(text);
            }
            None
        }
        _ => None,
    }
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

fn effective_accounts(store: &AccountStore) -> Result<Vec<QwenAccount>> {
    let accounts = store.list_accounts()?;
    if accounts.is_empty() {
        Ok(vec![global_account()])
    } else {
        Ok(accounts)
    }
}

fn account_id_for_bridge(account: &QwenAccount) -> Option<String> {
    (account.id != "global").then(|| account.id.clone())
}

fn qwen_session_key(user: Option<&str>) -> String {
    user.filter(|value| !value.trim().is_empty())
        .map(str::trim)
        .unwrap_or("default")
        .to_owned()
}

async fn pick_capture_headers_for_aux_request(
    state: &AppState,
) -> Result<(QwenAccount, HashMap<String, String>)> {
    let accounts = effective_accounts(&state.accounts)?;
    let account = state
        .account_manager
        .select_next(&accounts, false)
        .await
        .unwrap_or_else(global_account);
    let headers = state
        .bridge
        .capture_headers(CaptureHeadersParams {
            force_new: false,
            account_id: account_id_for_bridge(&account),
        })
        .await?
        .headers;
    Ok((account, headers))
}

fn extract_qwen_chat_id(body: &Value) -> Option<&str> {
    body.get("chat_id")
        .and_then(Value::as_str)
        .or_else(|| body.get("id").and_then(Value::as_str))
        .or_else(|| body.pointer("/data/chat_id").and_then(Value::as_str))
        .or_else(|| body.pointer("/data/id").and_then(Value::as_str))
        .or_else(|| body.pointer("/chat/chat_id").and_then(Value::as_str))
        .or_else(|| body.pointer("/chat/id").and_then(Value::as_str))
        .or_else(|| body.pointer("/data/chat/chat_id").and_then(Value::as_str))
        .or_else(|| body.pointer("/data/chat/id").and_then(Value::as_str))
}

fn extract_qwen_api_error(body: &Value) -> Option<String> {
    let success = body.get("success").and_then(Value::as_bool);
    let code = body
        .pointer("/data/code")
        .or_else(|| body.get("code"))
        .and_then(Value::as_str);
    let details = body
        .pointer("/data/details")
        .or_else(|| body.get("message"))
        .and_then(Value::as_str);

    if success == Some(false) || code.is_some() || details.is_some() {
        Some(match (code, details) {
            (Some(code), Some(details)) => format!("{code}: {details}"),
            (Some(code), None) => code.to_owned(),
            (None, Some(details)) => details.to_owned(),
            (None, None) => "unknown upstream error".to_owned(),
        })
    } else {
        None
    }
}

fn truncate_error_payload(text: &str, max_len: usize) -> String {
    crate::proxy_core::truncate_error_payload(text, max_len)
}

fn normalize_request(request: &OpenAIRequest) -> (OpenAIRequest, Vec<MediaUploadInput>) {
    let mut normalized = request.clone();
    let mut uploads = Vec::new();

    for message in &mut normalized.messages {
        let Some(Value::Array(items)) = message.content.clone() else {
            continue;
        };

        let mut text_parts = Vec::new();
        for item in items {
            if let Some(text) = item.as_str() {
                text_parts.push(text.to_owned());
                continue;
            }
            let Some(object) = item.as_object() else {
                continue;
            };
            match object.get("type").and_then(Value::as_str) {
                Some("text") => {
                    if let Some(text) = object.get("text").and_then(Value::as_str) {
                        text_parts.push(text.to_owned());
                    }
                }
                Some("image_url") => collect_upload(&mut uploads, "image_url", object, "image_url"),
                Some("video_url") => collect_upload(&mut uploads, "video_url", object, "video_url"),
                Some("audio_url") => collect_upload(&mut uploads, "audio_url", object, "audio_url"),
                Some("file_url") => collect_upload(&mut uploads, "file_url", object, "file_url"),
                _ => {}
            }
        }

        message.content = Some(Value::String(text_parts.join("\n")));
    }

    (normalized, uploads)
}

fn collect_upload(
    uploads: &mut Vec<MediaUploadInput>,
    kind: &str,
    object: &serde_json::Map<String, Value>,
    field: &str,
) {
    if let Some(url) = object
        .get(field)
        .and_then(Value::as_object)
        .and_then(|value| value.get("url"))
        .and_then(Value::as_str)
    {
        uploads.push(MediaUploadInput {
            kind: kind.to_owned(),
            url: url.to_owned(),
        });
    }
}

fn require_api_key(headers: &HeaderMap, api_key: Option<&str>) -> Result<(), Box<Response>> {
    let Some(api_key) = api_key else {
        return Ok(());
    };
    let provided = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    match provided {
        Some(provided) if constant_time_eq(provided, api_key) => Ok(()),
        _ => Err(Box::new(json_error(
            StatusCode::UNAUTHORIZED,
            "Missing or invalid Authorization header".to_owned(),
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
    eprintln!("[qwen] upstream error {id}: {err}");
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
        .header(header::CACHE_CONTROL, "no-cache, no-transform")
        .header(header::CONNECTION, "keep-alive")
        .header("x-accel-buffering", "no")
        .body(Body::from_stream(stream))
        .expect("valid streaming response")
}

pub async fn serve_embedded(
    config: AppConfig,
    helper_dir: std::path::PathBuf,
    node_path: Option<std::path::PathBuf>,
) -> Result<()> {
    ensure_runtime_layout(&config)?;

    let workspace_root = workspace_root();
    let accounts = AccountStore::new(
        config.db_path.clone(),
        &legacy_db_candidates(&workspace_root),
        &legacy_accounts_json_candidates(&workspace_root),
    )?;
    let metrics = Metrics::new().await;
    let cache = MemoryCache::new(config.cache.default_ttl, 10_000, metrics.clone());
    let model_registry = ModelRegistry::new().await;
    let stream_registry = StreamRegistry::new();
    let conversations = ConversationRegistry::open(&config.db_path)?;
    let watchdog = Watchdog::start(
        config.watchdog.clone(),
        metrics.clone(),
        stream_registry.clone(),
        cache.clone(),
        config.chat_timeout,
    );

    let bridge = Arc::new(PlaywrightBridge::new_with_node(&helper_dir, node_path, "qwen").await?);

    run_server(
        bridge,
        config,
        ServerRuntime {
            accounts,
            metrics,
            cache,
            model_registry,
            stream_registry,
            conversations,
            watchdog,
        },
    )
    .await
}

