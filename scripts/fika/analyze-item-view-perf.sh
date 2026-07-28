#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: analyze-item-view-perf.sh [OPTIONS] LOG
       FIKA_PERF_ITEM_VIEW=1 target/debug/fika ~/Downloads 2>&1 | analyze-item-view-perf.sh [OPTIONS] -

Summarizes FIKA_PERF_ITEM_VIEW item-view logs and optionally enforces perf-log
acceptance gates.

Options:
  --require-steady
      Fail if no [fika item-view] phase=steady frame is present.

  --require-details
      Fail if Details-specific visual, shape-cache, glyph-cache, and
      glyph-budget channels are missing.

  --require-warm-details-visual
      Fail if Details-specific warmed visual paint timing is missing.

  --require-static-visual
      Fail if [fika static-item-visual] Compact/Icons paint timing or static
      item shape/glyph cache/budget evidence is missing.

  --require-static-modes A,B,C
      Fail if any comma-separated view mode is absent from static visual logs.

  --require-interaction
      Fail if [fika item-interaction] hitbox timing is missing.

  --require-renderer-policy
      Fail if [fika renderer-policy] surface-count logs are missing.

  --expect-retained-item-policy
      Fail if renderer-policy logs do not show retained item visual and
      interaction surfaces, allowing only rename overlay and image-element
      fallback boundaries. Drag start and directory drop targets must be
      retained, with no per-item GPUI drag-start shell and no per-directory
      GPUI drag-move shell. Details headers must be custom-painted when
      present.

  --require-paint-slots
      Fail if [fika item-paint-slots] retained slot activity logs are missing.

  --require-autosmoke
      Fail if item-view autosmoke start/complete and scenario actions are missing.

  --require-renderer-policy-modes A,B,C
      Fail if any comma-separated view mode is absent from renderer-policy logs.

  --require-modes A,B,C
      Fail if any comma-separated view mode is absent from parsed perf logs.

  --steady-total-us N
      Fail if any item-view phase=steady total exceeds N microseconds.

  --warm-steady-total-us N
      Fail if any warmed non-empty item-view phase=steady total exceeds N
      microseconds. The first non-empty steady frame per mode is treated as
      retained-model warmup and remains reported in the all-frame phase maxima.

  --file-grid-build-us N
      Fail if any [fika file-grid] build exceeds N microseconds.

  --static-visual-paint-us N
      Fail if any [fika static-item-visual] paint exceeds N microseconds.

  --image-paint-us N
      Fail if any [fika item-image] paint exceeds N microseconds.

  --custom-paint-us N
      Fail if any custom paint channel exceeds N microseconds.

  --warm-static-visual-paint-us N
      Fail if any warmed [fika static-item-visual] paint exceeds N microseconds.
      The first non-zero paint per mode is treated as cold glyph/image-raster
      warmup and remains reported in the all-frame maxima.

  --warm-image-paint-us N
      Fail if any warmed [fika item-image] paint exceeds N microseconds.

  --warm-custom-paint-us N
      Fail if any warmed custom paint channel exceeds N microseconds.

  -h, --help
      Show this help.
EOF
}

require_steady=false
require_details=false
require_warm_details_visual=false
require_static_visual=false
require_interaction=false
require_renderer_policy=false
expect_retained_item_policy=false
require_paint_slots=false
require_autosmoke=false
required_modes=""
required_static_modes=""
required_renderer_policy_modes=""
steady_total_us=""
warm_steady_total_us=""
file_grid_build_us=""
static_visual_paint_us=""
image_paint_us=""
custom_paint_us=""
warm_static_visual_paint_us=""
warm_image_paint_us=""
warm_custom_paint_us=""
log_path=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --require-steady)
            require_steady=true
            ;;
        --require-details)
            require_details=true
            ;;
        --require-warm-details-visual)
            require_warm_details_visual=true
            ;;
        --require-static-visual)
            require_static_visual=true
            ;;
        --require-static-modes)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--require-static-modes requires a comma-separated value" >&2
                usage >&2
                exit 2
            fi
            required_static_modes="$2"
            shift
            ;;
        --require-static-modes=*)
            required_static_modes="${1#--require-static-modes=}"
            ;;
        --require-interaction)
            require_interaction=true
            ;;
        --require-renderer-policy)
            require_renderer_policy=true
            ;;
        --expect-retained-item-policy)
            expect_retained_item_policy=true
            ;;
        --require-paint-slots)
            require_paint_slots=true
            ;;
        --require-autosmoke)
            require_autosmoke=true
            ;;
        --require-renderer-policy-modes)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--require-renderer-policy-modes requires a comma-separated value" >&2
                usage >&2
                exit 2
            fi
            required_renderer_policy_modes="$2"
            shift
            ;;
        --require-renderer-policy-modes=*)
            required_renderer_policy_modes="${1#--require-renderer-policy-modes=}"
            ;;
        --require-modes)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--require-modes requires a comma-separated value" >&2
                usage >&2
                exit 2
            fi
            required_modes="$2"
            shift
            ;;
        --require-modes=*)
            required_modes="${1#--require-modes=}"
            ;;
        --steady-total-us)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--steady-total-us requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            steady_total_us="$2"
            shift
            ;;
        --steady-total-us=*)
            steady_total_us="${1#--steady-total-us=}"
            ;;
        --warm-steady-total-us)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--warm-steady-total-us requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            warm_steady_total_us="$2"
            shift
            ;;
        --warm-steady-total-us=*)
            warm_steady_total_us="${1#--warm-steady-total-us=}"
            ;;
        --file-grid-build-us)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--file-grid-build-us requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            file_grid_build_us="$2"
            shift
            ;;
        --file-grid-build-us=*)
            file_grid_build_us="${1#--file-grid-build-us=}"
            ;;
        --static-visual-paint-us)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--static-visual-paint-us requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            static_visual_paint_us="$2"
            shift
            ;;
        --static-visual-paint-us=*)
            static_visual_paint_us="${1#--static-visual-paint-us=}"
            ;;
        --image-paint-us)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--image-paint-us requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            image_paint_us="$2"
            shift
            ;;
        --image-paint-us=*)
            image_paint_us="${1#--image-paint-us=}"
            ;;
        --custom-paint-us)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--custom-paint-us requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            custom_paint_us="$2"
            shift
            ;;
        --custom-paint-us=*)
            custom_paint_us="${1#--custom-paint-us=}"
            ;;
        --warm-static-visual-paint-us)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--warm-static-visual-paint-us requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            warm_static_visual_paint_us="$2"
            shift
            ;;
        --warm-static-visual-paint-us=*)
            warm_static_visual_paint_us="${1#--warm-static-visual-paint-us=}"
            ;;
        --warm-image-paint-us)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--warm-image-paint-us requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            warm_image_paint_us="$2"
            shift
            ;;
        --warm-image-paint-us=*)
            warm_image_paint_us="${1#--warm-image-paint-us=}"
            ;;
        --warm-custom-paint-us)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--warm-custom-paint-us requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            warm_custom_paint_us="$2"
            shift
            ;;
        --warm-custom-paint-us=*)
            warm_custom_paint_us="${1#--warm-custom-paint-us=}"
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        --*)
            echo "unknown argument: $1" >&2
            usage >&2
            exit 2
            ;;
        *)
            if [[ -n "$log_path" ]]; then
                echo "only one LOG path is supported" >&2
                usage >&2
                exit 2
            fi
            log_path="$1"
            ;;
    esac
    shift
