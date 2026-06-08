use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{ChildStdin, Command},
    sync::{oneshot, Mutex},
};

#[derive(Debug, Clone, Serialize)]
pub struct InitParams {
    pub runtime_dir: String,
    pub headless: bool,
    pub browser: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CaptureHeadersParams {
    pub force_new: bool,
    pub account_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManualLoginParams {
    pub runtime_dir: String,
    pub browser: String,
    pub account_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoginAccountParams {
    pub account_id: String,
    pub email: String,
    pub password: String,
    pub headless: bool,
    pub browser: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CloseAccountParams {
    pub account_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BridgeCaptureResponse {
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub chat_session_id: Option<String>,
    #[serde(default)]
    pub parent_message_id: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BridgeBasicHeadersResponse {
    pub headers: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct RpcResponse {
    id: u64,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct RpcRequest<'a, T: Serialize> {
    id: u64,
    method: &'a str,
    provider: &'a str,
    params: T,
}

#[derive(Clone)]
pub struct PlaywrightBridge {
    provider: String,
    stdin: Arc<Mutex<ChildStdin>>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value>>>>>,
    next_id: Arc<AtomicU64>,
}

#[async_trait]
pub trait BrowserBridge: Send + Sync {
    async fn init(&self, params: InitParams) -> Result<()>;
    async fn capture_headers(&self, params: CaptureHeadersParams) -> Result<BridgeCaptureResponse>;
    async fn basic_headers(&self, account_id: Option<String>)
        -> Result<BridgeBasicHeadersResponse>;
    async fn manual_login(&self, params: ManualLoginParams) -> Result<()>;
    async fn login_account(&self, params: LoginAccountParams) -> Result<()>;
    async fn close_account(&self, params: CloseAccountParams) -> Result<()>;
    async fn shutdown(&self) -> Result<()>;
}

impl PlaywrightBridge {
    pub async fn new(helper_dir: impl AsRef<Path>, provider: impl Into<String>) -> Result<Self> {
        let helper_dir = helper_dir.as_ref();
        let helper_path = helper_dir.join("index.mjs");

        let mut child = Command::new("node")
            .arg(helper_path)
            .current_dir(helper_dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .context("failed to start Playwright bridge helper")?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("helper stdin unavailable"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("helper stdout unavailable"))?;

        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value>>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let pending_reader = Arc::clone(&pending);

        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        if line.trim().is_empty() {
                            continue;
                        }
                        let parsed: Result<RpcResponse> =
                            serde_json::from_str(&line).map_err(|err| anyhow!(err));
                        match parsed {
                            Ok(response) => {
                                let sender = pending_reader.lock().await.remove(&response.id);
                                if let Some(sender) = sender {
                                    let _ = sender.send(match response.error {
                                        Some(error) => Err(anyhow!(error)),
                                        None => Ok(response.result.unwrap_or(Value::Null)),
                                    });
                                }
                            }
                            Err(err) => {
                                eprintln!("[browser-bridge] failed to parse helper output: {err}");
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(err) => {
                        eprintln!("[browser-bridge] helper stdout error: {err}");
                        break;
                    }
                }
            }
        });

        Ok(Self {
            provider: provider.into(),
            stdin: Arc::new(Mutex::new(stdin)),
            pending,
            next_id: Arc::new(AtomicU64::new(1)),
        })
    }

    async fn call<T: Serialize, R: DeserializeOwned>(&self, method: &str, params: T) -> Result<R> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = RpcRequest {
            id,
            method,
            provider: &self.provider,
            params,
        };
        let payload = serde_json::to_vec(&request)?;

        let (sender, receiver) = oneshot::channel();
        self.pending.lock().await.insert(id, sender);

        {
            let mut stdin = self.stdin.lock().await;
            stdin.write_all(&payload).await?;
            stdin.write_all(b"\n").await?;
            stdin.flush().await?;
        }

        let value = receiver
            .await
            .map_err(|_| anyhow!("helper response channel closed"))??;
        Ok(serde_json::from_value(value)?)
    }
}

#[async_trait]
impl BrowserBridge for PlaywrightBridge {
    async fn init(&self, params: InitParams) -> Result<()> {
        self.call::<_, Value>("init", params).await.map(|_| ())
    }

    async fn capture_headers(&self, params: CaptureHeadersParams) -> Result<BridgeCaptureResponse> {
        self.call("capture_headers", params).await
    }

    async fn basic_headers(
        &self,
        account_id: Option<String>,
    ) -> Result<BridgeBasicHeadersResponse> {
        self.call(
            "basic_headers",
            serde_json::json!({ "account_id": account_id }),
        )
        .await
    }

    async fn manual_login(&self, params: ManualLoginParams) -> Result<()> {
        self.call::<_, Value>("manual_login", params)
            .await
            .map(|_| ())
    }

    async fn login_account(&self, params: LoginAccountParams) -> Result<()> {
        self.call::<_, Value>("login_account", params)
            .await
            .map(|_| ())
    }

    async fn close_account(&self, params: CloseAccountParams) -> Result<()> {
        self.call::<_, Value>("close_account", params)
            .await
            .map(|_| ())
    }

    async fn shutdown(&self) -> Result<()> {
        self.call::<_, Value>("shutdown", serde_json::json!({}))
            .await
            .map(|_| ())
    }
}

pub fn helper_dir_from(crate_dir: &str) -> PathBuf {
    PathBuf::from(crate_dir)
        .parent()
        .expect("crate has workspace parent")
        .join("playwright-bridge")
}
