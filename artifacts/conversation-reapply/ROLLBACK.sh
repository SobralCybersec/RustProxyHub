#!/usr/bin/env bash
set -euo pipefail

target=${1:?target file required}
baseline=${2:?baseline file required}
if [[ ! -f "$target" || ! -f "$baseline" ]]; then
  printf 'rollback input missing\n' >&2
  exit 2
fi
cp -- "$baseline" "$target"
printf 'restored %s from %s\n' "$target" "$baseline"
