#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
usage: scripts/native-wgpu-smoke.sh [options]

Run the native wgpu/Vulkan layer-shell helper on a real Wayland display and
sample process memory/CPU. This does not use playbin, waylandsink, or the
manual linux-dmabuf attach prototype.

Options:
  --display <name>      Wayland display name. Default: WAYLAND_DISPLAY.
  --report-dir <dir>    Exact evidence directory. Created and kept.
  --work-dir <dir>      Parent directory for temporary data. Default: /tmp.
  --sample-duration <s> Run/sample duration. Default: 5.
  --sample-interval <s> Sampling interval in whole seconds. Default: 1.
  --runtime-interval-ms <ms>
                        Native runtime JSONL sample interval. Default: 1000.
  --source <path>       Existing video source. When set, run wgpu video mode.
  --video-backend <name>
                        auto, cpu-upload, gpu-video, gst-gpu-video, or gst-dmabuf. Default: auto.
                        gpu-video expects Annex-B H.264 (.h264/.264).
  --fit <name>          cover, contain, stretch, or center. Default: cover.
  --decoder <policy>    auto, hardware-preferred, hardware-required, software.
                        Default: hardware-preferred.
  --loop                Loop video playback. Default.
  --no-loop             Do not loop video playback.
  --target-fps <n>      Render loop target. Default: 240.
  --no-fps-limit        Disable render loop sleep.
  --layer <name>        background, bottom, top, or overlay. Default: bottom.
  --allow-foreground-layer
                        Permit top/overlay layers for short visual debugging.
  --output-name <name>  Bind the layer-shell surface to a wl_output name such
                        as HDMI-A-1. Required for >=200fps smoke runs unless
                        --allow-compositor-output is passed.
  --allow-compositor-output
                        Permit compositor-selected output for high-fps smoke.
  --color <value>       #rrggbb or r,g,b clear color. Default: #0b5cff.
  --render-mode <name>  solid or pulse. Default: solid.
  --animate-color       Alias for --render-mode pulse.
  --no-build            Reuse existing target/release/gilder-native-wgpu.
  -h, --help            Show this help text.
EOF
}

work_parent="${TMPDIR:-/tmp}"
report_dir=""
wayland_display="${WAYLAND_DISPLAY:-}"
sample_duration=5
sample_interval=1
runtime_interval_ms=1000
source=""
video_backend="auto"
fit="cover"
decoder="hardware-preferred"
loop_playback=1
target_fps=240
fps_limit=1
layer="bottom"
allow_foreground_layer=0
output_name=""
allow_compositor_output=0
color="#0b5cff"
render_mode="solid"
no_build=0

