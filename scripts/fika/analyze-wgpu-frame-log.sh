#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: analyze-wgpu-frame-log.sh [OPTIONS] LOG
       FIKA_LOG=1 target/debug/fika /etc 2>&1 | analyze-wgpu-frame-log.sh -

Summarizes [fika-wgpu] frame logs and optionally enforces render latency gates.

Options:
  --require-frames
      Fail if no wgpu frame log lines are present.

  --max-render-us N
      Fail if any frame render time exceeds N microseconds.

  --warm-max-render-us N
      Fail if any warm frame render time exceeds N microseconds. The first
      frame per view is treated as cold.

  --warm-p95-render-us N
      Fail if warm render p95 exceeds N microseconds.

  --max-text-raster-us N
      Fail if any frame text raster time exceeds N microseconds.

  --max-icon-raster-us N
      Fail if any frame icon raster time exceeds N microseconds.

  --max-text-deferred N
      Fail if any frame text deferred count exceeds N.

  --gate-scope SCOPE
      Apply latency gates to this summary scope instead of all frames. Useful
      values include "view:compact" and "reason:autosmoke-scroll".

  --require-autosmoke-scroll
      Fail unless FIKA_WGPU_AUTOSMOKE_SCROLL produced changed scroll actions.

  --min-metadata-visible N
      Fail if the selected scope does not report at least N visible metadata
      role candidates.

  --min-metadata-results N
      Fail if the selected scope does not report at least N completed metadata
      role results.

  --min-metadata-applied N
      Fail if the selected scope does not report at least N applied metadata
      role results.

  -h, --help
      Show this help.
EOF
}

require_frames=false
max_render_us=""
warm_max_render_us=""
warm_p95_render_us=""
max_text_raster_us=""
max_icon_raster_us=""
max_text_deferred=""
gate_scope="all"
require_autosmoke_scroll=false
min_metadata_visible=""
min_metadata_results=""
min_metadata_applied=""
log_path=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --require-frames)
            require_frames=true
            ;;
        --max-render-us)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--max-render-us requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            max_render_us="$2"
            shift
            ;;
        --max-render-us=*)
            max_render_us="${1#--max-render-us=}"
            ;;
        --warm-max-render-us)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--warm-max-render-us requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            warm_max_render_us="$2"
            shift
            ;;
        --warm-max-render-us=*)
            warm_max_render_us="${1#--warm-max-render-us=}"
            ;;
        --warm-p95-render-us)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--warm-p95-render-us requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            warm_p95_render_us="$2"
            shift
            ;;
        --warm-p95-render-us=*)
            warm_p95_render_us="${1#--warm-p95-render-us=}"
            ;;
        --max-text-raster-us)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--max-text-raster-us requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            max_text_raster_us="$2"
            shift
            ;;
        --max-text-raster-us=*)
            max_text_raster_us="${1#--max-text-raster-us=}"
            ;;
        --max-icon-raster-us)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--max-icon-raster-us requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            max_icon_raster_us="$2"
            shift
            ;;
        --max-icon-raster-us=*)
            max_icon_raster_us="${1#--max-icon-raster-us=}"
            ;;
        --max-text-deferred)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--max-text-deferred requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            max_text_deferred="$2"
            shift
            ;;
        --max-text-deferred=*)
            max_text_deferred="${1#--max-text-deferred=}"
            ;;
        --gate-scope)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--gate-scope requires a scope value" >&2
                usage >&2
                exit 2
            fi
            gate_scope="$2"
            shift
            ;;
        --gate-scope=*)
            gate_scope="${1#--gate-scope=}"
            ;;
        --require-autosmoke-scroll)
            require_autosmoke_scroll=true
            ;;
        --min-metadata-visible)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--min-metadata-visible requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            min_metadata_visible="$2"
            shift
            ;;
        --min-metadata-visible=*)
            min_metadata_visible="${1#--min-metadata-visible=}"
            ;;
        --min-metadata-results)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--min-metadata-results requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            min_metadata_results="$2"
            shift
            ;;
        --min-metadata-results=*)
            min_metadata_results="${1#--min-metadata-results=}"
            ;;
        --min-metadata-applied)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--min-metadata-applied requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            min_metadata_applied="$2"
            shift
            ;;
        --min-metadata-applied=*)
            min_metadata_applied="${1#--min-metadata-applied=}"
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
                echo "only one LOG path may be provided" >&2
                usage >&2
                exit 2
            fi
            log_path="$1"
            ;;
    esac
    shift
