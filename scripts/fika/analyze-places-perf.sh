#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: analyze-places-perf.sh [OPTIONS] LOG
       FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=targets target/debug/fika /etc 2>&1 | analyze-places-perf.sh [OPTIONS] -

Summarizes FIKA_PERF_PLACES_VIEW logs and optionally enforces Places renderer
policy gates.

Options:
  --require-autosmoke
      Fail unless the non-destructive FIKA_AUTOSMOKE_PLACES=targets markers are
      present and show target/insert/clear projection.

  --require-overflow-autosmoke
      Fail unless FIKA_AUTOSMOKE_PLACES=overflow markers are present and the
      log proves sidebar overflow through [fika places-scrollbar].

  --require-layout-autosmoke
      Fail unless FIKA_AUTOSMOKE_PLACES=layout markers are present and prove
      sidebar hide/show, resize, reset, restore, and persisted settings.

  --require-hit-test-autosmoke
      Fail unless FIKA_AUTOSMOKE_PLACES=hit-test markers are present and prove
      retained Places row edge/body and section hit testing.

  --require-retained-targeting-autosmoke
      Fail unless FIKA_AUTOSMOKE_PLACES=targeting markers are present and prove
      retained Places activation-row, row context-menu, and section context-menu
      target classification.

  --require-retained-dnd-autosmoke
      Fail unless FIKA_AUTOSMOKE_PLACES=dnd markers are present and prove
      retained Places DnD target decisions for path-list and place drags.

  --require-interaction-policy
      Fail unless [fika places-interaction-policy] is present and proves the
      current retained target-decision / GPUI event-shell boundary.

  --require-interaction-geometry
      Fail unless [fika places-interaction-geometry] is present and matches
      the retained Places row/section projection counts.

  --require-event-probe
      Fail unless [fika places-event-probe] is present and its hitbox count
      matches the explicit retained-probe policy count.

  --expect-custom-row-visual-policy
      Fail unless [fika places-renderer-policy] matches the opt-in
      custom text policy: row_visual_layer/icon_gpui equal rows,
      drag_shell=0, row_gpui=0, text_gpui=0, retained_interaction matching the
      selected event policy, section_gpui=0, scrollbar_canvas=1, and
      visual_kind=text. Also requires aggregated [fika places-row-visual] logs
      whose rows count matches the policy rows and row text shape-cache logs
      for the opt-in visual path.

  --expect-custom-row-full-policy
      Fail unless [fika places-renderer-policy] matches the opt-in
      full custom row visual policy: row_visual_layer equal rows,
      drag_shell=0,
      row_gpui=0, text_gpui=0, icon_gpui=0, retained_interaction matching the
      selected event policy, section_gpui=0, scrollbar_canvas=1, and
      visual_kind=full. Also requires aggregated [fika places-row-visual] logs
      whose rows count matches the policy rows and row text shape-cache logs.

  --expect-custom-row-chrome-policy
      Fail unless [fika places-renderer-policy] matches the Dolphin-aligned
      custom chrome policy: row_visual_layer/text_gpui/icon_gpui equal rows,
      drag_shell=0, row_gpui=0, retained_interaction matching the selected event
      policy, section_gpui=sections, scrollbar_canvas=1, and
      visual_kind=chrome. Also requires aggregated [fika places-row-visual]
      logs whose rows count matches the policy rows and rejects row text
      shape-cache logs because text remains GPUI-rendered.

  --expect-retained-event-policy
      Fail unless [fika places-renderer-policy] and
      [fika places-interaction-policy] match the future retained event-delivery
      policy: retained interaction/hitboxes equal rows+sections,
      gpui_event_shells=0, drag_shells=0, and row visuals are either the
      current GPUI rows or the aggregated opt-in custom visual layer.

  --snapshot-us N
      Fail if any [fika places-view] snapshot exceeds N microseconds.

  --sidebar-build-us N
      Fail if any [fika places-sidebar] build exceeds N microseconds.

  --slot-project-us N
      Fail if any [fika places-slots] project exceeds N microseconds.

  --row-visual-paint-us N
      Fail if any [fika places-row-visual] paint exceeds N microseconds.

  --row-visual-prepaint-us N
      Fail if any [fika places-row-visual] prepaint exceeds N microseconds.

  --row-visual-warm-paint-us N
      Fail if any [fika places-row-visual] paint after the first two frames
      exceeds N microseconds.

  --row-visual-warm-prepaint-us N
      Fail if any [fika places-row-visual] prepaint after the first two frames
      exceeds N microseconds.

  --render-total-us N
      Fail if any [fika render] total exceeds N microseconds when render logs
      are present.

  -h, --help
      Show this help.
EOF
}

require_autosmoke=false
require_overflow_autosmoke=false
require_layout_autosmoke=false
require_hit_test_autosmoke=false
require_retained_targeting_autosmoke=false
require_retained_dnd_autosmoke=false
require_interaction_policy=false
require_interaction_geometry=false
require_event_probe=false
expect_custom_row_visual_policy=false
expect_custom_row_full_policy=false
expect_custom_row_chrome_policy=false
expect_retained_event_policy=false
snapshot_us=""
sidebar_build_us=""
slot_project_us=""
row_visual_paint_us=""
row_visual_prepaint_us=""
row_visual_warm_paint_us=""
row_visual_warm_prepaint_us=""
render_total_us=""
log_path=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --require-autosmoke)
            require_autosmoke=true
            ;;
        --require-overflow-autosmoke)
            require_overflow_autosmoke=true
            ;;
        --require-layout-autosmoke)
            require_layout_autosmoke=true
            ;;
        --require-hit-test-autosmoke)
            require_hit_test_autosmoke=true
            ;;
        --require-retained-targeting-autosmoke)
            require_retained_targeting_autosmoke=true
            ;;
        --require-retained-dnd-autosmoke)
            require_retained_dnd_autosmoke=true
            ;;
        --require-interaction-policy)
            require_interaction_policy=true
            ;;
        --require-interaction-geometry)
            require_interaction_geometry=true
            ;;
        --require-event-probe)
            require_event_probe=true
            ;;
        --expect-custom-row-visual-policy)
            expect_custom_row_visual_policy=true
            ;;
        --expect-custom-row-full-policy)
            expect_custom_row_full_policy=true
            ;;
        --expect-custom-row-chrome-policy)
            expect_custom_row_chrome_policy=true
            ;;
        --expect-retained-event-policy)
            expect_retained_event_policy=true
            ;;
        --snapshot-us)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--snapshot-us requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            snapshot_us="$2"
            shift
            ;;
        --snapshot-us=*)
            snapshot_us="${1#--snapshot-us=}"
            ;;
        --sidebar-build-us)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--sidebar-build-us requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            sidebar_build_us="$2"
            shift
            ;;
        --sidebar-build-us=*)
            sidebar_build_us="${1#--sidebar-build-us=}"
            ;;
        --slot-project-us)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--slot-project-us requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            slot_project_us="$2"
            shift
            ;;
        --slot-project-us=*)
            slot_project_us="${1#--slot-project-us=}"
            ;;
        --row-visual-paint-us)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--row-visual-paint-us requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            row_visual_paint_us="$2"
            shift
            ;;
        --row-visual-paint-us=*)
            row_visual_paint_us="${1#--row-visual-paint-us=}"
            ;;
        --row-visual-prepaint-us)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--row-visual-prepaint-us requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            row_visual_prepaint_us="$2"
            shift
            ;;
        --row-visual-prepaint-us=*)
            row_visual_prepaint_us="${1#--row-visual-prepaint-us=}"
            ;;
        --row-visual-warm-paint-us)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--row-visual-warm-paint-us requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            row_visual_warm_paint_us="$2"
            shift
            ;;
        --row-visual-warm-paint-us=*)
            row_visual_warm_paint_us="${1#--row-visual-warm-paint-us=}"
            ;;
        --row-visual-warm-prepaint-us)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--row-visual-warm-prepaint-us requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            row_visual_warm_prepaint_us="$2"
            shift
            ;;
        --row-visual-warm-prepaint-us=*)
            row_visual_warm_prepaint_us="${1#--row-visual-warm-prepaint-us=}"
            ;;
        --render-total-us)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--render-total-us requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            render_total_us="$2"
            shift
            ;;
        --render-total-us=*)
            render_total_us="${1#--render-total-us=}"
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

