#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
usage: scripts/native-vulkan-visible-codec-smoke.sh --codec <h264|av1|h265-main10> [options]

Generate or use a codec source, then run native Wayland + native Vulkan
--run-video on a real compositor output. This is a visible presentation smoke:
GStreamer may decode into GPU memory, but it does not own a display sink.

Options:
  --codec <name>        h264, av1, or h265-main10.
  --display <name>      Wayland display name. Default: WAYLAND_DISPLAY.
  --output-name <name>  Target Wayland output name, for example HDMI-A-1.
  --output <name>       Alias for --output-name.
  --source <path>       Existing source. Default: generate source.
  --report-dir <dir>    Exact evidence directory. Created and kept.
  --work-dir <dir>      Parent directory for generated evidence. Default: /tmp.
  --width <px>          Generated source width. Default: codec-specific.
  --height <px>         Generated source height. Default: codec-specific.
  --target-fps <fps>    Generated source and presentation FPS. Default: 60.
  --frames <count>      Generated frame count. Default: target-fps * duration.
  --duration <seconds>  Visible runtime duration. Default: 4.
  --layer <layer>       Wayland layer. Default: background.
  --fit <mode>          Render fit. Default: cover.
  --decoder <policy>    auto|hardware-preferred|hardware-required|software. Default: hardware-preferred.
  --no-build            Reuse existing target/release/gilder-native-vulkan.
  -h, --help            Show this help text.
EOF
}

codec=""
display="${WAYLAND_DISPLAY:-}"
output_name=""
source=""
report_dir=""
work_parent="${TMPDIR:-/tmp}"
width=0
height=0
target_fps=60
frames=0
duration=4
layer="background"
fit="cover"
decoder="hardware-preferred"
no_build=0
generated_source=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --codec)
      codec="${2:-}"
      shift 2
      ;;
    --display)
      display="${2:-}"
      shift 2
      ;;
    --output-name|--output)
      output_name="${2:-}"
      shift 2
      ;;
    --source)
      source="${2:-}"
      shift 2
      ;;
    --report-dir)
      report_dir="${2:-}"
      shift 2
      ;;
    --work-dir)
      work_parent="${2:-}"
      shift 2
      ;;
    --width)
      width="${2:-}"
      shift 2
      ;;
    --height)
      height="${2:-}"
      shift 2
      ;;
    --target-fps)
      target_fps="${2:-}"
      shift 2
      ;;
    --frames)
      frames="${2:-}"
      shift 2
      ;;
    --duration)
      duration="${2:-}"
      shift 2
      ;;
    --layer)
      layer="${2:-}"
      shift 2
      ;;
    --fit)
      fit="${2:-}"
      shift 2
      ;;
    --decoder)
      decoder="${2:-}"
      shift 2
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
      printf 'unknown option: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

case "$codec" in
  h264|av1|h265-main10) ;;
  *)
    printf 'FAIL: --codec must be h264, av1, or h265-main10\n' >&2
    usage >&2
    exit 2
    ;;
esac
if [[ -z "$display" ]]; then
  printf 'FAIL: WAYLAND_DISPLAY is empty; pass --display\n' >&2
  exit 1
fi
for tool in ffmpeg jq; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    printf 'FAIL: missing required tool: %s\n' "$tool" >&2
    exit 1
  fi
done
case "$layer" in
  background|bottom) ;;
  top|overlay)
    printf 'FAIL: foreground layer "%s" is not allowed by this smoke\n' "$layer" >&2
    exit 1
    ;;
  *)
    printf 'FAIL: unsupported layer: %s\n' "$layer" >&2
    exit 1
    ;;
esac
if [[ "$target_fps" -lt 1 || "$duration" -lt 1 ]]; then
  printf 'FAIL: target-fps and duration must be positive\n' >&2
  exit 1
fi

case "$codec" in
  h264)
    default_width=1280
    default_height=720
    extension="mp4"
    expected_format="NV12"
    ;;
  av1)
    default_width=640
    default_height=368
    extension="webm"
    expected_format="NV12"
    ;;
  h265-main10)
    default_width=640
    default_height=368
    extension="mp4"
    expected_format="P010_10LE"
    ;;
esac
if [[ "$width" -eq 0 ]]; then
  width="$default_width"
fi
if [[ "$height" -eq 0 ]]; then
  height="$default_height"
fi
if [[ "$width" -lt 128 || "$height" -lt 128 ]]; then
  printf 'FAIL: width/height must be at least 128\n' >&2
  exit 1
fi
if (( width % 16 != 0 || height % 16 != 0 )); then
  printf 'FAIL: generated codec smoke dimensions must be 16-pixel aligned; got %sx%s\n' "$width" "$height" >&2
  exit 1
fi
if [[ "$frames" -eq 0 ]]; then
  frames=$((target_fps * duration))
fi
if [[ "$frames" -lt 2 ]]; then
  printf 'FAIL: frames must be at least 2\n' >&2
  exit 1
fi

if [[ -z "$report_dir" ]]; then
  report_dir="$(mktemp -d "${work_parent%/}/gilder-vulkan-visible-${codec}.XXXXXX")"