write_smaps_mapping_summary() {
  local target_pid="$1"
  local report="$2"
  local categories_csv="$3"
  local smaps="/proc/${target_pid}/smaps"
  local mappings_tmp="${report}.mappings.tmp"
  local categories_tmp="${report}.categories.tmp"

  if [[ ! -r "$smaps" ]]; then
    printf 'category,pss_kib,rss_kib,private_clean_kib,private_dirty_kib,shared_kib\n' >"$categories_csv"
    {
      printf 'report: process-memory-mappings\n'
      printf 'pid: %s\n' "$target_pid"
      printf 'source: %s\n' "$smaps"
      printf 'status: unavailable\n'
    } >"$report"
    return 0
  fi

  : >"$mappings_tmp"
  : >"$categories_tmp"
  awk -v mappings="$mappings_tmp" -v categories="$categories_tmp" '
    function reset_current() {
      rss = 0
      pss = 0
      private_clean = 0
      private_dirty = 0
      shared_clean = 0
      shared_dirty = 0
    }
    function category_for(mapping) {
      if (mapping ~ /^\/dev\/nvidia/) { return "nvidia-device" }
      if (mapping ~ /^\/dev\/dri/) { return "dri-device" }
      if (mapping == "[heap]") { return "heap" }
      if (mapping ~ /^\[stack/) { return "stack" }
      if (mapping ~ /^\[anon/) { return "anonymous" }
      if (mapping ~ /^\/dev\/zero/) { return "shared-memory" }
      if (mapping ~ /\/libnvidia/ || mapping ~ /\/libcuda/ || mapping ~ /\/libnvcuvid/) {
        return "nvidia-library"
      }
      if (mapping ~ /\/gstreamer-1\.0\// || mapping ~ /\/libgst/) { return "gstreamer-library" }
      if (mapping ~ /\/libgtk/ || mapping ~ /\/libgdk/) { return "gtk-library" }
      if (mapping ~ /\/target\/(debug|release)\/gilder-native-wgpu$/) { return "gilder-binary" }
      if (mapping ~ /^\/usr\/lib/) { return "system-library" }
      if (mapping ~ /^\//) { return "file-mapping" }
      return "other"
    }
    function emit_current() {
      if (!have_mapping) { return }
      shared = shared_clean + shared_dirty
      printf "%d %d %d %d %d %s\n", pss, rss, private_clean, private_dirty, shared, key >> mappings
      category = category_for(key)
      category_pss[category] += pss
      category_rss[category] += rss
      category_private_clean[category] += private_clean
      category_private_dirty[category] += private_dirty
      category_shared[category] += shared
      mapping_count += 1
    }
    /^[0-9a-fA-F]+-[0-9a-fA-F]+/ {
      emit_current()
      have_mapping = 1
      reset_current()
      key = "[anon]"
      if (NF >= 6) {
        key = $6
        for (i = 7; i <= NF; i++) {
          key = key " " $i
        }
      }
      next
    }
    /^Rss:/ { rss = $2 + 0; next }
    /^Pss:/ { pss = $2 + 0; next }
    /^Private_Clean:/ { private_clean = $2 + 0; next }
    /^Private_Dirty:/ { private_dirty = $2 + 0; next }
    /^Shared_Clean:/ { shared_clean = $2 + 0; next }
    /^Shared_Dirty:/ { shared_dirty = $2 + 0; next }
    END {
      emit_current()
      for (category in category_pss) {
        printf "%d %d %d %d %d %s\n",
          category_pss[category],
          category_rss[category],
          category_private_clean[category],
          category_private_dirty[category],
          category_shared[category],
          category >> categories
      }
      printf "%d\n", mapping_count > (categories ".count")
    }
  ' "$smaps"

  {
    printf 'report: process-memory-mappings\n'
    printf 'pid: %s\n' "$target_pid"
    printf 'source: %s\n' "$smaps"
    printf 'status: available\n'
    printf 'mapping_count: '
    sed -n '1p' "${categories_tmp}.count" 2>/dev/null || printf '0\n'
    printf 'top_mappings_by_pss:\n'
    printf 'pss_kib rss_kib private_clean_kib private_dirty_kib shared_kib mapping\n'
    sort -k1,1nr "$mappings_tmp" 2>/dev/null | sed -n '1,30p'
    printf 'top_mappings_by_private_dirty:\n'
    printf 'pss_kib rss_kib private_clean_kib private_dirty_kib shared_kib mapping\n'
    sort -k4,4nr "$mappings_tmp" 2>/dev/null | sed -n '1,30p'
    printf 'category_summary_by_pss:\n'
    printf 'pss_kib rss_kib private_clean_kib private_dirty_kib shared_kib category\n'
    sort -k1,1nr "$categories_tmp" 2>/dev/null
    printf 'category_summary_by_private_dirty:\n'
    printf 'pss_kib rss_kib private_clean_kib private_dirty_kib shared_kib category\n'
    sort -k4,4nr "$categories_tmp" 2>/dev/null
  } >"$report"

  {
    printf 'category,pss_kib,rss_kib,private_clean_kib,private_dirty_kib,shared_kib\n'
    sort -k6,6 "$categories_tmp" 2>/dev/null | awk '
      {
        printf "%s,%d,%d,%d,%d,%d\n", $6, $1, $2, $3, $4, $5
      }
    '
  } >"$categories_csv"

  rm -f "$mappings_tmp" "$categories_tmp" "${categories_tmp}.count"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --display)
      [[ $# -ge 2 ]] || { echo "--display requires a value" >&2; exit 2; }
      wayland_display="$2"
      shift 2
      ;;
    --report-dir)
      [[ $# -ge 2 ]] || { echo "--report-dir requires a value" >&2; exit 2; }
      report_dir="$2"
      shift 2
      ;;
    --work-dir)
      [[ $# -ge 2 ]] || { echo "--work-dir requires a value" >&2; exit 2; }
      work_parent="$2"
      shift 2
      ;;
    --sample-duration)
      [[ $# -ge 2 ]] || { echo "--sample-duration requires seconds" >&2; exit 2; }
      sample_duration="$2"
      shift 2
      ;;
    --sample-interval)
      [[ $# -ge 2 ]] || { echo "--sample-interval requires seconds" >&2; exit 2; }
      sample_interval="$2"
      shift 2
      ;;
    --target-fps)
      [[ $# -ge 2 ]] || { echo "--target-fps requires a value" >&2; exit 2; }
      target_fps="$2"
      fps_limit=1
      shift 2
      ;;
    --runtime-interval-ms)
      [[ $# -ge 2 ]] || { echo "--runtime-interval-ms requires milliseconds" >&2; exit 2; }
      runtime_interval_ms="$2"
      shift 2
      ;;
    --source)
      [[ $# -ge 2 ]] || { echo "--source requires a path" >&2; exit 2; }
      source="$2"
      shift 2
      ;;
    --video-backend)
      [[ $# -ge 2 ]] || { echo "--video-backend requires a value" >&2; exit 2; }
      video_backend="$2"
      shift 2
      ;;
    --fit)
      [[ $# -ge 2 ]] || { echo "--fit requires a value" >&2; exit 2; }
      fit="$2"
      shift 2
      ;;
    --decoder)
      [[ $# -ge 2 ]] || { echo "--decoder requires a value" >&2; exit 2; }
      decoder="$2"
      shift 2
      ;;
    --loop)
      loop_playback=1
      shift
      ;;
    --no-loop)
      loop_playback=0
      shift
      ;;
    --no-fps-limit)
      fps_limit=0
      shift
      ;;
    --layer)
      [[ $# -ge 2 ]] || { echo "--layer requires a value" >&2; exit 2; }
      layer="$2"
      shift 2
      ;;
    --allow-foreground-layer)
      allow_foreground_layer=1
      shift
      ;;
    --output-name)
      [[ $# -ge 2 ]] || { echo "--output-name requires a value" >&2; exit 2; }
      output_name="$2"
      shift 2
      ;;
    --allow-compositor-output)
      allow_compositor_output=1
      shift
      ;;
    --color)
      [[ $# -ge 2 ]] || { echo "--color requires a value" >&2; exit 2; }
      color="$2"
      shift 2
      ;;
    --render-mode)
      [[ $# -ge 2 ]] || { echo "--render-mode requires a value" >&2; exit 2; }
      render_mode="$2"
      shift 2
      ;;
    --animate-color)
      render_mode="pulse"
      shift
      ;;
    --no-build)
      no_build=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

[[ -n "$wayland_display" ]] || { echo "WAYLAND_DISPLAY is empty; pass --display" >&2; exit 2; }
[[ "$sample_duration" =~ ^[0-9]+$ && "$sample_duration" -gt 0 ]] || {
  echo "--sample-duration must be a positive integer" >&2
  exit 2
}
[[ "$sample_interval" =~ ^[0-9]+$ && "$sample_interval" -gt 0 ]] || {
  echo "--sample-interval must be a positive integer" >&2
  exit 2
}
[[ "$runtime_interval_ms" =~ ^[0-9]+$ && "$runtime_interval_ms" -ge 100 ]] || {
  echo "--runtime-interval-ms must be an integer >= 100" >&2
  exit 2
}
if [[ "$fps_limit" -eq 1 ]]; then
  [[ "$target_fps" =~ ^[0-9]+$ && "$target_fps" -gt 0 ]] || {
    echo "--target-fps must be a positive integer" >&2
    exit 2
  }
fi
case "$layer" in
  background|bottom|top|overlay) ;;
  *) echo "--layer must be background, bottom, top, or overlay" >&2; exit 2 ;;
esac
case "$render_mode" in
  solid|pulse) ;;
  *) echo "--render-mode must be solid or pulse" >&2; exit 2 ;;
esac
case "$fit" in
  cover|contain|stretch|center) ;;
  *) echo "--fit must be cover, contain, stretch, or center" >&2; exit 2 ;;
esac
case "$video_backend" in
  auto|cpu-upload|cpu|appsink|gpu-video|gpu|vulkan-video|gst-gpu-video|gstreamer-gpu-video|gst-vulkan-video|gst-dmabuf|dmabuf|gstreamer-dmabuf) ;;
  *) echo "--video-backend must be auto, cpu-upload, gpu-video, gst-gpu-video, or gst-dmabuf" >&2; exit 2 ;;
esac
case "$decoder" in
  auto|hardware-preferred|hw-preferred|hardware-required|hw-required|software) ;;
  *) echo "--decoder must be auto, hardware-preferred, hardware-required, or software" >&2; exit 2 ;;
esac
if [[ -n "$source" && ! -f "$source" ]]; then
  echo "--source does not exist: $source" >&2
  exit 2
fi
if [[ "$allow_foreground_layer" -eq 0 ]]; then
  case "$layer" in
    top|overlay)
      echo "--layer ${layer} covers normal application windows; pass --allow-foreground-layer for foreground debug" >&2
      exit 2
      ;;
  esac
fi
if [[ "$fps_limit" -eq 1 && "$target_fps" -ge 200 && -z "$output_name" && "$allow_compositor_output" -eq 0 ]]; then
  echo "--output-name is required for >=200fps wgpu smoke runs; pass --allow-compositor-output only if compositor-selected output is intentional" >&2
  exit 2
fi

if [[ -n "$report_dir" ]]; then
  work_dir="$report_dir"
  mkdir -p "$work_dir"
else
  mkdir -p "$work_parent"
  work_dir="$(mktemp -d "${work_parent%/}/gilder-native-wgpu.XXXXXX")"
fi

exe="target/release/gilder-native-wgpu"
if [[ "$no_build" -eq 0 ]]; then
  if [[ -n "$source" ]]; then
    build_backend="$video_backend"
    if [[ "$build_backend" == "auto" ]]; then
      case "${source##*.}" in
        h264|H264|264) build_backend="gpu-video" ;;
        *) build_backend="gst-gpu-video" ;;
      esac
    fi
    case "$build_backend" in
      gpu-video|gpu|vulkan-video)
        cargo build --release --features native-wgpu-renderer,native-wgpu-gpu-video --bin gilder-native-wgpu
        ;;
      gst-gpu-video|gstreamer-gpu-video|gst-vulkan-video)
        cargo build --release --features native-wgpu-gst-gpu-video --bin gilder-native-wgpu
        ;;
      gst-dmabuf|dmabuf|gstreamer-dmabuf)
        cargo build --release --features native-wgpu-gst-dmabuf --bin gilder-native-wgpu
        ;;
      *)
        cargo build --release --features native-wgpu-renderer,video-renderer --bin gilder-native-wgpu
        ;;
    esac
  else
    cargo build --release --features native-wgpu-renderer --bin gilder-native-wgpu
  fi
