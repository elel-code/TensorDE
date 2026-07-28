#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd "$script_dir/../../apps/fika" && pwd)"
analyzer="$script_dir/analyze-item-view-perf.sh"
runtime_gate="$script_dir/check-item-view-runtime-log.sh"
renderer_evidence="$script_dir/summarize-item-view-renderer-evidence.sh"
image_renderer_compare="$script_dir/compare-item-image-renderers.sh"
tmpdir="$(mktemp -d)"
cleanup() {
    rm -rf "$tmpdir"
}
trap cleanup EXIT

bash -n "$analyzer"
bash -n "$runtime_gate"
bash -n "$renderer_evidence"
bash -n "$image_renderer_compare"

cat > "$tmpdir/complete.log" <<'EOF'
[fika autosmoke] item-view start pane=1 scenario=ZoomScroll
[fika item-view] pane=1 mode=Compact phase=steady items=48 visible=32 raw=50us icon_sync=2us queue=1us convert=40us total=120us
[fika item-view] pane=1 mode=Icons phase=steady items=48 visible=40 raw=45us icon_sync=3us queue=1us convert=35us total=110us
[fika item-view] pane=1 mode=Details phase=steady items=48 visible=30 raw=42us icon_sync=1us queue=1us convert=32us total=105us
[fika autosmoke] item-view action=zoom-in pane=1
[fika autosmoke] item-view action=zoom-out pane=1
[fika autosmoke] item-view action=scroll-forward pane=1 changed=true
[fika autosmoke] item-view action=scroll-back pane=1 changed=true
[fika file-grid] pane=1 mode=Compact visible=32 content=1602.5x882 build=400us
[fika file-grid] pane=1 mode=Icons visible=40 content=587x1168 build=450us
[fika file-grid] pane=1 mode=Details visible=30 content=601x882 build=420us
[fika icon-sync] pane=1 mode=Compact candidates=32 cached=20 queued=8 resolved=4 changed=4 budget_exhausted=false total=70us
[fika icon-sync] pane=1 mode=Icons candidates=40 cached=36 queued=0 resolved=4 changed=4 budget_exhausted=false total=90us
[fika item-paint-slots] pane=1 mode=Compact inserted=32 content=0 geometry=0 visual=0 unchanged=0 removed=0 entries=32
[fika item-paint-slots] pane=1 mode=Icons inserted=8 content=0 geometry=32 visual=0 unchanged=0 removed=0 entries=40
[fika item-paint-slots] pane=1 mode=Details inserted=30 content=0 geometry=0 visual=0 unchanged=0 removed=40 entries=30
[fika static-item-visual] pane=1 mode=Compact prepaint_count=32 prepaint=180us paint_count=32 paint=160us
[fika static-item-visual] pane=1 mode=Icons prepaint_count=40 prepaint=210us paint_count=40 paint=190us
[fika item-shape-cache] pane=1 mode=Compact hits=30 misses=2 evicted=0 compute=40us entries=32
[fika item-glyph-cache] pane=1 mode=Compact hits=28 misses=4 evicted=0 entries=32
[fika item-glyph-budget] pane=1 mode=Compact requested=32 hits=28 misses=4 computed=4 deferred=0 failed=0 budget_exhausted=false compute=80us
[fika item-shape-cache] pane=1 mode=Icons hits=38 misses=2 evicted=0 compute=45us entries=40
[fika item-glyph-cache] pane=1 mode=Icons hits=36 misses=4 evicted=0 entries=40
[fika item-glyph-budget] pane=1 mode=Icons requested=40 hits=36 misses=4 computed=4 deferred=0 failed=0 budget_exhausted=false compute=90us
[fika item-image-cache-refresh] pane=1 mode=Icons requested=8 retained=5 loaded=3 decoded=1 missing=0 non_svg=0 total=95us
[fika item-image] pane=1 mode=Icons prepaint_count=8 prepaint=70us paint_count=8 paint=80us theme_loaded=0 theme_decoded=0 theme_retained=8 theme_placeholder=0 thumb_loaded=2 thumb_decoded=1 thumb_retained=0 thumb_fallback=0
[fika details-visual] pane=1 mode=Details prepaint_count=48 prepaint=120us paint_count=48 paint=130us
[fika details-visual] pane=1 mode=Details prepaint_count=48 prepaint=110us paint_count=48 paint=120us
[fika details-shape-cache] pane=1 mode=Details hits=20 misses=2 evicted=0 compute=50us entries=22
[fika details-glyph-cache] pane=1 mode=Details hits=18 misses=4 evicted=0 entries=22
[fika details-glyph-budget] pane=1 mode=Details requested=22 hits=18 misses=4 computed=4 deferred=0 failed=0 budget_exhausted=false compute=110us
[fika item-interaction] pane=1 mode=Details prepaint_count=48 prepaint=60us paint_count=48 paint=50us
[fika renderer-policy] pane=1 mode=Compact items=48 visual_layer=48 image_layer=8 gpui_image_element=0 retained_interaction=48 gpui_drag_shell=0 rename_overlay=0
[fika renderer-policy] pane=1 mode=Icons items=48 visual_layer=48 image_layer=8 gpui_image_element=0 retained_interaction=48 gpui_drag_shell=0 rename_overlay=0
[fika renderer-policy] pane=1 mode=Details items=48 visual_layer=48 image_layer=0 gpui_image_element=0 retained_interaction=48 gpui_drag_shell=0 rename_overlay=0
[fika autosmoke] item-view complete pane=1 scenario=ZoomScroll
EOF

