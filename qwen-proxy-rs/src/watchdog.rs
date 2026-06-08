use crate::{
    cache::MemoryCache, config::WatchdogConfig, metrics::Metrics, stream_registry::StreamRegistry,
};
use serde::Serialize;
use std::{sync::Arc, time::Duration};
use sysinfo::System;
use tokio::sync::Mutex;

#[derive(Clone, Debug, Serialize)]
pub struct HealthSnapshot {
    pub ram: String,
    pub streams: String,
    pub overall: String,
    pub consecutive_failures: u32,
    pub active_streams: usize,
    pub memory_percent: f64,
    pub checked_at: u64,
}

#[derive(Clone)]
pub struct Watchdog {
    latest: Arc<Mutex<HealthSnapshot>>,
}

impl Watchdog {
    pub fn start(
        config: WatchdogConfig,
        metrics: Metrics,
        stream_registry: StreamRegistry,
        cache: MemoryCache,
        chat_timeout: Duration,
    ) -> Self {
        let latest = Arc::new(Mutex::new(HealthSnapshot {
            ram: "ok".to_owned(),
            streams: "ok".to_owned(),
            overall: "healthy".to_owned(),
            consecutive_failures: 0,
            active_streams: 0,
            memory_percent: 0.0,
            checked_at: proxy_core::current_timestamp(),
        }));

        let latest_task = Arc::clone(&latest);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(config.interval);
            let mut consecutive_failures = 0u32;
            let mut system = System::new_all();

            loop {
                interval.tick().await;
                system.refresh_memory();

                let total_memory = system.total_memory().max(1) as f64;
                let used_memory = system.used_memory() as f64;
                let memory_percent = (used_memory / total_memory) * 100.0;
                let active_streams = stream_registry.active_count().await;

                let ram = if memory_percent > config.ram_critical_percent {
                    "critical"
                } else if memory_percent > config.ram_warning_percent {
                    "warning"
                } else {
                    "ok"
                };

                let streams = if active_streams > config.streams_critical_threshold {
                    "blocked"
                } else if active_streams > config.streams_warning_threshold {
                    "congested"
                } else {
                    "ok"
                };

                let overall = if ram == "critical" || streams == "blocked" {
                    "unhealthy"
                } else if ram == "warning" || streams == "congested" {
                    "degraded"
                } else {
                    "healthy"
                };

                if overall == "unhealthy" {
                    consecutive_failures += 1;
                    if consecutive_failures >= config.consecutive_failures_threshold {
                        metrics.increment("watchdog.recovery.triggered", 1.0).await;
                        cache.flush_expired().await;
                        if ram == "critical" {
                            cache.flush_all().await;
                        }
                        let _ = stream_registry.prune_older_than(chat_timeout).await;
                        metrics.increment("watchdog.recovery.success", 1.0).await;
                        consecutive_failures = 0;
                    }
                } else {
                    consecutive_failures = 0;
                }

                metrics
                    .gauge(
                        "memory.heap.used",
                        system.used_memory().saturating_mul(1024) as f64,
                    )
                    .await;
                metrics
                    .gauge(
                        "memory.heap.total",
                        system.total_memory().saturating_mul(1024) as f64,
                    )
                    .await;
                metrics
                    .gauge(
                        "watchdog.ram.status",
                        if ram == "ok" {
                            0.0
                        } else if ram == "warning" {
                            1.0
                        } else {
                            2.0
                        },
                    )
                    .await;
                metrics
                    .gauge(
                        "watchdog.overall",
                        if overall == "healthy" {
                            0.0
                        } else if overall == "degraded" {
                            1.0
                        } else {
                            2.0
                        },
                    )
                    .await;

                *latest_task.lock().await = HealthSnapshot {
                    ram: ram.to_owned(),
                    streams: streams.to_owned(),
                    overall: overall.to_owned(),
                    consecutive_failures,
                    active_streams,
                    memory_percent,
                    checked_at: proxy_core::current_timestamp(),
                };
            }
        });

        Self { latest }
    }

    pub async fn snapshot(&self) -> HealthSnapshot {
        self.latest.lock().await.clone()
    }
}
