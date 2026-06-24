#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
usage: scripts/native-vulkan-h264-ready-prefix-video-smoke.sh [options]

Generate or use a 4K/240 H.264 High source, then run the native Vulkan direct
H.264 ready-prefix path on a real Wayland background surface. Each ready AU is
decoded with Vulkan Video into a sampled NV12 array layer and presented through
the native Vulkan swapchain. It does not use a GStreamer display sink.
By default, --playback-frames also expands the decoded ready prefix so the
generated source is a continuous 4K/240 stream comparable with the
GStreamer/appsink video source. Passing an explicit shorter --decode-prefix keeps
the old loop-window diagnostic mode.

Options:
  --display <name>      Wayland display name. Default: WAYLAND_DISPLAY.
  --output-name <name>  Target Wayland output name, for example HDMI-A-1.
  --output <name>       Alias for --output-name.
  --source <path>       Existing H.264 source. Default: generate continuous H.264 source.
  --report-dir <dir>    Exact evidence directory. Created and kept.
  --work-dir <dir>      Parent directory for generated evidence. Default: /tmp.
  --source-cache-dir <dir>
                        Persistent generated source cache. Default: artifacts/video-sources/h264.
  --decode-prefix <n>   Ready-prefix AU count to decode/present. Default:
                        playback-frames when playback-frames is set, otherwise target-fps.
  --playback-frames <n> Decode/present frames by looping the ready prefix. Default: decode-prefix.
  --streaming-queue    Compatibility no-op; bounded parser/appsink packet queue is always used.
  --target-fps <fps>    Presentation target FPS. Default: 240.
  --gop-size <frames>   Generated H.264 keyint/min-keyint. Default: target-fps.
  --refs <count>        Generated active reference frames. Default: 2.
  --bframes <count>     Generated B-frame count. Default: 0.
  --weightp <0|1|2>     Generated x264 P-frame weighted prediction mode. Default: 0.
  --weightb <0|1>       Generated x264 B-frame weighted prediction mode. Default: 0.
  --level <level>       Generated H.264 level. Default: 6.2.
  --width <px>          Generated/probed width. Default: 3840.
  --height <px>         Generated/probed height. Default: 2160.
  --frames <count>      Generated frame count. Default: decode-prefix + 2.
  --arbitrary-entry-offset <seconds>
                        Copy the source from a non-keyframe entry with -copyinkf,
                        then require streaming bootstrap to discard the broken
                        prefix and resume from the next decodable IDR.
  --require-loop-skip-replay
                        Require arbitrary-entry playback to cross EOS, seek,
                        skip the broken prefix again, and restart each loop on IDR.
  --audio-clock-probe  Run explicit AAC audio-only clock probe beside H.264 video
                        and gate clocked playback / no video decoder contamination.
  --pacing-master <target|audio>
                        Select pacing master. audio requires --audio-clock-probe.
  --allow-short-loop    Allow looped visible playback with a ready-prefix shorter than 1 second.
  --performance-snapshot
                        Capture process CPU/RSS/PSS/USS/Private_Dirty/smaps while the
                        native Vulkan process is running.
  --performance-duration <sec>
                        Performance sampling duration. Default: 10.
  --performance-interval <sec>
                        Performance sampling interval. Default: 1.
  --layer <layer>       Wayland layer. Default: background.
  --fit <mode>          Render fit. Default: cover.
  --no-build            Reuse existing target/release/gilder-native-vulkan.
  --keep                Compatibility no-op; evidence directories are always kept.
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
refs=2
bframes=0
weightp=0
weightb=0
level=6.2
width=3840
height=2160
frames=0
frames_explicit=0
arbitrary_entry_offset=""
arbitrary_entry_source=0
require_loop_skip_replay=0
audio_clock_probe=0
pacing_master="target"
allow_short_loop=0
layer="background"
fit="cover"
no_build=0
generated_source=0
source_duration_seconds=0
streaming_queue=1
performance_snapshot=0
performance_duration=10
performance_interval=1

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
    --streaming-queue)
      streaming_queue=1
      shift
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
    --weightp)
      weightp="${2:-}"
      shift 2
      ;;
    --weightb)
      weightb="${2:-}"
      shift 2
      ;;
    --level)
      level="${2:-}"
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
    --require-loop-skip-replay)
      require_loop_skip_replay=1
      shift
      ;;
    --audio-clock-probe)
      audio_clock_probe=1
      shift
      ;;
    --pacing-master)
      pacing_master="${2:-}"
      shift 2
      ;;
    --allow-short-loop)
      allow_short_loop=1
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

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
cd "$repo_root"
source "$script_dir/native-vulkan-ready-prefix-video-common.sh"
if [[ -z "$source_cache_dir" ]]; then
  source_cache_dir="$(gilder_default_source_cache_dir h264)"
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
if [[ "$decode_prefix" -lt 2 || "$playback_frames" -lt 0 || "$target_fps" -lt 1 || "$gop_size" -lt 2 || "$refs" -lt 1 || "$refs" -gt 16 || "$bframes" -lt 0 || "$bframes" -gt 16 || "$weightp" -lt 0 || "$weightp" -gt 2 || "$weightb" -lt 0 || "$weightb" -gt 1 || "$width" -lt 128 || "$height" -lt 128 ]]; then
  printf 'FAIL: decode-prefix/playback-frames/target-fps/gop-size/refs/bframes/weightp/weightb must be valid and width/height must be at least 128\n' >&2
  exit 1
fi
if (( width % 16 != 0 || height % 16 != 0 )); then
  printf 'FAIL: H.264 Vulkan Video source dimensions must be 16-pixel aligned; got %sx%s\n' "$width" "$height" >&2
  exit 1
fi
expected_frames="$decode_prefix"
if [[ "$playback_frames" -gt 0 ]]; then
  expected_frames="$playback_frames"
fi
ready_prefix_loop_period_ms=$((decode_prefix * 1000 / target_fps))
if [[ "$expected_frames" -gt "$decode_prefix" && "$decode_prefix" -lt "$target_fps" && "$allow_short_loop" -eq 0 ]]; then
  {
    printf 'FAIL: visible H.264 ready-prefix loop is too short for smoothness\n'
    printf 'decode_prefix: %s\n' "$decode_prefix"
    printf 'target_fps: %s\n' "$target_fps"
    printf 'ready_prefix_loop_period_ms: %s\n' "$ready_prefix_loop_period_ms"
    printf 'expected_playback_frames: %s\n' "$expected_frames"
    printf 'Pass --allow-short-loop only for deliberate short-loop diagnostics.\n'
  } >&2
  exit 1
fi

if [[ -z "$report_dir" ]]; then
  report_dir="$(mktemp -d "${work_parent%/}/gilder-vulkan-h264-ready-prefix-video.XXXXXX")"
else
  mkdir -p "$report_dir"
fi
mkdir -p "$report_dir"

if [[ "$no_build" -eq 0 ]]; then
  cargo build --release --features native-vulkan-gst-video --bin gilder-native-vulkan
fi

if [[ -z "$source" ]]; then
  generated_source=1
  generated_dir="$source_cache_dir"
  gilder_ensure_source_cache_dir "$generated_dir"
  if [[ "$frames" -eq 0 || "$frames" -lt $((decode_prefix + 2)) ]]; then
    frames=$((decode_prefix + 2))
  fi
  if [[ "$gop_size" -le "$decode_prefix" ]]; then
    gop_size=$((decode_prefix + 1))
  fi
  if [[ "$frames_explicit" -eq 0 && -n "$arbitrary_entry_offset" ]]; then
    offset_frames="$(awk -v offset="$arbitrary_entry_offset" -v fps="$target_fps" 'BEGIN { value = offset * fps; printf "%d", (value == int(value)) ? value : int(value) + 1 }')"
    arbitrary_window_frames="$expected_frames"
    if [[ "$require_loop_skip_replay" -eq 1 || "$expected_frames" -gt "$decode_prefix" ]]; then
      arbitrary_window_frames="$decode_prefix"
    fi
    arbitrary_min_frames=$((offset_frames + gop_size + arbitrary_window_frames + 2))
    if [[ "$frames" -lt "$arbitrary_min_frames" ]]; then
      frames="$arbitrary_min_frames"
    fi
  fi
  source_duration_seconds=$(( (frames + target_fps - 1) / target_fps ))
  source="$generated_dir/h264-high-b${bframes}-ref${refs}-weightp${weightp}-weightb${weightb}-${width}x${height}-${target_fps}fps-${frames}frames-g${gop_size}-d${decode_prefix}.mp4"
  if [[ ! -s "$source" ]]; then
    ffmpeg -hide_banner -loglevel error -y \
      -f lavfi -i "testsrc2=size=${width}x${height}:rate=${target_fps}:duration=${source_duration_seconds}" \
      -frames:v "$frames" -an \
      -c:v libx264 -profile:v high -level:v "$level" -preset veryfast -tune zerolatency -pix_fmt yuv420p \
      -x264-params "keyint=${gop_size}:min-keyint=${gop_size}:scenecut=0:open-gop=0:bframes=${bframes}:b-adapt=0:ref=${refs}:repeat-headers=1:cabac=1:8x8dct=1:weightp=${weightp}:weightb=${weightb}" \
      "$source"
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
if [[ "$arbitrary_entry_source" -eq 1 && "$expected_frames" -gt "$decode_prefix" ]]; then
  require_loop_skip_replay=1
