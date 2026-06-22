#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
usage: scripts/native-vulkan-av1-ready-prefix-video-smoke.sh [options]

Generate or use an AV1 source, then verify native Vulkan Video AV1 direct
decode/present on a real Wayland output.

Options:
  --source <path>                 Existing AV1 source. Default: generate source.
  --output-name <name>            Wayland output name, for example HDMI-A-1.
  --width <px>                    Source width. Default: 640.
  --height <px>                   Source height. Default: 368.
  --target-fps <fps>              Decode/present target FPS. Default: 60.
  --frames <n>                    Generated source frames. Default: decode-prefix + 2.
  --decode-prefix <n>             Bootstrap ready TU window. Default: 60.
  --playback-frames <n>           Presented frames. Default: decode-prefix.
  --bit-depth <8|10>              Generated/probed AV1 Main bit depth. Default: 10.
  --arbitrary-entry-offset <sec>  Generate a non-keyframe entry source with ffmpeg -copyinkf.
  --require-loop-skip-replay      Require EOS loop replay to skip leading non-key TUs.
  --report-dir <path>             Report directory. Default: mktemp under /tmp.
  --no-build                      Reuse target/release/gilder-native-vulkan.
  -h, --help                      Show this help.
USAGE
}

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
cd "$repo_root"

source=""
output_name="${GILDER_WAYLAND_OUTPUT:-}"
width=640
height=368
target_fps=60
frames=0
frames_explicit=0
decode_prefix=60
playback_frames=0
bit_depth=10
arbitrary_entry_offset=""
require_loop_skip_replay=0
report_dir=""
no_build=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --source)
      source="${2:?--source requires a path}"
      shift 2
      ;;
    --output-name)
      output_name="${2:?--output-name requires a value}"
      shift 2
      ;;
    --width)
      width="${2:?--width requires a value}"
      shift 2
      ;;
    --height)
      height="${2:?--height requires a value}"
      shift 2
      ;;
    --target-fps)
      target_fps="${2:?--target-fps requires a value}"
      shift 2
      ;;
    --frames)
      frames="${2:?--frames requires a value}"
      frames_explicit=1
      shift 2
      ;;
    --decode-prefix)
      decode_prefix="${2:?--decode-prefix requires a value}"
      shift 2
      ;;
    --playback-frames)
      playback_frames="${2:?--playback-frames requires a value}"
      shift 2
      ;;
    --bit-depth)
      bit_depth="${2:?--bit-depth requires a value}"
      shift 2
      ;;
    --arbitrary-entry-offset)
      arbitrary_entry_offset="${2:?--arbitrary-entry-offset requires seconds}"
      shift 2
      ;;
    --require-loop-skip-replay)
      require_loop_skip_replay=1
      shift
      ;;
    --report-dir)
      report_dir="${2:?--report-dir requires a path}"
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
      printf 'unknown argument: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ "$width" -le 0 || "$height" -le 0 || "$target_fps" -le 0 || "$decode_prefix" -le 0 ]]; then
  printf 'FAIL: width/height/target-fps/decode-prefix must be positive\n' >&2
  exit 1
fi
if (( width % 16 != 0 || height % 16 != 0 )); then
  printf 'FAIL: AV1 Vulkan Video source dimensions must be 16-pixel aligned; got %sx%s\n' "$width" "$height" >&2
  exit 1
fi
if [[ "$bit_depth" != "8" && "$bit_depth" != "10" ]]; then
  printf 'FAIL: --bit-depth must be 8 or 10\n' >&2
  exit 1
fi

if [[ "$playback_frames" -eq 0 ]]; then
  playback_frames="$decode_prefix"
fi

video_codec="av1-main-8"
pix_fmt="yuv420p"
expected_picture_format="G8_B8R8_2PLANE_420_UNORM"
if [[ "$bit_depth" -eq 10 ]]; then
  video_codec="av1-main-10"
  pix_fmt="yuv420p10le"
  expected_picture_format="G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16"
fi

if [[ -z "$report_dir" ]]; then
  report_dir="$(mktemp -d /tmp/gilder-vulkan-av1-ready-prefix-video.XXXXXX)"
else
  mkdir -p "$report_dir"
fi
summary="$report_dir/summary.txt"
runtime_json="$report_dir/runtime.json"
stderr_log="$report_dir/stderr.log"
generated_dir="$report_dir/source"
mkdir -p "$generated_dir"

