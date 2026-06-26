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
  --allocator-profile <system|glibc-low-dirty>
                        Runtime allocator env profile. Default: system.
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
allocator_profile="system"
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
    --allocator-profile)
      allocator_profile="${2:-}"
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
case "$allocator_profile" in
  system|glibc-low-dirty) ;;
  *)
    printf 'FAIL: --allocator-profile must be system or glibc-low-dirty\n' >&2
    exit 2
    ;;
esac
if [[ "$pacing_master" == "audio" && "$audio_clock_probe" -ne 1 ]]; then
  printf 'FAIL: --pacing-master audio requires --audio-clock-probe\n' >&2
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

if [[ "$no_build" -ne 1 ]]; then
  cargo build --release --features native-vulkan-video --bin gilder-native-vulkan
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
gilder_append_ready_prefix_runtime_env runtime_env "$allocator_profile"

performance_status=0
if [[ "$performance_snapshot" -eq 1 ]]; then
  if [[ ! -x scripts/performance-snapshot.sh ]]; then
    printf 'FAIL: missing executable scripts/performance-snapshot.sh\n' | tee "$summary"
    exit 1
  fi
  set +e
  env "${runtime_env[@]}" target/release/gilder-native-vulkan "${args[@]}" >"$runtime_json" 2>"$runtime_stderr" &
  runtime_pid=$!
  performance_args=(
    --pid "$runtime_pid"
    --label "native-vulkan-av1-ready-prefix-video"
    --duration "$performance_duration"
    --interval "$performance_interval"
    --output-dir "$performance_dir"
    --allow-missing
    --keep
  )
  if [[ -n "$max_private_dirty_kib_limit" ]]; then
    performance_args+=(--expect-max-private-dirty-kib-at-most "$max_private_dirty_kib_limit")
  fi
  scripts/performance-snapshot.sh "${performance_args[@]}" >"$performance_log" 2>&1
  performance_status=$?
  wait "$runtime_pid"
  runtime_status=$?
  set -e
else
  set +e
  env "${runtime_env[@]}" target/release/gilder-native-vulkan "${args[@]}" >"$runtime_json" 2>"$runtime_stderr"
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
max_private_dirty_kib="none"
avg_cpu_percent="none"
if [[ -s "$performance_dir/summary.txt" ]]; then
  max_private_dirty_kib="$(awk -F': ' '$1 == "max_private_dirty_kib" { print $2 }' "$performance_dir/summary.txt")"
  avg_cpu_percent="$(awk -F': ' '$1 == "avg_cpu_percent" { print $2 }' "$performance_dir/summary.txt")"
fi

if [[ -n "$av1_error" ]]; then
  printf 'FAIL: AV1 retained decode reported error: %s\n' "$av1_error" | tee "$summary"
  exit 1
fi
if [[ "$requested" -ne "$playback_frames" || "$presented" -ne "$playback_frames" || "$displayed" -ne "$playback_frames" ]]; then
  printf 'FAIL: AV1 presented/displayed count mismatch\n' | tee "$summary"
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
  printf 'allocator_profile: %s\n' "$allocator_profile"
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
  printf 'performance_snapshot: %s\n' "$([[ "$performance_snapshot" -eq 1 ]] && printf yes || printf no)"
  printf 'performance_max_private_dirty_kib_limit: %s\n' "${max_private_dirty_kib_limit:-none}"
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
