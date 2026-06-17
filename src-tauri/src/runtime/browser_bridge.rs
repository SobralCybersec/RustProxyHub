use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{ChildStdin, Command},
    sync::{oneshot, Mutex},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
    #[serde(default)]
    pub request_payload: Option<Value>,
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
    helper_dir: Arc<PathBuf>,
    node_path: Arc<PathBuf>,
    log_path: Arc<PathBuf>,
    process: Arc<Mutex<Option<BridgeProcess>>>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value>>>>>,
    next_id: Arc<AtomicU64>,
    init_params: Arc<Mutex<Option<InitParams>>>,
    initialized: Arc<Mutex<bool>>,
    request_timeout: Duration,
}

#[derive(Clone)]
struct BridgeProcess {
    pid: u32,
    stdin: Arc<Mutex<ChildStdin>>,
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
        Self::new_with_node(helper_dir, None::<PathBuf>, provider).await
    }

    pub async fn new_with_node(
        helper_dir: impl AsRef<Path>,
        node_path: Option<PathBuf>,
        provider: impl Into<String>,
    ) -> Result<Self> {
        let provider = provider.into();
        let helper_dir = normalize_spawn_path(
            helper_dir
                .as_ref()
                .canonicalize()
                .context("failed to resolve playwright helper directory")?,
        );
        let node = node_path
            .map(|path| normalize_spawn_path(path.canonicalize().unwrap_or(path)))
            .unwrap_or_else(|| PathBuf::from("node"));
        let log_path = Arc::new(bridge_log_path(&provider)?);
        reset_bridge_log(log_path.as_ref())?;
        let request_timeout = Duration::from_millis(
            std::env::var("RUST_PROXY_HUB_BRIDGE_TIMEOUT_MS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(120_000),
        );

        Ok(Self {
            provider,
            helper_dir: Arc::new(helper_dir),
            node_path: Arc::new(node),
            log_path,
            process: Arc::new(Mutex::new(None)),
            pending: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(AtomicU64::new(1)),
            init_params: Arc::new(Mutex::new(None)),
            initialized: Arc::new(Mutex::new(false)),
            request_timeout,
        })
    }

    pub fn log_path(&self) -> &Path {
        self.log_path.as_ref().as_path()
    }

    pub async fn request<T: Serialize, R: DeserializeOwned>(
        &self,
        method: &str,
        params: T,
    ) -> Result<R> {
        self.request_raw(method, params).await
    }

    async fn request_raw<T: Serialize, R: DeserializeOwned>(
        &self,
        method: &str,
        params: T,
    ) -> Result<R> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = RpcRequest {
            id,
            method,
            provider: &self.provider,
            params,
        };
        let payload = serde_json::to_vec(&request)?;
        append_bridge_log(
            self.log_path.as_ref(),
            format!("request id={id} method={method}"),
        );

        let stdin = self.ensure_process().await?;
        let (sender, receiver) = oneshot::channel();
        self.pending.lock().await.insert(id, sender);

        {
            let mut stdin = stdin.lock().await;
            if let Err(err) = stdin.write_all(&payload).await {
                self.pending.lock().await.remove(&id);
                return Err(err.into());
            }
            if let Err(err) = stdin.write_all(b"\n").await {
                self.pending.lock().await.remove(&id);
                return Err(err.into());
            }
            if let Err(err) = stdin.flush().await {
                self.pending.lock().await.remove(&id);
                return Err(err.into());
            }
        }

        let value = match tokio::time::timeout(self.request_timeout, receiver).await {
            Ok(Ok(result)) => result.map_err(|err| {
                anyhow!(
                    "helper method {method} failed; inspect {}",
                    self.log_path.display()
                )
                .context(err)
            })?,
            Ok(Err(err)) => {
                return Err(anyhow!(
                    "helper response channel closed for {method}; inspect {}",
                    self.log_path.display()
                )
                .context(err))
            }
            Err(_) => {
                self.pending.lock().await.remove(&id);
                return Err(anyhow!(
                    "helper request timed out after {}ms: {method}; inspect {}",
                    self.request_timeout.as_millis(),
                    self.log_path.display()
                ));
            }
        };
        Ok(serde_json::from_value(value)?)
    }

    async fn is_initialized_with(&self, params: &InitParams) -> bool {
        let initialized = *self.initialized.lock().await;
        if !initialized {
            return false;
        }
        self.init_params.lock().await.as_ref() == Some(params)
    }

    async fn ensure_process(&self) -> Result<Arc<Mutex<ChildStdin>>> {
        if let Some(process) = self.process.lock().await.as_ref() {
            return Ok(Arc::clone(&process.stdin));
        }

        let mut guard = self.process.lock().await;
        if let Some(process) = guard.as_ref() {
            return Ok(Arc::clone(&process.stdin));
        }

        let mut child = Command::new(self.node_path.as_ref())
            // Use relative entrypoint once cwd is set to avoid Windows drive-letter parsing edge cases.
            .arg("index.mjs")
            .current_dir(self.helper_dir.as_ref())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .context("failed to start Playwright bridge helper")?;
        let child_id = child.id().unwrap_or_default();
        append_bridge_log(
            self.log_path.as_ref(),
            format!(
                "spawn pid={child_id} provider={} cwd={} node={}",
                self.provider,
                self.helper_dir.display(),
                self.node_path.display()
            ),
        );

        let stdin = Arc::new(Mutex::new(
            child
                .stdin
                .take()
                .ok_or_else(|| anyhow!("helper stdin unavailable"))?,
        ));
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("helper stdout unavailable"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("helper stderr unavailable"))?;

        self.spawn_stdout_reader(stdout);
        self.spawn_stderr_reader(stderr);
        self.spawn_exit_waiter(child, child_id);

        *guard = Some(BridgeProcess {
            pid: child_id,
            stdin: Arc::clone(&stdin),
        });
        Ok(stdin)
    }

    fn spawn_stdout_reader(&self, stdout: tokio::process::ChildStdout) {
        let pending_reader = Arc::clone(&self.pending);
        let log_for_stdout = Arc::clone(&self.log_path);

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
                                append_bridge_log(
                                    log_for_stdout.as_ref(),
                                    format!(
                                        "stdout rpc id={} error={}",
                                        response.id,
                                        response.error.is_some()
                                    ),
                                );
                                let sender = pending_reader.lock().await.remove(&response.id);
                                if let Some(sender) = sender {
                                    let _ = sender.send(match response.error {
                                        Some(error) => Err(anyhow!(error)),
                                        None => Ok(response.result.unwrap_or(Value::Null)),
                                    });
                                }
                            }
                            Err(err) => {
                                append_bridge_log(
                                    log_for_stdout.as_ref(),
                                    format!("stdout parse error: {err}; line={line}"),
                                );
                                eprintln!("[browser-bridge] failed to parse helper output: {err}");
                            }
                        }
                    }
                    Ok(None) => {
                        append_bridge_log(log_for_stdout.as_ref(), "stdout closed".to_owned());
                        break;
                    }
                    Err(err) => {
                        append_bridge_log(
                            log_for_stdout.as_ref(),
                            format!("stdout read error: {err}"),
                        );
                        eprintln!("[browser-bridge] helper stdout error: {err}");
                        break;
                    }
                }
            }
        });
    }

    fn spawn_stderr_reader(&self, stderr: tokio::process::ChildStderr) {
        let log_for_stderr = Arc::clone(&self.log_path);

        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        append_bridge_log(log_for_stderr.as_ref(), format!("stderr: {line}"));
                    }
                    Ok(None) => {
                        append_bridge_log(log_for_stderr.as_ref(), "stderr closed".to_owned());
                        break;
                    }
                    Err(err) => {
                        append_bridge_log(
                            log_for_stderr.as_ref(),
                            format!("stderr read error: {err}"),
                        );
                        break;
                    }
                }
            }
        });
    }

    fn spawn_exit_waiter(&self, mut child: tokio::process::Child, child_id: u32) {
        let log_for_exit = Arc::clone(&self.log_path);
        let pending_for_exit = Arc::clone(&self.pending);
        let process_for_exit = Arc::clone(&self.process);
        let initialized_for_exit = Arc::clone(&self.initialized);

        tokio::spawn(async move {
            match child.wait().await {
                Ok(status) => {
                    append_bridge_log(
                        log_for_exit.as_ref(),
                        format!("process exit pid={child_id} status={status}"),
                    );
                }
                Err(err) => {
                    append_bridge_log(
                        log_for_exit.as_ref(),
                        format!("process wait error pid={child_id}: {err}"),
                    );
                }
            }

            let mut process = process_for_exit.lock().await;
            if process.as_ref().map(|item| item.pid) == Some(child_id) {
                *process = None;
                *initialized_for_exit.lock().await = false;
            }

            let mut pending = pending_for_exit.lock().await;
            for (_, sender) in pending.drain() {
                let _ = sender.send(Err(anyhow!(
                    "helper process exited unexpectedly; inspect {}",
                    log_for_exit.display()
                )));
            }
        });
    }
}