done

if [[ -z "$log_path" ]]; then
    echo "missing LOG path" >&2
    usage >&2
    exit 2
fi

for value in "$max_render_us" "$warm_max_render_us" "$warm_p95_render_us" "$max_text_raster_us" "$max_icon_raster_us" "$max_text_deferred" "$min_metadata_visible" "$min_metadata_results" "$min_metadata_applied"; do
    if [[ -n "$value" && ! "$value" =~ ^[0-9]+$ ]]; then
        echo "gate values must be integer values" >&2
        exit 2
    fi
done

if [[ -z "$gate_scope" ]]; then
    echo "--gate-scope must not be empty" >&2
    exit 2
fi

input_path="$log_path"
if [[ "$log_path" == "-" ]]; then
    input_path="/dev/stdin"
elif [[ ! -s "$log_path" ]]; then
    echo "missing or empty log: $log_path" >&2
    exit 1
fi

awk \
    -v require_frames="$require_frames" \
    -v max_render_us="${max_render_us:-}" \
    -v warm_max_render_us="${warm_max_render_us:-}" \
    -v warm_p95_render_us="${warm_p95_render_us:-}" \
    -v require_autosmoke_scroll="$require_autosmoke_scroll" \
    -v max_text_raster_us="${max_text_raster_us:-}" \
    -v max_icon_raster_us="${max_icon_raster_us:-}" \
    -v max_text_deferred="${max_text_deferred:-}" \
    -v min_metadata_visible="${min_metadata_visible:-}" \
    -v min_metadata_results="${min_metadata_results:-}" \
    -v min_metadata_applied="${min_metadata_applied:-}" \
    -v gate_scope="$gate_scope" '
function value_of(key,    i, pair) {
    for (i = 1; i <= NF; i++) {
        split($i, pair, "=")
        if (pair[1] == key) {
            return pair[2]
        }
    }
    return ""
}

function numeric_value(key,    value) {
    value = value_of(key)
    gsub(/[^0-9.].*/, "", value)
    return value + 0
}

function boolean_value(key,    value) {
    value = value_of(key)
    if (value == "true" || value == "1") {
        return 1
    }
    return 0
}

function bump_metric(prefix, metric, value,    key) {
    key = prefix SUBSEP metric
    if (!(key in max) || value > max[key]) {
        max[key] = value
    }
}

function add_render(prefix, value, warm,    key) {
    key = prefix SUBSEP "render"
    render_count[key]++
    render_values[key, render_count[key]] = value
    if (warm) {
        warm_key = prefix SUBSEP "warm_render"
        render_count[warm_key]++
        render_values[warm_key, render_count[warm_key]] = value
    }
}

function percentile(prefix, metric, percent,    key, count, i, j, tmp, rank) {
    key = prefix SUBSEP metric
    count = render_count[key] + 0
    if (count <= 0) {
        return 0
    }
    delete sorted
    for (i = 1; i <= count; i++) {
        sorted[i] = render_values[key, i]
    }
    for (i = 1; i <= count; i++) {
        for (j = i + 1; j <= count; j++) {
            if (sorted[j] < sorted[i]) {
                tmp = sorted[i]
                sorted[i] = sorted[j]
                sorted[j] = tmp
            }
        }
    }
    rank = int((percent * count + 99) / 100)
    if (rank < 1) {
        rank = 1
    }
    if (rank > count) {
        rank = count
    }
    return sorted[rank]
}

function add_prewarm(prefix, resolve, entries, deferred, read_ahead, over_budget,    key) {
    prewarm_count[prefix]++
    prewarm_values[prefix, prewarm_count[prefix]] = resolve
    bump_metric(prefix, "prewarm_resolve", resolve)
    bump_metric(prefix, "prewarm_entries", entries)
    bump_metric(prefix, "prewarm_deferred", deferred)
    bump_metric(prefix, "prewarm_read_ahead", read_ahead)
    bump_metric(prefix, "prewarm_over_budget", over_budget)
}

