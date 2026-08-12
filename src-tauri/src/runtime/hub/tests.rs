use super::{
    infer_provider, normalize_json_model, normalize_prefixed_model, openapi_document,
    tag_provider_models, AppConfig, ProviderConfig, ProviderName,
};
use serde_json::{json, Value};

fn test_config() -> AppConfig {
    let provider = || ProviderConfig::new("http://127.0.0.1:1", None);
    AppConfig {
        host: "127.0.0.1".to_owned(),
        port: 3100,
        api_key: None,
        deepseek: provider(),
        kimi: provider(),
        qwen: provider(),
        chatgpt: provider(),
        gemini: provider(),
        mistral: provider(),
        zai: provider(),
        meta: provider(),
    }
}

#[test]
fn openapi_advertises_provider_logs_route() {
    let document = openapi_document(&test_config());
    assert_eq!(
        document["paths"]["/providers/{provider}/logs"]["get"]["summary"],
        "Read recent provider bridge log entries"
    );
}

#[test]
fn routes_prefixed_mistral_models_to_mistral() {
    let routed = normalize_prefixed_model("mistral:mistral-large-latest");
    assert_eq!(routed.provider, ProviderName::Mistral);
    assert_eq!(routed.model, "mistral-large-latest");
}

#[test]
fn routes_prefixed_zai_models_to_zai() {
    let routed = normalize_prefixed_model("zai:glm-5.2");
    assert_eq!(routed.provider, ProviderName::Zai);
    assert_eq!(routed.model, "glm-5.2");
}

#[test]
fn routes_prefixed_meta_models_to_meta() {
    let routed = normalize_prefixed_model("meta:meta-ai-web-session");
    assert_eq!(routed.provider, ProviderName::Meta);
    assert_eq!(routed.model, "meta-ai-web-session");
}

#[test]
fn infers_mistral_family_models() {
    assert_eq!(infer_provider("mistral-medium"), ProviderName::Mistral);
    assert_eq!(infer_provider("magistral-medium"), ProviderName::Mistral);
    assert_eq!(infer_provider("codestral-latest"), ProviderName::Mistral);
}

#[test]
fn infers_glm_family_models_to_zai() {
    assert_eq!(infer_provider("glm-5.2"), ProviderName::Zai);
    assert_eq!(infer_provider("autoglm-agent"), ProviderName::Zai);
}

#[test]
fn infers_meta_family_models() {
    assert_eq!(infer_provider("meta-ai-web-session"), ProviderName::Meta);
    assert_eq!(infer_provider("llama-4"), ProviderName::Meta);
}

#[test]
fn infers_claude_family_models_to_browser_provider() {
    assert_eq!(infer_provider("claude-sonnet-4-5"), ProviderName::Chatgpt);
    assert_eq!(
        infer_provider("anthropic.claude-sonnet-4"),
        ProviderName::Chatgpt
    );
}

#[test]
fn normalizes_prefixed_json_model_payloads() {
    let routed = normalize_json_model(json!({
        "model": "mistral:mistral-large-latest",
        "input": "hello"
    }));

    assert_eq!(routed.provider, ProviderName::Mistral);
    assert_eq!(
        routed.original_model.as_deref(),
        Some("mistral:mistral-large-latest")
    );
    assert_eq!(
        routed.payload.get("model").and_then(Value::as_str),
        Some("mistral-large-latest")
    );
}

#[test]
fn infers_kimi_k2_family_models() {
    assert_eq!(infer_provider("k2d6"), ProviderName::Kimi);
    assert_eq!(infer_provider("k2d6-thinking"), ProviderName::Kimi);
    assert_eq!(infer_provider("kimi-k2.6"), ProviderName::Kimi);
    assert_eq!(infer_provider("kimi-k2.6-thinking"), ProviderName::Kimi);
}

#[test]
fn infers_kimi_k3_family_models() {
    assert_eq!(infer_provider("k3"), ProviderName::Kimi);
    assert_eq!(infer_provider("kimi-k3"), ProviderName::Kimi);
    assert_eq!(infer_provider("kimi-k3-thinking"), ProviderName::Kimi);
    assert_eq!(infer_provider("kimi-latest"), ProviderName::Kimi);
}

#[test]
fn routes_prefixed_kimi_models() {
    let routed = normalize_prefixed_model("kimi:kimi-k3");
    assert_eq!(routed.provider, ProviderName::Kimi);
    assert_eq!(routed.model, "kimi-k3");
}

#[test]
fn chatgpt_codex_models_get_kilo_capability_metadata() {
    let tagged = tag_provider_models(
        vec![json!({ "id": "gpt-5.3-codex" })],
        ProviderName::Chatgpt,
    );
    let item = &tagged[0];

    assert_eq!(
        item["description"],
        "Uses Codex OAuth Responses API; usage is billed/limited as Codex usage."
    );
    assert_eq!(item["api"], "codex_responses");
    assert_eq!(item["billing"], "Codex billing usage");
    assert_eq!(item["tool_call"], true);
    assert_eq!(item["supportsNativeTools"], true);
}

