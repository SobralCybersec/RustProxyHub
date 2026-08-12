impl ControlState {
    fn new(app: &AppHandle) -> Result<Self> {
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .context("failed to resolve workspace root")?
            .to_path_buf();

        let app_data_dir = app
            .path()
            .app_data_dir()
            .context("failed to resolve app data directory")?;
        fs::create_dir_all(&app_data_dir)?;

        let runtime = build_runtime_diagnostics(
            app.path().resource_dir().ok().as_deref(),
            &workspace_root,
            &app_data_dir,
            detect_browser_available(),
        );
        let startup_config = startup_config_from_env();
        let hub_api_key = std::env::var("RUST_PROXY_HUB_API_KEY")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let qwen_runtime_dir = app_data_dir.join("providers").join("qwen");

        let mut statuses = HashMap::new();
        let mut logs = HashMap::new();
        for name in service_names() {
            statuses.insert(name.to_owned(), ServiceRuntimeStatus::default());
            logs.insert(name.to_owned(), VecDeque::new());
        }

        Ok(Self {
            workspace_root,
            app_data_dir,
            qwen_runtime_dir,
            runtime,
            startup_config,
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .tcp_nodelay(true)
                .pool_idle_timeout(Duration::from_secs(90))
                .pool_max_idle_per_host(16)
                .timeout(Duration::from_secs(12))
                .build()?,
            hub_api_key,
            statuses: Arc::new(Mutex::new(statuses)),
            logs: Arc::new(Mutex::new(logs)),
            open_provider_login_sessions: Arc::new(Mutex::new(HashSet::new())),
            open_qwen_account_login_sessions: Arc::new(Mutex::new(HashSet::new())),
            tasks: Arc::new(std::sync::Mutex::new(HashMap::new())),
            app_handle: Some(app.clone()),
            dashboard_notify: Arc::new(Notify::new()),
        })
    }

    fn emit_dashboard_update(&self) {
        // Poke the coalescer; it will debounce bursts into one build+emit.
        self.dashboard_notify.notify_one();
    }

    fn start_dashboard_coalescer(&self) {
        let state = self.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                state.dashboard_notify.notified().await;
                // ponytail: 250ms coalesce window; tune if UI feels laggy
                tokio::time::sleep(Duration::from_millis(250)).await;
                // Any notify_one() calls that arrived during the sleep are
                // already collapsed into one permit — we consume them by doing
                // the build now; the loop will block on the next notified().
                if let (Some(handle), Ok(overview)) =
                    (&state.app_handle, state.build_dashboard_overview().await)
                {
                    let _ = handle.emit("dashboard:update", &overview);
                }
            }
        });
    }

    async fn bootstrap(&self) -> Result<()> {
        if !self.runtime.single_runner_ready {
            self.mark_runtime_blocked().await;
            return Ok(());
        }

        fs::create_dir_all(self.providers_root())?;
        fs::create_dir_all(self.qwen_runtime_dir())?;
        let _ = ensure_qwen_db(&self.qwen_runtime_dir(), &self.workspace_root)?;
        let selected = self.selected_startup_services();

        for name in &selected {
            self.ensure_service_dir(name);
            self.spawn_service_by_name(name)
                .map_err(|err| anyhow!("failed to start {name}: {err}"))?;
        }

        Ok(())
    }

    fn selected_startup_services(&self) -> Vec<&'static str> {
        let configured = self
            .startup_config
            .services
            .iter()
            .filter_map(|name| normalize_service_name(name).ok())
            .collect::<HashSet<_>>();

        service_names()
            .into_iter()
            .filter(|name| configured.contains(name))
            .collect()
    }

    fn ensure_service_dir(&self, name: &str) {
        let _ = match name {
            "qwen" => fs::create_dir_all(self.qwen_runtime_dir()),
            "deepseek" => fs::create_dir_all(self.deepseek_runtime_dir()),
            "kimi" => fs::create_dir_all(self.kimi_runtime_dir()),
            "chatgpt" => fs::create_dir_all(self.chatgpt_runtime_dir()),
            "gemini" => fs::create_dir_all(self.gemini_runtime_dir()),
            "mistral" => fs::create_dir_all(self.mistral_runtime_dir()),
            "zai" => fs::create_dir_all(self.zai_runtime_dir()),
            "meta" => fs::create_dir_all(self.meta_runtime_dir()),
            "hub" => fs::create_dir_all(self.hub_runtime_dir()),
            _ => Ok(()),
        };
    }

    fn spawn_service_by_name(&self, name: &str) -> Result<()> {
        let helper_dir = require_helper_dir(&self.runtime)?;
        let node_path = Some(require_node_path(&self.runtime)?);
        // Gate every embedded provider behind the same key the hub uses, so a local
        // process can't bypass the hub by hitting 127.0.0.1:3000-3006 directly.
        let provider_api_key = self.hub_api_key.clone();

        match name {
            "qwen" => {
                let runtime_dir = self.qwen_runtime_dir();
                let helper_dir = helper_dir.clone();
                let node_path = node_path.clone();
                self.spawn_service("qwen".to_owned(), async move {
                    serve_qwen(
                        build_embedded_config(
                            runtime_dir,
                            QWEN_PORT,
                            provider_api_key,
                            DEFAULT_BROWSER.to_owned(),
                            true,
                        ),
                        helper_dir,
                        node_path,
                    )
                    .await
                });
            }
            "deepseek" => {
                let runtime_dir = self.deepseek_runtime_dir();
                let helper_dir = helper_dir.clone();
                let node_path = node_path.clone();
                let api_key = provider_api_key.clone();
                self.spawn_service("deepseek".to_owned(), async move {
                    serve_deepseek(DeepseekServiceConfig {
                        host: "127.0.0.1".to_owned(),
                        port: DEEPSEEK_PORT,
                        api_key,
                        headless: true,
                        browser: DEFAULT_BROWSER.to_owned(),
                        runtime_dir,
                        helper_dir,
                        node_path,
                    })
                    .await
                });
            }
            "kimi" => {
                let runtime_dir = self.kimi_runtime_dir();
                let helper_dir = helper_dir.clone();
                let node_path = node_path.clone();
                let api_key = provider_api_key.clone();
                self.spawn_service("kimi".to_owned(), async move {
                    serve_kimi(KimiServiceConfig {
                        host: "127.0.0.1".to_owned(),
                        port: KIMI_PORT,
                        api_key,
                        headless: true,
                        browser: DEFAULT_BROWSER.to_owned(),
                        runtime_dir,
                        helper_dir,
                        node_path,
                    })
                    .await
                });
            }
            "chatgpt" => {
                let runtime_dir = self.chatgpt_runtime_dir();
                let helper_dir = helper_dir.clone();
                let node_path = node_path.clone();
                let api_key = provider_api_key.clone();
                self.spawn_service("chatgpt".to_owned(), async move {
                    serve_browser_provider(BrowserProviderServerConfig {
                        kind: BrowserProviderKind::Chatgpt,
                        host: "127.0.0.1".to_owned(),
                        port: CHATGPT_PORT,
                        api_key,
                        headless: true,
                        browser: DEFAULT_BROWSER.to_owned(),
                        runtime_dir,
                        helper_dir,
                        node_path,
                    })
                    .await
                });
            }
            "gemini" => {
                let runtime_dir = self.gemini_runtime_dir();
                let helper_dir = helper_dir.clone();
                let node_path = node_path.clone();
                let api_key = provider_api_key.clone();
                self.spawn_service("gemini".to_owned(), async move {
                    serve_browser_provider(BrowserProviderServerConfig {
                        kind: BrowserProviderKind::Gemini,
                        host: "127.0.0.1".to_owned(),
                        port: GEMINI_PORT,
                        api_key,
                        headless: true,
                        browser: DEFAULT_BROWSER.to_owned(),
                        runtime_dir,
                        helper_dir,
                        node_path,
                    })
                    .await
                });
            }
            "mistral" => {
                let runtime_dir = self.mistral_runtime_dir();
                let helper_dir = helper_dir.clone();
                let node_path = node_path.clone();
                let api_key = provider_api_key.clone();
                self.spawn_service("mistral".to_owned(), async move {
                    serve_browser_provider(BrowserProviderServerConfig {
                        kind: BrowserProviderKind::Mistral,
                        host: "127.0.0.1".to_owned(),
                        port: MISTRAL_PORT,
                        api_key,
                        headless: true,
                        browser: DEFAULT_BROWSER.to_owned(),
                        runtime_dir,
                        helper_dir,
                        node_path,
                    })
                    .await
                });
            }
            "zai" => {
                let runtime_dir = self.zai_runtime_dir();
                let helper_dir = helper_dir.clone();
                let node_path = node_path.clone();
                let api_key = provider_api_key.clone();
                self.spawn_service("zai".to_owned(), async move {
                    serve_browser_provider(BrowserProviderServerConfig {
                        kind: BrowserProviderKind::Zai,
                        host: "127.0.0.1".to_owned(),
                        port: ZAI_PORT,
                        api_key,
                        headless: true,
                        browser: DEFAULT_BROWSER.to_owned(),
                        runtime_dir,
                        helper_dir,
                        node_path,
                    })
                    .await
                });
            }
            "meta" => {
                let runtime_dir = self.meta_runtime_dir();
                let helper_dir = helper_dir.clone();
                let node_path = node_path.clone();
                let api_key = provider_api_key.clone();
                self.spawn_service("meta".to_owned(), async move {
                    serve_browser_provider(BrowserProviderServerConfig {
                        kind: BrowserProviderKind::Meta,
                        host: "127.0.0.1".to_owned(),
                        port: META_PORT,
                        api_key,
                        headless: true,
                        browser: DEFAULT_BROWSER.to_owned(),
                        runtime_dir,
                        helper_dir,
                        node_path,
                    })
                    .await
                });
            }
            "hub" => {
                let hub_api_key = self.hub_api_key.clone();
                self.spawn_service("hub".to_owned(), async move {
                    serve_hub(HubServiceConfig {
                        host: "127.0.0.1".to_owned(),
                        port: HUB_PORT,
                        api_key: hub_api_key.clone(),
                        qwen: ProviderConfig::new(provider_base_url("qwen"), hub_api_key.clone()),
                        deepseek: ProviderConfig::new(
                            provider_base_url("deepseek"),
                            hub_api_key.clone(),
                        ),
                        kimi: ProviderConfig::new(provider_base_url("kimi"), hub_api_key.clone()),
                        chatgpt: ProviderConfig::new(
                            provider_base_url("chatgpt"),
                            hub_api_key.clone(),
                        ),
                        gemini: ProviderConfig::new(
                            provider_base_url("gemini"),
                            hub_api_key.clone(),
                        ),
                        mistral: ProviderConfig::new(
                            provider_base_url("mistral"),
                            hub_api_key.clone(),
                        ),
                        zai: ProviderConfig::new(provider_base_url("zai"), hub_api_key.clone()),
                        meta: ProviderConfig::new(provider_base_url("meta"), hub_api_key),
                    })
                    .await
                });
            }
            _ => return Err(anyhow!("unsupported service: {name}")),
        }

        Ok(())
    }

    async fn start_services(&self, requested: Vec<String>) -> Result<Vec<String>> {
        if !self.runtime.single_runner_ready {
            return Err(anyhow!("runtime preflight is blocking startup"));
        }

        let mut started = Vec::new();
        for raw in requested {
            let name = normalize_service_name(&raw)?;
            self.ensure_service_dir(name);
            self.spawn_service_by_name(name)
                .map_err(|err| anyhow!("failed to start {name}: {err}"))?;
            started.push(name.to_owned());
        }

        Ok(started)
    }

    async fn mark_runtime_blocked(&self) {
        let summary = self.runtime.issues.join(" | ");
        {
            let mut guard = self.statuses.lock().await;
            for name in service_names() {
                let entry = guard.entry(name.to_owned()).or_default();
                entry.running = false;
                entry.last_error = Some(summary.clone());
            }
        }

        for issue in &self.runtime.issues {
            self.push_log("hub", format!("runtime preflight blocked startup: {issue}"))
                .await;
        }
    }

    fn spawn_service<F>(&self, name: String, future: F)
    where
        F: Future<Output = Result<()>> + Send + 'static,
    {
        let state = self.clone();
        let service_name = name;
        let service_name_for_task = service_name.clone();

        let handle = tauri::async_runtime::spawn(async move {
            state
                .mark_service_started(&service_name, format!("starting embedded {service_name}"))
                .await;

            match future.await {
                Ok(()) => {
                    state
                        .mark_service_stopped(
                            &service_name,
                            Some(format!("{service_name} server exited")),
                            None,
                        )
                        .await;
                }
                Err(err) => {
                    state
                        .mark_service_stopped(
                            &service_name,
                            Some(format!("{service_name} server failed: {err}")),
                            Some(err.to_string()),
                        )
                        .await;
                }
            }
        });

        // ponytail: one lock hold so check+insert are atomic; abort duplicate to keep live handle tracked
        let mut guard = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        use std::collections::hash_map::Entry;
        match guard.entry(service_name_for_task) {
            Entry::Occupied(_) => handle.abort(),
            Entry::Vacant(e) => {
                e.insert(handle);
            }
        }
    }

    async fn mark_service_started(&self, name: &str, message: String) {
        {
            let mut guard = self.statuses.lock().await;
            let entry = guard.entry(name.to_owned()).or_default();
            entry.running = true;
            entry.started_at = Some(current_timestamp());
            entry.last_error = None;
        }
        self.push_log(name, message).await;
        self.emit_dashboard_update();
    }

    async fn mark_service_stopped(
        &self,
        name: &str,
        log_message: Option<String>,
        last_error: Option<String>,
    ) {
        {
            let mut guard = self.statuses.lock().await;
            let entry = guard.entry(name.to_owned()).or_default();
            entry.running = false;
            if last_error.is_some() {
                entry.last_error = last_error;
            }
        }
        if let Some(message) = log_message {
            self.push_log(name, message).await;
        }
        self.emit_dashboard_update();
    }

    async fn push_log(&self, name: &str, message: impl Into<String>) {
        let mut guard = self.logs.lock().await;
        let entry = guard.entry(name.to_owned()).or_default();
        entry.push_back(format!("[{}] {}", current_timestamp(), message.into()));
        while entry.len() > LOG_LIMIT {
            entry.pop_front();
        }
    }

    async fn service_status(&self, name: &str) -> ServiceRuntimeStatus {
        self.statuses
            .lock()
            .await
            .get(name)
            .cloned()
            .unwrap_or_default()
    }

    async fn ensure_provider_ready(&self, provider: &str) -> Result<()> {
        if !self.runtime.single_runner_ready {
            return Err(anyhow!(
                "runtime preflight is blocking {provider} login: {}",
                self.runtime.issues.join(" | ")
            ));
        }

        if !self.service_status(provider).await.running {
            if let Some(stale) = self
                .tasks
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .remove(provider)
            {
                stale.abort();
            }
            {
                let mut statuses = self.statuses.lock().await;
                statuses.entry(provider.to_owned()).or_default().last_error = None;
            }
            self.ensure_service_dir(provider);
            self.spawn_service_by_name(provider)
                .map_err(|err| anyhow!("failed to start {provider} for manual login: {err}"))?;
        }

        self.wait_for_provider_ready_at(
            provider,
            &provider_base_url(provider),
            PROVIDER_READY_TIMEOUT,
        )
        .await
    }

    async fn wait_for_provider_ready_at(
        &self,
        provider: &str,
        base_url: &str,
        timeout: Duration,
    ) -> Result<()> {
        let started = std::time::Instant::now();
        let health_url = format!("{base_url}/health");
        loop {
            if self.client.get(&health_url).send().await.is_ok() {
                return Ok(());
            }

            let status = self.service_status(provider).await;
            if !status.running {
                if let Some(error) = status.last_error {
                    return Err(anyhow!(
                        "{provider} failed to start for manual login: {error}"
                    ));
                }
            }
            if started.elapsed() >= timeout {
                return Err(anyhow!(
                    "{provider} did not become ready for manual login within {} seconds",
                    timeout.as_secs()
                ));
            }
            tokio::time::sleep(PROVIDER_READY_POLL_INTERVAL).await;
        }
    }

    async fn service_logs(&self, name: &str) -> Vec<String> {
        let mut entries: Vec<String> = self
            .logs
            .lock()
            .await
            .get(name)
            .map(|items| items.iter().cloned().collect())
            .unwrap_or_default();
        if matches!(name, "chatgpt" | "gemini" | "mistral" | "zai" | "meta") {
            if let Ok(contents) = fs::read_to_string(provider_bridge_log_path(name)) {
                entries.extend(
                    contents
                        .lines()
                        .rev()
                        .take(LOG_LIMIT)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .map(|line| format!("[bridge] {line}")),
                );
            }
        }
        entries
    }

    fn hub_config(&self) -> HubConfigResponse {
        HubConfigResponse {
            port: HUB_PORT,
            base_url: hub_base_url(),
            openapi_url: format!("{}/openapi.json", hub_base_url()),
            api_key_enabled: self.hub_api_key.is_some(),
        }
    }

    fn providers_root(&self) -> PathBuf {
        self.app_data_dir.join("providers")
    }

    fn qwen_runtime_dir(&self) -> PathBuf {
        self.qwen_runtime_dir.clone()
    }

    fn deepseek_runtime_dir(&self) -> PathBuf {
        self.providers_root().join("deepseek")
    }

    fn kimi_runtime_dir(&self) -> PathBuf {
        self.providers_root().join("kimi")
    }

    fn chatgpt_runtime_dir(&self) -> PathBuf {
        self.providers_root().join("chatgpt")
    }

    fn gemini_runtime_dir(&self) -> PathBuf {
        self.providers_root().join("gemini")
    }

    fn mistral_runtime_dir(&self) -> PathBuf {
        self.providers_root().join("mistral")
    }

    fn zai_runtime_dir(&self) -> PathBuf {
        self.providers_root().join("zai")
    }

    fn meta_runtime_dir(&self) -> PathBuf {
        self.providers_root().join("meta")
    }

    fn hub_runtime_dir(&self) -> PathBuf {
        self.providers_root().join("hub")
    }

    async fn build_dashboard_overview(&self) -> Result<DashboardOverview> {
        let providers_future = join_all(
            provider_names()
                .into_iter()
                .map(|provider| self.provider_overview(provider, None)),
        );
        let hub_future = self.hub_overview();
        let (providers, hub) = tokio::join!(providers_future, hub_future);

        Ok(DashboardOverview {
            generated_at: current_timestamp(),
            runtime: self.runtime.clone(),
            startup_config: self.startup_config.clone(),
            hub,
            providers,
            qwen_account_count: list_qwen_accounts_db(
                &self.qwen_runtime_dir(),
                &self.workspace_root,
            )?
            .len(),
            open_provider_login_sessions: self
                .open_provider_login_sessions
                .lock()
                .await
                .iter()
                .cloned()
                .collect(),
            open_qwen_account_login_sessions: self
                .open_qwen_account_login_sessions
                .lock()
                .await
                .iter()
                .cloned()
                .collect(),
        })
    }

    async fn call_json_get(
        &self,
        base_url: &str,
        path: &str,
        api_key: Option<&str>,
    ) -> Result<(u16, Value)> {
        let url = format!("{base_url}{path}");
        let mut request = self.client.get(url);
        if let Some(api_key) = api_key.filter(|value| !value.trim().is_empty()) {
            request = request.header(AUTHORIZATION, format!("Bearer {api_key}"));
        }
        let response = request.send().await?;
        let status = response.status().as_u16();
        let value = response.json::<Value>().await.unwrap_or_else(|_| json!({}));
        Ok((status, value))
    }

    async fn call_json_post(
        &self,
        base_url: &str,
        path: &str,
        api_key: Option<&str>,
        payload: &Value,
    ) -> Result<(u16, Value)> {
        self.call_json_post_with_timeout(base_url, path, api_key, payload, None)
            .await
    }

    async fn call_json_post_with_timeout(
        &self,
        base_url: &str,
        path: &str,
        api_key: Option<&str>,
        payload: &Value,
        timeout: Option<Duration>,
    ) -> Result<(u16, Value)> {
        let url = format!("{base_url}{path}");
        let mut request = self
            .client
            .post(url)
            .header(CONTENT_TYPE, "application/json")
            .json(payload);
        if let Some(api_key) = api_key.filter(|value| !value.trim().is_empty()) {
            request = request.header(AUTHORIZATION, format!("Bearer {api_key}"));
        }
        if let Some(timeout) = timeout {
            request = request.timeout(timeout);
        }
        let response = request.send().await?;
        let status = response.status().as_u16();
        let value = response.json::<Value>().await.unwrap_or_else(|_| json!({}));
        Ok((status, value))
    }

    async fn hub_overview(&self) -> HubOverview {
        let status = self.service_status("hub").await;
        if !status.running {
            return HubOverview {
                running: status.running,
                started_at: status.started_at,
                health_status: "offline".to_owned(),
                model_count: 0,
                provider_statuses: Vec::new(),
                detail: None,
                config: self.hub_config(),
            };
        }

        let base_url = hub_base_url();
        let (health, models) = tokio::join!(
            self.call_json_get(&base_url, "/health", self.hub_api_key.as_deref()),
            self.call_json_get(&base_url, "/v1/models", self.hub_api_key.as_deref()),
        );
        let health = health.ok();
        let models = models.ok();

        let detail = health.as_ref().map(|(_, payload)| payload.clone());
        let health_status = health
            .as_ref()
            .and_then(|(_, payload)| payload.get("status"))
            .and_then(Value::as_str)
            .unwrap_or(if status.running {
                "starting"
            } else {
                "offline"
            })
            .to_owned();
        let provider_statuses = detail
            .as_ref()
            .and_then(|payload| payload.get("providers"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let model_count = models
            .as_ref()
            .and_then(|(_, payload)| payload.get("data"))
            .and_then(Value::as_array)
            .map(|items| items.len())
            .unwrap_or_default();

        HubOverview {
            running: status.running,
            started_at: status.started_at,
            health_status,
            model_count,
            provider_statuses,
            detail,
            config: self.hub_config(),
        }
    }

    async fn provider_overview(
        &self,
        name: &'static str,
        chatgpt_mode: Option<&str>,
    ) -> ProviderOverview {
        let status = self.service_status(name).await;
        let base_url = provider_base_url(name);
        let open_provider_sessions = self.open_provider_login_sessions.lock().await.clone();
        let open_qwen_account_sessions = self.open_qwen_account_login_sessions.lock().await.clone();
        let login_open = open_provider_sessions.contains(name);
        if !status.running {
            return ProviderOverview {
                name: name.to_owned(),
                running: false,
                started_at: status.started_at,
                base_url,
                health_status: "offline".to_owned(),
                login_state: "offline".to_owned(),
                model_count: 0,
                models: Vec::new(),
                model_modes: HashMap::new(),
                web_search_supported: provider_web_search_supported(name),
                last_error: status.last_error,
            };
        }

        let (health, models) = if login_open {
            (
                self.call_json_get(&base_url, "/health", self.hub_api_key.as_deref())
                    .await
                    .ok(),
                None,
            )
        } else {
            let models_path = if name == "chatgpt" {
                if chatgpt_mode == Some("codex") {
                    "/v1/models?chatgpt_mode=codex"
                } else if chatgpt_mode == Some("web") {
                    "/v1/models?chatgpt_mode=web"
                } else {
                    "/v1/models?chatgpt_mode=auto"
                }
            } else {
                "/v1/models"
            };
            let (health, models) = tokio::join!(
                self.call_json_get(&base_url, "/health", self.hub_api_key.as_deref()),
                self.call_json_get(&base_url, models_path, self.hub_api_key.as_deref()),
            );
            (health.ok(), models.ok())
        };

        let model_items = models
            .as_ref()
            .and_then(|(_, payload)| payload.get("data"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let model_ids = model_items
            .iter()
            .filter_map(|item| item.get("id").and_then(Value::as_str).map(str::to_owned))
            .collect::<Vec<_>>();
        let model_modes = model_items
            .iter()
            .filter_map(|item| {
                Some((
                    item.get("id")?.as_str()?.to_owned(),
                    item.get("api")?.as_str()?.to_owned(),
                ))
            })
            .collect::<HashMap<_, _>>();
        let model_count = model_ids.len();

        let health_status = health
            .as_ref()
            .and_then(|(_, payload)| payload.get("status"))
            .and_then(Value::as_str)
            .unwrap_or(if status.running {
                "starting"
            } else {
                "offline"
            })
            .to_owned();

        let login_state = if login_open {
            "login_open".to_owned()
        } else if name == "chatgpt"
            || name == "gemini"
            || name == "mistral"
            || name == "zai"
            || name == "meta"
        {
            if model_count > 0 {
                "authenticated".to_owned()
            } else if status.running {
                "login_required".to_owned()
            } else {
                "offline".to_owned()
            }
        } else if !open_qwen_account_sessions.is_empty() && name == "qwen" {
            "account_login_open".to_owned()
        } else if status.running {
            "ready".to_owned()
        } else {
            "offline".to_owned()
        };

        ProviderOverview {
            name: name.to_owned(),
            running: status.running,
            started_at: status.started_at,
            base_url,
            health_status,
            login_state,
            model_count,
            models: model_ids,
            model_modes,
            web_search_supported: provider_web_search_supported(name),
            last_error: status.last_error,
        }
    }
}
