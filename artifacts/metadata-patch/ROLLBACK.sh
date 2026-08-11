#!/usr/bin/env sh
set -eu
artifact_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
target=${1:?target file required}
cp "$artifact_dir/ORIGINAL_FILE" "$target"
printf 'restored %s\n' "$target"
