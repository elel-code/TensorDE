#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
max_lines="${RUST_FILE_MAX_LINES:-800}"

if [[ ! "$max_lines" =~ ^[1-9][0-9]*$ ]]; then
    echo "fail: RUST_FILE_MAX_LINES must be a positive integer, got: $max_lines" >&2
    exit 2
fi

failures=0
checked=0

while IFS= read -r -d '' file; do
    relative="${file#"$repo_root"/}"
    lines="$(wc -l < "$file")"
    lines="${lines//[[:space:]]/}"
    checked=$((checked + 1))

    if (( lines > max_lines )); then
        printf 'fail: %s has %d lines (limit %d)\n' \
            "$relative" "$lines" "$max_lines" >&2
        failures=$((failures + 1))
    fi
done < <(
    find "$repo_root" \
        -path "$repo_root/.git" -prune -o \
        -path "$repo_root/target" -prune -o \
        -path "$repo_root/artifacts" -prune -o \
        -path "$repo_root/docs/tensor-wallpaper" -prune -o \
        -path "$repo_root/references" -prune -o \
        -path "$repo_root/reverse-engineered" -prune -o \
        -type f -name '*.rs' -print0
)

if (( failures > 0 )); then
    printf 'Rust file line gate failed: files=%d failures=%d limit=%d\n' \
        "$checked" "$failures" "$max_lines" >&2
    exit 1
fi

printf 'ok: Rust file line gate files=%d limit=%d\n' "$checked" "$max_lines"
