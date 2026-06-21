#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
usage: scripts/native-vulkan-h264-first-frame-smoke.sh [options]

Generate or use an H.264 High 8-bit source, then verify the native Vulkan Video
direct H.264 first-frame path creates session parameters and submits the first
IDR through vkCmdDecodeVideoKHR with decode-output readback. This does not use a
GStreamer display sink.

Options:
  --display <name>      Wayland display name. Default: WAYLAND_DISPLAY.
  --source <path>       Existing H.264 source. Default: generate MP4 source.
  --report-dir <dir>    Exact evidence directory. Created and kept.
  --work-dir <dir>      Parent directory for generated evidence. Default: /tmp.
  --width <px>          Generated/probed width. Default: 1280.
  --height <px>         Generated/probed height. Default: 720.
  --rate <fps>          Generated frame rate. Default: 60.
  --level <level>       Generated H.264 level. Default: 4.2.
  --frames <count>      Generated frame count. Default: samples + 2.
  --samples <count>     AU samples to collect. Default: 8.
  --sample-output       Also sample decoded NV12 output through Vulkan graphics.
  --no-build            Reuse existing target/release/gilder-native-vulkan.
  -h, --help            Show this help text.
EOF
}

display="${WAYLAND_DISPLAY:-}"
source=""
report_dir=""
work_parent="${TMPDIR:-/tmp}"
width=1280
height=720
rate=60
level=4.2
frames=0
samples=8
sample_output=0
no_build=0
generated_source=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --display)
      display="${2:-}"
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
    --rate)
      rate="${2:-}"
      shift 2
      ;;
    --level)
      level="${2:-}"
      shift 2
      ;;
    --frames)
      frames="${2:-}"
      shift 2
      ;;
    --samples)
      samples="${2:-}"
      shift 2
      ;;
    --sample-output)
      sample_output=1
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
if [[ "$width" -lt 128 || "$height" -lt 128 || "$rate" -lt 1 || "$samples" -lt 1 ]]; then
  printf 'FAIL: width/height/rate/samples must be valid\n' >&2
  exit 1
fi
if (( width % 16 != 0 || height % 16 != 0 )); then
  printf 'FAIL: H.264 Vulkan Video source dimensions must be 16-pixel aligned; got %sx%s\n' "$width" "$height" >&2
  exit 1
fi

if [[ -z "$report_dir" ]]; then
  report_dir="$(mktemp -d "${work_parent%/}/gilder-vulkan-h264-first-frame.XXXXXX")"
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
  if [[ "$frames" -eq 0 || "$frames" -lt $((samples + 2)) ]]; then
    frames=$((samples + 2))
  fi
  source="$generated_dir/h264-high-${width}x${height}-${rate}fps-${frames}frames.mp4"
  ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i "testsrc2=size=${width}x${height}:rate=${rate}" \
    -frames:v "$frames" -an \
    -c:v libx264 -profile:v high -level:v "$level" -preset veryfast -tune zerolatency -pix_fmt yuv420p \
    -x264-params "keyint=2:min-keyint=2:scenecut=0:open-gop=0:bframes=0:ref=1:repeat-headers=1:cabac=1:8x8dct=1" \
    "$source"
fi

if [[ ! -f "$source" ]]; then
  printf 'FAIL: source does not exist: %s\n' "$source" >&2
  exit 1
fi

probe_json="$report_dir/probe.json"
probe_stderr="$report_dir/probe.stderr"
summary="$report_dir/summary.txt"
args=(
  --probe-video-session
  --video-codec h264
  --width "$width"
  --height "$height"
  --allocate-video-images
  --allocate-bitstream-buffer
  --extract-bitstream
  --source "$source"
  --bitstream-samples "$samples"
  --decode-first-frame
)
if [[ "$sample_output" -eq 1 ]]; then
  args+=(--sample-decoded-first-frame)
fi

set +e
env WAYLAND_DISPLAY="$display" \
  target/release/gilder-native-vulkan \
  "${args[@]}" \
  >"$probe_json" 2>"$probe_stderr"
status=$?
set -e

