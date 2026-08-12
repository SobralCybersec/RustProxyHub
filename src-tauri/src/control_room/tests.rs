use super::{
    add_qwen_account_db, ensure_qwen_db, normalize_provider, provider_base_url, service_names,
    ControlState, RuntimeDiagnostics, ServiceRuntimeStatus, StartupConfig,
};
use axum::{
    http::{header, HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;

const MANUAL_LOGIN_PROVIDERS: [&str; 8] = [
    "qwen", "deepseek", "kimi", "chatgpt", "gemini", "mistral", "zai", "meta",
];

fn temp_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("rustproxyhub-control-room-{name}-{unique}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn test_control_state(runtime_ready: bool, api_key: Option<&str>) -> ControlState {
    let root = temp_dir("state");
    let mut statuses = HashMap::new();
    let mut logs = HashMap::new();
    for name in service_names() {
        statuses.insert(name.to_owned(), ServiceRuntimeStatus::default());
        logs.insert(name.to_owned(), VecDeque::new());
    }

    ControlState {
        workspace_root: root.clone(),
        app_data_dir: root.join("app-data"),
        qwen_runtime_dir: root.join("app-data/providers/qwen"),
        runtime: RuntimeDiagnostics {
            node_path: runtime_ready.then(|| "node".to_owned()),
            node_source: runtime_ready.then(|| "test".to_owned()),
            helper_dir: runtime_ready.then(|| "helper".to_owned()),
            browser_available: runtime_ready,
            single_runner_ready: runtime_ready,
            issues: if runtime_ready {
                Vec::new()
            } else {
                vec!["test runtime unavailable".to_owned()]
            },
        },
        startup_config: StartupConfig {
            mode: "manual".to_owned(),
            services: Vec::new(),
        },
        client: reqwest::Client::builder()
            .connect_timeout(Duration::from_millis(100))
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap(),
        hub_api_key: api_key.map(str::to_owned),
        statuses: Arc::new(Mutex::new(statuses)),
        logs: Arc::new(Mutex::new(logs)),
        open_provider_login_sessions: Arc::new(Mutex::new(HashSet::new())),
        open_qwen_account_login_sessions: Arc::new(Mutex::new(HashSet::new())),
        tasks: Arc::new(std::sync::Mutex::new(HashMap::new())),
        app_handle: None,
        dashboard_notify: Arc::new(tokio::sync::Notify::new()),
    }
}

async fn mock_provider_server() -> (String, tokio::task::JoinHandle<()>) {
    let app = Router::new()
        .route("/health", get(|| async { Json(json!({ "status": "ok" })) }))
        .route(
            "/v1/models",
            get(|headers: HeaderMap| async move {
                let authorized = headers
                    .get(header::AUTHORIZATION)
                    .and_then(|value| value.to_str().ok())
                    == Some("Bearer test-key");
                if authorized {
                    (StatusCode::OK, Json(json!({ "data": [] })))
                } else {
                    (
                        StatusCode::UNAUTHORIZED,
                        Json(json!({ "error": { "message": "bad request" } })),
                    )
                }
            }),
        )
        .route(
            "/admin/manual_login",
            post(|headers: HeaderMap, Json(body): Json<Value>| async move {
                let authorized = headers
                    .get(header::AUTHORIZATION)
                    .and_then(|value| value.to_str().ok())
                    == Some("Bearer test-key");
                if authorized && body["browser"] == "chrome" {
                    (StatusCode::OK, Json(json!({ "ok": true })))
                } else {
                    (
                        StatusCode::UNAUTHORIZED,
                        Json(json!({ "error": { "message": "bad request" } })),
                    )
                }
            }),
        )
        .route(
            "/admin/close_login",
            post(|headers: HeaderMap| async move {
                let authorized = headers
                    .get(header::AUTHORIZATION)
                    .and_then(|value| value.to_str().ok())
                    == Some("Bearer test-key");
                if authorized {
                    (StatusCode::OK, Json(json!({ "ok": true })))
                } else {
                    (
                        StatusCode::UNAUTHORIZED,
                        Json(json!({ "error": { "message": "bad request" } })),
                    )
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{address}"), handle)
}

#[test]
fn provider_base_urls_keep_embedded_ports() {
    assert_eq!(provider_base_url("qwen"), "http://127.0.0.1:3000");
    assert_eq!(provider_base_url("deepseek"), "http://127.0.0.1:3001");
    assert_eq!(provider_base_url("kimi"), "http://127.0.0.1:3002");
    assert_eq!(provider_base_url("chatgpt"), "http://127.0.0.1:3003");
    assert_eq!(provider_base_url("gemini"), "http://127.0.0.1:3004");
    assert_eq!(provider_base_url("mistral"), "http://127.0.0.1:3005");
    assert_eq!(provider_base_url("zai"), "http://127.0.0.1:3006");
    assert_eq!(provider_base_url("meta"), "http://127.0.0.1:3007");
}

#[test]
fn provider_normalization_accepts_every_manual_login_provider() {
    for provider in MANUAL_LOGIN_PROVIDERS {
        assert_eq!(
            normalize_provider(&provider.to_uppercase()).unwrap(),
            provider
        );
    }
    assert!(normalize_provider("claude").is_err());
}

#[tokio::test]
async fn manual_login_preflight_rejects_every_provider_when_runtime_is_blocked() {
    let state = test_control_state(false, None);
    for provider in MANUAL_LOGIN_PROVIDERS {
        let error = state.ensure_provider_ready(provider).await.unwrap_err();
        assert!(error.to_string().contains(provider));
        assert!(error.to_string().contains("test runtime unavailable"));
    }
}

#[tokio::test]
async fn readiness_and_authenticated_admin_post_use_shared_provider_path() {
    let state = test_control_state(true, Some("test-key"));
    let (base_url, server) = mock_provider_server().await;

    for provider in MANUAL_LOGIN_PROVIDERS {
        state
            .wait_for_provider_ready_at(provider, &base_url, Duration::from_secs(1))
            .await
            .unwrap();
    }

    let (status, response) = state
        .call_json_post_with_timeout(
            &base_url,
            "/admin/manual_login",
            state.hub_api_key.as_deref(),
            &json!({ "browser": "chrome" }),
            Some(Duration::from_secs(1)),
        )
        .await
        .unwrap();
    assert_eq!(status, 200);
    assert_eq!(response["ok"], true);

    let (status, _) = state
        .call_json_post(
            &base_url,
            "/admin/manual_login",
            None,
            &json!({ "browser": "chrome" }),
        )
        .await
        .unwrap();
    assert_eq!(status, 401);

    let (status, response) = state
        .call_json_post(
            &base_url,
            "/admin/close_login",
            state.hub_api_key.as_deref(),
            &json!({}),
        )
        .await
        .unwrap();
    assert_eq!(status, 200);
    assert_eq!(response["ok"], true);

    let (status, _) = state
        .call_json_get(&base_url, "/v1/models", state.hub_api_key.as_deref())
        .await
        .unwrap();
    assert_eq!(status, 200);
    server.abort();
}

#[tokio::test]
async fn readiness_surfaces_provider_start_failure() {
    let state = test_control_state(true, None);
    state.statuses.lock().await.insert(
        "qwen".to_owned(),
        ServiceRuntimeStatus {
            running: false,
            started_at: None,
            last_error: Some("listener failed".to_owned()),
        },
    );

    let error = state
        .wait_for_provider_ready_at("qwen", "http://127.0.0.1:0", Duration::from_millis(10))
        .await
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "qwen failed to start for manual login: listener failed"
    );
}

#[tokio::test]
async fn runtime_block_marks_all_services_with_last_error() {
    let mut statuses = HashMap::new();
    let mut logs = HashMap::new();
    for name in service_names() {
        statuses.insert(name.to_owned(), ServiceRuntimeStatus::default());
        logs.insert(name.to_owned(), VecDeque::new());
    }

    let state = ControlState {
        workspace_root: PathBuf::from("G:/repo"),
        app_data_dir: PathBuf::from("G:/repo/runtime"),
        qwen_runtime_dir: PathBuf::from("G:/repo/runtime/providers/qwen"),
        runtime: RuntimeDiagnostics {
            node_path: None,
            node_source: None,
            helper_dir: None,
            browser_available: false,
            single_runner_ready: false,
            issues: vec!["Bundled node.exe not found in Tauri resources.".to_owned()],
        },
        startup_config: StartupConfig {
            mode: "manual".to_owned(),
            services: Vec::new(),
        },
        client: reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .tcp_nodelay(true)
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(16)
            .build()
            .unwrap(),
        hub_api_key: None,
        statuses: Arc::new(Mutex::new(statuses)),
        logs: Arc::new(Mutex::new(logs)),
        open_provider_login_sessions: Arc::new(Mutex::new(HashSet::new())),
        open_qwen_account_login_sessions: Arc::new(Mutex::new(HashSet::new())),
        tasks: Arc::new(std::sync::Mutex::new(HashMap::new())),
        app_handle: None,
        dashboard_notify: Arc::new(tokio::sync::Notify::new()),
    };

    state.mark_runtime_blocked().await;

    let statuses = state.statuses.lock().await;
    for name in service_names() {
        assert_eq!(
            statuses
                .get(name)
                .and_then(|entry| entry.last_error.as_deref()),
            Some("Bundled node.exe not found in Tauri resources.")
        );
    }
}

#[tokio::test]
async fn dashboard_overview_reports_runtime_and_qwen_counts() {
    let workspace_root = temp_dir("overview-workspace");
    let app_data_dir = workspace_root.join("app-data");
    let qwen_runtime_dir = app_data_dir.join("providers").join("qwen");

    ensure_qwen_db(&qwen_runtime_dir, &workspace_root).unwrap();
    add_qwen_account_db(
        &qwen_runtime_dir,
        &workspace_root,
        "pilot@example.com",
        "secret",
    )
    .unwrap();

    let mut statuses = HashMap::new();
    let mut logs = HashMap::new();
    for name in service_names() {
        statuses.insert(name.to_owned(), ServiceRuntimeStatus::default());
        logs.insert(name.to_owned(), VecDeque::new());
    }

    let state = ControlState {
        workspace_root: workspace_root.clone(),
        app_data_dir: app_data_dir.clone(),
        qwen_runtime_dir: qwen_runtime_dir.clone(),
        runtime: RuntimeDiagnostics {
            node_path: Some("C:/bundle/resources/node/node.exe".to_owned()),
            node_source: Some("bundled-resource".to_owned()),
            helper_dir: Some("C:/bundle/resources/playwright-bridge".to_owned()),
            browser_available: true,
            single_runner_ready: true,
            issues: Vec::new(),
        },
        startup_config: StartupConfig {
            mode: "manual".to_owned(),
            services: Vec::new(),
        },
        client: reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .tcp_nodelay(true)
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(16)
            .build()
            .unwrap(),
        hub_api_key: None,
        statuses: Arc::new(Mutex::new(statuses)),
        logs: Arc::new(Mutex::new(logs)),
        open_provider_login_sessions: Arc::new(Mutex::new(HashSet::from([String::from("qwen")]))),
        open_qwen_account_login_sessions: Arc::new(Mutex::new(HashSet::from([String::from(
            "acct-1",
        )]))),
        tasks: Arc::new(std::sync::Mutex::new(HashMap::new())),
        app_handle: None,
        dashboard_notify: Arc::new(tokio::sync::Notify::new()),
    };

    let overview = state.build_dashboard_overview().await.unwrap();

    assert!(overview.runtime.single_runner_ready);
    assert_eq!(
        overview.runtime.node_path.as_deref(),
        Some("C:/bundle/resources/node/node.exe")
    );
    assert_eq!(overview.qwen_account_count, 1);
    assert_eq!(
        overview.open_provider_login_sessions,
        vec![String::from("qwen")]
    );
    assert_eq!(
        overview.open_qwen_account_login_sessions,
        vec![String::from("acct-1")]
    );
}
