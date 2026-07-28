#!/usr/bin/env bash
# Start every maintained interactive TTY smoke client without pasting a long command.
set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
singbox="$HOME/Myapps/GUI.for.SingBox-linux-amd64/GUI.for.SingBox"

if (( $# > 1 )); then
    printf 'usage: %s [forever]\n' "$0" >&2
    exit 64
fi

case "${1:-}" in
    "") lifetime=(--duration "${TENSOR_TTY_DURATION:-60}") ;;
    forever) lifetime=(--forever) ;;
    *)
        printf 'usage: %s [forever]\n' "$0" >&2
        exit 64
        ;;
esac

if [[ ! -x "$singbox" ]]; then
    printf 'GUI.for.SingBox is not executable: %s\n' "$singbox" >&2
    exit 127
fi
for process in /proc/[0-9]*; do
    if [[ "$process/exe" -ef "$singbox" ]]; then
        printf 'Close the existing GUI.for.SingBox instance before this Tensor smoke run.\n' >&2
        exit 1
    fi
done

exec uv run "$root/scripts/tty.py" \
    --fcitx \
    "${lifetime[@]}" \
    --client ghostty \
    --client-arg=--gtk-single-instance=false \
    --client "$singbox"