#[test]
fn chatgpt_web_models_get_completions_metadata() {
    let tagged = tag_provider_models(vec![json!({ "id": "gpt-5-3" })], ProviderName::Chatgpt);
    let item = &tagged[0];

    assert_eq!(
        item["description"],
        "Uses Chat Completions API compatibility through the ChatGPT web session."
    );
    assert_eq!(item["api"], "chat_completions");
    assert_eq!(item["billing"], "ChatGPT subscription/web-session usage");
    assert_eq!(item["tool_call"], true);
    assert_eq!(item["supports_function_calling"], true);
}

#[test]
fn deepseek_reasoner_models_disable_native_tool_metadata() {
    let tagged = tag_provider_models(vec![json!({ "id": "deepseek-r1" })], ProviderName::Deepseek);
    let item = &tagged[0];

    assert_eq!(item["tool_call"], false);
    assert_eq!(item["supportsNativeTools"], false);
    assert_eq!(item["supports_function_calling"], false);
}

#[test]
fn model_merge_tags_provider_and_skips_invalid_items() {
    let items = vec![
        json!({ "id": "mistral-web-session" }),
        Value::String("bad".to_owned()),
    ];

    let tagged = tag_provider_models(items, ProviderName::Mistral);

    assert_eq!(tagged.len(), 1);
    assert_eq!(
        tagged[0].get("provider").and_then(Value::as_str),
        Some("mistral")
    );
    assert_eq!(
        tagged[0].get("id").and_then(Value::as_str),
        Some("mistral-web-session")
    );
}

#[test]
fn infers_deepseek_family_models() {
    assert_eq!(infer_provider("deepseek-v4-flash"), ProviderName::Deepseek);
    assert_eq!(infer_provider("deepseek-v4-pro"), ProviderName::Deepseek);
    assert_eq!(
        infer_provider("deepseek-v4-flash-thinking"),
        ProviderName::Deepseek
    );
    assert_eq!(infer_provider("deepseek-instant"), ProviderName::Deepseek);
}

#[test]
fn infers_gemini_family_models() {
    assert_eq!(infer_provider("gemini-2.5-pro"), ProviderName::Gemini);
    assert_eq!(infer_provider("gemini-flash"), ProviderName::Gemini);
    assert_eq!(infer_provider("gemini-exp-1206"), ProviderName::Gemini);
}

#[test]
fn infers_chatgpt_gpt_series() {
    assert_eq!(infer_provider("gpt-4o"), ProviderName::Chatgpt);
    assert_eq!(infer_provider("gpt-4o-mini"), ProviderName::Chatgpt);
    assert_eq!(infer_provider("o3"), ProviderName::Chatgpt);
    assert_eq!(infer_provider("o4-mini"), ProviderName::Chatgpt);
    assert_eq!(infer_provider("o1-preview"), ProviderName::Chatgpt);
    assert_eq!(infer_provider("chatgpt-web-session"), ProviderName::Chatgpt);
}

#[test]
fn infers_qwen_as_default_fallback() {
    assert_eq!(infer_provider("qwen3-30b-a3b"), ProviderName::Qwen);
    assert_eq!(infer_provider("qwq-32b-preview"), ProviderName::Qwen);
    // Anything unrecognised lands on Qwen
    assert_eq!(
        infer_provider("unknown-future-model-xyz"),
        ProviderName::Qwen
    );
}

#[test]
fn routes_prefixed_deepseek_model() {
    let routed = normalize_prefixed_model("deepseek:deepseek-v4-flash");
    assert_eq!(routed.provider, ProviderName::Deepseek);
    assert_eq!(routed.model, "deepseek-v4-flash");
}

#[test]
fn routes_prefixed_gemini_model() {
    let routed = normalize_prefixed_model("gemini:gemini-2.5-pro");
    assert_eq!(routed.provider, ProviderName::Gemini);
    assert_eq!(routed.model, "gemini-2.5-pro");
}

#[test]
fn routes_prefixed_qwen_model() {
    let routed = normalize_prefixed_model("qwen:qwen3-30b-a3b");
    assert_eq!(routed.provider, ProviderName::Qwen);
    assert_eq!(routed.model, "qwen3-30b-a3b");
}

#[test]
fn model_without_prefix_infers_provider() {
    // bare model names route by infer_provider (no colon)
    let routed = normalize_prefixed_model("deepseek-v4-flash");
    assert_eq!(routed.provider, ProviderName::Deepseek);
    assert_eq!(routed.model, "deepseek-v4-flash");
}

#[test]
fn tag_provider_models_empty_input() {
    let tagged = tag_provider_models(vec![], ProviderName::Kimi);
    assert!(tagged.is_empty());
}

#[test]
fn tag_provider_models_all_invalid() {
    let items = vec![Value::Bool(true), json!(null)];
    let tagged = tag_provider_models(items, ProviderName::Kimi);
    assert!(tagged.is_empty());
}