if [[ "$no_build" -eq 0 ]]; then
  cargo build --release --features native-vulkan-gst-video --bin gilder-native-vulkan
fi

if [[ -z "$source" ]]; then
  if [[ "$frames" -eq 0 || "$frames" -lt $((decode_prefix + 2)) ]]; then
    frames=$((decode_prefix + 2))
  fi
  if [[ "$frames_explicit" -eq 0 && -n "$arbitrary_entry_offset" ]]; then
    offset_frames="$(awk -v offset="$arbitrary_entry_offset" -v fps="$target_fps" 'BEGIN { value = offset * fps; printf "%d", (value == int(value)) ? value : int(value) + 1 }')"
    arbitrary_window_frames="$playback_frames"
    if [[ "$require_loop_skip_replay" -eq 1 || "$playback_frames" -gt "$decode_prefix" ]]; then
      arbitrary_window_frames="$decode_prefix"
    fi
    arbitrary_min_frames=$((offset_frames + decode_prefix + arbitrary_window_frames + 2))
    if [[ "$frames" -lt "$arbitrary_min_frames" ]]; then
      frames="$arbitrary_min_frames"
    fi
  fi
  source_duration_seconds=$(( (frames + target_fps - 1) / target_fps ))
  base_source="$generated_dir/av1-main${bit_depth}-${width}x${height}-${target_fps}fps-${frames}frames.webm"
  ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i "testsrc2=size=${width}x${height}:rate=${target_fps}:duration=${source_duration_seconds}" \
    -frames:v "$frames" -an -c:v libaom-av1 -cpu-used 8 -crf 40 -b:v 0 -row-mt 1 \
    -g "$decode_prefix" -pix_fmt "$pix_fmt" "$base_source"
  source="$base_source"
  if [[ -n "$arbitrary_entry_offset" ]]; then
    arbitrary_source="$generated_dir/av1-main${bit_depth}-${width}x${height}-${target_fps}fps-arbitrary-${arbitrary_entry_offset}s.webm"
    ffmpeg -hide_banner -loglevel error -y \
      -i "$base_source" -ss "$arbitrary_entry_offset" \
      -c copy -copyinkf -avoid_negative_ts make_zero \
      "$arbitrary_source"
    source="$arbitrary_source"
  fi
fi

cmd=(
  target/release/gilder-native-vulkan
  --run-av1-ready-prefix-video
  --source "$source"
  --video-codec "$video_codec"
  --width "$width"
  --height "$height"
  --target-fps "$target_fps"
  --decode-av1-ready-prefix "$decode_prefix"
  --playback-frames "$playback_frames"
)
if [[ -n "$output_name" ]]; then
  cmd+=(--output-name "$output_name")
fi

if ! "${cmd[@]}" >"$runtime_json" 2>"$stderr_log"; then
  {
    printf 'FAIL: native Vulkan AV1 ready-prefix visible runtime failed\n'
    printf 'source: %s\n' "$source"
    printf 'stderr:\n'
    sed -n '1,120p' "$stderr_log"
  } | tee "$summary"
  exit 1
fi

requested_codec="$(jq -r '.requested_codec // "none"' "$runtime_json")"
picture_format="$(jq -r '.picture_format // "none"' "$runtime_json")"
decoded_count="$(jq -r '.decoded_frame_count // 0' "$runtime_json")"
handoff_count="$(jq -r '.displayed_handoff_frame_count // 0' "$runtime_json")"
presented_count="$(jq -r '.presented_frame_count // 0' "$runtime_json")"
requested_playback_count="$(jq -r '.requested_playback_frame_count // 0' "$runtime_json")"
average_present_fps="$(jq -r '.average_present_fps // 0' "$runtime_json")"
configured="$(jq -r '.configured // false' "$runtime_json")"
queue_capacity="$(jq -r '.av1_packet_queue_capacity // 0' "$runtime_json")"
queue_pulled_count="$(jq -r '.av1_packet_queue_pulled_count // 0' "$runtime_json")"
queue_eos_count="$(jq -r '.av1_packet_queue_eos_count // 0' "$runtime_json")"
queue_loop_count="$(jq -r '.av1_packet_queue_loop_count // 0' "$runtime_json")"
queue_loop_skip_temporal_units="$(jq -r '.av1_packet_queue_loop_skip_temporal_units // 0' "$runtime_json")"
queue_bootstrap_discarded_temporal_units="$(jq -r '.av1_packet_queue_bootstrap_discarded_temporal_units // 0' "$runtime_json")"
queue_retained_payload_bytes="$(jq -r '.av1_packet_queue_retained_payload_bytes // 0' "$runtime_json")"
distinct_layers="$(jq -r '[.frames[]?.displayed_base_array_layer] | unique | length' "$runtime_json")"
bad_frames="$(jq -r '[.frames[]? | select((.show_existing_frame | not) and ((.tile_count <= 0) or (.src_buffer_range <= 0)))] | length' "$runtime_json")"
loop_boundary_reset_count="$(jq -r '.loop_boundary_reset_count // 0' "$runtime_json")"
playback_loop_count="$(jq -r '.playback_loop_count // 0' "$runtime_json")"