else
  mkdir -p "$report_dir"
fi
mkdir -p "$report_dir"

if [[ "$no_build" -eq 0 ]]; then
  cargo build --release --features native-vulkan-gst-video --bin gilder-native-vulkan
fi

if [[ -z "$source" ]]; then
  generated_source=1
  generated_dir="$report_dir/source"
  mkdir -p "$generated_dir"
  source_duration_seconds=$(( (frames + target_fps - 1) / target_fps ))
  source="$generated_dir/${codec}-${width}x${height}-${target_fps}fps-${frames}frames.${extension}"
  case "$codec" in
    h264)
      ffmpeg -hide_banner -loglevel error -y \
        -f lavfi -i "testsrc2=size=${width}x${height}:rate=${target_fps}:duration=${source_duration_seconds}" \
        -frames:v "$frames" -an \
        -c:v libx264 -profile:v high -level:v 4.2 -preset ultrafast -tune zerolatency -pix_fmt yuv420p \
        -x264-params "keyint=2:min-keyint=2:scenecut=0:open-gop=0:bframes=0:ref=1:repeat-headers=1" \
        "$source"
      ;;
    av1)
      ffmpeg -hide_banner -loglevel error -y \
        -f lavfi -i "testsrc2=size=${width}x${height}:rate=${target_fps}:duration=${source_duration_seconds}" \
        -frames:v "$frames" -an \
        -c:v libaom-av1 -cpu-used 8 -crf 45 -b:v 0 -row-mt 1 -pix_fmt yuv420p \
        "$source"
      ;;
    h265-main10)
      ffmpeg -hide_banner -loglevel error -y \
        -f lavfi -i "testsrc2=size=${width}x${height}:rate=${target_fps}:duration=${source_duration_seconds}" \
        -frames:v "$frames" -an \
        -c:v libx265 -profile:v main10 -preset ultrafast -tune zerolatency -pix_fmt yuv420p10le \
        -x265-params "keyint=2:min-keyint=2:scenecut=0:open-gop=0:bframes=0:ref=1:repeat-headers=1:hrd=0:rc-lookahead=0" \
        "$source"
      ;;
  esac
fi

if [[ ! -f "$source" ]]; then
  printf 'FAIL: source does not exist: %s\n' "$source" >&2
  exit 1
fi

runtime_json="$report_dir/runtime.json"
runtime_stderr="$report_dir/runtime.stderr"
summary="$report_dir/summary.txt"
args=(
  --run-video
  --source "$source"
  --duration "$duration"
  --target-fps "$target_fps"
  --layer "$layer"
  --fit "$fit"
  --decoder "$decoder"
  --loop
  --muted
)
if [[ -n "$output_name" ]]; then
  args+=(--output-name "$output_name")
fi

set +e
env WAYLAND_DISPLAY="$display" \
  target/release/gilder-native-vulkan \
  "${args[@]}" \
  >"$runtime_json" 2>"$runtime_stderr"
status=$?
set -e

if [[ "$status" -ne 0 ]]; then
  printf 'FAIL: native Vulkan visible %s smoke failed\n' "$codec" | tee "$summary"
  printf 'source: %s\n' "$source" | tee -a "$summary"
  printf 'stderr: %s\n' "$runtime_stderr" | tee -a "$summary"
  sed -n '1,160p' "$runtime_stderr" >&2
  exit "$status"
fi

configured="$(jq -r '.configured // false' "$runtime_json")"
frames_rendered="$(jq -r '.frames_rendered // 0' "$runtime_json")"
average_render_fps="$(jq -r '.average_render_fps // 0' "$runtime_json")"
frontend="$(jq -r '.video_runtime.frontend // "none"' "$runtime_json")"
frontend_status="$(jq -r '.video_runtime.frontend_status // "none"' "$runtime_json")"
texture_import_status="$(jq -r '.video_runtime.texture_import_status // "none"' "$runtime_json")"
frames_received="$(jq -r '.video_runtime.frames_received // 0' "$runtime_json")"
frames_imported="$(jq -r '.video_runtime.frames_imported // 0' "$runtime_json")"
last_sample_format="$(jq -r '.video_runtime.last_sample_format // "none"' "$runtime_json")"
last_import_memory_path="$(jq -r '.video_runtime.last_import_memory_path // "none"' "$runtime_json")"
last_import_error="$(jq -r '.video_runtime.last_import_error // "none"' "$runtime_json")"
memory_route="$(jq -r '.video_runtime.memory_route.route // "none"' "$runtime_json")"
memory_route_direct="$(jq -r '.video_runtime.memory_route.direct_import_confirmed // false' "$runtime_json")"
memory_route_copy_risk="$(jq -r '.video_runtime.memory_route.copy_risk // "none"' "$runtime_json")"
dmabuf_import_source="$(jq -r '.video_runtime.last_dmabuf_import.source // "none"' "$runtime_json")"
dmabuf_image_memory_type_bits="$(jq -r '.video_runtime.last_dmabuf_import.image_memory_type_bits_hex // "none"' "$runtime_json")"
dmabuf_fd_memory_type_bits="$(jq -r '.video_runtime.last_dmabuf_import.fd_memory_type_bits_hex // "none"' "$runtime_json")"
dmabuf_compatible_memory_type_bits="$(jq -r '.video_runtime.last_dmabuf_import.compatible_memory_type_bits_hex // "none"' "$runtime_json")"
dmabuf_selected_memory_type_index="$(jq -r '.video_runtime.last_dmabuf_import.selected_memory_type_index // "none"' "$runtime_json")"

