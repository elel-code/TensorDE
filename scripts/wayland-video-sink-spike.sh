#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
usage: scripts/wayland-video-sink-spike.sh [options]

Run a standalone Wayland/GStreamer video-sink spike. This bypasses GTK and
plays a 4K/240-style video through waylandsink, then samples process memory
with scripts/performance-snapshot.sh.

This is a GTK-bypass memory probe, not the final native renderer: it does not
create a Gilder-owned wlr-layer-shell surface or prove compositor presentation
quality. Use it to compare sink/runtime memory against the GTK paintable path.

Options:
  --source <path>       Existing video source. If shorter than the sampling
                        window, it is repeated into the work directory.
  --display <name>      Wayland display name. Default: WAYLAND_DISPLAY.
  --output <name>       Optional waylandsink fullscreen-output connector name.
  --work-dir <dir>      Parent directory for temporary data. Default: /tmp.
  --report-dir <dir>    Exact evidence directory. Created and kept.
  --sample-duration <s> Sampling duration. Default: 8.
  --sample-interval <s> Sampling interval. Default: 1.
  --video-size <WxH>    Generated video size when --source is omitted.
                        Default: 3840x2160.
  --video-rate <fps>    Generated video frame rate when --source is omitted.
                        Default: 240.
  --video-duration <s>  Generated/repeated playback duration. Default:
                        sample duration + warmup + 3 seconds.
  --keep                Keep temporary data when --report-dir is not used.
  -h, --help            Show this help text.
EOF
}

work_parent="${TMPDIR:-/tmp}"
report_dir=""
source_path=""
wayland_display="${WAYLAND_DISPLAY:-}"
output_name=""
sample_duration=8
sample_interval=1
video_size="3840x2160"
video_rate=240
video_duration=""
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
    --output)
      [[ $# -ge 2 ]] || { echo "--output requires a value" >&2; exit 2; }
      output_name="$2"
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
  if [[ -n "${gst_pid:-}" ]] && kill -0 "$gst_pid" >/dev/null 2>&1; then
    kill "$gst_pid" >/dev/null 2>&1 || true
    wait "$gst_pid" >/dev/null 2>&1 || true
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
[[ "$video_size" =~ ^[0-9]+x[0-9]+$ ]] || {
  echo "--video-size must look like WxH" >&2
  exit 2
}
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
require_command gst-inspect-1.0
require_command gst-launch-1.0
require_command ps
require_command sed
require_command awk

if ! gst-inspect-1.0 playbin >/dev/null 2>&1; then
  echo "GStreamer element playbin is not available" >&2
  exit 1
fi
if ! gst-inspect-1.0 waylandsink >/dev/null 2>&1; then
  echo "GStreamer element waylandsink is not available" >&2
  exit 1
fi

if [[ -n "$report_dir" ]]; then
  work_dir="$report_dir"
  mkdir -p "$work_dir"
else
  mkdir -p "$work_parent"
  work_dir="$(mktemp -d "${work_parent%/}/gilder-waylandsink-spike.XXXXXX")"
fi

video_path="$work_dir/loop.mp4"
video_info_path="$work_dir/video-info.txt"
gst_log="$work_dir/gst-launch.log"
gst_inspect_path="$work_dir/waylandsink-inspect.txt"
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
gst-inspect-1.0 waylandsink > "$gst_inspect_path"

sink_desc="waylandsink display=${wayland_display} fullscreen=true sync=true async=false enable-last-sample=false qos=true max-lateness=4166666 render-delay=0 processing-deadline=0 show-preroll-frame=false"
if [[ -n "$output_name" ]]; then
  sink_desc="${sink_desc} fullscreen-output=${output_name}"
fi

cat > "$metadata_path" <<EOF
label: standalone-waylandsink-spike
wayland_display: ${wayland_display}
output: ${output_name:-default}
source: ${source_path:-generated}
video: ${video_abs}
video_size: ${video_size}
video_rate: ${video_rate}
video_duration: ${video_duration}
sample_duration: ${sample_duration}
sample_interval: ${sample_interval}
sink: ${sink_desc}
video_info: ${video_info_path}
gst_log: ${gst_log}
performance_dir: ${performance_dir}
EOF

gst-launch-1.0 -q \
  playbin \
  uri="file://${video_abs}" \
  flags=video \
  video-sink="$sink_desc" \
  > "$gst_log" 2>&1 &
gst_pid="$!"
sleep "$warmup_seconds"

if ! kill -0 "$gst_pid" >/dev/null 2>&1; then
  echo "gst-launch exited before sampling; log: $gst_log" >&2
  tail -80 "$gst_log" >&2 || true
  exit 1
fi

scripts/performance-snapshot.sh \
  --pid "$gst_pid" \
  --label standalone-waylandsink-spike \
  --duration "$sample_duration" \
  --interval "$sample_interval" \
  --output-dir "$performance_dir" \
  --gilderctl "$work_dir/no-gilderctl" \
  --allow-missing \
  --keep

if kill -0 "$gst_pid" >/dev/null 2>&1; then
  kill "$gst_pid" >/dev/null 2>&1 || true
  wait "$gst_pid" >/dev/null 2>&1 || true
fi

{
  printf 'work_dir: %s\n' "$work_dir"
  printf 'metadata: %s\n' "$metadata_path"
  printf 'video_info: %s\n' "$video_info_path"
  printf 'gst_log: %s\n' "$gst_log"
  printf 'performance_summary: %s\n' "$performance_dir/summary.txt"
  printf 'performance_memory_mapping: %s\n' "$performance_dir/memory-mapping-summary.txt"
  printf 'kept: %s\n' "$([[ "$keep" -eq 1 || -n "$report_dir" ]] && printf yes || printf no)"
} > "$summary_path"

printf 'standalone waylandsink spike evidence: %s\n' "$work_dir"
printf 'summary: %s\n' "$summary_path"
printf 'performance summary: %s\n' "$performance_dir/summary.txt"
