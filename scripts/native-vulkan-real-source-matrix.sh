#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
usage: scripts/native-vulkan-real-source-matrix.sh [options]

Probe local real video sources and optionally run the native Vulkan direct video
smoke matching each supported codec. This is intended for local Wallpaper Engine
or user-owned video corpora; third-party media is not copied into the repo.

Options:
  --source <path>       Add one media file.
  --source-dir <dir>    Recursively scan a directory for media files.
  --wallpaper-engine    Scan the first existing default Wallpaper Engine Steam
                        Workshop directory for app 431960.
  --workshop-dir <dir>  Scan an explicit Wallpaper Engine workshop content dir.
  --report-dir <dir>    Matrix report directory. Default:
                        artifacts/video-real-source-matrix/<timestamp>.
  --run-video           Run matching native Vulkan direct video smokes.
                        Without this flag the script only probes/classifies.
  --display <name>      Wayland display for --run-video. Default: WAYLAND_DISPLAY.
  --output-name <name>  Target Wayland output for --run-video.
  --output <name>       Alias for --output-name.
  --duration <seconds>  Playback/probe window. Default: 10.
  --frames <count>      Override playback frame count for every source.
  --target-fps <fps>    Override probed source FPS for every source.
  --codec <all|h264|h265|av1>
                        Restrict supported direct-video runs. Default: all.
  --audio-clock-probe   Require an audio stream and enable runtime audio clock
                        probe for each video run.
  --audio-output <plan|clock-only|auto>
                        Audio output branch passed to codec smokes. Default: plan.
  --pacing-master <target|audio>
                        Video pacing master passed to codec smokes. Default:
                        audio when --audio-clock-probe is set, otherwise target.
  --muted|--unmuted     Effective plan audio policy. Default: muted.
  --align-up-coded-extent
                        For display dimensions not aligned to 16, pass the next
                        16-aligned coded extent to the codec smoke and record it.
                        Use this only to diagnose coded-extent/display-crop work.
  --max-sources <count> Limit scanned sources after sorting.
  --no-build            Do not build gilder-native-vulkan before --run-video.
  --keep-going          Continue after failed video runs. Default.
  --fail-fast           Stop after the first failed video run.
  -h, --help            Show this help text.
EOF
}

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"

sources=()
source_dirs=()
workshop_dirs=()
report_dir=""
run_video=0
display="${WAYLAND_DISPLAY:-}"
output_name="${GILDER_WAYLAND_OUTPUT:-}"
duration=10
frames_override=0
target_fps_override=0
codec_filter="all"
audio_clock_probe=0
audio_output="plan"
pacing_master=""
plan_muted=1
align_up_coded_extent=0
max_sources=0
build_binary=1
keep_going=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --source)
      sources+=("${2:?--source requires a path}")
      shift 2
      ;;
    --source-dir)
      source_dirs+=("${2:?--source-dir requires a directory}")
      shift 2
      ;;
    --wallpaper-engine)
      workshop_dirs+=("__default_wallpaper_engine__")
      shift
      ;;
    --workshop-dir)
      workshop_dirs+=("${2:?--workshop-dir requires a directory}")
      shift 2
      ;;
    --report-dir)
      report_dir="${2:?--report-dir requires a directory}"
      shift 2
      ;;
    --run-video)
      run_video=1
      shift
      ;;
    --display)
      display="${2:?--display requires a value}"
      shift 2
      ;;
    --output-name|--output)
      output_name="${2:?--output-name requires a value}"
      shift 2
      ;;
    --duration)
      duration="${2:?--duration requires seconds}"
      shift 2
      ;;
    --frames)
      frames_override="${2:?--frames requires a count}"
      shift 2
      ;;
    --target-fps)
      target_fps_override="${2:?--target-fps requires a value}"
      shift 2
      ;;
    --codec)
      codec_filter="${2:?--codec requires a value}"
      shift 2
      ;;
    --audio-clock-probe)
      audio_clock_probe=1
      shift
      ;;
    --audio-output)
      audio_output="${2:?--audio-output requires a value}"
      shift 2
      ;;
    --pacing-master)
      pacing_master="${2:?--pacing-master requires target or audio}"
      shift 2
      ;;
    --muted)
      plan_muted=1
      shift
      ;;
    --unmuted)
      plan_muted=0
      shift
      ;;
    --align-up-coded-extent)
      align_up_coded_extent=1
      shift
      ;;
    --max-sources)
      max_sources="${2:?--max-sources requires a count}"
      shift 2
      ;;
    --no-build)
      build_binary=0
      shift
      ;;
    --keep-going)
      keep_going=1
      shift
      ;;
    --fail-fast)
      keep_going=0
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

