use super::{
    anthropic_message_to_openai_messages, anthropic_tool_choice_to_openai,
    anthropic_tools_from_value, browser_provider_metadata, browser_request_model,
    browser_request_preflight, browser_tool_system_instructions, coerce_agent_model,
    fallback_model_payload_for, live_chatgpt_web_stream_requested, model_discovery_timeout,
    model_payload_item, openai_tools_from_value, parse_browser_output, require_api_key,
    response_input_to_messages, BridgeChatResponse, BrowserProviderKind,
};
use crate::proxy_core::{FunctionToolDefinition, FunctionToolSpec, Message, OpenAIRequest};
use axum::http::{header, HeaderMap, HeaderValue};
use serde_json::{json, Value};

#[test]
fn openai_request_deserializes_chatgpt_mode() {
    let request: OpenAIRequest = serde_json::from_value(json!({
        "model": "chatgpt:gpt-5-3",
        "messages": [],
        "chatgpt_mode": "web"
    }))
    .expect("request");

    assert_eq!(request.chatgpt_mode.as_deref(), Some("web"));
}

#[test]
fn chatgpt_advertises_web_search_support() {
    assert!(BrowserProviderKind::Chatgpt.web_search_supported());
    assert!(!BrowserProviderKind::Gemini.web_search_supported());
}

#[test]
fn live_streaming_only_uses_the_chatgpt_web_lane() {
    let web = OpenAIRequest {
        model: "chatgpt-web-session".to_owned(),
        messages: Vec::new(),
        stream: Some(true),
        web_search: None,
        chatgpt_mode: None,
        user: None,
        tools: None,
        tool_choice: None,
        stream_options: None,
    };
    assert!(live_chatgpt_web_stream_requested(
        BrowserProviderKind::Chatgpt,
        &web
    ));
    assert!(!live_chatgpt_web_stream_requested(
        BrowserProviderKind::Gemini,
        &web
    ));

    let codex = OpenAIRequest {
        model: "gpt-5.4-mini".to_owned(),
        chatgpt_mode: Some("codex".to_owned()),
        ..web
    };
    assert!(!live_chatgpt_web_stream_requested(
        BrowserProviderKind::Chatgpt,
        &codex
    ));
}

#[test]
fn chatgpt_model_payload_marks_codex_billing_and_tools() {
    let item = model_payload_item(BrowserProviderKind::Chatgpt, "gpt-5.3-codex", json!({}));

    assert_eq!(item["api"], "codex_responses");
    assert_eq!(item["billing"], "Codex billing usage");
    assert_eq!(item["tool_call"], true);
    assert_eq!(
        item["description"],
        "Uses Codex OAuth Responses API; usage is billed/limited as Codex usage."
    );
}

#[test]
fn chatgpt_model_payload_marks_web_chat_completions() {
    let item = model_payload_item(BrowserProviderKind::Chatgpt, "gpt-5-3", json!({}));

    assert_eq!(item["api"], "chat_completions");
    assert_eq!(item["billing"], "ChatGPT subscription/web-session usage");
    assert_eq!(item["tool_call"], true);
    assert_eq!(
        item["description"],
        "Uses Chat Completions API compatibility through the ChatGPT web session."
    );
}

#[test]
fn browser_model_fallback_uses_provider_default() {
    let payload = fallback_model_payload_for(
        BrowserProviderKind::Mistral,
        vec!["discovery failed".to_owned()],
    );

    let first = payload
        .get("data")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .expect("fallback model item");
    assert_eq!(
        first.get("id").and_then(Value::as_str),
        Some("mistral-web-session")
    );
    assert_eq!(
        first.get("owned_by").and_then(Value::as_str),
        Some("mistral")
    );
    assert_eq!(
        payload
            .get("errors")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(Value::as_str),
        Some("discovery failed")
    );
}

#[test]
fn chatgpt_model_discovery_allows_oauth_refresh() {
    assert_eq!(
        model_discovery_timeout(BrowserProviderKind::Chatgpt),
        std::time::Duration::from_secs(15)
    );
    assert_eq!(
        model_discovery_timeout(BrowserProviderKind::Gemini),
        std::time::Duration::from_secs(4)
    );
}

