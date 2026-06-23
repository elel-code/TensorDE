#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
usage: scripts/native-vulkan-audio-clock-probe.sh --source <media> [options]

Probe the audio side of a video source before wiring audio into the native
Vulkan playback loop. The probe records ffprobe stream/packet timing and runs a
short GStreamer audio-only playbin pipeline with fakesink, so later audio-clock
work has concrete codec, PTS, segment and clock evidence.

Options:
  --source <path>       Media source with an audio stream.
  --report-dir <dir>    Evidence directory. Default: mktemp under /tmp.
  --duration <seconds>  GStreamer probe duration. Default: 10.
  -h, --help            Show this help text.
EOF
}

source=""
report_dir=""
duration=10

while [[ $# -gt 0 ]]; do
  case "$1" in
    --source)
      source="${2:-}"
      shift 2
      ;;
    --report-dir)
      report_dir="${2:-}"
      shift 2
      ;;
    --duration)
      duration="${2:-}"
      shift 2
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

if [[ -z "$source" ]]; then
  printf 'FAIL: --source is required\n' >&2
  exit 2
fi
if [[ ! -f "$source" ]]; then
  printf 'FAIL: source does not exist: %s\n' "$source" >&2
  exit 1
fi
if [[ "$duration" -lt 1 ]]; then
  printf 'FAIL: --duration must be >= 1\n' >&2
  exit 2
fi
for tool in ffprobe gst-discoverer-1.0 gst-launch-1.0 jq timeout awk; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    printf 'FAIL: missing required tool: %s\n' "$tool" >&2
    exit 1
  fi
done

if [[ -z "$report_dir" ]]; then
  report_dir="$(mktemp -d "${TMPDIR:-/tmp}/gilder-native-vulkan-audio-clock-probe.XXXXXX")"
else
  mkdir -p "$report_dir"
fi

source_abs="$(readlink -f "$source")"
summary="$report_dir/summary.txt"
stream_json="$report_dir/audio-stream.json"
packets_csv="$report_dir/audio-packets.csv"
discoverer_log="$report_dir/gst-discoverer.log"
gst_log="$report_dir/gst-audio-clock.log"
gst_stderr="$report_dir/gst-audio-clock.stderr"

ffprobe -v error \
  -select_streams a:0 \
  -show_entries stream=index,codec_name,codec_long_name,profile,sample_rate,channels,channel_layout,start_time,duration,bit_rate \
  -of json \
  "$source_abs" >"$stream_json"

audio_stream_count="$(jq -r '.streams | length' "$stream_json")"
if [[ "$audio_stream_count" -lt 1 ]]; then
  {
    printf 'FAIL: source has no audio stream\n'
    printf 'source: %s\n' "$source_abs"
    printf 'audio_stream_json: %s\n' "$stream_json"
  } | tee "$summary"
  exit 1
fi
audio_codec="$(jq -r '.streams[0].codec_name // "none"' "$stream_json")"

ffprobe -v error \
  -select_streams a:0 \
  -read_intervals "%+${duration}" \
  -show_packets \
  -show_entries packet=pts_time,dts_time,duration_time,size,flags \
  -of csv=p=0 \
  "$source_abs" >"$packets_csv"

gst-discoverer-1.0 "$source_abs" >"$discoverer_log" 2>&1 || true

if [[ "$audio_codec" == "aac" ]]; then
  gst_pipeline_label="qtdemux-aacparse-avdec_aac-fakesink"
  gst_cmd=(
    gst-launch-1.0 -m
    filesrc "location=$source_abs"
    !
    qtdemux name=demux
    demux.audio_0
    !
    queue max-size-buffers=8 max-size-time=250000000 max-size-bytes=0
    !
    aacparse
    !
    avdec_aac
    !
    audioconvert
    !
    audioresample
    !
    fakesink sync=true async=false qos=true enable-last-sample=false
  )
else
  gst_pipeline_label="playbin-audio-fallback"
  gst_cmd=(
    gst-launch-1.0 -m playbin
    "uri=file://$source_abs"
    flags=audio
    video-sink=fakesink
    "audio-sink=fakesink sync=true async=false qos=true enable-last-sample=false"
  )
fi

gst_status=0
set +e
timeout "${duration}s" "${gst_cmd[@]}" >"$gst_log" 2>"$gst_stderr"
gst_status=$?
set -e
if [[ "$gst_status" -ne 0 && "$gst_status" -ne 124 ]]; then
  {
    printf 'FAIL: GStreamer audio clock probe failed\n'
    printf 'source: %s\n' "$source_abs"
    printf 'gst_status: %s\n' "$gst_status"
    printf 'gst_log: %s\n' "$gst_log"
    printf 'gst_stderr: %s\n' "$gst_stderr"
  } | tee "$summary"
  sed -n '1,120p' "$gst_stderr" >&2
  exit "$gst_status"
fi

