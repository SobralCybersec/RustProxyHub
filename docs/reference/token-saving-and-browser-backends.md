# Token saving and browser backend notes

## Implemented path

- Default browser runtime remains Playwright.
- Patchright is opt-in with `RUST_PROXY_BROWSER_BACKEND=patchright`.
- Patchright backend is allowed only for Chromium-family browser values: `chromium`, `chrome`, `edge`, `msedge`.
- Shared Rust preflight now does structured conversation compaction before browser send.
- Browser preflight applies an 18k-char structured cap.
- Prompt formatting compaction remains opt-in with `RUST_PROXY_HUB_PROMPT_COMPACTION=1`.
- Tool/function responses now compact by content type before prompt flattening:
  - JSON is minified first, then converted to a labeled excerpt only if still oversized.
  - Test/build output keeps summary and failure lines.
  - Logs keep ordered warn/error lines plus bounded head/tail context.
  - Media/base64-like payloads emit omission markers instead of raw blobs.

## Tokless wiring

Tokless is agent/tooling setup, not an LLM proxy transport. Wire it around agents that call the proxy, not inside the proxy runtime:

```bash
tokless doctor
tokless --dry-run --agents codex --tools rtk,caveman,ponytail,codegraph,context-mode
tokless --agents codex --tools rtk,caveman,ponytail,codegraph,context-mode
tokless index
```

Use `tokless --dry-run` before changing local agent config. Use `tokless index` after repo checkout or large code movement so codegraph/context tools stay useful.

## Research record

Local clones used for comparison:

- `research/repos/tokless` — agent/tool installer and index workflow.
- `research/repos/patchright` — opt-in Chromium automation backend candidate.
- `research/repos/playwright` — upstream browser automation behavior and compatibility baseline.
- `research/repos/copium` — token/context compaction prior art.
- `research/repos/tokensave` — layered cache/trim/compress pipeline ideas.
- `research/repos/sqz` — JSON/log/test-output reducers and prompt-cache heuristics.
- `research/repos/lean-ctx` — deterministic JSON crush, verbatim guards, spill/archive patterns.
- `research/repos/token-savior` — fail-open compactors and failure-first test output reduction.
- `research/repos/lowfat` — structured JSON safety gate and log/diff filters.
- `research/repos/zap` — schema-first JSON reducers and tee/recovery hints.
- `research/repos/code-context-engine` — deterministic grammar/output compression rules.

External sources reviewed:

- Tokless: https://github.com/HoangP8/tokless
- Patchright: https://github.com/Kaliiiiiiiiii-Vinyzu/patchright
- Playwright browsers/versioning: https://playwright.dev/docs/browsers
- Playwright BrowserType API: https://playwright.dev/docs/api/class-browsertype
- OpenAI prompt caching: https://openai.com/index/api-prompt-caching/
- OpenAI agent/computer environment guidance: https://openai.com/index/equip-responses-api-computer-environment/
- GitHub Copilot CLI context management: https://docs.github.com/en/enterprise-cloud%40latest/copilot/concepts/agents/copilot-cli/context-management

## Decision

Keep Playwright as default because it matches bundled dependency and browser-version maintenance path. As of Tuesday, August 11, 2026, Patchright remains Chromium-only and behind Playwright releases, so it stays explicit opt-in only.

Keep Tokless outside proxy runtime core. Use it around agents and repo bootstrap, not inside request transport.

Prefer content-type-aware tool-response compaction over generic whitespace trimming. Keep user/system semantics intact, compact noisy tool output, and never invent cache-usage metadata.

## Benchmark

Run:

```bash
node scripts/benchmark-token-savings.mjs
node scripts/benchmark-token-savings.mjs path/to/prompt.txt
```

Output fields:

- `baseline_chars`, `compacted_chars`
- `baseline_tokens`, `compacted_tokens`
- `tokens_saved`, `savings_percent`