for value_name in snapshot_us sidebar_build_us slot_project_us row_visual_paint_us row_visual_prepaint_us row_visual_warm_paint_us row_visual_warm_prepaint_us render_total_us; do
    value="${!value_name}"
    if [[ -n "$value" && ! "$value" =~ ^[0-9]+$ ]]; then
        echo "--${value_name//_/-} must be an integer microsecond value" >&2
        exit 2
    fi
done

awk \
    -v require_autosmoke="$require_autosmoke" \
    -v require_overflow_autosmoke="$require_overflow_autosmoke" \
    -v require_layout_autosmoke="$require_layout_autosmoke" \
    -v require_hit_test_autosmoke="$require_hit_test_autosmoke" \
    -v require_retained_targeting_autosmoke="$require_retained_targeting_autosmoke" \
    -v require_retained_dnd_autosmoke="$require_retained_dnd_autosmoke" \
    -v require_interaction_policy="$require_interaction_policy" \
    -v require_interaction_geometry="$require_interaction_geometry" \
    -v require_event_probe="$require_event_probe" \
    -v expect_custom_row_visual_policy="$expect_custom_row_visual_policy" \
    -v expect_custom_row_full_policy="$expect_custom_row_full_policy" \
    -v expect_custom_row_chrome_policy="$expect_custom_row_chrome_policy" \
    -v expect_retained_event_policy="$expect_retained_event_policy" \
    -v snapshot_limit="$snapshot_us" \
    -v sidebar_build_limit="$sidebar_build_us" \
    -v slot_project_limit="$slot_project_us" \
    -v row_visual_paint_limit="$row_visual_paint_us" \
    -v row_visual_prepaint_limit="$row_visual_prepaint_us" \
    -v row_visual_warm_paint_limit="$row_visual_warm_paint_us" \
    -v row_visual_warm_prepaint_limit="$row_visual_warm_prepaint_us" \
    -v render_total_limit="$render_total_us" '
function field(name,    prefix, i, value) {
    prefix = name "="
    for (i = 1; i <= NF; i++) {
        if (index($i, prefix) == 1) {
            value = substr($i, length(prefix) + 1)
            gsub(/,$/, "", value)
            gsub(/us$/, "", value)
            return value
        }
    }
    return ""
}

function max_update(name, value) {
    value += 0
    if (!(name in max_values) || value > max_values[name]) {
        max_values[name] = value
    }
}

function append_csv(list, value) {
    if (list == "") {
        return value
    }
    return list "," value
}

function fail(message) {
    print "error: " message > "/dev/stderr"
    exit_code = 1
}

function renderer_retained_interaction_for_policy(event_policy, rows, sections) {
    if (event_policy == "retained-targeting" || event_policy == "retained-dnd") {
        return rows + sections
    }
    return 0
}

/^\[fika places-view\]/ {
    places_view_frames++
    source = field("source") + 0
    visible = field("visible") + 0
    sections = field("sections") + 0
    snapshot = field("snapshot") + 0
    max_update("snapshot", snapshot)
    max_update("source", source)
    max_update("visible", visible)
    max_update("sections", sections)
}

/^\[fika render\]/ {
    render_frames++
    panes = field("panes") + 0
    window_setup = field("window_setup") + 0
    places = field("places") + 0
    tasks = field("tasks") + 0
    snapshots = field("snapshots") + 0
    chrome_inputs = field("chrome_inputs") + 0
    chrome_state = field("chrome_state") + 0
    chrome_icons = field("chrome_icons") + 0
    if (chrome_inputs == 0) {
        chrome_inputs = chrome_state + chrome_icons
    }
    pane_elements = field("pane_elements") + 0
    overlays = field("overlays") + 0
    root = field("root") + 0
    total = field("total") + 0
    accounted = window_setup + places + tasks + snapshots + chrome_inputs + pane_elements + overlays + root
    residual = total - accounted
    max_update("render_panes", panes)
    max_update("render_window_setup", window_setup)
    max_update("render_places", places)
    max_update("render_tasks", tasks)
    max_update("render_snapshots", snapshots)
    max_update("render_chrome_inputs", chrome_inputs)
    max_update("render_chrome_state", chrome_state)
    max_update("render_chrome_icons", chrome_icons)
    max_update("render_pane_elements", pane_elements)
    max_update("render_overlays", overlays)
    max_update("render_root", root)
    max_update("render_total", total)
    max_update("render_accounted", accounted)
    max_update("render_residual", residual)
    if (render_frames == 1 || total > render_at_max_total) {
        render_at_max_total = total
        render_at_max_panes = panes
        render_at_max_window_setup = window_setup
        render_at_max_places = places
        render_at_max_tasks = tasks
        render_at_max_snapshots = snapshots
        render_at_max_chrome_inputs = chrome_inputs
        render_at_max_chrome_state = chrome_state
        render_at_max_chrome_icons = chrome_icons
        render_at_max_pane_elements = pane_elements
        render_at_max_overlays = overlays
        render_at_max_root = root
        render_at_max_accounted = accounted
        render_at_max_residual = residual
    }
}

/^\[fika places-sidebar\]/ {
    sidebar_frames++
    rows = field("rows") + 0
    sections = field("sections") + 0
    elements = field("elements") + 0
    build = field("build") + 0
    max_update("sidebar_rows", rows)
    max_update("sidebar_sections", sections)
    max_update("sidebar_elements", elements)
    max_update("sidebar_build", build)
    last_sidebar_sections = sections
}

/^\[fika places-icon-cache-refresh\]/ {
    icon_cache_refresh_frames++
    rows = field("rows") + 0
    requested = field("requested") + 0
    retained = field("retained") + 0
    loaded = field("loaded") + 0
    decoded = field("decoded") + 0
    missing = field("missing") + 0
    non_svg = field("non_svg") + 0
    entries = field("entries") + 0
    bytes = field("bytes") + 0
    evicted = field("evicted") + 0
    total = field("total") + 0
    icon_cache_refresh_requested += requested
    icon_cache_refresh_retained += retained
    icon_cache_refresh_loaded += loaded
    icon_cache_refresh_decoded += decoded
    icon_cache_refresh_missing += missing
    icon_cache_refresh_non_svg += non_svg
    icon_cache_refresh_evicted += evicted
    max_update("icon_cache_refresh_rows", rows)
    max_update("icon_cache_refresh_requested", requested)
    max_update("icon_cache_refresh_retained", retained)
    max_update("icon_cache_refresh_loaded", loaded)
    max_update("icon_cache_refresh_decoded", decoded)
    max_update("icon_cache_refresh_missing", missing)
    max_update("icon_cache_refresh_non_svg", non_svg)
    max_update("icon_cache_refresh_entries", entries)
    max_update("icon_cache_refresh_bytes", bytes)
    max_update("icon_cache_refresh_evicted", evicted)
    max_update("icon_cache_refresh_total", total)
}