packet_stats="$(
  awk -F, '
    NF >= 3 && $1 != "N/A" {
      pts = $1 + 0
      dur = ($3 == "N/A" ? 0 : $3 + 0)
      if (count == 0) {
        first_pts = pts
      } else {
        delta = pts - prev_pts
        if (delta_min == "" || delta < delta_min) delta_min = delta
        if (delta_max == "" || delta > delta_max) delta_max = delta
      }
      prev_pts = pts
      last_pts = pts
      if (dur > 0) {
        if (dur_min == "" || dur < dur_min) dur_min = dur
        if (dur_max == "" || dur > dur_max) dur_max = dur
      }
      count++
    }
    END {
      printf "packet_count=%d\n", count
      if (count > 0) {
        printf "first_pts=%0.6f\n", first_pts
        printf "last_pts=%0.6f\n", last_pts
      } else {
        printf "first_pts=none\nlast_pts=none\n"
      }
      printf "pts_delta_min=%s\n", (delta_min == "" ? "none" : delta_min)
      printf "pts_delta_max=%s\n", (delta_max == "" ? "none" : delta_max)
      printf "duration_min=%s\n", (dur_min == "" ? "none" : dur_min)
      printf "duration_max=%s\n", (dur_max == "" ? "none" : dur_max)
    }
  ' "$packets_csv"
)"

new_clock_count="$(grep -c 'new-clock' "$gst_log" || true)"
stream_start_count="$(grep -c 'stream-start' "$gst_log" || true)"
async_done_count="$(grep -c 'async-done' "$gst_log" || true)"
state_playing_count="$(grep -c 'new-state=(GstState)playing' "$gst_log" || true)"
eos_count="$(grep -c 'eos' "$gst_log" || true)"
if [[ "$new_clock_count" -lt 1 || "$stream_start_count" -lt 1 || "$state_playing_count" -lt 1 ]]; then
  {
    printf 'FAIL: GStreamer audio clock probe did not reach clocked playback\n'
    printf 'source: %s\n' "$source_abs"
    printf 'gst_status: %s\n' "$gst_status"
    printf 'gst_new_clock_count: %s\n' "$new_clock_count"
    printf 'gst_stream_start_count: %s\n' "$stream_start_count"
    printf 'gst_state_playing_count: %s\n' "$state_playing_count"
    printf 'gst_log: %s\n' "$gst_log"
    printf 'gst_stderr: %s\n' "$gst_stderr"
  } | tee "$summary"
  exit 1
fi

{
  printf 'source: %s\n' "$source_abs"
  printf 'duration_seconds: %s\n' "$duration"
  printf 'audio_stream_count: %s\n' "$audio_stream_count"
  printf 'audio_codec: %s\n' "$audio_codec"
  printf 'audio_codec_long_name: %s\n' "$(jq -r '.streams[0].codec_long_name // "none"' "$stream_json")"
  printf 'audio_profile: %s\n' "$(jq -r '.streams[0].profile // "none"' "$stream_json")"
  printf 'audio_sample_rate: %s\n' "$(jq -r '.streams[0].sample_rate // "none"' "$stream_json")"
  printf 'audio_channels: %s\n' "$(jq -r '.streams[0].channels // "none"' "$stream_json")"
  printf 'audio_channel_layout: %s\n' "$(jq -r '.streams[0].channel_layout // "none"' "$stream_json")"
  printf 'audio_start_time: %s\n' "$(jq -r '.streams[0].start_time // "none"' "$stream_json")"
  printf 'audio_duration: %s\n' "$(jq -r '.streams[0].duration // "none"' "$stream_json")"
  printf 'audio_bit_rate: %s\n' "$(jq -r '.streams[0].bit_rate // "none"' "$stream_json")"
  printf '%s\n' "$packet_stats" | sed 's/^/audio_/'
  printf 'gst_status: %s\n' "$gst_status"
  printf 'gst_pipeline: %s\n' "$gst_pipeline_label"
  printf 'gst_timed_out_after_duration: %s\n' "$([[ "$gst_status" -eq 124 ]] && printf yes || printf no)"
  printf 'gst_new_clock_count: %s\n' "$new_clock_count"
  printf 'gst_stream_start_count: %s\n' "$stream_start_count"
  printf 'gst_async_done_count: %s\n' "$async_done_count"
  printf 'gst_state_playing_count: %s\n' "$state_playing_count"
  printf 'gst_eos_count: %s\n' "$eos_count"
  printf 'audio_stream_json: %s\n' "$stream_json"
  printf 'audio_packets_csv: %s\n' "$packets_csv"
  printf 'gst_discoverer_log: %s\n' "$discoverer_log"
  printf 'gst_log: %s\n' "$gst_log"
  printf 'gst_stderr: %s\n' "$gst_stderr"
} >"$summary"

printf 'PASS: native Vulkan audio clock probe completed\n'
printf 'summary: %s\n' "$summary"