function add_text_prewarm(prefix, raster, entries, read_ahead, hits, misses, deferred, over_budget) {
    text_prewarm_count[prefix]++
    text_prewarm_values[prefix, text_prewarm_count[prefix]] = raster
    bump_metric(prefix, "text_prewarm_raster", raster)
    bump_metric(prefix, "text_prewarm_entries", entries)
    bump_metric(prefix, "text_prewarm_read_ahead", read_ahead)
    bump_metric(prefix, "text_prewarm_hits", hits)
    bump_metric(prefix, "text_prewarm_misses", misses)
    bump_metric(prefix, "text_prewarm_deferred", deferred)
    bump_metric(prefix, "text_prewarm_over_budget", over_budget)
}

function add_metadata_prewarm(prefix, visible, deferred, batches, results, applied) {
    metadata_prewarm_count[prefix]++
    metadata_prewarm_visible_total[prefix] += visible
    metadata_prewarm_deferred_total[prefix] += deferred
    metadata_prewarm_batches_total[prefix] += batches
    metadata_prewarm_results_total[prefix] += results
    metadata_prewarm_applied_total[prefix] += applied
    bump_metric(prefix, "metadata_prewarm_visible", visible)
    bump_metric(prefix, "metadata_prewarm_deferred", deferred)
    bump_metric(prefix, "metadata_prewarm_batches", batches)
    bump_metric(prefix, "metadata_prewarm_results", results)
    bump_metric(prefix, "metadata_prewarm_applied", applied)
}

function prewarm_percentile(prefix, percent,    count, i, j, tmp, rank) {
    count = prewarm_count[prefix] + 0
    if (count <= 0) {
        return 0
    }
    delete sorted
    for (i = 1; i <= count; i++) {
        sorted[i] = prewarm_values[prefix, i]
    }
    for (i = 1; i <= count; i++) {
        for (j = i + 1; j <= count; j++) {
            if (sorted[j] < sorted[i]) {
                tmp = sorted[i]
                sorted[i] = sorted[j]
                sorted[j] = tmp
            }
        }
    }
    rank = int((percent * count + 99) / 100)
    if (rank < 1) {
        rank = 1
    }
    if (rank > count) {
        rank = count
    }
    return sorted[rank]
}

function text_prewarm_percentile(prefix, percent,    count, i, j, tmp, rank) {
    count = text_prewarm_count[prefix] + 0
    if (count <= 0) {
        return 0
    }
    delete sorted
    for (i = 1; i <= count; i++) {
        sorted[i] = text_prewarm_values[prefix, i]
    }
    for (i = 1; i <= count; i++) {
        for (j = i + 1; j <= count; j++) {
            if (sorted[j] < sorted[i]) {
                tmp = sorted[i]
                sorted[i] = sorted[j]
                sorted[j] = tmp
            }
        }
    }
    rank = int((percent * count + 99) / 100)
    if (rank < 1) {
        rank = 1
    }
    if (rank > count) {
        rank = count
    }
    return sorted[rank]
}

function print_summary(prefix, label,    frames, warm_frames, render_max, render_p50, render_p95, warm_p95, warm_max_value) {
    frames = frame_count[prefix] + 0
    if (frames <= 0) {
        return
    }
    warm_frames = render_count[prefix SUBSEP "warm_render"] + 0
    render_max = max[prefix SUBSEP "render"] + 0
    render_p50 = percentile(prefix, "render", 50)
    render_p95 = percentile(prefix, "render", 95)
    warm_p95 = percentile(prefix, "warm_render", 95)
    warm_max_value = max[prefix SUBSEP "warm_render"] + 0
    printf("wgpu-frame-summary scope=%s frames=%d warm_frames=%d render_us_p50=%d render_us_p95=%d render_us_max=%d warm_render_us_p95=%d warm_render_us_max=%d prepare_us_max=%d surface_us_max=%d encode_present_us_max=%d layout_us_max=%d text_raster_us_max=%d icon_resolve_us_max=%d icon_raster_us_max=%d text_atlas_reused_max=%d text_deferred_max=%d icon_deferred_max=%d icon_raster_deferred_max=%d visible_max=%d\n",
        label,
        frames,
        warm_frames,
        render_p50,
        render_p95,
        render_max,
        warm_p95,
        warm_max_value,
        max[prefix SUBSEP "prepare"] + 0,
        max[prefix SUBSEP "surface"] + 0,
        max[prefix SUBSEP "encode_present"] + 0,
        max[prefix SUBSEP "layout"] + 0,
        max[prefix SUBSEP "text_raster"] + 0,
        max[prefix SUBSEP "icon_resolve"] + 0,
        max[prefix SUBSEP "icon_raster"] + 0,
        max[prefix SUBSEP "text_atlas_reused"] + 0,
        max[prefix SUBSEP "text_deferred"] + 0,
        max[prefix SUBSEP "icon_deferred"] + 0,
        max[prefix SUBSEP "icon_raster_deferred"] + 0,
        max[prefix SUBSEP "visible"] + 0)
}

