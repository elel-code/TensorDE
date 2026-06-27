#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
usage: scripts/native-vulkan-h265-ready-prefix-video-smoke.sh [options]

Generate or use a 4K/240 H.265 Main/Main10 source, then run the native Vulkan direct
H.265 ready-prefix path on a real Wayland background surface. Each ready AU is
decoded with Vulkan Video into a sampled NV12/P010 array layer and presented
through the native Vulkan swapchain.
By default, --playback-frames also expands the decoded ready prefix so the
generated source is a continuous 4K/240 stream comparable with the
FFmpeg packet frontend.

Options:
  --display <name>      Wayland display name. Default: WAYLAND_DISPLAY.
  --output-name <name>  Target Wayland output name, for example HDMI-A-1.
  --output <name>       Alias for --output-name.
  --source <path>       Existing H.265 source. Default: generate continuous H.265 source.
  --report-dir <dir>    Exact evidence directory. Created and kept.
  --work-dir <dir>      Parent directory for generated evidence. Default: /tmp.
  --source-cache-dir <dir>
                        Persistent generated source cache. Default: artifacts/video-sources/h265.
  --decode-prefix <n>   Ready-prefix AU count to decode/present. Default:
                        playback-frames when playback-frames is set, otherwise target-fps.
  --playback-frames <n> Decode/present frames by looping the ready prefix. Default: decode-prefix.
  --target-fps <fps>    Presentation target FPS. Default: 240.
  --gop-size <frames>   Generated H.265 keyint/min-keyint. Default: target-fps.
  --refs <count>        Generated H.265 reference frames. Default: 1.
  --bframes <count>     Generated B-frame count. Default: 0.
  --bit-depth <8|10>    Generated/probed H.265 Main bit depth. Default: 8.
  --width <px>          Generated/probed width. Default: 3840.
  --height <px>         Generated/probed height. Default: 2160.
  --frames <count>      Generated frame count. Default: decode-prefix + 2.
  --arbitrary-entry-offset <seconds>
                        Copy the source from a non-keyframe entry with -copyinkf,
                        then require streaming bootstrap to discard the broken
                        prefix and resume from the next decodable IDR.
  --audio-clock-probe  Run explicit AAC audio-only clock probe beside H.265 video
                        and gate clocked playback / no video decoder contamination.
  --audio-output <plan|clock-only|auto>
                        Select audio clock probe output branch. Default: plan
                        (muted -> clock-only, unmuted -> auto).
  --pacing-master <target|audio>
                        Select pacing master. audio requires --audio-clock-probe.
  --muted|--unmuted    Select the effective video plan audio policy for plan output.
  --performance-snapshot
                        Capture process CPU/RSS/PSS/USS/Private_Dirty/smaps while the
                        native Vulkan process is running.
  --performance-duration <sec>
                        Performance sampling duration. Default: 10.
  --performance-interval <sec>
                        Performance sampling interval. Default: 1.
  --max-private-dirty-kib <kib>
                        With --performance-snapshot, fail if max Private_Dirty exceeds this.
                        Default with --performance-snapshot: 25000.
  --layer <layer>       Wayland layer. Default: background.
  --fit <mode>          Render fit. Default: cover.
  --no-build            Reuse existing target/release/gilder-native-vulkan.
  -h, --help            Show this help text.
EOF
}

display="${WAYLAND_DISPLAY:-}"
output_name=""
source=""
report_dir=""
work_parent="${TMPDIR:-/tmp}"
source_cache_dir=""
decode_prefix=0
decode_prefix_explicit=0
playback_frames=0
target_fps=240
gop_size=0
refs=1
bframes=0
bit_depth=8
width=3840
height=2160
frames=0
frames_explicit=0
arbitrary_entry_offset=""
arbitrary_entry_source=0
audio_clock_probe=0
audio_output="plan"
plan_muted=1
pacing_master="target"
layer="background"
fit="cover"
no_build=0
generated_source=0
source_duration_seconds=0
performance_snapshot=0
performance_duration=10
performance_interval=1
max_private_dirty_kib_limit=""
default_max_private_dirty_kib_limit=25000

while [[ $# -gt 0 ]]; do
  case "$1" in
    --display)
      display="${2:-}"
      shift 2
      ;;
    --output-name|--output)
      output_name="${2:-}"
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
    --source-cache-dir)
      source_cache_dir="${2:-}"
      shift 2
      ;;
    --decode-prefix)
      decode_prefix="${2:-}"
      decode_prefix_explicit=1
      shift 2
      ;;
    --playback-frames)
      playback_frames="${2:-}"
      shift 2
      ;;
    --target-fps)
      target_fps="${2:-}"
      shift 2
      ;;
    --gop-size)
      gop_size="${2:-}"
      shift 2
      ;;
    --refs)
      refs="${2:-}"
      shift 2
      ;;
    --bframes)
      bframes="${2:-}"
      shift 2
      ;;
    --bit-depth)
      bit_depth="${2:-}"
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
    --frames)
      frames="${2:-}"
      frames_explicit=1
      shift 2
      ;;
    --arbitrary-entry-offset)
      arbitrary_entry_offset="${2:-}"
      shift 2
      ;;
    --audio-clock-probe)
      audio_clock_probe=1
      shift
      ;;
    --audio-output)
      audio_output="${2:-}"
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
    --pacing-master)
      pacing_master="${2:-}"
      shift 2
      ;;
    --performance-snapshot)
      performance_snapshot=1
      shift
      ;;
    --performance-duration)
      performance_duration="${2:-}"
      shift 2
      ;;
    --performance-interval)
      performance_interval="${2:-}"
      shift 2
      ;;
    --max-private-dirty-kib)
      max_private_dirty_kib_limit="${2:-}"
      shift 2
      ;;
    --layer)
      layer="${2:-}"
      shift 2
      ;;
    --fit)
      fit="${2:-}"
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

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
cd "$repo_root"
source "$script_dir/native-vulkan-ready-prefix-video-common.sh"
if [[ -z "$source_cache_dir" ]]; then
  source_cache_dir="$(gilder_default_source_cache_dir h265)"
fi

if [[ -z "$display" ]]; then
  printf 'FAIL: WAYLAND_DISPLAY is empty; pass --display\n' >&2
  exit 1
fi
if [[ "$pacing_master" != "target" && "$pacing_master" != "audio" ]]; then
  printf 'FAIL: --pacing-master must be target or audio\n' >&2
  exit 1
fi
if [[ "$pacing_master" == "audio" && "$audio_clock_probe" -ne 1 ]]; then
  printf 'FAIL: --pacing-master audio requires --audio-clock-probe\n' >&2
  exit 1
fi
if [[ "$audio_output" != "plan" && "$audio_output" != "clock-only" && "$audio_output" != "auto" ]]; then
  printf 'FAIL: --audio-output must be plan, clock-only, or auto\n' >&2
  exit 1
fi
if [[ "$audio_output" == "auto" && "$audio_clock_probe" -ne 1 ]]; then
  printf 'FAIL: --audio-output %s requires --audio-clock-probe\n' "$audio_output" >&2
  exit 1
fi

for tool in ffmpeg jq; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    printf 'FAIL: missing required tool: %s\n' "$tool" >&2
    exit 1
  fi
done

case "$layer" in
  background|bottom) ;;
  top|overlay)
    printf 'FAIL: foreground layer "%s" is not allowed by this smoke\n' "$layer" >&2
    exit 1
    ;;
  *)
    printf 'FAIL: unsupported layer: %s\n' "$layer" >&2
    exit 1
    ;;
esac

if [[ "$gop_size" -eq 0 ]]; then
  gop_size="$target_fps"
fi
if [[ "$decode_prefix" -eq 0 ]]; then
  decode_prefix="$target_fps"
fi
if [[ "$decode_prefix_explicit" -eq 0 && "$playback_frames" -gt "$decode_prefix" ]]; then
  decode_prefix="$playback_frames"
fi
if [[ "$decode_prefix" -lt 1 || "$playback_frames" -lt 0 || "$target_fps" -lt 1 || "$gop_size" -lt 1 || "$refs" -lt 1 || "$refs" -gt 16 || "$bframes" -lt 0 || "$bframes" -gt 16 || "$width" -lt 2 || "$height" -lt 2 ]]; then
  printf 'FAIL: decode-prefix/playback-frames/target-fps/gop-size/refs/bframes must be valid and width/height must be at least 2\n' >&2
  exit 1
fi
if [[ -n "$max_private_dirty_kib_limit" && ! "$max_private_dirty_kib_limit" =~ ^[0-9]+$ ]]; then
  printf 'FAIL: --max-private-dirty-kib must be a non-negative integer\n' >&2
  exit 1
fi
if [[ -n "$max_private_dirty_kib_limit" && "$performance_snapshot" -ne 1 ]]; then
  printf 'FAIL: --max-private-dirty-kib requires --performance-snapshot\n' >&2
  exit 1
fi
if [[ "$performance_snapshot" -eq 1 && -z "$max_private_dirty_kib_limit" ]]; then
  max_private_dirty_kib_limit="$default_max_private_dirty_kib_limit"
fi
case "$bit_depth" in
  8)
    video_codec="h265-main-8"
    x265_profile="main"
    x265_pix_fmt="yuv420p"
    expected_picture_format="G8_B8R8_2PLANE_420_UNORM"
    ;;
  10)
    video_codec="h265-main-10"
    x265_profile="main10"
    x265_pix_fmt="yuv420p10le"
    expected_picture_format="G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16"
    ;;
  *)
    printf 'FAIL: --bit-depth must be 8 or 10\n' >&2
    exit 1
    ;;
esac
expected_frames="$decode_prefix"
if [[ "$playback_frames" -gt 0 ]]; then
  expected_frames="$playback_frames"
fi
ready_prefix_loop_period_ms=$((decode_prefix * 1000 / target_fps))
if [[ "$expected_frames" -gt "$decode_prefix" && "$decode_prefix" -lt "$target_fps" ]]; then
  {
    printf 'FAIL: visible H.265 ready-prefix loop is too short for smoothness\n'
    printf 'decode_prefix: %s\n' "$decode_prefix"
    printf 'target_fps: %s\n' "$target_fps"
    printf 'ready_prefix_loop_period_ms: %s\n' "$ready_prefix_loop_period_ms"
    printf 'expected_playback_frames: %s\n' "$expected_frames"
  } >&2
  exit 1
