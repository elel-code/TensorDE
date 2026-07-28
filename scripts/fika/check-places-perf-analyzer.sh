#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd "$script_dir/../../apps/fika" && pwd)"
analyzer="$script_dir/analyze-places-perf.sh"
tmpdir="$(mktemp -d)"
cleanup() {
    rm -rf "$tmpdir"
}
trap cleanup EXIT

bash -n "$analyzer"

cat > "$tmpdir/complete.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-view] source=11 visible=11 sections=2 snapshot=4457us
[fika places-sidebar] rows=11 sections=2 elements=13 build=232us
[fika places-renderer-policy] rows=11 row_gpui=11 row_visual_layer=0 icon_gpui=11 retained_interaction=0 drag_shell=0 section_gpui=2 scrollbar_canvas=1
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=0 gpui_event_shells=13 drag_shells=0
[fika places-interaction-geometry] rows=11 sections=2 entries=13 content_height=378 hit_tests=2 project=5us
[fika render] panes=1 viewport=1180x760 places=4457us tasks=30us snapshots=120us pane_elements=300us root=500us total=5400us
[fika autosmoke] places start scenario=DropTargets
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=89us
[fika places-sidebar] rows=11 sections=2 elements=13 build=186us
[fika places-renderer-policy] rows=11 row_gpui=11 row_visual_layer=0 icon_gpui=11 retained_interaction=0 drag_shell=0 section_gpui=2 scrollbar_canvas=1
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=0 gpui_event_shells=13 drag_shells=0
[fika autosmoke] places snapshot=initial visible=11 sections=2 active=1 place_targets=0 insert_before=0 insert_after=0
[fika autosmoke] places action=target-first-place target=/home/yk changed=true
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=1 unchanged=12 removed=0 project=22us
[fika places-view] source=11 visible=11 sections=2 snapshot=110us
[fika places-sidebar] rows=11 sections=2 elements=13 build=220us
[fika places-renderer-policy] rows=11 row_gpui=11 row_visual_layer=0 icon_gpui=11 retained_interaction=0 drag_shell=0 section_gpui=2 scrollbar_canvas=1
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=0 gpui_event_shells=13 drag_shells=0
[fika autosmoke] places snapshot=after-place-target visible=11 sections=2 active=1 place_targets=1 insert_before=0 insert_after=0
[fika autosmoke] places action=target-insert-start index=0 changed=true
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=1 unchanged=12 removed=0 project=40us
[fika places-view] source=11 visible=11 sections=2 snapshot=185us
[fika places-sidebar] rows=11 sections=2 elements=13 build=303us
[fika places-renderer-policy] rows=11 row_gpui=11 row_visual_layer=0 icon_gpui=11 retained_interaction=0 drag_shell=0 section_gpui=2 scrollbar_canvas=1
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=0 gpui_event_shells=13 drag_shells=0
[fika autosmoke] places snapshot=after-insert-start visible=11 sections=2 active=1 place_targets=0 insert_before=1 insert_after=0
[fika autosmoke] places action=target-insert-end index=11 changed=true
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=2 unchanged=11 removed=0 project=30us
[fika places-view] source=11 visible=11 sections=2 snapshot=120us
[fika places-sidebar] rows=11 sections=2 elements=13 build=225us
[fika places-renderer-policy] rows=11 row_gpui=11 row_visual_layer=0 icon_gpui=11 retained_interaction=0 drag_shell=0 section_gpui=2 scrollbar_canvas=1
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=0 gpui_event_shells=13 drag_shells=0
[fika autosmoke] places snapshot=after-insert-end visible=11 sections=2 active=1 place_targets=0 insert_before=0 insert_after=1
[fika autosmoke] places action=clear-targets changed=true
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=1 unchanged=12 removed=0 project=24us
[fika places-view] source=11 visible=11 sections=2 snapshot=110us
[fika places-sidebar] rows=11 sections=2 elements=13 build=224us
[fika places-renderer-policy] rows=11 row_gpui=11 row_visual_layer=0 icon_gpui=11 retained_interaction=0 drag_shell=0 section_gpui=2 scrollbar_canvas=1
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=0 gpui_event_shells=13 drag_shells=0
[fika autosmoke] places snapshot=after-clear visible=11 sections=2 active=1 place_targets=0 insert_before=0 insert_after=0
[fika autosmoke] places complete scenario=DropTargets
EOF

summary="$("$analyzer" \
    --require-autosmoke \
    --require-interaction-policy \
    --require-interaction-geometry \
    --snapshot-us 5000 \
    --sidebar-build-us 1000 \
    --slot-project-us 100 \
    --render-total-us 6000 \
    "$tmpdir/complete.log")"

if [[ "$summary" != *"places_view_frames="* || "$summary" != *"max_snapshot=4457us"* ]]; then
    echo "expected places-view summary" >&2
    exit 1
fi
if [[ "$summary" != *"places_slots_frames="* || "$summary" != *"max_visual=2"* ]]; then
    echo "expected places-slots summary" >&2
    exit 1
fi
if [[ "$summary" != *"render_frames=1 max_panes=1 max_window_setup=0us max_places=4457us max_tasks=30us max_snapshots=120us max_chrome_inputs=0us max_chrome_state=0us max_chrome_icons=0us max_pane_elements=300us max_overlays=0us max_root=500us max_total=5400us"* ]]; then
    echo "expected render summary" >&2
    exit 1
fi
if [[ "$summary" != *"render_at_max_total panes=1 total=5400us window_setup=0us places=4457us tasks=30us snapshots=120us chrome_inputs=0us chrome_state=0us chrome_icons=0us pane_elements=300us overlays=0us root=500us accounted=5407us residual=-7us"* ]]; then
    echo "expected render owner summary" >&2
    exit 1
fi
if [[ "$summary" != *"places_autosmoke target=1 insert_start=1 insert_end=1 clear=1 snapshots=1,1,1,1,1"* ]]; then
    echo "expected Places autosmoke summary" >&2
    exit 1
fi
if [[ "$summary" != *"places_interaction_policy_frames=6 max_rows=11 max_sections=2 max_row_target_decisions=11 max_section_target_decisions=2 max_retained_hitboxes=0 max_gpui_event_shells=13 max_gpui_row_section_event_shells=13 max_gpui_typed_dnd_payload_shells=0 max_drag_shells=0"* ]]; then
    echo "expected Places interaction policy summary" >&2
    exit 1
