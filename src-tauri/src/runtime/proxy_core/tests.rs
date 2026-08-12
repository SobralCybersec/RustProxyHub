use super::*;

#[test]
fn streaming_parser_extracts_multiple_json_array_tool_calls() {
    let mut parser = StreamingToolParser::new();
    let parsed = parser.feed(
        r#"<tool_call>[
{"name":"read_file","arguments":{"path":"Cargo.toml"}},
{"name":"run_tests","arguments":{"filter":"proxy_core"}}
]</tool_call>"#,
    );

    assert_eq!(parsed.text, "");
    assert_eq!(parsed.tool_calls.len(), 2);
    assert_eq!(parsed.tool_calls[0].name, "read_file");
    assert_eq!(parsed.tool_calls[1].name, "run_tests");
    assert_eq!(parser.emitted_tool_call_count(), 2);
}

#[test]
fn streaming_parser_extracts_deepseek_dsml_tool_calls() {
    let mut parser = StreamingToolParser::new();
    let parsed = parser.feed(
        r#"<｜DSML｜tool_calls>
<｜DSML｜invoke name="run_command">
<｜DSML｜parameter name="command" string="true">ls -la</｜DSML｜parameter>
<｜DSML｜parameter name="timeout" string="false">30</｜DSML｜parameter>
</｜DSML｜invoke>
</｜DSML｜tool_calls>"#,
    );

    assert!(parsed.text.is_empty());
    assert_eq!(parsed.tool_calls.len(), 1);
    assert_eq!(parsed.tool_calls[0].name, "run_command");
    assert_eq!(parsed.tool_calls[0].arguments["command"], "ls -la");
    assert_eq!(parsed.tool_calls[0].arguments["timeout"], 30);
}

#[test]
fn extract_lifts_whole_message_bare_json_tool_call() {
    let (text, calls) =
        extract_tool_calls_from_text(r#"{"name":"read_file","arguments":{"path":"a.rs"}}"#);
    assert_eq!(text, "");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "read_file");
}

#[test]
fn extract_lifts_fenced_json_tool_call_and_keeps_prose() {
    let (text, calls) = extract_tool_calls_from_text(
            "Sure, running it:\n```json\n{\"name\":\"run_tests\",\"arguments\":{\"filter\":\"x\"}}\n```",
        );
    assert_eq!(text, "Sure, running it:");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "run_tests");
}

#[test]
fn extract_ignores_plain_json_without_arguments_key() {
    /* {"name":..} with no args-key is data, not a tool call — must stay text */
    let (text, calls) = extract_tool_calls_from_text(r#"{"name":"John","age":30}"#);
    assert!(calls.is_empty());
    assert_eq!(text, r#"{"name":"John","age":30}"#);
}

#[test]
fn extract_leaves_ordinary_code_fence_alone() {
    let input = "Here:\n```rust\nfn main() {}\n```";
    let (text, calls) = extract_tool_calls_from_text(input);
    assert!(calls.is_empty());
    assert_eq!(text, input);
}

#[test]
fn custom_tool_deserializes_without_400_and_names_itself() {
    /* a type:"custom" tool has no `function` object — it must not fail the request */
    let req: OpenAIRequest = serde_json::from_value(json!({
        "model": "m",
        "messages": [],
        "tools": [
            { "type": "function", "function": { "name": "read_file" } },
            { "type": "custom", "name": "code_exec", "description": "run code" }
        ]
    }))
    .expect("custom tool must not break deserialization");
    let tools = req.tools.expect("tools present");
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0].tool_name(), Some("read_file"));
    assert_eq!(tools[1].tool_type, "custom");
    assert_eq!(tools[1].tool_name(), Some("code_exec"));
}

#[test]
fn tool_instructions_restricts_to_allowed_tools() {
    let tools = vec![FunctionToolDefinition {
        tool_type: "function".to_owned(),
        function: Some(FunctionToolSpec {
            name: "alpha".to_owned(),
            description: None,
            parameters: None,
            strict: None,
        }),
        name: None,
        description: None,
    }];
    let choice = json!({
        "type": "allowed_tools",
        "mode": "required",
        "tools": [{ "type": "function", "name": "alpha" }]
    });
    let out = tool_instructions(&tools, Some(&choice));
    assert!(out.contains("You MUST call one of these tools: alpha"));
}

