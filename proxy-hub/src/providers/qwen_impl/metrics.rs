use crate::proxy_core::current_timestamp;
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;

#[derive(Clone)]
enum MetricState {
    Counter(f64),
    Gauge(f64),
    Histogram {
        count: u64,
        sum: f64,
        buckets: Vec<(f64, u64)>,
    },
}

#[derive(Clone)]
struct MetricDefinition {
    help: &'static str,
    metric_type: &'static str,
    state: MetricState,
}

#[derive(Clone, Default)]
pub struct Metrics {
    inner: Arc<Mutex<HashMap<String, MetricDefinition>>>,
}

impl Metrics {
    pub async fn new() -> Self {
        let metrics = Self::default();
        metrics.register_defaults().await;
        metrics
    }

    async fn register_defaults(&self) {
        let mut guard = self.inner.lock().await;
        let histogram_buckets = vec![
            5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0,
        ];
        let defaults = [
            ("requests.total", "counter", "Total requests processed"),
            ("requests.errors", "counter", "Total request errors"),
            ("streams.active", "gauge", "Active SSE streams"),
            ("streams.errors", "counter", "Stream errors"),
            ("memory.heap.used", "gauge", "Memory used (bytes)"),
            ("memory.heap.total", "gauge", "Total system memory (bytes)"),
            ("cache.set", "counter", "Cache set operations"),
            ("cache.hit", "counter", "Cache hits"),
            ("cache.miss", "counter", "Cache misses"),
            ("cache.deleted", "counter", "Cache deletions"),
            ("cache.flushed", "counter", "Cache flushes"),
            (
                "watchdog.ram.status",
                "gauge",
                "Watchdog RAM status (0=ok, 1=warning, 2=critical)",
            ),
            (
                "watchdog.overall",
                "gauge",
                "Watchdog overall status (0=healthy, 1=degraded, 2=unhealthy)",
            ),
            (
                "watchdog.recovery.triggered",
                "counter",
                "Recovery attempts triggered",
            ),
            (
                "watchdog.recovery.success",
                "counter",
                "Successful recoveries",
            ),
            ("watchdog.recovery.failed", "counter", "Failed recoveries"),
            ("cache.value.size", "histogram", "Cache value size (bytes)"),
            ("cache.get.latency", "histogram", "Cache get latency (ms)"),
            ("latency.request", "histogram", "Request latency (ms)"),
        ];

        for (name, metric_type, help) in defaults {
            let state = match metric_type {
                "counter" => MetricState::Counter(0.0),
                "gauge" => MetricState::Gauge(0.0),
                "histogram" => MetricState::Histogram {
                    count: 0,
                    sum: 0.0,
                    buckets: histogram_buckets
                        .iter()
                        .map(|bucket| (*bucket, 0))
                        .collect(),
                },
                _ => continue,
            };
            guard.insert(
                name.to_owned(),
                MetricDefinition {
                    help,
                    metric_type,
                    state,
                },
            );
        }
    }

    pub async fn increment(&self, name: &str, by: f64) {
        let mut guard = self.inner.lock().await;
        if let Some(metric) = guard.get_mut(name) {
            if let MetricState::Counter(value) = &mut metric.state {
                *value += by;
            }
        }
    }

    pub async fn gauge(&self, name: &str, value: f64) {
        let mut guard = self.inner.lock().await;
        if let Some(metric) = guard.get_mut(name) {
            if let MetricState::Gauge(slot) = &mut metric.state {
                *slot = value;
            }
        }
    }

    pub async fn histogram(&self, name: &str, value: f64) {
        let mut guard = self.inner.lock().await;
        if let Some(metric) = guard.get_mut(name) {
            if let MetricState::Histogram {
                count,
                sum,
                buckets,
            } = &mut metric.state
            {
                *count += 1;
                *sum += value;
                for (bucket, bucket_count) in buckets.iter_mut() {
                    if value <= *bucket {
                        *bucket_count += 1;
                    }
                }
            }
        }
    }

    pub async fn snapshot_json(&self) -> Value {
        let guard = self.inner.lock().await;
        let mut metrics = serde_json::Map::new();
        for (name, definition) in guard.iter() {
            let value = match &definition.state {
                MetricState::Counter(value) | MetricState::Gauge(value) => json!(value),
                MetricState::Histogram {
                    count,
                    sum,
                    buckets,
                } => json!({
                    "count": count,
                    "sum": sum,
                    "buckets": buckets.iter().map(|(le, count)| json!({ "le": le, "count": count })).collect::<Vec<_>>()
                }),
            };
            metrics.insert(
                name.clone(),
                json!({
                    "type": definition.metric_type,
                    "help": definition.help,
                    "value": value,
                    "timestamp": current_timestamp(),
                }),
            );
        }
        Value::Object(metrics)
    }

    pub async fn format_prometheus(&self) -> String {
        let guard = self.inner.lock().await;
        let mut output = String::new();
        for (name, definition) in guard.iter() {
            output.push_str(&format!("# HELP {name} {}\n", definition.help));
            output.push_str(&format!("# TYPE {name} {}\n", definition.metric_type));
            match &definition.state {
                MetricState::Counter(value) | MetricState::Gauge(value) => {
                    output.push_str(&format!("{name} {value}\n"));
                }
                MetricState::Histogram {
                    count,
                    sum,
                    buckets,
                } => {
                    for (le, bucket_count) in buckets {
                        output.push_str(&format!("{name}_bucket{{le=\"{le}\"}} {bucket_count}\n"));
                    }
                    output.push_str(&format!("{name}_sum {sum}\n"));
                    output.push_str(&format!("{name}_count {count}\n"));
                }
            }
        }
        output
    }
}