fi

runtime_json="$report_dir/runtime.json"
runtime_stderr="$report_dir/runtime.stderr"
summary="$report_dir/summary.txt"
performance_dir="$report_dir/performance"
performance_log="$report_dir/performance.log"
args=(
  --run-h264-ready-prefix-video
  --source "$source"
  --width "$width"
  --height "$height"
  --target-fps "$target_fps"
  --layer "$layer"
  --fit "$fit"
  --bitstream-samples "$decode_prefix"
  --decode-h264-ready-prefix "$decode_prefix"
)
if [[ "$playback_frames" -gt 0 ]]; then
  args+=(--playback-frames "$playback_frames")
fi
if [[ "$streaming_queue" -eq 1 ]]; then
  args+=(--h264-input streaming-queue)
fi
if [[ "$audio_clock_probe" -eq 1 ]]; then
  args+=(--audio-clock-probe)
fi
if [[ -n "$output_name" ]]; then
  args+=(--output-name "$output_name")
fi

runtime_env=(WAYLAND_DISPLAY="$display")
if [[ "$pacing_master" == "audio" ]]; then
  runtime_env+=(GILDER_VIDEO_PACING_MASTER=audio)
fi

performance_status=0
if [[ "$performance_snapshot" -eq 1 ]]; then
  if [[ ! -x scripts/performance-snapshot.sh ]]; then
    printf 'FAIL: missing executable scripts/performance-snapshot.sh\n' | tee "$summary"
    exit 1
  fi
  set +e
  env "${runtime_env[@]}" \
    target/release/gilder-native-vulkan \
    "${args[@]}" \
    >"$runtime_json" 2>"$runtime_stderr" &
  runtime_pid=$!
  scripts/performance-snapshot.sh \
    --pid "$runtime_pid" \
    --label "native-vulkan-h264-ready-prefix-video" \
    --duration "$performance_duration" \
    --interval "$performance_interval" \
    --output-dir "$performance_dir" \
    --allow-missing \
    --keep \
    >"$performance_log" 2>&1
  performance_status=$?
  wait "$runtime_pid"
  runtime_status=$?
  set -e
else
  set +e
  env "${runtime_env[@]}" \
    target/release/gilder-native-vulkan \
    "${args[@]}" \
    >"$runtime_json" 2>"$runtime_stderr"
  runtime_status=$?
  set -e
fi

if [[ "$runtime_status" -ne 0 ]]; then
  printf 'FAIL: native Vulkan direct H.264 ready-prefix video smoke failed\n' | tee "$summary"
  printf 'source: %s\n' "$source" | tee -a "$summary"
  printf 'stderr: %s\n' "$runtime_stderr" | tee -a "$summary"
  sed -n '1,160p' "$runtime_stderr" >&2
  exit "$runtime_status"
fi
if [[ "$performance_snapshot" -eq 1 && "$performance_status" -ne 0 ]]; then
  printf 'FAIL: native Vulkan direct H.264 performance snapshot failed\n' | tee "$summary"
  printf 'source: %s\n' "$source" | tee -a "$summary"
  printf 'performance log: %s\n' "$performance_log" | tee -a "$summary"
  sed -n '1,200p' "$performance_log" >&2
  exit "$performance_status"
fi

decoded_count="$(jq -r '.decoded_frame_count // 0' "$runtime_json")"
presented_count="$(jq -r '.presented_frame_count // 0' "$runtime_json")"
frame_count="$(jq -r '(.frames // []) | length' "$runtime_json")"
bad_frames="$(jq -r '[.frames[]? | select(.decode_elapsed_us <= 0 or .present_elapsed_us <= 0)] | length' "$runtime_json")"
average_present_fps="$(jq -r '.average_present_fps // 0' "$runtime_json")"
average_present_result_fps="$(jq -r '.average_present_result_fps // 0' "$runtime_json")"
average_present_result_drop_first_fps="$(jq -r '.average_present_result_drop_first_fps // 0' "$runtime_json")"
average_present_result_drop_first_60_fps="$(jq -r '.average_present_result_drop_first_60_fps // 0' "$runtime_json")"
present_result_first_interval_us="$(jq -r '.present_result_first_interval_us // 0' "$runtime_json")"
present_result_max_interval_us="$(jq -r '.present_result_max_interval_us // 0' "$runtime_json")"
present_result_max_interval_after_warmup_us="$(jq -r '.present_result_max_interval_after_warmup_us // 0' "$runtime_json")"
present_result_over_budget_count="$(jq -r '.present_result_over_budget_count // 0' "$runtime_json")"
present_result_over_budget_after_warmup_count="$(jq -r '.present_result_over_budget_after_warmup_count // 0' "$runtime_json")"
present_result_missed_vblank_threshold_us="$(jq -r '.present_result_missed_vblank_threshold_us // 0' "$runtime_json")"
present_result_missed_vblank_count="$(jq -r '.present_result_missed_vblank_count // 0' "$runtime_json")"
present_result_missed_vblank_after_warmup_count="$(jq -r '.present_result_missed_vblank_after_warmup_count // 0' "$runtime_json")"
present_budget_us=$(((1000000 + target_fps - 1) / target_fps))
acquire_over_budget_count="$(jq -r --argjson budget "$present_budget_us" '[.frames[]?.acquire_elapsed_us // 0 | select(. > $budget)] | length' "$runtime_json")"
queue_present_over_budget_count="$(jq -r --argjson budget "$present_budget_us" '[.frames[]?.queue_present_elapsed_us // 0 | select(. > $budget)] | length' "$runtime_json")"
present_over_budget_count="$(jq -r --argjson budget "$present_budget_us" '[.frames[]?.present_elapsed_us // 0 | select(. > $budget)] | length' "$runtime_json")"
distinct_layers="$(jq -r '[.frames[]?.dst_base_array_layer] | unique | length' "$runtime_json")"
ready_prefix_count="$(jq -r '.ready_prefix_frame_count // 0' "$runtime_json")"
requested_playback_count="$(jq -r '.requested_playback_frame_count // 0' "$runtime_json")"
playback_loop_count="$(jq -r '.playback_loop_count // 0' "$runtime_json")"
loop_boundary_reset_count="$(jq -r '.loop_boundary_reset_count // 0' "$runtime_json")"
pts_delta_min="$(jq -r '.pts_delta_min_ms // "none"' "$runtime_json")"
pts_delta_max="$(jq -r '.pts_delta_max_ms // "none"' "$runtime_json")"
pts_delta_expected_min="$(jq -r '.pts_delta_expected_min_ms // "none"' "$runtime_json")"
pts_delta_expected_max="$(jq -r '.pts_delta_expected_max_ms // "none"' "$runtime_json")"
pts_delta_in_expected_range="$(jq -r '.pts_delta_in_expected_range // "none"' "$runtime_json")"
read -r script_pts_delta_expected_min script_pts_delta_expected_max < <(gilder_pts_delta_expected_bounds_ms "$target_fps")
pts_delta_script_in_expected_range=false
if gilder_pts_delta_in_expected_range "$pts_delta_min" "$pts_delta_max" "$target_fps"; then
  pts_delta_script_in_expected_range=true
