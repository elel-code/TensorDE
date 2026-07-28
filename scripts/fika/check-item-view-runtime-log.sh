#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: check-item-view-runtime-log.sh LOG [LOG ...]

Runs the standard post-P11e item-view runtime perf-log gates against a saved
FIKA_PERF_ITEM_VIEW=1 log. This script does not replace the manual DnD and
rename smoke checklist.
EOF
}

if [[ $# -lt 1 || "$1" == "-h" || "$1" == "--help" ]]; then
    usage
    if [[ $# -eq 1 && ( "$1" == "-h" || "$1" == "--help" ) ]]; then
        exit 0
    fi
    exit 2
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd "$script_dir/../../apps/fika" && pwd)"
tmpdir="$(mktemp -d)"
cleanup() {
    rm -rf "$tmpdir"
}
trap cleanup EXIT

combined_log="$tmpdir/item-view-runtime-combined.log"
: > "$combined_log"
for log_path in "$@"; do
    if [[ ! -s "$log_path" ]]; then
        echo "missing or empty item-view runtime log: $log_path" >&2
        exit 1
    fi
    printf '[fika analyzer] log-boundary path=%s\n' "$log_path" >> "$combined_log"
    sed -n '1,$p' "$log_path" >> "$combined_log"
done

"$script_dir/analyze-item-view-perf.sh" \
    --require-steady \
    --require-details \
    --require-warm-details-visual \
    --require-static-visual \
    --require-static-modes Compact,Icons \
    --require-interaction \
    --require-renderer-policy \
    --expect-retained-item-policy \
    --require-paint-slots \
    --require-renderer-policy-modes Compact,Icons,Details \
    --require-modes Compact,Icons,Details \
    --warm-steady-total-us 1500 \
    --file-grid-build-us 3000 \
    --image-paint-us 3000 \
    --warm-static-visual-paint-us 6000 \
    --warm-custom-paint-us 6000 \
    "$combined_log"