done

if [[ -z "$log_path" ]]; then
    echo "LOG path is required; use - for stdin" >&2
    usage >&2
    exit 2
fi

if [[ -n "$steady_total_us" && ! "$steady_total_us" =~ ^[0-9]+$ ]]; then
    echo "--steady-total-us must be an integer microsecond value" >&2
    exit 2
fi

if [[ -n "$warm_steady_total_us" && ! "$warm_steady_total_us" =~ ^[0-9]+$ ]]; then
    echo "--warm-steady-total-us must be an integer microsecond value" >&2
    exit 2
fi

if [[ -n "$file_grid_build_us" && ! "$file_grid_build_us" =~ ^[0-9]+$ ]]; then
    echo "--file-grid-build-us must be an integer microsecond value" >&2
    exit 2
fi

if [[ -n "$static_visual_paint_us" && ! "$static_visual_paint_us" =~ ^[0-9]+$ ]]; then
    echo "--static-visual-paint-us must be an integer microsecond value" >&2
    exit 2
fi

if [[ -n "$image_paint_us" && ! "$image_paint_us" =~ ^[0-9]+$ ]]; then
    echo "--image-paint-us must be an integer microsecond value" >&2
    exit 2
fi

if [[ -n "$custom_paint_us" && ! "$custom_paint_us" =~ ^[0-9]+$ ]]; then
    echo "--custom-paint-us must be an integer microsecond value" >&2
    exit 2
fi

if [[ -n "$warm_static_visual_paint_us" && ! "$warm_static_visual_paint_us" =~ ^[0-9]+$ ]]; then
    echo "--warm-static-visual-paint-us must be an integer microsecond value" >&2
    exit 2
fi

if [[ -n "$warm_image_paint_us" && ! "$warm_image_paint_us" =~ ^[0-9]+$ ]]; then
    echo "--warm-image-paint-us must be an integer microsecond value" >&2
    exit 2
fi

if [[ -n "$warm_custom_paint_us" && ! "$warm_custom_paint_us" =~ ^[0-9]+$ ]]; then
    echo "--warm-custom-paint-us must be an integer microsecond value" >&2
    exit 2
fi

awk \
    -v require_steady="$require_steady" \
    -v require_details="$require_details" \
    -v require_warm_details_visual="$require_warm_details_visual" \
    -v require_static_visual="$require_static_visual" \
    -v require_interaction="$require_interaction" \
    -v require_renderer_policy="$require_renderer_policy" \
    -v expect_retained_item_policy="$expect_retained_item_policy" \
    -v require_paint_slots="$require_paint_slots" \
    -v require_autosmoke="$require_autosmoke" \
    -v required_modes="$required_modes" \
    -v required_static_modes="$required_static_modes" \
    -v required_renderer_policy_modes="$required_renderer_policy_modes" \
    -v steady_total_limit="$steady_total_us" \
    -v warm_steady_total_limit="$warm_steady_total_us" \
    -v file_grid_build_limit="$file_grid_build_us" \
    -v static_visual_paint_limit="$static_visual_paint_us" \
    -v image_paint_limit="$image_paint_us" \
    -v custom_paint_limit="$custom_paint_us" \
    -v warm_static_visual_paint_limit="$warm_static_visual_paint_us" \
    -v warm_image_paint_limit="$warm_image_paint_us" \
    -v warm_custom_paint_limit="$warm_custom_paint_us" '
function trim(value) {
    sub(/^[[:space:]]+/, "", value)
    sub(/[[:space:]]+$/, "", value)
    return value
}

function field(name,    prefix, i, value) {
    prefix = name "="
    for (i = 1; i <= NF; i++) {
        if (index($i, prefix) == 1) {
            value = substr($i, length(prefix) + 1)
            sub(/,$/, "", value)
            return value
        }
    }
    return ""
}

function us_field(name,    value) {
    value = field(name)
    sub(/us$/, "", value)
    return value + 0
}

function required_us_field(name, log_name,    value) {
    value = field(name)
    if (value == "") {
        fail("missing " name "= in " log_name " line")
        return 0
    }
    sub(/us$/, "", value)
    return value + 0
}

function note_mode(mode) {
    if (mode != "") {
        modes[mode] = 1
    }
}

function max_assign(array, key, value) {
    if (!(key in array) || value > array[key]) {
        array[key] = value
    }
}

function fail(message) {
    print "fail: " message > "/dev/stderr"
    failures++
}

function reset_process_warm_state(    key) {
    for (key in painted_once) {
        delete painted_once[key]
    }
    for (key in steady_once) {
        delete steady_once[key]
    }
}

