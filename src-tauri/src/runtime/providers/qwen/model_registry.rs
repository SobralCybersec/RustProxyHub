use once_cell::sync::Lazy;
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};
use tokio::sync::Mutex;

pub const DEFAULT_CONTEXT_WINDOW: usize = 131_072;
pub const MAX_PAYLOAD_SIZE: usize = 10 * 1024 * 1024;

static DEFAULT_CONTEXT_WINDOWS: Lazy<HashMap<&'static str, usize>> = Lazy::new(|| {
    HashMap::from([
        ("qwen3.7-plus", 1_000_000),
        ("qwen3.7-max", 1_000_000),
        ("qwen3.6-plus", 1_000_000),
        ("qwen3.6-plus-preview", 1_000_000),
        ("qwen3.6-max-preview", 262_144),
        ("qwen3.6-27b", 262_144),
        ("qwen3.6-35b-a3b", 262_144),
        ("qwen3.5-plus", 1_000_000),
        ("qwen3.5-flash", 1_000_000),
        ("qwen3.5-omni-plus", 262_144),
        ("qwen3.5-omni-flash", 262_144),
        ("qwen3.5-max-2026-03-08", 262_144),
        ("qwen3.5-397b-a17b", 262_144),
        ("qwen3.5-122b-a10b", 262_144),
        ("qwen3.5-27b", 262_144),
        ("qwen3.5-35b-a3b", 262_144),
        ("qwen3-max-2026-01-23", 262_144),
        ("qwen3-coder-plus", 1_048_576),
        ("qwen3-vl-plus", 262_144),
        ("qwen3-omni-flash-2025-12-01", 65_536),
        ("qwen-plus-2025-07-28", 131_072),
        ("qwen-latest-series-invite-beta-v24", 262_144),
        ("qwen-latest-series-invite-beta-v16", 1_000_000),
    ])
});

static TOKEN_DIVISORS: Lazy<HashMap<&'static str, f64>> = Lazy::new(|| {
    HashMap::from([
        ("qwen3.7-max", 2.2),
        ("qwen3.6-max-preview", 2.2),
        ("qwen3.5-max-2026-03-08", 2.2),
        ("qwen3-max-2026-01-23", 2.2),
        ("qwen-latest-series-invite-beta-v24", 2.2),
        ("qwen3.7-plus", 2.0),
        ("qwen3.6-plus", 2.0),
        ("qwen3.6-plus-preview", 2.0),
        ("qwen3.5-plus", 2.0),
        ("qwen-plus-2025-07-28", 2.0),
        ("qwen-latest-series-invite-beta-v16", 2.0),
        ("qwen3.5-flash", 1.8),
        ("qwen3.5-omni-plus", 1.8),
        ("qwen3.5-omni-flash", 1.7),
        ("qwen3-omni-flash-2025-12-01", 1.7),
        ("qwen3.5-397b-a17b", 1.9),
        ("qwen3.5-122b-a10b", 1.9),
        ("qwen3.6-35b-a3b", 1.9),
        ("qwen3.5-35b-a3b", 1.9),
        ("qwen3.6-27b", 1.9),
        ("qwen3.5-27b", 1.9),
        ("qwen3-coder-plus", 2.3),
        ("qwen3-vl-plus", 2.1),
    ])
});

#[derive(Clone, Default)]
pub struct ModelRegistry {
    context_windows: Arc<Mutex<HashMap<String, usize>>>,
}

impl ModelRegistry {
    pub async fn new() -> Self {
        let mut map = HashMap::new();
        for (model, window) in DEFAULT_CONTEXT_WINDOWS.iter() {
            map.insert((*model).to_owned(), *window);
        }
        Self {
            context_windows: Arc::new(Mutex::new(map)),
        }
    }

    pub async fn context_window(&self, model_id: &str) -> usize {
        let key = normalize_model_id(model_id);
        self.context_windows
            .lock()
            .await
            .get(&key)
            .copied()
            .unwrap_or(DEFAULT_CONTEXT_WINDOW)
    }

    pub async fn fallback_catalog(&self) -> Vec<(String, usize)> {
        let guard = self.context_windows.lock().await;
        let mut catalog = BTreeMap::new();
        for (id, window) in guard.iter() {
            catalog.insert(id.clone(), *window);
            if !id.ends_with("-no-thinking") {
                catalog
                    .entry(format!("{id}-no-thinking"))
                    .or_insert(*window);
            }
        }
        catalog.into_iter().collect()
    }

    pub fn token_divisor(&self, model_id: &str) -> f64 {
        let key = normalize_model_id(model_id);
        TOKEN_DIVISORS.get(key.as_str()).copied().unwrap_or(2.0)
    }

    pub fn estimate_tokens(&self, text: &str, model_id: &str) -> usize {
        ((text.chars().count() as f64) / self.token_divisor(model_id)).ceil() as usize
    }
}

pub fn normalize_model_id(model_id: &str) -> String {
    model_id
        .trim()
        .split_once(':')
        .map(|(_, actual)| actual)
        .unwrap_or(model_id)
        .replace("-no-thinking", "")
}

#[cfg(test)]
mod tests {
    use super::{normalize_model_id, ModelRegistry};

    #[test]
    fn strips_provider_prefix_and_no_thinking_suffix() {
        assert_eq!(
            normalize_model_id("qwen:qwen3.6-max-preview-no-thinking"),
            "qwen3.6-max-preview"
        );
    }

    #[test]
    fn keeps_bare_model_ids_stable() {
        assert_eq!(normalize_model_id("qwen3.7-plus"), "qwen3.7-plus");
    }

    #[test]
    fn strips_prefix_only_when_no_thinking_suffix() {
        assert_eq!(normalize_model_id("qwen:qwen3-30b-a3b"), "qwen3-30b-a3b");
    }

    #[test]
    fn strips_no_thinking_suffix_without_prefix() {
        assert_eq!(
            normalize_model_id("qwen3-30b-a3b-no-thinking"),
            "qwen3-30b-a3b"
        );
    }

    #[test]
    fn empty_string_stays_empty() {
        assert_eq!(normalize_model_id(""), "");
    }

    #[test]
    fn prefix_colon_extracts_model_part_verbatim() {
        // split_once(':') takes everything after the colon; inner whitespace is not stripped
        assert_eq!(normalize_model_id("qwen:qwen3.7-plus"), "qwen3.7-plus");
        assert_eq!(
            normalize_model_id("deepseek:deepseek-v4-pro"),
            "deepseek-v4-pro"
        );
    }

    #[test]
    fn colon_with_no_actual_model_returns_empty() {
        // "prefix:" splits into ("prefix", ""), second part is ""
        assert_eq!(normalize_model_id("prefix:"), "");
    }

    #[tokio::test]
    async fn fallback_catalog_contains_registered_models_and_no_thinking_variants() {
        let registry = ModelRegistry::new().await;
        let catalog = registry.fallback_catalog().await;

        assert!(catalog.iter().any(|(id, _)| id == "qwen3.7-plus"));
        assert!(catalog
            .iter()
            .any(|(id, _)| id == "qwen3.7-plus-no-thinking"));
    }
}