fi
if [[ "$summary" != *"max_drag_start_models=11"* ]]; then
    echo "expected Places drag-start model summary" >&2
    exit 1
fi
if [[ "$summary" != *"places_interaction_geometry_frames=1 max_rows=11 max_sections=2 max_entries=13 max_content_height=378.0 max_hit_tests=2 max_project=5us"* ]]; then
    echo "expected Places interaction geometry summary" >&2
    exit 1
fi
if [[ "$summary" != *"places_row_visual_frames=0"* ]]; then
    echo "expected default Places row visual summary" >&2
    exit 1
fi
if [[ "$summary" != *"places_icon_cache_refresh_frames=0"* ]]; then
    echo "expected default Places icon cache-refresh summary" >&2
    exit 1
fi
if [[ "$summary" != *"places_scrollbar_frames=0"* ]]; then
    echo "expected default Places scrollbar summary" >&2
    exit 1
fi

cat > "$tmpdir/custom-row-visual.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=240us
[fika places-renderer-policy] rows=11 row_gpui=0 row_visual_layer=11 icon_gpui=11 retained_interaction=0 drag_shell=0 section_gpui=0 scrollbar_canvas=1
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=0 gpui_event_shells=13 drag_shells=0
[fika places-row-visual] rows=11 painted=11 prepaint=20us paint=31us
[fika places-row-shape-cache] hits=11 misses=0 evicted=0 entries=11
[fika places-row-glyph-cache] hits=11 misses=0 evicted=0 entries=11
EOF

custom_summary="$("$analyzer" \
    --expect-custom-row-visual-policy \
    "$tmpdir/custom-row-visual.log")"

if [[ "$custom_summary" != *"max_row_gpui=0 max_row_visual_layer=11"* ]]; then
    echo "expected custom Places row visual policy summary" >&2
    exit 1
fi
if [[ "$custom_summary" != *"places_row_visual_frames=1 max_rows=11 max_painted=11 max_prepaint=20us max_paint=31us"* ]]; then
    echo "expected Places row visual paint summary" >&2
    exit 1
fi
if [[ "$custom_summary" != *"places_row_shape_cache_frames=1 max_hits=11 max_misses=0 max_evicted=0 max_compute=0us max_entries=11"* ]]; then
    echo "expected Places row shape-cache summary" >&2
    exit 1
fi
if [[ "$custom_summary" != *"places_row_glyph_cache_frames=1 max_hits=11 max_misses=0 max_evicted=0 max_compute=0us max_entries=11"* ]]; then
    echo "expected Places row glyph-cache summary" >&2
    exit 1
fi

cat > "$tmpdir/custom-row-full.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=240us
[fika places-icon-cache-refresh] rows=11 requested=11 retained=9 loaded=2 decoded=2 missing=0 non_svg=0 total=120us
[fika places-renderer-policy] rows=11 row_gpui=0 row_visual_layer=11 text_gpui=0 icon_gpui=0 retained_interaction=0 drag_shell=0 section_gpui=0 scrollbar_canvas=1 visual_kind=full
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=0 gpui_event_shells=13 drag_shells=0
[fika places-row-visual] rows=11 painted=11 prepaint=24us paint=35us
[fika places-row-shape-cache] hits=11 misses=0 evicted=0 entries=11
[fika places-row-glyph-cache] hits=11 misses=0 evicted=0 entries=11
EOF

full_summary="$("$analyzer" \
    --expect-custom-row-full-policy \
    "$tmpdir/custom-row-full.log")"

if [[ "$full_summary" != *"max_row_gpui=0 max_row_visual_layer=11 max_icon_gpui=0"* || "$full_summary" != *"max_text_gpui=0 visual_kinds=full"* ]]; then
    echo "expected full custom Places row policy summary" >&2
    exit 1
fi
if [[ "$full_summary" != *"places_icon_cache_refresh_frames=1 max_rows=11 requested=11 retained=9 loaded=2 decoded=2 missing=0 non_svg=0 max_requested=11 max_retained=9 max_loaded=2 max_decoded=2 max_missing=0 max_non_svg=0 max_total=120us"* ]]; then
    echo "expected full custom Places icon cache-refresh summary" >&2
    exit 1
fi

cat > "$tmpdir/custom-row-full-cold-then-warm.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=240us
[fika places-renderer-policy] rows=11 row_gpui=0 row_visual_layer=11 text_gpui=0 icon_gpui=0 retained_interaction=0 drag_shell=0 section_gpui=0 scrollbar_canvas=1 visual_kind=full
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=0 gpui_event_shells=13 drag_shells=0
[fika places-row-visual] rows=11 painted=11 prepaint=1200us paint=3500us
[fika places-row-shape-cache] hits=0 misses=11 evicted=0 entries=11
[fika places-row-glyph-cache] hits=0 misses=11 evicted=0 entries=11
[fika places-row-visual] rows=11 painted=11 prepaint=40us paint=2600us
[fika places-row-shape-cache] hits=11 misses=0 evicted=0 entries=11
[fika places-row-glyph-cache] hits=11 misses=0 evicted=0 entries=11
[fika places-row-visual] rows=11 painted=11 prepaint=30us paint=70us
[fika places-row-shape-cache] hits=11 misses=0 evicted=0 entries=11
[fika places-row-glyph-cache] hits=11 misses=0 evicted=0 entries=11
EOF

"$analyzer" \
    --expect-custom-row-full-policy \
    --row-visual-warm-paint-us 100 \
    "$tmpdir/custom-row-full-cold-then-warm.log" >/dev/null

if "$analyzer" \
    --expect-custom-row-full-policy \
    --row-visual-paint-us 1000 \
    "$tmpdir/custom-row-full-cold-then-warm.log" >/dev/null 2>&1; then
    echo "expected full custom Places cold paint gate to fail" >&2
    exit 1
fi

cat > "$tmpdir/custom-row-chrome.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=240us
[fika places-renderer-policy] rows=11 row_gpui=0 row_visual_layer=11 text_gpui=11 icon_gpui=11 retained_interaction=0 drag_shell=0 section_gpui=2 scrollbar_canvas=1 visual_kind=chrome
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=0 gpui_event_shells=13 drag_shells=0
[fika places-row-visual] rows=11 painted=11 prepaint=18us paint=24us
EOF

chrome_summary="$("$analyzer" \
    --expect-custom-row-chrome-policy \
    "$tmpdir/custom-row-chrome.log")"

