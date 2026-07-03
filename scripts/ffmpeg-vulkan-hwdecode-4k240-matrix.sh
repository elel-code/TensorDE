#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
runner=(python3 "$script_dir/ffmpeg_vulkan_hwdecode_matrix.py")
if command -v uv >/dev/null 2>&1; then
  export UV_CACHE_DIR="${UV_CACHE_DIR:-${TMPDIR:-/tmp}/gilder-uv-cache}"
  runner=(uv run --script "$script_dir/ffmpeg_vulkan_hwdecode_matrix.py")
fi

exec "${runner[@]}" --artifact-prefix gilder-ffmpeg-vulkan-4k240 "$@"
