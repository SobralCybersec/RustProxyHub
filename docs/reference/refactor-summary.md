# Refactor Summary

Outcome of the phased refactor (see `REFACTOR_PLAN.md` for the live checklist). Everything below is **uncommitted** — LO owns git. Final state: `cargo build`/`clippy`/`fmt` clean, **146 tests green**, and the `standalone-provider-cli` feature build now warning-free (was 17 dead_code errors).

## Shipped

| Area | What changed |
|---|---|
| **Tool calling** (not a numbered phase — bug fix) | `StreamingToolParser` now lifts tool calls emitted as **bare or ```-fenced JSON** (single, multiple space-separated, split across stream chunks), not just `<tool_call>` tags — one shared parser, so every provider benefits, stream + non-stream. Fixed a streaming tag-fragment leak (`find_partial_tool_open_index`). Verified live: `tool_calls` fire with `finish_reason:"tool_calls"`. Kilo users: set provider `toolFormat: native`. |
| **DeepSeek Vision** | `deepseek-v4-vision` model: `model_type:"vision"` (mutually exclusive with expert/thinking), threaded through mode flags + payload builder + model registry. |
| **1 — deps** | Dropped unused `sha2`, `tower-http`. |
| **3 — panics** | 9 kimi static-regex `unwrap()` → `expect("valid static regex")`. Zero non-test unwraps remain. |
| **4 — newtypes** | `runtime/ids.rs`: `ModelId`/`SessionId`/`AccountId`/`ParentMessageId`. Adopted `SessionId`+`ParentMessageId` through deepseek's session-parent pipeline. |
| **5 — builder** | `DeepseekPayload` fluent builder replaced the positional-bool constructor (removed the `too_many_arguments` allow). |
| **6 — RAII** | `ActiveStreamGuard` frees the qwen stream-registry slot on early client disconnect. |
| **7 — SSE** | `sse_json`/`sse_done` DRY'd from 4 copies into `proxy_core` (one place owns framing). |
| **8 — SQL** | Every SQL statement hoisted into top-of-module `queries` blocks (both DB modules). |
| **10 — folders** | `providers/{deepseek,kimi,qwen}/` + `runtime/hub/`, `main.rs`→`mod.rs`, `lib.rs` wrappers deleted, `#[path]` repointed. Removed **all** vestigial `standalone-provider-cli` dead code + dropped the now-unused feature. |
| **11 — docs** | README Architecture section (mermaid diagram + module map + login flow). |

## Deferred (with reasons)

- **2 — comment reduction:** code already compliant (WHY-focused, mostly `///` rustdoc). No churn.
- **6 — typestate:** readiness already runtime-gated (`ensure_*_ready` + `initialized`); a compile-time typestate rewrite of the multi-provider bridge lifecycle is high-risk / low-marginal-value.
- **7 — full `axum::Sse` migration:** framing already centralized; a response-type rewrite across all providers isn't worth the risk on the hot path.
- **9 — FTS5:** verified **no full-text-search surface exists** (exact-match accounts, client-side provider filter, in-memory logs). Would be a new feature, not a refactor.
- **12 — release polish:** audit pass, nothing to fix.

## Remaining follow-ups

- **10b — file splits:** split the 2000-line `mod.rs` files by responsibility (`payload.rs`/`stream.rs`/…), file-by-file with tests between.
- **Tool leak — done in the shared parser**, but a native streaming `tool_calls` delta path (vs the current single-blob delta) could be added if any agent proves stricter.
- **Fail-fast login-required 503** when `capture_headers` times out with no session (a UX feature, not polish).
- **13 — final verification (LO/CI-gated):** manual smoke of the real workflows (provider login, model discovery, chat + SSE, Qwen accounts, agent-setup snippets, upload, dashboard status), `pnpm release:windows`, and CI green on the pushed branch. The loop cannot self-check these.
