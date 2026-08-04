# 2026 Proxy / AI-Gateway Reference Repos

Saved for later study. These are OpenAI/Anthropic-compatible LLM gateways in Rust — closest prior art to RustProxyHub's own routing + streaming layer. Mine them for SSE handling, multi-provider routing, and failover ideas.

| Repo | What to steal from it |
|---|---|
| [api7/aisix](https://github.com/api7/aisix) | Native SSE streaming, tool/function calling, JSON mode, vision/multimodal input, reasoning-content passthrough. Six routing strategies: `round_robin`, `weighted`, `failover`, `least_cost`, `least_latency`, `least_busy`. Best reference for our SSE + routing refactor. |
| [x5iu/llm-proxy](https://github.com/x5iu/llm-proxy) | Load balancing across OpenAI/Gemini/Anthropic, health monitoring, connection pooling. |
| [KochC/opencode-llm-proxy](https://github.com/KochC/opencode-llm-proxy) | Streaming + tool/function calling for OpenAI/Anthropic/Gemini/Responses API. |
| [ParzivalHack/Aegis.rs](https://github.com/ParzivalHack/Aegis.rs) | Single-binary transparent reverse proxy, two-layer security pipeline, live dashboard, sub-ms latency. |
| FerroGate (search "FerroGate rust ai gateway") | Route/secure/monitor/control traffic to OpenAI/Anthropic/Gemini/Azure. |
| [github.com/topics/llm-proxy?l=rust](https://github.com/topics/llm-proxy?l=rust&o=asc&s=updated) | Live topic feed — sorted by recently updated. |

## Pattern references (for the refactor)

- [Typestate pattern — Cliffle](https://cliffle.com/blog/rust-typestate/) — the canonical write-up.
- [Comprehensive Rust: typestate example](https://google.github.io/comprehensive-rust/idiomatic/leveraging-the-type-system/typestate-pattern/typestate-example.html) — Serde's Serializer as typestate.
- [Microsoft Rust Patterns: newtype & typestate](https://microsoft.github.io/RustTraining/rust-patterns-book/ch03-the-newtype-and-type-state-patterns.html) — connection-pool state (Idle → Active → Idle), our exact shape for provider sessions.
- [Software Patterns Lexicon: typestate](https://softwarepatternslexicon.com/rust/idiomatic-rust-patterns/the-typestate-pattern/) / [newtype](https://softwarepatternslexicon.com/rust/idiomatic-rust-patterns/the-newtype-pattern/) — RAII + typestate relationship.
- [typestate-builder crate](https://docs.rs/typestate-builder) — proc-macro builder with compile-time required-field enforcement, if we don't want to hand-roll builders.