/^\[fika places-slots\]/ {
    slot_frames++
    rows = field("rows") + 0
    sections = field("sections") + 0
    entries = field("entries") + 0
    inserted = field("inserted") + 0
    content = field("content") + 0
    geometry = field("geometry") + 0
    visual = field("visual") + 0
    unchanged = field("unchanged") + 0
    removed = field("removed") + 0
    project = field("project") + 0
    if (inserted > 0) {
        slot_inserted_frame_seen = 1
    }
    if (unchanged > 0) {
        slot_unchanged_frame_seen = 1
    }
    if (visual > 0) {
        slot_visual_frame_seen = 1
    }
    max_update("slot_rows", rows)
    max_update("slot_sections", sections)
    max_update("slot_entries", entries)
    max_update("slot_inserted", inserted)
    max_update("slot_content", content)
    max_update("slot_geometry", geometry)
    max_update("slot_visual", visual)
    max_update("slot_unchanged", unchanged)
    max_update("slot_removed", removed)
    max_update("slot_project", project)
}

/^\[fika places-renderer-policy\]/ {
    policy_frames++
    rows = field("rows") + 0
    row_gpui = field("row_gpui") + 0
    row_visual_layer = field("row_visual_layer") + 0
    text_gpui_field = field("text_gpui")
    if (text_gpui_field == "") {
        text_gpui = row_gpui
    } else {
        text_gpui = text_gpui_field + 0
    }
    icon_gpui = field("icon_gpui") + 0
    retained_interaction = field("retained_interaction") + 0
    retained_probe_hitboxes = field("retained_probe_hitboxes") + 0
    drag_shell = field("drag_shell") + 0
    section_gpui = field("section_gpui") + 0
    scrollbar_canvas = field("scrollbar_canvas") + 0
    event_policy = field("event_policy")
    visual_kind = field("visual_kind")
    if (visual_kind == "") {
        if (row_gpui == rows && row_visual_layer == 0) {
            visual_kind = "gpui"
        } else if (row_gpui == 0 && row_visual_layer == rows && text_gpui == rows) {
            visual_kind = "chrome"
        } else if (row_gpui == 0 && row_visual_layer == rows && text_gpui == 0 &&
            icon_gpui == rows) {
            visual_kind = "text"
        } else if (row_gpui == 0 && row_visual_layer == rows && text_gpui == 0 &&
            icon_gpui == 0) {
            visual_kind = "full"
        } else {
            visual_kind = "mixed"
        }
    }
    expected_retained_interaction = renderer_retained_interaction_for_policy(event_policy, rows, last_sidebar_sections)
    max_update("policy_rows", rows)
    max_update("policy_sections", last_sidebar_sections)
    max_update("policy_row_gpui", row_gpui)
    max_update("policy_row_visual_layer", row_visual_layer)
    max_update("policy_text_gpui", text_gpui)
    max_update("policy_icon_gpui", icon_gpui)
    max_update("policy_retained_interaction", retained_interaction)
    max_update("policy_retained_probe_hitboxes", retained_probe_hitboxes)
    max_update("policy_drag_shell", drag_shell)
    max_update("policy_section_gpui", section_gpui)
    max_update("policy_scrollbar_canvas", scrollbar_canvas)
    if (visual_kind == "gpui") {
        policy_kind_gpui_seen = 1
    } else if (visual_kind == "chrome") {
        policy_kind_chrome_seen = 1
    } else if (visual_kind == "text") {
        policy_kind_text_seen = 1
    } else if (visual_kind == "full") {
        policy_kind_full_seen = 1
    } else {
        policy_kind_other_seen = 1
    }
    if (expect_custom_row_visual_policy == "true") {
        if (row_gpui != 0 || text_gpui != 0 || icon_gpui != rows || drag_shell != 0 ||
            row_visual_layer != rows || retained_interaction != expected_retained_interaction ||
            section_gpui != 0 || scrollbar_canvas != 1 ||
            visual_kind != "text") {
            custom_policy_invalid = 1
        }
    }
    if (expect_custom_row_full_policy == "true") {
        if (row_gpui != 0 || text_gpui != 0 || icon_gpui != 0 || drag_shell != 0 ||
            row_visual_layer != rows || retained_interaction != expected_retained_interaction ||
            section_gpui != 0 || scrollbar_canvas != 1 ||
            visual_kind != "full") {
            custom_full_policy_invalid = 1
        }
    }
    if (expect_custom_row_chrome_policy == "true") {
        if (row_gpui != 0 || text_gpui != rows || icon_gpui != rows || drag_shell != 0 ||
            row_visual_layer != rows || retained_interaction != expected_retained_interaction ||
            section_gpui != last_sidebar_sections || scrollbar_canvas != 1 ||
            visual_kind != "chrome") {
            custom_chrome_policy_invalid = 1
        }
    }
    if (expect_retained_event_policy == "true") {
        row_visual_policy_valid = (row_gpui == rows && row_visual_layer == 0 &&
            text_gpui == rows && section_gpui == last_sidebar_sections &&
            visual_kind == "gpui") ||
            (row_gpui == 0 && row_visual_layer == rows &&
                ((text_gpui == rows && section_gpui == last_sidebar_sections &&
                    visual_kind == "chrome") ||
                 (text_gpui == 0 && icon_gpui == rows && section_gpui == 0 &&
                    visual_kind == "text") ||
                 (text_gpui == 0 && icon_gpui == 0 && section_gpui == 0 &&
                    visual_kind == "full")))
        if (!row_visual_policy_valid ||
            retained_interaction != rows + last_sidebar_sections ||
            drag_shell != 0 || scrollbar_canvas != 1) {
            retained_event_renderer_policy_invalid = 1
        }
    }
}

