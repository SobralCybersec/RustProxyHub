# PLAN

## Objective

Document research-driven token/context work now wired into RustProxyHub, what was studied in `research/repos`, what shipped, what remains, and how to verify it.

## Repos studied

### Token/context-saving

- `research/repos/copium`
  - proxy-side compression, phase-aware context management, tokenizer abstractions, quality-gate ideas
- `research/repos/tokless`
  - project indexing, runtime context tools, agent-bootstrap boundary
- `research/repos/tokensave`
  - layered optimization pipeline: cache, routing, prompt compression, context trim/summarize
- `research/repos/sqz`
  - dedup refs, per-command compression, token-savings ledger
- `research/repos/lean-ctx`
  - read modes, cache hits, benchmark discipline, context packages
- `research/repos/token-savior`
  - structural retrieval ladder, persistent memory, token benchmark framing
- `research/repos/code-context-engine`
  - code indexing and search-first retrieval
- `research/repos/lowfat`
  - lightweight output-noise stripping
- `research/repos/zap`
  - lazy-load/indexing ideas

### Browser/runtime

- `research/repos/playwright`
  - baseline browser automation
- `research/repos/patchright`
  - Chromium-only anti-detection option
- `research/repos/qwenproxy`
  - browser-provider architecture reference

## What is wired now

### Proxy/runtime

- Shared prompt flattening stays in `src-tauri/src/runtime/proxy_core.rs`.
- Shared prompt preflight now exists in `src-tauri/src/runtime/proxy_core.rs`:
  - optional token budget
  - minified tool-schema rendering
  - preserved assistant tool-call + matching tool-result groups
  - explicit failure when system + tool instructions alone exceed budget
  - optional dedup of normalized duplicate system blocks
- Qwen now uses shared preflight from `src-tauri/src/runtime/providers/qwen/mod.rs` instead of private truncation logic.
- Browser providers now use shared preflight from `src-tauri/src/runtime/providers/browser_runtime.rs`.
- Browser tool-mode instructions stay in system lane instead of duplicating the whole flattened prompt in conversation.

### Token saving

- Tool schema JSON is minified instead of pretty-printed.
- Rust shared preflight now supports structured conversation compaction.
- Browser preflight applies an 18k-char structured cap before bridge send.
- System/tool instructions remain preserved during shared structured compaction.
- Tool/function responses now compact by content type before prompt flattening:
  - JSON gets minified first, then labeled excerpt compaction if still oversized
  - long test/build output keeps summary and failure lines
  - long logs keep ordered warn/error lines plus bounded head/tail
  - media/base64-style payloads get omission markers instead of raw blobs
- Formatting-only Rust prompt compaction still remains opt-in via `RUST_PROXY_HUB_PROMPT_COMPACTION`.
- ChatGPT bridge structured compaction stays in `src-tauri/resources/playwright-bridge/prompt-compaction.mjs`.
- Benchmark script reports baseline vs trimmed vs structured savings in `scripts/benchmark-token-savings.mjs`.

### Browser backend

- Playwright remains default.
- Patchright remains opt-in only.
- Tokless remains documented as developer/bootstrap tooling, not proxy runtime transport.

## Ideas explicitly deferred

- No persistent compression cache in proxy runtime yet.
  - reason: stale instruction/schema reuse risk without versioned content-hash + TTL design
- No semantic cache yet.
  - reason: privacy/correctness risk higher than prompt-compaction gain for current scope
- No Tokless runtime integration.
  - reason: use around agents/tooling, not inside request transport
- No Patchright default.
  - reason: Chromium-only and packaging/maintenance cost

## Techniques ported from research

- **Copium**
  - shared preflight/compression thinking
  - system/tool-budget fail-fast guard
- **TokenSave**
  - layered pipeline mindset: preflight before route/send
  - explicit benchmark surface
- **sqz / lean-ctx / token-savior**
  - dedup-first thinking
  - search/index/context work should stay outside raw prompt transport when possible
- **lowfat / zap / sqz / copium**
  - safe JSON minify-first handling
  - bounded head/tail log excerpts with explicit omission markers
  - failure-first test-output reduction
  - media/blob omission instead of raw payload flattening

## Remaining work

1. Surface honest prompt-savings metadata in API responses.
2. Decide whether a bounded local cache is worth adding after exact-match hashing + TTL design.

## Validation

### Narrow automated checks

```bash
node --test tests/node/prompt-compaction.test.mjs tests/node/chatgpt-oauth.test.mjs tests/node/chatgpt-web-response.test.mjs
pnpm benchmark:tokens
printf '%s\n' 'System: keep every semantic instruction.' '' 'User: summarize this repo.' '' 'Assistant: inspecting files.' '' 'User: keep whitespace cleanup only.' > /tmp/rph-token-fixture.txt && pnpm benchmark:tokens /tmp/rph-token-fixture.txt 18000
cargo test --manifest-path src-tauri/Cargo.toml
```

### Manual checks still needed

- real logged-in browser-provider smoke
- packaged Windows flow
- tool-calling end-to-end against browser providers