if [[ "$chrome_summary" != *"max_row_gpui=0 max_row_visual_layer=11"* || "$chrome_summary" != *"max_text_gpui=11 visual_kinds=chrome"* ]]; then
    echo "expected custom Places row chrome policy summary" >&2
    exit 1
fi
if [[ "$chrome_summary" != *"places_row_shape_cache_frames=0"* ]]; then
    echo "expected Places chrome row policy without shape-cache summary" >&2
    exit 1
fi
if [[ "$chrome_summary" != *"places_row_glyph_cache_frames=0"* ]]; then
    echo "expected Places chrome row policy without glyph-cache summary" >&2
    exit 1
fi

cat > "$tmpdir/default-retained-dnd-chrome.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=240us
[fika places-renderer-policy] rows=11 row_gpui=0 row_visual_layer=11 text_gpui=11 icon_gpui=11 retained_interaction=13 drag_shell=0 section_gpui=2 scrollbar_canvas=1 visual_kind=chrome event_policy=retained-dnd retained_probe_hitboxes=13
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=13 retained_probe_hitboxes=13 gpui_event_shells=0 drag_shells=0 drag_start_models=11 event_policy=retained-dnd retained_targeting=13 retained_dnd=13
[fika places-row-visual] rows=11 painted=11 prepaint=18us paint=24us
EOF

default_retained_dnd_summary="$("$analyzer" \
    --require-interaction-policy \
    --expect-custom-row-chrome-policy \
    --expect-retained-event-policy \
    "$tmpdir/default-retained-dnd-chrome.log")"

if [[ "$default_retained_dnd_summary" != *"max_retained_interaction=13"* ||
    "$default_retained_dnd_summary" != *"max_retained_hitboxes=13 max_gpui_event_shells=0 max_gpui_row_section_event_shells=0 max_gpui_typed_dnd_payload_shells=0 max_drag_shells=0"* ||
    "$default_retained_dnd_summary" != *"max_retained_targeting=13 max_retained_dnd=13 max_drag_start_models=11"* ]]; then
    echo "expected default retained-DnD Places chrome policy summary" >&2
    exit 1
fi

cat > "$tmpdir/bad-retained-dnd-sidebar-leave-shells.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=240us
[fika places-renderer-policy] rows=11 row_gpui=0 row_visual_layer=11 text_gpui=11 icon_gpui=11 retained_interaction=13 drag_shell=0 section_gpui=2 scrollbar_canvas=1 visual_kind=chrome event_policy=retained-dnd retained_probe_hitboxes=13
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=13 retained_probe_hitboxes=13 gpui_event_shells=0 drag_shells=0 drag_start_models=11 gpui_sidebar_leave_shells=3 event_policy=retained-dnd retained_targeting=13 retained_dnd=13
[fika places-row-visual] rows=11 painted=11 prepaint=18us paint=24us
EOF

if "$analyzer" --require-interaction-policy "$tmpdir/bad-retained-dnd-sidebar-leave-shells.log" >/dev/null 2>&1; then
    echo "expected retained-DnD with GPUI sidebar leave shells to fail" >&2
    exit 1
fi

cat > "$tmpdir/bad-retained-dnd-row-section-shells.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=240us
[fika places-renderer-policy] rows=11 row_gpui=0 row_visual_layer=11 text_gpui=11 icon_gpui=11 retained_interaction=13 drag_shell=0 section_gpui=2 scrollbar_canvas=1 visual_kind=chrome event_policy=retained-dnd retained_probe_hitboxes=13
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=13 retained_probe_hitboxes=13 gpui_event_shells=13 gpui_row_section_event_shells=13 gpui_typed_dnd_payload_shells=0 drag_shells=0 drag_start_models=11 gpui_sidebar_leave_shells=0 event_policy=retained-dnd retained_targeting=13 retained_dnd=13
[fika places-row-visual] rows=11 painted=11 prepaint=18us paint=24us
EOF

if "$analyzer" --require-interaction-policy "$tmpdir/bad-retained-dnd-row-section-shells.log" >/dev/null 2>&1; then
    echo "expected retained-DnD with row/section GPUI event shells to fail" >&2
    exit 1
fi

cat > "$tmpdir/custom-row-chrome-with-shape-cache.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=240us
[fika places-renderer-policy] rows=11 row_gpui=0 row_visual_layer=11 text_gpui=11 icon_gpui=11 retained_interaction=0 drag_shell=0 section_gpui=2 scrollbar_canvas=1 visual_kind=chrome
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=0 gpui_event_shells=13 drag_shells=0
[fika places-row-visual] rows=11 painted=11 prepaint=18us paint=24us
[fika places-row-shape-cache] hits=1 misses=0 evicted=0 entries=1
EOF

if "$analyzer" --expect-custom-row-chrome-policy "$tmpdir/custom-row-chrome-with-shape-cache.log" >/dev/null 2>&1; then
    echo "expected chrome Places row policy with shape-cache to fail" >&2
    exit 1
fi

cat > "$tmpdir/retained-event-default-visual.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=240us
[fika places-renderer-policy] rows=11 row_gpui=11 row_visual_layer=0 icon_gpui=11 retained_interaction=13 drag_shell=0 section_gpui=2 scrollbar_canvas=1
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=13 gpui_event_shells=0 drag_shells=0
EOF

retained_default_summary="$("$analyzer" \
    --require-interaction-policy \
    --expect-retained-event-policy \
    "$tmpdir/retained-event-default-visual.log")"

if [[ "$retained_default_summary" != *"max_retained_interaction=13"* ]]; then
    echo "expected retained event renderer policy summary" >&2
    exit 1
fi
if [[ "$retained_default_summary" != *"max_retained_hitboxes=13 max_gpui_event_shells=0 max_gpui_row_section_event_shells=0 max_gpui_typed_dnd_payload_shells=0 max_drag_shells=0"* ]]; then
    echo "expected retained event interaction policy summary" >&2
    exit 1
fi

cat > "$tmpdir/retained-event-custom-visual.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=240us
[fika places-renderer-policy] rows=11 row_gpui=0 row_visual_layer=11 text_gpui=0 icon_gpui=11 retained_interaction=13 drag_shell=0 section_gpui=0 scrollbar_canvas=1 visual_kind=text
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=13 gpui_event_shells=0 drag_shells=0
[fika places-row-visual] rows=11 painted=11 prepaint=20us paint=31us
[fika places-row-shape-cache] hits=11 misses=0 evicted=0 entries=11
[fika places-row-glyph-cache] hits=11 misses=0 evicted=0 entries=11
EOF

