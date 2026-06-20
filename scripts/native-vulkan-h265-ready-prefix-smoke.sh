#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
usage: scripts/native-vulkan-h265-ready-prefix-smoke.sh [options]

Generate or use a 4K/240 H.265 Main source, then run the native Vulkan Video
session/bitstream probe and require a continuous ready H.265 decode prefix.
This does not create a visible Wayland surface.

Options:
  --display <name>      Wayland display name. Default: WAYLAND_DISPLAY.
  --source <path>       Existing H.265 source. Default: generate short-GOP source.
  --report-dir <dir>    Exact evidence directory. Created and kept.
  --work-dir <dir>      Parent directory for generated evidence. Default: /tmp.
  --samples <count>     AU samples to collect. Default: 8.
  --required-ready-prefix <count>
                        Required continuous ready AU prefix. Default: 8.
  --decode-prefix <count>
                        Also decode this many ready-prefix AUs and read back the final decoded
                        frame. Default: 0.
  --width <px>          Generated/probed width. Default: 3840.
  --height <px>         Generated/probed height. Default: 2160.
  --rate <fps>          Generated frame rate. Default: 240.
  --frames <count>      Generated frame count. Default: samples + 2.
  --no-build            Reuse existing target/release/gilder-native-vulkan.
  -h, --help            Show this help text.
EOF
}

display="${WAYLAND_DISPLAY:-}"
source=""
report_dir=""
work_parent="${TMPDIR:-/tmp}"
samples=8
required_ready_prefix=8
decode_prefix=0
width=3840
height=2160
rate=240
frames=0
no_build=0

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
    --samples)
      samples="${2:-}"
      shift 2
      ;;
    --required-ready-prefix)
      required_ready_prefix="${2:-}"
      shift 2
      ;;
    --decode-prefix)
      decode_prefix="${2:-}"
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
    --frames)
      frames="${2:-}"
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

if [[ "$samples" -lt 1 || "$required_ready_prefix" -lt 1 || "$decode_prefix" -lt 0 ]]; then
  printf 'FAIL: --samples and --required-ready-prefix must be positive, --decode-prefix must be non-negative\n' >&2
  exit 1
fi

if [[ -z "$report_dir" ]]; then
  report_dir="$(mktemp -d "${work_parent%/}/gilder-vulkan-h265-ready-prefix.XXXXXX")"
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
  source="$generated_dir/h265-main-short-gop-${width}x${height}-${rate}fps.mp4"
  if [[ "$frames" -eq 0 || "$frames" -lt $((samples + 2)) ]]; then
    frames=$((samples + 2))
  fi
  ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i "testsrc2=size=${width}x${height}:rate=${rate}" \
    -frames:v "$frames" -an \
    -c:v libx265 -profile:v main -preset ultrafast -tune zerolatency -pix_fmt yuv420p \
    -x265-params "keyint=2:min-keyint=2:scenecut=0:open-gop=0:bframes=0:ref=1:repeat-headers=1:hrd=0:rc-lookahead=0" \
    "$source"
fi

if [[ ! -f "$source" ]]; then
  printf 'FAIL: source does not exist: %s\n' "$source" >&2
  exit 1
fi

probe_json="$report_dir/probe.json"
probe_stderr="$report_dir/probe.stderr"
summary="$report_dir/summary.txt"
decode_args=()
if [[ "$decode_prefix" -gt 0 ]]; then
  decode_args=(--decode-h265-ready-prefix "$decode_prefix")
fi

set +e
env WAYLAND_DISPLAY="$display" \
  target/release/gilder-native-vulkan \
  --probe-video-session \
  --video-codec h265 \
  --width "$width" \
  --height "$height" \
  --extract-bitstream \
  --source "$source" \
  --bitstream-samples "$samples" \
  "${decode_args[@]}" \
  --require-h265-ready-prefix "$required_ready_prefix" \
  >"$probe_json" 2>"$probe_stderr"
probe_status=$?
set -e

