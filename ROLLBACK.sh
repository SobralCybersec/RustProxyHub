#!/usr/bin/env bash
set -euo pipefail

TARGET="${1:-src-tauri/src/runtime/providers/qwen/mod.rs}"
BACKUP="${2:-/tmp/rustproxyhub-connection-transaction/qwen.mod.original.rs}"
cp "$BACKUP" "$TARGET"
sha256sum "$TARGET"