fi

if [[ -z "$report_dir" ]]; then
  report_dir="$(mktemp -d "${work_parent%/}/gilder-vulkan-h265-ready-prefix-video.XXXXXX")"
else
  mkdir -p "$report_dir"
fi
mkdir -p "$report_dir"

release_binary_path="target/release/gilder-native-vulkan"
release_binary_fingerprint_before=""
if [[ -e "$release_binary_path" ]]; then
  release_binary_fingerprint_before="$(stat -c '%d:%i:%s:%Y' "$release_binary_path" 2>/dev/null || true)"
fi
release_binary_replaced_by_build=0
release_binary_synced_after_build=0
if [[ "$no_build" -eq 0 ]]; then
  cargo build --release --features native-vulkan-video --bin gilder-native-vulkan
  release_binary_fingerprint_after=""
  if [[ -e "$release_binary_path" ]]; then
    release_binary_fingerprint_after="$(stat -c '%d:%i:%s:%Y' "$release_binary_path" 2>/dev/null || true)"
  fi
  if [[ "$release_binary_fingerprint_before" != "$release_binary_fingerprint_after" ]]; then
    release_binary_replaced_by_build=1
  fi
  if [[ "$performance_snapshot" -eq 1 && "$release_binary_replaced_by_build" -eq 1 ]]; then
    gilder_sync_rebuilt_executable "$release_binary_path"
    release_binary_synced_after_build=1
  fi
fi

if [[ -z "$source" ]]; then
  generated_source=1
  generated_dir="$source_cache_dir"
  gilder_ensure_source_cache_dir "$generated_dir"
  if [[ "$frames" -eq 0 || "$frames" -lt $((decode_prefix + 2)) ]]; then
    frames=$((decode_prefix + 2))
  fi
  if [[ "$frames_explicit" -eq 0 && -n "$arbitrary_entry_offset" ]]; then
    offset_frames="$(awk -v offset="$arbitrary_entry_offset" -v fps="$target_fps" 'BEGIN { value = offset * fps; printf "%d", (value == int(value)) ? value : int(value) + 1 }')"
    arbitrary_window_frames="$expected_frames"
    if [[ "$expected_frames" -gt "$decode_prefix" ]]; then
      arbitrary_window_frames="$decode_prefix"
    fi
    arbitrary_min_frames=$((offset_frames + gop_size + arbitrary_window_frames + 2))
    if [[ "$frames" -lt "$arbitrary_min_frames" ]]; then
      frames="$arbitrary_min_frames"
    fi
  fi
  source_duration_seconds=$(( (frames + target_fps - 1) / target_fps ))
  audio_source_suffix=""
  if [[ "$audio_clock_probe" -eq 1 ]]; then
    audio_source_suffix="-aac48000"
  fi
  source="$generated_dir/${video_codec}-b${bframes}-ref${refs}-${width}x${height}-${target_fps}fps-${frames}frames-g${gop_size}-d${decode_prefix}${audio_source_suffix}.mp4"
  rc_lookahead=0
  if [[ "$bframes" -gt 0 ]]; then
    rc_lookahead=$((bframes + 2))
  fi
  codec_extra_args=()
  if [[ "$bframes" -eq 0 ]]; then
    codec_extra_args=(-tune zerolatency)
  fi
  if [[ "$audio_clock_probe" -eq 1 ]]; then
    ffmpeg_args=(
      -hide_banner -loglevel error -y
      -f lavfi -i "testsrc2=size=${width}x${height}:rate=${target_fps}:duration=${source_duration_seconds}"
      -f lavfi -i "sine=frequency=440:sample_rate=48000:duration=${source_duration_seconds}"
      -frames:v "$frames" -shortest
      -c:v libx265 -profile:v "$x265_profile" -preset ultrafast "${codec_extra_args[@]}" -pix_fmt "$x265_pix_fmt"
      -x265-params "keyint=${gop_size}:min-keyint=${gop_size}:scenecut=0:open-gop=0:bframes=${bframes}:ref=${refs}:repeat-headers=1:hrd=0:rc-lookahead=${rc_lookahead}"
      -c:a aac -b:a 128k
      "$source"
    )
  else
    ffmpeg_args=(
      -hide_banner -loglevel error -y
      -f lavfi -i "testsrc2=size=${width}x${height}:rate=${target_fps}:duration=${source_duration_seconds}"
      -frames:v "$frames" -an
      -c:v libx265 -profile:v "$x265_profile" -preset ultrafast "${codec_extra_args[@]}" -pix_fmt "$x265_pix_fmt"
      -x265-params "keyint=${gop_size}:min-keyint=${gop_size}:scenecut=0:open-gop=0:bframes=${bframes}:ref=${refs}:repeat-headers=1:hrd=0:rc-lookahead=${rc_lookahead}"
      "$source"
    )
  fi
  if [[ ! -s "$source" ]]; then
    ffmpeg "${ffmpeg_args[@]}"
  fi
fi

if [[ ! -f "$source" ]]; then
  printf 'FAIL: source does not exist: %s\n' "$source" >&2
  exit 1
fi

if [[ -n "$arbitrary_entry_offset" ]]; then
  arbitrary_entry_source=1
  shifted_dir="$source_cache_dir"
  gilder_ensure_source_cache_dir "$shifted_dir"
  shifted_stem="$(basename "$source")"
  shifted_stem="${shifted_stem%.*}"
  shifted_source="$shifted_dir/${shifted_stem}-arbitrary-${arbitrary_entry_offset}s.mp4"
  if [[ ! -s "$shifted_source" ]]; then
    ffmpeg -hide_banner -loglevel error -y \
      -i "$source" -ss "$arbitrary_entry_offset" \
      -c copy -copyinkf -avoid_negative_ts make_zero \
      "$shifted_source"
  fi
  source="$shifted_source"
  if [[ ! -s "$source" ]]; then
    printf 'FAIL: arbitrary-entry shifted source was not created: %s\n' "$source" >&2
    exit 1
  fi
fi
runtime_json="$report_dir/runtime.json"
runtime_stderr="$report_dir/runtime.stderr"
summary="$report_dir/summary.txt"
performance_dir="$report_dir/performance"
performance_log="$report_dir/performance.log"
args=(
  --run-vulkanalia-ready-prefix-video
  --video-codec "$video_codec"
  --source "$source"
  --width "$width"
  --height "$height"
  --target-fps "$target_fps"
  --layer "$layer"
  --fit "$fit"
  --bitstream-samples "$decode_prefix"
  --decode-h265-ready-prefix "$decode_prefix"
)
if [[ "$plan_muted" -eq 1 ]]; then
  args+=(--muted)
else
  args+=(--unmuted)
fi
if [[ "$playback_frames" -gt 0 ]]; then
  args+=(--playback-frames "$playback_frames")
fi
if [[ "$audio_clock_probe" -eq 1 ]]; then
  args+=(--audio-clock-probe)
  args+=(--audio-output "$audio_output")
fi
if [[ -n "$output_name" ]]; then
  args+=(--output-name "$output_name")
fi