function record_custom_paint(channel, mode, prepaint, paint,    key, warm) {
    custom_paint_count++
    max_assign(single_max, "custom_paint_prepaint", prepaint)
    max_assign(single_max, "custom_paint_paint", paint)
    if (custom_paint_limit != "" && paint > custom_paint_limit + 0) {
        custom_paint_over_limit++
    }
    key = channel SUBSEP mode
    warm = paint > 0 && painted_once[key] > 0
    if (paint > 0) {
        painted_once[key]++
    }
    if (!warm) {
        return
    }

    warm_custom_paint_count++
    max_assign(single_max, "warm_custom_paint_paint", paint)
    if (warm_custom_paint_limit != "" && paint > warm_custom_paint_limit + 0) {
        warm_custom_paint_over_limit++
    }
    if (channel == "static") {
        warm_static_visual_count++
        max_assign(single_max, "warm_static_visual_paint", paint)
        if (warm_static_visual_paint_limit != "" && paint > warm_static_visual_paint_limit + 0) {
            warm_static_visual_over_limit++
        }
    } else if (channel == "image") {
        warm_image_count++
        max_assign(single_max, "warm_image_paint", paint)
        if (warm_image_paint_limit != "" && paint > warm_image_paint_limit + 0) {
            warm_image_over_limit++
        }
    } else if (channel == "details") {
        warm_details_visual_count++
        max_assign(single_max, "warm_details_visual_paint", paint)
    }
}

function check_renderer_policy_count(name, items, value, mode) {
    if (value < 0) {
        fail("renderer-policy " name " is negative in mode " mode)
    }
    if (value > items) {
        fail("renderer-policy " name " exceeds items in mode " mode)
    }
}

function parse_required_list(list, target, label,    count, i, value) {
    if (list == "") {
        return
    }
    count = split(list, values, ",")
    for (i = 1; i <= count; i++) {
        value = trim(values[i])
        if (value == "") {
            fail("empty mode in " label)
        } else {
            target[value] = 1
        }
    }
}

BEGIN {
    parse_required_list(required_modes, required_mode, "--require-modes")
    parse_required_list(required_static_modes, required_static_mode, "--require-static-modes")
    parse_required_list(required_renderer_policy_modes, required_renderer_policy_mode, "--require-renderer-policy-modes")
}

/^\[fika analyzer\] log-boundary / {
    reset_process_warm_state()
}

/^\[fika item-view\]/ {
    item_view_count++
    mode = field("mode")
    phase = field("phase")
    if (phase == "") {
        phase = "unknown"
    }
    total = required_us_field("total", "[fika item-view]")
    raw = required_us_field("raw", "[fika item-view]")
    icon_sync = required_us_field("icon_sync", "[fika item-view]")
    queue = required_us_field("queue", "[fika item-view]")
    convert = required_us_field("convert", "[fika item-view]")
    visible = field("visible") + 0
    note_mode(mode)
    phase_count[phase]++
    max_assign(phase_max_total, phase, total)
    max_assign(phase_max_visible, phase, visible)
    max_assign(single_max, "item_view_raw", raw)
    max_assign(single_max, "item_view_icon_sync", icon_sync)
    max_assign(single_max, "item_view_queue", queue)
    max_assign(single_max, "item_view_convert", convert)
    if (phase == "steady") {
        steady_count++
        if (steady_total_limit != "" && total > steady_total_limit + 0) {
            steady_over_limit++
        }
        if (visible > 0) {
            steady_key = mode
            if (steady_once[steady_key] > 0) {
                warm_steady_count++
                max_assign(single_max, "warm_steady_total", total)
                if (warm_steady_total_limit != "" && total > warm_steady_total_limit + 0) {
                    warm_steady_over_limit++
                }
            }
            steady_once[steady_key]++
        }
    }
}

/^\[fika file-grid\]/ {
    file_grid_count++
    note_mode(field("mode"))
    build = us_field("build")
    max_assign(single_max, "file_grid_build", build)
    if (file_grid_build_limit != "" && build > file_grid_build_limit + 0) {
        file_grid_over_limit++
    }
}

/^\[fika icon-sync\]/ {
    icon_sync_count++
    note_mode(field("mode"))
    max_assign(single_max, "icon_sync_candidates", field("candidates") + 0)
    max_assign(single_max, "icon_sync_cached", field("cached") + 0)
    max_assign(single_max, "icon_sync_queued", field("queued") + 0)
    max_assign(single_max, "icon_sync_resolved", field("resolved") + 0)
    max_assign(single_max, "icon_sync_changed", field("changed") + 0)
    max_assign(single_max, "icon_sync_total", us_field("total"))
    if (field("budget_exhausted") == "true") {
        icon_sync_budget_exhausted++
    }
}

/^\[fika item-image-cache-refresh\]/ {
    image_cache_refresh_count++
    note_mode(field("mode"))
    image_cache_refresh_requested += field("requested") + 0
    image_cache_refresh_retained += field("retained") + 0
    image_cache_refresh_loaded += field("loaded") + 0
    image_cache_refresh_decoded += field("decoded") + 0
    image_cache_refresh_missing += field("missing") + 0
    image_cache_refresh_non_svg += field("non_svg") + 0
    image_cache_refresh_evicted += field("evicted") + 0
    max_assign(single_max, "image_cache_refresh_requested", field("requested") + 0)
    max_assign(single_max, "image_cache_refresh_retained", field("retained") + 0)
    max_assign(single_max, "image_cache_refresh_loaded", field("loaded") + 0)
    max_assign(single_max, "image_cache_refresh_decoded", field("decoded") + 0)
    max_assign(single_max, "image_cache_refresh_missing", field("missing") + 0)
    max_assign(single_max, "image_cache_refresh_non_svg", field("non_svg") + 0)
    max_assign(single_max, "image_cache_refresh_entries", field("entries") + 0)
    max_assign(single_max, "image_cache_refresh_bytes", field("bytes") + 0)
    max_assign(single_max, "image_cache_refresh_evicted", field("evicted") + 0)
    max_assign(single_max, "image_cache_refresh_total", us_field("total"))
}

/^\[fika static-item-visual\]/ {
    static_visual_count++
    mode = field("mode")
    note_mode(mode)
    if (mode != "") {
        static_visual_modes[mode] = 1
    }
    paint = us_field("paint")
    prepaint = us_field("prepaint")
    max_assign(single_max, "static_visual_prepaint", prepaint)
    max_assign(single_max, "static_visual_paint", paint)
    record_custom_paint("static", mode, prepaint, paint)
    if (static_visual_paint_limit != "" && paint > static_visual_paint_limit + 0) {
        static_visual_over_limit++
    }
}

