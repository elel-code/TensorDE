#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
usage: scripts/native-vulkan-av1-bitstream-smoke.sh [options]

Generate or use an AV1 Main source, then verify the native Vulkan Video AV1
session can ingest parsed encoded temporal units through GStreamer demux/parser
without using a GStreamer display sink.

Options:
  --display <name>      Wayland display name. Default: WAYLAND_DISPLAY.
  --source <path>       Existing AV1 source. Default: generate WebM/AV1 source.
  --report-dir <dir>    Exact evidence directory. Created and kept.
  --work-dir <dir>      Parent directory for generated evidence. Default: /tmp.
  --width <px>          Generated/probed width. Default: 640.
  --height <px>         Generated/probed height. Default: 368.
  --target-fps <fps>    Generated source FPS. Default: 60.
  --frames <count>      Generated frame count. Default: target-fps.
  --bit-depth <8|10>    Generated/probed AV1 Main bit depth. Default: 8.
  --bitstream-samples <n>
                        Parsed temporal units to collect. Default: 8.
  --no-build            Reuse existing target/release/gilder-native-vulkan.
  --keep                Compatibility no-op; evidence directories are always kept.
  -h, --help            Show this help text.
EOF
}

display="${WAYLAND_DISPLAY:-}"
source=""
report_dir=""
work_parent="${TMPDIR:-/tmp}"
width=640
height=368
target_fps=60
frames=0
bit_depth=8
bitstream_samples=8
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
    --target-fps)
      target_fps="${2:-}"
      shift 2
      ;;
    --frames)
      frames="${2:-}"
      shift 2
      ;;
    --bit-depth)
      bit_depth="${2:-}"
      shift 2
      ;;
    --bitstream-samples)
      bitstream_samples="${2:-}"
      shift 2
      ;;
    --no-build)
      no_build=1
      shift
      ;;
    --keep)
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
if [[ "$width" -lt 128 || "$height" -lt 128 || "$target_fps" -lt 1 || "$bitstream_samples" -lt 1 ]]; then
  printf 'FAIL: width/height/target-fps/bitstream-samples must be valid\n' >&2
  exit 1
fi
if [[ "$bit_depth" != "8" && "$bit_depth" != "10" ]]; then
  printf 'FAIL: --bit-depth must be 8 or 10\n' >&2
  exit 1
fi
if (( width % 16 != 0 || height % 16 != 0 )); then
  printf 'FAIL: AV1 Vulkan Video source dimensions must be 16-pixel aligned; got %sx%s\n' "$width" "$height" >&2
  exit 1
fi
if [[ "$bit_depth" == "10" ]]; then
  pix_fmt="yuv420p10le"
  video_codec="av1-main-10"
else
  pix_fmt="yuv420p"
  video_codec="av1-main-8"
fi

if [[ -z "$report_dir" ]]; then
  report_dir="$(mktemp -d "${work_parent%/}/gilder-vulkan-av1-bitstream.XXXXXX")"
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
  if [[ "$frames" -eq 0 ]]; then
    frames="$target_fps"
  fi
  source="$generated_dir/${video_codec}-${width}x${height}-${target_fps}fps-${frames}frames.webm"
  ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i "testsrc2=size=${width}x${height}:rate=${target_fps}:duration=$(( (frames + target_fps - 1) / target_fps ))" \
    -frames:v "$frames" -an \
    -c:v libaom-av1 -cpu-used 8 -crf 45 -b:v 0 -row-mt 1 -pix_fmt "$pix_fmt" \
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
  --video-codec "$video_codec" \
  --source "$source" \
  --width "$width" \
  --height "$height" \
  --extract-bitstream \
  --allocate-bitstream-buffer \
  --bitstream-samples "$bitstream_samples" \
  >"$probe_json" 2>"$probe_stderr"
status=$?
set -e

if [[ "$status" -ne 0 ]]; then
  printf 'FAIL: native Vulkan AV1 bitstream smoke failed\n' | tee "$summary"
  printf 'source: %s\n' "$source" | tee -a "$summary"
  printf 'stderr: %s\n' "$probe_stderr" | tee -a "$summary"
  sed -n '1,160p' "$probe_stderr" >&2
  exit "$status"