/^\[fika places-interaction-policy\]/ {
    interaction_policy_frames++
    rows = field("rows") + 0
    sections = field("sections") + 0
    row_target_decisions = field("row_target_decisions") + 0
    section_target_decisions = field("section_target_decisions") + 0
    retained_hitboxes = field("retained_hitboxes") + 0
    retained_probe_hitboxes = field("retained_probe_hitboxes") + 0
    gpui_event_shells = field("gpui_event_shells") + 0
    drag_shells = field("drag_shells") + 0
    event_policy = field("event_policy")
    gpui_row_section_event_shells_field = field("gpui_row_section_event_shells")
    gpui_typed_dnd_payload_shells_field = field("gpui_typed_dnd_payload_shells")
    drag_start_models_field = field("drag_start_models")
    if (drag_start_models_field == "") {
        drag_start_models = rows
    } else {
        drag_start_models = drag_start_models_field + 0
    }
    gpui_sidebar_leave_shells_field = field("gpui_sidebar_leave_shells")
    retained_targeting = field("retained_targeting") + 0
    retained_dnd = field("retained_dnd") + 0
    expected_gpui_sidebar_leave_shells = (expect_retained_event_policy == "true" ||
        event_policy == "retained-pointer" || event_policy == "retained-targeting" ||
        retained_dnd == rows + sections) ? 0 : 3
    expected_gpui_row_section_event_shells = (expect_retained_event_policy == "true" ||
        retained_dnd == rows + sections) ? 0 : rows + sections
    expected_gpui_typed_dnd_payload_shells = 0
    if (gpui_row_section_event_shells_field == "") {
        gpui_row_section_event_shells = expected_gpui_row_section_event_shells
    } else {
        gpui_row_section_event_shells = gpui_row_section_event_shells_field + 0
    }
    if (gpui_typed_dnd_payload_shells_field == "") {
        gpui_typed_dnd_payload_shells = expected_gpui_typed_dnd_payload_shells
    } else {
        gpui_typed_dnd_payload_shells = gpui_typed_dnd_payload_shells_field + 0
    }
    if (gpui_sidebar_leave_shells_field == "") {
        gpui_sidebar_leave_shells = expected_gpui_sidebar_leave_shells
    } else {
        gpui_sidebar_leave_shells = gpui_sidebar_leave_shells_field + 0
    }
    max_update("interaction_rows", rows)
    max_update("interaction_sections", sections)
    max_update("interaction_row_target_decisions", row_target_decisions)
    max_update("interaction_section_target_decisions", section_target_decisions)
    max_update("interaction_retained_hitboxes", retained_hitboxes)
    max_update("interaction_retained_probe_hitboxes", retained_probe_hitboxes)
    max_update("interaction_gpui_event_shells", gpui_event_shells)
    max_update("interaction_gpui_row_section_event_shells", gpui_row_section_event_shells)
    max_update("interaction_gpui_typed_dnd_payload_shells", gpui_typed_dnd_payload_shells)
    max_update("interaction_drag_shells", drag_shells)
    max_update("interaction_drag_start_models", drag_start_models)
    max_update("interaction_gpui_sidebar_leave_shells", gpui_sidebar_leave_shells)
    max_update("interaction_retained_targeting", retained_targeting)
    max_update("interaction_retained_dnd", retained_dnd)
    current_gpui_shell_boundary_valid = (gpui_event_shells == rows + sections && retained_dnd == 0)
    current_retained_dnd_boundary_valid = (gpui_event_shells == 0 && retained_dnd == rows + sections)
    expected_retained_hitboxes = (retained_targeting == rows + sections || retained_dnd == rows + sections) ? rows + sections : 0
    if (row_target_decisions != rows ||
        section_target_decisions != sections ||
        retained_hitboxes != expected_retained_hitboxes ||
        !(current_gpui_shell_boundary_valid || current_retained_dnd_boundary_valid) ||
        gpui_event_shells != gpui_row_section_event_shells + gpui_typed_dnd_payload_shells ||
        gpui_row_section_event_shells != expected_gpui_row_section_event_shells ||
        gpui_typed_dnd_payload_shells != expected_gpui_typed_dnd_payload_shells ||
        drag_shells != 0 ||
        drag_start_models != rows ||
        gpui_sidebar_leave_shells != expected_gpui_sidebar_leave_shells) {
        current_interaction_policy_invalid = 1
    }
    if (row_target_decisions != rows ||
        section_target_decisions != sections ||
        retained_hitboxes != rows + sections ||
        gpui_event_shells != 0 ||
        gpui_row_section_event_shells != 0 ||
        gpui_typed_dnd_payload_shells != 0 ||
        drag_shells != 0 ||
        drag_start_models != rows ||
        gpui_sidebar_leave_shells != 0) {
        retained_event_interaction_policy_invalid = 1
    }
}

/^\[fika places-interaction-geometry\]/ {
    interaction_geometry_frames++
    rows = field("rows") + 0
    sections = field("sections") + 0
    entries = field("entries") + 0
    content_height = field("content_height") + 0
    hit_tests = field("hit_tests") + 0
    project = field("project") + 0
    max_update("interaction_geometry_rows", rows)
    max_update("interaction_geometry_sections", sections)
    max_update("interaction_geometry_entries", entries)
    max_update("interaction_geometry_content_height", content_height)
    max_update("interaction_geometry_hit_tests", hit_tests)
    max_update("interaction_geometry_project", project)
    if (entries != rows + sections || content_height <= 0 ||
        (rows > 0 && hit_tests == 0)) {
        interaction_geometry_invalid = 1
    }
    if (policy_frames > 0 && rows != max_values["policy_rows"]) {
        interaction_geometry_invalid = 1
    }
}

/^\[fika places-event-probe\]/ {
    event_probe_frames++
    rows = field("rows") + 0
    sections = field("sections") + 0
    hitboxes = field("hitboxes") + 0
    hovered = field("hovered") + 0
    pointer = field("pointer") + 0
    targeting = field("targeting") + 0
    dnd = field("dnd") + 0
    prepaint = field("prepaint") + 0
    paint = field("paint") + 0
    max_update("event_probe_rows", rows)
    max_update("event_probe_sections", sections)
    max_update("event_probe_hitboxes", hitboxes)
    max_update("event_probe_hovered", hovered)
    max_update("event_probe_pointer", pointer)
    max_update("event_probe_targeting", targeting)
    max_update("event_probe_dnd", dnd)
    max_update("event_probe_prepaint", prepaint)
    max_update("event_probe_paint", paint)
    if (hitboxes != rows + sections || hitboxes < 1) {
        event_probe_invalid = 1
    }
}

/^\[fika places-row-visual\]/ {
    row_visual_frames++
    rows = field("rows") + 0
    painted = field("painted")
    if (painted == "") {
        painted = rows
    } else {
        painted += 0
    }
    prepaint = field("prepaint") + 0
    paint = field("paint") + 0
    max_update("row_visual_rows", rows)
    max_update("row_visual_painted", painted)
    max_update("row_visual_prepaint", prepaint)
    max_update("row_visual_paint", paint)
    if (row_visual_frames > 2) {
        row_visual_warm_frames++
        max_update("row_visual_warm_prepaint", prepaint)
        max_update("row_visual_warm_paint", paint)
    }
}

/^\[fika places-row-shape-cache\]/ {
    row_shape_cache_frames++
    hits = field("hits") + 0
    misses = field("misses") + 0
    evicted = field("evicted") + 0
    compute = field("compute") + 0
    entries = field("entries") + 0
    max_update("row_shape_hits", hits)
    max_update("row_shape_misses", misses)
    max_update("row_shape_evicted", evicted)
    max_update("row_shape_compute", compute)
    max_update("row_shape_entries", entries)
}

/^\[fika places-row-glyph-cache\]/ {
    row_glyph_cache_frames++
    hits = field("hits") + 0
    misses = field("misses") + 0
    evicted = field("evicted") + 0
    compute = field("compute") + 0
    entries = field("entries") + 0
    max_update("row_glyph_hits", hits)
    max_update("row_glyph_misses", misses)
    max_update("row_glyph_evicted", evicted)
    max_update("row_glyph_compute", compute)
    max_update("row_glyph_entries", entries)
}

/^\[fika places-scrollbar\]/ {
    scrollbar_frames++
    visible = field("visible") + 0
    max_scroll_y = field("max_scroll_y") + 0
    thumb_height = field("thumb_height") + 0
    track_height = field("track_height") + 0
    if (visible > 0) {
        scrollbar_visible_seen = 1
    }
    if (max_scroll_y > 0) {
        scrollbar_overflow_seen = 1
    }
    max_update("scrollbar_visible", visible)
    max_update("scrollbar_max_scroll_y", max_scroll_y)
    max_update("scrollbar_thumb_height", thumb_height)
    max_update("scrollbar_track_height", track_height)
}

/^\[fika autosmoke\] places start scenario=DropTargets/ {
    autosmoke_start_seen = 1
}

/^\[fika autosmoke\] places complete scenario=DropTargets/ {
    autosmoke_complete_seen = 1
}

/^\[fika autosmoke\] places start scenario=Overflow/ {
    overflow_autosmoke_start_seen = 1
}

/^\[fika autosmoke\] places complete scenario=Overflow/ {
    overflow_autosmoke_complete_seen = 1
}