case "$codec_filter" in
  all|h264|h265|av1) ;;
  *)
    printf 'FAIL: --codec must be all, h264, h265, or av1\n' >&2
    exit 2
    ;;
esac
case "$audio_output" in
  plan|clock-only|auto) ;;
  *)
    printf 'FAIL: --audio-output must be plan, clock-only, or auto\n' >&2
    exit 2
    ;;
esac
if [[ -z "$pacing_master" ]]; then
  if [[ "$audio_clock_probe" -eq 1 ]]; then
    pacing_master="audio"
  else
    pacing_master="target"
  fi
fi
case "$pacing_master" in
  target|audio) ;;
  *)
    printf 'FAIL: --pacing-master must be target or audio\n' >&2
    exit 2
    ;;
esac
if [[ "$pacing_master" == "audio" && "$audio_clock_probe" -ne 1 ]]; then
  printf 'FAIL: --pacing-master audio requires --audio-clock-probe\n' >&2
  exit 2
fi
if [[ "$duration" -lt 1 || "$frames_override" -lt 0 || "$target_fps_override" -lt 0 || "$max_sources" -lt 0 ]]; then
  printf 'FAIL: duration/frames/target-fps/max-sources must be valid positive values\n' >&2
  exit 2
fi
if [[ "$run_video" -eq 1 && -z "$display" ]]; then
  printf 'FAIL: --run-video requires WAYLAND_DISPLAY or --display\n' >&2
  exit 2
fi
for tool in ffprobe jq awk sort readlink find date sed grep; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    printf 'FAIL: missing required tool: %s\n' "$tool" >&2
    exit 1
  fi
done

default_wallpaper_engine_dirs=(
  "$HOME/.steam/steam/steamapps/workshop/content/431960"
  "$HOME/.local/share/Steam/steamapps/workshop/content/431960"
  "$HOME/Steam/steamapps/workshop/content/431960"
)

resolved_workshop_dirs=()
for dir in "${workshop_dirs[@]}"; do
  if [[ "$dir" != "__default_wallpaper_engine__" ]]; then
    resolved_workshop_dirs+=("$dir")
    continue
  fi
  found_default=""
  for candidate in "${default_wallpaper_engine_dirs[@]}"; do
    if [[ -d "$candidate" ]]; then
      found_default="$candidate"
      break
    fi
  done
  if [[ -z "$found_default" ]]; then
    printf 'FAIL: no default Wallpaper Engine workshop directory found; pass --workshop-dir\n' >&2
    exit 1
  fi
  resolved_workshop_dirs+=("$found_default")
done
source_dirs+=("${resolved_workshop_dirs[@]}")

if [[ "${#sources[@]}" -eq 0 && "${#source_dirs[@]}" -eq 0 ]]; then
  printf 'FAIL: pass at least one --source, --source-dir, --wallpaper-engine, or --workshop-dir\n' >&2
  usage >&2
  exit 2
fi

if [[ -z "$report_dir" ]]; then
  report_dir="$repo_root/artifacts/video-real-source-matrix/$(date +%Y%m%d-%H%M%S)-$$"
fi
mkdir -p "$report_dir/probes" "$report_dir/runs"