fi

codec="$(jq -r '.requested_codec' "$probe_json")"
samples="$(jq -r '.bitstream_extract.samples // 0' "$probe_json")"
frontend="$(jq -r '.bitstream_extract.frontend // "none"' "$probe_json")"
stream_format="$(jq -r '.bitstream_extract.stream_format // "none"' "$probe_json")"
alignment="$(jq -r '.bitstream_extract.alignment // "none"' "$probe_json")"
sequence_header_present="$(jq -r '.bitstream_extract.av1_sequence_header_present // false' "$probe_json")"
obu_count="$(jq -r '.bitstream_extract.av1_obu_count // 0' "$probe_json")"
sequence_header_count="$(jq -r '.bitstream_extract.av1_sequence_header_count // 0' "$probe_json")"
frame_count="$(jq -r '.bitstream_extract.av1_frame_count // 0' "$probe_json")"
decode_candidate="$(jq -r '.bitstream_extract.av1_decode_candidate // false' "$probe_json")"
tile_payload_bytes="$(jq -r '.bitstream_extract.av1_tile_payload_bytes // 0' "$probe_json")"
frame_payload_bytes="$(jq -r '.bitstream_extract.av1_frame_payload_bytes // 0' "$probe_json")"
sequence_profile="$(jq -r '.bitstream_extract.av1_sequence_header.seq_profile_label // "none"' "$probe_json")"
sequence_bit_depth="$(jq -r '.bitstream_extract.av1_sequence_header.color_config.bit_depth // 0' "$probe_json")"
sequence_width="$(jq -r '.bitstream_extract.av1_sequence_header.max_frame_width // 0' "$probe_json")"
sequence_height="$(jq -r '.bitstream_extract.av1_sequence_header.max_frame_height // 0' "$probe_json")"
sequence_std_ready="$(jq -r '.bitstream_extract.av1_sequence_header.vulkan_std_session_parameters_ready // false' "$probe_json")"
session_parameters_created="$(jq -r '.session_parameters_created // false' "$probe_json")"
session_parameters_codec="$(jq -r '.session_parameters.codec // "none"' "$probe_json")"
session_parameters_source="$(jq -r '.session_parameters.source // "none"' "$probe_json")"
mapped_write_source="$(jq -r '.bitstream_buffer.mapped_write_source // "none"' "$probe_json")"
mapped_write_bytes="$(jq -r '.bitstream_buffer.mapped_write_bytes // 0' "$probe_json")"