if [[ "$configured" != "true" || "$frames_rendered" -lt 1 || "$frontend" != "gstreamer-appsink" || "$frontend_status" != "appsink-receiving-samples" || "$frames_received" -lt 1 || "$frames_imported" -lt 1 || "$last_sample_format" != "$expected_format" || "$last_import_memory_path" == "none" || "$last_import_error" != "none" ]]; then
  {
    printf 'FAIL: native Vulkan visible %s runtime output was not valid\n' "$codec"
    printf 'configured: %s\n' "$configured"
    printf 'frames_rendered: %s\n' "$frames_rendered"
    printf 'average_render_fps: %s\n' "$average_render_fps"
    printf 'frontend: %s\n' "$frontend"
    printf 'frontend_status: %s\n' "$frontend_status"
    printf 'texture_import_status: %s\n' "$texture_import_status"
    printf 'frames_received: %s\n' "$frames_received"
    printf 'frames_imported: %s\n' "$frames_imported"
    printf 'last_sample_format: %s\n' "$last_sample_format"
    printf 'expected_format: %s\n' "$expected_format"
    printf 'last_import_memory_path: %s\n' "$last_import_memory_path"
    printf 'memory_route: %s\n' "$memory_route"
    printf 'memory_route_direct_import_confirmed: %s\n' "$memory_route_direct"
    printf 'memory_route_copy_risk: %s\n' "$memory_route_copy_risk"
    printf 'dmabuf_import_source: %s\n' "$dmabuf_import_source"
    printf 'dmabuf_image_memory_type_bits: %s\n' "$dmabuf_image_memory_type_bits"
    printf 'dmabuf_fd_memory_type_bits: %s\n' "$dmabuf_fd_memory_type_bits"
    printf 'dmabuf_compatible_memory_type_bits: %s\n' "$dmabuf_compatible_memory_type_bits"
    printf 'dmabuf_selected_memory_type_index: %s\n' "$dmabuf_selected_memory_type_index"
    printf 'last_import_error: %s\n' "$last_import_error"
    printf 'runtime JSON: %s\n' "$runtime_json"
  } | tee "$summary"
  exit 1
fi

{
  printf 'codec: %s\n' "$codec"
  printf 'source: %s\n' "$source"
  printf 'generated_source: %s\n' "$([[ "$generated_source" -eq 1 ]] && printf yes || printf no)"
  printf 'selected_device: %s\n' "$(jq -r '.selected_physical_device_name' "$runtime_json")"
  printf 'configured: %s\n' "$configured"
  printf 'swapchain_extent: %s\n' "$(jq -c '.swapchain_extent' "$runtime_json")"
  printf 'present_mode: %s\n' "$(jq -r '.present_mode' "$runtime_json")"
  printf 'frames_rendered: %s\n' "$frames_rendered"
  printf 'average_render_fps: %s\n' "$average_render_fps"
  printf 'frontend: %s\n' "$frontend"
  printf 'frontend_status: %s\n' "$frontend_status"
  printf 'texture_import_status: %s\n' "$texture_import_status"
  printf 'frames_received: %s\n' "$frames_received"
  printf 'frames_imported: %s\n' "$frames_imported"
  printf 'last_sample_format: %s\n' "$last_sample_format"
  printf 'last_sample_size: %s\n' "$(jq -c '.video_runtime.last_sample_size' "$runtime_json")"
  printf 'last_import_memory_path: %s\n' "$last_import_memory_path"
  printf 'memory_route: %s\n' "$memory_route"
  printf 'memory_route_direct_import_confirmed: %s\n' "$memory_route_direct"
  printf 'memory_route_copy_risk: %s\n' "$memory_route_copy_risk"
  printf 'dmabuf_import_source: %s\n' "$dmabuf_import_source"
  printf 'dmabuf_image_memory_type_bits: %s\n' "$dmabuf_image_memory_type_bits"
  printf 'dmabuf_fd_memory_type_bits: %s\n' "$dmabuf_fd_memory_type_bits"
  printf 'dmabuf_compatible_memory_type_bits: %s\n' "$dmabuf_compatible_memory_type_bits"
  printf 'dmabuf_selected_memory_type_index: %s\n' "$dmabuf_selected_memory_type_index"
  printf 'actual_decoders: %s\n' "$(jq -c '.video_runtime.actual_decoders' "$runtime_json")"
  printf 'caps_memory_features: %s\n' "$(jq -c '.video_runtime.caps_memory_features' "$runtime_json")"
} >"$summary"

printf 'PASS: native Vulkan visible %s smoke passed\n' "$codec"
printf 'summary: %s\n' "$summary"
printf 'runtime JSON: %s\n' "$runtime_json"