function print_prewarm_summary(prefix, label,    count, resolve_p50, resolve_p95) {
    count = prewarm_count[prefix] + 0
    if (count <= 0) {
        return
    }
    resolve_p50 = prewarm_percentile(prefix, 50)
    resolve_p95 = prewarm_percentile(prefix, 95)
    printf("wgpu-prewarm-summary scope=%s samples=%d resolve_us_p50=%d resolve_us_p95=%d resolve_us_max=%d entries_max=%d deferred_max=%d read_ahead_max=%d over_budget_max=%d\n",
        label,
        count,
        resolve_p50,
        resolve_p95,
        max[prefix SUBSEP "prewarm_resolve"] + 0,
        max[prefix SUBSEP "prewarm_entries"] + 0,
        max[prefix SUBSEP "prewarm_deferred"] + 0,
        max[prefix SUBSEP "prewarm_read_ahead"] + 0,
        max[prefix SUBSEP "prewarm_over_budget"] + 0)
}

function print_text_prewarm_summary(prefix, label,    count, raster_p50, raster_p95) {
    count = text_prewarm_count[prefix] + 0
    if (count <= 0) {
        return
    }
    raster_p50 = text_prewarm_percentile(prefix, 50)
    raster_p95 = text_prewarm_percentile(prefix, 95)
    printf("wgpu-text-prewarm-summary scope=%s samples=%d raster_us_p50=%d raster_us_p95=%d raster_us_max=%d entries_max=%d read_ahead_max=%d hits_max=%d misses_max=%d deferred_max=%d over_budget_max=%d\n",
        label,
        count,
        raster_p50,
        raster_p95,
        max[prefix SUBSEP "text_prewarm_raster"] + 0,
        max[prefix SUBSEP "text_prewarm_entries"] + 0,
        max[prefix SUBSEP "text_prewarm_read_ahead"] + 0,
        max[prefix SUBSEP "text_prewarm_hits"] + 0,
        max[prefix SUBSEP "text_prewarm_misses"] + 0,
        max[prefix SUBSEP "text_prewarm_deferred"] + 0,
        max[prefix SUBSEP "text_prewarm_over_budget"] + 0)
}

function print_metadata_prewarm_summary(prefix, label,    count) {
    count = metadata_prewarm_count[prefix] + 0
    if (count <= 0) {
        return
    }
    printf("wgpu-metadata-prewarm-summary scope=%s samples=%d visible_total=%d deferred_total=%d batches_total=%d results_total=%d applied_total=%d visible_max=%d deferred_max=%d batches_max=%d results_max=%d applied_max=%d\n",
        label,
        count,
        metadata_prewarm_visible_total[prefix] + 0,
        metadata_prewarm_deferred_total[prefix] + 0,
        metadata_prewarm_batches_total[prefix] + 0,
        metadata_prewarm_results_total[prefix] + 0,
        metadata_prewarm_applied_total[prefix] + 0,
        max[prefix SUBSEP "metadata_prewarm_visible"] + 0,
        max[prefix SUBSEP "metadata_prewarm_deferred"] + 0,
        max[prefix SUBSEP "metadata_prewarm_batches"] + 0,
        max[prefix SUBSEP "metadata_prewarm_results"] + 0,
        max[prefix SUBSEP "metadata_prewarm_applied"] + 0)
}