fi
present_queue="$(jq -r '.present_queue_family_index // "none"' "$runtime_json")"
video_queue="$(jq -r '.video_decode_queue_family_index // "none"' "$runtime_json")"
sync_strategy="$(jq -r '.cross_queue_sync_strategy // "none"' "$runtime_json")"
driver_max_dpb_slots="$(jq -r '.driver_max_dpb_slots // "none"' "$runtime_json")"
stream_sps_dpb_slots="$(jq -r '.stream_sps_dpb_slots // 0' "$runtime_json")"
stream_dpb_slots="$(jq -r '.stream_dpb_slots // 0' "$runtime_json")"
stream_max_active_reference_pictures="$(jq -r '.stream_max_active_reference_pictures // 0' "$runtime_json")"
session_max_dpb_slots="$(jq -r '.session_max_dpb_slots // 0' "$runtime_json")"
session_max_active_reference_pictures="$(jq -r '.session_max_active_reference_pictures // 0' "$runtime_json")"
h264_picture_layout="$(jq -r '.h264_picture_layout // "none"' "$runtime_json")"
h264_stream_profile="$(jq -r '.h264_stream_profile // "none"' "$runtime_json")"
h264_stream_profile_idc="$(jq -r '.h264_stream_profile_idc // "none"' "$runtime_json")"
h264_vulkan_std_profile_idc="$(jq -r '.h264_vulkan_std_profile_idc // "none"' "$runtime_json")"
present_mode="$(jq -r '.present_mode // "none"' "$runtime_json")"
pacing_strategy="$(jq -r '.pacing_strategy // "none"' "$runtime_json")"
expected_pacing_strategy="$(gilder_expected_pacing_strategy_with_master "$present_mode" "$target_fps" "$pacing_master")"
frame_sleep_count_value="$(jq -r '.frame_sleep_count // 0' "$runtime_json")"
bitstream_strategy="$(jq -r '.bitstream_buffer_strategy // "none"' "$runtime_json")"
bitstream_slot_count="$(jq -r '.bitstream_buffer_slot_count // 0' "$runtime_json")"
bitstream_slot_bytes="$(jq -r '.bitstream_buffer_slot_bytes // 0' "$runtime_json")"
bitstream_ring_capacity_bytes="$(jq -r '.bitstream_ring_capacity_bytes // 0' "$runtime_json")"
bitstream_ring_wrap_count="$(jq -r '.bitstream_ring_wrap_count // 0' "$runtime_json")"
bitstream_window_payload_bytes="$(jq -r '.bitstream_window_payload_bytes // 0' "$runtime_json")"
bitstream_upload_count="$(jq -r '.bitstream_upload_count // 0' "$runtime_json")"
bitstream_uploaded_bytes="$(jq -r '.bitstream_uploaded_bytes // 0' "$runtime_json")"
h264_input_mode="$(jq -r '.h264_input_mode // "none"' "$runtime_json")"
h264_display_handoff_strategy="$(jq -r '.h264_display_handoff_strategy // "none"' "$runtime_json")"
h264_resource_image_layout="$(jq -r '.h264_resource_image_layout // "none"' "$runtime_json")"
h264_video_queue_sync_strategy="$(jq -r '.h264_video_queue_sync_strategy // "none"' "$runtime_json")"
h264_present_frame_preroll_count="$(jq -r '.h264_present_frame_preroll_count // 0' "$runtime_json")"
h264_present_queue_count="$(jq -r '.h264_present_queue_count // 0' "$runtime_json")"
h264_async_present_depth="$(jq -r '.h264_async_present_depth // 0' "$runtime_json")"
h264_present_result_wait_count="$(jq -r '.h264_present_result_wait_count // 0' "$runtime_json")"
h264_present_result_wait_elapsed_us="$(jq -r '.h264_present_result_wait_elapsed_us // 0' "$runtime_json")"
h264_present_result_wait_max_us="$(jq -r '.h264_present_result_wait_max_us // 0' "$runtime_json")"
h264_acquire_not_ready_count="$(jq -r '.h264_acquire_not_ready_count // 0' "$runtime_json")"
h264_acquire_wait_present_result_count="$(jq -r '.h264_acquire_wait_present_result_count // 0' "$runtime_json")"
h264_acquire_wait_present_result_elapsed_us="$(jq -r '.h264_acquire_wait_present_result_elapsed_us // 0' "$runtime_json")"
h264_acquire_wait_present_result_max_us="$(jq -r '.h264_acquire_wait_present_result_max_us // 0' "$runtime_json")"
h264_display_ring_slot_count="$(jq -r '.h264_display_ring_slot_count // 0' "$runtime_json")"
h264_display_ring_memory_bytes="$(jq -r '.h264_display_ring_memory_bytes // 0' "$runtime_json")"
h264_display_copy_count="$(jq -r '.h264_display_copy_count // 0' "$runtime_json")"
h264_display_copy_record_elapsed_us="$(jq -r '.h264_display_copy_record_elapsed_us // 0' "$runtime_json")"
h264_display_copy_submit_elapsed_us="$(jq -r '.h264_display_copy_submit_elapsed_us // 0' "$runtime_json")"
h264_packet_queue_capacity="$(jq -r '.h264_packet_queue_capacity // 0' "$runtime_json")"
h264_packet_queue_pulled_count="$(jq -r '.h264_packet_queue_pulled_count // 0' "$runtime_json")"
h264_packet_queue_eos_count="$(jq -r '.h264_packet_queue_eos_count // 0' "$runtime_json")"
h264_packet_queue_loop_count="$(jq -r '.h264_packet_queue_loop_count // 0' "$runtime_json")"
h264_packet_queue_loop_skip_access_units="$(jq -r '.h264_packet_queue_loop_skip_access_units // 0' "$runtime_json")"
h264_packet_queue_bootstrap_discarded_access_units="$(jq -r '.h264_packet_queue_bootstrap_discarded_access_units // 0' "$runtime_json")"
h264_packet_queue_max_payload_bytes="$(jq -r '.h264_packet_queue_max_payload_bytes // 0' "$runtime_json")"
h264_decode_ahead_strategy="$(jq -r '.h264_decode_ahead_strategy // "none"' "$runtime_json")"
h264_decode_ahead_attempt_count="$(jq -r '.h264_decode_ahead_attempt_count // 0' "$runtime_json")"
h264_decode_ahead_submit_count="$(jq -r '.h264_decode_ahead_submit_count // 0' "$runtime_json")"
h264_decode_ahead_skip_unready_count="$(jq -r '.h264_decode_ahead_skip_unready_count // 0' "$runtime_json")"
h264_decode_ahead_skip_output_hazard_count="$(jq -r '.h264_decode_ahead_skip_output_hazard_count // 0' "$runtime_json")"
h264_decode_ahead_skip_reference_hazard_count="$(jq -r '.h264_decode_ahead_skip_reference_hazard_count // 0' "$runtime_json")"
h264_decode_ahead_skip_bitstream_overlap_count="$(jq -r '.h264_decode_ahead_skip_bitstream_overlap_count // 0' "$runtime_json")"
h264_decode_ahead_copy_wait_output_hazard_count="$(jq -r '.h264_decode_ahead_copy_wait_output_hazard_count // 0' "$runtime_json")"
h264_decode_ahead_copy_wait_reference_hazard_count="$(jq -r '.h264_decode_ahead_copy_wait_reference_hazard_count // 0' "$runtime_json")"
audio_clock_probe_present="$(jq -r '(.audio_clock_probe != null)' "$runtime_json")"
audio_reached_clocked_playback="$(jq -r '.audio_clock_probe.reached_clocked_playback // false' "$runtime_json")"
audio_no_video_decoder_instantiated="$(jq -r '.audio_clock_probe.no_video_decoder_instantiated // false' "$runtime_json")"
audio_buffer_count="$(jq -r '.audio_clock_probe.audio_buffer_count // 0' "$runtime_json")"
audio_loop_seek_count="$(jq -r '.audio_clock_probe.audio_loop_seek_count // 0' "$runtime_json")"
audio_loop_seek_error_count="$(jq -r '.audio_clock_probe.audio_loop_seek_error_count // 0' "$runtime_json")"
audio_loop_restart_count="$(jq -r '.audio_clock_probe.audio_loop_restart_count // 0' "$runtime_json")"
audio_last_loop_seek_position_ms="$(jq -r '.audio_clock_probe.audio_last_loop_seek_position_ms // "none"' "$runtime_json")"
audio_playback_started="$(jq -r '.audio_clock_probe.audio_playback_started // false' "$runtime_json")"
audio_clock_serial="$(jq -r '.audio_clock_probe.audio_clock_serial // 0' "$runtime_json")"
audio_initial_position_ms="$(jq -r '.audio_clock_probe.audio_initial_position_ms // "none"' "$runtime_json")"
audio_segment_start_position_ns="$(jq -r '.audio_clock_probe.audio_segment_start_position_ns // "none"' "$runtime_json")"
audio_segment_elapsed_ns="$(jq -r '.audio_clock_probe.audio_segment_elapsed_ns // "none"' "$runtime_json")"
audio_position_stale_count="$(jq -r '.audio_clock_probe.audio_position_stale_count // 0' "$runtime_json")"
audio_sample_stale_count="$(jq -r '.audio_clock_probe.audio_sample_stale_count // 0' "$runtime_json")"
audio_master_clock_estimate_ns="$(jq -r '.audio_clock_probe.audio_master_clock_estimate_ns // "none"' "$runtime_json")"
audio_position_query_count="$(jq -r '.audio_clock_probe.audio_position_query_count // 0' "$runtime_json")"
audio_position_query_hit_count="$(jq -r '.audio_clock_probe.audio_position_query_hit_count // 0' "$runtime_json")"
audio_sampled_video_frame_count="$(jq -r '.audio_clock_probe.sampled_video_frame_count // 0' "$runtime_json")"
audio_sample_rate="$(jq -r '.audio_clock_probe.audio_sample_rate // "none"' "$runtime_json")"
audio_channels="$(jq -r '.audio_clock_probe.audio_channels // "none"' "$runtime_json")"
audio_decoders="$(jq -c '.audio_clock_probe.audio_decoders // []' "$runtime_json")"
audio_video_decoders="$(jq -c '.audio_clock_probe.video_decoders // []' "$runtime_json")"
audio_video_zero_based_drift_latest_ns="$(jq -r '.audio_clock_probe.audio_video_zero_based_drift_latest_ns // "none"' "$runtime_json")"
audio_video_zero_based_drift_abs_max_ns="$(jq -r '.audio_clock_probe.audio_video_zero_based_drift_abs_max_ns // "none"' "$runtime_json")"
audio_video_clock_drift_latest_ns="$(jq -r '.audio_clock_probe.audio_video_clock_drift_latest_ns // "none"' "$runtime_json")"
audio_video_clock_drift_abs_max_ns="$(jq -r '.audio_clock_probe.audio_video_clock_drift_abs_max_ns // "none"' "$runtime_json")"
audio_video_master_clock_drift_latest_ns="$(jq -r '.audio_clock_probe.audio_video_master_clock_drift_latest_ns // "none"' "$runtime_json")"
audio_video_master_clock_drift_abs_max_ns="$(jq -r '.audio_clock_probe.audio_video_master_clock_drift_abs_max_ns // "none"' "$runtime_json")"
first_frame_idr="$(jq -r '.frames[0].idr // false' "$runtime_json")"
loop_first_non_idr_count="$(jq -r 'reduce (.frames // [])[] as $frame ({}; ($frame.playback_loop_index | tostring) as $loop | if has($loop) then . else .[$loop] = ($frame.idr == true) end) | [to_entries[] | select(.value != true)] | length' "$runtime_json")"
first_frame_recovery="$(jq -r '(.frames[0].reset_before_decode == true) and (.frames[0].idr == true)' "$runtime_json")"
loop_first_unrecovered_count="$(jq -r 'reduce (.frames // [])[] as $frame ({}; ($frame.playback_loop_index | tostring) as $loop | if has($loop) then . else .[$loop] = (($frame.reset_before_decode == true) and ($frame.idr == true)) end) | [to_entries[] | select(.value != true)] | length' "$runtime_json")"
swapchain_images="$(jq -r '.swapchain_image_count // 0' "$runtime_json")"
resource_bytes="$(jq -r '.video_resource_memory_bytes // 0' "$runtime_json")"
idr_frames="$(jq -r '[.frames[]? | select(.idr == true)] | length' "$runtime_json")"
p_frames="$(jq -r '[.frames[]? | select(.slice_type == 0 or .slice_type == 5)] | length' "$runtime_json")"
b_frames="$(jq -r '[.frames[]? | select(.slice_type == 1 or .slice_type == 6)] | length' "$runtime_json")"
max_requested_reference_count="$(jq -r '[.frames[]? | .requested_reference_count] | max // 0' "$runtime_json")"
max_reference_count="$(jq -r '[.frames[]? | .decode_reference_slot_count] | max // 0' "$runtime_json")"
reference_gate_failed=0
if [[ "$generated_source" -eq 1 && "$decode_prefix" -gt "$refs" && ( "$idr_frames" -lt 1 || "$p_frames" -lt 1 || "$max_requested_reference_count" -lt "$refs" || "$max_reference_count" -lt "$refs" ) ]]; then
  reference_gate_failed=1
