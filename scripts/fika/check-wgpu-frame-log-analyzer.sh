#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd "$script_dir/../../apps/fika" && pwd)"
analyzer="$script_dir/analyze-wgpu-frame-log.sh"
evidence_runner="$script_dir/run-retained-renderer-evidence.sh"
tmpdir="$(mktemp -d)"
cleanup() {
    rm -rf "$tmpdir"
}
trap cleanup EXIT

bash -n "$analyzer"
bash -n "$evidence_runner"

cat > "$tmpdir/metadata.log" <<'EOF'
[fika-wgpu] frame=1 reason=initial view=compact visible=36 layout=20us prepare=30us render=900us surface=40us encode_present=20us text_raster=0us icon_resolve=10us icon_raster=0us text_atlas_reused=0 text_deferred=0 icon_deferred=0 icon_raster_deferred=0
[fika-wgpu] prewarm-metadata reason=initial view=compact visible=4 deferred=20 batches=1 results=0 applied=0
[fika-wgpu] frame=2 reason=autosmoke-scroll view=compact visible=36 layout=18us prepare=22us render=700us surface=35us encode_present=18us text_raster=0us icon_resolve=3us icon_raster=0us text_atlas_reused=12 text_deferred=0 icon_deferred=0 icon_raster_deferred=0
[fika-wgpu] autosmoke-scroll action=forward delta=1000.0 changed=true old_scroll_x=0.0 new_scroll_x=0.0 old_scroll_y=0.0 new_scroll_y=820.0
[fika-wgpu] prewarm-metadata reason=autosmoke-scroll view=compact visible=8 deferred=12 batches=1 results=3 applied=2
[fika-wgpu] frame=3 reason=autosmoke-scroll view=compact visible=36 layout=17us prepare=20us render=680us surface=32us encode_present=17us text_raster=0us icon_resolve=2us icon_raster=0us text_atlas_reused=16 text_deferred=0 icon_deferred=0 icon_raster_deferred=0
[fika-wgpu] prewarm-metadata reason=autosmoke-scroll view=compact visible=0 deferred=0 batches=0 results=2 applied=2
EOF

summary="$(bash "$analyzer" \
    --require-frames \
    --require-autosmoke-scroll \
    --gate-scope reason:autosmoke-scroll \
    --warm-p95-render-us 1000 \
    --max-icon-raster-us 0 \
    --min-metadata-visible 8 \
    --min-metadata-results 5 \
    --min-metadata-applied 4 \
    "$tmpdir/metadata.log")"

if [[ "$summary" != *"wgpu-metadata-prewarm-summary scope=all"* ]]; then
    echo "expected all-scope metadata prewarm summary" >&2
    exit 1
fi
if [[ "$summary" != *"wgpu-metadata-prewarm-summary scope=reason:autosmoke-scroll"* ]]; then
    echo "expected reason-scope metadata prewarm summary" >&2
    exit 1
fi
if [[ "$summary" != *"visible_total=12"* || "$summary" != *"results_total=5"* || "$summary" != *"applied_total=4"* ]]; then
    echo "expected metadata totals in summary" >&2
    exit 1
fi
if [[ "$summary" != *"wgpu-autosmoke-scroll actions=1 changed=1"* ]]; then
    echo "expected autosmoke scroll summary" >&2
    exit 1
fi

if bash "$analyzer" \
    --gate-scope reason:autosmoke-scroll \
    --min-metadata-applied 5 \
    "$tmpdir/metadata.log" >/dev/null 2>&1; then
    echo "expected metadata applied minimum gate to fail" >&2
    exit 1
fi

if bash "$analyzer" --min-metadata-visible not-a-number "$tmpdir/metadata.log" >/dev/null 2>&1; then
    echo "expected non-numeric metadata gate to fail" >&2
    exit 1
fi