fi
if [[ ! -x "$exe" ]]; then
  echo "missing executable $exe; build it or omit --no-build" >&2
  exit 1
fi

runtime_json="$work_dir/runtime.json"
runtime_jsonl="$work_dir/runtime.jsonl"
samples_csv="$work_dir/samples.csv"
summary_txt="$work_dir/summary.txt"
runtime_summary_txt="$work_dir/runtime-summary.txt"
memory_mapping_summary_txt="$work_dir/memory-mapping-summary.txt"
memory_mapping_categories_csv="$work_dir/memory-mapping-categories.csv"
metadata_txt="$work_dir/metadata.txt"
stdout_log="$work_dir/stdout.log"
stderr_log="$work_dir/stderr.log"

native_args=(
  --duration "$sample_duration"
  --layer "$layer"
  --color "$color"
  --render-mode "$render_mode"
  --runtime-json "$runtime_json"
  --runtime-jsonl "$runtime_jsonl"
  --runtime-interval-ms "$runtime_interval_ms"
)
if [[ -n "$source" ]]; then
  native_args+=(--source "$source" --video-backend "$video_backend" --fit "$fit" --decoder "$decoder")
  if [[ "$loop_playback" -eq 1 ]]; then
    native_args+=(--loop)
  else
    native_args+=(--no-loop)
  fi
