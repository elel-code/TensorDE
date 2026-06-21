#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
usage: scripts/native-vulkan-h264-bitstream-smoke.sh [options]

Generate or use an H.264 High 8-bit source, then verify the native Vulkan Video
session can create NV12 decode resources and ingest parsed encoded access units.
This does not create a visible surface or submit vkCmdDecodeVideoKHR yet.

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
  report_dir="$(mktemp -d "${work_parent%/}/gilder-vulkan-h264-bitstream.XXXXXX")"
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

set +e
env WAYLAND_DISPLAY="$display" \
  target/release/gilder-native-vulkan \
  --probe-video-session \
  --video-codec h264 \
  --width "$width" \
  --height "$height" \
  --allocate-video-images \
  --allocate-bitstream-buffer \
  --extract-bitstream \
  --source "$source" \
  --bitstream-samples "$samples" \
  >"$probe_json" 2>"$probe_stderr"
status=$?
set -e

if [[ "$status" -ne 0 ]]; then
  printf 'FAIL: native Vulkan H.264 bitstream smoke failed\n' | tee "$summary"
  printf 'source: %s\n' "$source" | tee -a "$summary"
  printf 'stderr: %s\n' "$probe_stderr" | tee -a "$summary"
  sed -n '1,160p' "$probe_stderr" >&2
  exit "$status"
fi

codec="$(jq -r '.requested_codec' "$probe_json")"
profile="$(jq -r '.profile' "$probe_json")"
picture_format="$(jq -r '.picture_format' "$probe_json")"
target_dpb="$(jq -r '.target_picture_dpb_supported // false' "$probe_json")"
target_output="$(jq -r '.target_picture_output_supported // false' "$probe_json")"
target_sampled="$(jq -r '.target_picture_sampled_output_supported // false' "$probe_json")"
video_image_format="$(jq -r '.video_images[0].format // "none"' "$probe_json")"
frontend="$(jq -r '.bitstream_extract.frontend // "none"' "$probe_json")"
stream_format="$(jq -r '.bitstream_extract.stream_format // "none"' "$probe_json")"
alignment="$(jq -r '.bitstream_extract.alignment // "none"' "$probe_json")"
has_annex_b="$(jq -r '.bitstream_extract.has_annex_b_start_codes // false' "$probe_json")"
h264_sps_count="$(jq -r '.bitstream_extract.h264_sps_count // 0' "$probe_json")"
h264_pps_count="$(jq -r '.bitstream_extract.h264_pps_count // 0' "$probe_json")"
h264_idr_count="$(jq -r '.bitstream_extract.h264_idr_count // 0' "$probe_json")"
h264_slice_count="$(jq -r '.bitstream_extract.h264_slice_count // 0' "$probe_json")"
h264_parameter_sets_present="$(jq -r '.bitstream_extract.h264_parameter_sets_present // false' "$probe_json")"
h264_parameter_sets_ready="$(jq -r '.bitstream_extract.h264_parameter_sets.vulkan_std_session_parameters_ready // false' "$probe_json")"
h264_parameter_sets_parser="$(jq -r '.bitstream_extract.h264_parameter_sets.parser // "none"' "$probe_json")"
h264_sps_profile_idc="$(jq -r '.bitstream_extract.h264_parameter_sets.sps.profile_idc // 0' "$probe_json")"
h264_sps_level_idc="$(jq -r '.bitstream_extract.h264_parameter_sets.sps.level_idc // 0' "$probe_json")"
h264_sps_width="$(jq -r '.bitstream_extract.h264_parameter_sets.sps.width // 0' "$probe_json")"
h264_sps_height="$(jq -r '.bitstream_extract.h264_parameter_sets.sps.height // 0' "$probe_json")"
session_parameters_requested="$(jq -r 'if has("session_parameters_requested") then .session_parameters_requested else true end' "$probe_json")"
session_parameters_created="$(jq -r 'if has("session_parameters_created") then .session_parameters_created else true end' "$probe_json")"
session_parameters_codec="$(jq -r '.session_parameters.codec // "none"' "$probe_json")"
session_parameters_source="$(jq -r '.session_parameters.source // "none"' "$probe_json")"
mapped_write_source="$(jq -r '.bitstream_buffer.mapped_write_source // "none"' "$probe_json")"
mapped_write_bytes="$(jq -r '.bitstream_buffer.mapped_write_bytes // 0' "$probe_json")"
samples_collected="$(jq -r '.bitstream_extract.samples // 0' "$probe_json")"

