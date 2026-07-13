use super::metrics::Metrics;
use serde_json::Value;
use std::{collections::{HashMap, VecDeque}, sync::Arc, time::Duration};
use tokio::sync::RwLock;

#[derive(Clone)]
struct CacheEntry {
    value: Value,
    expires_at: std::time::Instant,
    size_bytes: usize,
}

struct CacheInner {
    map: HashMap<String, CacheEntry>,
    order: VecDeque<String>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct CacheStats {
    pub connected: bool,
    pub keys_count: usize,
    pub memory_usage: String,
}

#[derive(Clone)]
pub struct MemoryCache {
    inner: Arc<RwLock<CacheInner>>,
    default_ttl: Duration,
    max_entries: usize,
    metrics: Metrics,
}

impl MemoryCache {
    pub fn new(default_ttl: Duration, max_entries: usize, metrics: Metrics) -> Self {
        Self {
            inner: Arc::new(RwLock::new(CacheInner {
                map: HashMap::new(),
                order: VecDeque::new(),
            })),
            default_ttl,
            max_entries,
            metrics,
        }
    }

    pub async fn get_json(&self, key: &str) -> Option<Value> {
        let started = std::time::Instant::now();
        let guard = self.inner.read().await;
        self.metrics
            .histogram("cache.get.latency", started.elapsed().as_millis() as f64)
            .await;

        let entry = guard.map.get(key)?.clone();
        if entry.expires_at <= std::time::Instant::now() {
            // ponytail: skip write on expiry here; flush_expired owns cleanup
            self.metrics.increment("cache.miss", 1.0).await;
            return None;
        }

        self.metrics.increment("cache.hit", 1.0).await;
        Some(entry.value)
    }

    pub async fn set_json(&self, key: impl Into<String>, value: Value, ttl: Option<Duration>) {
        let key = key.into();
        // ponytail: rough size estimate, upgrade to exact if cache memory tracking needed
        let size_bytes = key.len() + 64;
        let mut guard = self.inner.write().await;

        if guard.map.len() >= self.max_entries {
            if let Some(evict_key) = guard.order.pop_front() {
                guard.map.remove(&evict_key);
            }
        }

        guard.map.insert(
            key.clone(),
            CacheEntry {
                value,
                expires_at: std::time::Instant::now() + ttl.unwrap_or(self.default_ttl),
                size_bytes,
            },
        );
        guard.order.push_back(key);

        self.metrics.increment("cache.set", 1.0).await;
        self.metrics
            .histogram("cache.value.size", size_bytes as f64)
            .await;
    }

    pub async fn flush_expired(&self) {
        let mut guard = self.inner.write().await;
        let now = std::time::Instant::now();
        guard.map.retain(|_, entry| entry.expires_at > now);
        // collect into owned set first — can't split-borrow map+order through the guard's Deref
        let live: std::collections::HashSet<String> = guard.map.keys().cloned().collect();
        guard.order.retain(|k| live.contains(k));
    }

    pub async fn flush_all(&self) {
        let mut guard = self.inner.write().await;
        guard.map.clear();
        guard.order.clear();
        drop(guard);
        self.metrics.increment("cache.flushed", 1.0).await;
    }

    pub async fn stats(&self) -> CacheStats {
        let guard = self.inner.read().await;
        let total_bytes: usize = guard.map.values().map(|entry| entry.size_bytes).sum();
        CacheStats {
            connected: true,
            keys_count: guard.map.len(),
            memory_usage: format!("{:.2}KB", total_bytes as f64 / 1024.0),
        }
    }
}
