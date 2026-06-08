use anyhow::Result;
use std::{
    env,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone)]
pub struct CacheConfig {
    pub default_ttl: Duration,
    pub response_ttl: Duration,
}

#[derive(Clone)]
pub struct WatchdogConfig {
    pub interval: Duration,
    pub consecutive_failures_threshold: u32,
    pub ram_warning_percent: f64,
    pub ram_critical_percent: f64,
    pub streams_warning_threshold: usize,
    pub streams_critical_threshold: usize,
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub api_key: Option<String>,
    pub headless: bool,
    pub browser: String,
    pub runtime_dir: PathBuf,
    pub data_dir: PathBuf,
    pub db_path: PathBuf,
    pub cache: CacheConfig,
    pub metrics_interval: Duration,
    pub watchdog: WatchdogConfig,
    pub qwen_base_url: String,
    pub qwen_http_endpoint: String,
    pub qwen_api_key: Option<String>,
    pub test_mock_playwright: bool,
    pub chat_timeout: Duration,
}

pub fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate has workspace parent")
        .to_path_buf()
}

pub fn load_config() -> AppConfig {
    let root = workspace_root();
    let runtime_dir = root.join("runtime").join("qwen");
    let data_dir = runtime_dir.join("data");
    let db_path = data_dir.join("qwenproxy.db");

    AppConfig {
        host: env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_owned()),
        port: env::var("PORT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(3000),
        api_key: env::var("API_KEY")
            .ok()
            .filter(|value| !value.trim().is_empty()),
        headless: env::var("HEADLESS")
            .map(|value| value != "false")
            .unwrap_or(true),
        browser: env::var("BROWSER").unwrap_or_else(|_| "chromium".to_owned()),
        runtime_dir,
        data_dir,
        db_path,
        cache: CacheConfig {
            default_ttl: Duration::from_secs(
                env::var("CACHE_TTL")
                    .ok()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(3600),
            ),
            response_ttl: Duration::from_secs(
                env::var("RESPONSE_TTL")
                    .ok()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(1800),
            ),
        },
        metrics_interval: Duration::from_millis(
            env::var("METRICS_INTERVAL")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(10_000),
        ),
        watchdog: WatchdogConfig {
            interval: Duration::from_millis(
                env::var("WATCHDOG_INTERVAL")
                    .ok()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(5_000),
            ),
            consecutive_failures_threshold: env::var("WATCHDOG_FAILURES")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(3),
            ram_warning_percent: env::var("RAM_WARNING")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(80.0),
            ram_critical_percent: env::var("RAM_CRITICAL")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(95.0),
            streams_warning_threshold: env::var("WS_WARNING")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(50),
            streams_critical_threshold: env::var("WS_CRITICAL")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(100),
        },
        qwen_base_url: env::var("QWEN_BASE_URL")
            .unwrap_or_else(|_| "https://chat.qwen.ai".to_owned()),
        qwen_http_endpoint: env::var("QWEN_HTTP_ENDPOINT")
            .unwrap_or_else(|_| "https://api.qwen.ai/v1/chat".to_owned()),
        qwen_api_key: env::var("QWEN_API_KEY")
            .ok()
            .filter(|value| !value.trim().is_empty()),
        test_mock_playwright: env::var("TEST_MOCK_PLAYWRIGHT")
            .map(|value| value == "true" || value == "1")
            .unwrap_or(false),
        chat_timeout: Duration::from_millis(
            env::var("CHAT_TIMEOUT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(120_000),
        ),
    }
}

pub fn ensure_runtime_layout(config: &AppConfig) -> Result<()> {
    std::fs::create_dir_all(&config.runtime_dir)?;
    std::fs::create_dir_all(&config.data_dir)?;
    std::fs::create_dir_all(config.runtime_dir.join("qwen_profiles").join("_default"))?;
    Ok(())
}

pub fn legacy_db_candidates(root: &Path) -> Vec<PathBuf> {
    vec![
        root.join("data").join("qwenproxy.db"),
        root.join("proxy")
            .join("qwenproxy")
            .join("data")
            .join("qwenproxy.db"),
    ]
}

pub fn legacy_accounts_json_candidates(root: &Path) -> Vec<PathBuf> {
    vec![
        root.join("accounts.json"),
        root.join("proxy").join("qwenproxy").join("accounts.json"),
    ]
}