is_media_path() {
  case "${1,,}" in
    *.mp4|*.m4v|*.mov|*.mkv|*.webm) return 0 ;;
    *) return 1 ;;
  esac
}

all_sources=()
for source in "${sources[@]}"; do
  if [[ ! -f "$source" ]]; then
    printf 'FAIL: source does not exist: %s\n' "$source" >&2
    exit 1
  fi
  all_sources+=("$(readlink -f "$source")")
done
for dir in "${source_dirs[@]}"; do
  if [[ ! -d "$dir" ]]; then
    printf 'FAIL: source directory does not exist: %s\n' "$dir" >&2
    exit 1
  fi
  while IFS= read -r -d '' candidate; do
    all_sources+=("$(readlink -f "$candidate")")
  done < <(
    find "$dir" -type f \
      \( -iname '*.mp4' -o -iname '*.m4v' -o -iname '*.mov' -o -iname '*.mkv' -o -iname '*.webm' \) \
      -print0 | sort -z
  )
done

unique_sources=()
seen_sources_file="$report_dir/.seen-sources"
: >"$seen_sources_file"
for source in "${all_sources[@]}"; do
  if ! is_media_path "$source"; then
    continue
  fi
  if grep -Fxq "$source" "$seen_sources_file"; then
    continue
  fi
  printf '%s\n' "$source" >>"$seen_sources_file"
  unique_sources+=("$source")
  if [[ "$max_sources" -gt 0 && "${#unique_sources[@]}" -ge "$max_sources" ]]; then
    break
  fi
done

summary="$report_dir/summary.tsv"
matrix_summary="$report_dir/summary.txt"
commands_file="$report_dir/commands.sh"
printf 'index\tstatus\tcodec\tprofile\twidth\theight\tsmoke_width\tsmoke_height\tpix_fmt\tbit_depth\tfps\taudio_codec\tframes\treport_dir\treason\tsource\n' >"$summary"
{
  printf '#!/usr/bin/env bash\n'
  printf 'set -euo pipefail\n'
} >"$commands_file"
chmod +x "$commands_file"

rate_to_int_fps() {
  local rate="${1:-0/0}"
  local num="${rate%/*}"
  local den="${rate#*/}"
  if [[ ! "$num" =~ ^[0-9]+$ || ! "$den" =~ ^[0-9]+$ || "$den" -eq 0 || "$num" -eq 0 ]]; then
    printf '0\n'
    return
  fi
  awk -v num="$num" -v den="$den" 'BEGIN {
    fps = num / den
    if (fps < 1) {
      print 0
    } else {
      printf "%d\n", int(fps + 0.5)
    }
  }'
}

align_up_16() {
  local value="${1:?value is required}"
  printf '%s\n' $(( (value + 15) / 16 * 16 ))
}

source_slug() {
  local source="${1:?source is required}"
  local base
  base="$(basename "$source")"
  base="${base%.*}"
  base="$(printf '%s' "$base" | sed 's/[^A-Za-z0-9._-]/_/g; s/__*/_/g; s/^_//; s/_$//')"
  if [[ -z "$base" ]]; then
    base="source"
  fi
  printf '%s\n' "$base"
}

append_summary_row() {
  local index="$1"
  local status="$2"
  local codec="$3"
  local profile="$4"
  local width="$5"
  local height="$6"
  local smoke_width="$7"
  local smoke_height="$8"
  local pix_fmt="$9"
  local bit_depth="${10}"
  local fps="${11}"
  local audio_codec="${12}"
  local frames="${13}"
  local run_report="${14}"
  local reason="${15}"
  local source="${16}"
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$index" "$status" "$codec" "$profile" "$width" "$height" "$smoke_width" "$smoke_height" \
    "$pix_fmt" "$bit_depth" "$fps" "$audio_codec" "$frames" "$run_report" "$reason" "$source" \
    >>"$summary"
}