cp "$tmpdir/metadata.log" "$tmpdir/evidence-metadata-tail-scroll.log"
bash "$evidence_runner" \
    --metadata-tail-scroll \
    --analyze-only \
    --skip-build \
    --out-dir "$tmpdir" \
    --prefix evidence >/dev/null

cat > "$tmpdir/evidence-item-downloads.log" <<'EOF'
[fika-wgpu] frame=1 reason=initial view=icons visible=36 layout=20us prepare=30us render=900us surface=40us encode_present=20us text_raster=0us icon_resolve=10us icon_raster=0us text_atlas_reused=0 text_deferred=0 icon_deferred=0 icon_raster_deferred=0
EOF
cat > "$tmpdir/evidence-item-etc-compact.log" <<'EOF'
[fika-wgpu] frame=1 reason=initial view=compact visible=36 layout=20us prepare=30us render=900us surface=40us encode_present=20us text_raster=0us icon_resolve=10us icon_raster=0us text_atlas_reused=0 text_deferred=0 icon_deferred=0 icon_raster_deferred=0
EOF
cat > "$tmpdir/evidence-item-etc-compact-zoom-scroll.log" <<'EOF'
[fika-wgpu] frame=1 reason=initial view=compact visible=36 layout=20us prepare=30us render=900us surface=40us encode_present=20us text_raster=0us icon_resolve=10us icon_raster=0us text_atlas_reused=0 text_deferred=0 icon_deferred=0 icon_raster_deferred=0
[fika-wgpu] autosmoke-scroll action=forward delta=64.0 changed=true old_scroll_x=0.0 new_scroll_x=0.0 old_scroll_y=0.0 new_scroll_y=64.0
[fika-wgpu] frame=2 reason=autosmoke-scroll view=compact visible=36 layout=18us prepare=22us render=700us surface=35us encode_present=18us text_raster=0us icon_resolve=3us icon_raster=0us text_atlas_reused=12 text_deferred=0 icon_deferred=0 icon_raster_deferred=0
EOF
cat > "$tmpdir/evidence-item-etc-icons-zoom-scroll.log" <<'EOF'
[fika-wgpu] frame=1 reason=initial view=icons visible=36 layout=20us prepare=30us render=900us surface=40us encode_present=20us text_raster=0us icon_resolve=10us icon_raster=0us text_atlas_reused=0 text_deferred=0 icon_deferred=0 icon_raster_deferred=0
[fika-wgpu] autosmoke-scroll action=forward delta=64.0 changed=true old_scroll_x=0.0 new_scroll_x=0.0 old_scroll_y=0.0 new_scroll_y=64.0
[fika-wgpu] frame=2 reason=autosmoke-scroll view=icons visible=36 layout=18us prepare=22us render=700us surface=35us encode_present=18us text_raster=0us icon_resolve=3us icon_raster=0us text_atlas_reused=12 text_deferred=0 icon_deferred=0 icon_raster_deferred=0
EOF
cat > "$tmpdir/evidence-item-etc-details-zoom-scroll.log" <<'EOF'
[fika-wgpu] frame=1 reason=initial view=details visible=36 layout=20us prepare=30us render=900us surface=40us encode_present=20us text_raster=0us icon_resolve=10us icon_raster=0us text_atlas_reused=0 text_deferred=0 icon_deferred=0 icon_raster_deferred=0
[fika-wgpu] autosmoke-scroll action=forward delta=64.0 changed=true old_scroll_x=0.0 new_scroll_x=0.0 old_scroll_y=0.0 new_scroll_y=64.0
[fika-wgpu] frame=2 reason=autosmoke-scroll view=details visible=36 layout=18us prepare=22us render=700us surface=35us encode_present=18us text_raster=0us icon_resolve=3us icon_raster=0us text_atlas_reused=12 text_deferred=0 icon_deferred=0 icon_raster_deferred=0
EOF

bash "$evidence_runner" \
    --items-only \
    --analyze-only \
    --skip-build \
    --out-dir "$tmpdir" \
    --prefix evidence >/dev/null