retained_custom_summary="$("$analyzer" \
    --expect-retained-event-policy \
    "$tmpdir/retained-event-custom-visual.log")"

if [[ "$retained_custom_summary" != *"max_row_gpui=0 max_row_visual_layer=11"* ]]; then
    echo "expected retained event custom visual renderer summary" >&2
    exit 1
fi
if [[ "$retained_custom_summary" != *"places_row_visual_frames=1 max_rows=11 max_painted=11 max_prepaint=20us max_paint=31us"* ]]; then
    echo "expected retained event custom row visual summary" >&2
    exit 1
fi

cat > "$tmpdir/retained-event-mixed-gpui-shell.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=240us
[fika places-renderer-policy] rows=11 row_gpui=0 row_visual_layer=11 icon_gpui=11 retained_interaction=0 drag_shell=0 section_gpui=0 scrollbar_canvas=1
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=0 gpui_event_shells=13 drag_shells=0
[fika places-row-visual] rows=11 painted=11 prepaint=20us paint=31us
[fika places-row-shape-cache] hits=11 misses=0 evicted=0 entries=11
[fika places-row-glyph-cache] hits=11 misses=0 evicted=0 entries=11
EOF

if "$analyzer" --expect-retained-event-policy "$tmpdir/retained-event-mixed-gpui-shell.log" >/dev/null 2>&1; then
    echo "expected retained event policy with GPUI event shells to fail" >&2
    exit 1
fi

cat > "$tmpdir/retained-event-probe.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=240us
[fika places-renderer-policy] rows=11 row_gpui=0 row_visual_layer=11 text_gpui=11 icon_gpui=11 retained_interaction=0 drag_shell=0 section_gpui=2 scrollbar_canvas=1 visual_kind=chrome event_policy=retained-probe retained_probe_hitboxes=13
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=0 retained_probe_hitboxes=13 gpui_event_shells=13 drag_shells=0 event_policy=retained-probe
[fika places-event-probe] rows=11 sections=2 hitboxes=13 hovered=1 pointer=0 prepaint=40us paint=3us
[fika places-row-visual] rows=11 painted=11 prepaint=20us paint=31us
EOF

probe_summary="$("$analyzer" \
    --require-interaction-policy \
    --require-event-probe \
    --expect-custom-row-chrome-policy \
    "$tmpdir/retained-event-probe.log")"

if [[ "$probe_summary" != *"max_retained_probe_hitboxes=13"* ]]; then
    echo "expected retained event probe hitbox summary" >&2
    exit 1
fi
if [[ "$probe_summary" != *"places_event_probe_frames=1 max_rows=11 max_sections=2 max_hitboxes=13 max_hovered=1 max_pointer=0 max_prepaint=40us max_paint=3us"* ]]; then
    echo "expected retained event probe layer summary" >&2
    exit 1
fi
if "$analyzer" --expect-retained-event-policy "$tmpdir/retained-event-probe.log" >/dev/null 2>&1; then
    echo "expected retained event probe to fail the retained event policy gate" >&2
    exit 1
fi

cat > "$tmpdir/retained-event-pointer.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=240us
[fika places-renderer-policy] rows=11 row_gpui=0 row_visual_layer=11 text_gpui=11 icon_gpui=11 retained_interaction=0 drag_shell=0 section_gpui=2 scrollbar_canvas=1 visual_kind=chrome event_policy=retained-pointer retained_probe_hitboxes=13
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=0 retained_probe_hitboxes=13 gpui_event_shells=13 drag_shells=0 event_policy=retained-pointer
[fika places-event-probe] rows=11 sections=2 hitboxes=13 hovered=1 pointer=1 prepaint=44us paint=5us
[fika places-row-visual] rows=11 painted=11 prepaint=20us paint=31us
EOF

pointer_summary="$("$analyzer" \
    --require-interaction-policy \
    --require-event-probe \
    --expect-custom-row-chrome-policy \
    "$tmpdir/retained-event-pointer.log")"

if [[ "$pointer_summary" != *"places_event_probe_frames=1 max_rows=11 max_sections=2 max_hitboxes=13 max_hovered=1 max_pointer=1 max_prepaint=44us max_paint=5us"* ]]; then
    echo "expected retained pointer event layer summary" >&2
    exit 1
fi
if "$analyzer" --expect-retained-event-policy "$tmpdir/retained-event-pointer.log" >/dev/null 2>&1; then
    echo "expected retained pointer event layer to fail the retained event policy gate" >&2
    exit 1
fi

cat > "$tmpdir/retained-event-targeting.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=240us
[fika places-renderer-policy] rows=11 row_gpui=0 row_visual_layer=11 text_gpui=11 icon_gpui=11 retained_interaction=13 drag_shell=0 section_gpui=2 scrollbar_canvas=1 visual_kind=chrome event_policy=retained-targeting retained_probe_hitboxes=13
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=13 retained_probe_hitboxes=13 gpui_event_shells=13 drag_shells=0 event_policy=retained-targeting retained_targeting=13
[fika places-event-probe] rows=11 sections=2 hitboxes=13 hovered=1 pointer=1 targeting=1 prepaint=46us paint=6us
[fika places-row-visual] rows=11 painted=11 prepaint=20us paint=31us
EOF

targeting_summary="$("$analyzer" \
    --require-interaction-policy \
    --require-event-probe \
    --expect-custom-row-chrome-policy \
    "$tmpdir/retained-event-targeting.log")"

if [[ "$targeting_summary" != *"max_retained_hitboxes=13"* || "$targeting_summary" != *"max_retained_targeting=13"* ]]; then
    echo "expected retained targeting interaction policy summary" >&2
    exit 1
fi
if [[ "$targeting_summary" != *"places_event_probe_frames=1 max_rows=11 max_sections=2 max_hitboxes=13 max_hovered=1 max_pointer=1 max_prepaint=46us max_paint=6us max_targeting=1"* ]]; then
    echo "expected retained targeting event layer summary" >&2
    exit 1
fi
if "$analyzer" --expect-retained-event-policy "$tmpdir/retained-event-targeting.log" >/dev/null 2>&1; then
    echo "expected retained targeting event layer to fail the retained event policy gate" >&2
    exit 1
