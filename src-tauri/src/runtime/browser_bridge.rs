use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    fs::{self, DirBuilder, OpenOptions},
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
    runtime::Handle,
    sync::{mpsc, oneshot, Mutex},
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
    event: Option<String>,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BridgeStreamEvent {
    pub event: String,
    pub payload: Value,
}

pub struct BridgeStream {
    events: mpsc::UnboundedReceiver<Result<BridgeStreamEvent>>,
    completion: oneshot::Receiver<Result<Value>>,
    pending: Arc<Mutex<HashMap<u64, PendingRpc>>>,
    request_id: u64,
    timeout: Duration,
}

impl BridgeStream {
    pub async fn next_event(&mut self) -> Option<Result<BridgeStreamEvent>> {
        match tokio::time::timeout(self.timeout, self.events.recv()).await {
            Ok(event) => event,
            Err(_) => {
                if let Ok(mut pending) = self.pending.try_lock() {
                    pending.remove(&self.request_id);
                }
                Some(Err(anyhow!(
                    "helper stream produced no event for {}ms",
                    self.timeout.as_millis()
                )))
            }
        }
    }

    pub async fn finish<R: DeserializeOwned>(self) -> Result<R> {
        let BridgeStream {
            completion,
            pending,
            request_id,
            timeout,
            ..
        } = self;
        match tokio::time::timeout(timeout, completion).await {
            Ok(Ok(result)) => Ok(serde_json::from_value(result?)?),
            Ok(Err(err)) => Err(anyhow!("helper stream response channel closed").context(err)),
            Err(_) => {
                pending.lock().await.remove(&request_id);
                Err(anyhow!(
                    "helper stream timed out after {}ms",
                    timeout.as_millis()
                ))
            }
        }
    }
}

struct PendingRpc {
    completion: oneshot::Sender<Result<Value>>,
    stream: Option<mpsc::UnboundedSender<Result<BridgeStreamEvent>>>,
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
    pending: Arc<Mutex<HashMap<u64, PendingRpc>>>,
    next_id: Arc<AtomicU64>,
    init_params: Arc<Mutex<Option<InitParams>>>,
    initialized: Arc<Mutex<bool>>,
    request_timeout: Duration,
}

struct BridgeProcess {
    pid: u32,
    stdin: Arc<Mutex<ChildStdin>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
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
        // Keep the selected executable path intact. On Unix, Node version-manager
        // shims commonly expose `node` as a symlink to their launcher; resolving
        // it changes argv[0] to `mise`/`nvm` and starts the launcher as a command
        // instead of Node.
        let node = node_path
            .map(node_spawn_path)
            .unwrap_or_else(|| PathBuf::from("node"));
        let log_path = Arc::new(bridge_log_path(&provider)?);
        reset_bridge_log(log_path.as_ref())?;
        let request_timeout = Duration::from_millis(
            std::env::var("RUST_PROXY_HUB_BRIDGE_TIMEOUT_MS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(300_000),
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

    pub async fn request_stream<T: Serialize>(
        &self,
        method: &str,
        params: T,
    ) -> Result<BridgeStream> {
        let (stream_sender, events) = mpsc::unbounded_channel();
        let (request_id, completion) = self
            .send_request(method, params, Some(stream_sender))
            .await?;
        Ok(BridgeStream {
            events,
            completion,
            pending: Arc::clone(&self.pending),
            request_id,
            timeout: self.request_timeout,
        })
    }

    async fn request_raw<T: Serialize, R: DeserializeOwned>(
        &self,
        method: &str,
        params: T,
    ) -> Result<R> {
        let (request_id, receiver) = self.send_request(method, params, None).await?;
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
                self.pending.lock().await.remove(&request_id);
                return Err(anyhow!(
                    "helper request timed out after {}ms: {method}; inspect {}",
                    self.request_timeout.as_millis(),
                    self.log_path.display()
                ));
            }
        };
        Ok(serde_json::from_value(value)?)
    }

    async fn send_request<T: Serialize>(
        &self,
        method: &str,
        params: T,
        stream: Option<mpsc::UnboundedSender<Result<BridgeStreamEvent>>>,
    ) -> Result<(u64, oneshot::Receiver<Result<Value>>)> {
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
        self.pending.lock().await.insert(
            id,
            PendingRpc {
                completion: sender,
                stream,
            },
        );

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

        Ok((id, receiver))
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

        let mut command = Command::new(self.node_path.as_ref());
        command
            // Use relative entrypoint once cwd is set to avoid Windows drive-letter parsing edge cases.
            .arg("index.mjs")
            .current_dir(self.helper_dir.as_ref())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        let mut child = command
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
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        self.spawn_stdout_reader(stdout);
        self.spawn_stderr_reader(stderr);
        self.spawn_exit_waiter(child, child_id, shutdown_rx);

        *guard = Some(BridgeProcess {
            pid: child_id,
            stdin: Arc::clone(&stdin),
            shutdown_tx: Some(shutdown_tx),
        });
        Ok(stdin)
    }

