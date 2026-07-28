#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: compare-item-image-renderers.sh [--gate-default-promotion] CANDIDATE_LOG BASELINE_LOG

Compares two FIKA_PERF_ITEM_VIEW logs for Compact/Icons item image rendering:

  CANDIDATE_LOG: run with the default full custom image renderer.
  BASELINE_LOG:  comparison baseline from an older or experimental renderer.

This is a log comparison helper. It cannot judge subjective smoothness, but it
does identify renderer-policy activation and custom image-layer placeholder,
decode, retained-image churn, and Dolphin-style cache-refresh placement.

Options:
  --gate-default-promotion
      Exit non-zero unless the full custom theme-icon path is clean enough to
      remain the default renderer against the comparison baseline.
EOF
}

gate_mode="none"
while [[ $# -gt 0 ]]; do
    case "${1:-}" in
        --gate-default-promotion)
            if [[ "$gate_mode" != "none" ]]; then
                usage >&2
                exit 2
            fi
            gate_mode="default-promotion"
            shift
            ;;
        *)
            break
            ;;
    esac
done

if [[ $# -ne 2 || "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    usage
    if [[ $# -eq 1 && ( "$1" == "-h" || "$1" == "--help" ) ]]; then
        exit 0
    fi
    exit 2
fi

custom_log="$1"
baseline_log="$2"
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd "$script_dir/../../apps/fika" && pwd)"
analyzer="$script_dir/analyze-item-view-perf.sh"

custom_summary="$("$analyzer" "$custom_log")"
baseline_summary="$("$analyzer" "$baseline_log")"

metric() {
    local summary="$1"
    local line_name="$2"
    local key="$3"
    printf '%s\n' "$summary" | awk -v line_name="$line_name" -v key="$key" '
        $1 == line_name {
            if (key == substr(line_name, 1, length(line_name) - 1)) {
                print $2 + 0
                found = 1
                exit
            }
            prefix = key "="
            for (i = 2; i <= NF; i++) {
                if (index($i, prefix) == 1) {
                    value = substr($i, length(prefix) + 1)
                    sub(/us$/, "", value)
                    print value
                    found = 1
                    exit
                }
            }
        }
        END {
            if (!found) {
                print 0
            }
        }
    '
}

phase_metric() {
    local summary="$1"
    local phase="$2"
    local key="$3"
    printf '%s\n' "$summary" | awk -v phase="$phase" -v key="$key" '
        $1 == "phase" && $2 == phase {
            prefix = key "="
            for (i = 3; i <= NF; i++) {
                if (index($i, prefix) == 1) {
                    value = substr($i, length(prefix) + 1)
                    sub(/us$/, "", value)
                    print value + 0
                    found = 1
                    exit
                }
            }
        }
        END {
            if (!found) {
                print 0
            }
        }
    '
}

perf_limit() {
    local baseline="$1"
    local percent="$2"
    local slack="$3"
    local floor="$4"
    local ratio=$(( (baseline * percent + 99) / 100 ))
    local plus=$(( baseline + slack ))
    local limit="$ratio"
    if (( plus > limit )); then
        limit="$plus"
    fi
    if (( floor > limit )); then
        limit="$floor"
    fi
    printf '%s\n' "$limit"
}

require_within() {
    local label="$1"
    local candidate="$2"
    local baseline="$3"
    local percent="$4"
    local slack="$5"
    local floor="$6"
    local limit
    limit="$(perf_limit "$baseline" "$percent" "$slack" "$floor")"
    if (( candidate > limit )); then
        gate_reasons+=("$label regression: candidate=${candidate}us baseline=${baseline}us limit=${limit}us")
    fi
}

custom_image_frames="$(metric "$custom_summary" "image_frames:" "image_frames")"
custom_image_layer="$(metric "$custom_summary" "renderer_policy_frames:" "max_image_layer")"
custom_gpui_image="$(metric "$custom_summary" "renderer_policy_frames:" "max_gpui_image_element")"
custom_icon_sync="$(metric "$custom_summary" "item_view_stage_max:" "icon_sync")"
custom_phase_initial="$(phase_metric "$custom_summary" "initial" "max_total")"
custom_phase_content="$(phase_metric "$custom_summary" "content-change" "max_total")"
custom_phase_geometry="$(phase_metric "$custom_summary" "geometry-change" "max_total")"
custom_phase_steady="$(phase_metric "$custom_summary" "steady" "max_total")"
custom_static_prepaint="$(metric "$custom_summary" "static_visual_frames:" "max_prepaint")"
custom_static_paint="$(metric "$custom_summary" "static_visual_frames:" "max_paint")"
custom_theme_loaded="$(metric "$custom_summary" "image_sources:" "theme_loaded")"
custom_theme_decoded="$(metric "$custom_summary" "image_sources:" "theme_decoded")"
custom_theme_retained="$(metric "$custom_summary" "image_sources:" "theme_retained")"
custom_theme_placeholder="$(metric "$custom_summary" "image_sources:" "theme_placeholder")"
custom_cache_refresh_frames="$(metric "$custom_summary" "image_cache_refresh_frames:" "image_cache_refresh_frames")"
custom_cache_refresh_loaded="$(metric "$custom_summary" "image_cache_refresh_frames:" "loaded")"
custom_cache_refresh_decoded="$(metric "$custom_summary" "image_cache_refresh_frames:" "decoded")"
custom_cache_refresh_retained="$(metric "$custom_summary" "image_cache_refresh_frames:" "retained")"
custom_cache_refresh_max_total="$(metric "$custom_summary" "image_cache_refresh_frames:" "max_total")"
custom_thumb_fallback="$(metric "$custom_summary" "image_sources:" "thumb_fallback")"
custom_image_paint="$(metric "$custom_summary" "image_frames:" "max_paint")"

baseline_image_frames="$(metric "$baseline_summary" "image_frames:" "image_frames")"
baseline_image_layer="$(metric "$baseline_summary" "renderer_policy_frames:" "max_image_layer")"
baseline_gpui_image="$(metric "$baseline_summary" "renderer_policy_frames:" "max_gpui_image_element")"
baseline_icon_sync="$(metric "$baseline_summary" "item_view_stage_max:" "icon_sync")"
baseline_phase_initial="$(phase_metric "$baseline_summary" "initial" "max_total")"
baseline_phase_content="$(phase_metric "$baseline_summary" "content-change" "max_total")"
baseline_phase_geometry="$(phase_metric "$baseline_summary" "geometry-change" "max_total")"
baseline_phase_steady="$(phase_metric "$baseline_summary" "steady" "max_total")"
baseline_static_prepaint="$(metric "$baseline_summary" "static_visual_frames:" "max_prepaint")"
baseline_static_paint="$(metric "$baseline_summary" "static_visual_frames:" "max_paint")"
baseline_theme_placeholder="$(metric "$baseline_summary" "image_sources:" "theme_placeholder")"
baseline_cache_refresh_max_total="$(metric "$baseline_summary" "image_cache_refresh_frames:" "max_total")"
baseline_image_paint="$(metric "$baseline_summary" "image_frames:" "max_paint")"

custom_renderer_state="unexpected"
if (( custom_image_layer > 0 && custom_gpui_image == 0 )); then
    custom_renderer_state="custom-image-layer"
elif (( custom_image_layer > 0 && custom_gpui_image > 0 )); then
    custom_renderer_state="mixed-gpui-and-custom"
fi

default_renderer_state="unexpected"
if (( baseline_gpui_image > 0 )); then
    default_renderer_state="gpui-theme-icons"
elif (( baseline_image_layer > 0 && baseline_gpui_image == 0 )); then
    default_renderer_state="custom-image-layer"
fi

placeholder_judgement="no custom first-load placeholder evidence"
if (( custom_theme_placeholder > 0 )); then
    placeholder_judgement="custom image layer showed first-load theme placeholders"
fi

decode_judgement="no theme decode completion churn in custom log"
if (( custom_theme_decoded > 0 )); then
    decode_judgement="custom image layer observed GPUI theme decode completion"
fi

promotion_gate_state="not requested"
gate_reasons=()
if [[ "$gate_mode" == "default-promotion" ]]; then
    promotion_gate_state="pass"
    if [[ "$custom_renderer_state" != "custom-image-layer" ]]; then
        gate_reasons+=("custom log did not route theme icons exclusively through the custom image layer")
    fi
    if (( custom_image_frames == 0 )); then
        gate_reasons+=("custom log did not include item-image frames")
    fi
    if (( custom_theme_placeholder > 0 )); then
        gate_reasons+=("custom log still has theme_placeholder churn")
    fi
    if (( custom_theme_decoded > 0 )); then
        gate_reasons+=("custom log still has theme_decoded first-ready churn")
    fi
    if (( baseline_theme_placeholder > 0 )); then
        gate_reasons+=("baseline log unexpectedly has theme placeholders")
    fi
    if (( ${#gate_reasons[@]} > 0 )); then
        promotion_gate_state="fail"
    fi
fi

gate_reason_text="none"
if (( ${#gate_reasons[@]} > 0 )); then
    gate_reason_text="$(printf '%s; ' "${gate_reasons[@]}")"
    gate_reason_text="${gate_reason_text%; }"
fi

cat <<EOF
## Item Image Renderer A/B Evidence

- Candidate log: \`$custom_log\`
- Baseline log: \`$baseline_log\`

| Metric | Candidate theme-icons | Baseline |
| --- | ---: | ---: |
| renderer max image_layer | $custom_image_layer | $baseline_image_layer |
| renderer max gpui_image_element | $custom_gpui_image | $baseline_gpui_image |
| icon_sync max us | $custom_icon_sync | $baseline_icon_sync |
| phase initial max total us | $custom_phase_initial | $baseline_phase_initial |
| phase content-change max total us | $custom_phase_content | $baseline_phase_content |
| phase geometry-change max total us | $custom_phase_geometry | $baseline_phase_geometry |
| phase steady max total us | $custom_phase_steady | $baseline_phase_steady |
| static visual max prepaint us | $custom_static_prepaint | $baseline_static_prepaint |
| static visual max paint us | $custom_static_paint | $baseline_static_paint |
| item-image frames | $custom_image_frames | $baseline_image_frames |
| max item-image paint us | $custom_image_paint | $baseline_image_paint |
| theme loaded | $custom_theme_loaded | 0 |
| theme decoded first-ready | $custom_theme_decoded | 0 |
| theme retained | $custom_theme_retained | 0 |
| theme placeholder | $custom_theme_placeholder | $baseline_theme_placeholder |
| cache-refresh frames | $custom_cache_refresh_frames | 0 |
| cache-refresh loaded | $custom_cache_refresh_loaded | 0 |
| cache-refresh decoded | $custom_cache_refresh_decoded | 0 |
| cache-refresh retained | $custom_cache_refresh_retained | 0 |
| cache-refresh max total us | $custom_cache_refresh_max_total | $baseline_cache_refresh_max_total |
| thumbnail fallback | $custom_thumb_fallback | 0 |

Automated interpretation:

- Candidate renderer state: $custom_renderer_state
- Custom-theme renderer state: $custom_renderer_state
- Baseline renderer state: $default_renderer_state
- Placeholder evidence: $placeholder_judgement
- Decode evidence: $decode_judgement
- Retained same-icon evidence: theme_retained=$custom_theme_retained
- Cache-refresh evidence: loaded=$custom_cache_refresh_loaded decoded=$custom_cache_refresh_decoded retained=$custom_cache_refresh_retained max_total=${custom_cache_refresh_max_total}us
- Default-promotion gate: $promotion_gate_state
- Gate reasons: $gate_reason_text

Candidate analyzer summary:

\`\`\`text
$custom_summary
\`\`\`

Baseline analyzer summary:

\`\`\`text
$baseline_summary
\`\`\`
EOF

if [[ "$promotion_gate_state" == "fail" ]]; then
    exit 1
fi
