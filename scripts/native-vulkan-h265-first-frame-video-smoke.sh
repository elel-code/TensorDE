#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
usage: scripts/native-vulkan-h265-first-frame-video-smoke.sh [options]

Generate or use a 4K/240 H.265 Main source, then run the native Vulkan direct
H.265 first-frame path on a real Wayland background surface. The path decodes
the first H.265 IDR with Vulkan Video into a sampled NV12 image and presents it
through the native Vulkan swapchain. It does not use a GStreamer display sink.

Options:
  --display <name>      Wayland display name. Default: WAYLAND_DISPLAY.
  --output-name <name>  Target Wayland output name, for example HDMI-A-1.
  --source <path>       Existing H.265 source. Default: generate short-GOP source.
  --report-dir <dir>    Exact evidence directory. Created and kept.
  --work-dir <dir>      Parent directory for generated evidence. Default: /tmp.
  --duration <seconds>  Visible smoke duration. Default: 3.
  --target-fps <fps>    Presentation target FPS. Default: 240.
  --min-frames <count>  Minimum rendered frames. Default: duration * target-fps * 80%.
  --width <px>          Generated/probed width. Default: 3840.
  --height <px>         Generated/probed height. Default: 2160.
  --frames <count>      Generated frame count. Default: 8.
  --layer <layer>       Wayland layer. Default: background.
  --fit <mode>          Render fit. Default: cover.
  --no-build            Reuse existing target/release/gilder-native-vulkan.
  -h, --help            Show this help text.
EOF
}

display="${WAYLAND_DISPLAY:-}"
output_name=""
source=""
report_dir=""
work_parent="${TMPDIR:-/tmp}"
duration=3
target_fps=240
min_frames=0
width=3840
height=2160
frames=8
layer="background"
fit="cover"
no_build=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --display)
      display="${2:-}"
      shift 2
      ;;
    --output-name)
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
    --duration)
      duration="${2:-}"
      shift 2
      ;;
    --target-fps)
      target_fps="${2:-}"
      shift 2
      ;;
    --min-frames)
      min_frames="${2:-}"
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
    --frames)
      frames="${2:-}"
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

if [[ "$duration" -lt 1 || "$target_fps" -lt 1 || "$width" -lt 2 || "$height" -lt 2 ]]; then
  printf 'FAIL: duration/target-fps must be positive and width/height must be at least 2\n' >&2
  exit 1
fi

if [[ "$min_frames" -eq 0 ]]; then
  min_frames=$((duration * target_fps * 80 / 100))
fi

if [[ -z "$report_dir" ]]; then
  report_dir="$(mktemp -d "${work_parent%/}/gilder-vulkan-h265-first-frame-video.XXXXXX")"
else
  mkdir -p "$report_dir"
fi
mkdir -p "$report_dir"

if [[ "$no_build" -eq 0 ]]; then
  cargo build --release --features native-vulkan-gst-video --bin gilder-native-vulkan
fi

if [[ -z "$source" ]]; then
  generated_dir="$report_dir/source"
  mkdir -p "$generated_dir"
  source="$generated_dir/h265-main-short-gop-${width}x${height}-${target_fps}fps.mp4"
  ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i "testsrc2=size=${width}x${height}:rate=${target_fps}" \
    -frames:v "$frames" -an \
    -c:v libx265 -profile:v main -preset ultrafast -tune zerolatency -pix_fmt yuv420p \
    -x265-params "keyint=2:min-keyint=2:scenecut=0:open-gop=0:bframes=0:ref=1:repeat-headers=1:hrd=0:rc-lookahead=0" \
    "$source"
fi

if [[ ! -f "$source" ]]; then
  printf 'FAIL: source does not exist: %s\n' "$source" >&2
  exit 1
fi

runtime_json="$report_dir/runtime.json"
runtime_stderr="$report_dir/runtime.stderr"
summary="$report_dir/summary.txt"
args=(
  --run-h265-first-frame-video
  --source "$source"
  --width "$width"
  --height "$height"
  --duration "$duration"
  --target-fps "$target_fps"
  --layer "$layer"
  --fit "$fit"
)
if [[ -n "$output_name" ]]; then
  args+=(--output-name "$output_name")
fi

set +e
env WAYLAND_DISPLAY="$display" \
  target/release/gilder-native-vulkan \
  "${args[@]}" \
  >"$runtime_json" 2>"$runtime_stderr"
runtime_status=$?
set -e

if [[ "$runtime_status" -ne 0 ]]; then
  printf 'FAIL: native Vulkan direct H.265 first-frame video smoke failed\n' | tee "$summary"
  printf 'source: %s\n' "$source" | tee -a "$summary"
  printf 'stderr: %s\n' "$runtime_stderr" | tee -a "$summary"
  sed -n '1,160p' "$runtime_stderr" >&2
  exit "$runtime_status"
