# syntax=docker/dockerfile:1
# ============================================================
# RustProxyHub — reproducible build + verify environment
#
# Why this exists: the app is a Tauri desktop GUI — it can't meaningfully
# run headless in a container (interactive browser login, per-profile
# cookies). What CAN live in Docker is a reproducible *verify/build box*:
# the same toolchain on any PC, running the full CI gate. The runnable
# installers for end users come from `tauri-action` (release.yml), not
# from this image.
#
# Base is Debian (glibc). Alpine/musl is a hard no — webkit2gtk-4.1
# (Tauri) and Playwright's bundled Chromium both dynamically link glibc.
#
# Build:  docker build -f docker/Dockerfile -t rustproxyhub-verify .
# Verify: docker run --rm rustproxyhub-verify
# ============================================================
FROM node:22-bookworm-slim

# ── Tauri v2 build chain (GTK3 / webkit2gtk-4.1, glibc) ──
# One layer, --no-install-recommends, apt lists dropped to keep it lean.
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential curl wget file pkg-config git ca-certificates \
      libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev \
      libayatana-appindicator3-dev libxdo-dev libssl-dev patchelf \
    && rm -rf /var/lib/apt/lists/*

# ── Rust: stable + clippy + rustfmt + cargo-audit ──
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --profile minimal --default-toolchain stable \
        -c clippy -c rustfmt \
    && cargo install cargo-audit --locked

# ── pnpm 11 (matches the version pinned in ci.yml; reads lockfile 9.0) ──
# Plain npm global install sidesteps the corepack signature-verification
# failures that bite in Docker when Node/Corepack versions drift.
RUN npm install -g pnpm@11

WORKDIR /app

# ── Dependency layers FIRST — they survive source-only edits ──
# pnpm fetch needs only the lockfile; cargo fetch only the manifests.
COPY package.json pnpm-lock.yaml ./
RUN --mount=type=cache,id=pnpm,target=/root/.local/share/pnpm/store \
    pnpm fetch
COPY src-tauri/Cargo.toml src-tauri/Cargo.lock ./src-tauri/
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo fetch --manifest-path src-tauri/Cargo.toml

# ── Real source + offline install (no network, uses fetched stores) ──
COPY . .
RUN pnpm install --frozen-lockfile --offline

# The default `pnpm verify` shells out to `rtk`, a local wrapper not present
# in CI — so run the gate directly here (same checks, no wrapper).
# ponytail: single-stage builder, no cargo-chef. The cache mounts above
# already skip dep re-downloads; if compile time becomes the bottleneck,
# split into chef planner/cacher stages. Not worth it for a CI box today.
CMD ["sh", "-c", "pnpm lint && pnpm type-check && pnpm test --run && node ./scripts/prepare-tauri-playwright-resources.mjs && pnpm build && cargo fmt --manifest-path src-tauri/Cargo.toml --all --check && cargo test --manifest-path src-tauri/Cargo.toml && cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings && cargo audit -f src-tauri/Cargo.lock"]
