#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: summarize-item-view-renderer-evidence.sh LOG [LOG ...]

Runs the standard item-view runtime perf-log gate, then prints a Markdown
evidence block for a task-local report. This summarizes perf-log evidence only;
manual DnD and rename smoke results must still be recorded by a human reviewer.
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
runtime_gate="$script_dir/check-item-view-runtime-log.sh"

summary="$("$runtime_gate" "$@")"
log_list=""
for log_path in "$@"; do
    log_list="${log_list}- \`$log_path\`
"
done

cat <<EOF
## Item View Renderer Evidence

- Logs:
$log_list- Perf gate: \`scripts/fika/check-item-view-runtime-log.sh $*\`
- Manual review still required: DnD and rename behavior.

\`\`\`text
$summary
\`\`\`

Renderer decision follow-up:

- Compact/Icons base visuals: keep or revisit custom paint using static visual,
  item shape-cache, item glyph-cache, item glyph-budget, and custom paint maxima
  above.
- Compact/Icons image layer: keep or revisit custom paint using item image maxima, custom paint maxima, and the image_sources counters above.
- Details visual layer: keep or revisit custom paint using details visual,
  shape-cache, glyph-cache, and glyph-budget channels above.
- Renderer surface counts: compare renderer_policy_frames across the candidate and baseline logs.
- Retained hitbox drag start: keep gpui_drag_shell=0 and validate behavior with the DnD smoke checklist, not perf logs alone.
EOF