"$analyzer" \
    --require-steady \
    --require-autosmoke \
    --require-details \
    --require-static-visual \
    --require-static-modes Compact,Icons \
    --require-interaction \
    --require-renderer-policy \
    --expect-retained-item-policy \
    --require-paint-slots \
    --require-renderer-policy-modes Compact,Icons,Details \
    --require-modes Compact,Icons,Details \
    --steady-total-us 1000 \
    --file-grid-build-us 3000 \
    --static-visual-paint-us 1000 \
    --image-paint-us 1000 \
    --custom-paint-us 1000 \
    "$tmpdir/complete.log" >/dev/null

"$runtime_gate" "$tmpdir/complete.log" >/dev/null

evidence="$("$renderer_evidence" "$tmpdir/complete.log")"
if [[ "$evidence" != *"## Item View Renderer Evidence"* ]]; then
    echo "expected renderer evidence heading" >&2
    exit 1
fi
if [[ "$evidence" != *"custom_paint_frames"* ]]; then
    echo "expected renderer evidence to include analyzer summary" >&2
    exit 1
fi
if [[ "$evidence" != *"image_sources"* || "$evidence" != *"theme_retained=8"* || "$evidence" != *"image_cache_refresh_frames"* ]]; then
    echo "expected renderer evidence to include image source summary" >&2
    exit 1
fi
if [[ "$evidence" != *"item_view_stage_max"* || "$evidence" != *"icon_sync=3us"* ]]; then
    echo "expected renderer evidence to include item-view stage summary" >&2
    exit 1
fi
if [[ "$evidence" != *"icon_sync_frames"* || "$evidence" != *"max_resolved=4"* ]]; then
    echo "expected renderer evidence to include icon-sync detail summary" >&2
    exit 1
fi
if [[ "$evidence" != *"renderer_policy_frames"* ]]; then
    echo "expected renderer evidence to include renderer policy summary" >&2
    exit 1
fi
if [[ "$evidence" != *"item_paint_slots_frames"* || "$evidence" != *"max_removed=40"* ]]; then
    echo "expected renderer evidence to include retained paint-slot summary" >&2
    exit 1
fi
if [[ "$evidence" != *"autosmoke:"* || "$evidence" != *"scenario=ZoomScroll"* ]]; then
    echo "expected renderer evidence to include autosmoke summary" >&2
    exit 1
fi

cat > "$tmpdir/custom-theme.log" <<'EOF'
[fika item-view] pane=1 mode=Compact phase=initial items=197 visible=48 raw=187us icon_sync=99us queue=180us convert=196us total=770us
[fika renderer-policy] pane=1 mode=Compact items=48 visual_layer=48 image_layer=48 gpui_image_element=0 retained_interaction=48 gpui_drag_shell=0 rename_overlay=0
[fika item-image] pane=1 mode=Compact prepaint_count=48 prepaint=263us paint_count=0 paint=0us theme_loaded=0 theme_decoded=0 theme_retained=0 theme_placeholder=48 thumb_loaded=0 thumb_decoded=0 thumb_retained=0 thumb_fallback=0
[fika item-image] pane=1 mode=Compact prepaint_count=48 prepaint=211us paint_count=48 paint=990us theme_loaded=48 theme_decoded=1 theme_retained=0 theme_placeholder=0 thumb_loaded=0 thumb_decoded=0 thumb_retained=0 thumb_fallback=0
EOF