fi

cat > "$tmpdir/bad-retained-targeting-hitboxes.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=240us
[fika places-renderer-policy] rows=11 row_gpui=0 row_visual_layer=11 text_gpui=11 icon_gpui=11 retained_interaction=13 drag_shell=0 section_gpui=2 scrollbar_canvas=1 visual_kind=chrome event_policy=retained-targeting retained_probe_hitboxes=13
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=0 retained_probe_hitboxes=13 gpui_event_shells=13 drag_shells=0 event_policy=retained-targeting retained_targeting=13
EOF

if "$analyzer" --require-interaction-policy "$tmpdir/bad-retained-targeting-hitboxes.log" >/dev/null 2>&1; then
    echo "expected retained targeting without retained hitboxes to fail" >&2
    exit 1
fi

cat > "$tmpdir/retained-event-dnd.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=240us
[fika places-renderer-policy] rows=11 row_gpui=0 row_visual_layer=11 text_gpui=11 icon_gpui=11 retained_interaction=13 drag_shell=0 section_gpui=2 scrollbar_canvas=1 visual_kind=chrome event_policy=retained-dnd retained_probe_hitboxes=13
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=13 retained_probe_hitboxes=13 gpui_event_shells=0 drag_shells=0 event_policy=retained-dnd retained_targeting=13 retained_dnd=13
[fika places-event-probe] rows=11 sections=2 hitboxes=13 hovered=1 pointer=1 targeting=1 dnd=1 prepaint=48us paint=7us
[fika places-row-visual] rows=11 painted=11 prepaint=20us paint=31us
EOF

dnd_summary="$("$analyzer" \
    --require-interaction-policy \
    --require-event-probe \
    --expect-custom-row-chrome-policy \
    --expect-retained-event-policy \
    "$tmpdir/retained-event-dnd.log")"

if [[ "$dnd_summary" != *"max_retained_hitboxes=13 max_gpui_event_shells=0 max_gpui_row_section_event_shells=0 max_gpui_typed_dnd_payload_shells=0 max_drag_shells=0 max_retained_probe_hitboxes=13 max_retained_targeting=13 max_retained_dnd=13"* ]]; then
    echo "expected retained dnd interaction policy summary" >&2
    exit 1
fi
if [[ "$dnd_summary" != *"places_event_probe_frames=1 max_rows=11 max_sections=2 max_hitboxes=13 max_hovered=1 max_pointer=1 max_prepaint=48us max_paint=7us max_targeting=1 max_dnd=1"* ]]; then
    echo "expected retained dnd event layer summary" >&2
    exit 1
fi
cat > "$tmpdir/custom-row-visual-per-row.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=240us
[fika places-renderer-policy] rows=11 row_gpui=0 row_visual_layer=11 icon_gpui=11 retained_interaction=0 drag_shell=0 section_gpui=0 scrollbar_canvas=1
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=0 gpui_event_shells=13 drag_shells=0
[fika places-row-visual] rows=1 painted=1 prepaint=20us paint=31us
[fika places-row-shape-cache] hits=11 misses=0 evicted=0 entries=11
[fika places-row-glyph-cache] hits=11 misses=0 evicted=0 entries=11
EOF

if "$analyzer" --expect-custom-row-visual-policy "$tmpdir/custom-row-visual-per-row.log" >/dev/null 2>&1; then
    echo "expected per-row Places visual policy to fail" >&2
    exit 1
fi

cat > "$tmpdir/custom-row-visual-missing-shape-cache.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=240us
[fika places-renderer-policy] rows=11 row_gpui=0 row_visual_layer=11 icon_gpui=11 retained_interaction=0 drag_shell=0 section_gpui=0 scrollbar_canvas=1
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=0 gpui_event_shells=13 drag_shells=0
[fika places-row-visual] rows=11 painted=11 prepaint=20us paint=31us
EOF

if "$analyzer" --expect-custom-row-visual-policy "$tmpdir/custom-row-visual-missing-shape-cache.log" >/dev/null 2>&1; then
    echo "expected missing Places row shape-cache policy to fail" >&2
    exit 1
fi

cat > "$tmpdir/overflow.log" <<'EOF'
[fika places-slots] rows=75 sections=3 entries=78 inserted=78 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=90us
[fika places-view] source=11 visible=75 sections=3 snapshot=600us
[fika places-sidebar] rows=75 sections=3 elements=78 build=1400us
[fika places-renderer-policy] rows=75 row_gpui=75 row_visual_layer=0 icon_gpui=75 retained_interaction=0 drag_shell=0 section_gpui=3 scrollbar_canvas=1
[fika places-interaction-policy] rows=75 sections=3 row_target_decisions=75 section_target_decisions=3 retained_hitboxes=0 gpui_event_shells=78 drag_shells=0
[fika places-scrollbar] visible=1 max_scroll_y=1420 thumb_height=118 track_height=620
[fika autosmoke] places start scenario=Overflow
[fika places-slots] rows=75 sections=3 entries=78 inserted=0 content=0 geometry=0 visual=0 unchanged=78 removed=0 project=72us
[fika places-view] source=11 visible=75 sections=3 snapshot=510us
[fika autosmoke] places snapshot=overflow visible=75 sections=3 active=1 place_targets=0 insert_before=0 insert_after=0
[fika places-scrollbar] visible=1 max_scroll_y=1420 thumb_height=118 track_height=620
[fika autosmoke] places complete scenario=Overflow
EOF

overflow_summary="$("$analyzer" \
    --require-overflow-autosmoke \
    "$tmpdir/overflow.log")"

if [[ "$overflow_summary" != *"places_scrollbar_frames=2 max_visible=1 max_scroll_y=1420.0"* ]]; then
    echo "expected Places overflow scrollbar summary" >&2
    exit 1
fi
if [[ "$overflow_summary" != *"places_overflow_autosmoke start=1 complete=1 snapshot=1 max_visible=75"* ]]; then
    echo "expected Places overflow autosmoke summary" >&2
    exit 1
fi

