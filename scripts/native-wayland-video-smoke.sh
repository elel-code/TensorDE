#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
usage: scripts/native-wayland-video-smoke.sh [options]

Run the experimental native Wayland video helper on a real Wayland display and
sample process memory, CPU, NVIDIA process memory, and native runtime JSON.

Options:
  --source <path>       Existing video source. If omitted, generate one.
  --display <name>      Wayland display name. Default: WAYLAND_DISPLAY.
  --work-dir <dir>      Parent directory for temporary data. Default: /tmp.
  --report-dir <dir>    Exact evidence directory. Created and kept.
  --sample-duration <s> Sampling duration. Default: 8.
  --sample-interval <s> Sampling interval. Default: 1.
  --video-size <WxH>    Generated video size. Default: 3840x2160.
  --video-rate <fps>    Generated video frame rate. Default: 240.
  --video-duration <s>  Generated/repeated playback duration. Default:
                        sample duration + warmup + 3 seconds.
  --target-max-fps <n>  Native max-lateness hint. Default: 240.
  --no-fps-limit        Disable native max-lateness hint.
  --sink-throttle       Also enforce target with waylandsink throttle-time.
  --pipeline <name>     playbin or playbin3. Default: playbin.
  --layer <name>        background, bottom, top, or overlay. Default: background.
  --decoder <policy>    auto, hardware-preferred, hardware-required, or software.
                        Default: hardware-preferred.
  --no-runtime-json     Do not collect native runtime JSONL during sampling.
  --no-opaque-region    Do not mark the native surface as opaque.
  --no-input-passthrough
                        Keep default compositor input region.
  --no-build            Reuse existing target/release/gilder-native-video.
  --keep                Keep temporary data when --report-dir is not used.
  -h, --help            Show this help text.
EOF
}

work_parent="${TMPDIR:-/tmp}"
report_dir=""
source_path=""
wayland_display="${WAYLAND_DISPLAY:-}"
sample_duration=8
sample_interval=1
video_size="3840x2160"
video_rate=240
video_duration=""
target_max_fps=240
fps_limit=1
sink_throttle=0
pipeline="playbin"
layer="background"
decoder="hardware-preferred"
runtime_json_enabled=1
opaque_region=1
input_passthrough=1
build=1
keep=0
warmup_seconds=2