#[test]
fn tool_instructions_forces_top_level_named_tool() {
    let tools = vec![FunctionToolDefinition {
        tool_type: "custom".to_owned(),
        function: None,
        name: Some("code_exec".to_owned()),
        description: None,
    }];
    /* Responses/custom style: name at the top level, not under `function` */
    let choice = json!({ "type": "custom", "name": "code_exec" });
    let out = tool_instructions(&tools, Some(&choice));
    assert!(out.contains("You MUST call the tool \"code_exec\""));
}

#[test]
fn streaming_parser_lifts_bare_json_tool_call_without_tags() {
    let mut parser = StreamingToolParser::new();
    let parsed = parser.feed(r#"{"name":"bash","arguments":{"command":"ls"}}"#);
    assert_eq!(parsed.text, "");
    assert_eq!(parsed.tool_calls.len(), 1);
    assert_eq!(parsed.tool_calls[0].name, "bash");
}

#[test]
fn streaming_parser_lifts_multiple_space_separated_bare_json_calls() {
    /* the exact Kilo failure: {..} {..} {..} bare, space-separated, no tags */
    let mut parser = StreamingToolParser::new();
    let parsed = parser.feed(
            r#"{"name":"bash","arguments":{"command":"ls"}} {"name":"glob","arguments":{"pattern":"*.md"}} {"name":"read","arguments":{"filePath":"a"}}"#,
        );
    let names: Vec<_> = parsed.tool_calls.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["bash", "glob", "read"]);
}

