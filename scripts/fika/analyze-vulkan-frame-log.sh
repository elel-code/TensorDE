#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: analyze-vulkan-frame-log.sh [OPTIONS] LOG
       FIKA_LOG=1 FIKA_FRAME_LOG_ALL=1 target/debug/fika /etc 2>&1 |
           analyze-vulkan-frame-log.sh --require-frames -

Summarizes Fika's native Vulkan frame logs and optionally enforces runtime
evidence gates.

Options:
  --require-frames
      Fail if the selected scope contains no successful Vulkan frame.

  --require-autosmoke-scroll
      Fail unless FIKA_AUTOSMOKE_SCROLL produced a changed scroll action.

  --gate-scope all|view:MODE|reason:REASON
      Select frames included in the summary and render-time gate. Default: all.

  --max-render-us N
      Fail if any selected frame takes more than N microseconds.

  -h, --help
      Show this help.
EOF
}

require_frames=false
require_autosmoke_scroll=false
gate_scope=all
max_render_us=""
log_path=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --require-frames)
            require_frames=true
            ;;
        --require-autosmoke-scroll)
            require_autosmoke_scroll=true
            ;;
        --gate-scope)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--gate-scope requires all, view:MODE, or reason:REASON" >&2
                usage >&2
                exit 2
            fi
            gate_scope="$2"
            shift
            ;;
        --gate-scope=*)
            gate_scope="${1#--gate-scope=}"
            ;;
        --max-render-us)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--max-render-us requires a non-negative integer" >&2
                usage >&2
                exit 2
            fi
            max_render_us="$2"
            shift
            ;;
        --max-render-us=*)
            max_render_us="${1#--max-render-us=}"
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        --*)
            echo "unknown option: $1" >&2
            usage >&2
            exit 2
            ;;
        *)
            if [[ -n "$log_path" ]]; then
                echo "only one log path may be supplied" >&2
                usage >&2
                exit 2
            fi
            log_path="$1"
            ;;
    esac
    shift
done

if [[ -z "$log_path" ]]; then
    echo "missing log path" >&2
    usage >&2
    exit 2
fi
if [[ "$gate_scope" != all && "$gate_scope" != view:* && "$gate_scope" != reason:* ]]; then
    echo "invalid --gate-scope: $gate_scope" >&2
    exit 2
fi
if [[ -n "$max_render_us" && ! "$max_render_us" =~ ^[0-9]+$ ]]; then
    echo "--max-render-us must be a non-negative integer" >&2
    exit 2
fi
if [[ "$log_path" != - && ! -r "$log_path" ]]; then
    echo "cannot read log: $log_path" >&2
    exit 2
fi

awk \
    -v scope="$gate_scope" \
    -v require_frames="$require_frames" \
    -v require_scroll="$require_autosmoke_scroll" \
    -v max_render_gate="$max_render_us" '
function field(prefix,    i) {
    for (i = 1; i <= NF; i++) {
        if (index($i, prefix) == 1) {
            return substr($i, length(prefix) + 1)
        }
    }
    return ""
}
function selected(view, reason) {
    if (scope == "all") {
        return 1
    }
    if (index(scope, "view:") == 1) {
        return view == substr(scope, 6)
    }
    return reason == substr(scope, 8)
}
function percentile(sorted, count, percent,    index_value) {
    if (count == 0) {
        return 0
    }
    index_value = int((count * percent + 99) / 100)
    if (index_value < 1) {
        index_value = 1
    }
    return sorted[index_value]
}
/\[fika-vulkan\] frame=[0-9]+ / {
    reason = field("reason=")
    view = field("view=")
    render_us = field("render_us=") + 0
    if (!selected(view, reason)) {
        next
    }
    frame_count++
    render_times[frame_count] = render_us
    if (field("work_pending=") + 0 != 0) {
        pending_count++
    }
    if (render_us > render_max) {
        render_max = render_us
    }
}
/\[fika\] autosmoke-scroll / {
    scroll_actions++
    if (field("changed=") + 0 != 0) {
        scroll_changed++
    }
}
END {
    for (i = 1; i <= frame_count; i++) {
        sorted[i] = render_times[i]
    }
    for (i = 1; i <= frame_count; i++) {
        for (j = i + 1; j <= frame_count; j++) {
            if (sorted[j] < sorted[i]) {
                value = sorted[i]
                sorted[i] = sorted[j]
                sorted[j] = value
            }
        }
    }
    printf("vulkan-frame-summary scope=%s frames=%d render_us_p50=%d render_us_p95=%d render_us_max=%d work_pending=%d\n",
        scope,
        frame_count,
        percentile(sorted, frame_count, 50),
        percentile(sorted, frame_count, 95),
        render_max,
        pending_count)
    printf("vulkan-autosmoke-scroll actions=%d changed=%d\n", scroll_actions, scroll_changed)

    failed = 0
    if (require_frames == "true" && frame_count == 0) {
        print "vulkan-frame-gate-fail metric=frames actual=0 gate=>0" > "/dev/stderr"
        failed = 1
    }
    if (require_scroll == "true" && scroll_changed == 0) {
        print "vulkan-frame-gate-fail metric=autosmoke_scroll_changed actual=0 gate=>0" > "/dev/stderr"
        failed = 1
    }
    if (max_render_gate != "" && render_max > max_render_gate + 0) {
        printf("vulkan-frame-gate-fail metric=render_us_max actual=%d gate=%d\n",
            render_max,
            max_render_gate) > "/dev/stderr"
        failed = 1
    }
    exit failed
}' "$log_path"