cat > "$tmpdir/default-split.log" <<'EOF'
[fika item-view] pane=1 mode=Compact phase=initial items=197 visible=48 raw=141us icon_sync=75us queue=124us convert=150us total=570us
[fika renderer-policy] pane=1 mode=Compact items=48 visual_layer=48 image_layer=0 gpui_image_element=48 retained_interaction=48 gpui_drag_shell=0 rename_overlay=0
EOF

image_renderer_evidence="$("$image_renderer_compare" "$tmpdir/custom-theme.log" "$tmpdir/default-split.log")"
if [[ "$image_renderer_evidence" != *"## Item Image Renderer A/B Evidence"* ]]; then
    echo "expected item image renderer comparison heading" >&2
    exit 1
fi
if [[ "$image_renderer_evidence" != *"Custom-theme renderer state: custom-image-layer"* ]]; then
    echo "expected custom image renderer state in comparison" >&2
    exit 1
fi
if [[ "$image_renderer_evidence" != *"Baseline renderer state: gpui-theme-icons"* ]]; then
    echo "expected baseline GPUI theme-icon renderer state in comparison" >&2
    exit 1
fi
if [[ "$image_renderer_evidence" != *"theme placeholder | 48 | 0"* ]]; then
    echo "expected image renderer comparison to include placeholder delta" >&2
    exit 1
fi
if "$image_renderer_compare" --gate-default-promotion "$tmpdir/custom-theme.log" "$tmpdir/default-split.log" >/dev/null 2>&1; then
    echo "expected default-promotion gate to fail on placeholder/decode churn" >&2
    exit 1
fi

cat > "$tmpdir/custom-theme-clean.log" <<'EOF'
[fika item-view] pane=1 mode=Compact phase=steady items=197 visible=48 raw=187us icon_sync=99us queue=180us convert=196us total=770us
[fika renderer-policy] pane=1 mode=Compact items=48 visual_layer=48 image_layer=48 gpui_image_element=0 retained_interaction=48 gpui_drag_shell=0 rename_overlay=0
[fika item-image] pane=1 mode=Compact prepaint_count=48 prepaint=180us paint_count=48 paint=700us theme_loaded=48 theme_decoded=0 theme_retained=48 theme_placeholder=0 thumb_loaded=0 thumb_decoded=0 thumb_retained=0 thumb_fallback=0
EOF

image_renderer_gate_evidence="$("$image_renderer_compare" --gate-default-promotion "$tmpdir/custom-theme-clean.log" "$tmpdir/default-split.log")"
if [[ "$image_renderer_gate_evidence" != *"Default-promotion gate: pass"* ]]; then
    echo "expected clean custom/default comparison to pass default-promotion gate" >&2
    exit 1
fi

cat > "$tmpdir/missing-icon-sync.log" <<'EOF'
[fika item-view] pane=1 mode=Compact phase=steady items=48 visible=32 raw=50us queue=1us convert=40us total=120us
EOF

if "$analyzer" "$tmpdir/missing-icon-sync.log" >/dev/null 2>&1; then
    echo "expected item-view logs without icon_sync to fail" >&2
    exit 1
fi

cat > "$tmpdir/missing-channels.log" <<'EOF'
[fika item-view] pane=1 mode=Compact phase=steady items=48 visible=32 raw=50us icon_sync=2us queue=1us convert=40us total=120us
EOF

if "$analyzer" --require-details --require-interaction "$tmpdir/missing-channels.log" >/dev/null 2>&1; then
    echo "expected missing details/interaction channels to fail" >&2
    exit 1
fi

if "$analyzer" --require-warm-details-visual "$tmpdir/missing-channels.log" >/dev/null 2>&1; then
    echo "expected missing warmed details visual channel to fail" >&2
    exit 1