if [[ "$probe_status" -ne 0 ]]; then
  printf 'FAIL: native Vulkan H.265 ready-prefix probe failed\n' | tee "$summary"
  printf 'source: %s\n' "$source" | tee -a "$summary"
  printf 'stderr: %s\n' "$probe_stderr" | tee -a "$summary"
  sed -n '1,120p' "$probe_stderr" >&2
  exit "$probe_status"
fi

if [[ "$decode_prefix" -gt 0 ]]; then
  decoded_frames="$(jq -r '.h265_ready_prefix_decode.decoded_frame_count // 0' "$probe_json")"
  y_unique="$(jq -r '.h265_ready_prefix_decode.output_readback.y_plane_unique_values // 0' "$probe_json")"
  uv_unique="$(jq -r '.h265_ready_prefix_decode.output_readback.uv_plane_unique_values // 0' "$probe_json")"
  if [[ "$decoded_frames" -ne "$decode_prefix" || "$y_unique" -le 1 || "$uv_unique" -le 1 ]]; then
    {
      printf 'FAIL: native Vulkan H.265 decode-prefix output was not valid\n'
      printf 'decoded_frames: %s\n' "$decoded_frames"
      printf 'requested_decode_prefix: %s\n' "$decode_prefix"
      printf 'y_unique: %s\n' "$y_unique"
      printf 'uv_unique: %s\n' "$uv_unique"
      printf 'probe JSON: %s\n' "$probe_json"
    } | tee "$summary"
    exit 1
  fi
fi

{
  printf 'result: %s\n' "$(jq -r '.result' "$probe_json")"
  printf 'source: %s\n' "$source"
  printf 'samples: %s\n' "$(jq -r '.bitstream_extract.samples' "$probe_json")"
  printf 'required_ready_prefix: %s\n' "$required_ready_prefix"
  printf 'h265_decode_ready_prefix_count: %s\n' "$(jq -r '.bitstream_extract.h265_decode_ready_prefix_count' "$probe_json")"
  printf 'h265_decode_ready_count: %s\n' "$(jq -r '.bitstream_extract.h265_decode_ready_count' "$probe_json")"
  printf 'decode_prefix_requested: %s\n' "$decode_prefix"
  printf 'decode_prefix_completed: %s\n' "$(jq -r '.h265_ready_prefix_decode.completed // false' "$probe_json")"
  printf 'decode_prefix_decoded_frames: %s\n' "$(jq -r '.h265_ready_prefix_decode.decoded_frame_count // 0' "$probe_json")"
  printf 'decode_prefix_readback_au: %s\n' "$(jq -r '.h265_ready_prefix_decode.readback_access_unit_index // "none"' "$probe_json")"
  printf 'decode_prefix_readback_layer: %s\n' "$(jq -r '.h265_ready_prefix_decode.readback_base_array_layer // "none"' "$probe_json")"
  printf 'decode_prefix_readback_y_unique: %s\n' "$(jq -r '.h265_ready_prefix_decode.output_readback.y_plane_unique_values // "none"' "$probe_json")"
  printf 'decode_prefix_readback_uv_unique: %s\n' "$(jq -r '.h265_ready_prefix_decode.output_readback.uv_plane_unique_values // "none"' "$probe_json")"
  printf 'session_parameters_created: %s\n' "$(jq -r '.session_parameters_created' "$probe_json")"
  printf 'session_parameters_error: %s\n' "$(jq -r '.session_parameters_error // "none"' "$probe_json")"
  printf 'profile: %s\n' "$(jq -r '.bitstream_extract.h265_parameter_sets.sps.profile_label' "$probe_json")"
  printf 'vui_hrd_parameters_present: %s\n' "$(jq -r '.bitstream_extract.h265_parameter_sets.sps.vui.vui_hrd_parameters_present_flag // false' "$probe_json")"
  printf 'first_unready_access_unit: %s\n' "$(jq -r '.bitstream_extract.h265_decode_first_unready_access_unit_index // "none"' "$probe_json")"
} >"$summary"

printf 'PASS: native Vulkan H.265 ready-prefix smoke passed\n'
printf 'summary: %s\n' "$summary"
printf 'probe JSON: %s\n' "$probe_json"