fi
if [[ "$fps_limit" -eq 1 ]]; then
  native_args+=(--target-fps "$target_fps")
else
  native_args+=(--no-fps-limit)
fi
if [[ "$allow_foreground_layer" -eq 1 ]]; then
  native_args+=(--allow-foreground-layer)
fi
if [[ -n "$output_name" ]]; then
  native_args+=(--output-name "$output_name")
fi

cat >"$metadata_txt" <<EOF
display: ${wayland_display}
sample_duration: ${sample_duration}
sample_interval: ${sample_interval}
runtime_interval_ms: ${runtime_interval_ms}
mode: $([[ -n "$source" ]] && printf video || printf clear)
source: ${source:-none}
video_backend: ${video_backend}
fit: ${fit}
decoder: ${decoder}
loop_playback: $([[ "$loop_playback" -eq 1 ]] && printf yes || printf no)
target_fps: $([[ "$fps_limit" -eq 1 ]] && printf '%s' "$target_fps" || printf unlimited)
layer: ${layer}
output_name: ${output_name:-compositor-selected}
color: ${color}
render_mode: ${render_mode}
manual_linux_dmabuf_attach: no
legacy_waylandsink: no
EOF

env WAYLAND_DISPLAY="$wayland_display" "$exe" "${native_args[@]}" >"$stdout_log" 2>"$stderr_log" &
app_pid=$!