fi
b_frame_gate_failed=0
if [[ "$generated_source" -eq 1 && "$bframes" -gt 0 && "$b_frames" -lt 1 ]]; then
  b_frame_gate_failed=1
fi
loop_gate_failed=0
if [[ "$expected_frames" -gt "$decode_prefix" && ( "$playback_loop_count" -le 1 || "$loop_boundary_reset_count" -lt 1 ) ]]; then
  loop_gate_failed=1
fi
bitstream_gate_failed=0
if [[ "$bitstream_strategy" != "fixed-capacity-persistent-mapped-ring" || "$bitstream_slot_count" -le 0 || "$bitstream_slot_bytes" -le 0 || "$bitstream_ring_capacity_bytes" -lt "$bitstream_slot_bytes" || "$bitstream_window_payload_bytes" -le 0 || "$bitstream_upload_count" -ne "$expected_frames" || "$bitstream_uploaded_bytes" -le 0 ]]; then
  bitstream_gate_failed=1
fi
input_gate_failed=0
if [[ "$h264_input_mode" != "streaming-queue" || "$h264_packet_queue_capacity" -le 0 || "$h264_packet_queue_pulled_count" -lt "$expected_frames" || "$h264_packet_queue_max_payload_bytes" -le 0 ]]; then
  input_gate_failed=1
fi
arbitrary_entry_gate_failed=0
if [[ "$arbitrary_entry_source" -eq 1 && ( "$h264_packet_queue_bootstrap_discarded_access_units" -le 0 || "$h264_packet_queue_loop_skip_access_units" -le 0 || "$first_frame_recovery" != "true" ) ]]; then
  arbitrary_entry_gate_failed=1
fi
loop_skip_replay_gate_failed=0
if [[ "$require_loop_skip_replay" -eq 1 && ( "$h264_packet_queue_eos_count" -le 0 || "$h264_packet_queue_loop_count" -le 0 || "$playback_loop_count" -le 1 || "$loop_boundary_reset_count" -le 0 || "$h264_packet_queue_bootstrap_discarded_access_units" -le 0 || "$h264_packet_queue_loop_skip_access_units" -le 0 || "$first_frame_recovery" != "true" || "$loop_first_unrecovered_count" -ne 0 ) ]]; then
  loop_skip_replay_gate_failed=1
fi
if [[ "$decode_prefix" -gt 1 && ( "$bitstream_slot_count" -le 1 || "$bitstream_ring_capacity_bytes" -le "$bitstream_slot_bytes" ) ]]; then
  bitstream_gate_failed=1
fi
if [[ "$decode_prefix" -gt 2 && "$bitstream_slot_count" -ge "$decode_prefix" ]]; then
  bitstream_gate_failed=1
fi
pacing_gate_failed=0
if [[ "$pacing_strategy" != "$expected_pacing_strategy" ]]; then
  pacing_gate_failed=1
