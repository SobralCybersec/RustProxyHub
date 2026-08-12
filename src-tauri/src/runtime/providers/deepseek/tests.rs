use super::{
    collect_fragment_events, deepseek_mode_flags, DeepSeekParseState, DeepseekPayload, ParsedEvent,
};
use crate::ids::{ParentMessageId, SessionId};
use crate::proxy_core::StreamingToolParser;
use serde_json::json;

#[test]
fn fragment_arrays_keep_reasoning_and_visible_text_separate() {
    let mut state = DeepSeekParseState::default();
    let mut parser = None::<StreamingToolParser>;
    let events = collect_fragment_events(
        &[
            json!({ "type": "THINK", "content": "internal" }),
            json!({ "type": "TEXT", "content": "visible" }),
        ],
        &mut state,
        &mut parser,
    );

    assert_eq!(state.reasoning, "internal");
    assert_eq!(
        events
            .into_iter()
            .filter_map(|event| match event {
                ParsedEvent::Text(text) => Some(text),
                _ => None,
            })
            .collect::<String>(),
        "visible"
    );
}

#[test]
fn deepseek_tool_prompt_uses_native_dsml_format() {
    let request = crate::proxy_core::OpenAIRequest {
        model: "deepseek-v4-pro".to_owned(),
        messages: Vec::new(),
        stream: None,
        web_search: None,
        chatgpt_mode: None,
        user: None,
        tools: Some(vec![]),
        tool_choice: None,
        stream_options: None,
    };
    let prompt = super::deepen_tool_prompt("request".to_owned(), &request);
    assert!(prompt.contains("<｜DSML｜tool_calls>"));
    assert!(prompt.contains("<｜DSML｜invoke name=\"tool_name\">"));
    assert!(!prompt.contains("<tool_call>"));
}

#[test]
fn deepseek_payload_reuses_template_and_syncs_direct_events() {
    let payload = DeepseekPayload::new("READY", &SessionId::new("session-1"))
        .template(Some(json!({
            "events": [
                { "event": "switchModelType", "params": "default" },
                { "event": "thinkingSwitchToggled", "params": false }
            ],
            "unused": true
        })))
        .parent(Some(ParentMessageId::from(42)))
        .pro(true)
        .thinking(true)
        .build();

    assert_eq!(payload["chat_session_id"], "session-1");
    assert_eq!(payload["parent_message_id"], 42);
    assert_eq!(payload["model_type"], "expert");
    assert_eq!(payload["prompt"], "READY");
    assert_eq!(payload["thinking_enabled"], true);
    assert_eq!(payload["search_enabled"], false);
    assert_eq!(payload["unused"], true);
    assert_eq!(payload["events"][0]["params"], "expert");
    assert_eq!(payload["events"][1]["params"], true);
}

#[test]
fn deepseek_payload_syncs_nested_event_groups() {
    let payload = DeepseekPayload::new("READY", &SessionId::new("session-2"))
        .template(Some(json!({
            "events": [
                {
                    "events": [
                        { "event": "switchModelType", "params": "expert" }
                    ]
                }
            ]
        })))
        .search(true)
        .build();

    assert_eq!(payload["model_type"], "default");
    assert_eq!(payload["events"][0]["events"][0]["params"], "default");
    assert_eq!(
        payload["events"][0]["events"][1]["event"],
        "thinkingSwitchToggled"
    );
    assert_eq!(payload["events"][0]["events"][1]["params"], false);
}

#[test]
fn deepseek_mode_flags_support_instant_expert_and_deepthink_aliases() {
    assert_eq!(
        deepseek_mode_flags("deepseek-v4-flash"),
        (false, false, false)
    );
    assert_eq!(deepseek_mode_flags("deepseek-v4-pro"), (true, false, false));
    assert_eq!(
        deepseek_mode_flags("deepseek-v4-flash-thinking"),
        (false, true, false)
    );
    assert_eq!(
        deepseek_mode_flags("deepseek-v4-pro-thinking"),
        (true, true, false)
    );
    assert_eq!(
        deepseek_mode_flags("deepseek-instant"),
        (false, false, false)
    );
    assert_eq!(deepseek_mode_flags("deepseek-expert"), (true, false, false));
    assert_eq!(
        deepseek_mode_flags("deepseek-instant-deepthink"),
        (false, true, false)
    );
    assert_eq!(
        deepseek_mode_flags("deepseek-expert-deepthink"),
        (true, true, false)
    );
}

