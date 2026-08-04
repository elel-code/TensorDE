#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
analyzer="$script_dir/analyze-vulkan-frame-log.sh"
tmpdir="$(mktemp -d /tmp/tensor-files-vulkan-frame-analyzer.XXXXXX)"
trap 'rm -rf "$tmpdir"' EXIT

log="$tmpdir/frames.log"
cat >"$log" <<'EOF'
[tensor-files-vulkan] frame=1 reason=initial view=compact size=1280x720 work_pending=1 render_us=900
[tensor-files-vulkan] icons=36 quads=0 fallback=0 deferred=1
[tensor-files] autosmoke-scroll action=forward delta=64.0 changed=1 old_scroll_x=0.0 new_scroll_x=0.0 old_scroll_y=0.0 new_scroll_y=64.0
[tensor-files-vulkan] frame=2 reason=autosmoke-scroll view=compact size=1280x720 work_pending=0 render_us=680
[tensor-files-vulkan] frame=3 reason=initial view=icons size=1280x720 work_pending=0 render_us=720
EOF

summary="$($analyzer --require-frames "$log")"
[[ "$summary" == *"vulkan-frame-summary scope=all frames=3"* ]]
[[ "$summary" == *"render_us_p50=720"* ]]
[[ "$summary" == *"render_us_p95=900"* ]]
[[ "$summary" == *"render_us_max=900"* ]]
[[ "$summary" == *"work_pending=1"* ]]

compact_summary="$($analyzer --require-frames --gate-scope view:compact "$log")"
[[ "$compact_summary" == *"frames=2"* ]]

scroll_summary="$($analyzer --require-frames --require-autosmoke-scroll \
    --gate-scope reason:autosmoke-scroll --max-render-us 700 "$log")"
[[ "$scroll_summary" == *"frames=1"* ]]
[[ "$scroll_summary" == *"render_us_max=680"* ]]
[[ "$scroll_summary" == *"vulkan-autosmoke-scroll actions=1 changed=1"* ]]

if "$analyzer" --require-frames --gate-scope view:details "$log" >/dev/null 2>&1; then
    echo "expected missing-scope frame gate to fail" >&2
    exit 1
fi
if "$analyzer" --max-render-us 899 "$log" >/dev/null 2>&1; then
    echo "expected max-render-us gate to fail" >&2
    exit 1
fi

echo "ok: Vulkan frame log analyzer"