/^\[fika autosmoke\] places start scenario=Layout/ {
    layout_autosmoke_start_seen = 1
}

/^\[fika autosmoke\] places complete scenario=Layout/ {
    layout_autosmoke_complete_seen = 1
}

/^\[fika autosmoke\] places start scenario=HitTest/ {
    hit_test_autosmoke_start_seen = 1
}

/^\[fika autosmoke\] places complete scenario=HitTest/ {
    hit_test_autosmoke_complete_seen = 1
}

/^\[fika autosmoke\] places start scenario=RetainedTargeting/ {
    retained_targeting_autosmoke_start_seen = 1
}

/^\[fika autosmoke\] places complete scenario=RetainedTargeting/ {
    retained_targeting_autosmoke_complete_seen = 1
}

/^\[fika autosmoke\] places start scenario=RetainedDnd/ {
    retained_dnd_autosmoke_start_seen = 1
}

/^\[fika autosmoke\] places complete scenario=RetainedDnd/ {
    retained_dnd_autosmoke_complete_seen = 1
}

/^\[fika autosmoke\] places action=/ {
    action = field("action")
    changed = field("changed")
    visible = field("visible")
    ok = field("ok")
    if (action == "target-first-place" && changed == "true") {
        autosmoke_target_action_seen = 1
    } else if (action == "target-insert-start" && changed == "true") {
        autosmoke_insert_start_action_seen = 1
    } else if (action == "target-insert-end" && changed == "true") {
        autosmoke_insert_end_action_seen = 1
    } else if (action == "clear-targets" && changed == "true") {
        autosmoke_clear_action_seen = 1
    } else if (action == "layout-initial") {
        layout_initial_seen = 1
    } else if (action == "layout-hide" && changed == "true" && visible == "false") {
        layout_hide_seen = 1
    } else if (action == "layout-show" && changed == "true" && visible == "true") {
        layout_show_seen = 1
    } else if (action == "layout-resize" && changed == "true" && visible == "true") {
        layout_resize_seen = 1
    } else if (action == "layout-reset" && changed == "true" && visible == "true") {
        layout_reset_seen = 1
    } else if (action == "layout-restore") {
        layout_restore_seen = 1
    } else if (action == "layout-verify-saved" && ok == "true") {
        layout_verify_saved_seen = 1
    }
}

/^\[fika autosmoke\] places hit-test / {
    sample = field("sample")
    kind = field("kind")
    zone = field("zone")
    ok = field("ok")
    if (sample == "row-before" && kind == "Row" && zone == "InsertBefore" && ok == "true") {
        hit_test_row_before_seen = 1
    } else if (sample == "row-body" && kind == "Row" && zone == "OnPlace" && ok == "true") {
        hit_test_row_body_seen = 1
    } else if (sample == "row-after" && kind == "Row" && zone == "InsertAfter" && ok == "true") {
        hit_test_row_after_seen = 1
    } else if (sample == "section" && kind == "Section" && zone == "Section" && ok == "true") {
        hit_test_section_seen = 1
    }
}

/^\[fika autosmoke\] places hit-test-summary / {
    ok = field("ok")
    rows = field("rows") + 0
    sections = field("sections") + 0
    if (ok == "true" && rows > 0 && sections > 0) {
        hit_test_summary_seen = 1
        max_update("hit_test_rows", rows)
        max_update("hit_test_sections", sections)
    }
}

/^\[fika autosmoke\] places targeting / {
    sample = field("sample")
    target = field("target")
    activatable = field("activatable")
    ok = field("ok")
    if (sample == "activation-row" && target == "ActivationRow" && activatable == "true" && ok == "true") {
        retained_targeting_activation_row_seen = 1
    } else if (sample == "context-row" && target == "ContextRow" && ok == "true") {
        retained_targeting_context_row_seen = 1
    } else if (sample == "context-section" && target == "ContextSection" && ok == "true") {
        retained_targeting_context_section_seen = 1
    }
}

/^\[fika autosmoke\] places targeting-summary / {
    ok = field("ok")
    rows = field("rows") + 0
    sections = field("sections") + 0
    if (ok == "true" && rows > 0 && sections > 0) {
        retained_targeting_summary_seen = 1
        max_update("retained_targeting_rows", rows)
        max_update("retained_targeting_sections", sections)
    }
}

/^\[fika autosmoke\] places dnd / {
    sample = field("sample")
    drag = field("drag")
    target = field("target")
    cursor = field("cursor")
    ok = field("ok")
    if (sample == "path-row-body" && drag == "path-list" && target == "Place" && cursor == "DropMenu" && ok == "true") {
        retained_dnd_path_row_body_seen = 1
    } else if (sample == "path-row-before" && drag == "path-list" && target == "Insert" && cursor == "Copy" && ok == "true") {
        retained_dnd_path_row_before_seen = 1
    } else if (sample == "path-section" && drag == "path-list" && target == "Insert" && cursor == "Copy" && ok == "true") {
        retained_dnd_path_section_seen = 1
    } else if (sample == "place-row-body" && drag == "place" && target == "Insert" && cursor == "Move" && ok == "true") {
        retained_dnd_place_row_body_seen = 1
    } else if (sample == "path-outside" && drag == "path-list" && target == "Clear" && cursor == "NotAllowed" && ok == "true") {
        retained_dnd_path_outside_seen = 1
    }
}

/^\[fika autosmoke\] places dnd-summary / {
    ok = field("ok")
    rows = field("rows") + 0
    sections = field("sections") + 0
    if (ok == "true" && rows > 1 && sections > 0) {
        retained_dnd_summary_seen = 1
        max_update("retained_dnd_rows", rows)
        max_update("retained_dnd_sections", sections)
    }
}

/^\[fika autosmoke\] places snapshot=/ {
    snapshot_label = field("snapshot")
    visible = field("visible") + 0
    place_targets = field("place_targets") + 0
    insert_before = field("insert_before") + 0
    insert_after = field("insert_after") + 0
    if (snapshot_label == "initial" && place_targets == 0 && insert_before == 0 && insert_after == 0) {
        autosmoke_initial_seen = 1
    } else if (snapshot_label == "after-place-target" && place_targets > 0 && insert_before == 0 && insert_after == 0) {
        autosmoke_after_place_target_seen = 1
    } else if (snapshot_label == "after-insert-start" && place_targets == 0 && insert_before > 0 && insert_after == 0) {
        autosmoke_after_insert_start_seen = 1
    } else if (snapshot_label == "after-insert-end" && place_targets == 0 && insert_before == 0 && insert_after > 0) {
        autosmoke_after_insert_end_seen = 1
    } else if (snapshot_label == "after-clear" && place_targets == 0 && insert_before == 0 && insert_after == 0) {
        autosmoke_after_clear_seen = 1
    } else if (snapshot_label == "overflow" && visible > 0) {
        overflow_autosmoke_snapshot_seen = 1
        max_update("overflow_visible", visible)
    }
}

