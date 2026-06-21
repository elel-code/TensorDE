#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
usage: scripts/native-vulkan-h264-ready-prefix-smoke.sh [options]

Generate or use a normal H.264 High 8-bit IDR+P source, then verify the
native Vulkan Video direct H.264 path submits a reference-ready AU prefix
through vkCmdDecodeVideoKHR and reads back the final decoded NV12 frame.
This is the first continuous direct gate: IDR + P, no B frames, default
short-term L0 references.

Options:
  --display <name>      Wayland display name. Default: WAYLAND_DISPLAY.
  --source <path>       Existing H.264 source. Default: generate IPPP MP4.
  --report-dir <dir>    Exact evidence directory. Created and kept.
  --work-dir <dir>      Parent directory for generated evidence. Default: /tmp.
  --width <px>          Generated/probed width. Default: 1280.
  --height <px>         Generated/probed height. Default: 720.
  --rate <fps>          Generated frame rate. Default: 60.
  --level <level>       Generated H.264 level. Default: 4.2.
  --frames <count>      Generated frame count. Default: decode-prefix + 2.
  --keyint <count>      Generated GOP keyint. Default: 240.
  --refs <count>        Generated active reference frames. Default: 1.
  --samples <count>     AU samples to collect. Default: max(decode-prefix, 8).
  --decode-prefix <n>   AUs to decode. Default: 8.
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
keyint=240
refs=1
samples=8
decode_prefix=8
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
    --keyint)
      keyint="${2:-}"
      shift 2
      ;;
    --refs)
      refs="${2:-}"
      shift 2
      ;;
    --samples)
      samples="${2:-}"
      shift 2
      ;;
    --decode-prefix)
      decode_prefix="${2:-}"
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
if [[ "$width" -lt 128 || "$height" -lt 128 || "$rate" -lt 1 || "$samples" -lt 1 || "$decode_prefix" -lt 2 || "$keyint" -lt 2 || "$refs" -lt 1 || "$refs" -gt 16 ]]; then
  printf 'FAIL: width/height/rate/samples/decode-prefix/keyint/refs must be valid\n' >&2
  exit 1
fi
if (( width % 16 != 0 || height % 16 != 0 )); then
  printf 'FAIL: H.264 Vulkan Video source dimensions must be 16-pixel aligned; got %sx%s\n' "$width" "$height" >&2
  exit 1
fi
if [[ "$samples" -lt "$decode_prefix" ]]; then
  samples="$decode_prefix"
fi

if [[ -z "$report_dir" ]]; then
  report_dir="$(mktemp -d "${work_parent%/}/gilder-vulkan-h264-ready-prefix.XXXXXX")"
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
  if [[ "$frames" -eq 0 || "$frames" -lt $((decode_prefix + 2)) ]]; then
    frames=$((decode_prefix + 2))
  fi
  if [[ "$keyint" -le "$decode_prefix" ]]; then
    keyint=$((decode_prefix + 1))
  fi
  source="$generated_dir/h264-high-ippp-ref${refs}-${width}x${height}-${rate}fps-${frames}frames.mp4"
  ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i "testsrc2=size=${width}x${height}:rate=${rate}" \
    -frames:v "$frames" -an \
    -c:v libx264 -profile:v high -level:v "$level" -preset veryfast -tune zerolatency -pix_fmt yuv420p \
    -x264-params "keyint=${keyint}:min-keyint=${keyint}:scenecut=0:open-gop=0:bframes=0:ref=${refs}:repeat-headers=1:cabac=1:8x8dct=1:weightp=0" \
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
  --require-h264-ready-prefix "$decode_prefix" \
  --decode-h264-ready-prefix "$decode_prefix" \
  >"$probe_json" 2>"$probe_stderr"
status=$?
set -e