printf 'elapsed_s,pid,rss_kib,ps_rss_kib,vsz_kib,private_dirty_kib,shared_dirty_kib,private_clean_kib,shared_clean_kib,swap_kib,cpu_percent\n' >"$samples_csv"

elapsed=0
while kill -0 "$app_pid" 2>/dev/null; do
  rss=0
  private_dirty=0
  shared_dirty=0
  private_clean=0
  shared_clean=0
  swap=0
  if [[ -r "/proc/$app_pid/smaps_rollup" ]]; then
    read -r rss private_dirty shared_dirty private_clean shared_clean swap < <(
      awk '
        /^Rss:/ { rss=$2 }
        /^Private_Dirty:/ { private_dirty=$2 }
        /^Shared_Dirty:/ { shared_dirty=$2 }
        /^Private_Clean:/ { private_clean=$2 }
        /^Shared_Clean:/ { shared_clean=$2 }
        /^Swap:/ { swap=$2 }
        END { printf "%d %d %d %d %d %d\n", rss, private_dirty, shared_dirty, private_clean, shared_clean, swap }
      ' "/proc/$app_pid/smaps_rollup"
    )
    write_smaps_mapping_summary "$app_pid" "$memory_mapping_summary_txt" "$memory_mapping_categories_csv"
  fi
  ps_line="$(ps -o rss=,vsz=,%cpu= -p "$app_pid" 2>/dev/null || true)"
  ps_rss=0
  vsz=0
  cpu=0
  if [[ -n "$ps_line" ]]; then
    read -r ps_rss vsz cpu <<<"$ps_line"
  fi
  printf '%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s\n' \
    "$elapsed" "$app_pid" "$rss" "$ps_rss" "$vsz" \
    "$private_dirty" "$shared_dirty" "$private_clean" "$shared_clean" "$swap" "$cpu" \
    >>"$samples_csv"
  sleep "$sample_interval"
  elapsed=$((elapsed + sample_interval))
done

set +e
wait "$app_pid"
app_status=$?
set -e

awk -F, '
  NR > 1 {
    count++
    rss_sum += $3
    dirty_sum += $6
    cpu_sum += $11
    if ($3 > rss_max) rss_max = $3
    if ($6 > dirty_max) dirty_max = $6
  }
  END {
    printf "samples: %d\n", count
    if (count > 0) {
      printf "rss_kib_avg: %.0f\n", rss_sum / count
      printf "rss_kib_max: %.0f\n", rss_max
      printf "private_dirty_kib_avg: %.0f\n", dirty_sum / count
      printf "private_dirty_kib_max: %.0f\n", dirty_max
      printf "cpu_percent_avg: %.2f\n", cpu_sum / count
    }
  }