if [[ "$codec" != "h264-high-8" || "$profile" != "high-8" || "$picture_format" != "G8_B8R8_2PLANE_420_UNORM" || "$target_dpb" != "true" || "$target_output" != "true" || "$target_sampled" != "true" || "$video_image_format" != "$picture_format" || "$frontend" != "gstreamer-qtdemux-h264parse-appsink" || "$stream_format" != "byte-stream" || "$alignment" != "au" || "$has_annex_b" != "true" || "$h264_sps_count" -lt 1 || "$h264_pps_count" -lt 1 || "$h264_idr_count" -lt 1 || "$h264_slice_count" -lt 1 || "$h264_parameter_sets_present" != "true" || "$h264_parameter_sets_ready" != "true" || "$h264_parameter_sets_parser" != "native-rust-h264-sps-pps" || "$h264_sps_profile_idc" -ne 100 || "$h264_sps_width" -le 0 || "$h264_sps_height" -le 0 || "$session_parameters_requested" != "true" || "$session_parameters_created" != "true" || "$session_parameters_codec" != "h264-high-8" || "$session_parameters_source" != "native-rust-h264-sps-pps-to-vulkan-std" || "$mapped_write_source" != "extracted-encoded-video-unit" || "$mapped_write_bytes" -le 0 || "$samples_collected" -lt 1 ]]; then
  {
    printf 'FAIL: native Vulkan H.264 bitstream output was not valid\n'
    printf 'codec: %s\n' "$codec"
    printf 'profile: %s\n' "$profile"
    printf 'picture_format: %s\n' "$picture_format"
    printf 'target_dpb: %s\n' "$target_dpb"
    printf 'target_output: %s\n' "$target_output"
    printf 'target_sampled: %s\n' "$target_sampled"
    printf 'video_image_format: %s\n' "$video_image_format"
    printf 'frontend: %s\n' "$frontend"
    printf 'stream_format: %s\n' "$stream_format"
    printf 'alignment: %s\n' "$alignment"
    printf 'has_annex_b: %s\n' "$has_annex_b"
    printf 'h264_sps_count: %s\n' "$h264_sps_count"
    printf 'h264_pps_count: %s\n' "$h264_pps_count"
    printf 'h264_idr_count: %s\n' "$h264_idr_count"
    printf 'h264_slice_count: %s\n' "$h264_slice_count"
    printf 'h264_parameter_sets_present: %s\n' "$h264_parameter_sets_present"
    printf 'h264_parameter_sets_ready: %s\n' "$h264_parameter_sets_ready"
    printf 'h264_parameter_sets_parser: %s\n' "$h264_parameter_sets_parser"
    printf 'h264_sps_profile_idc: %s\n' "$h264_sps_profile_idc"
    printf 'h264_sps_level_idc: %s\n' "$h264_sps_level_idc"
    printf 'h264_sps_width: %s\n' "$h264_sps_width"
    printf 'h264_sps_height: %s\n' "$h264_sps_height"
    printf 'session_parameters_requested: %s\n' "$session_parameters_requested"
    printf 'session_parameters_created: %s\n' "$session_parameters_created"
    printf 'session_parameters_codec: %s\n' "$session_parameters_codec"
    printf 'session_parameters_source: %s\n' "$session_parameters_source"
    printf 'mapped_write_source: %s\n' "$mapped_write_source"
    printf 'mapped_write_bytes: %s\n' "$mapped_write_bytes"
    printf 'samples_collected: %s\n' "$samples_collected"
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
  printf 'samples: %s\n' "$samples_collected"
  printf 'picture_format: %s\n' "$picture_format"
  printf 'target_picture_dpb_supported: %s\n' "$target_dpb"
  printf 'target_picture_output_supported: %s\n' "$target_output"
  printf 'target_picture_sampled_output_supported: %s\n' "$target_sampled"
  printf 'video_image_format: %s\n' "$video_image_format"
  printf 'frontend: %s\n' "$frontend"
  printf 'stream_format: %s\n' "$stream_format"
  printf 'alignment: %s\n' "$alignment"
  printf 'has_annex_b_start_codes: %s\n' "$has_annex_b"
  printf 'h264_sps_count: %s\n' "$h264_sps_count"
  printf 'h264_pps_count: %s\n' "$h264_pps_count"
  printf 'h264_idr_count: %s\n' "$h264_idr_count"
  printf 'h264_slice_count: %s\n' "$h264_slice_count"
  printf 'h264_parameter_sets_present: %s\n' "$h264_parameter_sets_present"
  printf 'h264_parameter_sets_ready: %s\n' "$h264_parameter_sets_ready"
  printf 'h264_parameter_sets_parser: %s\n' "$h264_parameter_sets_parser"
  printf 'h264_sps_profile_idc: %s\n' "$h264_sps_profile_idc"
  printf 'h264_sps_level_idc: %s\n' "$h264_sps_level_idc"
  printf 'h264_sps_width: %s\n' "$h264_sps_width"
  printf 'h264_sps_height: %s\n' "$h264_sps_height"
  printf 'session_parameters_requested: %s\n' "$session_parameters_requested"
  printf 'session_parameters_created: %s\n' "$session_parameters_created"
  printf 'session_parameters_codec: %s\n' "$session_parameters_codec"
  printf 'session_parameters_source: %s\n' "$session_parameters_source"
  printf 'selected_access_unit_bytes: %s\n' "$(jq -r '.bitstream_extract.selected_access_unit_bytes' "$probe_json")"
  printf 'mapped_write_source: %s\n' "$mapped_write_source"
  printf 'mapped_write_bytes: %s\n' "$mapped_write_bytes"
  printf 'bitstream_buffer_bytes: %s\n' "$(jq -r '.bitstream_buffer.size' "$probe_json")"
  printf 'session_memory_bytes: %s\n' "$(jq -r '.total_bound_memory_bytes' "$probe_json")"
  printf 'video_resource_memory_bytes: %s\n' "$(jq -r '.total_video_image_memory_bytes' "$probe_json")"
} >"$summary"

printf 'PASS: native Vulkan H.264 bitstream smoke passed\n'
printf 'summary: %s\n' "$summary"
printf 'probe JSON: %s\n' "$probe_json"