if [[ "$status" -ne 0 ]]; then
  printf 'FAIL: native Vulkan H.264 ready-prefix smoke failed\n' | tee "$summary"
  printf 'source: %s\n' "$source" | tee -a "$summary"
  printf 'stderr: %s\n' "$probe_stderr" | tee -a "$summary"
  sed -n '1,160p' "$probe_stderr" >&2
  exit "$status"
fi

result="$(jq -r '.result' "$probe_json")"
codec="$(jq -r '.requested_codec' "$probe_json")"
profile="$(jq -r '.profile' "$probe_json")"
session_parameters_created="$(jq -r '.session_parameters_created // false' "$probe_json")"
ready_prefix="$(jq -r '.bitstream_extract.h264_decode_ready_prefix_count // 0' "$probe_json")"
decode_requested="$(jq -r '.h264_ready_prefix_decode_requested // false' "$probe_json")"
decode_completed="$(jq -r '.h264_ready_prefix_decode.completed // false' "$probe_json")"
decoded_count="$(jq -r '.h264_ready_prefix_decode.decoded_frame_count // 0' "$probe_json")"
reset_count="$(jq -r '.h264_ready_prefix_decode.reset_control_count // 0' "$probe_json")"
frame_count="$(jq -r '.h264_ready_prefix_decode.frames | length' "$probe_json")"
non_idr_frames="$(jq -r '[.h264_ready_prefix_decode.frames[]? | select(.idr != true)] | length' "$probe_json")"
reference_frames="$(jq -r '[.h264_ready_prefix_decode.frames[]? | select(.decode_reference_slot_count > 0)] | length' "$probe_json")"
max_reference_count="$(jq -r '[.h264_ready_prefix_decode.frames[]? | .decode_reference_slot_count] | max // 0' "$probe_json")"
readback_copied="$(jq -r '.h264_ready_prefix_decode.output_readback.copied // false' "$probe_json")"
y_nonzero="$(jq -r '.h264_ready_prefix_decode.output_readback.y_plane_nonzero_bytes // 0' "$probe_json")"
uv_nonzero="$(jq -r '.h264_ready_prefix_decode.output_readback.uv_plane_nonzero_bytes // 0' "$probe_json")"
readback_bytes="$(jq -r '.h264_ready_prefix_decode.output_readback.total_bytes // 0' "$probe_json")"

if [[ "$result" != "h264-ready-prefix-decode-output-readback-completed" || "$codec" != "h264-high-8" || "$profile" != "high-8" || "$session_parameters_created" != "true" || "$ready_prefix" -lt "$decode_prefix" || "$decode_requested" != "true" || "$decode_completed" != "true" || "$decoded_count" -ne "$decode_prefix" || "$frame_count" -ne "$decode_prefix" || "$reset_count" -lt 1 || "$non_idr_frames" -lt 1 || "$reference_frames" -lt 1 || "$max_reference_count" -lt "$refs" || "$readback_copied" != "true" || "$y_nonzero" -le 0 || "$uv_nonzero" -le 0 || "$readback_bytes" -le 0 ]]; then
  {
    printf 'FAIL: native Vulkan H.264 ready-prefix output was not valid\n'
    printf 'result: %s\n' "$result"
    printf 'codec: %s\n' "$codec"
    printf 'profile: %s\n' "$profile"
    printf 'session_parameters_created: %s\n' "$session_parameters_created"
    printf 'ready_prefix: %s\n' "$ready_prefix"
    printf 'decode_requested: %s\n' "$decode_requested"
    printf 'decode_completed: %s\n' "$decode_completed"
    printf 'decoded_count: %s\n' "$decoded_count"
    printf 'frame_count: %s\n' "$frame_count"
    printf 'reset_count: %s\n' "$reset_count"
    printf 'non_idr_frames: %s\n' "$non_idr_frames"
    printf 'reference_frames: %s\n' "$reference_frames"
    printf 'max_reference_count: %s\n' "$max_reference_count"
    printf 'readback_copied: %s\n' "$readback_copied"
    printf 'y_nonzero: %s\n' "$y_nonzero"
    printf 'uv_nonzero: %s\n' "$uv_nonzero"
    printf 'readback_bytes: %s\n' "$readback_bytes"
    printf 'probe JSON: %s\n' "$probe_json"
  } | tee "$summary"
  exit 1
