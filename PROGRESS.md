# PROGRESS

Current goal: make manual login auto-start and await every provider runtime, authenticate internal admin calls, and cover all provider login routes with regression tests.

Manual-login task plan:
- [x] Trace frontend → Tauri → provider `/admin/manual_login` flow
- [x] Auto-start stopped provider and await HTTP readiness before login POST
- [x] Send configured hub API key on internal login/close/model calls
- [x] Add all-provider frontend and Rust coverage plus readiness/error tests
- [x] Run focused and full validation

Manual-login root cause:
- `start_provider_login_session` posted directly to fixed provider port without starting the service or waiting for its listener.
- Internal admin POSTs omitted the configured hub API key even though provider routes enforce it.

Manual-login files touched:
- `src-tauri/src/control_room/mod.rs`
- `tests/unit/store.test.ts`
- `PROGRESS.md`

Manual-login decisions:
- Keep provider runtimes as login owners; control room now starts a stopped provider and polls `/health` before calling its admin route.
- Use one 30-second listener-readiness window and a 60-second manual browser action timeout for every provider.
- Reuse configured hub API key for internal provider admin and model requests.
- Abort a stale tracked task only when its service status is stopped, allowing failed providers to restart on the next login attempt.

Manual-login verified checks:
- [x] `rtk npm run test -- --run tests/unit/store.test.ts` — 5 passed
- [x] `rtk cargo test --manifest-path src-tauri/Cargo.toml control_room::tests` — 7 passed
- [x] `rtk npm run test -- --run` — 18 passed
- [x] `rtk cargo test --manifest-path src-tauri/Cargo.toml` — 153 passed
- [x] `rtk node --test tests/node/*.test.mjs` — 6 passed
- [x] `rtk npm run lint`
- [x] `rtk npm run build`
- [x] `rtk cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
- [x] `rtk cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`
- [x] `rtk npx prettier --check tests/unit/store.test.ts`
- [x] `rtk git diff --check -- src-tauri/src/control_room/mod.rs tests/unit/store.test.ts PROGRESS.md`

Manual-login remaining work:
- [ ] Packaged-Tauri smoke: open and close login once for each provider with a real browser installation

Previous translation-task files touched:
- src/lib/ui-i18n.ts
- src/store.ts
- src/components/dashboard/HubHeader.vue
- src/components/dashboard/LoginStudio.vue
- src/components/dashboard/ProviderGrid.vue
- src/components/dashboard/DetailsDrawer.vue
- src/components/dashboard/WorkbenchPanel.vue
- src-tauri/src/runtime/providers/browser_runtime.rs
- tests/unit/runtime-ui.test.ts

Previous translation-task decisions:
- Keep `src/lib/ui-i18n.ts` as translation source of truth and expose one `statusLabel` store action for backend status values.
- Count header models from `store.hubModelOptions`, which is built from current provider catalogs, rather than `overview.hub.model_count`.
- Allow ChatGPT OAuth refresh/model discovery 15 seconds; keep all other browser providers at 4 seconds.

Previous translation-task verified checks:
- [x] `rtk npx prettier --check ...` — changed frontend/test files formatted
- [x] `rtk npm run test -- --run tests/unit/runtime-ui.test.ts` — 4 passed
- [x] `rtk cargo test --manifest-path src-tauri/Cargo.toml chatgpt_model_discovery_allows_oauth_refresh` — 1 passed
- [x] `rtk npm run lint`
- [x] `rtk npm run build`
- [x] `rtk cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`
- [x] `rtk git diff --check`
- [x] `rtk npm run test -- --run` — 16 passed
- [x] `rtk cargo test --manifest-path src-tauri/Cargo.toml` — 150 passed
- [x] `rtk cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`

Previous translation-task remaining work:
- [x] Translate dashboard headers, login studio, provider states, details drawer, and workbench hints
- [x] Render model count from discovered provider models
- [x] Extend ChatGPT model-discovery timeout and cover it with a Rust test
- [ ] Manual packaged-Tauri smoke for locale switching and live ChatGPT model discovery

Prior goal: replace ChatGPT browser-conversation replay with ChatGPT OAuth credentials and the Codex `/responses` upstream used by `openai-oauth`, eliminating user-message-folded system instructions that trigger refusal-like behavior.

Files touched:
- PROGRESS.md
- README.md
- src-tauri/resources/playwright-bridge/chatgpt-oauth.mjs
- src-tauri/resources/playwright-bridge/index.mjs
- src-tauri/src/runtime/providers/browser_runtime.rs
- tests/node/chatgpt-oauth.test.mjs

Source of truth:
- `/tmp/openai-oauth/README.md`
- `/tmp/openai-oauth/packages/core/src/runtime.ts`
- `/tmp/openai-oauth/packages/core/src/models.ts`
- `/tmp/openai-oauth/packages/local/src/auth-file.ts`
- `/tmp/openai-oauth/packages/openai-oauth/src/login.ts`
- `/tmp/openai-oauth/packages/openai-oauth/src/responses.ts`
- `src-tauri/resources/playwright-bridge/index.mjs`
- `src-tauri/src/runtime/providers/browser_runtime.rs`

Decisions made:
- Keep existing Rust provider/hub contract; replace only ChatGPT sidecar internals.
- Use OAuth authorization-code + PKCE, loopback callback on port 1455, Codex auth-file schema, refresh thresholds, account header, model catalog, and stateless `/responses` normalization from reference implementation.
- Preserve current dirty `src-tauri/tauri.conf.json`, `tsconfig.app.json`, and `vite.config.ts`; they predate this task.
- Preserve all non-ChatGPT browser providers unchanged.

Verified checks:
- [x] Existing ChatGPT capture/replay path mapped with CodeGraph
- [x] `openai-oauth` cloned to `/tmp/openai-oauth` and README/contracts inspected
- [x] Root cause identified: ChatGPT web endpoint drops inline system turns; current bridge folds trusted instructions into user text
- [x] `rtk node --check src-tauri/resources/playwright-bridge/chatgpt-oauth.mjs`
- [x] `rtk node --check src-tauri/resources/playwright-bridge/index.mjs`
- [x] `rtk node --test tests/node/chatgpt-oauth.test.mjs` — 6 passed
- [x] `rtk cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`
- [x] `CARGO_TARGET_DIR=/tmp/rustproxyhub-target cargo test --manifest-path src-tauri/Cargo.toml` — 149 passed
- [x] `CARGO_TARGET_DIR=/tmp/rustproxyhub-target cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
- [x] `rtk npm run build`

Done Work:
- [x] Add OAuth/Codex client with runnable Node tests
- [x] Wire ChatGPT login, models, and chat bridge calls
- [x] Remove obsolete ChatGPT `/backend-api/f/conversation` capture/replay helpers
- [x] Update ChatGPT fallback model and architecture docs
- [x] Run Node syntax/tests and Rust focused/full validation
- [X] Live smoke OAuth callback, account model discovery, and one Codex chat against an interactive ChatGPT account
- [x] Review final diff and confirm task edits are limited to ChatGPT integration, its tests/docs, and PROGRESS.md

Prior task carry-over (unchanged):
- [X] Manual smoke of Meta login/model discovery/chat replay against a real authenticated session (It's working, made that myself)
- [X] Confirmed meta