cat > "$tmpdir/layout.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=200us
[fika places-renderer-policy] rows=11 row_gpui=11 row_visual_layer=0 icon_gpui=11 retained_interaction=0 drag_shell=0 section_gpui=2 scrollbar_canvas=1
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=0 gpui_event_shells=13 drag_shells=0
[fika autosmoke] places start scenario=Layout
[fika autosmoke] places action=layout-initial width=220.0 visible=true
[fika autosmoke] places action=layout-hide width=220.0 visible=false changed=true
[fika autosmoke] places action=layout-show width=220.0 visible=true changed=true
[fika autosmoke] places action=layout-resize width=320.0 visible=true target_width=320.0 changed=true
[fika autosmoke] places action=layout-reset width=220.0 visible=true changed=true
[fika autosmoke] places action=layout-restore width=220.0 visible=true changed=false
[fika autosmoke] places action=layout-verify-saved width=220.0 visible=true saved_width=220.0 saved_visible=true ok=true path=/tmp/fika-settings/settings.tsv
[fika autosmoke] places complete scenario=Layout
EOF

layout_summary="$("$analyzer" \
    --require-layout-autosmoke \
    "$tmpdir/layout.log")"

if [[ "$layout_summary" != *"places_layout_autosmoke start=1 complete=1 initial=1 hide=1 show=1 resize=1 reset=1 restore=1 verify_saved=1"* ]]; then
    echo "expected Places layout autosmoke summary" >&2
    exit 1
fi

cat > "$tmpdir/hit-test.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=200us
[fika places-renderer-policy] rows=11 row_gpui=11 row_visual_layer=0 icon_gpui=11 retained_interaction=0 drag_shell=0 section_gpui=2 scrollbar_canvas=1
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=0 gpui_event_shells=13 drag_shells=0
[fika places-interaction-geometry] rows=11 sections=2 entries=13 content_height=378 hit_tests=2 project=5us
[fika autosmoke] places start scenario=HitTest
[fika autosmoke] places hit-test label=retained-hit-test sample=row-before y=19.0 kind=Row zone=InsertBefore visible_index=0 insert_index=0 ok=true
[fika autosmoke] places hit-test label=retained-hit-test sample=row-body y=36.0 kind=Row zone=OnPlace visible_index=0 insert_index=1 ok=true
[fika autosmoke] places hit-test label=retained-hit-test sample=row-after y=53.0 kind=Row zone=InsertAfter visible_index=0 insert_index=1 ok=true
[fika autosmoke] places hit-test label=retained-hit-test sample=section y=1.0 kind=Section zone=Section visible_index=<none> insert_index=0 ok=true
[fika autosmoke] places hit-test-summary label=retained-hit-test rows=11 sections=2 ok=true
[fika autosmoke] places complete scenario=HitTest
EOF

hit_test_summary="$("$analyzer" \
    --require-hit-test-autosmoke \
    --require-interaction-policy \
    --require-interaction-geometry \
    "$tmpdir/hit-test.log")"

if [[ "$hit_test_summary" != *"places_hit_test_autosmoke start=1 complete=1 row_before=1 row_body=1 row_after=1 section=1 summary=1 max_rows=11 max_sections=2"* ]]; then
    echo "expected Places retained hit-test autosmoke summary" >&2
    exit 1
fi

cat > "$tmpdir/bad-hit-test.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=200us
[fika places-renderer-policy] rows=11 row_gpui=11 row_visual_layer=0 icon_gpui=11 retained_interaction=0 drag_shell=0 section_gpui=2 scrollbar_canvas=1
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=0 gpui_event_shells=13 drag_shells=0
[fika places-interaction-geometry] rows=11 sections=2 entries=13 content_height=378 hit_tests=2 project=5us
[fika autosmoke] places start scenario=HitTest
[fika autosmoke] places hit-test label=retained-hit-test sample=row-before y=19.0 kind=Row zone=InsertBefore visible_index=0 insert_index=0 ok=true
[fika autosmoke] places hit-test label=retained-hit-test sample=row-body y=36.0 kind=Row zone=InsertAfter visible_index=0 insert_index=1 ok=false
[fika autosmoke] places hit-test label=retained-hit-test sample=row-after y=53.0 kind=Row zone=InsertAfter visible_index=0 insert_index=1 ok=true
[fika autosmoke] places hit-test label=retained-hit-test sample=section y=1.0 kind=Section zone=Section visible_index=<none> insert_index=0 ok=true
[fika autosmoke] places hit-test-summary label=retained-hit-test rows=11 sections=2 ok=false
[fika autosmoke] places complete scenario=HitTest
EOF

if "$analyzer" --require-hit-test-autosmoke "$tmpdir/bad-hit-test.log" >/dev/null 2>&1; then
    echo "expected invalid Places retained hit-test autosmoke to fail" >&2
    exit 1
fi

cat > "$tmpdir/retained-targeting-autosmoke.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=200us
[fika places-renderer-policy] rows=11 row_gpui=0 row_visual_layer=11 text_gpui=11 icon_gpui=11 retained_interaction=13 drag_shell=0 section_gpui=2 scrollbar_canvas=1 visual_kind=chrome event_policy=retained-targeting retained_probe_hitboxes=13
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=13 retained_probe_hitboxes=13 gpui_event_shells=13 drag_shells=0 event_policy=retained-targeting retained_targeting=13
[fika places-interaction-geometry] rows=11 sections=2 entries=13 content_height=378 hit_tests=2 project=5us
[fika autosmoke] places start scenario=RetainedTargeting
[fika autosmoke] places targeting label=retained-targeting sample=activation-row y=36.0 target=ActivationRow visible_index=0 group= activatable=true ok=true
[fika autosmoke] places targeting label=retained-targeting sample=context-row y=36.0 target=ContextRow visible_index=0 group= activatable=true ok=true
[fika autosmoke] places targeting label=retained-targeting sample=context-section y=61.0 target=ContextSection visible_index=<none> group=Devices activatable=false ok=true
[fika autosmoke] places targeting-summary label=retained-targeting rows=11 sections=2 ok=true
[fika autosmoke] places complete scenario=RetainedTargeting
[fika places-event-probe] rows=11 sections=2 hitboxes=13 hovered=1 pointer=1 targeting=1 prepaint=46us paint=6us
[fika places-row-visual] rows=11 painted=11 prepaint=20us paint=31us
EOF

retained_targeting_autosmoke_summary="$("$analyzer" \
    --require-retained-targeting-autosmoke \
    --require-interaction-policy \
    --require-interaction-geometry \
    --require-event-probe \
    --expect-custom-row-chrome-policy \
    "$tmpdir/retained-targeting-autosmoke.log")"