fi

if "$analyzer" --require-static-visual "$tmpdir/missing-channels.log" >/dev/null 2>&1; then
    echo "expected missing static visual channel to fail" >&2
    exit 1
fi

cat > "$tmpdir/missing-static-mode.log" <<'EOF'
[fika item-view] pane=1 mode=Compact phase=steady items=48 visible=32 raw=50us icon_sync=2us queue=1us convert=40us total=120us
[fika item-view] pane=1 mode=Icons phase=steady items=48 visible=40 raw=45us icon_sync=3us queue=1us convert=35us total=110us
[fika static-item-visual] pane=1 mode=Compact prepaint_count=32 prepaint=180us paint_count=32 paint=160us
EOF

if "$analyzer" --require-static-modes Compact,Icons "$tmpdir/missing-static-mode.log" >/dev/null 2>&1; then
    echo "expected missing required static visual mode to fail" >&2
    exit 1
fi

cat > "$tmpdir/missing-renderer-policy-mode.log" <<'EOF'
[fika item-view] pane=1 mode=Compact phase=steady items=48 visible=32 raw=50us icon_sync=2us queue=1us convert=40us total=120us
[fika item-view] pane=1 mode=Icons phase=steady items=48 visible=40 raw=45us icon_sync=3us queue=1us convert=35us total=110us
[fika item-view] pane=1 mode=Details phase=steady items=48 visible=30 raw=42us icon_sync=1us queue=1us convert=32us total=105us
[fika renderer-policy] pane=1 mode=Compact items=48 visual_layer=48 image_layer=8 gpui_image_element=0 retained_interaction=48 gpui_drag_shell=0 rename_overlay=0
[fika renderer-policy] pane=1 mode=Icons items=48 visual_layer=48 image_layer=8 gpui_image_element=0 retained_interaction=48 gpui_drag_shell=0 rename_overlay=0
EOF

if "$analyzer" --require-renderer-policy-modes Compact,Icons,Details "$tmpdir/missing-renderer-policy-mode.log" >/dev/null 2>&1; then
    echo "expected missing required renderer-policy mode to fail" >&2
    exit 1
fi

if "$analyzer" --require-paint-slots "$tmpdir/missing-channels.log" >/dev/null 2>&1; then
    echo "expected missing retained paint-slot logs to fail" >&2
    exit 1
fi

cat > "$tmpdir/empty-paint-slots.log" <<'EOF'
[fika item-view] pane=1 mode=Compact phase=steady items=48 visible=32 raw=50us icon_sync=2us queue=1us convert=40us total=120us
[fika item-paint-slots] pane=1 mode=Compact inserted=0 content=0 geometry=0 visual=0 unchanged=0 removed=0 entries=0
EOF

if "$analyzer" --require-paint-slots "$tmpdir/empty-paint-slots.log" >/dev/null 2>&1; then
    echo "expected empty retained paint-slot evidence to fail" >&2
    exit 1
fi

cat > "$tmpdir/missing-autosmoke-action.log" <<'EOF'
[fika autosmoke] item-view start pane=1 scenario=ZoomScroll
[fika item-view] pane=1 mode=Compact phase=steady items=48 visible=32 raw=50us icon_sync=2us queue=1us convert=40us total=120us
[fika autosmoke] item-view action=zoom-in pane=1
[fika autosmoke] item-view action=zoom-out pane=1
[fika autosmoke] item-view complete pane=1 scenario=ZoomScroll
EOF

if "$analyzer" --require-autosmoke "$tmpdir/missing-autosmoke-action.log" >/dev/null 2>&1; then
    echo "expected missing required autosmoke scroll markers to fail" >&2
    exit 1
fi