if [[ "$status" -ne 0 ]]; then
  printf 'FAIL: native Vulkan H.264 first-frame smoke failed\n' | tee "$summary"
  printf 'source: %s\n' "$source" | tee -a "$summary"
  printf 'stderr: %s\n' "$probe_stderr" | tee -a "$summary"
  sed -n '1,160p' "$probe_stderr" >&2
  exit "$status"
fi

codec="$(jq -r '.requested_codec' "$probe_json")"
profile="$(jq -r '.profile' "$probe_json")"
picture_format="$(jq -r '.picture_format' "$probe_json")"
session_parameters_created="$(jq -r '.session_parameters_created // false' "$probe_json")"
session_parameters_codec="$(jq -r '.session_parameters.codec // "none"' "$probe_json")"
session_parameters_source="$(jq -r '.session_parameters.source // "none"' "$probe_json")"
frontend="$(jq -r '.bitstream_extract.frontend // "none"' "$probe_json")"
stream_format="$(jq -r '.bitstream_extract.stream_format // "none"' "$probe_json")"
alignment="$(jq -r '.bitstream_extract.alignment // "none"' "$probe_json")"
h264_parameter_sets_ready="$(jq -r '.bitstream_extract.h264_parameter_sets.vulkan_std_session_parameters_ready // false' "$probe_json")"
first_frame_requested="$(jq -r '.first_frame_decode_requested // false' "$probe_json")"
decode_completed="$(jq -r '.first_frame_decode.completed // false' "$probe_json")"
decode_codec="$(jq -r '.first_frame_decode.codec // "none"' "$probe_json")"
decode_idr="$(jq -r '.first_frame_decode.idr // false' "$probe_json")"
decode_irap="$(jq -r '.first_frame_decode.irap // false' "$probe_json")"
slice_count="$(jq -r '.first_frame_decode.slice_segment_count // 0' "$probe_json")"
readback_copied="$(jq -r '.first_frame_decode.output_readback.copied // false' "$probe_json")"
y_nonzero="$(jq -r '.first_frame_decode.output_readback.y_plane_nonzero_bytes // 0' "$probe_json")"
uv_nonzero="$(jq -r '.first_frame_decode.output_readback.uv_plane_nonzero_bytes // 0' "$probe_json")"
readback_bytes="$(jq -r '.first_frame_decode.output_readback.total_bytes // 0' "$probe_json")"
sample_copied="$(jq -r '.first_frame_decode.output_sampling.copied // false' "$probe_json")"

if [[ "$codec" != "h264-high-8" || "$profile" != "high-8" || "$picture_format" != "G8_B8R8_2PLANE_420_UNORM" || "$session_parameters_created" != "true" || "$session_parameters_codec" != "h264-high-8" || "$session_parameters_source" != "native-rust-h264-sps-pps-to-vulkan-std" || "$frontend" != "gstreamer-qtdemux-h264parse-appsink" || "$stream_format" != "byte-stream" || "$alignment" != "au" || "$h264_parameter_sets_ready" != "true" || "$first_frame_requested" != "true" || "$decode_completed" != "true" || "$decode_codec" != "h264-high-8" || "$decode_idr" != "true" || "$decode_irap" != "true" || "$slice_count" -lt 1 || "$readback_copied" != "true" || "$y_nonzero" -le 0 || "$uv_nonzero" -le 0 || "$readback_bytes" -le 0 ]]; then
  {
    printf 'FAIL: native Vulkan H.264 first-frame output was not valid\n'
    printf 'codec: %s\n' "$codec"
    printf 'profile: %s\n' "$profile"
    printf 'picture_format: %s\n' "$picture_format"
    printf 'session_parameters_created: %s\n' "$session_parameters_created"
    printf 'session_parameters_codec: %s\n' "$session_parameters_codec"
    printf 'session_parameters_source: %s\n' "$session_parameters_source"
    printf 'frontend: %s\n' "$frontend"
    printf 'stream_format: %s\n' "$stream_format"
    printf 'alignment: %s\n' "$alignment"
    printf 'h264_parameter_sets_ready: %s\n' "$h264_parameter_sets_ready"
    printf 'first_frame_requested: %s\n' "$first_frame_requested"
    printf 'decode_completed: %s\n' "$decode_completed"
    printf 'decode_codec: %s\n' "$decode_codec"
    printf 'decode_idr: %s\n' "$decode_idr"
    printf 'decode_irap: %s\n' "$decode_irap"
    printf 'slice_count: %s\n' "$slice_count"
    printf 'readback_copied: %s\n' "$readback_copied"
    printf 'y_nonzero: %s\n' "$y_nonzero"
    printf 'uv_nonzero: %s\n' "$uv_nonzero"
    printf 'readback_bytes: %s\n' "$readback_bytes"
    printf 'probe JSON: %s\n' "$probe_json"
  } | tee "$summary"
  exit 1
