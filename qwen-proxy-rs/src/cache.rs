use crate::metrics::Metrics;
use serde_json::Value;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::Mutex;

#[derive(Clone)]
struct CacheEntry {
    value: Value,
    expires_at: std::time::Instant,
    size_bytes: usize,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct CacheStats {
    pub connected: bool,
    pub keys_count: usize,
    pub memory_usage: String,
}

#[derive(Clone)]
pub struct MemoryCache {
    inner: Arc<Mutex<HashMap<String, CacheEntry>>>,
    default_ttl: Duration,
    max_entries: usize,
    metrics: Metrics,
}

impl MemoryCache {
    pub fn new(default_ttl: Duration, max_entries: usize, metrics: Metrics) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            default_ttl,
            max_entries,
            metrics,
        }
    }

    pub async fn get_json(&self, key: &str) -> Option<Value> {
        let started = std::time::Instant::now();
        let mut guard = self.inner.lock().await;
        self.metrics
            .histogram("cache.get.latency", started.elapsed().as_millis() as f64)
            .await;

        let entry = guard.get(key)?.clone();
        if entry.expires_at <= std::time::Instant::now() {
            guard.remove(key);
            self.metrics.increment("cache.miss", 1.0).await;
            return None;
        }

        self.metrics.increment("cache.hit", 1.0).await;
        Some(entry.value)
    }

    pub async fn set_json(&self, key: impl Into<String>, value: Value, ttl: Option<Duration>) {
        let key = key.into();
        let size_bytes = key.len() + value.to_string().len();
        let mut guard = self.inner.lock().await;

        if guard.len() >= self.max_entries {
            if let Some(first_key) = guard.keys().next().cloned() {
                guard.remove(&first_key);
            }
        }

        guard.insert(
            key,
            CacheEntry {
                value,
                expires_at: std::time::Instant::now() + ttl.unwrap_or(self.default_ttl),
                size_bytes,
            },
        );
        self.metrics.increment("cache.set", 1.0).await;
        self.metrics
            .histogram("cache.value.size", size_bytes as f64)
            .await;
    }

    pub async fn flush_expired(&self) {
        let mut guard = self.inner.lock().await;
        let now = std::time::Instant::now();
        guard.retain(|_, entry| entry.expires_at > now);
    }

    pub async fn flush_all(&self) {
        self.inner.lock().await.clear();
        self.metrics.increment("cache.flushed", 1.0).await;
    }

    pub async fn stats(&self) -> CacheStats {
        let guard = self.inner.lock().await;
        let total_bytes: usize = guard.values().map(|entry| entry.size_bytes).sum();
        CacheStats {
            connected: true,
            keys_count: guard.len(),
            memory_usage: format!("{:.2}KB", total_bytes as f64 / 1024.0),
        }
    }
}
