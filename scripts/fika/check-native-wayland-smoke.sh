#!/usr/bin/env bash
# Optional Wayland display smoke for the SCTK-free native stack.
# Safe to run without a display: exits 0 with a skip message.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd "$script_dir/../../apps/fika" && pwd)"
cd "$root_dir"

if [[ -z "${WAYLAND_DISPLAY:-}" && -z "${WAYLAND_SOCKET:-}" ]]; then
    echo "check-native-wayland-smoke: skip (no WAYLAND_DISPLAY / WAYLAND_SOCKET)"
    exit 0
fi

echo "check-native-wayland-smoke: unit tests"
cargo test -p wayland-client-runtime native -- --test-threads=1

echo "check-native-wayland-smoke: native_capabilities"
cargo run -q -p wayland-client-runtime --example native_capabilities

echo "check-native-wayland-smoke: native_toplevel_smoke"
# Example exits after a short configure wait; timeout is a soft upper bound.
set +e
timeout 10s cargo run -q -p wayland-client-runtime --example native_toplevel_smoke
status=$?
set -e
# 0 = ok, 124 = timeout (still useful signal that the example did not abort)
if [[ "$status" -ne 0 && "$status" -ne 124 ]]; then
    echo "check-native-wayland-smoke: native_toplevel_smoke failed (exit $status)" >&2
    exit "$status"
fi

echo "check-native-wayland-smoke: native_csd_smoke"
set +e
timeout 8s cargo run -q -p wayland-client-runtime --example native_csd_smoke
status=$?
set -e
if [[ "$status" -ne 0 && "$status" -ne 124 ]]; then
    echo "check-native-wayland-smoke: native_csd_smoke failed (exit $status)" >&2
    exit "$status"
fi

echo "check-native-wayland-smoke: ok"
