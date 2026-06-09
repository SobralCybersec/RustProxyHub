use once_cell::sync::Lazy;
use serde_json::Value;
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

    pub async fn sync_from_models(&self, models: &[Value]) {
        let mut guard = self.context_windows.lock().await;
        for model in models {
            let Some(id) = model.get("id").and_then(Value::as_str) else {
                continue;
            };
            let Some(context_window) = model.get("context_window").and_then(Value::as_u64) else {
                continue;
            };
            guard.insert(id.to_owned(), context_window as usize);
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
    model_id.replace("-no-thinking", "")
}