cat > "$tmpdir/details-autosmoke.log" <<'EOF'
[fika autosmoke] item-view start pane=1 scenario=DetailsZoomScroll
[fika autosmoke] item-view action=view-details pane=1 mode=Details
[fika item-view] pane=1 mode=Details phase=mode-switch items=48 visible=30 raw=50us icon_sync=2us queue=1us convert=40us total=120us
[fika autosmoke] item-view action=zoom-in pane=1
[fika autosmoke] item-view action=zoom-out pane=1
[fika autosmoke] item-view action=scroll-forward pane=1 changed=true
[fika autosmoke] item-view action=scroll-back pane=1 changed=true
[fika details-visual] pane=1 mode=Details prepaint_count=30 prepaint=120us paint_count=30 paint=130us
[fika details-shape-cache] pane=1 mode=Details hits=20 misses=2 evicted=0 compute=50us entries=22
[fika details-glyph-cache] pane=1 mode=Details hits=18 misses=4 evicted=0 entries=22
[fika details-glyph-budget] pane=1 mode=Details requested=22 hits=18 misses=4 computed=4 deferred=0 failed=0 budget_exhausted=false compute=110us
[fika renderer-policy] pane=1 mode=Details items=30 visual_layer=30 image_layer=0 gpui_image_element=0 retained_interaction=30 retained_directory_drop_target=6 gpui_drag_shell=0 gpui_directory_drop_shell=0 details_header_visual_layer=1 gpui_details_header=0 rename_overlay=0
[fika autosmoke] item-view complete pane=1 scenario=DetailsZoomScroll
EOF

"$analyzer" \
    --require-autosmoke \
    --require-details \
    --require-renderer-policy \
    --expect-retained-item-policy \
    --require-modes Details \
    --require-renderer-policy-modes Details \
    "$tmpdir/details-autosmoke.log" >/dev/null

cat > "$tmpdir/details-autosmoke-missing-view-action.log" <<'EOF'
[fika autosmoke] item-view start pane=1 scenario=DetailsZoomScroll
[fika item-view] pane=1 mode=Details phase=mode-switch items=48 visible=30 raw=50us icon_sync=2us queue=1us convert=40us total=120us
[fika autosmoke] item-view action=zoom-in pane=1
[fika autosmoke] item-view action=zoom-out pane=1
[fika autosmoke] item-view action=scroll-forward pane=1 changed=true
[fika autosmoke] item-view action=scroll-back pane=1 changed=true
[fika autosmoke] item-view complete pane=1 scenario=DetailsZoomScroll
EOF

if "$analyzer" --require-autosmoke "$tmpdir/details-autosmoke-missing-view-action.log" >/dev/null 2>&1; then
    echo "expected Details autosmoke without view-details action to fail" >&2
    exit 1
fi

cat > "$tmpdir/invalid-renderer-policy-count.log" <<'EOF'
[fika item-view] pane=1 mode=Compact phase=steady items=2 visible=2 raw=50us icon_sync=2us queue=1us convert=40us total=120us
[fika renderer-policy] pane=1 mode=Compact items=2 visual_layer=3 image_layer=0 gpui_image_element=0 retained_interaction=2 gpui_drag_shell=0 rename_overlay=0
EOF

if "$analyzer" "$tmpdir/invalid-renderer-policy-count.log" >/dev/null 2>&1; then
    echo "expected invalid renderer-policy surface count to fail" >&2
    exit 1
fi

cat > "$tmpdir/invalid-retained-item-policy.log" <<'EOF'
[fika item-view] pane=1 mode=Compact phase=steady items=48 visible=32 raw=50us icon_sync=2us queue=1us convert=40us total=120us
[fika renderer-policy] pane=1 mode=Compact items=48 visual_layer=48 image_layer=8 gpui_image_element=0 retained_interaction=47 gpui_drag_shell=0 rename_overlay=0
EOF

if "$analyzer" --expect-retained-item-policy "$tmpdir/invalid-retained-item-policy.log" >/dev/null 2>&1; then
    echo "expected invalid retained item renderer policy to fail" >&2
    exit 1
fi

cat > "$tmpdir/invalid-drag-start-shell-policy.log" <<'EOF'
[fika item-view] pane=1 mode=Compact phase=steady items=48 visible=32 raw=50us icon_sync=2us queue=1us convert=40us total=120us
[fika renderer-policy] pane=1 mode=Compact items=48 visual_layer=48 image_layer=8 gpui_image_element=0 retained_interaction=48 gpui_drag_shell=1 rename_overlay=0
EOF

if "$analyzer" --expect-retained-item-policy "$tmpdir/invalid-drag-start-shell-policy.log" >/dev/null 2>&1; then
    echo "expected retained item policy with GPUI drag-start shell to fail" >&2
    exit 1
