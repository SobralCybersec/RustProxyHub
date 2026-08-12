fn provider_request(
    client: &reqwest::Client,
    config: &ProviderConfig,
    method: Method,
    path: &str,
) -> reqwest::RequestBuilder {
    let url = format!("{}{}", config.base_url.trim_end_matches('/'), path);
    let mut request = client.request(method, url);
    if let Some(api_key) = config.api_key.as_deref() {
        request = request.bearer_auth(api_key);
        request = request.header("x-api-key", api_key);
    }
    request
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

/// Map an upstream error to a generic 502 with an opaque id; log the real cause server-side
/// so internal base URLs / IPs / path fragments never reach the client.
fn bad_gateway_error(err: impl std::fmt::Display) -> Response {
    let id = uuid::Uuid::new_v4();
    eprintln!("[hub] upstream error {id}: {err}");
    json_error(
        StatusCode::BAD_GATEWAY,
        format!("upstream provider error (id={id})"),
    )
}

#[derive(Clone)]
struct RoutedModel {
    provider: ProviderName,
    model: String,
}

struct RoutedJson {
    provider: ProviderName,
    payload: Value,
    original_model: Option<String>,
}

fn normalize_json_model(mut payload: Value) -> RoutedJson {
    let original_model = payload
        .get("model")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let routed = original_model
        .as_deref()
        .map(normalize_prefixed_model)
        .unwrap_or_else(|| RoutedModel {
            provider: ProviderName::Chatgpt,
            model: String::new(),
        });

    if !routed.model.is_empty() {
        if let Some(object) = payload.as_object_mut() {
            object.insert("model".to_owned(), Value::String(routed.model));
        }
    }

    RoutedJson {
        provider: routed.provider,
        payload,
        original_model,
    }
}

fn normalize_prefixed_model(model: &str) -> RoutedModel {
    let trimmed = model.trim();
    if let Some((prefix, actual)) = trimmed.split_once(':') {
        let provider = match prefix.to_ascii_lowercase().as_str() {
            "deepseek" => ProviderName::Deepseek,
            "kimi" => ProviderName::Kimi,
            "qwen" => ProviderName::Qwen,
            "chatgpt" => ProviderName::Chatgpt,
            "gemini" => ProviderName::Gemini,
            "mistral" => ProviderName::Mistral,
            "zai" => ProviderName::Zai,
            "meta" => ProviderName::Meta,
            _ => infer_provider(trimmed),
        };
        return RoutedModel {
            provider,
            model: actual.to_owned(),
        };
    }

    RoutedModel {
        provider: infer_provider(trimmed),
        model: trimmed.to_owned(),
    }
}

fn infer_provider(model: &str) -> ProviderName {
    let lower = model.to_ascii_lowercase();
    if lower.starts_with("deepseek") {
        ProviderName::Deepseek
    } else if lower.starts_with("k2") || lower.starts_with("k3") || lower.starts_with("kimi") {
        ProviderName::Kimi
    } else if lower.starts_with("gemini") {
        ProviderName::Gemini
    } else if lower.starts_with("mistral")
        || lower.starts_with("magistral")
        || lower.starts_with("codestral")
    {
        ProviderName::Mistral
    } else if lower.starts_with("glm") || lower.starts_with("autoglm") || lower.starts_with("zai") {
        ProviderName::Zai
    } else if lower.starts_with("meta") || lower.starts_with("llama") {
        ProviderName::Meta
    } else if lower.starts_with("gpt")
        || lower.starts_with("o1")
        || lower.starts_with("o3")
        || lower.starts_with("o4")
        || lower.starts_with("chatgpt")
        || lower.starts_with("claude")
        || lower.starts_with("anthropic.")
    {
        ProviderName::Chatgpt
    } else {
        ProviderName::Qwen
    }
}

impl AppState {
    fn provider_config(&self, provider: ProviderName) -> &ProviderConfig {
        match provider {
            ProviderName::Deepseek => &self.config.deepseek,
            ProviderName::Kimi => &self.config.kimi,
            ProviderName::Qwen => &self.config.qwen,
            ProviderName::Chatgpt => &self.config.chatgpt,
            ProviderName::Gemini => &self.config.gemini,
            ProviderName::Mistral => &self.config.mistral,
            ProviderName::Zai => &self.config.zai,
            ProviderName::Meta => &self.config.meta,
        }
    }
}

pub async fn serve_embedded(config: HubServiceConfig) -> Result<()> {
    run_server(AppConfig {
        host: config.host,
        port: config.port,
        api_key: config.api_key,
        qwen: config.qwen,
        deepseek: config.deepseek,
        kimi: config.kimi,
        chatgpt: config.chatgpt,
        gemini: config.gemini,
        mistral: config.mistral,
        zai: config.zai,
        meta: config.meta,
    })
    .await
}

