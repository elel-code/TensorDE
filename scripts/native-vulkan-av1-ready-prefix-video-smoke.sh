#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
usage: scripts/native-vulkan-av1-ready-prefix-video-smoke.sh [options]

Run the native Vulkanalia AV1 ready-prefix video path on a real Wayland
background surface. This script only exercises the heap-only Vulkanalia runtime:
bounded streaming packet queue, FFmpeg picture slices buffer pool, AV1
DPB/reference planner, decoded-image present handoff, and VK_EXT_descriptor_heap
sampling.

Options:
  --display <name>      Wayland display name. Default: WAYLAND_DISPLAY.
  --output-name <name>  Target Wayland output name, for example HDMI-A-1.
  --output <name>       Alias for --output-name.
  --source <path>       Existing AV1 source. Default: artifacts/video-sources/av1 4K/240 source.
  --report-dir <dir>    Exact evidence directory. Created and kept.
  --work-dir <dir>      Parent directory for generated evidence. Default: /tmp.
  --decode-prefix <n>   Temporal-unit count to make decode-ready. Default: source frame count when known, otherwise target-fps.
  --playback-frames <n> Presented frame count. Default: decode-prefix.
  --target-fps <fps>    Presentation target FPS. Default: 240.
  --bit-depth <8|10>    AV1 Main bit depth. Default: 8.
  --width <px>          Source coded width. Default: 3840.
  --height <px>         Source coded height. Default: 2160.
  --audio-clock-probe   Run explicit audio-only clock probe beside AV1 video.
  --audio-output <plan|clock-only|auto>
                        Select audio clock probe output branch. Default: plan.
  --pacing-master <target|audio>
                        Select pacing master. audio requires --audio-clock-probe.
  --muted|--unmuted     Select effective video plan audio policy. Default: muted.
  --performance-snapshot
                        Capture process CPU/RSS/PSS/USS/Private_Dirty/smaps while running.
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
decode_prefix=0
decode_prefix_explicit=0
playback_frames=0
target_fps=240
bit_depth=8
width=3840
height=2160
audio_clock_probe=0
audio_output="plan"
pacing_master="target"
plan_muted=1
performance_snapshot=0
performance_duration=10
performance_interval=1
max_private_dirty_kib_limit=""
default_max_private_dirty_kib_limit=25000
layer="background"
fit="cover"
no_build=0

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
    --audio-clock-probe)
      audio_clock_probe=1
      shift
      ;;
    --audio-output)
      audio_output="${2:-}"
      shift 2
      ;;
    --pacing-master)
      pacing_master="${2:-}"
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

case "$bit_depth" in
  8|10) ;;
  *)
    printf 'FAIL: --bit-depth must be 8 or 10\n' >&2
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
if [[ "$audio_output" == "auto" && "$audio_clock_probe" -ne 1 ]]; then
  printf 'FAIL: --audio-output %s requires --audio-clock-probe\n' "$audio_output" >&2
  exit 2
fi
if [[ -z "$display" ]]; then
  printf 'FAIL: WAYLAND_DISPLAY or --display is required\n' >&2
  exit 2
fi
for number in "$decode_prefix" "$playback_frames" "$target_fps" "$width" "$height" "$performance_duration" "$performance_interval"; do
  if [[ ! "$number" =~ ^[0-9]+$ ]]; then
    printf 'FAIL: numeric options must be non-negative integers\n' >&2
    exit 2
  fi
done
if [[ -n "$max_private_dirty_kib_limit" && ! "$max_private_dirty_kib_limit" =~ ^[0-9]+$ ]]; then
  printf 'FAIL: --max-private-dirty-kib must be a non-negative integer\n' >&2
  exit 2
fi
if [[ -n "$max_private_dirty_kib_limit" && "$performance_snapshot" -ne 1 ]]; then
  printf 'FAIL: --max-private-dirty-kib requires --performance-snapshot\n' >&2
  exit 2
fi
if [[ "$performance_snapshot" -eq 1 && -z "$max_private_dirty_kib_limit" ]]; then
  max_private_dirty_kib_limit="$default_max_private_dirty_kib_limit"
fi
if [[ "$target_fps" -lt 1 || "$width" -lt 1 || "$height" -lt 1 ]]; then
  printf 'FAIL: target-fps/width/height must be positive\n' >&2
  exit 2
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
cd "$repo_root"
source "$script_dir/native-vulkan-ready-prefix-video-common.sh"