fi
if [[ "$sample_output" -eq 1 && "$sample_copied" != "true" ]]; then
  {
    printf 'FAIL: native Vulkan H.264 first-frame sampled output was not valid\n'
    printf 'sample_copied: %s\n' "$sample_copied"
    printf 'probe JSON: %s\n' "$probe_json"
  } | tee "$summary"
  exit 1
fi

{
  printf 'source: %s\n' "$source"
  printf 'generated_source: %s\n' "$([[ "$generated_source" -eq 1 ]] && printf yes || printf no)"
  printf 'selected_device: %s\n' "$(jq -r '.selected_physical_device_name' "$probe_json")"
  printf 'requested_extent: %s\n' "$(jq -c '.requested_extent' "$probe_json")"
  printf 'result: %s\n' "$(jq -r '.result' "$probe_json")"
  printf 'requested_codec: %s\n' "$codec"
  printf 'profile: %s\n' "$profile"
  printf 'generated_h264_level: %s\n' "$level"
  printf 'picture_format: %s\n' "$picture_format"
  printf 'frontend: %s\n' "$frontend"
  printf 'stream_format: %s\n' "$stream_format"
  printf 'alignment: %s\n' "$alignment"
  printf 'session_parameters_created: %s\n' "$session_parameters_created"
  printf 'session_parameters_codec: %s\n' "$session_parameters_codec"
  printf 'session_parameters_source: %s\n' "$session_parameters_source"
  printf 'decode_completed: %s\n' "$decode_completed"
  printf 'decode_codec: %s\n' "$decode_codec"
  printf 'slice_count: %s\n' "$slice_count"
  printf 'slice_offsets: %s\n' "$(jq -c '.first_frame_decode.slice_segment_offsets' "$probe_json")"
  printf 'nal_type_label: %s\n' "$(jq -r '.first_frame_decode.nal_type_label' "$probe_json")"
  printf 'pic_order_cnt_val: %s\n' "$(jq -r '.first_frame_decode.pic_order_cnt_val' "$probe_json")"
  printf 'src_buffer_range: %s\n' "$(jq -r '.first_frame_decode.src_buffer_range' "$probe_json")"
  printf 'readback_bytes: %s\n' "$readback_bytes"
  printf 'y_plane_nonzero_bytes: %s\n' "$y_nonzero"
  printf 'uv_plane_nonzero_bytes: %s\n' "$uv_nonzero"
  printf 'y_plane_hash: %s\n' "$(jq -r '.first_frame_decode.output_readback.y_plane_hash' "$probe_json")"
  printf 'uv_plane_hash: %s\n' "$(jq -r '.first_frame_decode.output_readback.uv_plane_hash' "$probe_json")"
  printf 'sample_output: %s\n' "$([[ "$sample_output" -eq 1 ]] && printf yes || printf no)"
  printf 'sample_copied: %s\n' "$sample_copied"
  printf 'selected_access_unit_bytes: %s\n' "$(jq -r '.bitstream_extract.selected_access_unit_bytes' "$probe_json")"
  printf 'mapped_write_bytes: %s\n' "$(jq -r '.bitstream_buffer.mapped_write_bytes // 0' "$probe_json")"
  printf 'bitstream_buffer_bytes: %s\n' "$(jq -r '.bitstream_buffer.size' "$probe_json")"
  printf 'session_memory_bytes: %s\n' "$(jq -r '.total_bound_memory_bytes' "$probe_json")"
  printf 'video_resource_memory_bytes: %s\n' "$(jq -r '.total_video_image_memory_bytes' "$probe_json")"
} >"$summary"

printf 'PASS: native Vulkan H.264 first-frame smoke passed\n'
printf 'summary: %s\n' "$summary"
printf 'probe JSON: %s\n' "$probe_json"