function print_autosmoke_scroll_summary() {
    if (autosmoke_scroll_actions <= 0) {
        return
    }
    printf("wgpu-autosmoke-scroll actions=%d changed=%d forward_changed=%d back_changed=%d max_new_scroll_x=%.1f max_new_scroll_y=%.1f\n",
        autosmoke_scroll_actions + 0,
        autosmoke_scroll_changed + 0,
        autosmoke_scroll_forward_changed + 0,
        autosmoke_scroll_back_changed + 0,
        max["autosmoke-scroll" SUBSEP "new_scroll_x"] + 0,
        max["autosmoke-scroll" SUBSEP "new_scroll_y"] + 0)
}

function gate_metric(gate, actual, label,    failed) {
    if (gate != "" && actual > gate) {
        printf("wgpu-frame-gate-fail scope=%s metric=%s actual=%d gate=%d\n", gate_scope, label, actual, gate) > "/dev/stderr"
        return 1
    }
    return 0
}

function gate_min_metric(gate, actual, label,    failed) {
    if (gate != "" && actual < gate) {
        printf("wgpu-frame-gate-fail scope=%s metric=%s actual=%d gate=>=%d\n", gate_scope, label, actual, gate) > "/dev/stderr"
        return 1
    }
    return 0
}

/\[fika-wgpu\] frame=/ {
    view = value_of("view")
    if (view == "") {
        view = "unknown"
    }
    reason = value_of("reason")
    if (reason == "") {
        reason = "unknown"
    }
    prefix = "view:" view
    reason_prefix = "reason:" reason
    render = numeric_value("render")
    layout = numeric_value("layout")
    prepare = numeric_value("prepare")
    surface = numeric_value("surface")
    encode_present = numeric_value("encode_present")
    text_raster = numeric_value("text_raster")
    icon_resolve = numeric_value("icon_resolve")
    icon_raster = numeric_value("icon_raster")
    text_atlas_reused = numeric_value("text_atlas_reused")
    text_deferred = numeric_value("text_deferred")
    icon_deferred = numeric_value("icon_deferred")
    icon_raster_deferred = numeric_value("icon_raster_deferred")
    visible = numeric_value("visible")

    total_frames++
    frame_count["all"]++
    frame_count[prefix]++
    frame_count[reason_prefix]++
    view_seen[view] = 1
    reason_seen[reason] = 1

    warm = frame_count[prefix] > 1
    reason_warm = frame_count[reason_prefix] > 1
    add_render("all", render, total_frames > 1)
    add_render(prefix, render, warm)
    add_render(reason_prefix, render, reason_warm)

    bump_metric("all", "render", render)
    bump_metric(prefix, "render", render)
    bump_metric(reason_prefix, "render", render)
    if (total_frames > 1) {
        bump_metric("all", "warm_render", render)
    }
    if (warm) {
        bump_metric(prefix, "warm_render", render)
    }
    if (reason_warm) {
        bump_metric(reason_prefix, "warm_render", render)
    }

    for (target in targets) {
        # no-op placeholder so awk versions with strict parsers keep target local.
    }
    bump_metric("all", "layout", layout)
    bump_metric(prefix, "layout", layout)
    bump_metric(reason_prefix, "layout", layout)
    bump_metric("all", "prepare", prepare)
    bump_metric(prefix, "prepare", prepare)
    bump_metric(reason_prefix, "prepare", prepare)
    bump_metric("all", "surface", surface)
    bump_metric(prefix, "surface", surface)
    bump_metric(reason_prefix, "surface", surface)
    bump_metric("all", "encode_present", encode_present)
    bump_metric(prefix, "encode_present", encode_present)
    bump_metric(reason_prefix, "encode_present", encode_present)
    bump_metric("all", "text_raster", text_raster)
    bump_metric(prefix, "text_raster", text_raster)
    bump_metric(reason_prefix, "text_raster", text_raster)
    bump_metric("all", "text_atlas_reused", text_atlas_reused)
    bump_metric(prefix, "text_atlas_reused", text_atlas_reused)
    bump_metric(reason_prefix, "text_atlas_reused", text_atlas_reused)
    bump_metric("all", "icon_resolve", icon_resolve)
    bump_metric(prefix, "icon_resolve", icon_resolve)
    bump_metric(reason_prefix, "icon_resolve", icon_resolve)
    bump_metric("all", "icon_raster", icon_raster)
    bump_metric(prefix, "icon_raster", icon_raster)
    bump_metric(reason_prefix, "icon_raster", icon_raster)
    bump_metric("all", "text_deferred", text_deferred)
    bump_metric(prefix, "text_deferred", text_deferred)
    bump_metric(reason_prefix, "text_deferred", text_deferred)
    bump_metric("all", "icon_deferred", icon_deferred)
    bump_metric(prefix, "icon_deferred", icon_deferred)
    bump_metric(reason_prefix, "icon_deferred", icon_deferred)
    bump_metric("all", "icon_raster_deferred", icon_raster_deferred)
    bump_metric(prefix, "icon_raster_deferred", icon_raster_deferred)
    bump_metric(reason_prefix, "icon_raster_deferred", icon_raster_deferred)
    bump_metric("all", "visible", visible)
    bump_metric(prefix, "visible", visible)
    bump_metric(reason_prefix, "visible", visible)
}