fi

decode_completed="$(jq -r '.decode.completed // false' "$runtime_json")"
frames_rendered="$(jq -r '.frames_rendered // 0' "$runtime_json")"
present_queue="$(jq -r '.present_queue_family_index // "none"' "$runtime_json")"
video_queue="$(jq -r '.video_decode_queue_family_index // "none"' "$runtime_json")"
video_codecs="$(jq -c '.video_decode_queue_codec_operations // []' "$runtime_json")"
swapchain_images="$(jq -r '.swapchain_image_count // 0' "$runtime_json")"
resource_bytes="$(jq -r '.video_resource_memory_bytes // 0' "$runtime_json")"
bitstream_bytes="$(jq -r '.bitstream_buffer_bytes // 0' "$runtime_json")"

if [[ "$decode_completed" != "true" || "$frames_rendered" -lt "$min_frames" || "$present_queue" == "none" || "$video_queue" == "none" || "$swapchain_images" -lt 2 || "$resource_bytes" -le 0 || "$bitstream_bytes" -le 0 ]]; then
  {
    printf 'FAIL: native Vulkan direct H.265 first-frame video output was not valid\n'
    printf 'decode_completed: %s\n' "$decode_completed"
    printf 'frames_rendered: %s\n' "$frames_rendered"
    printf 'min_frames: %s\n' "$min_frames"
    printf 'present_queue: %s\n' "$present_queue"
    printf 'video_queue: %s\n' "$video_queue"
    printf 'video_codecs: %s\n' "$video_codecs"
    printf 'swapchain_images: %s\n' "$swapchain_images"
    printf 'video_resource_memory_bytes: %s\n' "$resource_bytes"
    printf 'bitstream_buffer_bytes: %s\n' "$bitstream_bytes"
    printf 'runtime JSON: %s\n' "$runtime_json"
  } | tee "$summary"
  exit 1
fi

{
  printf 'source: %s\n' "$source"
  printf 'selected_device: %s\n' "$(jq -r '.selected_physical_device_name' "$runtime_json")"
  printf 'requested_output_name: %s\n' "${output_name:-auto}"
  printf 'surface_logical_size: %s\n' "$(jq -c '.wayland_surface_logical_size' "$runtime_json")"
  printf 'surface_buffer_size: %s\n' "$(jq -c '.wayland_surface_buffer_size' "$runtime_json")"
  printf 'source_extent: %s\n' "$(jq -c '.source_extent' "$runtime_json")"
  printf 'swapchain_extent: %s\n' "$(jq -c '.swapchain_extent' "$runtime_json")"
  printf 'swapchain_format: %s\n' "$(jq -r '.swapchain_format' "$runtime_json")"
  printf 'present_mode: %s\n' "$(jq -r '.present_mode' "$runtime_json")"
  printf 'runtime_elapsed_ms: %s\n' "$(jq -r '.runtime_elapsed_ms' "$runtime_json")"
  printf 'frames_rendered: %s\n' "$frames_rendered"
  printf 'average_render_fps: %s\n' "$(jq -r '.average_render_fps' "$runtime_json")"
  printf 'target_max_fps: %s\n' "$(jq -r '.target_max_fps // "none"' "$runtime_json")"
  printf 'present_queue_family_index: %s\n' "$present_queue"
  printf 'present_queue_flags: %s\n' "$(jq -c '.present_queue_flags' "$runtime_json")"
  printf 'video_decode_queue_family_index: %s\n' "$video_queue"
  printf 'video_decode_queue_flags: %s\n' "$(jq -c '.video_decode_queue_flags' "$runtime_json")"
  printf 'video_decode_queue_codec_operations: %s\n' "$video_codecs"
  printf 'cross_queue_sync_strategy: %s\n' "$(jq -r '.cross_queue_sync_strategy' "$runtime_json")"
  printf 'decode_elapsed_us: %s\n' "$(jq -r '.decode.decode_elapsed_us' "$runtime_json")"
  printf 'video_resource_memory_bytes: %s\n' "$resource_bytes"
  printf 'session_memory_bytes: %s\n' "$(jq -r '.session_memory_bytes' "$runtime_json")"
  printf 'bitstream_buffer_bytes: %s\n' "$bitstream_bytes"
} >"$summary"

printf 'PASS: native Vulkan direct H.265 first-frame video smoke passed\n'
printf 'summary: %s\n' "$summary"
printf 'runtime JSON: %s\n' "$runtime_json"
