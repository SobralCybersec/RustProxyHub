use super::*;

#[allow(dead_code)]
fn loopback_guard_rejects_non_loopback_without_key() {
    assert!(enforce_loopback_guard("127.0.0.1", None).is_ok());
    assert!(enforce_loopback_guard("0.0.0.0", None).is_err());
    assert!(enforce_loopback_guard("0.0.0.0", Some("")).is_err());
    assert!(enforce_loopback_guard("0.0.0.0", Some("secret")).is_ok());
}

#[test]
fn estimate_tokens_rounds_up() {
    // 7 chars / 3.5 = 2.0 exactly
    assert_eq!(estimate_tokens("1234567"), 2);
    // 8 chars / 3.5 = 2.28… → ceil = 3
    assert_eq!(estimate_tokens("12345678"), 3);
    assert_eq!(estimate_tokens(""), 0);
}

#[test]
fn compact_prompt_removes_formatting_only() {
    let input = "System: keep exact text  \n\n\nUser: hello\n\n\n\nAssistant: ok  ";
    assert_eq!(
        compact_prompt(input),
        "System: keep exact text\n\nUser: hello\n\nAssistant: ok"
    );
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
        chatgpt_mode: None,
        user: None,
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
        chatgpt_mode: None,
        user: None,
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
        chatgpt_mode: None,
        user: None,
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
        chatgpt_mode: None,
        user: None,
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
        chatgpt_mode: None,
        user: None,
        tools: None,
        tool_choice: None,
        stream_options: None,
    };
    let (sys, _) = split_prompt(&req);
    assert!(sys.contains("Part A."));
    assert!(sys.contains("Part B."));
}