END {
    if (places_view_frames == 0) {
        fail("missing [fika places-view] logs")
    }
    if (sidebar_frames == 0) {
        fail("missing [fika places-sidebar] logs")
    }
    if (slot_frames == 0) {
        fail("missing [fika places-slots] logs")
    }
    if (policy_frames == 0) {
        fail("missing [fika places-renderer-policy] logs")
    }
    if (!slot_inserted_frame_seen) {
        fail("missing initial inserted places slot frame")
    }
    if (!slot_unchanged_frame_seen) {
        fail("missing steady unchanged places slot frame")
    }
    if (expect_custom_row_visual_policy == "true") {
        if (custom_policy_invalid) {
            fail("places renderer policy does not match opt-in custom text row visual policy")
        }
        if (row_visual_frames == 0) {
            fail("missing [fika places-row-visual] logs for custom text row visual policy")
        }
        if (row_shape_cache_frames == 0) {
            fail("missing [fika places-row-shape-cache] logs for custom text row visual policy")
        }
        if (row_glyph_cache_frames == 0) {
            fail("missing [fika places-row-glyph-cache] logs for custom text row visual policy")
        }
        if (max_values["row_visual_rows"] != max_values["policy_rows"]) {
            fail("custom text Places row visual layer is not aggregated to the policy row count")
        }
    }
    if (expect_custom_row_full_policy == "true") {
        if (custom_full_policy_invalid) {
            fail("places renderer policy does not match opt-in full custom row visual policy")
        }
        if (row_visual_frames == 0) {
            fail("missing [fika places-row-visual] logs for full custom row visual policy")
        }
        if (row_shape_cache_frames == 0) {
            fail("missing [fika places-row-shape-cache] logs for full custom row visual policy")
        }
        if (row_glyph_cache_frames == 0) {
            fail("missing [fika places-row-glyph-cache] logs for full custom row visual policy")
        }
        if (max_values["row_visual_rows"] != max_values["policy_rows"]) {
            fail("full custom Places row visual layer is not aggregated to the policy row count")
        }
    }
    if (expect_custom_row_chrome_policy == "true") {
        if (custom_chrome_policy_invalid) {
            fail("places renderer policy does not match custom row chrome policy")
        }
        if (row_visual_frames == 0) {
            fail("missing [fika places-row-visual] logs for custom row chrome policy")
        }
        if (row_shape_cache_frames != 0) {
            fail("custom row chrome policy must not emit Places row shape-cache logs")
        }
        if (row_glyph_cache_frames != 0) {
            fail("custom row chrome policy must not emit Places row glyph-cache logs")
        }
        if (max_values["row_visual_rows"] != max_values["policy_rows"]) {
            fail("custom Places row chrome layer is not aggregated to the policy row count")
        }
    }
    if (expect_retained_event_policy == "true") {
        if (retained_event_renderer_policy_invalid) {
            fail("places renderer policy does not match retained event-delivery policy")
        }
        if (interaction_policy_frames == 0) {
            fail("missing [fika places-interaction-policy] logs for retained event-delivery policy")
        }
        if (retained_event_interaction_policy_invalid) {
            fail("Places interaction policy does not match retained event-delivery policy")
        }
        if (max_values["policy_row_visual_layer"] > 0) {
            if (row_visual_frames == 0) {
                fail("missing [fika places-row-visual] logs for retained custom row visual policy")
            }
            if ((policy_kind_text_seen || policy_kind_full_seen) && row_shape_cache_frames == 0) {
                fail("missing [fika places-row-shape-cache] logs for retained custom row visual policy")
            }
            if ((policy_kind_text_seen || policy_kind_full_seen) && row_glyph_cache_frames == 0) {
                fail("missing [fika places-row-glyph-cache] logs for retained custom row visual policy")
            }
            if (policy_kind_chrome_seen && row_shape_cache_frames != 0) {
                fail("retained custom row chrome policy must not emit Places row shape-cache logs")
            }
            if (policy_kind_chrome_seen && row_glyph_cache_frames != 0) {
                fail("retained custom row chrome policy must not emit Places row glyph-cache logs")
            }
            if (max_values["row_visual_rows"] != max_values["policy_rows"]) {
                fail("retained custom Places row visual layer is not aggregated to the policy row count")
            }
        }
    }
    if (snapshot_limit != "" && max_values["snapshot"] > snapshot_limit) {
        fail("places snapshot exceeded threshold: " max_values["snapshot"] "us > " snapshot_limit "us")
    }
    if (sidebar_build_limit != "" && max_values["sidebar_build"] > sidebar_build_limit) {
        fail("places sidebar build exceeded threshold: " max_values["sidebar_build"] "us > " sidebar_build_limit "us")
    }
    if (slot_project_limit != "" && max_values["slot_project"] > slot_project_limit) {
        fail("places slot projection exceeded threshold: " max_values["slot_project"] "us > " slot_project_limit "us")
    }
    if (row_visual_paint_limit != "" && max_values["row_visual_paint"] > row_visual_paint_limit) {
        fail("places row visual paint exceeded threshold: " max_values["row_visual_paint"] "us > " row_visual_paint_limit "us")
    }
    if (row_visual_prepaint_limit != "" && max_values["row_visual_prepaint"] > row_visual_prepaint_limit) {
        fail("places row visual prepaint exceeded threshold: " max_values["row_visual_prepaint"] "us > " row_visual_prepaint_limit "us")
    }
    if (row_visual_warm_paint_limit != "" && max_values["row_visual_warm_paint"] > row_visual_warm_paint_limit) {
        fail("places warm row visual paint exceeded threshold: " max_values["row_visual_warm_paint"] "us > " row_visual_warm_paint_limit "us")
    }
    if (row_visual_warm_prepaint_limit != "" && max_values["row_visual_warm_prepaint"] > row_visual_warm_prepaint_limit) {
        fail("places warm row visual prepaint exceeded threshold: " max_values["row_visual_warm_prepaint"] "us > " row_visual_warm_prepaint_limit "us")
    }
    if (render_total_limit != "" && max_values["render_total"] > render_total_limit) {
        fail("render total exceeded threshold: " max_values["render_total"] "us > " render_total_limit "us")
    }
    if (require_autosmoke == "true") {
        if (!autosmoke_start_seen || !autosmoke_complete_seen) {
            fail("missing Places autosmoke start/complete markers")
        }
        if (!autosmoke_target_action_seen || !autosmoke_insert_start_action_seen ||
            !autosmoke_insert_end_action_seen || !autosmoke_clear_action_seen) {
            fail("missing Places autosmoke action markers")
        }
        if (!autosmoke_initial_seen || !autosmoke_after_place_target_seen ||
            !autosmoke_after_insert_start_seen || !autosmoke_after_insert_end_seen ||
            !autosmoke_after_clear_seen) {
            fail("missing or invalid Places autosmoke snapshot projections")
        }
        if (!slot_visual_frame_seen) {
            fail("Places autosmoke did not produce a visual slot-change frame")
        }
    }
    if (require_overflow_autosmoke == "true") {
        if (!overflow_autosmoke_start_seen || !overflow_autosmoke_complete_seen) {
            fail("missing Places overflow autosmoke start/complete markers")
        }
        if (!overflow_autosmoke_snapshot_seen) {
            fail("missing Places overflow autosmoke snapshot")
        }
        if (!scrollbar_visible_seen || !scrollbar_overflow_seen) {
            fail("missing visible overflowing Places scrollbar evidence")
        }
    }
    if (require_layout_autosmoke == "true") {
        if (!layout_autosmoke_start_seen || !layout_autosmoke_complete_seen) {
            fail("missing Places layout autosmoke start/complete markers")
        }
        if (!layout_initial_seen || !layout_hide_seen || !layout_show_seen ||
            !layout_resize_seen || !layout_reset_seen || !layout_restore_seen ||
            !layout_verify_saved_seen) {
            fail("missing or invalid Places layout autosmoke action markers")
        }
    }
    if (require_hit_test_autosmoke == "true") {
        if (!hit_test_autosmoke_start_seen || !hit_test_autosmoke_complete_seen) {
            fail("missing Places retained hit-test autosmoke start/complete markers")
        }
        if (!hit_test_row_before_seen || !hit_test_row_body_seen ||
            !hit_test_row_after_seen || !hit_test_section_seen ||
            !hit_test_summary_seen) {
            fail("missing or invalid Places retained hit-test autosmoke markers")
        }
    }
    if (require_retained_targeting_autosmoke == "true") {
        if (!retained_targeting_autosmoke_start_seen || !retained_targeting_autosmoke_complete_seen) {
            fail("missing Places retained targeting autosmoke start/complete markers")
        }
        if (!retained_targeting_activation_row_seen ||
            !retained_targeting_context_row_seen ||
            !retained_targeting_context_section_seen ||
            !retained_targeting_summary_seen) {
            fail("missing or invalid Places retained targeting autosmoke markers")
        }
    }
    if (require_retained_dnd_autosmoke == "true") {
        if (!retained_dnd_autosmoke_start_seen || !retained_dnd_autosmoke_complete_seen) {
            fail("missing Places retained DnD autosmoke start/complete markers")
        }
        if (!retained_dnd_path_row_body_seen || !retained_dnd_path_row_before_seen ||
            !retained_dnd_path_section_seen || !retained_dnd_place_row_body_seen ||
            !retained_dnd_path_outside_seen || !retained_dnd_summary_seen) {
            fail("missing or invalid Places retained DnD autosmoke markers")
        }
    }
    if (require_interaction_policy == "true") {
        if (interaction_policy_frames == 0) {
            fail("missing [fika places-interaction-policy] logs")
        }
        if (expect_retained_event_policy == "true" && retained_event_interaction_policy_invalid) {
            fail("Places interaction policy does not match retained event-delivery policy")
        } else if (expect_retained_event_policy != "true" && current_interaction_policy_invalid) {
            fail("Places interaction policy does not match the retained target-decision / GPUI shell boundary")
        }
    }
    if (require_interaction_geometry == "true") {
        if (interaction_geometry_frames == 0) {
            fail("missing [fika places-interaction-geometry] logs")
        }
        if (interaction_geometry_invalid) {
            fail("Places interaction geometry does not match retained row/section projection counts")
        }
        if (max_values["interaction_geometry_rows"] != max_values["policy_rows"] ||
            max_values["interaction_geometry_sections"] != max_values["policy_sections"]) {
            fail("Places interaction geometry count does not match renderer policy")
        }
    }
    if (require_event_probe == "true") {
        if (event_probe_frames == 0) {
            fail("missing [fika places-event-probe] logs")
        }
        if (event_probe_invalid) {
            fail("Places event probe visible hitbox count does not match visible rows+sections")
        }
        if (max_values["event_probe_hitboxes"] > max_values["policy_retained_probe_hitboxes"]) {
            fail("Places event probe visible hitbox count exceeds retained-probe policy")
        }
    }
    if (exit_code) {
        exit exit_code
    }

    printf("places_view_frames=%d max_source=%d max_visible=%d max_sections=%d max_snapshot=%dus\n",
        places_view_frames,
        max_values["source"],
        max_values["visible"],
        max_values["sections"],
        max_values["snapshot"])
    printf("places_sidebar_frames=%d max_rows=%d max_sections=%d max_elements=%d max_build=%dus\n",
        sidebar_frames,
        max_values["sidebar_rows"],
        max_values["sidebar_sections"],
        max_values["sidebar_elements"],
        max_values["sidebar_build"])
    printf("places_slots_frames=%d max_rows=%d max_sections=%d max_entries=%d max_inserted=%d max_content=%d max_geometry=%d max_visual=%d max_unchanged=%d max_removed=%d max_project=%dus\n",
        slot_frames,
        max_values["slot_rows"],
        max_values["slot_sections"],
        max_values["slot_entries"],
        max_values["slot_inserted"],
        max_values["slot_content"],
        max_values["slot_geometry"],
        max_values["slot_visual"],
        max_values["slot_unchanged"],
        max_values["slot_removed"],
        max_values["slot_project"])
    printf("render_frames=%d max_panes=%d max_window_setup=%dus max_places=%dus max_tasks=%dus max_snapshots=%dus max_chrome_inputs=%dus max_chrome_state=%dus max_chrome_icons=%dus max_pane_elements=%dus max_overlays=%dus max_root=%dus max_total=%dus\n",
        render_frames,
        max_values["render_panes"],
        max_values["render_window_setup"],
        max_values["render_places"],
        max_values["render_tasks"],
        max_values["render_snapshots"],
        max_values["render_chrome_inputs"],
        max_values["render_chrome_state"],
        max_values["render_chrome_icons"],
        max_values["render_pane_elements"],
        max_values["render_overlays"],
        max_values["render_root"],
        max_values["render_total"])
    printf("render_at_max_total panes=%d total=%dus window_setup=%dus places=%dus tasks=%dus snapshots=%dus chrome_inputs=%dus chrome_state=%dus chrome_icons=%dus pane_elements=%dus overlays=%dus root=%dus accounted=%dus residual=%dus max_accounted=%dus max_residual=%dus\n",
        render_at_max_panes,
        render_at_max_total,
        render_at_max_window_setup,
        render_at_max_places,
        render_at_max_tasks,
        render_at_max_snapshots,
        render_at_max_chrome_inputs,
        render_at_max_chrome_state,
        render_at_max_chrome_icons,
        render_at_max_pane_elements,
        render_at_max_overlays,
        render_at_max_root,
        render_at_max_accounted,
        render_at_max_residual,
        max_values["render_accounted"],
        max_values["render_residual"])
    policy_kinds = ""
    if (policy_kind_gpui_seen) {
        policy_kinds = append_csv(policy_kinds, "gpui")
    }
    if (policy_kind_chrome_seen) {
        policy_kinds = append_csv(policy_kinds, "chrome")
    }
    if (policy_kind_text_seen) {
        policy_kinds = append_csv(policy_kinds, "text")
    }
    if (policy_kind_full_seen) {
        policy_kinds = append_csv(policy_kinds, "full")
    }
    if (policy_kind_other_seen) {
        policy_kinds = append_csv(policy_kinds, "other")
    }
    printf("places_renderer_policy_frames=%d max_rows=%d max_sections=%d max_row_gpui=%d max_row_visual_layer=%d max_icon_gpui=%d max_retained_interaction=%d max_drag_shell=%d max_section_gpui=%d max_scrollbar_canvas=%d max_text_gpui=%d visual_kinds=%s max_retained_probe_hitboxes=%d\n",
        policy_frames,
        max_values["policy_rows"],
        max_values["policy_sections"],
        max_values["policy_row_gpui"],
        max_values["policy_row_visual_layer"],
        max_values["policy_icon_gpui"],
        max_values["policy_retained_interaction"],
        max_values["policy_drag_shell"],
        max_values["policy_section_gpui"],
        max_values["policy_scrollbar_canvas"],
        max_values["policy_text_gpui"],
        policy_kinds,
        max_values["policy_retained_probe_hitboxes"])
    printf("places_interaction_policy_frames=%d max_rows=%d max_sections=%d max_row_target_decisions=%d max_section_target_decisions=%d max_retained_hitboxes=%d max_gpui_event_shells=%d max_gpui_row_section_event_shells=%d max_gpui_typed_dnd_payload_shells=%d max_drag_shells=%d max_retained_probe_hitboxes=%d max_retained_targeting=%d max_retained_dnd=%d max_drag_start_models=%d max_gpui_sidebar_leave_shells=%d\n",
        interaction_policy_frames,
        max_values["interaction_rows"],
        max_values["interaction_sections"],
        max_values["interaction_row_target_decisions"],
        max_values["interaction_section_target_decisions"],
        max_values["interaction_retained_hitboxes"],
        max_values["interaction_gpui_event_shells"],
        max_values["interaction_gpui_row_section_event_shells"],
        max_values["interaction_gpui_typed_dnd_payload_shells"],
        max_values["interaction_drag_shells"],
        max_values["interaction_retained_probe_hitboxes"],
        max_values["interaction_retained_targeting"],
        max_values["interaction_retained_dnd"],
        max_values["interaction_drag_start_models"],
        max_values["interaction_gpui_sidebar_leave_shells"])
    printf("places_interaction_geometry_frames=%d max_rows=%d max_sections=%d max_entries=%d max_content_height=%.1f max_hit_tests=%d max_project=%dus\n",
        interaction_geometry_frames,
        max_values["interaction_geometry_rows"],
        max_values["interaction_geometry_sections"],
        max_values["interaction_geometry_entries"],
        max_values["interaction_geometry_content_height"],
        max_values["interaction_geometry_hit_tests"],
        max_values["interaction_geometry_project"])
    printf("places_event_probe_frames=%d max_rows=%d max_sections=%d max_hitboxes=%d max_hovered=%d max_pointer=%d max_prepaint=%dus max_paint=%dus max_targeting=%d max_dnd=%d\n",
        event_probe_frames,
        max_values["event_probe_rows"],
        max_values["event_probe_sections"],
        max_values["event_probe_hitboxes"],
        max_values["event_probe_hovered"],
        max_values["event_probe_pointer"],
        max_values["event_probe_prepaint"],
        max_values["event_probe_paint"],
        max_values["event_probe_targeting"],
        max_values["event_probe_dnd"])
    printf("places_row_visual_frames=%d max_rows=%d max_painted=%d max_prepaint=%dus max_paint=%dus warm_frames=%d max_warm_prepaint=%dus max_warm_paint=%dus\n",
        row_visual_frames,
        max_values["row_visual_rows"],
        max_values["row_visual_painted"],
        max_values["row_visual_prepaint"],
        max_values["row_visual_paint"],
        row_visual_warm_frames,
        max_values["row_visual_warm_prepaint"],
        max_values["row_visual_warm_paint"])
    printf("places_icon_cache_refresh_frames=%d max_rows=%d requested=%d retained=%d loaded=%d decoded=%d missing=%d non_svg=%d max_requested=%d max_retained=%d max_loaded=%d max_decoded=%d max_missing=%d max_non_svg=%d max_total=%dus evicted=%d max_entries=%d max_bytes=%d max_evicted=%d\n",
        icon_cache_refresh_frames,
        max_values["icon_cache_refresh_rows"],
        icon_cache_refresh_requested,
        icon_cache_refresh_retained,
        icon_cache_refresh_loaded,
        icon_cache_refresh_decoded,
        icon_cache_refresh_missing,
        icon_cache_refresh_non_svg,
        max_values["icon_cache_refresh_requested"],
        max_values["icon_cache_refresh_retained"],
        max_values["icon_cache_refresh_loaded"],
        max_values["icon_cache_refresh_decoded"],
        max_values["icon_cache_refresh_missing"],
        max_values["icon_cache_refresh_non_svg"],
        max_values["icon_cache_refresh_total"],
        icon_cache_refresh_evicted,
        max_values["icon_cache_refresh_entries"],
        max_values["icon_cache_refresh_bytes"],
        max_values["icon_cache_refresh_evicted"])
    printf("places_row_shape_cache_frames=%d max_hits=%d max_misses=%d max_evicted=%d max_compute=%dus max_entries=%d\n",
        row_shape_cache_frames,
        max_values["row_shape_hits"],
        max_values["row_shape_misses"],
        max_values["row_shape_evicted"],
        max_values["row_shape_compute"],
        max_values["row_shape_entries"])
    printf("places_row_glyph_cache_frames=%d max_hits=%d max_misses=%d max_evicted=%d max_compute=%dus max_entries=%d\n",
        row_glyph_cache_frames,
        max_values["row_glyph_hits"],
        max_values["row_glyph_misses"],
        max_values["row_glyph_evicted"],
        max_values["row_glyph_compute"],
        max_values["row_glyph_entries"])
    printf("places_scrollbar_frames=%d max_visible=%d max_scroll_y=%.1f max_thumb_height=%.1f max_track_height=%.1f\n",
        scrollbar_frames,
        max_values["scrollbar_visible"],
        max_values["scrollbar_max_scroll_y"],
        max_values["scrollbar_thumb_height"],
        max_values["scrollbar_track_height"])
    printf("places_overflow_autosmoke start=%d complete=%d snapshot=%d max_visible=%d\n",
        overflow_autosmoke_start_seen,
        overflow_autosmoke_complete_seen,
        overflow_autosmoke_snapshot_seen,
        max_values["overflow_visible"])
    printf("places_layout_autosmoke start=%d complete=%d initial=%d hide=%d show=%d resize=%d reset=%d restore=%d verify_saved=%d\n",
        layout_autosmoke_start_seen,
        layout_autosmoke_complete_seen,
        layout_initial_seen,
        layout_hide_seen,
        layout_show_seen,
        layout_resize_seen,
        layout_reset_seen,
        layout_restore_seen,
        layout_verify_saved_seen)
    printf("places_hit_test_autosmoke start=%d complete=%d row_before=%d row_body=%d row_after=%d section=%d summary=%d max_rows=%d max_sections=%d\n",
        hit_test_autosmoke_start_seen,
        hit_test_autosmoke_complete_seen,
        hit_test_row_before_seen,
        hit_test_row_body_seen,
        hit_test_row_after_seen,
        hit_test_section_seen,
        hit_test_summary_seen,
        max_values["hit_test_rows"],
        max_values["hit_test_sections"])
    printf("places_retained_targeting_autosmoke start=%d complete=%d activation_row=%d context_row=%d context_section=%d summary=%d max_rows=%d max_sections=%d\n",
        retained_targeting_autosmoke_start_seen,
        retained_targeting_autosmoke_complete_seen,
        retained_targeting_activation_row_seen,
        retained_targeting_context_row_seen,
        retained_targeting_context_section_seen,
        retained_targeting_summary_seen,
        max_values["retained_targeting_rows"],
        max_values["retained_targeting_sections"])
    printf("places_retained_dnd_autosmoke start=%d complete=%d path_row_body=%d path_row_before=%d path_section=%d place_row_body=%d path_outside=%d summary=%d max_rows=%d max_sections=%d\n",
        retained_dnd_autosmoke_start_seen,
        retained_dnd_autosmoke_complete_seen,
        retained_dnd_path_row_body_seen,
        retained_dnd_path_row_before_seen,
        retained_dnd_path_section_seen,
        retained_dnd_place_row_body_seen,
        retained_dnd_path_outside_seen,
        retained_dnd_summary_seen,
        max_values["retained_dnd_rows"],
        max_values["retained_dnd_sections"])
    printf("places_autosmoke target=%d insert_start=%d insert_end=%d clear=%d snapshots=%d,%d,%d,%d,%d\n",
        autosmoke_target_action_seen,
        autosmoke_insert_start_action_seen,
        autosmoke_insert_end_action_seen,
        autosmoke_clear_action_seen,
        autosmoke_initial_seen,
        autosmoke_after_place_target_seen,
        autosmoke_after_insert_start_seen,
        autosmoke_after_insert_end_seen,
        autosmoke_after_clear_seen)
}
' "$log_path"