expected_frames="$playback_frames"
loop_replay_gate_failed=0
if [[ "$require_loop_skip_replay" -eq 1 && ( "$queue_eos_count" -le 0 || "$queue_loop_count" -le 0 || "$playback_loop_count" -le 1 || "$loop_boundary_reset_count" -le 0 ) ]]; then
  loop_replay_gate_failed=1
fi

if [[ "$requested_codec" != "$video_codec" || "$picture_format" != "$expected_picture_format" || "$presented_count" -ne "$expected_frames" || "$requested_playback_count" -ne "$expected_frames" || $((decoded_count + handoff_count)) -ne "$expected_frames" || "$configured" != "true" || "$queue_capacity" -le 0 || "$queue_pulled_count" -lt "$expected_frames" || "$queue_retained_payload_bytes" -ne 0 || "$distinct_layers" -le 0 || "$bad_frames" -ne 0 || "$loop_replay_gate_failed" -ne 0 ]]; then
  {
    printf 'FAIL: native Vulkan AV1 ready-prefix visible runtime output was not valid\n'
    printf 'requested_codec: %s\n' "$requested_codec"
    printf 'picture_format: %s\n' "$picture_format"
    printf 'decoded_frame_count: %s\n' "$decoded_count"
    printf 'displayed_handoff_frame_count: %s\n' "$handoff_count"
    printf 'presented_frame_count: %s\n' "$presented_count"
    printf 'requested_playback_frame_count: %s\n' "$requested_playback_count"
    printf 'average_present_fps: %s\n' "$average_present_fps"
    printf 'configured: %s\n' "$configured"
    printf 'av1_packet_queue_capacity: %s\n' "$queue_capacity"
    printf 'av1_packet_queue_pulled_count: %s\n' "$queue_pulled_count"
    printf 'av1_packet_queue_eos_count: %s\n' "$queue_eos_count"
    printf 'av1_packet_queue_loop_count: %s\n' "$queue_loop_count"
    printf 'av1_packet_queue_loop_skip_temporal_units: %s\n' "$queue_loop_skip_temporal_units"
    printf 'av1_packet_queue_bootstrap_discarded_temporal_units: %s\n' "$queue_bootstrap_discarded_temporal_units"
    printf 'av1_packet_queue_retained_payload_bytes: %s\n' "$queue_retained_payload_bytes"
    printf 'distinct_displayed_layers: %s\n' "$distinct_layers"
    printf 'bad_frames: %s\n' "$bad_frames"
    printf 'loop_replay_gate_failed: %s\n' "$loop_replay_gate_failed"
  } | tee "$summary"
  exit 1
fi

{
  printf 'PASS: native Vulkan AV1 ready-prefix visible runtime passed\n'
  printf 'source: %s\n' "$source"
  printf 'requested_codec: %s\n' "$requested_codec"
  printf 'picture_format: %s\n' "$picture_format"
  printf 'decoded_frame_count: %s\n' "$decoded_count"
  printf 'displayed_handoff_frame_count: %s\n' "$handoff_count"
  printf 'presented_frame_count: %s\n' "$presented_count"
  printf 'average_present_fps: %s\n' "$average_present_fps"
  printf 'av1_packet_queue_eos_count: %s\n' "$queue_eos_count"
  printf 'av1_packet_queue_loop_count: %s\n' "$queue_loop_count"
  printf 'av1_packet_queue_loop_skip_temporal_units: %s\n' "$queue_loop_skip_temporal_units"
  printf 'av1_packet_queue_bootstrap_discarded_temporal_units: %s\n' "$queue_bootstrap_discarded_temporal_units"
  printf 'runtime_json: %s\n' "$runtime_json"
} | tee "$summary"