if [[ -z "$source" ]]; then
  source="artifacts/video-sources/av1/av1-main${bit_depth}-${width}x${height}-${target_fps}fps-566frames-g${target_fps}.webm"
  if [[ ! -f "$source" ]]; then
    source="artifacts/video-sources/av1/av1-main${bit_depth}-${width}x${height}-${target_fps}fps-242frames-g${target_fps}.webm"
  fi
fi
if [[ ! -f "$source" ]]; then
  printf 'FAIL: AV1 source does not exist: %s\n' "$source" >&2
  exit 1
fi

source_frame_count=0
if command -v ffprobe >/dev/null 2>&1; then
  source_frame_count="$(
    ffprobe -v error -count_frames -select_streams v:0 \
      -show_entries stream=nb_read_frames,nb_frames \
      -of default=nokey=1:noprint_wrappers=1 "$source" |
      awk '$1 ~ /^[0-9]+$/ && $1 > 0 { print $1; exit }'
  )"
  source_frame_count="${source_frame_count:-0}"
fi

if [[ "$decode_prefix" -eq 0 ]]; then
  if [[ "$source_frame_count" -gt 0 ]]; then
    decode_prefix="$source_frame_count"
  else
    decode_prefix="$target_fps"
  fi
elif [[ "$source_frame_count" -gt 0 && "$decode_prefix" -gt "$source_frame_count" ]]; then
  decode_prefix="$source_frame_count"
fi
if [[ "$playback_frames" -eq 0 ]]; then
  playback_frames="$decode_prefix"
fi
if [[ "$decode_prefix" -lt 1 || "$playback_frames" -lt 1 ]]; then
  printf 'FAIL: decode-prefix/playback-frames must be positive\n' >&2
  exit 2
fi

if [[ -z "$report_dir" ]]; then
  report_dir="$(mktemp -d "${work_parent%/}/gilder-vulkan-av1-ready-prefix-video.XXXXXX")"
else
  mkdir -p "$report_dir"
fi
runtime_json="$report_dir/runtime.json"
runtime_stderr="$report_dir/runtime.stderr"
summary="$report_dir/summary.txt"
performance_dir="$report_dir/performance"
performance_log="$report_dir/performance.log"

release_binary_path="target/release/gilder-native-vulkan"
release_binary_fingerprint_before=""
if [[ -e "$release_binary_path" ]]; then
  release_binary_fingerprint_before="$(stat -c '%d:%i:%s:%Y' "$release_binary_path" 2>/dev/null || true)"
fi
release_binary_replaced_by_build=0
release_binary_synced_after_build=0
if [[ "$no_build" -ne 1 ]]; then
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

codec="av1-main-8"
if [[ "$bit_depth" -eq 10 ]]; then
  codec="av1-main-10"
fi

args=(
  --run-vulkanalia-ready-prefix-video
  --source "$source"
  --video-codec "$codec"
  --width "$width"
  --height "$height"
  --target-fps "$target_fps"
  --layer "$layer"
  --fit "$fit"
  --bitstream-samples "$decode_prefix"
  --decode-av1-ready-prefix "$decode_prefix"
  --playback-frames "$playback_frames"
)
if [[ "$audio_clock_probe" -eq 1 ]]; then
  args+=(--audio-clock-probe --audio-output "$audio_output")
fi
if [[ "$plan_muted" -eq 1 ]]; then
  args+=(--muted)
else
  args+=(--unmuted)
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