if [[ "$codec" != "$video_codec" || "$samples" -lt 1 || "$frontend" != "gstreamer-demux-av1parse-appsink" || "$stream_format" != "obu-stream" || "$alignment" != "tu" || "$sequence_header_present" != "true" || "$obu_count" -lt 1 || "$sequence_header_count" -lt 1 || "$frame_count" -lt 1 || "$decode_candidate" != "true" || "$sequence_profile" != "main" || "$sequence_bit_depth" -ne "$bit_depth" || "$sequence_width" -ne "$width" || "$sequence_height" -ne "$height" || "$sequence_std_ready" != "true" || "$session_parameters_created" != "true" || "$session_parameters_codec" != "$video_codec" || "$session_parameters_source" != "native-rust-av1-sequence-header-to-vulkan-std" || "$mapped_write_source" != "extracted-encoded-video-unit" || "$mapped_write_bytes" -le 0 ]]; then
  {
    printf 'FAIL: native Vulkan AV1 bitstream output was not valid\n'
    printf 'codec: %s\n' "$codec"
    printf 'samples: %s\n' "$samples"
    printf 'frontend: %s\n' "$frontend"
    printf 'stream_format: %s\n' "$stream_format"
    printf 'alignment: %s\n' "$alignment"
    printf 'sequence_header_present: %s\n' "$sequence_header_present"
    printf 'obu_count: %s\n' "$obu_count"
    printf 'sequence_header_count: %s\n' "$sequence_header_count"
    printf 'frame_count: %s\n' "$frame_count"
    printf 'decode_candidate: %s\n' "$decode_candidate"
    printf 'tile_payload_bytes: %s\n' "$tile_payload_bytes"
    printf 'frame_payload_bytes: %s\n' "$frame_payload_bytes"
    printf 'sequence_profile: %s\n' "$sequence_profile"
    printf 'sequence_bit_depth: %s\n' "$sequence_bit_depth"
    printf 'sequence_width: %s\n' "$sequence_width"
    printf 'sequence_height: %s\n' "$sequence_height"
    printf 'sequence_std_ready: %s\n' "$sequence_std_ready"
    printf 'session_parameters_created: %s\n' "$session_parameters_created"
    printf 'session_parameters_codec: %s\n' "$session_parameters_codec"
    printf 'session_parameters_source: %s\n' "$session_parameters_source"
    printf 'mapped_write_source: %s\n' "$mapped_write_source"
    printf 'mapped_write_bytes: %s\n' "$mapped_write_bytes"
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
  printf 'requested_bit_depth: %s\n' "$bit_depth"
  printf 'samples: %s\n' "$samples"
  printf 'frontend: %s\n' "$frontend"
  printf 'stream_format: %s\n' "$stream_format"
  printf 'alignment: %s\n' "$alignment"
  printf 'selected_access_unit_bytes: %s\n' "$(jq -r '.bitstream_extract.selected_access_unit_bytes' "$probe_json")"
  printf 'av1_sequence_header_present: %s\n' "$sequence_header_present"
  printf 'av1_obu_count: %s\n' "$obu_count"
  printf 'av1_sequence_header_count: %s\n' "$sequence_header_count"
  printf 'av1_temporal_delimiter_count: %s\n' "$(jq -r '.bitstream_extract.av1_temporal_delimiter_count' "$probe_json")"
  printf 'av1_frame_header_count: %s\n' "$(jq -r '.bitstream_extract.av1_frame_header_count' "$probe_json")"
  printf 'av1_tile_group_count: %s\n' "$(jq -r '.bitstream_extract.av1_tile_group_count' "$probe_json")"
  printf 'av1_frame_count: %s\n' "$frame_count"
  printf 'av1_decode_candidate: %s\n' "$decode_candidate"
  printf 'av1_tile_payload_bytes: %s\n' "$tile_payload_bytes"
  printf 'av1_frame_payload_bytes: %s\n' "$frame_payload_bytes"
  printf 'av1_first_frame_header_obu_offset: %s\n' "$(jq -r '.bitstream_extract.av1_first_frame_header_obu_offset // "none"' "$probe_json")"
  printf 'av1_first_tile_group_obu_offset: %s\n' "$(jq -r '.bitstream_extract.av1_first_tile_group_obu_offset // "none"' "$probe_json")"
  printf 'av1_sequence_profile: %s\n' "$sequence_profile"
  printf 'av1_sequence_bit_depth: %s\n' "$sequence_bit_depth"
  printf 'av1_sequence_extent: %sx%s\n' "$sequence_width" "$sequence_height"
  printf 'av1_vulkan_std_session_parameters_ready: %s\n' "$sequence_std_ready"
  printf 'session_parameters_created: %s\n' "$session_parameters_created"
  printf 'session_parameters_codec: %s\n' "$session_parameters_codec"
  printf 'session_parameters_source: %s\n' "$session_parameters_source"
  printf 'av1_obus_head: %s\n' "$(jq -c '[.bitstream_extract.av1_obus[0:8][]?]' "$probe_json")"
  printf 'mapped_write_source: %s\n' "$mapped_write_source"
  printf 'mapped_write_bytes: %s\n' "$mapped_write_bytes"
  printf 'bitstream_buffer_bytes: %s\n' "$(jq -r '.bitstream_buffer.size' "$probe_json")"
  printf 'session_memory_bytes: %s\n' "$(jq -r '.total_bound_memory_bytes' "$probe_json")"
} >"$summary"

printf 'PASS: native Vulkan AV1 bitstream smoke passed\n'
printf 'summary: %s\n' "$summary"
printf 'probe JSON: %s\n' "$probe_json"