if [[ "$retained_targeting_autosmoke_summary" != *"places_retained_targeting_autosmoke start=1 complete=1 activation_row=1 context_row=1 context_section=1 summary=1 max_rows=11 max_sections=2"* ]]; then
    echo "expected Places retained targeting autosmoke summary" >&2
    exit 1
fi

cat > "$tmpdir/bad-retained-targeting-autosmoke.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=200us
[fika places-renderer-policy] rows=11 row_gpui=0 row_visual_layer=11 text_gpui=11 icon_gpui=11 retained_interaction=13 drag_shell=0 section_gpui=2 scrollbar_canvas=1 visual_kind=chrome event_policy=retained-targeting retained_probe_hitboxes=13
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=13 retained_probe_hitboxes=13 gpui_event_shells=13 drag_shells=0 event_policy=retained-targeting retained_targeting=13
[fika autosmoke] places start scenario=RetainedTargeting
[fika autosmoke] places targeting label=retained-targeting sample=activation-row y=36.0 target=ContextRow visible_index=0 group= activatable=true ok=false
[fika autosmoke] places targeting label=retained-targeting sample=context-row y=36.0 target=ContextRow visible_index=0 group= activatable=true ok=true
[fika autosmoke] places targeting label=retained-targeting sample=context-section y=61.0 target=ContextSection visible_index=<none> group=Devices activatable=false ok=true
[fika autosmoke] places targeting-summary label=retained-targeting rows=11 sections=2 ok=false
[fika autosmoke] places complete scenario=RetainedTargeting
EOF

if "$analyzer" --require-retained-targeting-autosmoke "$tmpdir/bad-retained-targeting-autosmoke.log" >/dev/null 2>&1; then
    echo "expected invalid Places retained targeting autosmoke to fail" >&2
    exit 1
fi

cat > "$tmpdir/retained-dnd-autosmoke.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=200us
[fika places-renderer-policy] rows=11 row_gpui=0 row_visual_layer=11 text_gpui=11 icon_gpui=11 retained_interaction=13 drag_shell=0 section_gpui=2 scrollbar_canvas=1 visual_kind=chrome event_policy=retained-dnd retained_probe_hitboxes=13
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=13 retained_probe_hitboxes=13 gpui_event_shells=0 drag_shells=0 event_policy=retained-dnd retained_targeting=13 retained_dnd=13
[fika places-interaction-geometry] rows=11 sections=2 entries=13 content_height=378 hit_tests=2 project=5us
[fika autosmoke] places start scenario=RetainedDnd
[fika autosmoke] places dnd label=retained-dnd sample=path-row-body drag=path-list y=36.0 target=Place cursor=DropMenu ok=true
[fika autosmoke] places dnd label=retained-dnd sample=path-row-before drag=path-list y=19.0 target=Insert cursor=Copy ok=true
[fika autosmoke] places dnd label=retained-dnd sample=path-section drag=path-list y=61.0 target=Insert cursor=Copy ok=true
[fika autosmoke] places dnd label=retained-dnd sample=place-row-body drag=place y=92.0 target=Insert cursor=Move ok=true
[fika autosmoke] places dnd label=retained-dnd sample=path-outside drag=path-list y=386.0 target=Clear cursor=NotAllowed ok=true
[fika autosmoke] places dnd-summary label=retained-dnd rows=11 sections=2 ok=true
[fika autosmoke] places complete scenario=RetainedDnd
[fika places-row-visual] rows=11 painted=11 prepaint=20us paint=31us
EOF

retained_dnd_autosmoke_summary="$("$analyzer" \
    --require-retained-dnd-autosmoke \
    --require-interaction-policy \
    --require-interaction-geometry \
    --expect-custom-row-chrome-policy \
    "$tmpdir/retained-dnd-autosmoke.log")"

if [[ "$retained_dnd_autosmoke_summary" != *"places_retained_dnd_autosmoke start=1 complete=1 path_row_body=1 path_row_before=1 path_section=1 place_row_body=1 path_outside=1 summary=1 max_rows=11 max_sections=2"* ]]; then
    echo "expected Places retained DnD autosmoke summary" >&2
    exit 1
fi

cat > "$tmpdir/bad-retained-dnd-autosmoke.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=200us
[fika places-renderer-policy] rows=11 row_gpui=0 row_visual_layer=11 text_gpui=11 icon_gpui=11 retained_interaction=13 drag_shell=0 section_gpui=2 scrollbar_canvas=1 visual_kind=chrome event_policy=retained-dnd retained_probe_hitboxes=13
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=13 retained_probe_hitboxes=13 gpui_event_shells=0 drag_shells=0 event_policy=retained-dnd retained_targeting=13 retained_dnd=13
[fika autosmoke] places start scenario=RetainedDnd
[fika autosmoke] places dnd label=retained-dnd sample=path-row-body drag=path-list y=36.0 target=Insert cursor=Copy ok=false
[fika autosmoke] places dnd label=retained-dnd sample=path-row-before drag=path-list y=19.0 target=Insert cursor=Copy ok=true
[fika autosmoke] places dnd label=retained-dnd sample=path-section drag=path-list y=61.0 target=Insert cursor=Copy ok=true
[fika autosmoke] places dnd label=retained-dnd sample=place-row-body drag=place y=92.0 target=Insert cursor=Move ok=true
[fika autosmoke] places dnd label=retained-dnd sample=path-outside drag=path-list y=386.0 target=Clear cursor=NotAllowed ok=true
[fika autosmoke] places dnd-summary label=retained-dnd rows=11 sections=2 ok=false
[fika autosmoke] places complete scenario=RetainedDnd
EOF

if "$analyzer" --require-retained-dnd-autosmoke "$tmpdir/bad-retained-dnd-autosmoke.log" >/dev/null 2>&1; then
    echo "expected invalid Places retained DnD autosmoke to fail" >&2
    exit 1
fi

cat > "$tmpdir/bad-layout.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=200us
[fika places-renderer-policy] rows=11 row_gpui=11 row_visual_layer=0 icon_gpui=11 retained_interaction=0 drag_shell=0 section_gpui=2 scrollbar_canvas=1
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=0 gpui_event_shells=13 drag_shells=0
[fika autosmoke] places start scenario=Layout
[fika autosmoke] places action=layout-initial width=220.0 visible=true
[fika autosmoke] places action=layout-hide width=220.0 visible=false changed=true
[fika autosmoke] places action=layout-show width=220.0 visible=true changed=true
[fika autosmoke] places action=layout-resize width=320.0 visible=true target_width=320.0 changed=true
[fika autosmoke] places action=layout-reset width=220.0 visible=true changed=true
[fika autosmoke] places action=layout-restore width=220.0 visible=true changed=false
[fika autosmoke] places action=layout-verify-saved width=220.0 visible=true saved_width=220.0 saved_visible=true ok=false path=/tmp/fika-settings/settings.tsv
[fika autosmoke] places complete scenario=Layout
EOF