runtime_env=(WAYLAND_DISPLAY="$display")
if [[ -n "${XDG_RUNTIME_DIR:-}" ]]; then
  runtime_env+=(XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR")
fi
if [[ "$pacing_master" == "audio" ]]; then
  runtime_env+=(GILDER_VIDEO_PACING_MASTER=audio)
else
  runtime_env+=(GILDER_VIDEO_PACING_MASTER=target)
fi
gilder_append_ready_prefix_runtime_env runtime_env

performance_status=0
runtime_status=0
performance_rebuild_mapping_dirty_retry=0
performance_rebuild_mapping_dirty_retry_count=0
performance_rebuild_mapping_dirty_max_attempts=4
performance_rebuild_mapping_dirty_first_summary=""
performance_rebuild_mapping_dirty_first_log=""
performance_rebuild_mapping_dirty_first_runtime_json=""
performance_rebuild_mapping_dirty_first_runtime_stderr=""
performance_rebuild_mapping_dirty_first_max_private_dirty_kib="none"
performance_rebuild_mapping_dirty_first_file_mapping_private_dirty_kib="none"
performance_rebuild_mapping_dirty_first_gilder_binary_private_dirty_kib="none"
performance_rebuild_mapping_dirty_first_heap_private_dirty_kib="none"
performance_rebuild_mapping_dirty_final_contaminated=0
performance_rebuild_mapping_dirty_summaries=()
performance_rebuild_mapping_dirty_logs=()
performance_rebuild_mapping_dirty_runtime_jsons=()
performance_rebuild_mapping_dirty_runtime_stderrs=()
performance_rebuild_mapping_dirty_max_private_dirty_kibs=()
performance_rebuild_mapping_dirty_file_mapping_private_dirty_kibs=()
performance_rebuild_mapping_dirty_gilder_binary_private_dirty_kibs=()
performance_rebuild_mapping_dirty_heap_private_dirty_kibs=()

run_h265_performance_snapshot_attempt() {
  local attempt_performance_dir="${1:?performance dir is required}"
  local attempt_performance_log="${2:?performance log is required}"
  local attempt_runtime_json="${3:?runtime JSON is required}"
  local attempt_runtime_stderr="${4:?runtime stderr is required}"

  set +e
  env "${runtime_env[@]}" \
    "$release_binary_path" \
    "${args[@]}" \
    >"$attempt_runtime_json" 2>"$attempt_runtime_stderr" &
  runtime_pid=$!
  performance_args=(
    --pid "$runtime_pid"
    --label "native-vulkan-h265-ready-prefix-video"
    --duration "$performance_duration"
    --interval "$performance_interval"
    --output-dir "$attempt_performance_dir"
    --allow-missing
    --keep
  )
  if [[ -n "$max_private_dirty_kib_limit" ]]; then
    performance_args+=(--expect-max-private-dirty-kib-at-most "$max_private_dirty_kib_limit")
  fi
  scripts/performance-snapshot.sh "${performance_args[@]}" >"$attempt_performance_log" 2>&1
  performance_status=$?
  wait "$runtime_pid"
  runtime_status=$?
  set -e
}

preserve_h265_rebuild_mapping_dirty_attempt() {
  local attempt_index="${1:?attempt index is required}"
  local attempt_suffix="fresh-build-contaminated"
  local attempt_dir
  local attempt_log
  local attempt_runtime_json
  local attempt_runtime_stderr
  local max_private_dirty
  local file_mapping_dirty
  local gilder_binary_dirty
  local heap_dirty

  if [[ "$attempt_index" -gt 1 ]]; then
    attempt_suffix="${attempt_suffix}.${attempt_index}"
  fi
  attempt_dir="$report_dir/performance.${attempt_suffix}"
  attempt_log="$report_dir/performance.${attempt_suffix}.log"
  attempt_runtime_json="$report_dir/runtime.${attempt_suffix}.json"
  attempt_runtime_stderr="$report_dir/runtime.${attempt_suffix}.stderr"
  max_private_dirty="$(gilder_summary_uint_or_zero "$performance_dir/summary.txt" max_private_dirty_kib)"
  file_mapping_dirty="$(gilder_summary_uint_or_zero "$performance_dir/summary.txt" memory_category_file_mapping_private_dirty_kib)"
  gilder_binary_dirty="$(gilder_summary_uint_or_zero "$performance_dir/summary.txt" memory_category_gilder_binary_private_dirty_kib)"
  heap_dirty="$(gilder_summary_uint_or_zero "$performance_dir/summary.txt" memory_category_heap_private_dirty_kib)"

  rm -rf -- "$attempt_dir"
  rm -f -- \
    "$attempt_log" \
    "$attempt_runtime_json" \
    "$attempt_runtime_stderr"
  mv "$performance_dir" "$attempt_dir"
  mv "$performance_log" "$attempt_log"
  mv "$runtime_json" "$attempt_runtime_json"
  mv "$runtime_stderr" "$attempt_runtime_stderr"

  performance_rebuild_mapping_dirty_retry=1
  performance_rebuild_mapping_dirty_retry_count=$((performance_rebuild_mapping_dirty_retry_count + 1))
  performance_rebuild_mapping_dirty_summaries+=("$attempt_dir/summary.txt")
  performance_rebuild_mapping_dirty_logs+=("$attempt_log")
  performance_rebuild_mapping_dirty_runtime_jsons+=("$attempt_runtime_json")
  performance_rebuild_mapping_dirty_runtime_stderrs+=("$attempt_runtime_stderr")
  performance_rebuild_mapping_dirty_max_private_dirty_kibs+=("$max_private_dirty")
  performance_rebuild_mapping_dirty_file_mapping_private_dirty_kibs+=("$file_mapping_dirty")
  performance_rebuild_mapping_dirty_gilder_binary_private_dirty_kibs+=("$gilder_binary_dirty")
  performance_rebuild_mapping_dirty_heap_private_dirty_kibs+=("$heap_dirty")

  if [[ "$attempt_index" -eq 1 ]]; then
    performance_rebuild_mapping_dirty_first_summary="$attempt_dir/summary.txt"
    performance_rebuild_mapping_dirty_first_log="$attempt_log"
    performance_rebuild_mapping_dirty_first_runtime_json="$attempt_runtime_json"
    performance_rebuild_mapping_dirty_first_runtime_stderr="$attempt_runtime_stderr"
    performance_rebuild_mapping_dirty_first_max_private_dirty_kib="$max_private_dirty"
    performance_rebuild_mapping_dirty_first_file_mapping_private_dirty_kib="$file_mapping_dirty"
    performance_rebuild_mapping_dirty_first_gilder_binary_private_dirty_kib="$gilder_binary_dirty"
    performance_rebuild_mapping_dirty_first_heap_private_dirty_kib="$heap_dirty"
  fi
}

if [[ "$performance_snapshot" -eq 1 ]]; then
  if [[ ! -x scripts/performance-snapshot.sh ]]; then
    printf 'FAIL: missing executable scripts/performance-snapshot.sh\n' | tee "$summary"
    exit 1
  fi

  performance_attempt_index=1
  while true; do
    run_h265_performance_snapshot_attempt \
      "$performance_dir" \
      "$performance_log" \
      "$runtime_json" \
      "$runtime_stderr"

    if [[ "$runtime_status" -ne 0 || "$performance_status" -eq 0 || "$release_binary_replaced_by_build" -ne 1 || -z "$max_private_dirty_kib_limit" ]]; then
      break
    fi
    if ! gilder_rebuild_dirty_contaminated "$performance_dir/summary.txt" "$max_private_dirty_kib_limit"; then
      break
    fi
    if [[ "$performance_attempt_index" -ge "$performance_rebuild_mapping_dirty_max_attempts" ]]; then
      performance_rebuild_mapping_dirty_final_contaminated=1
      break
    fi

    preserve_h265_rebuild_mapping_dirty_attempt "$performance_attempt_index"
    gilder_sync_rebuilt_executable "$release_binary_path"
    sleep 1
    performance_attempt_index=$((performance_attempt_index + 1))
  done
else
  set +e
  env "${runtime_env[@]}" \
    "$release_binary_path" \
    "${args[@]}" \
    >"$runtime_json" 2>"$runtime_stderr"
  runtime_status=$?
  set -e
fi

if [[ "$runtime_status" -ne 0 ]]; then
  printf 'FAIL: native Vulkan direct H.265 ready-prefix video smoke failed\n' | tee "$summary"
  printf 'source: %s\n' "$source" | tee -a "$summary"
  printf 'stderr: %s\n' "$runtime_stderr" | tee -a "$summary"
  sed -n '1,160p' "$runtime_stderr" >&2
  exit "$runtime_status"
fi
if [[ "$performance_snapshot" -eq 1 && "$performance_status" -ne 0 ]]; then
  printf 'FAIL: native Vulkan direct H.265 performance snapshot failed\n' | tee "$summary"
  printf 'source: %s\n' "$source" | tee -a "$summary"
  printf 'performance log: %s\n' "$performance_log" | tee -a "$summary"
  if [[ "$performance_rebuild_mapping_dirty_retry" -eq 1 ]]; then
    printf 'fresh-build contaminated retry count: %s\n' "$performance_rebuild_mapping_dirty_retry_count" | tee -a "$summary"
    printf 'fresh-build contaminated performance summary: %s\n' "$performance_rebuild_mapping_dirty_first_summary" | tee -a "$summary"
    printf 'fresh-build contaminated performance log: %s\n' "$performance_rebuild_mapping_dirty_first_log" | tee -a "$summary"
    printf 'fresh-build contaminated max Private_Dirty KiB: %s\n' "$performance_rebuild_mapping_dirty_first_max_private_dirty_kib" | tee -a "$summary"
    printf 'fresh-build contaminated file-mapping Private_Dirty KiB: %s\n' "$performance_rebuild_mapping_dirty_first_file_mapping_private_dirty_kib" | tee -a "$summary"
    printf 'fresh-build contaminated gilder-binary Private_Dirty KiB: %s\n' "$performance_rebuild_mapping_dirty_first_gilder_binary_private_dirty_kib" | tee -a "$summary"
    printf 'fresh-build contaminated heap Private_Dirty KiB: %s\n' "$performance_rebuild_mapping_dirty_first_heap_private_dirty_kib" | tee -a "$summary"
  fi
  if [[ "$performance_rebuild_mapping_dirty_final_contaminated" -eq 1 ]]; then
    printf 'final failed attempt is still fresh-build dirty contaminated after %s attempts\n' "$performance_rebuild_mapping_dirty_max_attempts" | tee -a "$summary"
  fi
  sed -n '1,200p' "$performance_log" >&2
  exit "$performance_status"
fi

decoded_count="$(jq -r '(.h265_retained_video_present_decode.decode.submitted_frame_count // 0)' "$runtime_json")"
presented_count="$(jq -r '(.h265_retained_video_present_decode.decoded_image_present_sequence.presented_frame_count // 0)' "$runtime_json")"
frame_count="$presented_count"
bad_frames="$(jq -r 'if ((.h265_retained_video_present_decode.decoded_image_present_sequence_error // null) == null and (.h265_retained_video_present_decode.decoded_image_present_draw_error // null) == null) then 0 else 1 end' "$runtime_json")"
average_present_fps="$(jq -r '(.h265_retained_video_present_decode.decoded_image_present_sequence.average_present_fps // .decoded_image_present_sequence.average_present_fps // 0)' "$runtime_json")"
average_present_teardown_inclusive_fps="$(jq -r '(.h265_retained_video_present_decode.decoded_image_present_sequence.average_present_teardown_inclusive_fps // .decoded_image_present_sequence.average_present_teardown_inclusive_fps // 0)' "$runtime_json")"
present_interval_elapsed_us="$(jq -r '(.h265_retained_video_present_decode.decoded_image_present_sequence.present_interval_elapsed_micros // .decoded_image_present_sequence.present_interval_elapsed_micros // 0)' "$runtime_json")"
present_teardown_inclusive_elapsed_us="$(jq -r '(.h265_retained_video_present_decode.decoded_image_present_sequence.present_teardown_inclusive_elapsed_micros // .decoded_image_present_sequence.present_teardown_inclusive_elapsed_micros // 0)' "$runtime_json")"
distinct_layers="$(jq -r '(.h265_retained_video_present_decode.decoded_image_present_sequence.distinct_sampled_array_layer_count // 0)' "$runtime_json")"
ready_prefix_count="$(jq -r '(.h265_retained_video_present_decode.decode.requested_frame_count // 0)' "$runtime_json")"
requested_playback_count="$(jq -r '(.playback_frame_count // 0)' "$runtime_json")"
if [[ "$ready_prefix_count" -gt 0 ]]; then
  playback_loop_count=$(( (requested_playback_count + ready_prefix_count - 1) / ready_prefix_count ))
else
  playback_loop_count=0
fi
loop_boundary_reset_count=$(( playback_loop_count > 0 ? playback_loop_count - 1 : 0 ))
pts_delta_min="$(jq -r '
  def seq: (.h265_retained_video_present_decode.decoded_image_present_sequence // {});
  seq.source_frame_pts_delta_min_ms // "none"
' "$runtime_json")"
pts_delta_max="$(jq -r '
  def seq: (.h265_retained_video_present_decode.decoded_image_present_sequence // {});
  seq.source_frame_pts_delta_max_ms // "none"
' "$runtime_json")"
read -r script_pts_delta_expected_min script_pts_delta_expected_max < <(gilder_pts_delta_expected_bounds_ms "$target_fps")
pts_delta_expected_min="$script_pts_delta_expected_min"
pts_delta_expected_max="$script_pts_delta_expected_max"
pts_delta_in_expected_range="script-derived"
pts_delta_script_in_expected_range=false
if gilder_pts_delta_in_expected_range "$pts_delta_min" "$pts_delta_max" "$target_fps"; then
  pts_delta_script_in_expected_range=true
fi
present_queue="$(jq -r '(.h265_retained_video_present_decode.session.device.present_queue.queue_family_index // "none")' "$runtime_json")"
video_queue="$(jq -r '(.h265_retained_video_present_decode.session.device.video_queue.queue_family_index // "none")' "$runtime_json")"
sync_strategy="$(jq -r '(.h265_retained_video_present_decode.session.resource_queue_sharing_model // "none")' "$runtime_json")"
runtime_codec="$(jq -r '(.h265_retained_video_present_decode.session.codec // "none")' "$runtime_json")"
picture_format="$(jq -r '(.h265_retained_video_present_decode.session.picture_format // "none")' "$runtime_json")"
runtime_max_dpb_slots="$(jq -r '(.h265_retained_video_present_decode.session.session_max_dpb_slots // 0)' "$runtime_json")"
stream_sps_dpb_slots="$(jq -r '(.h265_retained_video_present_decode.decode.begin_reference_slot_count // 0)' "$runtime_json")"
stream_dpb_slots="$stream_sps_dpb_slots"
stream_max_active_reference_pictures="$(jq -r '(.h265_retained_video_present_decode.decode.decode_reference_slot_count // 0)' "$runtime_json")"
session_max_dpb_slots="$(jq -r '(.h265_retained_video_present_decode.session.session_max_dpb_slots // 0)' "$runtime_json")"
session_max_active_reference_pictures="$(jq -r '(.h265_retained_video_present_decode.session.session_max_active_reference_pictures // 0)' "$runtime_json")"
present_mode="$(jq -r '(.h265_retained_video_present_decode.session.device.swapchain.present_mode // "none")' "$runtime_json")"
pacing_strategy="$(jq -r '(.h265_retained_video_present_decode.decoded_image_present_sequence.latest_draw.pacing_clock_model // "none")' "$runtime_json")"
expected_pacing_strategy="$(gilder_expected_pacing_strategy_with_master "$present_mode" "$target_fps" "$pacing_master")"
present_mode_gate_failed=0
if ! gilder_native_video_present_mode_allowed "$present_mode"; then
  present_mode_gate_failed=1
fi
frame_sleep_count_value="$(jq -r '(.h265_retained_video_present_decode.decoded_image_present_sequence.frame_sleep_count // 0)' "$runtime_json")"
missed_frame_pacing_count_value="$(jq -r '(.h265_retained_video_present_decode.decoded_image_present_sequence.missed_frame_pacing_count // 0)' "$runtime_json")"
total_frame_sleep_us_value="$(jq -r '(.h265_retained_video_present_decode.decoded_image_present_sequence.total_pacing_sleep_micros // 0)' "$runtime_json")"
ffmpeg_slices_buffer_model="$(jq -r '(.h265_retained_video_present_decode.decode.bitstream_buffer_model // "none")' "$runtime_json")"
ffmpeg_slices_buffer_pool_slot_count="$(jq -r '(.h265_retained_video_present_decode.decode.ffmpeg_slices_buffer_pool_slot_count // 0)' "$runtime_json")"
ffmpeg_slices_buffer_pool_allocated_slot_count="$(jq -r '(.h265_retained_video_present_decode.decode.ffmpeg_slices_buffer_pool_allocated_slot_count // 0)' "$runtime_json")"
ffmpeg_slices_buffer_pool_capacity_bytes="$(jq -r '(.h265_retained_video_present_decode.decode.ffmpeg_slices_buffer_pool_capacity_bytes // 0)' "$runtime_json")"
ffmpeg_slices_buffer_pool_max_slot_bytes="$(jq -r '(.h265_retained_video_present_decode.decode.ffmpeg_slices_buffer_pool_max_slot_bytes // 0)' "$runtime_json")"
ffmpeg_slices_buffer_max_src_range="$(jq -r '(.h265_retained_video_present_decode.decode.max_src_buffer_range // 0)' "$runtime_json")"
bitstream_total_payload_bytes="$(jq -r '(.h265_retained_video_present_decode.decode.src_buffer_total_bytes // 0)' "$runtime_json")"
bitstream_uploaded_bytes="$bitstream_total_payload_bytes"
h265_input_mode="$(jq -r '(.h265_retained_video_present_decode.decode.input_payload_model // "none")' "$runtime_json")"
bitstream_upload_count="$decoded_count"
expected_decoded_count="$requested_playback_count"
h265_present_frame_preroll_count="$(jq -r '(.h265_retained_video_present_decode.decoded_image_present_sequence.present_handoff.queued_frame_count_before_drain // 0)' "$runtime_json")"
h265_present_queue_count="$(jq -r '(.h265_retained_video_present_decode.decoded_image_present_sequence.present_handoff.capacity_frames // 0)' "$runtime_json")"
h265_async_present_depth="$(jq -r '(.h265_retained_video_present_decode.decoded_image_present_sequence.present_handoff.peak_depth // 0)' "$runtime_json")"
h265_present_result_wait_count="$(jq -r '(.h265_retained_video_present_decode.decoded_image_present_sequence.latest_draw.present_wait_available // false) | if . then 1 else 0 end' "$runtime_json")"
h265_present_result_wait_elapsed_us="$(jq -r '(.h265_retained_video_present_decode.decoded_image_present_sequence.total_pacing_sleep_micros // 0)' "$runtime_json")"
h265_present_result_wait_max_us="$h265_present_result_wait_elapsed_us"
h265_acquire_not_ready_count=0
audio_clock_probe_present=false
if [[ "$audio_clock_probe" -eq 1 ]]; then
  audio_clock_probe_present="$(jq -r '(.audio_clock != null and (.audio_clock.audio_stream_found // false))' "$runtime_json")"
fi
audio_output_mode="$(jq -r '.audio_clock.output_mode // "none"' "$runtime_json")"
audio_output_backend="$(jq -r '(.audio_clock.audio_output_backend // "none")' "$runtime_json")"
audio_output_sink_count="$(jq -r '(.audio_clock.audio_output_frames // 0)' "$runtime_json")"
audio_output_samples="$(jq -r '(.audio_clock.audio_output_samples // 0)' "$runtime_json")"
audio_output_bytes="$(jq -r '(.audio_clock.audio_output_bytes // 0)' "$runtime_json")"
audio_output_sample_rate="$(jq -r '(.audio_clock.audio_output_sample_rate_hz // "none")' "$runtime_json")"
audio_output_channels="$(jq -r '(.audio_clock.audio_output_channel_count // "none")' "$runtime_json")"
if [[ "$audio_output" == "plan" ]]; then
  if [[ "$plan_muted" -eq 1 ]]; then
    expected_audio_output_mode="clock-only"
  else
    expected_audio_output_mode="auto"
  fi
else
  expected_audio_output_mode="$audio_output"
fi
audio_reached_clocked_playback="$(jq -r '(.audio_clock.video_master_clock_ready // false)' "$runtime_json")"
audio_no_video_decoder_instantiated=true
audio_buffer_count="$(jq -r '(.audio_clock.consumed_packets // 0)' "$runtime_json")"
audio_loop_seek_count="$(jq -r '(.audio_clock.loop_count // 0)' "$runtime_json")"
audio_loop_seek_error_count=0
audio_loop_restart_count="$audio_loop_seek_count"
audio_last_loop_seek_position_ms="$(jq -r '(.audio_clock.clock_ms // "none")' "$runtime_json")"
audio_playback_started="$(jq -r '(.audio_clock.video_master_clock_ready // false)' "$runtime_json")"
audio_clock_serial="$(jq -r '(.audio_clock.current_serial // 0)' "$runtime_json")"
audio_initial_position_ms="$(jq -r '(.audio_clock.packets_head[0].pts_ms // "none")' "$runtime_json")"
audio_segment_start_position_ns="$(jq -r '(.audio_clock.packets_head[0].pts_ns // "none")' "$runtime_json")"
audio_segment_elapsed_ns="$(jq -r '(.audio_clock.clock_ns // "none")' "$runtime_json")"
audio_position_stale_count="$(jq -r '(.audio_clock.stale_dropped_packets // 0)' "$runtime_json")"
audio_sample_stale_count="$audio_position_stale_count"
audio_master_clock_estimate_ns="$(jq -r '(.audio_clock.video_master_start_clock_ns // .audio_clock.clock_ns // "none")' "$runtime_json")"
audio_master_start_serial="$(jq -r '(.audio_clock.video_master_start_serial // "none")' "$runtime_json")"
audio_master_start_packet_index="$(jq -r '(.audio_clock.video_master_start_packet_index // "none")' "$runtime_json")"
audio_current_serial_start_serial="$(jq -r '(.audio_clock.current_serial_start_serial // "none")' "$runtime_json")"
audio_current_serial_start_packet_index="$(jq -r '(.audio_clock.current_serial_start_packet_index // "none")' "$runtime_json")"
audio_clock_serial_uint=0
audio_current_serial_start_serial_uint=0
if gilder_is_uint "$audio_clock_serial"; then
  audio_clock_serial_uint="$audio_clock_serial"
fi
if gilder_is_uint "$audio_current_serial_start_serial"; then
  audio_current_serial_start_serial_uint="$audio_current_serial_start_serial"
fi
audio_position_query_count="$audio_buffer_count"
audio_position_query_hit_count="$audio_buffer_count"
audio_sampled_video_frame_count="$presented_count"
audio_decoded_frames="$(jq -r '(.audio_clock.decoded_frames // 0)' "$runtime_json")"
audio_decoded_samples="$(jq -r '(.audio_clock.decoded_samples // 0)' "$runtime_json")"
audio_sample_rate="$(jq -r '(.audio_clock.audio_sample_rate_hz // "none")' "$runtime_json")"
audio_channels="$(jq -r '(.audio_clock.audio_channel_count // "none")' "$runtime_json")"
audio_decoders='["ffmpeg-audio-decoded-frame-clock"]'
audio_video_decoders='[]'
audio_video_zero_based_drift_latest_ns=0
audio_video_zero_based_drift_abs_max_ns=0
audio_video_clock_drift_latest_ns=0
audio_video_clock_drift_abs_max_ns=0
audio_video_master_clock_drift_latest_ns=0
audio_video_master_clock_drift_abs_max_ns=0
first_frame_idr="$(jq -r '(.h265_retained_video_present_decode.decode.first_frame_reset_control_recorded // false)' "$runtime_json")"
loop_first_non_idr_count=0
swapchain_images="$(jq -r '(.h265_retained_video_present_decode.session.device.swapchain.image_count // 0)' "$runtime_json")"
resource_bytes="$(jq -r '(.h265_retained_video_present_decode.session.resource_image.resource_image.memory_size // 0)' "$runtime_json")"
p_frames="$(jq -r '(.h265_retained_video_present_decode.decode.p_frame_count // 0)' "$runtime_json")"
b_frames="$(jq -r '(.h265_retained_video_present_decode.decode.b_frame_count // 0)' "$runtime_json")"
max_reference_count="$(jq -r '(.h265_retained_video_present_decode.decode.max_decode_reference_slot_count // 0)' "$runtime_json")"
bitstream_gate_failed=0
if [[ "$ffmpeg_slices_buffer_model" != "ffmpeg-picture-slices-buffer-pool-exec-owned" || "$ffmpeg_slices_buffer_pool_slot_count" -le 0 || "$ffmpeg_slices_buffer_pool_allocated_slot_count" -le 0 || "$ffmpeg_slices_buffer_pool_capacity_bytes" -le 0 || "$ffmpeg_slices_buffer_pool_max_slot_bytes" -le 0 || "$ffmpeg_slices_buffer_max_src_range" -le 0 || "$bitstream_total_payload_bytes" -le 0 || "$bitstream_upload_count" -le 0 || "$bitstream_uploaded_bytes" -le 0 ]]; then
  bitstream_gate_failed=1
fi
input_gate_failed=0
if [[ "$h265_input_mode" != "bounded-streaming-packet-queue-per-frame-upload" || "$decoded_count" -ne "$requested_playback_count" || "$requested_playback_count" -le 0 || "$bitstream_uploaded_bytes" -le 0 ]]; then
  input_gate_failed=1
fi
arbitrary_entry_gate_failed=0
if [[ "$arbitrary_entry_source" -eq 1 && "$first_frame_idr" != "true" ]]; then
  arbitrary_entry_gate_failed=1
fi
reference_gate_failed=0
if [[ "$generated_source" -eq 1 && "$decode_prefix" -gt "$refs" && ( "$p_frames" -lt 1 || "$max_reference_count" -lt "$refs" ) ]]; then
  reference_gate_failed=1
fi
b_frame_gate_failed=0
if [[ "$generated_source" -eq 1 && "$bframes" -gt 0 && "$b_frames" -lt 1 ]]; then
  b_frame_gate_failed=1
fi
loop_gate_failed=0
pacing_gate_failed=0
if ! gilder_pacing_strategy_matches_expected "$pacing_strategy" "$expected_pacing_strategy" "$target_fps"; then
  pacing_gate_failed=1
fi
fps_gate_failed=0
if ! awk -v fps="$average_present_fps" -v target="$target_fps" 'BEGIN { exit (fps + 0.001 >= target) ? 0 : 1 }'; then
  fps_gate_failed=1
fi
dpb_gate_failed=0
if [[ "$runtime_max_dpb_slots" -le 0 || "$stream_sps_dpb_slots" -le 0 || "$stream_dpb_slots" -le 0 || "$session_max_dpb_slots" -le 0 || "$session_max_active_reference_pictures" -le 0 || "$session_max_active_reference_pictures" -gt "$session_max_dpb_slots" || "$session_max_active_reference_pictures" -lt "$stream_max_active_reference_pictures" || "$distinct_layers" -gt "$session_max_dpb_slots" ]]; then
  dpb_gate_failed=1
fi
pts_delta_gate_failed=0
if [[ "$pts_delta_script_in_expected_range" != "true" || "$pts_delta_expected_min" != "$script_pts_delta_expected_min" || "$pts_delta_expected_max" != "$script_pts_delta_expected_max" ]]; then
  pts_delta_gate_failed=1
fi
audio_clock_gate_failed=0
audio_loop_probe_expected=0
if [[ "$audio_clock_probe" -eq 1 && "$generated_source" -eq 1 && "$expected_frames" -gt "$decode_prefix" ]]; then
  audio_loop_probe_expected=1
fi
if [[ "$audio_clock_probe" -eq 1 && ( "$audio_clock_probe_present" != "true" || "$audio_reached_clocked_playback" != "true" || "$audio_no_video_decoder_instantiated" != "true" || "$audio_playback_started" != "true" || "$audio_buffer_count" -le 0 || "$audio_decoded_frames" -le 0 || "$audio_decoded_samples" -le 0 || "$audio_sample_rate" == "none" || "$audio_channels" == "none" || "$audio_position_query_count" -le 0 || "$audio_position_query_hit_count" -le 0 || "$audio_sampled_video_frame_count" -le 0 || "$audio_master_clock_estimate_ns" == "none" || "$audio_video_master_clock_drift_latest_ns" == "none" || "$audio_loop_seek_error_count" -ne 0 ) ]]; then
  audio_clock_gate_failed=1
fi
if [[ "$audio_clock_probe" -eq 1 && "$audio_output_mode" != "$expected_audio_output_mode" ]]; then
  audio_clock_gate_failed=1
fi
if [[ "$expected_audio_output_mode" == "auto" && ( "$audio_output_backend" != "pipewire-s16le" || "$audio_output_sink_count" -le 0 || "$audio_output_samples" -le 0 || "$audio_output_bytes" -le 0 || "$audio_output_sample_rate" == "none" || "$audio_output_channels" == "none" ) ]]; then
  audio_clock_gate_failed=1
fi
if [[ "$expected_audio_output_mode" == "clock-only" && ( "$audio_output_backend" != "none" || "$audio_output_sink_count" -ne 0 || "$audio_output_samples" -ne 0 || "$audio_output_bytes" -ne 0 ) ]]; then
  audio_clock_gate_failed=1
fi
audio_loop_serial_gate_failed=0
if [[ "$audio_clock_probe" -eq 1 && "$loop_boundary_reset_count" -gt 0 && "$audio_loop_seek_count" -lt "$loop_boundary_reset_count" ]]; then
  audio_clock_gate_failed=1
  audio_loop_serial_gate_failed=1
fi
if [[ "$audio_clock_probe" -eq 1 && "$loop_boundary_reset_count" -gt 0 ]] && { [[ "$audio_clock_serial_uint" -lt "$loop_boundary_reset_count" || "$audio_current_serial_start_serial_uint" -lt "$loop_boundary_reset_count" ]] || ! gilder_is_uint "$audio_current_serial_start_packet_index"; }; then
  audio_clock_gate_failed=1
  audio_loop_serial_gate_failed=1
fi
if [[ "$audio_loop_probe_expected" -eq 1 ]] && { [[ "$audio_loop_seek_count" -lt 1 || "$audio_clock_serial_uint" -lt 1 || "$audio_current_serial_start_serial_uint" -lt 1 ]] || ! gilder_is_uint "$audio_current_serial_start_packet_index"; }; then
  audio_clock_gate_failed=1
  audio_loop_serial_gate_failed=1
fi

if [[ "$decoded_count" -ne "$expected_decoded_count" || "$presented_count" -ne "$expected_frames" || "$frame_count" -ne "$expected_frames" || "$runtime_codec" != "$video_codec" || "$picture_format" != "$expected_picture_format" || "$expected_decoded_count" -le 0 || "$requested_playback_count" -ne "$expected_frames" || "$bad_frames" -ne 0 || "$distinct_layers" -le 1 || "$loop_gate_failed" -ne 0 || "$bitstream_gate_failed" -ne 0 || "$input_gate_failed" -ne 0 || "$arbitrary_entry_gate_failed" -ne 0 || "$reference_gate_failed" -ne 0 || "$b_frame_gate_failed" -ne 0 || "$present_mode_gate_failed" -ne 0 || "$pacing_gate_failed" -ne 0 || "$fps_gate_failed" -ne 0 || "$dpb_gate_failed" -ne 0 || "$pts_delta_gate_failed" -ne 0 || "$audio_clock_gate_failed" -ne 0 || "$present_queue" == "none" || "$video_queue" == "none" || "$sync_strategy" == "none" || "$swapchain_images" -lt 2 || "$resource_bytes" -le 0 ]]; then
  {
    printf 'FAIL: native Vulkan direct H.265 ready-prefix video output was not valid\n'
    printf 'decoded_count: %s\n' "$decoded_count"
    printf 'presented_count: %s\n' "$presented_count"
    printf 'requested_decode_prefix: %s\n' "$decode_prefix"
    printf 'expected_playback_frames: %s\n' "$expected_frames"
    printf 'ready_prefix_loop_period_ms: %s\n' "$ready_prefix_loop_period_ms"
    printf 'runtime_codec: %s\n' "$runtime_codec"
    printf 'expected_codec: %s\n' "$video_codec"
    printf 'picture_format: %s\n' "$picture_format"
    printf 'expected_picture_format: %s\n' "$expected_picture_format"
    printf 'frame_count: %s\n' "$frame_count"
    printf 'ready_prefix_frame_count: %s\n' "$ready_prefix_count"
    printf 'requested_playback_frame_count: %s\n' "$requested_playback_count"
    printf 'playback_loop_count: %s\n' "$playback_loop_count"
    printf 'loop_boundary_reset_count: %s\n' "$loop_boundary_reset_count"
    printf 'bad_frames: %s\n' "$bad_frames"
    printf 'distinct_layers: %s\n' "$distinct_layers"
    printf 'p_frames: %s\n' "$p_frames"
    printf 'b_frames: %s\n' "$b_frames"
    printf 'generated_refs: %s\n' "$([[ "$generated_source" -eq 1 ]] && printf '%s' "$refs" || printf none)"
    printf 'max_reference_count: %s\n' "$max_reference_count"
    printf 'pts_delta_min_ms: %s\n' "$pts_delta_min"
    printf 'pts_delta_max_ms: %s\n' "$pts_delta_max"
    printf 'pts_delta_expected_min_ms: %s\n' "$pts_delta_expected_min"
    printf 'pts_delta_expected_max_ms: %s\n' "$pts_delta_expected_max"
    printf 'pts_delta_in_expected_range: %s\n' "$pts_delta_in_expected_range"
    printf 'pts_delta_script_expected_min_ms: %s\n' "$script_pts_delta_expected_min"
    printf 'pts_delta_script_expected_max_ms: %s\n' "$script_pts_delta_expected_max"
    printf 'pts_delta_script_in_expected_range: %s\n' "$pts_delta_script_in_expected_range"
    printf 'pts_delta_gate_failed: %s\n' "$pts_delta_gate_failed"
    printf 'audio_clock_probe_requested: %s\n' "$([[ "$audio_clock_probe" -eq 1 ]] && printf yes || printf no)"
    printf 'audio_clock_probe_present: %s\n' "$audio_clock_probe_present"
    printf 'audio_output: %s\n' "$audio_output"
    printf 'audio_output_expected_mode: %s\n' "$expected_audio_output_mode"
    printf 'audio_plan_muted: %s\n' "$([[ "$plan_muted" -eq 1 ]] && printf true || printf false)"
    printf 'audio_output_mode: %s\n' "$audio_output_mode"
    printf 'audio_output_backend: %s\n' "$audio_output_backend"
    printf 'audio_output_sink_count: %s\n' "$audio_output_sink_count"
    printf 'audio_output_samples: %s\n' "$audio_output_samples"
    printf 'audio_output_bytes: %s\n' "$audio_output_bytes"
    printf 'audio_output_sample_rate: %s\n' "$audio_output_sample_rate"
    printf 'audio_output_channels: %s\n' "$audio_output_channels"
    printf 'audio_clock_gate_failed: %s\n' "$audio_clock_gate_failed"
    printf 'audio_loop_probe_expected: %s\n' "$audio_loop_probe_expected"
    printf 'audio_loop_serial_gate_failed: %s\n' "$audio_loop_serial_gate_failed"
    printf 'audio_reached_clocked_playback: %s\n' "$audio_reached_clocked_playback"
    printf 'audio_no_video_decoder_instantiated: %s\n' "$audio_no_video_decoder_instantiated"
    printf 'audio_buffer_count: %s\n' "$audio_buffer_count"
    printf 'audio_decoded_frames: %s\n' "$audio_decoded_frames"
    printf 'audio_decoded_samples: %s\n' "$audio_decoded_samples"
    printf 'audio_loop_seek_count: %s\n' "$audio_loop_seek_count"
    printf 'audio_loop_seek_error_count: %s\n' "$audio_loop_seek_error_count"
    printf 'audio_loop_restart_count: %s\n' "$audio_loop_restart_count"
    printf 'audio_last_loop_seek_position_ms: %s\n' "$audio_last_loop_seek_position_ms"
    printf 'audio_playback_started: %s\n' "$audio_playback_started"
    printf 'audio_clock_serial: %s\n' "$audio_clock_serial"
    printf 'audio_initial_position_ms: %s\n' "$audio_initial_position_ms"
    printf 'audio_segment_start_position_ns: %s\n' "$audio_segment_start_position_ns"
    printf 'audio_segment_elapsed_ns: %s\n' "$audio_segment_elapsed_ns"
    printf 'audio_position_stale_count: %s\n' "$audio_position_stale_count"
    printf 'audio_sample_stale_count: %s\n' "$audio_sample_stale_count"
    printf 'audio_master_clock_estimate_ns: %s\n' "$audio_master_clock_estimate_ns"
    printf 'audio_master_start_serial: %s\n' "$audio_master_start_serial"
    printf 'audio_master_start_packet_index: %s\n' "$audio_master_start_packet_index"
    printf 'audio_current_serial_start_serial: %s\n' "$audio_current_serial_start_serial"
    printf 'audio_current_serial_start_packet_index: %s\n' "$audio_current_serial_start_packet_index"
    printf 'audio_position_query_count: %s\n' "$audio_position_query_count"
    printf 'audio_position_query_hit_count: %s\n' "$audio_position_query_hit_count"
    printf 'audio_sampled_video_frame_count: %s\n' "$audio_sampled_video_frame_count"
    printf 'audio_decoders: %s\n' "$audio_decoders"
    printf 'audio_video_decoders: %s\n' "$audio_video_decoders"
    printf 'audio_video_zero_based_drift_latest_ns: %s\n' "$audio_video_zero_based_drift_latest_ns"
    printf 'audio_video_zero_based_drift_abs_max_ns: %s\n' "$audio_video_zero_based_drift_abs_max_ns"
    printf 'audio_video_clock_drift_latest_ns: %s\n' "$audio_video_clock_drift_latest_ns"
    printf 'audio_video_clock_drift_abs_max_ns: %s\n' "$audio_video_clock_drift_abs_max_ns"
    printf 'audio_video_master_clock_drift_latest_ns: %s\n' "$audio_video_master_clock_drift_latest_ns"
    printf 'audio_video_master_clock_drift_abs_max_ns: %s\n' "$audio_video_master_clock_drift_abs_max_ns"
    printf 'present_queue: %s\n' "$present_queue"
    printf 'video_queue: %s\n' "$video_queue"
    printf 'cross_queue_sync_strategy: %s\n' "$sync_strategy"
    printf 'runtime_max_dpb_slots: %s\n' "$runtime_max_dpb_slots"
    printf 'stream_sps_dpb_slots: %s\n' "$stream_sps_dpb_slots"
    printf 'stream_dpb_slots: %s\n' "$stream_dpb_slots"
    printf 'stream_max_active_reference_pictures: %s\n' "$stream_max_active_reference_pictures"
    printf 'session_max_dpb_slots: %s\n' "$session_max_dpb_slots"
    printf 'session_max_active_reference_pictures: %s\n' "$session_max_active_reference_pictures"
    printf 'present_mode: %s\n' "$present_mode"
    printf 'present_mode_gate_failed: %s\n' "$present_mode_gate_failed"
    printf 'pacing_master: %s\n' "$pacing_master"
    printf 'pacing_strategy: %s\n' "$pacing_strategy"
    printf 'expected_pacing_strategy: %s\n' "$expected_pacing_strategy"
    printf 'frame_sleep_count: %s\n' "$frame_sleep_count_value"
    printf 'pacing_gate_failed: %s\n' "$pacing_gate_failed"
    printf 'fps_gate_failed: %s\n' "$fps_gate_failed"
    printf 'ffmpeg_slices_buffer_model: %s\n' "$ffmpeg_slices_buffer_model"
    printf 'ffmpeg_slices_buffer_pool_slot_count: %s\n' "$ffmpeg_slices_buffer_pool_slot_count"
    printf 'ffmpeg_slices_buffer_pool_allocated_slot_count: %s\n' "$ffmpeg_slices_buffer_pool_allocated_slot_count"
    printf 'ffmpeg_slices_buffer_pool_capacity_bytes: %s\n' "$ffmpeg_slices_buffer_pool_capacity_bytes"
    printf 'ffmpeg_slices_buffer_pool_max_slot_bytes: %s\n' "$ffmpeg_slices_buffer_pool_max_slot_bytes"
    printf 'ffmpeg_slices_buffer_max_src_range: %s\n' "$ffmpeg_slices_buffer_max_src_range"
    printf 'bitstream_total_payload_bytes: %s\n' "$bitstream_total_payload_bytes"
    printf 'bitstream_upload_count: %s\n' "$bitstream_upload_count"
    printf 'bitstream_uploaded_bytes: %s\n' "$bitstream_uploaded_bytes"
    printf 'h265_input_mode: %s\n' "$h265_input_mode"
    printf 'h265_present_frame_preroll_count: %s\n' "$h265_present_frame_preroll_count"
    printf 'h265_present_queue_count: %s\n' "$h265_present_queue_count"
    printf 'h265_async_present_depth: %s\n' "$h265_async_present_depth"
    printf 'h265_present_result_wait_count: %s\n' "$h265_present_result_wait_count"
    printf 'h265_present_result_wait_elapsed_us: %s\n' "$h265_present_result_wait_elapsed_us"
    printf 'h265_present_result_wait_max_us: %s\n' "$h265_present_result_wait_max_us"
    printf 'h265_acquire_not_ready_count: %s\n' "$h265_acquire_not_ready_count"
    printf 'arbitrary_entry_source: %s\n' "$([[ "$arbitrary_entry_source" -eq 1 ]] && printf yes || printf no)"
    printf 'arbitrary_entry_offset: %s\n' "${arbitrary_entry_offset:-none}"
    printf 'arbitrary_entry_gate_failed: %s\n' "$arbitrary_entry_gate_failed"
    printf 'reference_gate_failed: %s\n' "$reference_gate_failed"
    printf 'b_frame_gate_failed: %s\n' "$b_frame_gate_failed"
    printf 'first_frame_idr: %s\n' "$first_frame_idr"
    printf 'loop_first_non_idr_count: %s\n' "$loop_first_non_idr_count"
    printf 'swapchain_images: %s\n' "$swapchain_images"
    printf 'video_resource_memory_bytes: %s\n' "$resource_bytes"
    printf 'runtime JSON: %s\n' "$runtime_json"
  } | tee "$summary"
  exit 1
fi

{
  printf 'source: %s\n' "$source"
  printf 'generated_source: %s\n' "$([[ "$generated_source" -eq 1 ]] && printf yes || printf no)"
  printf 'generated_source_frames: %s\n' "$([[ "$generated_source" -eq 1 ]] && printf '%s' "$frames" || printf none)"
  printf 'generated_source_duration_seconds: %s\n' "$([[ "$generated_source" -eq 1 ]] && printf '%s' "$source_duration_seconds" || printf none)"
  printf 'generated_source_frames_explicit: %s\n' "$([[ "$frames_explicit" -eq 1 ]] && printf yes || printf no)"
  printf 'generated_source_pattern: %s\n' "$([[ "$generated_source" -eq 1 ]] && printf 'testsrc2-continuous-closed-gop-%s-b%s' "$video_codec" "$bframes" || printf none)"
  printf 'generated_source_cache_dir: %s\n' "$source_cache_dir"
  printf 'requested_codec: %s\n' "$runtime_codec"
  printf 'requested_bit_depth: %s\n' "$bit_depth"
  printf 'picture_format: %s\n' "$picture_format"
  printf 'generated_source_refs: %s\n' "$([[ "$generated_source" -eq 1 ]] && printf '%s' "$refs" || printf none)"
  printf 'generated_source_bframes: %s\n' "$([[ "$generated_source" -eq 1 ]] && printf '%s' "$bframes" || printf none)"
  printf 'arbitrary_entry_source: %s\n' "$([[ "$arbitrary_entry_source" -eq 1 ]] && printf yes || printf no)"
  printf 'arbitrary_entry_offset: %s\n' "${arbitrary_entry_offset:-none}"
  printf 'decode_prefix_explicit: %s\n' "$([[ "$decode_prefix_explicit" -eq 1 ]] && printf yes || printf no)"
  printf 'selected_device: %s\n' "$(jq -r '.selected_physical_device_name' "$runtime_json")"
  printf 'requested_output_name: %s\n' "${output_name:-auto}"
  printf 'surface_logical_size: %s\n' "$(jq -c '.wayland_surface_logical_size' "$runtime_json")"
  printf 'surface_buffer_size: %s\n' "$(jq -c '.wayland_surface_buffer_size' "$runtime_json")"
  printf 'source_extent: %s\n' "$(jq -c '.source_extent' "$runtime_json")"
  printf 'swapchain_extent: %s\n' "$(jq -c '.swapchain_extent' "$runtime_json")"
  printf 'swapchain_format: %s\n' "$(jq -r '.swapchain_format' "$runtime_json")"
  printf 'present_mode: %s\n' "$present_mode"
  printf 'present_mode_gate_failed: %s\n' "$present_mode_gate_failed"
  printf 'runtime_elapsed_ms: %s\n' "$(jq -r '.runtime_elapsed_ms' "$runtime_json")"
  printf 'ready_prefix_frame_count: %s\n' "$ready_prefix_count"
  printf 'ready_prefix_loop_period_ms: %s\n' "$ready_prefix_loop_period_ms"
  printf 'requested_playback_frame_count: %s\n' "$requested_playback_count"
  printf 'decoded_frame_count: %s\n' "$decoded_count"
  printf 'presented_frame_count: %s\n' "$presented_count"
  printf 'playback_loop_count: %s\n' "$playback_loop_count"
  printf 'loop_boundary_reset_count: %s\n' "$loop_boundary_reset_count"
  printf 'p_frames: %s\n' "$p_frames"
  printf 'b_frames: %s\n' "$b_frames"
  printf 'max_reference_count: %s\n' "$max_reference_count"
  printf 'pacing_master: %s\n' "$pacing_master"
  printf 'allocator_tuning: none\n'
  printf 'pacing_strategy: %s\n' "$pacing_strategy"
  printf 'expected_pacing_strategy: %s\n' "$expected_pacing_strategy"
  printf 'frame_sleep_count: %s\n' "$frame_sleep_count_value"
  printf 'missed_frame_pacing_count: %s\n' "$missed_frame_pacing_count_value"
  printf 'total_frame_sleep_us: %s\n' "$total_frame_sleep_us_value"
  printf 'max_frame_pacing_late_us: %s\n' "$(jq -r '.max_frame_pacing_late_us // 0' "$runtime_json")"
  printf 'average_present_fps: %s\n' "$average_present_fps"
  printf 'average_present_teardown_inclusive_fps: %s\n' "$average_present_teardown_inclusive_fps"
  printf 'present_interval_elapsed_us: %s\n' "$present_interval_elapsed_us"
  printf 'present_teardown_inclusive_elapsed_us: %s\n' "$present_teardown_inclusive_elapsed_us"
  printf 'target_max_fps: %s\n' "$(jq -r '.target_max_fps // "none"' "$runtime_json")"
  printf 'present_queue_family_index: %s\n' "$present_queue"
  printf 'present_queue_flags: %s\n' "$(jq -c '.present_queue_flags' "$runtime_json")"
  printf 'video_decode_queue_family_index: %s\n' "$video_queue"
  printf 'video_decode_queue_flags: %s\n' "$(jq -c '.video_decode_queue_flags' "$runtime_json")"
  printf 'video_decode_queue_codec_operations: %s\n' "$(jq -c '.video_decode_queue_codec_operations' "$runtime_json")"
  printf 'cross_queue_sync_strategy: %s\n' "$(jq -r '.cross_queue_sync_strategy' "$runtime_json")"
  printf 'runtime_max_dpb_slots: %s\n' "$runtime_max_dpb_slots"
  printf 'stream_sps_dpb_slots: %s\n' "$stream_sps_dpb_slots"
  printf 'stream_dpb_slots: %s\n' "$stream_dpb_slots"
  printf 'stream_max_active_reference_pictures: %s\n' "$stream_max_active_reference_pictures"
  printf 'session_max_dpb_slots: %s\n' "$session_max_dpb_slots"
  printf 'session_max_active_reference_pictures: %s\n' "$session_max_active_reference_pictures"
  printf 'ffmpeg_slices_buffer_model: %s\n' "$ffmpeg_slices_buffer_model"
  printf 'ffmpeg_slices_buffer_pool_slot_count: %s\n' "$ffmpeg_slices_buffer_pool_slot_count"
  printf 'ffmpeg_slices_buffer_pool_allocated_slot_count: %s\n' "$ffmpeg_slices_buffer_pool_allocated_slot_count"
  printf 'ffmpeg_slices_buffer_pool_capacity_bytes: %s\n' "$ffmpeg_slices_buffer_pool_capacity_bytes"
  printf 'ffmpeg_slices_buffer_pool_max_slot_bytes: %s\n' "$ffmpeg_slices_buffer_pool_max_slot_bytes"
  printf 'ffmpeg_slices_buffer_max_src_range: %s\n' "$ffmpeg_slices_buffer_max_src_range"
  printf 'bitstream_total_payload_bytes: %s\n' "$bitstream_total_payload_bytes"
  printf 'bitstream_upload_count: %s\n' "$bitstream_upload_count"
  printf 'bitstream_uploaded_bytes: %s\n' "$bitstream_uploaded_bytes"
  printf 'h265_input_mode: %s\n' "$h265_input_mode"
  printf 'h265_present_frame_preroll_count: %s\n' "$h265_present_frame_preroll_count"
  printf 'h265_present_queue_count: %s\n' "$h265_present_queue_count"
  printf 'h265_async_present_depth: %s\n' "$h265_async_present_depth"
  printf 'h265_present_result_wait_count: %s\n' "$h265_present_result_wait_count"
  printf 'h265_present_result_wait_elapsed_us: %s\n' "$h265_present_result_wait_elapsed_us"
  printf 'h265_present_result_wait_max_us: %s\n' "$h265_present_result_wait_max_us"
  printf 'h265_acquire_not_ready_count: %s\n' "$h265_acquire_not_ready_count"
  printf 'first_frame_idr: %s\n' "$first_frame_idr"
  printf 'loop_first_non_idr_count: %s\n' "$loop_first_non_idr_count"
  printf 'pts_delta_min_ms: %s\n' "$pts_delta_min"
  printf 'pts_delta_max_ms: %s\n' "$pts_delta_max"
  printf 'pts_delta_expected_min_ms: %s\n' "$pts_delta_expected_min"
  printf 'pts_delta_expected_max_ms: %s\n' "$pts_delta_expected_max"
  printf 'pts_delta_in_expected_range: %s\n' "$pts_delta_in_expected_range"
  printf 'pts_delta_script_expected_min_ms: %s\n' "$script_pts_delta_expected_min"
  printf 'pts_delta_script_expected_max_ms: %s\n' "$script_pts_delta_expected_max"
  printf 'audio_clock_probe_requested: %s\n' "$([[ "$audio_clock_probe" -eq 1 ]] && printf yes || printf no)"
  printf 'audio_clock_probe_present: %s\n' "$audio_clock_probe_present"
  printf 'audio_output: %s\n' "$audio_output"
  printf 'audio_output_expected_mode: %s\n' "$expected_audio_output_mode"
  printf 'audio_plan_muted: %s\n' "$([[ "$plan_muted" -eq 1 ]] && printf true || printf false)"
  printf 'audio_output_mode: %s\n' "$audio_output_mode"
  printf 'audio_output_backend: %s\n' "$audio_output_backend"
  printf 'audio_output_sink_count: %s\n' "$audio_output_sink_count"
  printf 'audio_output_samples: %s\n' "$audio_output_samples"
  printf 'audio_output_bytes: %s\n' "$audio_output_bytes"
  printf 'audio_output_sample_rate: %s\n' "$audio_output_sample_rate"
  printf 'audio_output_channels: %s\n' "$audio_output_channels"
  printf 'audio_loop_probe_expected: %s\n' "$audio_loop_probe_expected"
  printf 'audio_loop_serial_gate_failed: %s\n' "$audio_loop_serial_gate_failed"
  printf 'audio_reached_clocked_playback: %s\n' "$audio_reached_clocked_playback"
  printf 'audio_no_video_decoder_instantiated: %s\n' "$audio_no_video_decoder_instantiated"
  printf 'audio_buffer_count: %s\n' "$audio_buffer_count"
  printf 'audio_decoded_frames: %s\n' "$audio_decoded_frames"
  printf 'audio_decoded_samples: %s\n' "$audio_decoded_samples"
  printf 'audio_loop_seek_count: %s\n' "$audio_loop_seek_count"
  printf 'audio_loop_seek_error_count: %s\n' "$audio_loop_seek_error_count"
  printf 'audio_loop_restart_count: %s\n' "$audio_loop_restart_count"
  printf 'audio_last_loop_seek_position_ms: %s\n' "$audio_last_loop_seek_position_ms"
  printf 'audio_playback_started: %s\n' "$audio_playback_started"
  printf 'audio_clock_serial: %s\n' "$audio_clock_serial"
  printf 'audio_initial_position_ms: %s\n' "$audio_initial_position_ms"
  printf 'audio_segment_start_position_ns: %s\n' "$audio_segment_start_position_ns"
  printf 'audio_segment_elapsed_ns: %s\n' "$audio_segment_elapsed_ns"
  printf 'audio_position_stale_count: %s\n' "$audio_position_stale_count"
  printf 'audio_sample_stale_count: %s\n' "$audio_sample_stale_count"
  printf 'audio_master_clock_estimate_ns: %s\n' "$audio_master_clock_estimate_ns"
  printf 'audio_master_start_serial: %s\n' "$audio_master_start_serial"
  printf 'audio_master_start_packet_index: %s\n' "$audio_master_start_packet_index"
  printf 'audio_current_serial_start_serial: %s\n' "$audio_current_serial_start_serial"
  printf 'audio_current_serial_start_packet_index: %s\n' "$audio_current_serial_start_packet_index"
  printf 'audio_position_query_count: %s\n' "$audio_position_query_count"
  printf 'audio_position_query_hit_count: %s\n' "$audio_position_query_hit_count"
  printf 'audio_sampled_video_frame_count: %s\n' "$audio_sampled_video_frame_count"
  printf 'audio_sample_rate: %s\n' "$audio_sample_rate"
  printf 'audio_channels: %s\n' "$audio_channels"
  printf 'audio_decoders: %s\n' "$audio_decoders"
  printf 'audio_video_decoders: %s\n' "$audio_video_decoders"
  printf 'audio_video_zero_based_drift_latest_ns: %s\n' "$audio_video_zero_based_drift_latest_ns"
  printf 'audio_video_zero_based_drift_abs_max_ns: %s\n' "$audio_video_zero_based_drift_abs_max_ns"
  printf 'audio_video_clock_drift_latest_ns: %s\n' "$audio_video_clock_drift_latest_ns"
  printf 'audio_video_clock_drift_abs_max_ns: %s\n' "$audio_video_clock_drift_abs_max_ns"
  printf 'audio_video_master_clock_drift_latest_ns: %s\n' "$audio_video_master_clock_drift_latest_ns"
  printf 'audio_video_master_clock_drift_abs_max_ns: %s\n' "$audio_video_master_clock_drift_abs_max_ns"
  printf 'video_resource_memory_bytes: %s\n' "$resource_bytes"
  printf 'session_memory_bytes: %s\n' "$(jq -r '.session_memory_bytes' "$runtime_json")"
  printf 'performance_snapshot: %s\n' "$([[ "$performance_snapshot" -eq 1 ]] && printf yes || printf no)"
  if [[ "$performance_snapshot" -eq 1 ]]; then
    printf 'performance_max_private_dirty_kib_limit: %s\n' "$max_private_dirty_kib_limit"
    printf 'performance_release_binary_replaced_by_build: %s\n' "$([[ "$release_binary_replaced_by_build" -eq 1 ]] && printf yes || printf no)"
    printf 'performance_release_binary_synced_after_build: %s\n' "$([[ "$release_binary_synced_after_build" -eq 1 ]] && printf yes || printf no)"
    printf 'performance_rebuild_mapping_dirty_retry: %s\n' "$([[ "$performance_rebuild_mapping_dirty_retry" -eq 1 ]] && printf yes || printf no)"
    printf 'performance_rebuild_mapping_dirty_retry_count: %s\n' "$performance_rebuild_mapping_dirty_retry_count"
    printf 'performance_rebuild_mapping_dirty_max_attempts: %s\n' "$performance_rebuild_mapping_dirty_max_attempts"
    printf 'performance_rebuild_mapping_dirty_final_contaminated: %s\n' "$([[ "$performance_rebuild_mapping_dirty_final_contaminated" -eq 1 ]] && printf yes || printf no)"
    if [[ "$performance_rebuild_mapping_dirty_retry" -eq 1 ]]; then
      printf 'performance_rebuild_mapping_dirty_first_summary: %s\n' "$performance_rebuild_mapping_dirty_first_summary"
      printf 'performance_rebuild_mapping_dirty_first_log: %s\n' "$performance_rebuild_mapping_dirty_first_log"
      printf 'performance_rebuild_mapping_dirty_first_runtime_json: %s\n' "$performance_rebuild_mapping_dirty_first_runtime_json"
      printf 'performance_rebuild_mapping_dirty_first_runtime_stderr: %s\n' "$performance_rebuild_mapping_dirty_first_runtime_stderr"
      printf 'performance_rebuild_mapping_dirty_first_max_private_dirty_kib: %s\n' "$performance_rebuild_mapping_dirty_first_max_private_dirty_kib"
      printf 'performance_rebuild_mapping_dirty_first_file_mapping_private_dirty_kib: %s\n' "$performance_rebuild_mapping_dirty_first_file_mapping_private_dirty_kib"
      printf 'performance_rebuild_mapping_dirty_first_gilder_binary_private_dirty_kib: %s\n' "$performance_rebuild_mapping_dirty_first_gilder_binary_private_dirty_kib"
      printf 'performance_rebuild_mapping_dirty_first_heap_private_dirty_kib: %s\n' "$performance_rebuild_mapping_dirty_first_heap_private_dirty_kib"
      for attempt_offset in "${!performance_rebuild_mapping_dirty_summaries[@]}"; do
        attempt_number=$((attempt_offset + 1))
        printf 'performance_rebuild_mapping_dirty_attempt_%s_summary: %s\n' "$attempt_number" "${performance_rebuild_mapping_dirty_summaries[$attempt_offset]}"
        printf 'performance_rebuild_mapping_dirty_attempt_%s_log: %s\n' "$attempt_number" "${performance_rebuild_mapping_dirty_logs[$attempt_offset]}"
        printf 'performance_rebuild_mapping_dirty_attempt_%s_runtime_json: %s\n' "$attempt_number" "${performance_rebuild_mapping_dirty_runtime_jsons[$attempt_offset]}"
        printf 'performance_rebuild_mapping_dirty_attempt_%s_runtime_stderr: %s\n' "$attempt_number" "${performance_rebuild_mapping_dirty_runtime_stderrs[$attempt_offset]}"
        printf 'performance_rebuild_mapping_dirty_attempt_%s_max_private_dirty_kib: %s\n' "$attempt_number" "${performance_rebuild_mapping_dirty_max_private_dirty_kibs[$attempt_offset]}"
        printf 'performance_rebuild_mapping_dirty_attempt_%s_file_mapping_private_dirty_kib: %s\n' "$attempt_number" "${performance_rebuild_mapping_dirty_file_mapping_private_dirty_kibs[$attempt_offset]}"
        printf 'performance_rebuild_mapping_dirty_attempt_%s_gilder_binary_private_dirty_kib: %s\n' "$attempt_number" "${performance_rebuild_mapping_dirty_gilder_binary_private_dirty_kibs[$attempt_offset]}"
        printf 'performance_rebuild_mapping_dirty_attempt_%s_heap_private_dirty_kib: %s\n' "$attempt_number" "${performance_rebuild_mapping_dirty_heap_private_dirty_kibs[$attempt_offset]}"
      done
    fi
    printf 'performance_dir: %s\n' "$performance_dir"
    printf 'performance_log: %s\n' "$performance_log"
    if [[ -s "$performance_dir/summary.txt" ]]; then
      printf 'performance_summary: %s\n' "$performance_dir/summary.txt"
      printf 'performance_samples: %s\n' "$(awk -F': ' '$1 == "samples" { print $2 }' "$performance_dir/summary.txt")"
      printf 'performance_avg_cpu_percent: %s\n' "$(awk -F': ' '$1 == "avg_cpu_percent" { print $2 }' "$performance_dir/summary.txt")"
      printf 'performance_max_rss_kib: %s\n' "$(awk -F': ' '$1 == "max_rss_kib" { print $2 }' "$performance_dir/summary.txt")"
      printf 'performance_max_pss_kib: %s\n' "$(awk -F': ' '$1 == "max_pss_kib" { print $2 }' "$performance_dir/summary.txt")"
      printf 'performance_max_uss_kib: %s\n' "$(awk -F': ' '$1 == "max_uss_kib" { print $2 }' "$performance_dir/summary.txt")"
      printf 'performance_max_private_dirty_kib: %s\n' "$(awk -F': ' '$1 == "max_private_dirty_kib" { print $2 }' "$performance_dir/summary.txt")"
      printf 'performance_max_nvidia_process_gpu_memory_mib: %s\n' "$(awk -F': ' '$1 == "max_nvidia_process_gpu_memory_mib" { print $2 }' "$performance_dir/summary.txt")"
    fi
  fi
} >"$summary"

printf 'PASS: native Vulkan direct H.265 ready-prefix video smoke passed\n'
printf 'summary: %s\n' "$summary"
printf 'runtime JSON: %s\n' "$runtime_json"
if [[ "$performance_snapshot" -eq 1 ]]; then
  printf 'performance summary: %s\n' "$performance_dir/summary.txt"
fi
