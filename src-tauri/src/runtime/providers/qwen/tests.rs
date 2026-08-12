use super::{
    collect_qwen_events, extract_qwen_api_error, extract_qwen_chat_id, live_qwen_model_data,
    qwen_session_key, qwen_sse_data, QwenEvent, QwenParseState, StreamRegistry,
};
use serde_json::json;

#[test]
fn session_key_defaults_and_trims_client_identity() {
    assert_eq!(qwen_session_key(None), "default");
    assert_eq!(qwen_session_key(Some("  ")), "default");
    assert_eq!(qwen_session_key(Some("  client-1 ")), "client-1");
}

#[test]
fn recognizes_reset_connections_as_retryable_transport_failures() {
    assert!(super::is_retryable_transport_message(
        "connection reset by server"
    ));
    assert!(super::is_retryable_transport_message("error: ECONNRESET"));
    assert!(!super::is_retryable_transport_message("invalid request"));
}

#[test]
fn accepts_qwen_sse_data_with_or_without_a_space() {
    assert_eq!(
        qwen_sse_data("data:{\"answer\":\"ok\"}"),
        Some("{\"answer\":\"ok\"}")
    );
    assert_eq!(
        qwen_sse_data("data: {\"answer\":\"ok\"}"),
        Some("{\"answer\":\"ok\"}")
    );
    assert_eq!(qwen_sse_data("event: message"), None);
}

#[test]
fn extracts_nested_chat_id_variants() {
    assert_eq!(
        extract_qwen_chat_id(&json!({ "data": { "id": "chat-123" } })),
        Some("chat-123")
    );
    assert_eq!(
        extract_qwen_chat_id(&json!({ "data": { "chat_id": "chat-456" } })),
        Some("chat-456")
    );
    assert_eq!(
        extract_qwen_chat_id(&json!({ "id": "chat-789" })),
        Some("chat-789")
    );
    assert_eq!(
        extract_qwen_chat_id(&json!({ "data": { "chat": { "id": "chat-987" } } })),
        Some("chat-987")
    );
}

#[test]
fn extracts_qwen_api_error_message() {
    assert_eq!(
        extract_qwen_api_error(&json!({
            "success": false,
            "data": {
                "code": "Unauthorized",
                "details": "login required"
            }
        })),
        Some("Unauthorized: login required".to_owned())
    );
}

#[test]
fn expands_live_qwen_catalog_with_thinking_variants() {
    let payload = live_qwen_model_data(&json!({
        "data": [{ "id": "qwen-live", "info": { "created_at": 1 } }]
    }))
    .unwrap();
    let ids = payload["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|model| model["id"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        ["qwen-live", "qwen-live-thinking", "qwen-live-no-thinking"]
    );
    assert_eq!(payload["discovery"]["live"], true);
}

#[tokio::test]
async fn surfaces_qwen_stream_error_payloads() {
    let registry = StreamRegistry::new();
    let mut state = QwenParseState::default();
    let mut parser = None;
    let result = collect_qwen_events(
        &json!({
            "success": false,
            "data": { "code": "Bad_Request", "details": "model unavailable" }
        })
        .to_string(),
        "chatcmpl-test",
        &registry,
        &mut state,
        &mut parser,
    )
    .await;
    assert!(result.is_err());
    let error = result.err().unwrap();
    assert!(error.to_string().contains("Bad_Request: model unavailable"));
}

#[tokio::test]
async fn parses_answer_content_without_phase() {
    let registry = StreamRegistry::new();
    let mut state = QwenParseState::default();
    let mut parser = None;
    let data = json!({
        "response_id": "resp-1",
        "choices": [{
            "delta": {
                "content": "visible answer"
            }
        }]
    })
    .to_string();

    let events = collect_qwen_events(&data, "chatcmpl-test", &registry, &mut state, &mut parser)
        .await
        .unwrap();

    assert!(matches!(
        events.as_slice(),
        [QwenEvent::Text(text)] if text == "visible answer"
    ));
    assert_eq!(state.last_full_content, "visible answer");
}

#[tokio::test]
async fn maps_thinking_summary_to_reasoning() {
    let registry = StreamRegistry::new();
    let mut state = QwenParseState::default();
    let mut parser = None;
    let data = json!({
        "response_id": "resp-1",
        "choices": [{
            "delta": {
                "phase": "thinking_summary",
                "extra": {
                    "summary_thought": {
                        "content": ["first thought"]
                    }
                }
            }
        }]
    })
    .to_string();

    let events = collect_qwen_events(&data, "chatcmpl-test", &registry, &mut state, &mut parser)
        .await
        .unwrap();

    assert!(matches!(
        events.as_slice(),
        [QwenEvent::Reasoning(text)] if text == "first thought"
    ));
    assert_eq!(state.reasoning, "first thought");
    assert!(state.last_full_content.is_empty());
}

#[tokio::test]
async fn parses_answer_content_array_shape() {
    let registry = StreamRegistry::new();
    let mut state = QwenParseState::default();
    let mut parser = None;
    let data = json!({
        "response_id": "resp-1",
        "choices": [{
            "delta": {
                "content": [
                    { "type": "text", "text": "visible " },
                    { "type": "text", "content": "answer" }
                ]
            }
        }]
    })
    .to_string();

    let events = collect_qwen_events(&data, "chatcmpl-test", &registry, &mut state, &mut parser)
        .await
        .unwrap();

    assert!(matches!(
        events.as_slice(),
        [QwenEvent::Text(text)] if text == "visible answer"
    ));
    assert_eq!(state.last_full_content, "visible answer");
}

#[tokio::test]
async fn parses_nested_answer_content_object() {
    let registry = StreamRegistry::new();
    let mut state = QwenParseState::default();
    let mut parser = None;
    let data = json!({
        "response_id": "resp-1",
        "choices": [{
            "delta": {
                "content": {
                    "answer": {
                        "parts": [
                            { "text": "nested " },
                            { "value": "answer" }
                        ]
                    }
                }
            }
        }]
    })
    .to_string();

    let events = collect_qwen_events(&data, "chatcmpl-test", &registry, &mut state, &mut parser)
        .await
        .unwrap();

    assert!(matches!(
        events.as_slice(),
        [QwenEvent::Text(text)] if text == "nested answer"
    ));
    assert_eq!(state.last_full_content, "nested answer");
}