fi

cat > "$tmpdir/invalid-directory-drop-shell-policy.log" <<'EOF'
[fika item-view] pane=1 mode=Compact phase=steady items=48 visible=32 raw=50us icon_sync=2us queue=1us convert=40us total=120us
[fika renderer-policy] pane=1 mode=Compact items=48 visual_layer=48 image_layer=8 gpui_image_element=0 retained_interaction=48 retained_directory_drop_target=6 gpui_drag_shell=0 gpui_directory_drop_shell=1 rename_overlay=0
EOF

if "$analyzer" --expect-retained-item-policy "$tmpdir/invalid-directory-drop-shell-policy.log" >/dev/null 2>&1; then
    echo "expected retained item policy with GPUI directory drop shell to fail" >&2
    exit 1
fi

cat > "$tmpdir/invalid-details-header-policy.log" <<'EOF'
[fika item-view] pane=1 mode=Details phase=steady items=48 visible=30 raw=50us icon_sync=2us queue=1us convert=40us total=120us
[fika renderer-policy] pane=1 mode=Details items=30 visual_layer=30 image_layer=0 gpui_image_element=0 retained_interaction=30 retained_directory_drop_target=6 gpui_drag_shell=0 gpui_directory_drop_shell=0 details_header_visual_layer=0 gpui_details_header=1 rename_overlay=0
EOF

if "$analyzer" --expect-retained-item-policy "$tmpdir/invalid-details-header-policy.log" >/dev/null 2>&1; then
    echo "expected retained item policy with GPUI Details header to fail" >&2
    exit 1
fi

if "$analyzer" --require-modes Compact,Icons,Details "$tmpdir/missing-channels.log" >/dev/null 2>&1; then
    echo "expected missing required modes to fail" >&2
    exit 1
fi

if "$runtime_gate" "$tmpdir/missing-channels.log" >/dev/null 2>&1; then
    echo "expected runtime log gate to fail missing channels" >&2
    exit 1
fi

if "$renderer_evidence" "$tmpdir/missing-channels.log" >/dev/null 2>&1; then
    echo "expected renderer evidence to fail missing channels" >&2
    exit 1
fi

if "$analyzer" --steady-total-us 100 "$tmpdir/complete.log" >/dev/null 2>&1; then
    echo "expected steady threshold violation to fail" >&2
    exit 1
fi

cat > "$tmpdir/warm-steady.log" <<'EOF'
[fika item-view] pane=1 mode=Compact phase=steady items=48 visible=32 raw=50us icon_sync=2us queue=1us convert=40us total=5000us
[fika item-view] pane=1 mode=Compact phase=steady items=48 visible=32 raw=50us icon_sync=2us queue=1us convert=40us total=900us
[fika analyzer] log-boundary path=/tmp/next.log
[fika item-view] pane=1 mode=Compact phase=steady items=48 visible=32 raw=50us icon_sync=2us queue=1us convert=40us total=4000us
[fika item-view] pane=1 mode=Compact phase=steady items=48 visible=32 raw=50us icon_sync=2us queue=1us convert=40us total=800us
EOF

"$analyzer" --warm-steady-total-us 1000 "$tmpdir/warm-steady.log" >/dev/null

if "$analyzer" --steady-total-us 1000 "$tmpdir/warm-steady.log" >/dev/null 2>&1; then
    echo "expected all-frame steady threshold to still fail on warmup frame" >&2
    exit 1
fi

if "$analyzer" --warm-steady-total-us 700 "$tmpdir/warm-steady.log" >/dev/null 2>&1; then
    echo "expected warm steady threshold violation to fail" >&2
    exit 1
fi

if "$analyzer" --static-visual-paint-us 100 "$tmpdir/complete.log" >/dev/null 2>&1; then
    echo "expected static visual paint threshold violation to fail" >&2
    exit 1
fi

if "$analyzer" --image-paint-us 50 "$tmpdir/complete.log" >/dev/null 2>&1; then
    echo "expected item image paint threshold violation to fail" >&2
    exit 1
fi

if "$analyzer" --custom-paint-us 100 "$tmpdir/complete.log" >/dev/null 2>&1; then
    echo "expected custom paint threshold violation to fail" >&2
    exit 1
fi