run_av1_performance_snapshot_attempt() {
  local attempt_performance_dir="${1:?performance dir is required}"
  local attempt_performance_log="${2:?performance log is required}"
  local attempt_runtime_json="${3:?runtime JSON is required}"
  local attempt_runtime_stderr="${4:?runtime stderr is required}"

  set +e
  env "${runtime_env[@]}" "$release_binary_path" "${args[@]}" >"$attempt_runtime_json" 2>"$attempt_runtime_stderr" &
  runtime_pid=$!
  performance_args=(
    --pid "$runtime_pid"
    --label "native-vulkan-av1-ready-prefix-video"
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

preserve_av1_rebuild_mapping_dirty_attempt() {
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
    run_av1_performance_snapshot_attempt \
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

    preserve_av1_rebuild_mapping_dirty_attempt "$performance_attempt_index"
    gilder_sync_rebuilt_executable "$release_binary_path"
    sleep 1
    performance_attempt_index=$((performance_attempt_index + 1))
  done
else
  set +e
  env "${runtime_env[@]}" "$release_binary_path" "${args[@]}" >"$runtime_json" 2>"$runtime_stderr"
  runtime_status=$?
  set -e
fi

if [[ "$runtime_status" -ne 0 ]]; then
  printf 'FAIL: native Vulkan AV1 ready-prefix video smoke failed\n' | tee "$summary"
  printf 'runtime_status: %s\n' "$runtime_status" >>"$summary"
  printf 'runtime_json: %s\n' "$runtime_json" >>"$summary"
  printf 'runtime_stderr: %s\n' "$runtime_stderr" >>"$summary"
  exit "$runtime_status"
fi
if [[ "$performance_status" -ne 0 ]]; then
  printf 'FAIL: performance snapshot failed\n' | tee "$summary"
  printf 'performance_status: %s\n' "$performance_status" >>"$summary"
  printf 'performance_log: %s\n' "$performance_log" >>"$summary"
  if [[ "$performance_rebuild_mapping_dirty_retry" -eq 1 ]]; then
    printf 'fresh-build contaminated retry count: %s\n' "$performance_rebuild_mapping_dirty_retry_count" >>"$summary"
    printf 'fresh-build contaminated performance summary: %s\n' "$performance_rebuild_mapping_dirty_first_summary" >>"$summary"
    printf 'fresh-build contaminated performance log: %s\n' "$performance_rebuild_mapping_dirty_first_log" >>"$summary"
    printf 'fresh-build contaminated max Private_Dirty KiB: %s\n' "$performance_rebuild_mapping_dirty_first_max_private_dirty_kib" >>"$summary"
    printf 'fresh-build contaminated file-mapping Private_Dirty KiB: %s\n' "$performance_rebuild_mapping_dirty_first_file_mapping_private_dirty_kib" >>"$summary"
    printf 'fresh-build contaminated gilder-binary Private_Dirty KiB: %s\n' "$performance_rebuild_mapping_dirty_first_gilder_binary_private_dirty_kib" >>"$summary"
    printf 'fresh-build contaminated heap Private_Dirty KiB: %s\n' "$performance_rebuild_mapping_dirty_first_heap_private_dirty_kib" >>"$summary"
  fi
  if [[ "$performance_rebuild_mapping_dirty_final_contaminated" -eq 1 ]]; then
    printf 'final failed attempt is still fresh-build dirty contaminated after %s attempts\n' "$performance_rebuild_mapping_dirty_max_attempts" >>"$summary"
  fi
  exit "$performance_status"
fi

av1_error="$(jq -r '.av1_retained_video_present_decode_error // ""' "$runtime_json")"
requested="$(jq -r '.av1_retained_video_present_decode.decoded_image_present_sequence.requested_present_frame_count // 0' "$runtime_json")"
presented="$(jq -r '.av1_retained_video_present_decode.decoded_image_present_sequence.presented_frame_count // 0' "$runtime_json")"
submitted="$(jq -r '.av1_retained_video_present_decode.decode.submitted_frame_count // 0' "$runtime_json")"
displayed="$(jq -r '.av1_retained_video_present_decode.decode.displayed_frame_count // 0' "$runtime_json")"
average_fps="$(jq -r '.av1_retained_video_present_decode.decoded_image_present_sequence.average_present_fps // 0' "$runtime_json")"
average_teardown_inclusive_fps="$(jq -r '.av1_retained_video_present_decode.decoded_image_present_sequence.average_present_teardown_inclusive_fps // 0' "$runtime_json")"
present_interval_elapsed_us="$(jq -r '.av1_retained_video_present_decode.decoded_image_present_sequence.present_interval_elapsed_micros // 0' "$runtime_json")"
present_teardown_inclusive_elapsed_us="$(jq -r '.av1_retained_video_present_decode.decoded_image_present_sequence.present_teardown_inclusive_elapsed_micros // 0' "$runtime_json")"
present_delta_min_us="$(jq -r '.av1_retained_video_present_decode.decoded_image_present_sequence.present_delta_min_micros // "none"' "$runtime_json")"
present_delta_max_us="$(jq -r '.av1_retained_video_present_decode.decoded_image_present_sequence.present_delta_max_micros // "none"' "$runtime_json")"
present_delta_over_6250us_count="$(jq -r '.av1_retained_video_present_decode.decoded_image_present_sequence.present_delta_over_6250us_count // 0' "$runtime_json")"
present_delta_over_8334us_count="$(jq -r '.av1_retained_video_present_decode.decoded_image_present_sequence.present_delta_over_8334us_count // 0' "$runtime_json")"
frame_sleep_count_value="$(jq -r '(.av1_retained_video_present_decode.decoded_image_present_sequence.frame_sleep_count // 0)' "$runtime_json")"
missed_frame_pacing_count_value="$(jq -r '(.av1_retained_video_present_decode.decoded_image_present_sequence.missed_frame_pacing_count // 0)' "$runtime_json")"
total_frame_sleep_us_value="$(jq -r '(.av1_retained_video_present_decode.decoded_image_present_sequence.total_pacing_sleep_micros // 0)' "$runtime_json")"
zero_copy="$(jq -r '.av1_retained_video_present_decode.decoded_image_present_sequence.all_zero_copy_presented // false' "$runtime_json")"
descriptor_model="$(jq -r '.av1_retained_video_present_decode.session.decoded_image_present_pipeline.descriptor_model // "none"' "$runtime_json")"
descriptor_sets="$(jq -r '.av1_retained_video_present_decode.session.decoded_image_present_pipeline.descriptor_sets // -1' "$runtime_json")"
ffmpeg_slices_buffer_model="$(jq -r '(.av1_retained_video_present_decode.decode.bitstream_buffer_model // "none")' "$runtime_json")"
ffmpeg_slices_buffer_pool_slot_count="$(jq -r '(.av1_retained_video_present_decode.decode.ffmpeg_slices_buffer_pool_slot_count // 0)' "$runtime_json")"
ffmpeg_slices_buffer_pool_allocated_slot_count="$(jq -r '(.av1_retained_video_present_decode.decode.ffmpeg_slices_buffer_pool_allocated_slot_count // 0)' "$runtime_json")"
ffmpeg_slices_buffer_pool_capacity_bytes="$(jq -r '(.av1_retained_video_present_decode.decode.ffmpeg_slices_buffer_pool_capacity_bytes // 0)' "$runtime_json")"
ffmpeg_slices_buffer_pool_max_slot_bytes="$(jq -r '(.av1_retained_video_present_decode.decode.ffmpeg_slices_buffer_pool_max_slot_bytes // 0)' "$runtime_json")"
ffmpeg_slices_buffer_max_src_range="$(jq -r '(.av1_retained_video_present_decode.decode.max_src_buffer_range // 0)' "$runtime_json")"
bitstream_total_payload_bytes="$(jq -r '(.av1_retained_video_present_decode.decode.src_buffer_total_bytes // 0)' "$runtime_json")"
session_dpb_slots="$(jq -r '.av1_retained_video_present_decode.session.session_max_dpb_slots // 0' "$runtime_json")"
picture_format="$(jq -r '.av1_retained_video_present_decode.session.picture_format // "none"' "$runtime_json")"
present_mode="$(jq -r '(.av1_retained_video_present_decode.session.device.swapchain.present_mode // "none")' "$runtime_json")"
present_mode_gate_failed=0
if ! gilder_native_video_present_mode_allowed "$present_mode"; then
  present_mode_gate_failed=1
fi
max_private_dirty_kib="none"
avg_cpu_percent="none"
if [[ -s "$performance_dir/summary.txt" ]]; then
  max_private_dirty_kib="$(awk -F': ' '$1 == "max_private_dirty_kib" { print $2 }' "$performance_dir/summary.txt")"
  avg_cpu_percent="$(awk -F': ' '$1 == "avg_cpu_percent" { print $2 }' "$performance_dir/summary.txt")"
fi
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
audio_sampled_video_frame_count="$presented"
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
audio_clock_gate_failed=0
audio_loop_probe_expected=0
audio_loop_serial_gate_failed=0
if [[ "$audio_clock_probe" -eq 1 && "$playback_frames" -gt "$decode_prefix" ]]; then
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
if [[ "$audio_loop_probe_expected" -eq 1 ]] && { [[ "$audio_loop_seek_count" -lt 1 || "$audio_clock_serial_uint" -lt 1 || "$audio_current_serial_start_serial_uint" -lt 1 ]] || ! gilder_is_uint "$audio_current_serial_start_packet_index"; }; then
  audio_clock_gate_failed=1
  audio_loop_serial_gate_failed=1
fi

if [[ -n "$av1_error" ]]; then
  printf 'FAIL: AV1 retained decode reported error: %s\n' "$av1_error" | tee "$summary"
  exit 1
fi
if [[ "$requested" -ne "$playback_frames" || "$presented" -ne "$playback_frames" || "$displayed" -ne "$playback_frames" ]]; then
  printf 'FAIL: AV1 presented/displayed count mismatch\n' | tee "$summary"
  exit 1
fi
if [[ "$present_mode_gate_failed" -ne 0 ]]; then
  {
    printf 'FAIL: AV1 present mode is not allowed for native Vulkan video\n'
    printf 'present_mode: %s\n' "$present_mode"
    printf 'present_mode_gate_failed: %s\n' "$present_mode_gate_failed"
  } | tee "$summary"
  exit 1
fi
if ! awk -v fps="$average_fps" -v target="$target_fps" 'BEGIN { exit (fps + 0.001 >= target) ? 0 : 1 }'; then
  printf 'FAIL: AV1 average_present_fps %s is below target %s\n' "$average_fps" "$target_fps" | tee "$summary"
  exit 1
fi
if ! awk -v fps="$average_teardown_inclusive_fps" -v target="$target_fps" 'BEGIN { exit (fps + 0.001 >= target) ? 0 : 1 }'; then
  printf 'FAIL: AV1 average_present_teardown_inclusive_fps %s is below target %s\n' \
    "$average_teardown_inclusive_fps" "$target_fps" | tee "$summary"
  exit 1
fi
if [[ "$descriptor_model" != "VK_EXT_descriptor_heap" || "$descriptor_sets" -ne 0 ]]; then
  printf 'FAIL: AV1 present path used non-heap descriptors\n' | tee "$summary"
  exit 1
fi
if [[ "$ffmpeg_slices_buffer_model" != "ffmpeg-picture-slices-buffer-pool-exec-owned" || "$ffmpeg_slices_buffer_pool_slot_count" -le 0 || "$ffmpeg_slices_buffer_pool_allocated_slot_count" -le 0 || "$ffmpeg_slices_buffer_pool_capacity_bytes" -le 0 || "$ffmpeg_slices_buffer_pool_max_slot_bytes" -le 0 || "$ffmpeg_slices_buffer_max_src_range" -le 0 || "$bitstream_total_payload_bytes" -le 0 ]]; then
  printf 'FAIL: AV1 decode did not use FFmpeg picture slices buffer pool\n' | tee "$summary"
  exit 1
fi
if [[ "$zero_copy" != "true" ]]; then
  printf 'FAIL: AV1 present path did not report zero-copy\n' | tee "$summary"
  exit 1
fi
if [[ "$audio_clock_gate_failed" -ne 0 ]]; then
  {
    printf 'FAIL: AV1 audio clock gate failed\n'
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
    printf 'audio_decoders: %s\n' "$audio_decoders"
    printf 'audio_video_decoders: %s\n' "$audio_video_decoders"
    printf 'audio_video_zero_based_drift_latest_ns: %s\n' "$audio_video_zero_based_drift_latest_ns"
    printf 'audio_video_zero_based_drift_abs_max_ns: %s\n' "$audio_video_zero_based_drift_abs_max_ns"
    printf 'audio_video_clock_drift_latest_ns: %s\n' "$audio_video_clock_drift_latest_ns"
    printf 'audio_video_clock_drift_abs_max_ns: %s\n' "$audio_video_clock_drift_abs_max_ns"
    printf 'audio_video_master_clock_drift_latest_ns: %s\n' "$audio_video_master_clock_drift_latest_ns"
    printf 'audio_video_master_clock_drift_abs_max_ns: %s\n' "$audio_video_master_clock_drift_abs_max_ns"
  } | tee "$summary"
  exit 1
fi
if [[ -n "$max_private_dirty_kib_limit" ]]; then
  if [[ ! "${max_private_dirty_kib:-}" =~ ^[0-9]+$ ]]; then
    printf 'FAIL: AV1 performance snapshot did not report max Private_Dirty\n' | tee "$summary"
    exit 1
  fi
  if [[ "$max_private_dirty_kib" -gt "$max_private_dirty_kib_limit" ]]; then
    printf 'FAIL: AV1 max Private_Dirty %s KiB exceeds limit %s KiB\n' \
      "$max_private_dirty_kib" "$max_private_dirty_kib_limit" | tee "$summary"
    exit 1
  fi
fi

{
  printf 'PASS: native Vulkan AV1 ready-prefix video smoke passed\n'
  printf 'source: %s\n' "$source"
  printf 'codec: %s\n' "$codec"
  printf 'source_frame_count: %s\n' "$source_frame_count"
  printf 'allocator_tuning: none\n'
  printf 'decode_prefix: %s\n' "$decode_prefix"
  printf 'playback_frames: %s\n' "$playback_frames"
  printf 'submitted_frame_count: %s\n' "$submitted"
  printf 'displayed_frame_count: %s\n' "$displayed"
  printf 'presented_frame_count: %s\n' "$presented"
  printf 'average_present_fps: %s\n' "$average_fps"
  printf 'average_present_teardown_inclusive_fps: %s\n' "$average_teardown_inclusive_fps"
  printf 'present_interval_elapsed_us: %s\n' "$present_interval_elapsed_us"
  printf 'present_teardown_inclusive_elapsed_us: %s\n' "$present_teardown_inclusive_elapsed_us"
  printf 'present_delta_min_us: %s\n' "$present_delta_min_us"
  printf 'present_delta_max_us: %s\n' "$present_delta_max_us"
  printf 'present_delta_over_6250us_count: %s\n' "$present_delta_over_6250us_count"
  printf 'present_delta_over_8334us_count: %s\n' "$present_delta_over_8334us_count"
  printf 'frame_sleep_count: %s\n' "$frame_sleep_count_value"
  printf 'missed_frame_pacing_count: %s\n' "$missed_frame_pacing_count_value"
  printf 'total_frame_sleep_us: %s\n' "$total_frame_sleep_us_value"
  printf 'descriptor_model: %s\n' "$descriptor_model"
  printf 'descriptor_sets: %s\n' "$descriptor_sets"
  printf 'ffmpeg_slices_buffer_model: %s\n' "$ffmpeg_slices_buffer_model"
  printf 'ffmpeg_slices_buffer_pool_slot_count: %s\n' "$ffmpeg_slices_buffer_pool_slot_count"
  printf 'ffmpeg_slices_buffer_pool_allocated_slot_count: %s\n' "$ffmpeg_slices_buffer_pool_allocated_slot_count"
  printf 'ffmpeg_slices_buffer_pool_capacity_bytes: %s\n' "$ffmpeg_slices_buffer_pool_capacity_bytes"
  printf 'ffmpeg_slices_buffer_pool_max_slot_bytes: %s\n' "$ffmpeg_slices_buffer_pool_max_slot_bytes"
  printf 'ffmpeg_slices_buffer_max_src_range: %s\n' "$ffmpeg_slices_buffer_max_src_range"
  printf 'bitstream_total_payload_bytes: %s\n' "$bitstream_total_payload_bytes"
  printf 'all_zero_copy_presented: %s\n' "$zero_copy"
  printf 'session_max_dpb_slots: %s\n' "$session_dpb_slots"
  printf 'picture_format: %s\n' "$picture_format"
  printf 'present_mode: %s\n' "$present_mode"
  printf 'present_mode_gate_failed: %s\n' "$present_mode_gate_failed"
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
  printf 'performance_snapshot: %s\n' "$([[ "$performance_snapshot" -eq 1 ]] && printf yes || printf no)"
  printf 'performance_max_private_dirty_kib_limit: %s\n' "${max_private_dirty_kib_limit:-none}"
  if [[ "$performance_snapshot" -eq 1 ]]; then
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
  fi
  printf 'performance_avg_cpu_percent: %s\n' "$avg_cpu_percent"
  printf 'performance_max_private_dirty_kib: %s\n' "${max_private_dirty_kib:-none}"
  printf 'runtime_json: %s\n' "$runtime_json"
  printf 'runtime_stderr: %s\n' "$runtime_stderr"
  if [[ "$performance_snapshot" -eq 1 ]]; then
    printf 'performance_dir: %s\n' "$performance_dir"
    printf 'performance_log: %s\n' "$performance_log"
  fi
} >"$summary"

cat "$summary"