#[test]
fn browser_model_fallback_uses_zai_default() {
    let payload = fallback_model_payload_for(BrowserProviderKind::Zai, Vec::new());

    let first = payload
        .get("data")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .expect("fallback model item");
    assert_eq!(first.get("id").and_then(Value::as_str), Some("glm-5.2"));
    assert_eq!(first.get("owned_by").and_then(Value::as_str), Some("zai"));
}

#[test]
fn browser_model_fallback_uses_meta_default() {
    let payload = fallback_model_payload_for(BrowserProviderKind::Meta, Vec::new());

    let first = payload
        .get("data")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .expect("fallback model item");
    assert_eq!(
        first.get("id").and_then(Value::as_str),
        Some("meta-ai-web-session")
    );
    assert_eq!(first.get("owned_by").and_then(Value::as_str), Some("meta"));
}

#[test]
fn browser_tool_system_instructions_tell_tools_they_are_real() {
    let request = OpenAIRequest {
        model: "chatgpt-web-session".to_owned(),
        messages: vec![],
        stream: Some(false),
        web_search: Some(false),
        chatgpt_mode: Some("web".to_owned()),
        user: None,
        tools: Some(vec![FunctionToolDefinition {
            tool_type: "function".to_owned(),
            function: Some(FunctionToolSpec {
                name: "bash".to_owned(),
                description: Some("Run command".to_owned()),
                parameters: None,
                strict: None,
            }),
            name: None,
            description: None,
        }]),
        tool_choice: Some(json!("required")),
        stream_options: None,
    };
    let instructions = browser_tool_system_instructions(&request).expect("tool instructions");

    assert!(instructions.contains("These tools are real and executable"));
    assert!(instructions.contains("tool_choice is required"));
}

#[test]
fn browser_request_preflight_keeps_tool_mode_in_system_lane() {
    let request = OpenAIRequest {
        model: "chatgpt-web-session".to_owned(),
        messages: vec![Message {
            role: "user".to_owned(),
            content: Some(Value::String("Test your tools.".to_owned())),
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning_content: None,
        }],
        stream: Some(false),
        web_search: Some(false),
        chatgpt_mode: Some("web".to_owned()),
        user: None,
        tools: Some(vec![FunctionToolDefinition {
            tool_type: "function".to_owned(),
            function: Some(FunctionToolSpec {
                name: "bash".to_owned(),
                description: Some("Run command".to_owned()),
                parameters: None,
                strict: None,
            }),
            name: None,
            description: None,
        }]),
        tool_choice: Some(json!("required")),
        stream_options: None,
    };
    let preflight = browser_request_preflight(&request).expect("preflight");

    assert!(preflight.system_prompt.contains("BROWSER TOOL MODE"));
    assert!(preflight
        .system_prompt
        .contains("These tools are real and executable"));
    assert!(preflight
        .system_prompt
        .contains("You must respond with one or more <tool_call>"));
    assert!(preflight.system_prompt.contains("tool_choice is required"));
    assert!(preflight.system_prompt.contains("# TOOLS AVAILABLE"));
    assert!(!preflight.conversation.contains("BROWSER TOOL MODE"));
    assert!(preflight.conversation.contains("User: Test your tools."));
}