fi
dpb_gate_failed=0
if [[ "$driver_max_dpb_slots" == "none" || "$stream_sps_dpb_slots" -le 0 || "$stream_dpb_slots" -le 0 || "$session_max_dpb_slots" -ne "$stream_dpb_slots" || "$session_max_active_reference_pictures" -le 0 || "$session_max_active_reference_pictures" -gt "$session_max_dpb_slots" || "$session_max_active_reference_pictures" -lt "$stream_max_active_reference_pictures" || "$distinct_layers" -gt "$session_max_dpb_slots" ]]; then
  dpb_gate_failed=1
fi
pts_delta_gate_failed=0
if [[ "$pts_delta_in_expected_range" != "true" || "$pts_delta_script_in_expected_range" != "true" || "$pts_delta_expected_min" != "$script_pts_delta_expected_min" || "$pts_delta_expected_max" != "$script_pts_delta_expected_max" ]]; then
  pts_delta_gate_failed=1
fi
audio_clock_gate_failed=0
if [[ "$audio_clock_probe" -eq 1 && ( "$audio_clock_probe_present" != "true" || "$audio_reached_clocked_playback" != "true" || "$audio_no_video_decoder_instantiated" != "true" || "$audio_playback_started" != "true" || "$audio_clock_serial" -lt 1 || "$audio_buffer_count" -le 0 || "$audio_position_query_count" -le 0 || "$audio_position_query_hit_count" -le 0 || "$audio_sampled_video_frame_count" -le 0 || "$audio_master_clock_estimate_ns" == "none" || "$audio_video_master_clock_drift_latest_ns" == "none" || "$audio_loop_seek_error_count" -ne 0 ) ]]; then
  audio_clock_gate_failed=1
fi
if [[ "$audio_clock_probe" -eq 1 && "$loop_boundary_reset_count" -gt 0 && "$audio_loop_seek_count" -lt "$loop_boundary_reset_count" ]]; then
  audio_clock_gate_failed=1
fi