#[async_trait]
impl BrowserBridge for PlaywrightBridge {
    async fn init(&self, params: InitParams) -> Result<()> {
        if self.is_initialized_with(&params).await {
            return Ok(());
        }
        *self.init_params.lock().await = Some(params.clone());
        self.request_raw::<_, Value>("init", params).await?;
        *self.initialized.lock().await = true;
        Ok(())
    }

    async fn capture_headers(&self, params: CaptureHeadersParams) -> Result<BridgeCaptureResponse> {
        self.request("capture_headers", params).await
    }

    async fn basic_headers(
        &self,
        account_id: Option<String>,
    ) -> Result<BridgeBasicHeadersResponse> {
        self.request(
            "basic_headers",
            serde_json::json!({ "account_id": account_id }),
        )
        .await
    }

    async fn manual_login(&self, params: ManualLoginParams) -> Result<()> {
        self.request::<_, Value>("manual_login", params)
            .await
            .map(|_| ())
    }

    async fn login_account(&self, params: LoginAccountParams) -> Result<()> {
        self.request::<_, Value>("login_account", params)
            .await
            .map(|_| ())
    }

    async fn close_account(&self, params: CloseAccountParams) -> Result<()> {
        self.request::<_, Value>("close_account", params)
            .await
            .map(|_| ())
    }