#[test]
fn browser_parser_lifts_fenced_json_tool_call_like_deepseek() {
    let parsed = parse_browser_output(
        &OpenAIRequest {
            model: "chatgpt-web-session".to_owned(),
            messages: Vec::new(),
            stream: Some(false),
            web_search: Some(false),
            chatgpt_mode: None,
            user: None,
            tools: Some(vec![FunctionToolDefinition {
                tool_type: "function".to_owned(),
                function: Some(FunctionToolSpec {
                    name: "bash".to_owned(),
                    description: None,
                    parameters: None,
                    strict: None,
                }),
                name: None,
                description: None,
            }]),
            tool_choice: None,
            stream_options: None,
        },
        r#"```json
{"name":"bash","arguments":{"cmd":"pwd"}}
```"#,
    );

    assert_eq!(parsed.text, "");
    assert_eq!(parsed.tool_calls.len(), 1);
    assert_eq!(parsed.tool_calls[0].function.name, "bash");
}

#[test]
fn browser_parser_extracts_tool_calls_from_markup() {
    let parsed = parse_browser_output(
        &OpenAIRequest {
            model: "chatgpt-web-session".to_owned(),
            messages: Vec::new(),
            stream: Some(false),
            web_search: Some(false),
            chatgpt_mode: None,
            user: None,
            tools: Some(vec![FunctionToolDefinition {
                tool_type: "function".to_owned(),
                function: Some(FunctionToolSpec {
                    name: "lookup".to_owned(),
                    description: None,
                    parameters: None,
                    strict: None,
                }),
                name: None,
                description: None,
            }]),
            tool_choice: None,
            stream_options: None,
        },
        "before <tool_call>{\"name\":\"lookup\",\"arguments\":{\"id\":7}}</tool_call>",
    );

    assert_eq!(parsed.text, "before");
    assert_eq!(parsed.tool_calls.len(), 1);
    assert_eq!(parsed.tool_calls[0].function.name, "lookup");
}

#[test]
fn browser_parser_lifts_kilo_command_objects_into_tool_calls() {
    let parsed = parse_browser_output(
        &OpenAIRequest {
            model: "chatgpt-web-session".to_owned(),
            messages: Vec::new(),
            stream: Some(false),
            web_search: Some(false),
            chatgpt_mode: None,
            user: None,
            tools: Some(vec![FunctionToolDefinition {
                tool_type: "function".to_owned(),
                function: Some(FunctionToolSpec {
                    name: "execute_command".to_owned(),
                    description: None,
                    parameters: None,
                    strict: None,
                }),
                name: None,
                description: None,
            }]),
            tool_choice: None,
            stream_options: None,
        },
        r#"{"command":"git ls-files | wc -l","description":"Count tracked files","workdir":"/repo"} {"command":"ls -la","description":"List root","workdir":"/repo"}"#,
    );

    assert!(parsed.text.is_empty());
    assert_eq!(parsed.tool_calls.len(), 2);
    assert_eq!(parsed.tool_calls[0].function.name, "execute_command");
    assert!(parsed.tool_calls[0]
        .function
        .arguments
        .contains("git ls-files"));
    assert!(parsed.tool_calls[1].function.arguments.contains("ls -la"));
}

#[test]
fn api_key_accepts_x_api_key_header() {
    let mut headers = HeaderMap::new();
    headers.insert("x-api-key", HeaderValue::from_static("local"));
    assert!(require_api_key(&headers, Some("local")).is_ok());

    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_static("Bearer nope"),
    );
    assert!(require_api_key(&headers, Some("local")).is_ok());
}

#[test]
fn claude_model_names_fall_back_to_provider_default() {
    assert_eq!(
        coerce_agent_model(BrowserProviderKind::Chatgpt, "claude-sonnet-4-5"),
        "gpt-5.4-mini"
    );
}

#[test]
fn anthropic_tools_convert_to_openai_function_tools() {
    let tools = anthropic_tools_from_value(Some(&json!([{
        "name": "read_file",
        "description": "Read file",
        "input_schema": { "type": "object" }
    }])))
    .expect("tools");

    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].tool_name(), Some("read_file"));
}

#[test]
fn anthropic_messages_preserve_tool_use_and_tool_results() {
    let assistant = anthropic_message_to_openai_messages(&json!({
        "role": "assistant",
        "content": [
            { "type": "text", "text": "checking" },
            { "type": "tool_use", "id": "toolu_1", "name": "read_file", "input": { "path": "Cargo.toml" } }
        ]
    }));
    let user = anthropic_message_to_openai_messages(&json!({
        "role": "user",
        "content": [
            { "type": "tool_result", "tool_use_id": "toolu_1", "content": "ok" }
        ]
    }));

    assert_eq!(assistant.len(), 1);
    assert_eq!(assistant[0].role, "assistant");
    assert_eq!(
        assistant[0].tool_calls.as_ref().unwrap()[0].function.name,
        "read_file"
    );
    assert_eq!(user.len(), 1);
    assert_eq!(user[0].role, "tool");
    assert_eq!(user[0].tool_call_id.as_deref(), Some("toolu_1"));
}

#[test]
fn anthropic_tool_results_preserve_structured_content() {
    let user = anthropic_message_to_openai_messages(&json!({
        "role": "user",
        "content": [
            { "type": "tool_result", "tool_use_id": "toolu_1", "content": { "rows": [1, 2, 3], "status": "ok" } }
        ]
    }));

    assert_eq!(user.len(), 1);
    assert_eq!(user[0].role, "tool");
    assert_eq!(
        user[0].content,
        Some(json!({ "rows": [1, 2, 3], "status": "ok" }))
    );
}

#[test]
fn anthropic_any_tool_choice_maps_to_openai_required() {
    assert_eq!(
        anthropic_tool_choice_to_openai(Some(&json!({ "type": "any" }))),
        Some(json!("required"))
    );
    assert_eq!(
        anthropic_tool_choice_to_openai(Some(&json!("any"))),
        Some(json!("required"))
    );
}

#[test]
fn responses_input_preserves_function_call_outputs() {
    let messages = response_input_to_messages(Some(&json!([
        { "role": "user", "content": "read it" },
        { "type": "function_call", "call_id": "call_1", "name": "read_file", "arguments": "{\"path\":\"Cargo.toml\"}" },
        { "type": "function_call_output", "call_id": "call_1", "output": "ok" }
    ])));

    assert_eq!(messages.len(), 3);
    assert_eq!(messages[1].role, "assistant");
    assert_eq!(
        messages[1].tool_calls.as_ref().unwrap()[0].function.name,
        "read_file"
    );
    assert_eq!(messages[2].role, "tool");
    assert_eq!(messages[2].tool_call_id.as_deref(), Some("call_1"));
}

#[test]
fn responses_input_preserves_structured_function_call_outputs() {
    let messages = response_input_to_messages(Some(&json!([
        { "role": "user", "content": "read it" },
        { "type": "function_call", "call_id": "call_1", "name": "read_file", "arguments": "{\"path\":\"Cargo.toml\"}" },
        { "type": "function_call_output", "call_id": "call_1", "output": { "summary": "ok", "items": [1, 2, 3] } }
    ])));

    assert_eq!(messages.len(), 3);
    assert_eq!(messages[2].role, "tool");
    assert_eq!(
        messages[2].content,
        Some(json!({ "summary": "ok", "items": [1, 2, 3] }))
    );
}

#[test]
fn responses_flat_function_tools_convert_to_chat_tools() {
    let tools = openai_tools_from_value(Some(&json!([{
        "type": "function",
        "name": "read_file",
        "description": "Read file",
        "parameters": { "type": "object" },
        "strict": true
    }])))
    .expect("tools");

    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].tool_name(), Some("read_file"));
    assert_eq!(tools[0].function.as_ref().unwrap().strict, Some(true));
}

#[test]
fn anthropic_tool_choice_auto_stays_auto() {
    // Anthropic "auto" → OpenAI "auto"
    assert_eq!(
        anthropic_tool_choice_to_openai(Some(&json!({ "type": "auto" }))),
        Some(json!("auto"))
    );
}

#[test]
fn anthropic_tool_choice_specific_maps_to_named_function() {
    let result = anthropic_tool_choice_to_openai(Some(&json!({
        "type": "tool",
        "name": "read_file"
    })));
    let obj = result.expect("some value").as_object().cloned().unwrap();
    assert_eq!(obj["type"], "function");
    assert_eq!(obj["function"]["name"], "read_file");
}

#[test]
fn anthropic_tool_choice_none_object_passes_through_as_is() {
    // type="none" as an object isn't a matched arm — falls through to Some(other.clone())
    let result = anthropic_tool_choice_to_openai(Some(&json!({ "type": "none" })));
    assert_eq!(result, Some(json!({ "type": "none" })));
}

#[test]
fn anthropic_tool_choice_none_string_maps_correctly() {
    // string "none" is matched and returned as-is
    assert_eq!(
        anthropic_tool_choice_to_openai(Some(&json!("none"))),
        Some(json!("none"))
    );
}

#[test]
fn anthropic_tool_choice_absent_returns_option_none() {
    assert_eq!(anthropic_tool_choice_to_openai(None), None);
}

#[test]
fn coerce_agent_model_passes_through_native_provider_model() {
    // A model already native to the provider should pass through unchanged
    assert_eq!(
        coerce_agent_model(BrowserProviderKind::Chatgpt, "chatgpt-web-session"),
        "chatgpt-web-session"
    );
    assert_eq!(
        coerce_agent_model(BrowserProviderKind::Mistral, "mistral-web-session"),
        "mistral-web-session"
    );
}

#[test]
fn parse_browser_output_plain_text_no_tools() {
    let parsed = parse_browser_output(
        &OpenAIRequest {
            model: "chatgpt-web-session".to_owned(),
            messages: Vec::new(),
            stream: None,
            web_search: None,
            chatgpt_mode: None,
            user: None,
            tools: None,
            tool_choice: None,
            stream_options: None,
        },
        "just some plain text response",
    );
    assert_eq!(parsed.text.trim(), "just some plain text response");
    assert!(parsed.tool_calls.is_empty());
}

#[test]
fn browser_request_model_uses_provider_default_when_missing() {
    assert_eq!(
        browser_request_model(BrowserProviderKind::Chatgpt, ""),
        BrowserProviderKind::Chatgpt.default_model()
    );
    assert_eq!(
        browser_request_model(BrowserProviderKind::Chatgpt, "chatgpt-web-session"),
        "chatgpt-web-session"
    );
}

#[test]
fn anthropic_tools_empty_array_returns_some_empty_vec() {
    // Empty array → Some([]) rather than None (the array exists, just has no items)
    let result = anthropic_tools_from_value(Some(&json!([])));
    assert!(result.is_some_and(|v| v.is_empty()));
}

#[test]
fn anthropic_tools_missing_field_returns_none() {
    assert!(anthropic_tools_from_value(None).is_none());
}

#[test]
fn openai_tools_none_returns_none() {
    assert!(openai_tools_from_value(None).is_none());
}

#[test]
fn openai_tools_empty_array_returns_some_empty_vec() {
    // Empty array → Some([]) — callers must handle empty-but-present
    let result = openai_tools_from_value(Some(&json!([])));
    assert!(result.is_some_and(|v| v.is_empty()));
}

#[test]
fn fallback_payload_chatgpt_uses_codex_model_id() {
    let payload = fallback_model_payload_for(BrowserProviderKind::Chatgpt, Vec::new());
    let first = payload["data"]
        .as_array()
        .and_then(|a| a.first())
        .expect("model item");
    assert_eq!(first["id"], "gpt-5.4-mini");
    assert_eq!(first["owned_by"], "chatgpt");
}

#[test]
fn fallback_payload_gemini_uses_web_session_id() {
    let payload = fallback_model_payload_for(BrowserProviderKind::Gemini, Vec::new());
    let first = payload["data"]
        .as_array()
        .and_then(|a| a.first())
        .expect("model item");
    assert_eq!(first["id"], "gemini-web-session");
}

#[test]
fn anthropic_message_plain_string_content_becomes_user_message() {
    let msgs = anthropic_message_to_openai_messages(&json!({
        "role": "user",
        "content": "simple text"
    }));
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].role, "user");
}

#[test]
fn anthropic_message_image_block_preserved_as_json_text() {
    // image_url blocks that aren't text/tool_use should still produce a message
    let msgs = anthropic_message_to_openai_messages(&json!({
        "role": "user",
        "content": [
            { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
        ]
    }));
    assert!(!msgs.is_empty());
}

#[test]
fn browser_provider_metadata_passes_only_present_upstream_fields() {
    let empty = BridgeChatResponse {
        text: String::new(),
        reasoning_content: None,
        model: None,
        conversation_id: None,
        warning: None,
        upstream_usage: None,
        upstream_cache: None,
    };
    assert_eq!(
        browser_provider_metadata(&BrowserProviderKind::Chatgpt, &empty),
        json!({ "provider": "chatgpt" })
    );

    let observed = BridgeChatResponse {
        upstream_usage: Some(
            json!({ "input_tokens": 7, "input_tokens_details": { "cached_tokens": 2 } }),
        ),
        upstream_cache: Some(json!({ "cache-control": "private, no-store" })),
        ..empty
    };
    assert_eq!(
        browser_provider_metadata(&BrowserProviderKind::Chatgpt, &observed),
        json!({
            "provider": "chatgpt",
            "upstream_usage": { "input_tokens": 7, "input_tokens_details": { "cached_tokens": 2 } },
            "upstream_cache": { "cache-control": "private, no-store" },
        })
    );
}

#[test]
fn response_input_ignores_unknown_item_types() {
    let msgs = response_input_to_messages(Some(&json!([
        { "role": "user", "content": "hi" },
        { "type": "unknown_future_type", "data": {} }
    ])));
    // Only the user message should survive; unknown items produce nothing or empty
    assert!(!msgs.is_empty());
    assert_eq!(msgs[0].role, "user");
}