shell_quote_command() {
  local -n command_ref="$1"
  local part
  for part in "${command_ref[@]}"; do
    printf '%q ' "$part"
  done
  printf '\n'
}

if [[ "${#unique_sources[@]}" -eq 0 ]]; then
  {
    printf 'FAIL: no media sources found\n'
    printf 'report_dir: %s\n' "$report_dir"
  } | tee "$matrix_summary"
  exit 1
fi

cd "$repo_root"
if [[ "$run_video" -eq 1 && "$build_binary" -eq 1 ]]; then
  cargo build --release --features native-vulkan-video --bin gilder-native-vulkan
fi

passed=0
failed=0
skipped=0
probed=0
index=0

for source in "${unique_sources[@]}"; do
  index=$((index + 1))
  probe_json="$report_dir/probes/source-$(printf '%04d' "$index").json"
  if ! ffprobe -v error \
    -show_entries stream=index,codec_type,codec_name,codec_long_name,profile,width,height,pix_fmt,bits_per_raw_sample,r_frame_rate,avg_frame_rate,duration,bit_rate:format=duration \
    -of json \
    "$source" >"$probe_json"; then
    append_summary_row "$index" "skip-probe-failed" "none" "none" 0 0 0 0 "none" 0 0 "none" 0 "" "ffprobe-failed" "$source"
    skipped=$((skipped + 1))
    continue
  fi

  video_count="$(jq '[.streams[]? | select(.codec_type == "video")] | length' "$probe_json")"
  if [[ "$video_count" -lt 1 ]]; then
    append_summary_row "$index" "skip-no-video" "none" "none" 0 0 0 0 "none" 0 0 "none" 0 "" "no-video-stream" "$source"
    skipped=$((skipped + 1))
    continue
  fi

  codec="$(jq -r '[.streams[]? | select(.codec_type == "video")][0].codec_name // "none"' "$probe_json")"
  profile="$(jq -r '[.streams[]? | select(.codec_type == "video")][0].profile // "none"' "$probe_json")"
  width="$(jq -r '[.streams[]? | select(.codec_type == "video")][0].width // 0' "$probe_json")"
  height="$(jq -r '[.streams[]? | select(.codec_type == "video")][0].height // 0' "$probe_json")"
  pix_fmt="$(jq -r '[.streams[]? | select(.codec_type == "video")][0].pix_fmt // "none"' "$probe_json")"
  bits_per_raw_sample="$(jq -r '[.streams[]? | select(.codec_type == "video")][0].bits_per_raw_sample // "0"' "$probe_json")"
  avg_frame_rate="$(jq -r '[.streams[]? | select(.codec_type == "video")][0].avg_frame_rate // "0/0"' "$probe_json")"
  r_frame_rate="$(jq -r '[.streams[]? | select(.codec_type == "video")][0].r_frame_rate // "0/0"' "$probe_json")"
  audio_codec="$(jq -r '[.streams[]? | select(.codec_type == "audio")][0].codec_name // "none"' "$probe_json")"

  direct_codec="unsupported"
  case "$codec" in
    h264) direct_codec="h264" ;;
    hevc|h265) direct_codec="h265" ;;
    av1) direct_codec="av1" ;;
  esac
  if [[ "$direct_codec" == "unsupported" ]]; then
    append_summary_row "$index" "skip-unsupported-codec" "$codec" "$profile" "$width" "$height" "$width" "$height" "$pix_fmt" 0 0 "$audio_codec" 0 "" "unsupported-direct-codec" "$source"
    skipped=$((skipped + 1))
    continue
  fi
  if [[ "$codec_filter" != "all" && "$codec_filter" != "$direct_codec" ]]; then
    append_summary_row "$index" "skip-filtered" "$direct_codec" "$profile" "$width" "$height" "$width" "$height" "$pix_fmt" 0 0 "$audio_codec" 0 "" "codec-filter" "$source"
    skipped=$((skipped + 1))
    continue
  fi

  bit_depth=8
  if [[ "$bits_per_raw_sample" =~ ^[0-9]+$ && "$bits_per_raw_sample" -gt 8 ]]; then
    bit_depth=10
  elif [[ "$pix_fmt" == *10* || "$pix_fmt" == *12* ]]; then
    bit_depth=10
  fi
  if [[ "$direct_codec" == "h264" && "$bit_depth" -ne 8 ]]; then
    append_summary_row "$index" "skip-unsupported-bit-depth" "$direct_codec" "$profile" "$width" "$height" "$width" "$height" "$pix_fmt" "$bit_depth" 0 "$audio_codec" 0 "" "h264-direct-smoke-supports-8bit" "$source"
    skipped=$((skipped + 1))
    continue
  fi

  fps="$target_fps_override"
  if [[ "$fps" -eq 0 ]]; then
    fps="$(rate_to_int_fps "$avg_frame_rate")"
  fi
  if [[ "$fps" -eq 0 ]]; then
    fps="$(rate_to_int_fps "$r_frame_rate")"
  fi
  if [[ "$fps" -lt 1 ]]; then
    append_summary_row "$index" "skip-invalid-fps" "$direct_codec" "$profile" "$width" "$height" "$width" "$height" "$pix_fmt" "$bit_depth" 0 "$audio_codec" 0 "" "could-not-probe-fps" "$source"
    skipped=$((skipped + 1))
    continue
  fi

  frames="$frames_override"
  if [[ "$frames" -eq 0 ]]; then
    frames=$((fps * duration))
  fi
  smoke_width="$width"
  smoke_height="$height"
  if (( smoke_width % 16 != 0 || smoke_height % 16 != 0 )); then
    if [[ "$align_up_coded_extent" -eq 1 ]]; then
      smoke_width="$(align_up_16 "$smoke_width")"
      smoke_height="$(align_up_16 "$smoke_height")"
    else
      append_summary_row "$index" "skip-unaligned-extent" "$direct_codec" "$profile" "$width" "$height" "$smoke_width" "$smoke_height" "$pix_fmt" "$bit_depth" "$fps" "$audio_codec" "$frames" "" "display-extent-not-16-aligned" "$source"
      skipped=$((skipped + 1))
      continue
    fi
  fi

  if [[ "$audio_clock_probe" -eq 1 && "$audio_codec" == "none" ]]; then
    append_summary_row "$index" "skip-no-audio" "$direct_codec" "$profile" "$width" "$height" "$smoke_width" "$smoke_height" "$pix_fmt" "$bit_depth" "$fps" "$audio_codec" "$frames" "" "audio-clock-probe-requested-without-audio" "$source"
    skipped=$((skipped + 1))
    continue
  fi

  slug="$(source_slug "$source")"
  run_report="$report_dir/runs/$(printf '%04d' "$index")-$direct_codec-$slug"
  smoke_script=""
  smoke_args=()
  case "$direct_codec" in
    h264)
      smoke_script="scripts/native-vulkan-h264-ready-prefix-video-smoke.sh"
      smoke_args=(
        "$smoke_script"
        --no-build
        --source "$source"
        --width "$smoke_width"
        --height "$smoke_height"
        --target-fps "$fps"
        --decode-prefix "$frames"
        --playback-frames "$frames"
        --report-dir "$run_report"
      )
      ;;
    h265)
      smoke_script="scripts/native-vulkan-h265-ready-prefix-video-smoke.sh"
      smoke_args=(
        "$smoke_script"
        --no-build
        --source "$source"
        --width "$smoke_width"
        --height "$smoke_height"
        --target-fps "$fps"
        --decode-prefix "$frames"
        --playback-frames "$frames"
        --bit-depth "$bit_depth"
        --report-dir "$run_report"
      )
      ;;
    av1)
      smoke_script="scripts/native-vulkan-av1-ready-prefix-video-smoke.sh"
      smoke_args=(
        "$smoke_script"
        --no-build
        --source "$source"
        --width "$smoke_width"
        --height "$smoke_height"
        --target-fps "$fps"
        --decode-prefix "$frames"
        --playback-frames "$frames"
        --bit-depth "$bit_depth"
        --report-dir "$run_report"
      )
      ;;
  esac
  if [[ -n "$display" ]]; then
    smoke_args+=(--display "$display")
  fi
  if [[ -n "$output_name" ]]; then
    smoke_args+=(--output-name "$output_name")
  fi
  if [[ "$audio_clock_probe" -eq 1 ]]; then
    smoke_args+=(--audio-clock-probe --audio-output "$audio_output" --pacing-master "$pacing_master")
  elif [[ "$pacing_master" == "target" ]]; then
    smoke_args+=(--pacing-master target)
  fi
  if [[ "$plan_muted" -eq 1 ]]; then
    smoke_args+=(--muted)
  else
    smoke_args+=(--unmuted)
  fi

  shell_quote_command smoke_args >>"$commands_file"
  if [[ "$run_video" -eq 0 ]]; then
    append_summary_row "$index" "probe-supported" "$direct_codec" "$profile" "$width" "$height" "$smoke_width" "$smoke_height" "$pix_fmt" "$bit_depth" "$fps" "$audio_codec" "$frames" "$run_report" "not-run" "$source"
    probed=$((probed + 1))
    continue
  fi

  set +e
  "${smoke_args[@]}"
  smoke_status=$?
  set -e
  if [[ "$smoke_status" -eq 0 ]]; then
    append_summary_row "$index" "pass" "$direct_codec" "$profile" "$width" "$height" "$smoke_width" "$smoke_height" "$pix_fmt" "$bit_depth" "$fps" "$audio_codec" "$frames" "$run_report" "ok" "$source"
    passed=$((passed + 1))
  else
    append_summary_row "$index" "fail" "$direct_codec" "$profile" "$width" "$height" "$smoke_width" "$smoke_height" "$pix_fmt" "$bit_depth" "$fps" "$audio_codec" "$frames" "$run_report" "smoke-exit-$smoke_status" "$source"
    failed=$((failed + 1))
    if [[ "$keep_going" -eq 0 ]]; then
      break
    fi
  fi