' "$samples_csv" >"$summary_txt"

if [[ -s "$runtime_json" ]]; then
  if command -v jq >/dev/null 2>&1; then
    jq -r '
      def renderer: .renderer // .;
      "runtime_elapsed_ms: \(renderer.runtime_elapsed_ms)",
      "render_mode: \(renderer.render_mode)",
      "render_calls: \(renderer.render_calls)",
      "frames_rendered: \(renderer.frames_rendered)",
      "frames_skipped: \(renderer.frames_skipped)",
      "average_render_fps: \(renderer.average_render_fps)",
      "render_duration_us_avg: \(renderer.render_duration_us_avg)",
      "render_duration_us_max: \(renderer.render_duration_us_max)",
      "last_render_duration_us: \(renderer.last_render_duration_us)",
      "surface_suboptimal_frames: \(renderer.surface_suboptimal_frames)",
      "surface_lost_skips: \(renderer.surface_lost_skips)",
      "surface_outdated_skips: \(renderer.surface_outdated_skips)",
      "surface_timeout_skips: \(renderer.surface_timeout_skips)",
      "surface_occluded_skips: \(renderer.surface_occluded_skips)",
      "surface_validation_skips: \(renderer.surface_validation_skips)",
      "surface_format: \(renderer.surface_format)",
      "present_mode: \(renderer.present_mode)",
      "last_render_error: \(renderer.last_render_error)",
      if .video then
        "video_backend: \(.video.backend // "cpu-upload")",
        "video_pipeline_kind: \(.video.pipeline_kind // null)",
        "video_state: \(.video.state // .video.gst_state // null)",
        "video_pulled_samples: \(.video.pulled_samples // null)",
        "video_uploaded_frames: \(.video.uploaded_frames // null)",
        "video_exported_frames: \(.video.exported_frames // null)",
        "video_export_failures: \(.video.export_failures // null)",
        "video_import_attempts: \(.video.import_attempts // null)",
        "video_imported_frames: \(.video.imported_frames // null)",
        "video_import_failures: \(.video.import_failures // null)",
        "video_decoded_frames: \(.video.decoded_frames // null)",
        "video_presented_frames: \(.video.presented_frames // null)",
        "video_pending_frames: \(.video.pending_frames // null)",
        "video_last_memory_types: \((.video.last_memory_types // []) | join("|"))",
        "video_last_export_source: \(.video.last_export_source // null)",
        "video_last_drm_fourcc: \(.video.last_drm_fourcc // null)",
        "video_last_drm_modifier: \(.video.last_drm_modifier // null)",
        "video_last_plane_offsets: \((.video.last_plane_offsets // []) | join("|"))",
        "video_last_plane_strides: \((.video.last_plane_strides // []) | join("|"))",
        "video_last_fd_count: \(.video.last_fd_count // null)",
        "video_cuda_direct_pending_copies: \(.video.last_cuda_direct_pending_copies // null)",
        "video_bytes_read: \(.video.bytes_read // null)",
        "video_eos_messages: \(.video.eos_messages)",
        "video_decoder_resets: \(.video.decoder_resets // null)",
        "video_last_frame_size: \(.video.last_frame_size | if . == null then null else "\(.[0])x\(.[1])" end)",
        "video_last_frame_format: \(.video.last_frame_format)",
        "video_last_source_stride: \(.video.last_source_stride // null)",
        "video_last_upload_stride: \(.video.last_upload_stride // null)",
        "video_last_error: \(.video.last_error)"
      else empty end
    ' "$runtime_json" >"$runtime_summary_txt"
  else
    awk '
    function value() {
      line = $0
      sub(/^[[:space:]]*"[^"]+":[[:space:]]*/, "", line)
      gsub(/[",]/, "", line)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", line)
      return line
    }
    /"runtime_elapsed_ms":/ { print "runtime_elapsed_ms: " value() }
    /"render_mode":/ { print "render_mode: " value() }
    /"render_calls":/ { print "render_calls: " value() }
    /"frames_rendered":/ { print "frames_rendered: " value() }
    /"frames_skipped":/ { print "frames_skipped: " value() }
    /"average_render_fps":/ { print "average_render_fps: " value() }
    /"render_duration_us_avg":/ { print "render_duration_us_avg: " value() }
    /"render_duration_us_max":/ { print "render_duration_us_max: " value() }
    /"last_render_duration_us":/ { print "last_render_duration_us: " value() }
    /"surface_suboptimal_frames":/ { print "surface_suboptimal_frames: " value() }
    /"surface_lost_skips":/ { print "surface_lost_skips: " value() }
    /"surface_outdated_skips":/ { print "surface_outdated_skips: " value() }
    /"surface_timeout_skips":/ { print "surface_timeout_skips: " value() }
    /"surface_occluded_skips":/ { print "surface_occluded_skips: " value() }
    /"surface_validation_skips":/ { print "surface_validation_skips: " value() }
    /"surface_format":/ { print "surface_format: " value() }
    /"present_mode":/ { print "present_mode: " value() }
    /"last_render_error":/ { print "last_render_error: " value() }
    /"backend":/ { print "video_backend: " value() }
    /"pipeline_kind":/ { print "video_pipeline_kind: " value() }
    /"state":/ { print "video_state: " value() }
    /"gst_state":/ { print "video_gst_state: " value() }
    /"pulled_samples":/ { print "video_pulled_samples: " value() }
    /"uploaded_frames":/ { print "video_uploaded_frames: " value() }
    /"exported_frames":/ { print "video_exported_frames: " value() }
    /"export_failures":/ { print "video_export_failures: " value() }
    /"import_attempts":/ { print "video_import_attempts: " value() }
    /"imported_frames":/ { print "video_imported_frames: " value() }
    /"import_failures":/ { print "video_import_failures: " value() }
    /"decoded_frames":/ { print "video_decoded_frames: " value() }
    /"presented_frames":/ { print "video_presented_frames: " value() }
    /"pending_frames":/ { print "video_pending_frames: " value() }
    /"last_memory_types":/ { print "video_last_memory_types: " value() }
    /"last_export_source":/ { print "video_last_export_source: " value() }
    /"last_drm_fourcc":/ { print "video_last_drm_fourcc: " value() }
    /"last_drm_modifier":/ { print "video_last_drm_modifier: " value() }
    /"last_plane_offsets":/ { print "video_last_plane_offsets: " value() }
    /"last_plane_strides":/ { print "video_last_plane_strides: " value() }
    /"last_fd_count":/ { print "video_last_fd_count: " value() }
    /"last_cuda_direct_pending_copies":/ { print "video_cuda_direct_pending_copies: " value() }
    /"bytes_read":/ { print "video_bytes_read: " value() }
    /"eos_messages":/ { print "video_eos_messages: " value() }
    /"decoder_resets":/ { print "video_decoder_resets: " value() }
    /"last_frame_size":/ { print "video_last_frame_size: " value() }
    /"last_frame_format":/ { print "video_last_frame_format: " value() }
    /"last_source_stride":/ { print "video_last_source_stride: " value() }
    /"last_upload_stride":/ { print "video_last_upload_stride: " value() }
    /"last_error":/ { print "video_last_error: " value() }
  ' "$runtime_json" >"$runtime_summary_txt"
  fi
fi

if [[ "$app_status" -ne 0 ]]; then
  echo "FAIL: gilder-native-wgpu exited with $app_status" >&2
  echo "stderr: $stderr_log" >&2
  echo "evidence: $work_dir" >&2
  exit "$app_status"
fi

echo "PASS: native wgpu smoke completed"
echo "evidence: $work_dir"
echo "metadata: $metadata_txt"
echo "samples:  $samples_csv"
echo "summary:  $summary_txt"
echo "runtime:  $runtime_json"
echo "runtime samples: $runtime_jsonl"
echo "runtime summary: $runtime_summary_txt"
echo "memory mapping summary: $memory_mapping_summary_txt"
echo "memory mapping categories: $memory_mapping_categories_csv"