/^\[fika item-shape-cache\]/ {
    item_shape_count++
    note_mode(field("mode"))
    item_shape_hits += field("hits") + 0
    item_shape_misses += field("misses") + 0
    item_shape_evicted += field("evicted") + 0
    max_assign(single_max, "item_shape_compute", us_field("compute"))
    max_assign(single_max, "item_shape_entries", field("entries") + 0)
}

/^\[fika item-glyph-cache\]/ {
    item_glyph_count++
    note_mode(field("mode"))
    item_glyph_hits += field("hits") + 0
    item_glyph_misses += field("misses") + 0
    item_glyph_evicted += field("evicted") + 0
    max_assign(single_max, "item_glyph_entries", field("entries") + 0)
}

/^\[fika item-glyph-budget\]/ {
    item_glyph_budget_count++
    note_mode(field("mode"))
    item_glyph_budget_requested += field("requested") + 0
    item_glyph_budget_hits += field("hits") + 0
    item_glyph_budget_misses += field("misses") + 0
    item_glyph_budget_computed += field("computed") + 0
    item_glyph_budget_deferred += field("deferred") + 0
    item_glyph_budget_failed += field("failed") + 0
    max_assign(single_max, "item_glyph_budget_requested", field("requested") + 0)
    max_assign(single_max, "item_glyph_budget_misses", field("misses") + 0)
    max_assign(single_max, "item_glyph_budget_computed", field("computed") + 0)
    max_assign(single_max, "item_glyph_budget_deferred", field("deferred") + 0)
    max_assign(single_max, "item_glyph_budget_compute", us_field("compute"))
    if (field("budget_exhausted") == "true") {
        item_glyph_budget_exhausted++
    }
}

/^\[fika item-image\]/ {
    image_count++
    note_mode(field("mode"))
    paint = us_field("paint")
    prepaint = us_field("prepaint")
    image_theme_loaded += field("theme_loaded") + 0
    image_theme_decoded += field("theme_decoded") + 0
    image_theme_retained += field("theme_retained") + 0
    image_theme_placeholder += field("theme_placeholder") + 0
    image_thumb_loaded += field("thumb_loaded") + 0
    image_thumb_decoded += field("thumb_decoded") + 0
    image_thumb_retained += field("thumb_retained") + 0
    image_thumb_fallback += field("thumb_fallback") + 0
    max_assign(single_max, "image_prepaint", prepaint)
    max_assign(single_max, "image_paint", paint)
    record_custom_paint("image", field("mode"), prepaint, paint)
    if (image_paint_limit != "" && paint > image_paint_limit + 0) {
        image_over_limit++
    }
}

/^\[fika details-visual\]/ {
    details_visual_count++
    note_mode(field("mode"))
    prepaint = us_field("prepaint")
    paint = us_field("paint")
    max_assign(single_max, "details_visual_prepaint", prepaint)
    max_assign(single_max, "details_visual_paint", paint)
    record_custom_paint("details", field("mode"), prepaint, paint)
}

/^\[fika details-shape-cache\]/ {
    details_shape_count++
    note_mode(field("mode"))
    details_shape_hits += field("hits") + 0
    details_shape_misses += field("misses") + 0
    details_shape_evicted += field("evicted") + 0
    max_assign(single_max, "details_shape_compute", us_field("compute"))
    max_assign(single_max, "details_shape_entries", field("entries") + 0)
}

/^\[fika details-glyph-cache\]/ {
    details_glyph_count++
    note_mode(field("mode"))
    details_glyph_hits += field("hits") + 0
    details_glyph_misses += field("misses") + 0
    details_glyph_evicted += field("evicted") + 0
    max_assign(single_max, "details_glyph_entries", field("entries") + 0)
}

/^\[fika details-glyph-budget\]/ {
    details_glyph_budget_count++
    note_mode(field("mode"))
    details_glyph_budget_requested += field("requested") + 0
    details_glyph_budget_hits += field("hits") + 0
    details_glyph_budget_misses += field("misses") + 0
    details_glyph_budget_computed += field("computed") + 0
    details_glyph_budget_deferred += field("deferred") + 0
    details_glyph_budget_failed += field("failed") + 0
    max_assign(single_max, "details_glyph_budget_requested", field("requested") + 0)
    max_assign(single_max, "details_glyph_budget_misses", field("misses") + 0)
    max_assign(single_max, "details_glyph_budget_computed", field("computed") + 0)
    max_assign(single_max, "details_glyph_budget_deferred", field("deferred") + 0)
    max_assign(single_max, "details_glyph_budget_compute", us_field("compute"))
    if (field("budget_exhausted") == "true") {
        details_glyph_budget_exhausted++
    }
}

/^\[fika item-interaction\]/ {
    item_interaction_count++
    note_mode(field("mode"))
    max_assign(single_max, "interaction_prepaint", us_field("prepaint"))
    max_assign(single_max, "interaction_paint", us_field("paint"))
    max_assign(single_max, "interaction_prepaint_count", field("prepaint_count") + 0)
    max_assign(single_max, "interaction_paint_count", field("paint_count") + 0)
}

/^\[fika item-paint-slots\]/ {
    item_paint_slot_count++
    note_mode(field("mode"))
    max_assign(single_max, "paint_slot_inserted", field("inserted") + 0)
    max_assign(single_max, "paint_slot_content", field("content") + 0)
    max_assign(single_max, "paint_slot_geometry", field("geometry") + 0)
    max_assign(single_max, "paint_slot_visual", field("visual") + 0)
    max_assign(single_max, "paint_slot_unchanged", field("unchanged") + 0)
    max_assign(single_max, "paint_slot_removed", field("removed") + 0)
    max_assign(single_max, "paint_slot_entries", field("entries") + 0)
}