cat > "$tmpdir/warm-paint.log" <<'EOF'
[fika item-view] pane=1 mode=Compact phase=steady items=48 visible=32 raw=50us icon_sync=2us queue=1us convert=40us total=120us
[fika static-item-visual] pane=1 mode=Compact prepaint_count=32 prepaint=180us paint_count=32 paint=5000us
[fika static-item-visual] pane=1 mode=Compact prepaint_count=32 prepaint=160us paint_count=32 paint=900us
[fika item-image] pane=1 mode=Compact prepaint_count=32 prepaint=80us paint_count=32 paint=4000us theme_loaded=0 theme_decoded=0 theme_retained=32 theme_placeholder=0 thumb_loaded=0 thumb_decoded=0 thumb_retained=0 thumb_fallback=0
[fika item-image] pane=1 mode=Compact prepaint_count=32 prepaint=70us paint_count=32 paint=700us theme_loaded=0 theme_decoded=0 theme_retained=32 theme_placeholder=0 thumb_loaded=0 thumb_decoded=0 thumb_retained=0 thumb_fallback=0
EOF

"$analyzer" \
    --warm-static-visual-paint-us 1000 \
    --warm-image-paint-us 1000 \
    --warm-custom-paint-us 1000 \
    "$tmpdir/warm-paint.log" >/dev/null

if "$analyzer" --static-visual-paint-us 1000 "$tmpdir/warm-paint.log" >/dev/null 2>&1; then
    echo "expected all-frame static visual threshold to still fail on cold paint" >&2
    exit 1
fi

if "$analyzer" --warm-static-visual-paint-us 800 "$tmpdir/warm-paint.log" >/dev/null 2>&1; then
    echo "expected warm static visual threshold violation to fail" >&2
    exit 1
fi

if "$analyzer" --warm-image-paint-us 600 "$tmpdir/warm-paint.log" >/dev/null 2>&1; then
    echo "expected warm item image threshold violation to fail" >&2
    exit 1
fi

if "$analyzer" --warm-custom-paint-us 800 "$tmpdir/warm-paint.log" >/dev/null 2>&1; then
    echo "expected warm custom paint threshold violation to fail" >&2
    exit 1
fi

cat > "$tmpdir/icons-autosmoke.log" <<'EOF'
[fika autosmoke] item-view start pane=1 scenario=IconsZoomScroll
[fika autosmoke] item-view action=view-icons pane=1 mode=Icons
[fika item-view] pane=1 mode=Icons phase=mode-switch items=48 visible=40 raw=50us icon_sync=2us queue=1us convert=40us total=120us
[fika autosmoke] item-view action=zoom-in pane=1
[fika autosmoke] item-view action=zoom-out pane=1
[fika autosmoke] item-view action=scroll-forward pane=1 changed=true
[fika autosmoke] item-view action=scroll-back pane=1 changed=true
[fika static-item-visual] pane=1 mode=Icons prepaint_count=40 prepaint=210us paint_count=40 paint=190us
[fika item-shape-cache] pane=1 mode=Icons hits=38 misses=2 evicted=0 compute=45us entries=40
[fika item-glyph-cache] pane=1 mode=Icons hits=36 misses=4 evicted=0 entries=40
[fika item-glyph-budget] pane=1 mode=Icons requested=40 hits=36 misses=4 computed=4 deferred=0 failed=0 budget_exhausted=false compute=90us
[fika renderer-policy] pane=1 mode=Icons items=48 visual_layer=48 image_layer=8 gpui_image_element=0 retained_interaction=48 gpui_drag_shell=0 rename_overlay=0
[fika autosmoke] item-view complete pane=1 scenario=IconsZoomScroll
EOF

"$analyzer" \
    --require-autosmoke \
    --require-static-visual \
    --require-renderer-policy \
    --expect-retained-item-policy \
    --require-modes Icons \
    --require-static-modes Icons \
    --require-renderer-policy-modes Icons \
    "$tmpdir/icons-autosmoke.log" >/dev/null

: > "$tmpdir/empty.log"
if "$analyzer" "$tmpdir/empty.log" >/dev/null 2>&1; then
    echo "expected empty log to fail" >&2
    exit 1
fi

echo "item-view perf analyzer check passed"