fi

{
  printf 'source: %s\n' "$source"
  printf 'generated_source: %s\n' "$([[ "$generated_source" -eq 1 ]] && printf yes || printf no)"
  printf 'selected_device: %s\n' "$(jq -r '.selected_physical_device_name' "$probe_json")"
  printf 'requested_extent: %s\n' "$(jq -c '.requested_extent' "$probe_json")"
  printf 'result: %s\n' "$result"
  printf 'requested_codec: %s\n' "$codec"
  printf 'profile: %s\n' "$profile"
  printf 'generated_h264_level: %s\n' "$level"
  printf 'generated_h264_refs: %s\n' "$refs"
  printf 'samples: %s\n' "$(jq -r '.bitstream_extract.samples' "$probe_json")"
  printf 'ready_prefix: %s\n' "$ready_prefix"
  printf 'decode_prefix: %s\n' "$decode_prefix"
  printf 'decoded_frame_count: %s\n' "$decoded_count"
  printf 'reset_control_count: %s\n' "$reset_count"
  printf 'non_idr_frames: %s\n' "$non_idr_frames"
  printf 'reference_frames: %s\n' "$reference_frames"
  printf 'max_reference_count: %s\n' "$max_reference_count"
  printf 'reference_plan_dpb_slots: %s\n' "$(jq -r '.bitstream_extract.h264_reference_plan_dpb_slots' "$probe_json")"
  printf 'src_buffer_total_bytes: %s\n' "$(jq -r '.h264_ready_prefix_decode.src_buffer_total_bytes' "$probe_json")"
  printf 'frame_offsets: %s\n' "$(jq -c '[.h264_ready_prefix_decode.frames[] | .src_buffer_offset]' "$probe_json")"
  printf 'frame_ranges: %s\n' "$(jq -c '[.h264_ready_prefix_decode.frames[] | .src_buffer_range]' "$probe_json")"
  printf 'frame_reference_counts: %s\n' "$(jq -c '[.h264_ready_prefix_decode.frames[] | .decode_reference_slot_count]' "$probe_json")"
  printf 'requested_reference_counts: %s\n' "$(jq -c '[.bitstream_extract.h264_decode_reference_plan[] | .requested_reference_count]' "$probe_json")"
  printf 'planned_output_slots: %s\n' "$(jq -c '[.bitstream_extract.h264_decode_reference_plan[] | .planned_output_slot]' "$probe_json")"
  printf 'slice_types: %s\n' "$(jq -c '[.h264_ready_prefix_decode.frames[] | .slice_type]' "$probe_json")"
  printf 'readback_access_unit_index: %s\n' "$(jq -r '.h264_ready_prefix_decode.readback_access_unit_index' "$probe_json")"
  printf 'readback_bytes: %s\n' "$readback_bytes"
  printf 'y_plane_nonzero_bytes: %s\n' "$y_nonzero"
  printf 'uv_plane_nonzero_bytes: %s\n' "$uv_nonzero"
  printf 'y_plane_hash: %s\n' "$(jq -r '.h264_ready_prefix_decode.output_readback.y_plane_hash' "$probe_json")"
  printf 'uv_plane_hash: %s\n' "$(jq -r '.h264_ready_prefix_decode.output_readback.uv_plane_hash' "$probe_json")"
  printf 'bitstream_buffer_bytes: %s\n' "$(jq -r '.bitstream_buffer.size' "$probe_json")"
  printf 'session_memory_bytes: %s\n' "$(jq -r '.total_bound_memory_bytes' "$probe_json")"
  printf 'video_resource_memory_bytes: %s\n' "$(jq -r '.total_video_image_memory_bytes' "$probe_json")"
} >"$summary"

printf 'PASS: native Vulkan H.264 ready-prefix smoke passed\n'