/^\[fika renderer-policy\]/ {
    renderer_policy_count++
    mode = field("mode")
    items = field("items") + 0
    visual_layer = field("visual_layer") + 0
    image_layer = field("image_layer") + 0
    gpui_image_element = field("gpui_image_element") + 0
    retained_interaction = field("retained_interaction") + 0
    retained_directory_drop_target = field("retained_directory_drop_target") + 0
    gpui_drag_shell = field("gpui_drag_shell") + 0
    gpui_directory_drop_shell = field("gpui_directory_drop_shell") + 0
    details_header_visual_layer = field("details_header_visual_layer") + 0
    gpui_details_header = field("gpui_details_header") + 0
    rename_overlay = field("rename_overlay") + 0
    note_mode(mode)
    if (mode != "") {
        renderer_policy_modes[mode] = 1
    }
    if (items < 0) {
        fail("renderer-policy items is negative in mode " mode)
    }
    check_renderer_policy_count("visual_layer", items, visual_layer, mode)
    check_renderer_policy_count("image_layer", items, image_layer, mode)
    check_renderer_policy_count("gpui_image_element", items, gpui_image_element, mode)
    check_renderer_policy_count("retained_interaction", items, retained_interaction, mode)
    check_renderer_policy_count("retained_directory_drop_target", items, retained_directory_drop_target, mode)
    check_renderer_policy_count("gpui_drag_shell", items, gpui_drag_shell, mode)
    check_renderer_policy_count("gpui_directory_drop_shell", items, gpui_directory_drop_shell, mode)
    check_renderer_policy_count("rename_overlay", items, rename_overlay, mode)
    if (details_header_visual_layer < 0 || gpui_details_header < 0) {
        fail("renderer-policy details header count is negative in mode " mode)
    }
    if (expect_retained_item_policy == "true") {
        if (visual_layer != items ||
            retained_interaction + rename_overlay != items ||
            gpui_drag_shell != 0 ||
            gpui_directory_drop_shell != 0 ||
            gpui_details_header != 0 ||
            image_layer + gpui_image_element > items) {
            retained_item_policy_invalid = 1
        }
    }
    max_assign(single_max, "renderer_policy_items", items)
    max_assign(single_max, "renderer_policy_visual_layer", visual_layer)
    max_assign(single_max, "renderer_policy_image_layer", image_layer)
    max_assign(single_max, "renderer_policy_gpui_image_element", gpui_image_element)
    max_assign(single_max, "renderer_policy_retained_interaction", retained_interaction)
    max_assign(single_max, "renderer_policy_retained_directory_drop_target", retained_directory_drop_target)
    max_assign(single_max, "renderer_policy_gpui_drag_shell", gpui_drag_shell)
    max_assign(single_max, "renderer_policy_gpui_directory_drop_shell", gpui_directory_drop_shell)
    max_assign(single_max, "renderer_policy_details_header_visual_layer", details_header_visual_layer)
    max_assign(single_max, "renderer_policy_gpui_details_header", gpui_details_header)
    max_assign(single_max, "renderer_policy_rename_overlay", rename_overlay)
}

/^\[fika autosmoke\] item-view start / {
    autosmoke_start_seen++
    scenario = field("scenario")
    if (scenario != "") {
        if (autosmoke_start_scenario != "" && autosmoke_start_scenario != scenario) {
            autosmoke_scenario_conflict = 1
        }
        autosmoke_start_scenario = scenario
    }
}

/^\[fika autosmoke\] item-view complete / {
    autosmoke_complete_seen++
    scenario = field("scenario")
    if (scenario != "") {
        if (autosmoke_complete_scenario != "" && autosmoke_complete_scenario != scenario) {
            autosmoke_scenario_conflict = 1
        }
        autosmoke_complete_scenario = scenario
    }
}

/^\[fika autosmoke\] item-view action=/ {
    action = field("action")
    changed = field("changed")
    if (action == "zoom-in") {
        autosmoke_zoom_in_seen++
    } else if (action == "zoom-out") {
        autosmoke_zoom_out_seen++
    } else if (action == "scroll-forward" && changed == "true") {
        autosmoke_scroll_forward_seen++
    } else if (action == "scroll-back" && changed == "true") {
        autosmoke_scroll_back_seen++
    } else if (action == "view-details") {
        autosmoke_view_details_seen++
    }
}

