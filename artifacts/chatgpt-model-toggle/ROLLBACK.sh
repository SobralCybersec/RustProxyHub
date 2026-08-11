#!/usr/bin/env bash
set -euo pipefail
target=${1:?target copy required}
baseline=${2:?baseline copy required}
cp "$baseline" "$target"
printf 'restored %s from %s\n' "$target" "$baseline"