done

{
  printf 'report_dir: %s\n' "$report_dir"
  printf 'source_count: %s\n' "${#unique_sources[@]}"
  printf 'run_video: %s\n' "$([[ "$run_video" -eq 1 ]] && printf yes || printf no)"
  printf 'codec_filter: %s\n' "$codec_filter"
  printf 'duration_seconds: %s\n' "$duration"
  printf 'frames_override: %s\n' "$frames_override"
  printf 'target_fps_override: %s\n' "$target_fps_override"
  printf 'audio_clock_probe: %s\n' "$([[ "$audio_clock_probe" -eq 1 ]] && printf yes || printf no)"
  printf 'audio_output: %s\n' "$audio_output"
  printf 'pacing_master: %s\n' "$pacing_master"
  printf 'plan_muted: %s\n' "$([[ "$plan_muted" -eq 1 ]] && printf yes || printf no)"
  printf 'align_up_coded_extent: %s\n' "$([[ "$align_up_coded_extent" -eq 1 ]] && printf yes || printf no)"
  printf 'probe_supported_count: %s\n' "$probed"
  printf 'passed_count: %s\n' "$passed"
  printf 'failed_count: %s\n' "$failed"
  printf 'skipped_count: %s\n' "$skipped"
  printf 'summary_tsv: %s\n' "$summary"
  printf 'commands: %s\n' "$commands_file"
} >"$matrix_summary"

if [[ "$failed" -gt 0 ]]; then
  printf 'FAIL: native Vulkan real source matrix completed with %s failed run(s)\n' "$failed"
  printf 'summary: %s\n' "$matrix_summary"
  exit 1
fi

printf 'PASS: native Vulkan real source matrix completed\n'
printf 'summary: %s\n' "$matrix_summary"