END {
    print "Item view perf summary"
    print "  item_view_frames: " item_view_count
    for (i = 1; i <= split("initial mode-switch content-change geometry-change visual-change steady unknown", ordered, " "); i++) {
        phase = ordered[i]
        if (phase in phase_count) {
            printf "  phase %-15s frames=%d max_total=%dus max_visible=%d\n",
                phase, phase_count[phase], phase_max_total[phase], phase_max_visible[phase]
        }
    }

    print "  item_view_stage_max:" \
        " raw=" (("item_view_raw" in single_max) ? single_max["item_view_raw"] : 0) "us" \
        " icon_sync=" (("item_view_icon_sync" in single_max) ? single_max["item_view_icon_sync"] : 0) "us" \
        " queue=" (("item_view_queue" in single_max) ? single_max["item_view_queue"] : 0) "us" \
        " convert=" (("item_view_convert" in single_max) ? single_max["item_view_convert"] : 0) "us"
    print "  warm_steady_frames: " (warm_steady_count + 0) \
        " max_total=" (("warm_steady_total" in single_max) ? single_max["warm_steady_total"] : 0) "us"
    print "  icon_sync_frames: " (icon_sync_count + 0) \
        " max_candidates=" (("icon_sync_candidates" in single_max) ? single_max["icon_sync_candidates"] : 0) \
        " max_cached=" (("icon_sync_cached" in single_max) ? single_max["icon_sync_cached"] : 0) \
        " max_queued=" (("icon_sync_queued" in single_max) ? single_max["icon_sync_queued"] : 0) \
        " max_resolved=" (("icon_sync_resolved" in single_max) ? single_max["icon_sync_resolved"] : 0) \
        " max_changed=" (("icon_sync_changed" in single_max) ? single_max["icon_sync_changed"] : 0) \
        " budget_exhausted=" (icon_sync_budget_exhausted + 0) \
        " max_total=" (("icon_sync_total" in single_max) ? single_max["icon_sync_total"] : 0) "us"
    modes_text = ""
    for (mode in modes) {
        modes_text = modes_text (modes_text == "" ? "" : ",") mode
    }
    print "  modes: " (modes_text == "" ? "<none>" : modes_text)
    print "  file_grid_frames: " (file_grid_count + 0) " max_build=" (("file_grid_build" in single_max) ? single_max["file_grid_build"] : 0) "us"
    print "  static_visual_frames: " (static_visual_count + 0) \
        " max_prepaint=" (("static_visual_prepaint" in single_max) ? single_max["static_visual_prepaint"] : 0) "us" \
        " max_paint=" (("static_visual_paint" in single_max) ? single_max["static_visual_paint"] : 0) "us"
    print "  item_shape_frames: " (item_shape_count + 0) \
        " hits=" (item_shape_hits + 0) \
        " misses=" (item_shape_misses + 0) \
        " evicted=" (item_shape_evicted + 0) \
        " max_compute=" (("item_shape_compute" in single_max) ? single_max["item_shape_compute"] : 0) "us" \
        " max_entries=" (("item_shape_entries" in single_max) ? single_max["item_shape_entries"] : 0)
    print "  item_glyph_frames: " (item_glyph_count + 0) \
        " hits=" (item_glyph_hits + 0) \
        " misses=" (item_glyph_misses + 0) \
        " evicted=" (item_glyph_evicted + 0) \
        " max_entries=" (("item_glyph_entries" in single_max) ? single_max["item_glyph_entries"] : 0)
    print "  item_glyph_budget_frames: " (item_glyph_budget_count + 0) \
        " requested=" (item_glyph_budget_requested + 0) \
        " hits=" (item_glyph_budget_hits + 0) \
        " misses=" (item_glyph_budget_misses + 0) \
        " computed=" (item_glyph_budget_computed + 0) \
        " deferred=" (item_glyph_budget_deferred + 0) \
        " failed=" (item_glyph_budget_failed + 0) \
        " budget_exhausted=" (item_glyph_budget_exhausted + 0) \
        " max_requested=" (("item_glyph_budget_requested" in single_max) ? single_max["item_glyph_budget_requested"] : 0) \
        " max_misses=" (("item_glyph_budget_misses" in single_max) ? single_max["item_glyph_budget_misses"] : 0) \
        " max_computed=" (("item_glyph_budget_computed" in single_max) ? single_max["item_glyph_budget_computed"] : 0) \
        " max_deferred=" (("item_glyph_budget_deferred" in single_max) ? single_max["item_glyph_budget_deferred"] : 0) \
        " max_compute=" (("item_glyph_budget_compute" in single_max) ? single_max["item_glyph_budget_compute"] : 0) "us"
    print "  warm_static_visual_frames: " (warm_static_visual_count + 0) \
        " max_paint=" (("warm_static_visual_paint" in single_max) ? single_max["warm_static_visual_paint"] : 0) "us"
    print "  image_frames: " (image_count + 0) \
        " max_prepaint=" (("image_prepaint" in single_max) ? single_max["image_prepaint"] : 0) "us" \
        " max_paint=" (("image_paint" in single_max) ? single_max["image_paint"] : 0) "us"
    print "  warm_image_frames: " (warm_image_count + 0) \
        " max_paint=" (("warm_image_paint" in single_max) ? single_max["warm_image_paint"] : 0) "us"
    print "  image_cache_refresh_frames: " (image_cache_refresh_count + 0) \
        " requested=" (image_cache_refresh_requested + 0) \
        " retained=" (image_cache_refresh_retained + 0) \
        " loaded=" (image_cache_refresh_loaded + 0) \
        " decoded=" (image_cache_refresh_decoded + 0) \
        " missing=" (image_cache_refresh_missing + 0) \
        " non_svg=" (image_cache_refresh_non_svg + 0) \
        " max_requested=" (("image_cache_refresh_requested" in single_max) ? single_max["image_cache_refresh_requested"] : 0) \
        " max_retained=" (("image_cache_refresh_retained" in single_max) ? single_max["image_cache_refresh_retained"] : 0) \
        " max_loaded=" (("image_cache_refresh_loaded" in single_max) ? single_max["image_cache_refresh_loaded"] : 0) \
        " max_decoded=" (("image_cache_refresh_decoded" in single_max) ? single_max["image_cache_refresh_decoded"] : 0) \
        " max_missing=" (("image_cache_refresh_missing" in single_max) ? single_max["image_cache_refresh_missing"] : 0) \
        " max_non_svg=" (("image_cache_refresh_non_svg" in single_max) ? single_max["image_cache_refresh_non_svg"] : 0) \
        " evicted=" (image_cache_refresh_evicted + 0) \
        " max_entries=" (("image_cache_refresh_entries" in single_max) ? single_max["image_cache_refresh_entries"] : 0) \
        " max_bytes=" (("image_cache_refresh_bytes" in single_max) ? single_max["image_cache_refresh_bytes"] : 0) \
        " max_evicted=" (("image_cache_refresh_evicted" in single_max) ? single_max["image_cache_refresh_evicted"] : 0) \
        " max_total=" (("image_cache_refresh_total" in single_max) ? single_max["image_cache_refresh_total"] : 0) "us"
    print "  image_sources: theme_loaded=" (image_theme_loaded + 0) \
        " theme_decoded=" (image_theme_decoded + 0) \
        " theme_retained=" (image_theme_retained + 0) \
        " theme_placeholder=" (image_theme_placeholder + 0) \
        " thumb_loaded=" (image_thumb_loaded + 0) \
        " thumb_decoded=" (image_thumb_decoded + 0) \
        " thumb_retained=" (image_thumb_retained + 0) \
        " thumb_fallback=" (image_thumb_fallback + 0)
    print "  custom_paint_frames: " (custom_paint_count + 0) \
        " max_prepaint=" (("custom_paint_prepaint" in single_max) ? single_max["custom_paint_prepaint"] : 0) "us" \
        " max_paint=" (("custom_paint_paint" in single_max) ? single_max["custom_paint_paint"] : 0) "us"
    print "  warm_custom_paint_frames: " (warm_custom_paint_count + 0) \
        " max_paint=" (("warm_custom_paint_paint" in single_max) ? single_max["warm_custom_paint_paint"] : 0) "us"
    print "  details_visual_frames: " (details_visual_count + 0) \
        " max_prepaint=" (("details_visual_prepaint" in single_max) ? single_max["details_visual_prepaint"] : 0) "us" \
        " max_paint=" (("details_visual_paint" in single_max) ? single_max["details_visual_paint"] : 0) "us"
    print "  warm_details_visual_frames: " (warm_details_visual_count + 0) \
        " max_paint=" (("warm_details_visual_paint" in single_max) ? single_max["warm_details_visual_paint"] : 0) "us"
    print "  details_shape_frames: " (details_shape_count + 0) \
        " hits=" (details_shape_hits + 0) \
        " misses=" (details_shape_misses + 0) \
        " evicted=" (details_shape_evicted + 0) \
        " max_compute=" (("details_shape_compute" in single_max) ? single_max["details_shape_compute"] : 0) "us" \
        " max_entries=" (("details_shape_entries" in single_max) ? single_max["details_shape_entries"] : 0)
    print "  details_glyph_frames: " (details_glyph_count + 0) \
        " hits=" (details_glyph_hits + 0) \
        " misses=" (details_glyph_misses + 0) \
        " evicted=" (details_glyph_evicted + 0) \
        " max_entries=" (("details_glyph_entries" in single_max) ? single_max["details_glyph_entries"] : 0)
    print "  details_glyph_budget_frames: " (details_glyph_budget_count + 0) \
        " requested=" (details_glyph_budget_requested + 0) \
        " hits=" (details_glyph_budget_hits + 0) \
        " misses=" (details_glyph_budget_misses + 0) \
        " computed=" (details_glyph_budget_computed + 0) \
        " deferred=" (details_glyph_budget_deferred + 0) \
        " failed=" (details_glyph_budget_failed + 0) \
        " budget_exhausted=" (details_glyph_budget_exhausted + 0) \
        " max_requested=" (("details_glyph_budget_requested" in single_max) ? single_max["details_glyph_budget_requested"] : 0) \
        " max_misses=" (("details_glyph_budget_misses" in single_max) ? single_max["details_glyph_budget_misses"] : 0) \
        " max_computed=" (("details_glyph_budget_computed" in single_max) ? single_max["details_glyph_budget_computed"] : 0) \
        " max_deferred=" (("details_glyph_budget_deferred" in single_max) ? single_max["details_glyph_budget_deferred"] : 0) \
        " max_compute=" (("details_glyph_budget_compute" in single_max) ? single_max["details_glyph_budget_compute"] : 0) "us"
    print "  interaction_frames: " (item_interaction_count + 0) \
        " max_prepaint_count=" (("interaction_prepaint_count" in single_max) ? single_max["interaction_prepaint_count"] : 0) \
        " max_paint_count=" (("interaction_paint_count" in single_max) ? single_max["interaction_paint_count"] : 0)
    print "  item_paint_slots_frames: " (item_paint_slot_count + 0) \
        " max_inserted=" (("paint_slot_inserted" in single_max) ? single_max["paint_slot_inserted"] : 0) \
        " max_content=" (("paint_slot_content" in single_max) ? single_max["paint_slot_content"] : 0) \
        " max_geometry=" (("paint_slot_geometry" in single_max) ? single_max["paint_slot_geometry"] : 0) \
        " max_visual=" (("paint_slot_visual" in single_max) ? single_max["paint_slot_visual"] : 0) \
        " max_unchanged=" (("paint_slot_unchanged" in single_max) ? single_max["paint_slot_unchanged"] : 0) \
        " max_removed=" (("paint_slot_removed" in single_max) ? single_max["paint_slot_removed"] : 0) \
        " max_entries=" (("paint_slot_entries" in single_max) ? single_max["paint_slot_entries"] : 0)
    print "  renderer_policy_frames: " (renderer_policy_count + 0) \
        " max_items=" (("renderer_policy_items" in single_max) ? single_max["renderer_policy_items"] : 0) \
        " max_visual_layer=" (("renderer_policy_visual_layer" in single_max) ? single_max["renderer_policy_visual_layer"] : 0) \
        " max_image_layer=" (("renderer_policy_image_layer" in single_max) ? single_max["renderer_policy_image_layer"] : 0) \
        " max_gpui_image_element=" (("renderer_policy_gpui_image_element" in single_max) ? single_max["renderer_policy_gpui_image_element"] : 0) \
        " max_retained_interaction=" (("renderer_policy_retained_interaction" in single_max) ? single_max["renderer_policy_retained_interaction"] : 0) \
        " max_retained_directory_drop_target=" (("renderer_policy_retained_directory_drop_target" in single_max) ? single_max["renderer_policy_retained_directory_drop_target"] : 0) \
        " max_gpui_drag_shell=" (("renderer_policy_gpui_drag_shell" in single_max) ? single_max["renderer_policy_gpui_drag_shell"] : 0) \
        " max_gpui_directory_drop_shell=" (("renderer_policy_gpui_directory_drop_shell" in single_max) ? single_max["renderer_policy_gpui_directory_drop_shell"] : 0) \
        " max_details_header_visual_layer=" (("renderer_policy_details_header_visual_layer" in single_max) ? single_max["renderer_policy_details_header_visual_layer"] : 0) \
        " max_gpui_details_header=" (("renderer_policy_gpui_details_header" in single_max) ? single_max["renderer_policy_gpui_details_header"] : 0) \
        " max_rename_overlay=" (("renderer_policy_rename_overlay" in single_max) ? single_max["renderer_policy_rename_overlay"] : 0)
    autosmoke_scenario = autosmoke_start_scenario != "" ? autosmoke_start_scenario : autosmoke_complete_scenario
    print "  autosmoke:" \
        " start=" (autosmoke_start_seen + 0) \
        " complete=" (autosmoke_complete_seen + 0) \
        " scenario=" (autosmoke_scenario == "" ? "<none>" : autosmoke_scenario) \
        " view_details=" (autosmoke_view_details_seen + 0) \
        " zoom_in=" (autosmoke_zoom_in_seen + 0) \
        " zoom_out=" (autosmoke_zoom_out_seen + 0) \
        " scroll_forward=" (autosmoke_scroll_forward_seen + 0) \
        " scroll_back=" (autosmoke_scroll_back_seen + 0)

    if (item_view_count == 0) {
        fail("missing [fika item-view] lines")
    }
    if (require_steady == "true" && steady_count == 0) {
        fail("missing [fika item-view] phase=steady frames")
    }
    if (require_details == "true" && details_visual_count == 0) {
        fail("missing [fika details-visual] lines")
    }
    if (require_details == "true" && details_shape_count == 0) {
        fail("missing [fika details-shape-cache] lines")
    }
    if (require_details == "true" && details_glyph_count == 0) {
        fail("missing [fika details-glyph-cache] lines")
    }
    if (require_details == "true" && details_glyph_budget_count == 0) {
        fail("missing [fika details-glyph-budget] lines")
    }
    if (require_warm_details_visual == "true" && warm_details_visual_count == 0) {
        fail("missing warmed [fika details-visual] paint frames")
    }
    if (require_static_visual == "true" && static_visual_count == 0) {
        fail("missing [fika static-item-visual] lines")
    }
    if (require_static_visual == "true" && item_shape_count == 0) {
        fail("missing [fika item-shape-cache] lines")
    }
    if (require_static_visual == "true" && item_glyph_count == 0) {
        fail("missing [fika item-glyph-cache] lines")
    }
    if (require_static_visual == "true" && item_glyph_budget_count == 0) {
        fail("missing [fika item-glyph-budget] lines")
    }
    for (mode in required_static_mode) {
        if (!(mode in static_visual_modes)) {
            fail("missing required static visual mode " mode)
        }
    }
    if (require_interaction == "true" && item_interaction_count == 0) {
        fail("missing [fika item-interaction] lines")
    }
    if (require_renderer_policy == "true" && renderer_policy_count == 0) {
        fail("missing [fika renderer-policy] lines")
    }
    if (expect_retained_item_policy == "true") {
        if (renderer_policy_count == 0) {
            fail("missing [fika renderer-policy] lines for retained item policy")
        }
        if (retained_item_policy_invalid) {
            fail("renderer-policy does not match retained item policy")
        }
    }
    if (require_paint_slots == "true") {
        if (item_paint_slot_count == 0) {
            fail("missing [fika item-paint-slots] lines")
        }
        if (!(("paint_slot_entries" in single_max) && single_max["paint_slot_entries"] > 0)) {
            fail("missing non-empty retained item paint-slot evidence")
        }
    }
    if (require_autosmoke == "true") {
        if (autosmoke_start_seen == 0 || autosmoke_complete_seen == 0) {
            fail("missing item-view autosmoke start/complete markers")
        }
        if (autosmoke_start_scenario == "" || autosmoke_complete_scenario == "") {
            fail("missing item-view autosmoke scenario markers")
        }
        if (autosmoke_start_scenario != "" && autosmoke_complete_scenario != "" &&
            autosmoke_start_scenario != autosmoke_complete_scenario) {
            fail("item-view autosmoke start/complete scenarios differ")
        }
        if (autosmoke_scenario_conflict) {
            fail("conflicting item-view autosmoke scenarios")
        }
        if (autosmoke_scenario != "Zoom" &&
            autosmoke_scenario != "Scroll" &&
            autosmoke_scenario != "ScrollEnd" &&
            autosmoke_scenario != "ZoomScroll" &&
            autosmoke_scenario != "IconsZoomScroll" &&
            autosmoke_scenario != "DetailsZoomScroll") {
            fail("unknown item-view autosmoke scenario " autosmoke_scenario)
        }
        if (autosmoke_scenario == "DetailsZoomScroll" && autosmoke_view_details_seen == 0) {
            fail("missing item-view autosmoke Details mode action marker")
        }
        if (autosmoke_scenario == "Zoom" ||
            autosmoke_scenario == "ZoomScroll" ||
            autosmoke_scenario == "IconsZoomScroll" ||
            autosmoke_scenario == "DetailsZoomScroll") {
            if (autosmoke_zoom_in_seen == 0 || autosmoke_zoom_out_seen == 0) {
                fail("missing item-view autosmoke zoom action markers")
            }
        }
        if (autosmoke_scenario == "Scroll" ||
            autosmoke_scenario == "ZoomScroll" ||
            autosmoke_scenario == "IconsZoomScroll" ||
            autosmoke_scenario == "DetailsZoomScroll") {
            if (autosmoke_scroll_forward_seen == 0 || autosmoke_scroll_back_seen == 0) {
                fail("missing changed=true item-view autosmoke scroll action markers")
            }
        }
        if (autosmoke_scenario == "ScrollEnd" && autosmoke_scroll_forward_seen == 0) {
            fail("missing changed=true item-view autosmoke scroll-end action marker")
        }
    }
    for (mode in required_renderer_policy_mode) {
        if (!(mode in renderer_policy_modes)) {
            fail("missing required renderer-policy mode " mode)
        }
    }
    for (mode in required_mode) {
        if (!(mode in modes)) {
            fail("missing required mode " mode)
        }
    }
    if (steady_over_limit > 0) {
        fail(steady_over_limit " steady item-view frame(s) exceeded " steady_total_limit "us")
    }
    if (warm_steady_over_limit > 0) {
        fail(warm_steady_over_limit " warmed steady item-view frame(s) exceeded " warm_steady_total_limit "us")
    }
    if (file_grid_over_limit > 0) {
        fail(file_grid_over_limit " file-grid build frame(s) exceeded " file_grid_build_limit "us")
    }
    if (static_visual_over_limit > 0) {
        fail(static_visual_over_limit " static visual paint frame(s) exceeded " static_visual_paint_limit "us")
    }
    if (image_over_limit > 0) {
        fail(image_over_limit " item image paint frame(s) exceeded " image_paint_limit "us")
    }
    if (custom_paint_over_limit > 0) {
        fail(custom_paint_over_limit " custom paint frame(s) exceeded " custom_paint_limit "us")
    }
    if (warm_static_visual_over_limit > 0) {
        fail(warm_static_visual_over_limit " warmed static visual paint frame(s) exceeded " warm_static_visual_paint_limit "us")
    }
    if (warm_image_over_limit > 0) {
        fail(warm_image_over_limit " warmed item image paint frame(s) exceeded " warm_image_paint_limit "us")
    }
    if (warm_custom_paint_over_limit > 0) {
        fail(warm_custom_paint_over_limit " warmed custom paint frame(s) exceeded " warm_custom_paint_limit "us")
    }

    exit failures > 0 ? 1 : 0
}
' "$log_path"