if "$analyzer" --require-layout-autosmoke "$tmpdir/bad-layout.log" >/dev/null 2>&1; then
    echo "expected invalid Places layout autosmoke to fail" >&2
    exit 1
fi

cat > "$tmpdir/missing-slots.log" <<'EOF'
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=200us
[fika places-renderer-policy] rows=11 row_gpui=11 row_visual_layer=0 icon_gpui=11 retained_interaction=0 drag_shell=0 section_gpui=2 scrollbar_canvas=1
EOF

if "$analyzer" "$tmpdir/missing-slots.log" >/dev/null 2>&1; then
    echo "expected missing places-slots channel to fail" >&2
    exit 1
fi

cat > "$tmpdir/no-unchanged.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=200us
[fika places-renderer-policy] rows=11 row_gpui=11 row_visual_layer=0 icon_gpui=11 retained_interaction=0 drag_shell=0 section_gpui=2 scrollbar_canvas=1
EOF

if "$analyzer" "$tmpdir/no-unchanged.log" >/dev/null 2>&1; then
    echo "expected missing unchanged slot frame to fail" >&2
    exit 1
fi

cat > "$tmpdir/bad-policy.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=200us
[fika places-renderer-policy] rows=11 row_gpui=10 row_visual_layer=1 icon_gpui=11 retained_interaction=1 drag_shell=0 section_gpui=2 scrollbar_canvas=1
EOF

if "$analyzer" --expect-custom-row-visual-policy "$tmpdir/bad-policy.log" >/dev/null 2>&1; then
    echo "expected invalid custom row visual policy to fail" >&2
    exit 1
fi

cat > "$tmpdir/bad-interaction-policy.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=200us
[fika places-renderer-policy] rows=11 row_gpui=11 row_visual_layer=0 icon_gpui=11 retained_interaction=0 drag_shell=0 section_gpui=2 scrollbar_canvas=1
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=10 section_target_decisions=2 retained_hitboxes=1 gpui_event_shells=12 drag_shells=0
EOF

if "$analyzer" --require-interaction-policy "$tmpdir/bad-interaction-policy.log" >/dev/null 2>&1; then
    echo "expected invalid Places interaction policy to fail" >&2
    exit 1
fi

cat > "$tmpdir/bad-drag-start-model-policy.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=200us
[fika places-renderer-policy] rows=11 row_gpui=11 row_visual_layer=0 icon_gpui=11 retained_interaction=0 drag_shell=0 section_gpui=2 scrollbar_canvas=1
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=0 gpui_event_shells=13 drag_shells=0 drag_start_models=10
EOF

if "$analyzer" --require-interaction-policy "$tmpdir/bad-drag-start-model-policy.log" >/dev/null 2>&1; then
    echo "expected invalid Places drag-start model policy to fail" >&2
    exit 1
fi

cat > "$tmpdir/bad-interaction-geometry.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=200us
[fika places-renderer-policy] rows=11 row_gpui=11 row_visual_layer=0 icon_gpui=11 retained_interaction=0 drag_shell=0 section_gpui=2 scrollbar_canvas=1
[fika places-interaction-policy] rows=11 sections=2 row_target_decisions=11 section_target_decisions=2 retained_hitboxes=0 gpui_event_shells=13 drag_shells=0
[fika places-interaction-geometry] rows=10 sections=2 entries=11 content_height=0 hit_tests=0 project=5us
EOF

if "$analyzer" --require-interaction-geometry "$tmpdir/bad-interaction-geometry.log" >/dev/null 2>&1; then
    echo "expected invalid Places interaction geometry to fail" >&2
    exit 1
fi

cat > "$tmpdir/missing-autosmoke.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=200us
[fika places-renderer-policy] rows=11 row_gpui=11 row_visual_layer=0 icon_gpui=11 retained_interaction=0 drag_shell=0 section_gpui=2 scrollbar_canvas=1
EOF

if "$analyzer" --require-autosmoke "$tmpdir/missing-autosmoke.log" >/dev/null 2>&1; then
    echo "expected missing autosmoke markers to fail" >&2
    exit 1
fi

if "$analyzer" --require-overflow-autosmoke "$tmpdir/missing-autosmoke.log" >/dev/null 2>&1; then
    echo "expected missing overflow autosmoke markers to fail" >&2
    exit 1
fi

if "$analyzer" --require-layout-autosmoke "$tmpdir/missing-autosmoke.log" >/dev/null 2>&1; then
    echo "expected missing layout autosmoke markers to fail" >&2
    exit 1
fi

if "$analyzer" --require-hit-test-autosmoke "$tmpdir/missing-autosmoke.log" >/dev/null 2>&1; then
    echo "expected missing hit-test autosmoke markers to fail" >&2
    exit 1
fi

if "$analyzer" --require-interaction-policy "$tmpdir/missing-autosmoke.log" >/dev/null 2>&1; then
    echo "expected missing Places interaction policy to fail" >&2
    exit 1
fi

if "$analyzer" --require-interaction-geometry "$tmpdir/missing-autosmoke.log" >/dev/null 2>&1; then
    echo "expected missing Places interaction geometry to fail" >&2
    exit 1
fi

if "$analyzer" --snapshot-us 100 "$tmpdir/complete.log" >/dev/null 2>&1; then
    echo "expected snapshot threshold violation to fail" >&2
    exit 1
fi

if "$analyzer" --sidebar-build-us 100 "$tmpdir/complete.log" >/dev/null 2>&1; then
    echo "expected sidebar threshold violation to fail" >&2
    exit 1
fi

if "$analyzer" --slot-project-us 10 "$tmpdir/complete.log" >/dev/null 2>&1; then
    echo "expected slot threshold violation to fail" >&2
    exit 1
fi

: > "$tmpdir/empty.log"
if "$analyzer" "$tmpdir/empty.log" >/dev/null 2>&1; then
    echo "expected empty log to fail" >&2
    exit 1
fi

echo "places perf analyzer check passed"