while [[ $# -gt 0 ]]; do
  case "$1" in
    --source)
      [[ $# -ge 2 ]] || { echo "--source requires a path" >&2; exit 2; }
      source_path="$2"
      shift 2
      ;;
    --display)
      [[ $# -ge 2 ]] || { echo "--display requires a value" >&2; exit 2; }
      wayland_display="$2"
      shift 2
      ;;
    --work-dir)
      [[ $# -ge 2 ]] || { echo "--work-dir requires a directory" >&2; exit 2; }
      work_parent="$2"
      shift 2
      ;;
    --report-dir)
      [[ $# -ge 2 ]] || { echo "--report-dir requires a directory" >&2; exit 2; }
      report_dir="$2"
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
    --video-size)
      [[ $# -ge 2 ]] || { echo "--video-size requires WxH" >&2; exit 2; }
      video_size="$2"
      shift 2
      ;;
    --video-rate)
      [[ $# -ge 2 ]] || { echo "--video-rate requires fps" >&2; exit 2; }
      video_rate="$2"
      shift 2
      ;;
    --video-duration)
      [[ $# -ge 2 ]] || { echo "--video-duration requires seconds" >&2; exit 2; }
      video_duration="$2"
      shift 2
      ;;
    --target-max-fps)
      [[ $# -ge 2 ]] || { echo "--target-max-fps requires fps" >&2; exit 2; }
      target_max_fps="$2"
      fps_limit=1
      shift 2
      ;;
    --no-fps-limit)
      fps_limit=0
      shift
      ;;
    --sink-throttle)
      sink_throttle=1
      shift
      ;;
    --pipeline)
      [[ $# -ge 2 ]] || { echo "--pipeline requires a value" >&2; exit 2; }
      pipeline="$2"
      shift 2
      ;;
    --layer)
      [[ $# -ge 2 ]] || { echo "--layer requires a value" >&2; exit 2; }
      layer="$2"
      shift 2
      ;;
    --decoder)
      [[ $# -ge 2 ]] || { echo "--decoder requires a value" >&2; exit 2; }
      decoder="$2"
      shift 2
      ;;
    --no-runtime-json)
      runtime_json_enabled=0
      shift
      ;;
    --no-opaque-region)
      opaque_region=0
      shift
      ;;
    --no-input-passthrough)
      input_passthrough=0
      shift
      ;;
    --no-build)
      build=0
      shift
      ;;
    --keep)
      keep=1
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

require_command() {
  local command="$1"
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "missing command: $command" >&2
    exit 1
  fi
}

absolute_path() {
  local path="$1"
  local dir
  local base
  dir="$(dirname "$path")"
  base="$(basename "$path")"
  (cd "$dir" && printf '%s/%s\n' "$(pwd -P)" "$base")
}

probe_duration_seconds() {
  local path="$1"
  ffprobe -v error -select_streams v:0 \
    -show_entries stream=duration \
    -of default=noprint_wrappers=1:nokey=1 \
    "$path" | sed -n '1p'
}

source_is_long_enough() {
  local duration="$1"
  local required="$2"
  awk -v duration="$duration" -v required="$required" '
    BEGIN {
      if (duration + 0 >= required + 0) {
        exit 0
      }
      exit 1
    }
  '
}

cleanup() {
  if [[ -n "${native_pid:-}" ]] && kill -0 "$native_pid" >/dev/null 2>&1; then
    kill "$native_pid" >/dev/null 2>&1 || true
    wait "$native_pid" >/dev/null 2>&1 || true
  fi
  if [[ "$keep" -eq 0 && -z "$report_dir" && -n "${work_dir:-}" ]]; then
    rm -rf "$work_dir"
  fi
}
trap cleanup EXIT

[[ "$sample_duration" =~ ^[0-9]+$ && "$sample_duration" -gt 0 ]] || {
  echo "--sample-duration must be a positive integer" >&2
  exit 2
}
[[ "$sample_interval" =~ ^[0-9]+$ && "$sample_interval" -gt 0 ]] || {
  echo "--sample-interval must be a positive integer" >&2
  exit 2
}
[[ "$video_rate" =~ ^[0-9]+$ && "$video_rate" -gt 0 ]] || {
  echo "--video-rate must be a positive integer" >&2
  exit 2
}
[[ "$target_max_fps" =~ ^[0-9]+$ && "$target_max_fps" -gt 0 ]] || {
  echo "--target-max-fps must be a positive integer" >&2
  exit 2
}
[[ "$video_size" =~ ^[0-9]+x[0-9]+$ ]] || {
  echo "--video-size must look like WxH" >&2
  exit 2
}
case "$pipeline" in
  playbin|playbin3) ;;
  *) echo "--pipeline must be playbin or playbin3" >&2; exit 2 ;;
esac
case "$layer" in
  background|bottom|top|overlay) ;;
  *) echo "--layer must be background, bottom, top, or overlay" >&2; exit 2 ;;
esac
if [[ -z "$video_duration" ]]; then
  video_duration=$((sample_duration + warmup_seconds + 3))
fi
[[ "$video_duration" =~ ^[0-9]+$ && "$video_duration" -gt 0 ]] || {
  echo "--video-duration must be a positive integer" >&2
  exit 2
}
if [[ -z "$wayland_display" ]]; then
  echo "WAYLAND_DISPLAY is not set; pass --display <name>" >&2
  exit 1
fi

require_command ffmpeg
require_command ffprobe
require_command ps
require_command sed
require_command awk

if [[ "$build" -eq 1 ]]; then
  cargo build --release --features native-wayland-renderer,video-renderer --bin gilder-native-video
fi
native_bin="target/release/gilder-native-video"
[[ -x "$native_bin" ]] || {
  echo "missing native helper binary: $native_bin" >&2
  exit 1
}

if [[ -n "$report_dir" ]]; then
  work_dir="$report_dir"
  mkdir -p "$work_dir"
else
  mkdir -p "$work_parent"
  work_dir="$(mktemp -d "${work_parent%/}/gilder-native-wayland-video.XXXXXX")"
fi

video_path="$work_dir/loop.mp4"
video_info_path="$work_dir/video-info.txt"
native_log="$work_dir/native.log"
runtime_json="$work_dir/native-runtime.jsonl"
metadata_path="$work_dir/metadata.txt"
performance_dir="$work_dir/performance"
summary_path="$work_dir/summary.txt"

if [[ -n "$source_path" ]]; then
  [[ -f "$source_path" ]] || { echo "source does not exist: $source_path" >&2; exit 1; }
  source_abs="$(absolute_path "$source_path")"
  source_duration="$(probe_duration_seconds "$source_abs")"
  required_duration=$((sample_duration + warmup_seconds + 1))
  if [[ -n "$source_duration" ]] && source_is_long_enough "$source_duration" "$required_duration"; then
    video_path="$source_abs"
  else
    ffmpeg -hide_banner -loglevel error -y \
      -stream_loop -1 -i "$source_abs" \
      -an -c:v copy -t "$video_duration" \
      "$video_path"
  fi
else
  ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i "testsrc2=size=${video_size}:rate=${video_rate}:duration=${video_duration}" \
    -an -c:v libx264 -preset ultrafast -tune zerolatency -pix_fmt yuv420p \
    "$video_path"
fi

video_abs="$(absolute_path "$video_path")"
ffprobe -v error -select_streams v:0 \
  -show_entries stream=codec_name,width,height,r_frame_rate,avg_frame_rate,duration,nb_frames \
  -of default=noprint_wrappers=1 \
  "$video_abs" > "$video_info_path"

native_args=(
  --source "$video_abs"
  --duration "$((sample_duration + warmup_seconds + 1))"
  --decoder "$decoder"
  --pipeline "$pipeline"
  --layer "$layer"
)
if [[ "$runtime_json_enabled" -eq 1 ]]; then
  native_args+=(--runtime-json "$runtime_json")
fi
if [[ "$fps_limit" -eq 1 ]]; then
  native_args+=(--target-max-fps "$target_max_fps")
else
  native_args+=(--no-fps-limit)
fi
if [[ "$sink_throttle" -eq 1 ]]; then
  native_args+=(--sink-throttle)
fi
if [[ "$opaque_region" -eq 0 ]]; then
  native_args+=(--no-opaque-region)
fi
if [[ "$input_passthrough" -eq 0 ]]; then
  native_args+=(--no-input-passthrough)
fi

cat > "$metadata_path" <<EOF
label: native-wayland-video-smoke
wayland_display: ${wayland_display}
source: ${source_path:-generated}
video: ${video_abs}
video_size: ${video_size}
video_rate: ${video_rate}
video_duration: ${video_duration}
sample_duration: ${sample_duration}
sample_interval: ${sample_interval}
target_max_fps: $([[ "$fps_limit" -eq 1 ]] && printf '%s' "$target_max_fps" || printf none)
sink_throttle: $([[ "$sink_throttle" -eq 1 ]] && printf yes || printf no)
pipeline: ${pipeline}
layer: ${layer}
opaque_region: $([[ "$opaque_region" -eq 1 ]] && printf yes || printf no)
input_passthrough: $([[ "$input_passthrough" -eq 1 ]] && printf yes || printf no)
decoder: ${decoder}
runtime_json: ${runtime_json}
runtime_json_enabled: $([[ "$runtime_json_enabled" -eq 1 ]] && printf yes || printf no)
native_log: ${native_log}
performance_dir: ${performance_dir}
EOF

env WAYLAND_DISPLAY="$wayland_display" "$native_bin" "${native_args[@]}" > "$native_log" 2>&1 &
native_pid="$!"
sleep "$warmup_seconds"

if ! kill -0 "$native_pid" >/dev/null 2>&1; then
  echo "gilder-native-video exited before sampling; log: $native_log" >&2
  tail -80 "$native_log" >&2 || true
  exit 1
fi

scripts/performance-snapshot.sh \
  --pid "$native_pid" \
  --label native-wayland-video \
  --duration "$sample_duration" \
  --interval "$sample_interval" \
  --output-dir "$performance_dir" \
  --gilderctl "$work_dir/no-gilderctl" \
  --allow-missing \
  --keep

if kill -0 "$native_pid" >/dev/null 2>&1; then
  kill "$native_pid" >/dev/null 2>&1 || true
  wait "$native_pid" >/dev/null 2>&1 || true
fi

{
  printf 'work_dir: %s\n' "$work_dir"
  printf 'metadata: %s\n' "$metadata_path"
  printf 'video_info: %s\n' "$video_info_path"
  printf 'native_log: %s\n' "$native_log"
  if [[ "$runtime_json_enabled" -eq 1 ]]; then
    printf 'native_runtime_json: %s\n' "$runtime_json"
  else
    printf 'native_runtime_json: none\n'
  fi
  printf 'performance_summary: %s\n' "$performance_dir/summary.txt"
  printf 'performance_memory_mapping: %s\n' "$performance_dir/memory-mapping-summary.txt"
  printf 'kept: %s\n' "$([[ "$keep" -eq 1 || -n "$report_dir" ]] && printf yes || printf no)"
} > "$summary_path"

printf 'native Wayland video evidence: %s\n' "$work_dir"
printf 'summary: %s\n' "$summary_path"
if [[ "$runtime_json_enabled" -eq 1 ]]; then
  printf 'native runtime JSONL: %s\n' "$runtime_json"
else
  printf 'native runtime JSONL: none\n'
fi
printf 'performance summary: %s\n' "$performance_dir/summary.txt"