#[test]
fn streaming_parser_buffers_bare_json_split_across_chunks() {
    let mut parser = StreamingToolParser::new();
    let first = parser.feed(r#"{"name":"read","argu"#);
    assert!(first.tool_calls.is_empty());
    let second = parser.feed(r#"ments":{"filePath":"a.rs"}}"#);
    assert_eq!(second.tool_calls.len(), 1);
    assert_eq!(second.tool_calls[0].name, "read");
}

#[test]
fn streaming_parser_does_not_leak_tool_call_tag_split_at_bracket() {
    /* the exact live streaming bug: a chunk lands on "<tool_call" (no '>' yet);
    it must be held, not leaked as content, and the tags must never appear in text */
    let mut parser = StreamingToolParser::new();
    let first = parser.feed("<tool_call");
    assert_eq!(first.text, "");
    let second =
        parser.feed(">\n{\"name\":\"bash\",\"arguments\":{\"command\":\"ls\"}}\n</tool_call>");
    assert_eq!(second.text, "");
    assert_eq!(second.tool_calls.len(), 1);
    assert_eq!(second.tool_calls[0].name, "bash");
}

#[test]
fn streaming_parser_leaves_non_tool_json_as_text() {
    let mut parser = StreamingToolParser::new();
    let parsed = parser.feed(r#"the config is {"name":"John","age":30} today"#);
    assert!(parsed.tool_calls.is_empty());
    assert_eq!(
        parsed.text,
        r#"the config is {"name":"John","age":30} today"#
    );
}

#[test]
fn streaming_parser_handles_split_tool_call_tags() {
    let mut parser = StreamingToolParser::new();
    let first = parser.feed("before <tool");
    let second = parser.feed(r#"_call>{"name":"lookup","arguments":{"id":7}}</tool_call> after"#);

    assert_eq!(first.text, "before ");
    assert_eq!(second.text, " after");
    assert_eq!(second.tool_calls.len(), 1);
    assert_eq!(second.tool_calls[0].name, "lookup");
}

#[test]
fn build_prompt_honors_required_tool_choice() {
    let prompt = build_prompt(&OpenAIRequest {
        model: "test".to_owned(),
        messages: vec![Message {
            role: "user".to_owned(),
            content: Some(Value::String("inspect".to_owned())),
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning_content: None,
        }],
        stream: None,
        web_search: None,
        chatgpt_mode: None,
        user: None,
        tools: Some(vec![FunctionToolDefinition {
            tool_type: "function".to_owned(),
            function: Some(FunctionToolSpec {
                name: "read_file".to_owned(),
                description: None,
                parameters: None,
                strict: None,
            }),
            name: None,
            description: None,
        }]),
        tool_choice: Some(json!("required")),
        stream_options: None,
    });

    assert!(prompt.contains("MUST call one of the available tools"));
}

#[test]
fn tool_instructions_minify_schema_and_omit_nulls() {
    let request = OpenAIRequest {
        model: "test".to_owned(),
        messages: vec![Message {
            role: "user".to_owned(),
            content: Some(Value::String("inspect".to_owned())),
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning_content: None,
        }],
        stream: None,
        web_search: None,
        chatgpt_mode: None,
        user: None,
        tools: Some(vec![FunctionToolDefinition {
            tool_type: "function".to_owned(),
            function: Some(FunctionToolSpec {
                name: "read_file".to_owned(),
                description: None,
                parameters: Some(json!({"type":"object","properties":{"path":{"type":"string"}}})),
                strict: None,
            }),
            name: None,
            description: None,
        }]),
        tool_choice: None,
        stream_options: None,
    };
    let (system_prompt, _conversation) = split_prompt(&request);

    assert!(system_prompt.contains(r#""name":"read_file""#));
    assert!(system_prompt.contains(r#"[{"name":"read_file""#));
    assert!(!system_prompt.contains(r#""description":null"#));
    assert!(!system_prompt.contains(r#""strict":null"#));
}

#[test]
fn constant_time_eq_rejects_mismatched_inputs() {
    assert!(constant_time_eq("secret", "secret"));
    assert!(!constant_time_eq("secret", "secre"));
    assert!(!constant_time_eq("secret", "secrex"));
    assert!(!constant_time_eq("", "x"));
    assert!(constant_time_eq("", ""));
}

#[test]
fn safe_account_id_rejects_path_traversal() {
    assert!(is_safe_account_id("a1b2c3d4-e5f6-7890-abcd-ef1234567890"));
    assert!(is_safe_account_id("work-account"));
    assert!(is_safe_account_id("_default"));
    assert!(!is_safe_account_id(""));
    assert!(!is_safe_account_id("../escape"));
    assert!(!is_safe_account_id("a/b"));
    assert!(!is_safe_account_id("a\\b"));
    assert!(!is_safe_account_id("a:b"));
    assert!(!is_safe_account_id("."));
    assert!(!is_safe_account_id(&"a".repeat(65)));
}

#[test]
fn truncate_error_payload_handles_multibyte_boundary() {
    // 4-byte emoji repeated; byte 400 lands mid-codepoint.
    let text = "🦀".repeat(200);
    let truncated = truncate_error_payload(&text, 400);
    assert!(truncated.ends_with("..."));
    // Must not panic and must be valid UTF-8.
    let _ = truncated.chars().count();
}

#[test]
fn truncate_error_payload_preserves_short_input() {
    assert_eq!(truncate_error_payload("short", 400), "short");
}

#[test]
fn url_safety_denies_private_and_metadata() {
    assert!(url_is_safe_for_fetch("http://127.0.0.1/").is_err());
    assert!(url_is_safe_for_fetch("http://10.0.0.1/").is_err());
    assert!(url_is_safe_for_fetch("http://169.254.169.254/").is_err());
    assert!(url_is_safe_for_fetch("http://192.168.1.1/").is_err());
    assert!(url_is_safe_for_fetch("http://[::1]/").is_err());
    assert!(url_is_safe_for_fetch("ftp://example.com/").is_err());
    assert!(url_is_safe_for_fetch("javascript:alert(1)").is_err());
    assert!(url_is_safe_for_fetch("http://metadata.google.internal/").is_err());
}

#[test]
fn url_safety_allows_public() {
    assert!(url_is_safe_for_fetch("https://example.com/img.png").is_ok());
    assert!(url_is_safe_for_fetch("https://chat.qwen.ai/img.png").is_ok());
}

#[test]
fn split_prompt_extracts_system_from_conversation() {
    let req = OpenAIRequest {
        model: "test".to_owned(),
        messages: vec![
            Message {
                role: "system".to_owned(),
                content: Some(Value::String("You are a pirate.".to_owned())),
                tool_calls: None,
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            },
            Message {
                role: "user".to_owned(),
                content: Some(Value::String("Hello.".to_owned())),
                tool_calls: None,
                tool_call_id: None,
                name: None,
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
    let (system, convo) = split_prompt(&req);
    assert_eq!(system.trim(), "You are a pirate.");
    assert!(convo.contains("Hello."));
    assert!(!convo.contains("You are a pirate."));
}

#[test]
fn split_prompt_empty_system_when_no_system_message() {
    let req = OpenAIRequest {
        model: "test".to_owned(),
        messages: vec![Message {
            role: "user".to_owned(),
            content: Some(Value::String("Hi.".to_owned())),
            tool_calls: None,
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
    let (system, convo) = split_prompt(&req);
    assert!(system.is_empty());
    assert!(convo.contains("Hi."));
}

#[test]
fn split_prompt_appends_tool_definitions_to_system() {
    let req = OpenAIRequest {
        model: "test".to_owned(),
        messages: vec![Message {
            role: "user".to_owned(),
            content: Some(Value::String("go".to_owned())),
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning_content: None,
        }],
        stream: None,
        web_search: None,
        chatgpt_mode: None,
        user: None,
        tools: Some(vec![FunctionToolDefinition {
            tool_type: "function".to_owned(),
            function: Some(FunctionToolSpec {
                name: "get_weather".to_owned(),
                description: None,
                parameters: None,
                strict: None,
            }),
            name: None,
            description: None,
        }]),
        tool_choice: None,
        stream_options: None,
    };
    let (system, _convo) = split_prompt(&req);
    assert!(system.contains("get_weather"));
}

#[test]
fn tool_response_small_content_stays_exact_and_resolves_call_name() {
    let req = OpenAIRequest {
        model: "test".to_owned(),
        messages: vec![
            Message {
                role: "assistant".to_owned(),
                content: Some(Value::String("calling tool".to_owned())),
                tool_calls: Some(vec![MessageToolCall {
                    id: "call_1".to_owned(),
                    tool_type: "function".to_owned(),
                    function: ToolCallFunction {
                        name: "read_file".to_owned(),
                        arguments: "{\"path\":\"Cargo.toml\"}".to_owned(),
                    },
                }]),
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            },
            Message {
                role: "tool".to_owned(),
                content: Some(Value::String("ok".to_owned())),
                tool_calls: None,
                tool_call_id: Some("call_1".to_owned()),
                name: None,
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

    let (_system, convo) = split_prompt(&req);
    assert!(convo.contains("Tool Response (read_file): ok"));
}

#[test]
fn tool_response_json_compaction_labels_excerpt_and_keeps_order() {
    let raw_json = json!({
        "items": (0..600)
            .map(|index| json!({"id": index, "value": format!("entry-{index}-{}", "x".repeat(12))}))
            .collect::<Vec<_>>()
    });
    let req = OpenAIRequest {
        model: "test".to_owned(),
        messages: vec![
            Message {
                role: "user".to_owned(),
                content: Some(Value::String("inspect".to_owned())),
                tool_calls: None,
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            },
            Message {
                role: "assistant".to_owned(),
                content: Some(Value::String("running tool".to_owned())),
                tool_calls: Some(vec![MessageToolCall {
                    id: "call_2".to_owned(),
                    tool_type: "function".to_owned(),
                    function: ToolCallFunction {
                        name: "query".to_owned(),
                        arguments: "{}".to_owned(),
                    },
                }]),
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            },
            Message {
                role: "tool".to_owned(),
                content: Some(raw_json.clone()),
                tool_calls: None,
                tool_call_id: Some("call_2".to_owned()),
                name: Some("query".to_owned()),
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

    let prompt = build_prompt(&req);
    assert!(
        prompt.find("User: inspect").unwrap() < prompt.find("Assistant: running tool").unwrap()
    );
    assert!(
        prompt.find("Assistant: running tool").unwrap()
            < prompt.find("Tool Response (query):").unwrap()
    );
    assert!(prompt.contains("[Tool Response JSON compacted; original"));
    assert!(prompt.contains("Excerpt is not complete JSON."));
    assert!(prompt.len() < raw_json.to_string().len() + 256);
}

#[test]
fn tool_response_test_output_compaction_keeps_failure_lines() {
    let mut output = String::from("running 120 tests\n");
    for _ in 0..220 {
        output.push_str("test module::passing_case_with_verbose_name_and_fixture_setup ... ok\n");
    }
    output.push_str("thread 'main' panicked at src/lib.rs:42: boom\n");
    output.push_str("failures:\n");
    output.push_str("test result: FAILED. 119 passed; 1 failed;\n");

    let compacted = compact_tool_response_content(&Some(Value::String(output)));
    assert!(compacted.contains("[Tool Response test output compacted; original"));
    assert!(compacted.contains("panicked"));
    assert!(compacted.contains("FAILED"));
    assert!(!compacted.contains("passing_case ... ok\ntest module::passing_case ... ok\ntest module::passing_case ... ok\ntest module::passing_case ... ok\ntest module::passing_case ... ok\ntest module::passing_case ... ok\ntest module::passing_case ... ok\ntest module::passing_case ... ok\ntest module::passing_case ... ok\ntest module::passing_case ... ok"));
}

#[test]
fn tool_response_log_compaction_keeps_error_lines_in_order() {
    let output = [
        "2026-08-11T09:00:00Z INFO booting",
        "2026-08-11T09:00:01Z INFO warming cache",
        "2026-08-11T09:00:02Z WARN slow response",
        "2026-08-11T09:00:03Z INFO heartbeat",
        "2026-08-11T09:00:04Z ERROR upstream timeout",
        "2026-08-11T09:00:05Z INFO retrying",
        "2026-08-11T09:00:06Z ERROR retry failed",
        "2026-08-11T09:00:07Z INFO done",
    ]
    .join("\n")
    .repeat(120);

    let compacted = compact_tool_response_content(&Some(Value::String(output)));
    let warn_index = compacted.find("WARN slow response").unwrap();
    let error_one_index = compacted.find("ERROR upstream timeout").unwrap();
    let error_two_index = compacted.rfind("ERROR retry failed").unwrap();
    assert!(compacted.contains("[Tool Response log output compacted; original"));
    assert!(warn_index < error_one_index);
    assert!(error_one_index < error_two_index);
}

#[test]
fn tool_response_media_payload_is_omitted() {
    let compacted = compact_tool_response_content(&Some(json!({
        "type": "image_url",
        "image_url": "data:image/png;base64,AAAAABBBBBCCCCCDDDD"
    })));
    assert!(compacted.contains("[Tool Response media payload omitted]"));
    assert!(compacted.contains("payload omitted"));
}

#[test]
fn compact_structured_prompt_preserves_first_and_latest_blocks() {
    let prompt = [
        "User: system bootstrap and repo contract",
        "Assistant: acknowledged",
        "User: very old context that can be omitted safely",
        "Assistant: old answer",
        "User: latest request with exact task",
        "Assistant: latest answer",
    ]
    .join("\n\n");

    let compacted = compact_structured_prompt(
        &prompt,
        StructuredPromptCompactionOptions {
            max_chars: 145,
            preserve_first_block: true,
        },
    );

    assert!(compacted.truncated);
    assert!(compacted.text.starts_with("User: system bootstrap"));
    assert!(compacted
        .text
        .contains("User: latest request with exact task"));
    assert!(!compacted.text.contains("very old context"));
}

#[test]
fn compact_structured_prompt_removes_duplicate_blocks_before_omitting_unique() {
    let prompt = [
        "User: repeated turn",
        "Assistant: same output",
        "Assistant: same output",
        "Assistant: same output",
        "User: final turn",
    ]
    .join("\n\n");

    let compacted = compact_structured_prompt(
        &prompt,
        StructuredPromptCompactionOptions {
            max_chars: 70,
            preserve_first_block: true,
        },
    );

    assert!(compacted.truncated);
    assert!(compacted.removed_duplicate_blocks >= 1);
    assert!(compacted.text.contains("User: final turn"));
}

#[test]
fn compact_structured_prompt_uses_head_tail_for_single_long_block() {
    let prompt = format!("User: {}", "A".repeat(300));
    let compacted = compact_structured_prompt(
        &prompt,
        StructuredPromptCompactionOptions {
            max_chars: 80,
            preserve_first_block: true,
        },
    );

    assert!(compacted.truncated);
    assert_eq!(compacted.mode, "head-tail");
    assert!(compacted.compacted_chars <= 80);
    assert!(!compacted.text.is_empty());
}

#[test]
fn compact_structured_prompt_does_not_middle_trim_tool_call_block() {
    let prompt = [
            "User: bootstrap",
            "Assistant: latest tool run\n<tool_call>\n{\"name\":\"read_file\",\"arguments\":{\"path\":\"Cargo.toml\"}}\n</tool_call>",
        ]
        .join("\n\n");

    let compacted = compact_structured_prompt(
        &prompt,
        StructuredPromptCompactionOptions {
            max_chars: 60,
            preserve_first_block: true,
        },
    );

    assert!(compacted.truncated);
    let has_open = compacted.text.contains("<tool_call>");
    let has_close = compacted.text.contains("</tool_call>");
    assert_eq!(has_open, has_close);
}

#[test]
fn preflight_preserves_assistant_tool_call_and_tool_result_group() {
    let request = OpenAIRequest {
        model: "test".to_owned(),
        messages: vec![
            Message {
                role: "system".to_owned(),
                content: Some(Value::String("Keep exact tools.".to_owned())),
                tool_calls: None,
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            },
            Message {
                role: "user".to_owned(),
                content: Some(Value::String("older".repeat(30))),
                tool_calls: None,
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            },
            Message {
                role: "assistant".to_owned(),
                content: Some(Value::String("calling tool".to_owned())),
                tool_calls: Some(vec![MessageToolCall {
                    id: "call_1".to_owned(),
                    tool_type: "function".to_owned(),
                    function: ToolCallFunction {
                        name: "read_file".to_owned(),
                        arguments: "{\"path\":\"Cargo.toml\"}".to_owned(),
                    },
                }]),
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            },
            Message {
                role: "tool".to_owned(),
                content: Some(Value::String("ok".to_owned())),
                tool_calls: None,
                tool_call_id: Some("call_1".to_owned()),
                name: Some("read_file".to_owned()),
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
    let expected_kept = request_with_messages(
        &request,
        vec![
            request.messages[0].clone(),
            request.messages[2].clone(),
            request.messages[3].clone(),
        ],
    );
    let expected_budget = estimate_tokens(&build_prompt(&expected_kept));

    let preflight = preflight_request_to_budget(
        &request,
        &PromptPreflightOptions {
            max_prompt_tokens: Some(expected_budget),
            extra_system_instructions: None,
            dedup_system_blocks: false,
            structured_compaction_max_chars: None,
        },
        estimate_tokens,
    )
    .expect("preflight");

    assert_eq!(preflight.request.messages.len(), 3);
    assert_eq!(preflight.request.messages[1].role, "assistant");
    assert_eq!(preflight.request.messages[2].role, "tool");
    assert!(preflight.truncated);
}

#[test]
fn preflight_rejects_over_budget_system_and_tools() {
    let request = OpenAIRequest {
        model: "test".to_owned(),
        messages: vec![Message {
            role: "system".to_owned(),
            content: Some(Value::String("A".repeat(400))),
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning_content: None,
        }],
        stream: None,
        web_search: None,
        chatgpt_mode: None,
        user: None,
        tools: Some(vec![FunctionToolDefinition {
            tool_type: "function".to_owned(),
            function: Some(FunctionToolSpec {
                name: "read_file".to_owned(),
                description: Some("B".repeat(400)),
                parameters: Some(json!({"type":"object","properties":{"path":{"type":"string"}}})),
                strict: None,
            }),
            name: None,
            description: None,
        }]),
        tool_choice: None,
        stream_options: None,
    };

    let error = preflight_request_to_budget(
        &request,
        &PromptPreflightOptions {
            max_prompt_tokens: Some(20),
            extra_system_instructions: None,
            dedup_system_blocks: false,
            structured_compaction_max_chars: None,
        },
        estimate_tokens,
    )
    .expect_err("budget failure");

    assert!(error
        .to_string()
        .contains("prompt preflight exceeded budget with system/tool instructions only"));
}

#[test]
fn preflight_dedups_system_blocks_only_when_enabled() {
    let request = OpenAIRequest {
        model: "test".to_owned(),
        messages: vec![
            Message {
                role: "system".to_owned(),
                content: Some(Value::String("Keep it terse.".to_owned())),
                tool_calls: None,
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            },
            Message {
                role: "system".to_owned(),
                content: Some(Value::String("Keep   it terse.".to_owned())),
                tool_calls: None,
                tool_call_id: None,
                name: None,
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

    let disabled = preflight_request_to_budget(
        &request,
        &PromptPreflightOptions::default(),
        estimate_tokens,
    )
    .expect("disabled");
    let enabled = preflight_request_to_budget(
        &request,
        &PromptPreflightOptions {
            max_prompt_tokens: None,
            extra_system_instructions: None,
            dedup_system_blocks: true,
            structured_compaction_max_chars: None,
        },
        estimate_tokens,
    )
    .expect("enabled");

    assert!(disabled.system_prompt.matches("Keep").count() >= 2);
    assert_eq!(enabled.system_prompt.matches("Keep").count(), 1);
}

#[test]
fn preflight_structured_compaction_compacts_conversation_not_system() {
    let request = OpenAIRequest {
        model: "test".to_owned(),
        messages: vec![
            Message {
                role: "system".to_owned(),
                content: Some(Value::String("System line stays intact.".to_owned())),
                tool_calls: None,
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            },
            Message {
                role: "user".to_owned(),
                content: Some(Value::String("old context ".repeat(12))),
                tool_calls: None,
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            },
            Message {
                role: "assistant".to_owned(),
                content: Some(Value::String("older answer ".repeat(8))),
                tool_calls: None,
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            },
            Message {
                role: "user".to_owned(),
                content: Some(Value::String("latest exact request".to_owned())),
                tool_calls: None,
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            },
        ],
        stream: None,
        web_search: None,
        chatgpt_mode: None,
        user: None,
        tools: Some(vec![FunctionToolDefinition {
            tool_type: "function".to_owned(),
            function: Some(FunctionToolSpec {
                name: "read_file".to_owned(),
                description: Some("Reads file contents".to_owned()),
                parameters: Some(json!({"type":"object","properties":{"path":{"type":"string"}}})),
                strict: None,
            }),
            name: None,
            description: None,
        }]),
        tool_choice: None,
        stream_options: None,
    };

    let preflight = preflight_request_to_budget(
        &request,
        &PromptPreflightOptions {
            max_prompt_tokens: None,
            extra_system_instructions: None,
            dedup_system_blocks: false,
            structured_compaction_max_chars: Some(120),
        },
        estimate_tokens,
    )
    .expect("preflight");

    assert!(preflight
        .system_prompt
        .contains("System line stays intact."));
    assert!(preflight.system_prompt.contains("\"name\":\"read_file\""));
    assert!(preflight.conversation.chars().count() <= 120);
    assert!(preflight.flat_prompt.contains("System line stays intact."));
    assert!(preflight.flat_prompt.contains(&preflight.conversation));
}

#[test]
fn build_prompt_concatenates_system_and_convo() {
    let req = OpenAIRequest {
        model: "test".to_owned(),
        messages: vec![
            Message {
                role: "system".to_owned(),
                content: Some(Value::String("Be terse.".to_owned())),
                tool_calls: None,
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            },
            Message {
                role: "user".to_owned(),
                content: Some(Value::String("Summarize.".to_owned())),
                tool_calls: None,
                tool_call_id: None,
                name: None,
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
    let flat = build_prompt(&req);
    assert!(flat.contains("Be terse."));
    assert!(flat.contains("Summarize."));
}

#[test]
fn build_prompt_returns_just_convo_when_no_system() {
    let req = OpenAIRequest {
        model: "test".to_owned(),
        messages: vec![Message {
            role: "user".to_owned(),
            content: Some(Value::String("Do it.".to_owned())),
            tool_calls: None,
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
    let flat = build_prompt(&req);
    // No "system\n\n" prefix bloating the prompt
    assert!(!flat.starts_with('\n'));
    assert!(flat.contains("Do it."));
}

#[path = "tests_prompt.rs"]
mod tests_prompt;
