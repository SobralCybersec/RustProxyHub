#!/usr/bin/env sh
set -eu
artifact_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
target=${1:-/home/satu/Documents/RustProxyHub/src-tauri/resources/playwright-bridge/index.mjs}
cp "$artifact_dir/rollback-source-index.mjs" "$target"
node --check "$target"