if [[ "$decoded_count" -ne "$expected_frames" || "$presented_count" -ne "$expected_frames" || "$frame_count" -ne "$expected_frames" || "$ready_prefix_count" -ne "$decode_prefix" || "$requested_playback_count" -ne "$expected_frames" || "$bad_frames" -ne 0 || "$distinct_layers" -le 1 || "$reference_gate_failed" -ne 0 || "$b_frame_gate_failed" -ne 0 || "$loop_gate_failed" -ne 0 || "$bitstream_gate_failed" -ne 0 || "$input_gate_failed" -ne 0 || "$arbitrary_entry_gate_failed" -ne 0 || "$loop_skip_replay_gate_failed" -ne 0 || "$pacing_gate_failed" -ne 0 || "$dpb_gate_failed" -ne 0 || "$pts_delta_gate_failed" -ne 0 || "$audio_clock_gate_failed" -ne 0 || "$present_queue" == "none" || "$video_queue" == "none" || "$sync_strategy" != "per-frame-binary-semaphore-decode-signal-present-wait" || "$swapchain_images" -lt 2 || "$resource_bytes" -le 0 ]]; then
  {
    printf 'FAIL: native Vulkan direct H.264 ready-prefix video output was not valid\n'
    printf 'decoded_count: %s\n' "$decoded_count"
    printf 'presented_count: %s\n' "$presented_count"
    printf 'requested_decode_prefix: %s\n' "$decode_prefix"
    printf 'expected_playback_frames: %s\n' "$expected_frames"
    printf 'ready_prefix_loop_period_ms: %s\n' "$ready_prefix_loop_period_ms"
    printf 'frame_count: %s\n' "$frame_count"
    printf 'ready_prefix_frame_count: %s\n' "$ready_prefix_count"
    printf 'requested_playback_frame_count: %s\n' "$requested_playback_count"
    printf 'playback_loop_count: %s\n' "$playback_loop_count"
    printf 'loop_boundary_reset_count: %s\n' "$loop_boundary_reset_count"
    printf 'bad_frames: %s\n' "$bad_frames"
    printf 'distinct_layers: %s\n' "$distinct_layers"
    printf 'idr_frames: %s\n' "$idr_frames"
    printf 'p_frames: %s\n' "$p_frames"
    printf 'b_frames: %s\n' "$b_frames"
    printf 'generated_refs: %s\n' "$([[ "$generated_source" -eq 1 ]] && printf '%s' "$refs" || printf none)"
    printf 'max_requested_reference_count: %s\n' "$max_requested_reference_count"
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
    printf 'audio_clock_gate_failed: %s\n' "$audio_clock_gate_failed"
    printf 'audio_reached_clocked_playback: %s\n' "$audio_reached_clocked_playback"
    printf 'audio_no_video_decoder_instantiated: %s\n' "$audio_no_video_decoder_instantiated"
    printf 'audio_buffer_count: %s\n' "$audio_buffer_count"
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
    printf 'driver_max_dpb_slots: %s\n' "$driver_max_dpb_slots"
    printf 'stream_sps_dpb_slots: %s\n' "$stream_sps_dpb_slots"
    printf 'stream_dpb_slots: %s\n' "$stream_dpb_slots"
    printf 'stream_max_active_reference_pictures: %s\n' "$stream_max_active_reference_pictures"
    printf 'session_max_dpb_slots: %s\n' "$session_max_dpb_slots"
    printf 'session_max_active_reference_pictures: %s\n' "$session_max_active_reference_pictures"
    printf 'h264_picture_layout: %s\n' "$h264_picture_layout"
    printf 'h264_stream_profile: %s\n' "$h264_stream_profile"
    printf 'h264_stream_profile_idc: %s\n' "$h264_stream_profile_idc"
    printf 'h264_vulkan_std_profile_idc: %s\n' "$h264_vulkan_std_profile_idc"
    printf 'present_mode: %s\n' "$present_mode"
    printf 'pacing_master: %s\n' "$pacing_master"
    printf 'pacing_strategy: %s\n' "$pacing_strategy"
    printf 'expected_pacing_strategy: %s\n' "$expected_pacing_strategy"
    printf 'frame_sleep_count: %s\n' "$frame_sleep_count_value"
    printf 'average_present_fps: %s\n' "$average_present_fps"
    printf 'average_present_result_fps: %s\n' "$average_present_result_fps"
    printf 'average_present_result_drop_first_fps: %s\n' "$average_present_result_drop_first_fps"
    printf 'average_present_result_drop_first_60_fps: %s\n' "$average_present_result_drop_first_60_fps"
    printf 'present_result_first_interval_us: %s\n' "$present_result_first_interval_us"
    printf 'present_result_max_interval_us: %s\n' "$present_result_max_interval_us"
    printf 'present_result_max_interval_after_warmup_us: %s\n' "$present_result_max_interval_after_warmup_us"
    printf 'present_result_over_budget_count: %s\n' "$present_result_over_budget_count"
    printf 'present_result_over_budget_after_warmup_count: %s\n' "$present_result_over_budget_after_warmup_count"
    printf 'present_result_missed_vblank_threshold_us: %s\n' "$present_result_missed_vblank_threshold_us"
    printf 'present_result_missed_vblank_count: %s\n' "$present_result_missed_vblank_count"
    printf 'present_result_missed_vblank_after_warmup_count: %s\n' "$present_result_missed_vblank_after_warmup_count"
    printf 'bitstream_buffer_strategy: %s\n' "$bitstream_strategy"
    printf 'bitstream_buffer_slot_count: %s\n' "$bitstream_slot_count"
    printf 'bitstream_buffer_slot_bytes: %s\n' "$bitstream_slot_bytes"
    printf 'bitstream_ring_capacity_bytes: %s\n' "$bitstream_ring_capacity_bytes"
    printf 'bitstream_ring_wrap_count: %s\n' "$bitstream_ring_wrap_count"
    printf 'bitstream_window_payload_bytes: %s\n' "$bitstream_window_payload_bytes"
    printf 'bitstream_upload_count: %s\n' "$bitstream_upload_count"
    printf 'bitstream_uploaded_bytes: %s\n' "$bitstream_uploaded_bytes"
    printf 'h264_input_mode: %s\n' "$h264_input_mode"
    printf 'h264_display_handoff_strategy: %s\n' "$h264_display_handoff_strategy"
    printf 'h264_resource_image_layout: %s\n' "$h264_resource_image_layout"
    printf 'h264_video_queue_sync_strategy: %s\n' "$h264_video_queue_sync_strategy"
    printf 'h264_present_frame_preroll_count: %s\n' "$h264_present_frame_preroll_count"
    printf 'h264_present_queue_count: %s\n' "$h264_present_queue_count"
    printf 'h264_async_present_depth: %s\n' "$h264_async_present_depth"
    printf 'h264_present_result_wait_count: %s\n' "$h264_present_result_wait_count"
    printf 'h264_present_result_wait_elapsed_us: %s\n' "$h264_present_result_wait_elapsed_us"
    printf 'h264_present_result_wait_max_us: %s\n' "$h264_present_result_wait_max_us"
    printf 'h264_acquire_not_ready_count: %s\n' "$h264_acquire_not_ready_count"
    printf 'h264_acquire_wait_present_result_count: %s\n' "$h264_acquire_wait_present_result_count"
    printf 'h264_acquire_wait_present_result_elapsed_us: %s\n' "$h264_acquire_wait_present_result_elapsed_us"
    printf 'h264_acquire_wait_present_result_max_us: %s\n' "$h264_acquire_wait_present_result_max_us"
    printf 'h264_display_ring_slot_count: %s\n' "$h264_display_ring_slot_count"
    printf 'h264_display_ring_memory_bytes: %s\n' "$h264_display_ring_memory_bytes"
    printf 'h264_display_copy_count: %s\n' "$h264_display_copy_count"
    printf 'h264_display_copy_record_elapsed_us: %s\n' "$h264_display_copy_record_elapsed_us"
    printf 'h264_display_copy_submit_elapsed_us: %s\n' "$h264_display_copy_submit_elapsed_us"
    printf 'h264_packet_queue_capacity: %s\n' "$h264_packet_queue_capacity"
    printf 'h264_packet_queue_pulled_count: %s\n' "$h264_packet_queue_pulled_count"
    printf 'h264_packet_queue_eos_count: %s\n' "$h264_packet_queue_eos_count"
    printf 'h264_packet_queue_loop_count: %s\n' "$h264_packet_queue_loop_count"
    printf 'h264_packet_queue_loop_skip_access_units: %s\n' "$h264_packet_queue_loop_skip_access_units"
    printf 'h264_packet_queue_bootstrap_discarded_access_units: %s\n' "$h264_packet_queue_bootstrap_discarded_access_units"
    printf 'h264_packet_queue_max_payload_bytes: %s\n' "$h264_packet_queue_max_payload_bytes"
    printf 'h264_decode_ahead_strategy: %s\n' "$h264_decode_ahead_strategy"
    printf 'h264_decode_ahead_attempt_count: %s\n' "$h264_decode_ahead_attempt_count"
    printf 'h264_decode_ahead_submit_count: %s\n' "$h264_decode_ahead_submit_count"
    printf 'h264_decode_ahead_skip_unready_count: %s\n' "$h264_decode_ahead_skip_unready_count"
    printf 'h264_decode_ahead_skip_output_hazard_count: %s\n' "$h264_decode_ahead_skip_output_hazard_count"
    printf 'h264_decode_ahead_skip_reference_hazard_count: %s\n' "$h264_decode_ahead_skip_reference_hazard_count"
    printf 'h264_decode_ahead_skip_bitstream_overlap_count: %s\n' "$h264_decode_ahead_skip_bitstream_overlap_count"
    printf 'h264_decode_ahead_copy_wait_output_hazard_count: %s\n' "$h264_decode_ahead_copy_wait_output_hazard_count"
    printf 'h264_decode_ahead_copy_wait_reference_hazard_count: %s\n' "$h264_decode_ahead_copy_wait_reference_hazard_count"
    printf 'arbitrary_entry_source: %s\n' "$([[ "$arbitrary_entry_source" -eq 1 ]] && printf yes || printf no)"
    printf 'arbitrary_entry_offset: %s\n' "${arbitrary_entry_offset:-none}"
    printf 'arbitrary_entry_gate_failed: %s\n' "$arbitrary_entry_gate_failed"
    printf 'require_loop_skip_replay: %s\n' "$([[ "$require_loop_skip_replay" -eq 1 ]] && printf yes || printf no)"
    printf 'loop_skip_replay_gate_failed: %s\n' "$loop_skip_replay_gate_failed"
    printf 'first_frame_idr: %s\n' "$first_frame_idr"
    printf 'loop_first_non_idr_count: %s\n' "$loop_first_non_idr_count"
    printf 'first_frame_recovery: %s\n' "$first_frame_recovery"
    printf 'loop_first_unrecovered_count: %s\n' "$loop_first_unrecovered_count"
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
  printf 'generated_source_pattern: %s\n' "$([[ "$generated_source" -eq 1 ]] && printf 'testsrc2-continuous-closed-gop-h264-high-b%s-weightp%s-weightb%s' "$bframes" "$weightp" "$weightb" || printf none)"
  printf 'generated_source_cache_dir: %s\n' "$source_cache_dir"
  printf 'generated_source_refs: %s\n' "$([[ "$generated_source" -eq 1 ]] && printf '%s' "$refs" || printf none)"
  printf 'generated_source_bframes: %s\n' "$([[ "$generated_source" -eq 1 ]] && printf '%s' "$bframes" || printf none)"
  printf 'generated_source_weightp: %s\n' "$([[ "$generated_source" -eq 1 ]] && printf '%s' "$weightp" || printf none)"
  printf 'generated_source_weightb: %s\n' "$([[ "$generated_source" -eq 1 ]] && printf '%s' "$weightb" || printf none)"
  printf 'generated_source_level: %s\n' "$([[ "$generated_source" -eq 1 ]] && printf '%s' "$level" || printf none)"
  printf 'arbitrary_entry_source: %s\n' "$([[ "$arbitrary_entry_source" -eq 1 ]] && printf yes || printf no)"
  printf 'arbitrary_entry_offset: %s\n' "${arbitrary_entry_offset:-none}"
  printf 'require_loop_skip_replay: %s\n' "$([[ "$require_loop_skip_replay" -eq 1 ]] && printf yes || printf no)"
  printf 'decode_prefix_explicit: %s\n' "$([[ "$decode_prefix_explicit" -eq 1 ]] && printf yes || printf no)"
  printf 'selected_device: %s\n' "$(jq -r '.selected_physical_device_name' "$runtime_json")"
  printf 'requested_output_name: %s\n' "${output_name:-auto}"
  printf 'surface_logical_size: %s\n' "$(jq -c '.wayland_surface_logical_size' "$runtime_json")"
  printf 'surface_buffer_size: %s\n' "$(jq -c '.wayland_surface_buffer_size' "$runtime_json")"
  printf 'source_extent: %s\n' "$(jq -c '.source_extent' "$runtime_json")"
  printf 'swapchain_extent: %s\n' "$(jq -c '.swapchain_extent' "$runtime_json")"
  printf 'swapchain_format: %s\n' "$(jq -r '.swapchain_format' "$runtime_json")"
  printf 'present_mode: %s\n' "$present_mode"
  printf 'runtime_elapsed_ms: %s\n' "$(jq -r '.runtime_elapsed_ms' "$runtime_json")"
  printf 'ready_prefix_frame_count: %s\n' "$ready_prefix_count"
  printf 'ready_prefix_loop_period_ms: %s\n' "$ready_prefix_loop_period_ms"
  printf 'requested_playback_frame_count: %s\n' "$requested_playback_count"
  printf 'decoded_frame_count: %s\n' "$decoded_count"
  printf 'presented_frame_count: %s\n' "$presented_count"
  printf 'playback_loop_count: %s\n' "$playback_loop_count"
  printf 'loop_boundary_reset_count: %s\n' "$loop_boundary_reset_count"
  printf 'idr_frames: %s\n' "$idr_frames"
  printf 'p_frames: %s\n' "$p_frames"
  printf 'b_frames: %s\n' "$b_frames"
  printf 'max_requested_reference_count: %s\n' "$max_requested_reference_count"
  printf 'max_reference_count: %s\n' "$max_reference_count"
  printf 'pacing_master: %s\n' "$pacing_master"
  printf 'pacing_strategy: %s\n' "$pacing_strategy"
  printf 'expected_pacing_strategy: %s\n' "$expected_pacing_strategy"
  printf 'frame_sleep_count: %s\n' "$frame_sleep_count_value"
  printf 'missed_frame_pacing_count: %s\n' "$(jq -r '.missed_frame_pacing_count // 0' "$runtime_json")"
  printf 'total_frame_sleep_us: %s\n' "$(jq -r '.total_frame_sleep_us // 0' "$runtime_json")"
  printf 'max_frame_pacing_late_us: %s\n' "$(jq -r '.max_frame_pacing_late_us // 0' "$runtime_json")"
  printf 'average_present_fps: %s\n' "$average_present_fps"
  printf 'average_present_result_fps: %s\n' "$average_present_result_fps"
  printf 'average_present_result_drop_first_fps: %s\n' "$average_present_result_drop_first_fps"
  printf 'average_present_result_drop_first_60_fps: %s\n' "$average_present_result_drop_first_60_fps"
  printf 'present_result_first_interval_us: %s\n' "$present_result_first_interval_us"
  printf 'present_result_max_interval_us: %s\n' "$present_result_max_interval_us"
  printf 'present_result_max_interval_after_warmup_us: %s\n' "$present_result_max_interval_after_warmup_us"
  printf 'present_result_over_budget_count: %s\n' "$present_result_over_budget_count"
  printf 'present_result_over_budget_after_warmup_count: %s\n' "$present_result_over_budget_after_warmup_count"
  printf 'present_result_missed_vblank_threshold_us: %s\n' "$present_result_missed_vblank_threshold_us"
  printf 'present_result_missed_vblank_count: %s\n' "$present_result_missed_vblank_count"
  printf 'present_result_missed_vblank_after_warmup_count: %s\n' "$present_result_missed_vblank_after_warmup_count"
  printf 'target_max_fps: %s\n' "$(jq -r '.target_max_fps // "none"' "$runtime_json")"
  printf 'present_queue_family_index: %s\n' "$present_queue"
  printf 'present_queue_flags: %s\n' "$(jq -c '.present_queue_flags' "$runtime_json")"
  printf 'video_decode_queue_family_index: %s\n' "$video_queue"
  printf 'video_decode_queue_flags: %s\n' "$(jq -c '.video_decode_queue_flags' "$runtime_json")"
  printf 'video_decode_queue_codec_operations: %s\n' "$(jq -c '.video_decode_queue_codec_operations' "$runtime_json")"
  printf 'cross_queue_sync_strategy: %s\n' "$(jq -r '.cross_queue_sync_strategy' "$runtime_json")"
  printf 'driver_max_dpb_slots: %s\n' "$driver_max_dpb_slots"
  printf 'stream_sps_dpb_slots: %s\n' "$stream_sps_dpb_slots"
  printf 'stream_dpb_slots: %s\n' "$stream_dpb_slots"
  printf 'stream_max_active_reference_pictures: %s\n' "$stream_max_active_reference_pictures"
  printf 'session_max_dpb_slots: %s\n' "$session_max_dpb_slots"
  printf 'session_max_active_reference_pictures: %s\n' "$session_max_active_reference_pictures"
  printf 'h264_picture_layout: %s\n' "$h264_picture_layout"
  printf 'h264_stream_profile: %s\n' "$h264_stream_profile"
  printf 'h264_stream_profile_idc: %s\n' "$h264_stream_profile_idc"
  printf 'h264_vulkan_std_profile_idc: %s\n' "$h264_vulkan_std_profile_idc"
  printf 'bitstream_buffer_strategy: %s\n' "$bitstream_strategy"
  printf 'bitstream_buffer_slot_count: %s\n' "$bitstream_slot_count"
  printf 'bitstream_buffer_slot_bytes: %s\n' "$bitstream_slot_bytes"
  printf 'bitstream_ring_capacity_bytes: %s\n' "$bitstream_ring_capacity_bytes"
  printf 'bitstream_ring_min_offset_alignment: %s\n' "$(jq -r '.bitstream_ring_min_offset_alignment // 0' "$runtime_json")"
  printf 'bitstream_ring_min_size_alignment: %s\n' "$(jq -r '.bitstream_ring_min_size_alignment // 0' "$runtime_json")"
  printf 'bitstream_ring_wrap_count: %s\n' "$bitstream_ring_wrap_count"
  printf 'bitstream_window_payload_bytes: %s\n' "$bitstream_window_payload_bytes"
  printf 'bitstream_upload_count: %s\n' "$bitstream_upload_count"
  printf 'bitstream_uploaded_bytes: %s\n' "$bitstream_uploaded_bytes"
  printf 'h264_input_mode: %s\n' "$h264_input_mode"
  printf 'h264_display_handoff_strategy: %s\n' "$h264_display_handoff_strategy"
  printf 'h264_resource_image_layout: %s\n' "$h264_resource_image_layout"
  printf 'h264_video_queue_sync_strategy: %s\n' "$h264_video_queue_sync_strategy"
  printf 'h264_present_frame_preroll_count: %s\n' "$h264_present_frame_preroll_count"
  printf 'h264_present_queue_count: %s\n' "$h264_present_queue_count"
  printf 'h264_async_present_depth: %s\n' "$h264_async_present_depth"
  printf 'h264_present_result_wait_count: %s\n' "$h264_present_result_wait_count"
  printf 'h264_present_result_wait_elapsed_us: %s\n' "$h264_present_result_wait_elapsed_us"
  printf 'h264_present_result_wait_max_us: %s\n' "$h264_present_result_wait_max_us"
  printf 'h264_acquire_not_ready_count: %s\n' "$h264_acquire_not_ready_count"
  printf 'h264_acquire_wait_present_result_count: %s\n' "$h264_acquire_wait_present_result_count"
  printf 'h264_acquire_wait_present_result_elapsed_us: %s\n' "$h264_acquire_wait_present_result_elapsed_us"
  printf 'h264_acquire_wait_present_result_max_us: %s\n' "$h264_acquire_wait_present_result_max_us"
  printf 'h264_display_ring_slot_count: %s\n' "$h264_display_ring_slot_count"
  printf 'h264_display_ring_memory_bytes: %s\n' "$h264_display_ring_memory_bytes"
  printf 'h264_display_copy_count: %s\n' "$h264_display_copy_count"
  printf 'h264_display_copy_record_elapsed_us: %s\n' "$h264_display_copy_record_elapsed_us"
  printf 'h264_display_copy_submit_elapsed_us: %s\n' "$h264_display_copy_submit_elapsed_us"
  printf 'h264_packet_queue_capacity: %s\n' "$h264_packet_queue_capacity"
  printf 'h264_packet_queue_pulled_count: %s\n' "$h264_packet_queue_pulled_count"
  printf 'h264_packet_queue_eos_count: %s\n' "$h264_packet_queue_eos_count"
  printf 'h264_packet_queue_loop_count: %s\n' "$h264_packet_queue_loop_count"
  printf 'h264_packet_queue_loop_skip_access_units: %s\n' "$h264_packet_queue_loop_skip_access_units"
  printf 'h264_packet_queue_bootstrap_discarded_access_units: %s\n' "$h264_packet_queue_bootstrap_discarded_access_units"
  printf 'h264_packet_queue_max_payload_bytes: %s\n' "$h264_packet_queue_max_payload_bytes"
  printf 'h264_packet_queue_retained_payload_bytes: %s\n' "$(jq -r '.h264_packet_queue_retained_payload_bytes // 0' "$runtime_json")"
  printf 'h264_decode_ahead_strategy: %s\n' "$h264_decode_ahead_strategy"
  printf 'h264_decode_ahead_attempt_count: %s\n' "$h264_decode_ahead_attempt_count"
  printf 'h264_decode_ahead_submit_count: %s\n' "$h264_decode_ahead_submit_count"
  printf 'h264_decode_ahead_skip_unready_count: %s\n' "$h264_decode_ahead_skip_unready_count"
  printf 'h264_decode_ahead_skip_output_hazard_count: %s\n' "$h264_decode_ahead_skip_output_hazard_count"
  printf 'h264_decode_ahead_skip_reference_hazard_count: %s\n' "$h264_decode_ahead_skip_reference_hazard_count"
  printf 'h264_decode_ahead_skip_bitstream_overlap_count: %s\n' "$h264_decode_ahead_skip_bitstream_overlap_count"
  printf 'h264_decode_ahead_copy_wait_output_hazard_count: %s\n' "$h264_decode_ahead_copy_wait_output_hazard_count"
  printf 'h264_decode_ahead_copy_wait_reference_hazard_count: %s\n' "$h264_decode_ahead_copy_wait_reference_hazard_count"
  printf 'first_frame_idr: %s\n' "$first_frame_idr"
  printf 'loop_first_non_idr_count: %s\n' "$loop_first_non_idr_count"
  printf 'first_frame_recovery: %s\n' "$first_frame_recovery"
  printf 'loop_first_unrecovered_count: %s\n' "$loop_first_unrecovered_count"
  printf 'frame_layers_head: %s\n' "$(jq -c '[.frames[0:32][]?.dst_base_array_layer]' "$runtime_json")"
  printf 'frame_layers_tail: %s\n' "$(jq -c '[.frames[-32:][]?.dst_base_array_layer]' "$runtime_json")"
  printf 'frame_display_slots_head: %s\n' "$(jq -c '[.frames[0:32][]?.display_slot]' "$runtime_json")"
  printf 'frame_display_slots_tail: %s\n' "$(jq -c '[.frames[-32:][]?.display_slot]' "$runtime_json")"
  printf 'frame_access_units_head: %s\n' "$(jq -c '[.frames[0:32][]?.access_unit_index]' "$runtime_json")"
  printf 'frame_access_units_tail: %s\n' "$(jq -c '[.frames[-32:][]?.access_unit_index]' "$runtime_json")"
  printf 'frame_requested_reference_counts_head: %s\n' "$(jq -c '[.frames[0:32][]?.requested_reference_count]' "$runtime_json")"
  printf 'frame_reference_counts_head: %s\n' "$(jq -c '[.frames[0:32][]?.decode_reference_slot_count]' "$runtime_json")"
  printf 'frame_reference_counts_tail: %s\n' "$(jq -c '[.frames[-32:][]?.decode_reference_slot_count]' "$runtime_json")"
  printf 'frame_loop_indices_head: %s\n' "$(jq -c '[.frames[0:32][]?.playback_loop_index]' "$runtime_json")"
  printf 'frame_loop_indices_tail: %s\n' "$(jq -c '[.frames[-32:][]?.playback_loop_index]' "$runtime_json")"
  printf 'frame_bitstream_offsets_head: %s\n' "$(jq -c '[.frames[0:32][]?.src_buffer_offset]' "$runtime_json")"
  printf 'frame_bitstream_offsets_tail: %s\n' "$(jq -c '[.frames[-32:][]?.src_buffer_offset]' "$runtime_json")"
  printf 'frame_bitstream_wraps_head: %s\n' "$(jq -c '[.frames[0:32][]?.bitstream_ring_wrap_count]' "$runtime_json")"
  printf 'frame_bitstream_wraps_tail: %s\n' "$(jq -c '[.frames[-32:][]?.bitstream_ring_wrap_count]' "$runtime_json")"
  printf 'pts_delta_min_ms: %s\n' "$pts_delta_min"
  printf 'pts_delta_max_ms: %s\n' "$pts_delta_max"
  printf 'pts_delta_expected_min_ms: %s\n' "$pts_delta_expected_min"
  printf 'pts_delta_expected_max_ms: %s\n' "$pts_delta_expected_max"
  printf 'pts_delta_in_expected_range: %s\n' "$pts_delta_in_expected_range"
  printf 'pts_delta_script_expected_min_ms: %s\n' "$script_pts_delta_expected_min"
  printf 'pts_delta_script_expected_max_ms: %s\n' "$script_pts_delta_expected_max"
  printf 'pts_delta_script_in_expected_range: %s\n' "$pts_delta_script_in_expected_range"
  printf 'audio_clock_probe_requested: %s\n' "$([[ "$audio_clock_probe" -eq 1 ]] && printf yes || printf no)"
  printf 'audio_clock_probe_present: %s\n' "$audio_clock_probe_present"
  printf 'audio_reached_clocked_playback: %s\n' "$audio_reached_clocked_playback"
  printf 'audio_no_video_decoder_instantiated: %s\n' "$audio_no_video_decoder_instantiated"
  printf 'audio_buffer_count: %s\n' "$audio_buffer_count"
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
  printf 'max_bitstream_upload_elapsed_us: %s\n' "$(jq -r '[.frames[]?.bitstream_upload_elapsed_us] | max' "$runtime_json")"
  printf 'max_decode_elapsed_us: %s\n' "$(jq -r '[.frames[]?.decode_elapsed_us] | max' "$runtime_json")"
  printf 'avg_display_slot_wait_elapsed_us: %s\n' "$(jq -r '[.frames[]?.display_slot_wait_elapsed_us] | add / length' "$runtime_json")"
  printf 'max_display_slot_wait_elapsed_us: %s\n' "$(jq -r '[.frames[]?.display_slot_wait_elapsed_us] | max' "$runtime_json")"
  printf 'avg_display_copy_record_elapsed_us: %s\n' "$(jq -r '[.frames[]?.display_copy_record_elapsed_us] | add / length' "$runtime_json")"
  printf 'max_display_copy_record_elapsed_us: %s\n' "$(jq -r '[.frames[]?.display_copy_record_elapsed_us] | max' "$runtime_json")"
  printf 'avg_display_copy_submit_elapsed_us: %s\n' "$(jq -r '[.frames[]?.display_copy_submit_elapsed_us] | add / length' "$runtime_json")"
  printf 'max_display_copy_submit_elapsed_us: %s\n' "$(jq -r '[.frames[]?.display_copy_submit_elapsed_us] | max' "$runtime_json")"
  printf 'avg_acquire_elapsed_us: %s\n' "$(jq -r '[.frames[]?.acquire_elapsed_us] | add / length' "$runtime_json")"
  printf 'max_acquire_elapsed_us: %s\n' "$(jq -r '[.frames[]?.acquire_elapsed_us] | max' "$runtime_json")"
  printf 'present_budget_us: %s\n' "$present_budget_us"
  printf 'acquire_over_budget_count: %s\n' "$acquire_over_budget_count"
  printf 'avg_record_elapsed_us: %s\n' "$(jq -r '[.frames[]?.record_elapsed_us] | add / length' "$runtime_json")"
  printf 'max_record_elapsed_us: %s\n' "$(jq -r '[.frames[]?.record_elapsed_us] | max' "$runtime_json")"
  printf 'avg_submit_elapsed_us: %s\n' "$(jq -r '[.frames[]?.submit_elapsed_us] | add / length' "$runtime_json")"
  printf 'max_submit_elapsed_us: %s\n' "$(jq -r '[.frames[]?.submit_elapsed_us] | max' "$runtime_json")"
  printf 'avg_queue_present_elapsed_us: %s\n' "$(jq -r '[.frames[]?.queue_present_elapsed_us] | add / length' "$runtime_json")"
  printf 'max_queue_present_elapsed_us: %s\n' "$(jq -r '[.frames[]?.queue_present_elapsed_us] | max' "$runtime_json")"
  printf 'queue_present_over_budget_count: %s\n' "$queue_present_over_budget_count"
  printf 'max_present_elapsed_us: %s\n' "$(jq -r '[.frames[]?.present_elapsed_us] | max' "$runtime_json")"
  printf 'present_over_budget_count: %s\n' "$present_over_budget_count"
  printf 'video_resource_memory_bytes: %s\n' "$resource_bytes"
  printf 'session_memory_bytes: %s\n' "$(jq -r '.session_memory_bytes' "$runtime_json")"
  printf 'bitstream_buffer_bytes: %s\n' "$(jq -r '.bitstream_buffer_bytes' "$runtime_json")"
  printf 'performance_snapshot: %s\n' "$([[ "$performance_snapshot" -eq 1 ]] && printf yes || printf no)"
  if [[ "$performance_snapshot" -eq 1 ]]; then
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

printf 'PASS: native Vulkan direct H.264 ready-prefix video smoke passed\n'
printf 'summary: %s\n' "$summary"
printf 'runtime JSON: %s\n' "$runtime_json"
if [[ "$performance_snapshot" -eq 1 ]]; then
  printf 'performance summary: %s\n' "$performance_dir/summary.txt"
fi