/\[fika-wgpu\] prewarm-icons/ {
    view = value_of("view")
    if (view == "") {
        view = "unknown"
    }
    reason = value_of("reason")
    if (reason == "") {
        reason = "unknown"
    }
    prefix = "view:" view
    reason_prefix = "reason:" reason
    entries = numeric_value("entries")
    deferred = numeric_value("deferred")
    read_ahead = numeric_value("read_ahead")
    resolve = numeric_value("resolve")
    over_budget = numeric_value("over_budget")

    add_prewarm("all", resolve, entries, deferred, read_ahead, over_budget)
    add_prewarm(prefix, resolve, entries, deferred, read_ahead, over_budget)
    add_prewarm(reason_prefix, resolve, entries, deferred, read_ahead, over_budget)
    prewarm_view_seen[view] = 1
    prewarm_reason_seen[reason] = 1
}

/\[fika-wgpu\] prewarm-text/ {
    view = value_of("view")
    if (view == "") {
        view = "unknown"
    }
    reason = value_of("reason")
    if (reason == "") {
        reason = "unknown"
    }
    prefix = "view:" view
    reason_prefix = "reason:" reason
    entries = numeric_value("entries")
    read_ahead = numeric_value("read_ahead")
    hits = numeric_value("hits")
    misses = numeric_value("misses")
    deferred = numeric_value("deferred")
    raster = numeric_value("raster")
    over_budget = numeric_value("over_budget")

    add_text_prewarm("all", raster, entries, read_ahead, hits, misses, deferred, over_budget)
    add_text_prewarm(prefix, raster, entries, read_ahead, hits, misses, deferred, over_budget)
    add_text_prewarm(reason_prefix, raster, entries, read_ahead, hits, misses, deferred, over_budget)
    text_prewarm_view_seen[view] = 1
    text_prewarm_reason_seen[reason] = 1
}

/\[fika-wgpu\] prewarm-metadata/ {
    view = value_of("view")
    if (view == "") {
        view = "unknown"
    }
    reason = value_of("reason")
    if (reason == "") {
        reason = "unknown"
    }
    prefix = "view:" view
    reason_prefix = "reason:" reason
    visible = numeric_value("visible")
    deferred = numeric_value("deferred")
    batches = numeric_value("batches")
    results = numeric_value("results")
    applied = numeric_value("applied")

    add_metadata_prewarm("all", visible, deferred, batches, results, applied)
    add_metadata_prewarm(prefix, visible, deferred, batches, results, applied)
    add_metadata_prewarm(reason_prefix, visible, deferred, batches, results, applied)
    metadata_prewarm_view_seen[view] = 1
    metadata_prewarm_reason_seen[reason] = 1
}

/\[fika-wgpu\] prewarm-scene/ {
    view = value_of("view")
    if (view == "") {
        view = "unknown"
    }
    reason = value_of("reason")
    if (reason == "") {
        reason = "unknown"
    }
    prefix = "view:" view
    reason_prefix = "reason:" reason
    visible = numeric_value("metadata_visible")
    deferred = numeric_value("metadata_deferred")
    batches = numeric_value("metadata_batches")
    results = numeric_value("metadata_results")
    applied = numeric_value("metadata_applied")

    if (visible + deferred + batches + results + applied > 0) {
        add_metadata_prewarm("all", visible, deferred, batches, results, applied)
        add_metadata_prewarm(prefix, visible, deferred, batches, results, applied)
        add_metadata_prewarm(reason_prefix, visible, deferred, batches, results, applied)
        metadata_prewarm_view_seen[view] = 1
        metadata_prewarm_reason_seen[reason] = 1
    }
}