    async fn signal_process_shutdown(&self) -> bool {
        let shutdown_tx = {
            let mut guard = self.process.lock().await;
            guard
                .as_mut()
                .and_then(|process| process.shutdown_tx.take())
        };
        if let Some(tx) = shutdown_tx {
            let _ = tx.send(());
            true
        } else {
            false
        }
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
                                if let Some(event) = response.event {
                                    let payload = response.result.unwrap_or(Value::Null);
                                    let stream = pending_reader
                                        .lock()
                                        .await
                                        .get(&response.id)
                                        .and_then(|pending| pending.stream.as_ref().cloned());
                                    append_bridge_log(
                                        log_for_stdout.as_ref(),
                                        format!("stdout stream id={} event={event}", response.id),
                                    );
                                    if let Some(sender) = stream {
                                        let _ =
                                            sender.send(Ok(BridgeStreamEvent { event, payload }));
                                    }
                                    continue;
                                }
                                append_bridge_log(
                                    log_for_stdout.as_ref(),
                                    format!(
                                        "stdout rpc id={} error={}",
                                        response.id,
                                        response.error.is_some()
                                    ),
                                );
                                let pending = pending_reader.lock().await.remove(&response.id);
                                if let Some(pending) = pending {
                                    if let Some(stream) = pending.stream {
                                        if let Some(error) = response.error.as_ref() {
                                            let _ = stream.send(Err(anyhow!(error.clone())));
                                        }
                                    }
                                    let _ = pending.completion.send(match response.error {
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

    fn spawn_exit_waiter(
        &self,
        mut child: tokio::process::Child,
        child_id: u32,
        mut shutdown_rx: oneshot::Receiver<()>,
    ) {
        let log_for_exit = Arc::clone(&self.log_path);
        let pending_for_exit = Arc::clone(&self.pending);
        let process_for_exit = Arc::clone(&self.process);
        let initialized_for_exit = Arc::clone(&self.initialized);

        tokio::spawn(async move {
            let expected_shutdown = tokio::select! {
                result = child.wait() => {
                    match result {
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
                    false
                }
                _ = &mut shutdown_rx => {
                    append_bridge_log(
                        log_for_exit.as_ref(),
                        format!("shutdown signal pid={child_id}"),
                    );
                    if let Err(err) = child.kill().await {
                        append_bridge_log(
                            log_for_exit.as_ref(),
                            format!("process kill error pid={child_id}: {err}"),
                        );
                    }
                    match child.wait().await {
                        Ok(status) => {
                            append_bridge_log(
                                log_for_exit.as_ref(),
                                format!("process exit after shutdown pid={child_id} status={status}"),
                            );
                        }
                        Err(err) => {
                            append_bridge_log(
                                log_for_exit.as_ref(),
                                format!("process wait after shutdown error pid={child_id}: {err}"),
                            );
                        }
                    }
                    true
                }
            };

            let mut process = process_for_exit.lock().await;
            if process.as_ref().map(|item| item.pid) == Some(child_id) {
                *process = None;
                *initialized_for_exit.lock().await = false;
            }
            drop(process);

            let mut pending = pending_for_exit.lock().await;
            let error_message = if expected_shutdown {
                format!(
                    "helper process shut down; inspect {}",
                    log_for_exit.display()
                )
            } else {
                format!(
                    "helper process exited unexpectedly; inspect {}",
                    log_for_exit.display()
                )
            };
            for (_, pending) in pending.drain() {
                if let Some(stream) = pending.stream {
                    let _ = stream.send(Err(anyhow!(error_message.clone())));
                }
                let _ = pending.completion.send(Err(anyhow!(error_message.clone())));
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

        let result = tokio::time::timeout(
            Duration::from_secs(10),
            self.request_raw::<_, Value>("shutdown", serde_json::json!({})),
        )
        .await;
        *self.initialized.lock().await = false;
        match result {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(err)) => {
                self.signal_process_shutdown().await;
                Err(err)
            }
            Err(_) => {
                self.signal_process_shutdown().await;
                Err(anyhow!(
                    "helper shutdown timed out; inspect {}",
                    self.log_path.display()
                ))
            }
        }
    }
}

impl Drop for PlaywrightBridge {
    fn drop(&mut self) {
        if Arc::strong_count(&self.process) != 1 {
            return;
        }

        if let Ok(mut process) = self.process.try_lock() {
            let shutdown_tx = process
                .as_mut()
                .and_then(|bridge_process| bridge_process.shutdown_tx.take());
            drop(process);
            if let Some(tx) = shutdown_tx {
                append_bridge_log(
                    self.log_path.as_ref(),
                    "drop signaled helper shutdown".to_owned(),
                );
                let _ = tx.send(());
            }
            if let Ok(mut initialized) = self.initialized.try_lock() {
                *initialized = false;
            }
            return;
        }

        if let Ok(handle) = Handle::try_current() {
            let process = Arc::clone(&self.process);
            let initialized = Arc::clone(&self.initialized);
            let log_path = Arc::clone(&self.log_path);
            handle.spawn(async move {
                let shutdown_tx = {
                    let mut guard = process.lock().await;
                    guard
                        .as_mut()
                        .and_then(|bridge_process| bridge_process.shutdown_tx.take())
                };
                if let Some(tx) = shutdown_tx {
                    append_bridge_log(
                        log_path.as_ref(),
                        "drop scheduled helper shutdown".to_owned(),
                    );
                    let _ = tx.send(());
                }
                *initialized.lock().await = false;
            });
        }
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

fn node_spawn_path(path: PathBuf) -> PathBuf {
    normalize_spawn_path(path)
}

fn bridge_log_path(provider: &str) -> Result<PathBuf> {
    // ponytail: logs live in the OS temp dir for portability. Secured by:
    //   - dir created 0700 (owner-only)
    //   - refuse existing symlinks (symlink-attack guard)
    //   - log file opened 0600
    //   - secret-bearing stderr lines scrubbed before write
    // Upgrade path: move under app_data_dir when a path is threaded through.
    let path = provider_bridge_log_path(provider);
    let dir = path
        .parent()
        .ok_or_else(|| anyhow!("bridge log path has no parent"))?;
    secure_create_dir(dir)?;
    Ok(path)
}

pub fn provider_bridge_log_path(provider: &str) -> PathBuf {
    std::env::temp_dir()
        .join("rustproxyhub-playwright")
        .join(format!("{provider}-bridge.log"))
}

fn secure_create_dir(dir: &Path) -> Result<()> {
    if dir.exists() {
        let meta = fs::symlink_metadata(dir)
            .with_context(|| format!("failed to stat {}", dir.display()))?;
        if meta.file_type().is_symlink() {
            return Err(anyhow!(
                "refusing to use log dir {}: symlink detected (possible attack)",
                dir.display()
            ));
        }
        // Already exists (possibly created by another provider's bridge); trust it.
        return Ok(());
    }
    let mut builder = DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(dir)
        .with_context(|| format!("failed to create log dir {}", dir.display()))?;
    Ok(())
}

fn reset_bridge_log(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        secure_create_dir(parent)?;
    }
    // Refuse to truncate a symlink (avoids clobbering a victim file via symlink).
    if path.exists() {
        let meta = fs::symlink_metadata(path)
            .with_context(|| format!("failed to stat {}", path.display()))?;
        if meta.file_type().is_symlink() {
            return Err(anyhow!(
                "refusing to reset log {}: symlink detected (possible attack)",
                path.display()
            ));
        }
    }
    let mut file = open_secure_log(path)?;
    file.write_all(b"")?;
    Ok(())
}

fn open_secure_log(path: &Path) -> Result<std::fs::File> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .with_context(|| format!("failed to open log {}", path.display()))
}

/// Strip lines that look like they carry capture/cookie/auth material, so a chatty
/// Playwright helper can't leak session secrets to a world-readable log.
fn scrub_secret_line(line: &str) -> String {
    const SENSITIVE: &[&str] = &[
        "cookie",
        "set-cookie",
        "authorization",
        "bx-v",
        "bx-umidtoken",
        "bx-ua",
        "x-api-key",
    ];
    let lower = line.to_ascii_lowercase();
    if SENSITIVE.iter().any(|needle| lower.contains(needle)) {
        "[scrubbed: line matched secret pattern]".to_owned()
    } else {
        line.to_owned()
    }
}

fn append_bridge_log(path: &Path, line: String) {
    let result = (|| -> Result<()> {
        let mut file = open_secure_log(path)?;
        writeln!(file, "{}", scrub_secret_line(&line))?;
        Ok(())
    })();

    if let Err(err) = result {
        eprintln!(
            "[browser-bridge] failed to write log {}: {err}",
            path.display()
        );
    }
}
#[cfg(test)]
mod tests {
    use super::node_spawn_path;
    #[cfg(not(unix))]
    use std::path::PathBuf;
    #[cfg(unix)]
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[cfg(unix)]
    #[test]
    fn node_spawn_path_preserves_version_manager_shim() {
        use std::os::unix::fs::symlink;

        let dir = std::env::temp_dir().join(format!(
            "rust-proxy-hub-node-shim-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock after epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&dir).expect("create temp directory");
        let launcher = dir.join("mise");
        let node_shim = dir.join("node");
        fs::write(&launcher, "launcher").expect("write launcher");
        symlink(&launcher, &node_shim).expect("create node shim");

        assert_eq!(node_spawn_path(node_shim.clone()), node_shim);
        assert_ne!(node_shim.canonicalize().expect("resolve shim"), node_shim);

        fs::remove_dir_all(&dir).expect("remove temp directory");
    }

    #[cfg(not(unix))]
    #[test]
    fn node_spawn_path_preserves_selected_path() {
        let node = PathBuf::from("node.exe");
        assert_eq!(node_spawn_path(node.clone()), node);
    }
}