    async fn shutdown(&self) -> Result<()> {
        if self.process.lock().await.is_none() {
            return Ok(());
        }

        let result = self
            .request_raw::<_, Value>("shutdown", serde_json::json!({}))
            .await
            .map(|_| ());
        *self.initialized.lock().await = false;
        result
    }
}

pub fn helper_dir_from(crate_dir: &str) -> PathBuf {
    PathBuf::from(crate_dir)
        .join("resources")
        .join("playwright-bridge")
}

fn normalize_spawn_path(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        const VERBATIM_PREFIX: &str = r"\\?\";
        const UNC_PREFIX: &str = r"\\?\UNC\";
        let value = path.to_string_lossy();
        if let Some(stripped) = value.strip_prefix(UNC_PREFIX) {
            return PathBuf::from(format!(r"\\{stripped}"));
        }
        if let Some(stripped) = value.strip_prefix(VERBATIM_PREFIX) {
            return PathBuf::from(stripped);
        }
    }

    path
}

fn bridge_log_path(provider: &str) -> Result<PathBuf> {
    let dir = std::env::temp_dir().join("rustproxyhub-playwright");
    fs::create_dir_all(&dir)?;
    Ok(dir.join(format!("{provider}-bridge.log")))
}

fn reset_bridge_log(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, b"")?;
    Ok(())
}

fn append_bridge_log(path: &Path, line: String) {
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        writeln!(file, "{line}")?;
        Ok(())
    })();

    if let Err(err) = result {
        eprintln!(
            "[browser-bridge] failed to write log {}: {err}",
            path.display()
        );
    }
}