/\[fika-wgpu\] autosmoke-scroll/ {
    action = value_of("action")
    changed = boolean_value("changed")
    new_scroll_x = numeric_value("new_scroll_x")
    new_scroll_y = numeric_value("new_scroll_y")
    autosmoke_scroll_actions++
    if (changed > 0) {
        autosmoke_scroll_changed++
        if (action == "forward") {
            autosmoke_scroll_forward_changed++
        } else if (action == "back") {
            autosmoke_scroll_back_changed++
        }
    }
    bump_metric("autosmoke-scroll", "new_scroll_x", new_scroll_x)
    bump_metric("autosmoke-scroll", "new_scroll_y", new_scroll_y)
}

END {
    failed = 0
    if (require_frames == "true" && total_frames == 0) {
        print "wgpu-frame-gate-fail metric=frames actual=0 gate=>0" > "/dev/stderr"
        exit 1
    }
    print_summary("all", "all")
    for (view in view_seen) {
        print_summary("view:" view, view)
    }
    for (reason in reason_seen) {
        print_summary("reason:" reason, "reason:" reason)
    }
    print_prewarm_summary("all", "all")
    for (view in prewarm_view_seen) {
        print_prewarm_summary("view:" view, view)
    }
    for (reason in prewarm_reason_seen) {
        print_prewarm_summary("reason:" reason, "reason:" reason)
    }
    print_text_prewarm_summary("all", "all")
    for (view in text_prewarm_view_seen) {
        print_text_prewarm_summary("view:" view, view)
    }
    for (reason in text_prewarm_reason_seen) {
        print_text_prewarm_summary("reason:" reason, "reason:" reason)
    }
    print_metadata_prewarm_summary("all", "all")
    for (view in metadata_prewarm_view_seen) {
        print_metadata_prewarm_summary("view:" view, view)
    }
    for (reason in metadata_prewarm_reason_seen) {
        print_metadata_prewarm_summary("reason:" reason, "reason:" reason)
    }
    print_autosmoke_scroll_summary()

    if (gate_scope != "all" && frame_count[gate_scope] == 0) {
        printf("wgpu-frame-gate-fail scope=%s metric=frames actual=0 gate=>0\n", gate_scope) > "/dev/stderr"
        failed++
    }
    failed += gate_metric(max_render_us, max[gate_scope SUBSEP "render"] + 0, "render_us_max")
    failed += gate_metric(warm_max_render_us, max[gate_scope SUBSEP "warm_render"] + 0, "warm_render_us_max")
    failed += gate_metric(warm_p95_render_us, percentile(gate_scope, "warm_render", 95), "warm_render_us_p95")
    failed += gate_metric(max_text_raster_us, max[gate_scope SUBSEP "text_raster"] + 0, "text_raster_us_max")
    failed += gate_metric(max_icon_raster_us, max[gate_scope SUBSEP "icon_raster"] + 0, "icon_raster_us_max")
    failed += gate_metric(max_text_deferred, max[gate_scope SUBSEP "text_deferred"] + 0, "text_deferred_max")
    failed += gate_min_metric(min_metadata_visible, metadata_prewarm_visible_total[gate_scope] + 0, "metadata_visible_total")
    failed += gate_min_metric(min_metadata_results, metadata_prewarm_results_total[gate_scope] + 0, "metadata_results_total")
    failed += gate_min_metric(min_metadata_applied, metadata_prewarm_applied_total[gate_scope] + 0, "metadata_applied_total")
    if (require_autosmoke_scroll == "true" && autosmoke_scroll_changed == 0) {
        print "wgpu-frame-gate-fail metric=autosmoke_scroll_changed actual=0 gate=>0" > "/dev/stderr"
        failed++
    }
    if (failed > 0) {
        exit 1
    }
}
' "$input_path"