#[test]
fn deepseek_v4_pro_payload_uses_expert_model_type() {
    let (is_pro, is_thinking, is_vision) = deepseek_mode_flags("deepseek-v4-pro");
    let payload = DeepseekPayload::new("READY", &SessionId::new("session-v4-pro"))
        .pro(is_pro)
        .thinking(is_thinking)
        .vision(is_vision)
        .build();

    assert_eq!(payload["model_type"], "expert");
    assert_eq!(payload["thinking_enabled"], false);
}

#[test]
fn deepseek_v4_vision_payload_uses_vision_model_type() {
    let (is_pro, is_thinking, is_vision) = deepseek_mode_flags("deepseek-v4-vision");
    assert_eq!((is_pro, is_thinking, is_vision), (false, false, true));
    let payload = DeepseekPayload::new("describe this image", &SessionId::new("session-v4-vision"))
        .pro(is_pro)
        .thinking(is_thinking)
        .vision(is_vision)
        .build();

    assert_eq!(payload["model_type"], "vision");
    assert_eq!(payload["thinking_enabled"], false);
    assert_eq!(payload["events"][0]["params"], "vision");
}

#[test]
fn deepseek_vision_wins_over_pro_and_thinking_aliases() {
    // A greedy id should still land in vision, never expert/thinking.
    assert_eq!(
        deepseek_mode_flags("deepseek-v4-pro-vision-thinking"),
        (false, false, true)
    );
}

#[test]
fn deepseek_mode_flags_plain_model_is_not_pro_not_thinking() {
    assert_eq!(deepseek_mode_flags("deepseek-chat"), (false, false, false));
    assert_eq!(deepseek_mode_flags("deepseek-v3"), (false, false, false));
}

#[test]
fn deepseek_mode_flags_case_insensitive() {
    assert_eq!(deepseek_mode_flags("DeepSeek-Expert"), (true, false, false));
    assert_eq!(
        deepseek_mode_flags("DEEPSEEK-DEEPTHINK"),
        (false, true, false)
    );
    assert_eq!(
        deepseek_mode_flags("DeepSeek-V4-VISION"),
        (false, false, true)
    );
}

#[test]
fn deepseek_mode_flags_whitespace_trimmed() {
    assert_eq!(
        deepseek_mode_flags("  deepseek-expert  "),
        (true, false, false)
    );
}

#[test]
fn fragment_events_with_only_think_blocks_produce_no_text() {
    let mut state = DeepSeekParseState::default();
    let mut parser = None::<StreamingToolParser>;
    let events = collect_fragment_events(
        &[
            json!({ "type": "THINK", "content": "internal A" }),
            json!({ "type": "THINK", "content": " internal B" }),
        ],
        &mut state,
        &mut parser,
    );

    assert_eq!(state.reasoning, "internal A internal B");
    // No TEXT events → no visible text output
    let visible: String = events
        .into_iter()
        .filter_map(|e| match e {
            ParsedEvent::Text(t) => Some(t),
            _ => None,
        })
        .collect();
    assert!(visible.is_empty());
}

#[test]
fn fragment_events_empty_array_produces_no_events() {
    let mut state = DeepSeekParseState::default();
    let mut parser = None::<StreamingToolParser>;
    let events = collect_fragment_events(&[], &mut state, &mut parser);
    assert!(events.is_empty());
}

#[test]
fn deepseek_payload_without_template_creates_events_array() {
    let payload = DeepseekPayload::new("hello", &SessionId::new("sess")).build();
    assert!(payload.get("events").is_some());
    assert_eq!(payload["prompt"], "hello");
    assert_eq!(payload["model_type"], "default");
    assert_eq!(payload["thinking_enabled"], false);
    assert_eq!(payload["search_enabled"], false);
}

#[test]
fn deepseek_payload_parent_message_id_included_when_some() {
    let payload = DeepseekPayload::new("p", &SessionId::new("s"))
        .parent(Some(ParentMessageId::from(99)))
        .build();
    assert_eq!(payload["parent_message_id"], 99);
}

#[test]
fn deepseek_payload_web_search_flag_set() {
    let payload = DeepseekPayload::new("q", &SessionId::new("s"))
        .search(true)
        .build();
    assert_eq!(payload["search_enabled"], true);
}
