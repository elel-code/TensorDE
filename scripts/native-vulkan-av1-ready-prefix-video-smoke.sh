#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
usage: scripts/native-vulkan-av1-ready-prefix-video-smoke.sh [options]

Generate or use an AV1 source, then verify native Vulkan Video AV1 direct
decode/present on a real Wayland output.

Options:
  --display <name>                Wayland display name. Default: WAYLAND_DISPLAY.
  --source <path>                 Existing AV1 source. Default: generate source.
  --output-name <name>            Wayland output name, for example HDMI-A-1.
  --output <name>                 Alias for --output-name.
  --work-dir <path>               Parent directory for generated evidence. Default: /tmp.
  --source-cache-dir <path>       Persistent generated source cache. Default: artifacts/video-sources/av1.
  --width <px>                    Source width. Default: 640.
  --height <px>                   Source height. Default: 368.
  --target-fps <fps>              Decode/present target FPS. Default: 60.
  --frames <n>                    Generated source frames. Default: decode-prefix + 2.
  --decode-prefix <n>             Bootstrap ready TU window. Default: 60.
  --playback-frames <n>           Presented frames. Default: decode-prefix.
  --bit-depth <8|10>              Generated/probed AV1 Main bit depth. Default: 10.
  --arbitrary-entry-offset <sec>  Generate a non-keyframe entry source with ffmpeg -copyinkf.
  --require-loop-skip-replay      Require EOS loop replay to skip leading non-key TUs.
  --require-readback-diversity    Require visible diagnostic readback hashes to change.
  --readback-frames <n>           Diagnostic visible readback frame count. Default: 16 when required.
  --readback-hidden               Also read back hidden decode outputs.
  --performance-snapshot          Capture process CPU/RSS/PSS/USS/Private_Dirty/smaps.
  --performance-duration <sec>    Performance sampling duration. Default: 10.
  --performance-interval <sec>    Performance sampling interval. Default: 1.
  --layer <layer>                 Wayland layer. Default: background.
  --fit <mode>                    Render fit. Default: cover.
  --allow-short-loop              Allow looped playback with a ready-prefix shorter than 1 second.
  --report-dir <path>             Report directory. Default: mktemp under /tmp.
  --no-build                      Reuse target/release/gilder-native-vulkan.
  --keep                          Compatibility no-op; evidence directories are always kept.
  -h, --help                      Show this help.
USAGE
}

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
cd "$repo_root"
source "$script_dir/native-vulkan-ready-prefix-video-common.sh"

source=""
display="${WAYLAND_DISPLAY:-}"
output_name="${GILDER_WAYLAND_OUTPUT:-}"
work_parent="${TMPDIR:-/tmp}"
width=640
height=368
target_fps=60
frames=0
frames_explicit=0
decode_prefix=60
playback_frames=0
bit_depth=10
arbitrary_entry_offset=""
arbitrary_entry_source=0
arbitrary_entry_demux_dropped_prefix=0
arbitrary_entry_first_decodable_pts="none"
arbitrary_entry_first_key_pts="none"
arbitrary_entry_probe_log=""
arbitrary_entry_probe_frames=""
arbitrary_entry_probe_status=0
require_loop_skip_replay=0
require_readback_diversity=0
readback_frames=0
readback_hidden=0
performance_snapshot=0
performance_duration=10
performance_interval=1
layer="background"
fit="cover"
allow_short_loop=0
report_dir=""
source_cache_dir="$(gilder_default_source_cache_dir av1)"
no_build=0
generated_source=0
source_duration_seconds=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --display)
      display="${2:?--display requires a value}"
      shift 2
      ;;
    --source)
      source="${2:?--source requires a path}"
      shift 2
      ;;
    --output-name|--output)
      output_name="${2:?--output-name requires a value}"
      shift 2
      ;;
    --work-dir)
      work_parent="${2:?--work-dir requires a path}"
      shift 2
      ;;
    --source-cache-dir)
      source_cache_dir="${2:?--source-cache-dir requires a path}"
      shift 2
      ;;
    --width)
      width="${2:?--width requires a value}"
      shift 2
      ;;
    --height)
      height="${2:?--height requires a value}"
      shift 2
      ;;
    --target-fps)
      target_fps="${2:?--target-fps requires a value}"
      shift 2
      ;;
    --frames)
      frames="${2:?--frames requires a value}"
      frames_explicit=1
      shift 2
      ;;
    --decode-prefix)
      decode_prefix="${2:?--decode-prefix requires a value}"
      shift 2
      ;;
    --playback-frames)
      playback_frames="${2:?--playback-frames requires a value}"
      shift 2
      ;;
    --bit-depth)
      bit_depth="${2:?--bit-depth requires a value}"
      shift 2
      ;;
    --arbitrary-entry-offset)
      arbitrary_entry_offset="${2:?--arbitrary-entry-offset requires seconds}"
      shift 2
      ;;
    --require-loop-skip-replay)
      require_loop_skip_replay=1
      shift
      ;;
    --require-readback-diversity)
      require_readback_diversity=1
      shift
      ;;
    --readback-frames)
      readback_frames="${2:?--readback-frames requires a value}"
      shift 2
      ;;
    --readback-hidden)
      readback_hidden=1
      shift
      ;;
    --performance-snapshot)
      performance_snapshot=1
      shift
      ;;
    --performance-duration)
      performance_duration="${2:?--performance-duration requires seconds}"
      shift 2
      ;;
    --performance-interval)
      performance_interval="${2:?--performance-interval requires seconds}"
      shift 2
      ;;
    --layer)
      layer="${2:?--layer requires a value}"
      shift 2
      ;;
    --fit)
      fit="${2:?--fit requires a value}"
      shift 2
      ;;
    --allow-short-loop)
      allow_short_loop=1
      shift
      ;;
    --report-dir)
      report_dir="${2:?--report-dir requires a path}"
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
      printf 'unknown argument: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "$display" ]]; then
  printf 'FAIL: WAYLAND_DISPLAY is empty; pass --display\n' >&2
  exit 1
fi
for tool in ffmpeg ffprobe jq; do
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
if [[ "$width" -le 0 || "$height" -le 0 || "$target_fps" -le 0 || "$decode_prefix" -le 0 ]]; then
  printf 'FAIL: width/height/target-fps/decode-prefix must be positive\n' >&2
  exit 1
fi
if (( width % 16 != 0 || height % 16 != 0 )); then
  printf 'FAIL: AV1 Vulkan Video source dimensions must be 16-pixel aligned; got %sx%s\n' "$width" "$height" >&2
  exit 1
fi
if [[ "$bit_depth" != "8" && "$bit_depth" != "10" ]]; then
  printf 'FAIL: --bit-depth must be 8 or 10\n' >&2
  exit 1
fi

if [[ "$playback_frames" -eq 0 ]]; then
  playback_frames="$decode_prefix"
fi
if [[ "$readback_frames" -lt 0 || "$performance_duration" -lt 1 || "$performance_interval" -lt 1 ]]; then
  printf 'FAIL: readback-frames/performance-duration/performance-interval must be valid\n' >&2
  exit 1
fi
if [[ "$require_readback_diversity" -eq 1 && "$readback_frames" -eq 0 ]]; then
  readback_frames=16
fi
ready_prefix_loop_period_ms=$((decode_prefix * 1000 / target_fps))
if [[ "$playback_frames" -gt "$decode_prefix" && "$decode_prefix" -lt "$target_fps" && "$allow_short_loop" -eq 0 ]]; then
  {
    printf 'FAIL: visible AV1 ready-prefix loop is too short for smoothness\n'
    printf 'decode_prefix: %s\n' "$decode_prefix"
    printf 'target_fps: %s\n' "$target_fps"
    printf 'ready_prefix_loop_period_ms: %s\n' "$ready_prefix_loop_period_ms"
    printf 'expected_playback_frames: %s\n' "$playback_frames"
    printf 'Pass --allow-short-loop only for deliberate short-loop diagnostics.\n'
  } >&2
  exit 1
fi

video_codec="av1-main-8"
pix_fmt="yuv420p"
expected_picture_format="G8_B8R8_2PLANE_420_UNORM"
if [[ "$bit_depth" -eq 10 ]]; then
  video_codec="av1-main-10"
  pix_fmt="yuv420p10le"
  expected_picture_format="G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16"
fi

if [[ -z "$report_dir" ]]; then
  report_dir="$(mktemp -d "${work_parent%/}/gilder-vulkan-av1-ready-prefix-video.XXXXXX")"
else
  mkdir -p "$report_dir"
fi
summary="$report_dir/summary.txt"
runtime_json="$report_dir/runtime.json"
stderr_log="$report_dir/stderr.log"
generated_dir="$source_cache_dir"
performance_dir="$report_dir/performance"
performance_log="$report_dir/performance.log"
gilder_ensure_source_cache_dir "$generated_dir"

if [[ "$no_build" -eq 0 ]]; then
  cargo build --release --features native-vulkan-gst-video --bin gilder-native-vulkan
fi

if [[ -z "$source" ]]; then
  generated_source=1
  if [[ "$frames" -eq 0 || "$frames" -lt $((decode_prefix + 2)) ]]; then
    frames=$((decode_prefix + 2))
  fi
  if [[ "$frames_explicit" -eq 0 && -n "$arbitrary_entry_offset" ]]; then
    offset_frames="$(awk -v offset="$arbitrary_entry_offset" -v fps="$target_fps" 'BEGIN { value = offset * fps; printf "%d", (value == int(value)) ? value : int(value) + 1 }')"
    arbitrary_window_frames="$playback_frames"
    if [[ "$require_loop_skip_replay" -eq 1 || "$playback_frames" -gt "$decode_prefix" ]]; then
      arbitrary_window_frames="$decode_prefix"
    fi
    arbitrary_min_frames=$((offset_frames + decode_prefix + arbitrary_window_frames + 2))
    if [[ "$frames" -lt "$arbitrary_min_frames" ]]; then
      frames="$arbitrary_min_frames"
    fi
  fi
  source_duration_seconds=$(( (frames + target_fps - 1) / target_fps ))
  base_source="$generated_dir/av1-main${bit_depth}-${width}x${height}-${target_fps}fps-${frames}frames-g${decode_prefix}.webm"
  if [[ ! -s "$base_source" ]]; then
    ffmpeg -hide_banner -loglevel error -y \
      -f lavfi -i "testsrc2=size=${width}x${height}:rate=${target_fps}:duration=${source_duration_seconds}" \
      -frames:v "$frames" -an -c:v libaom-av1 -cpu-used 8 -crf 40 -b:v 0 -row-mt 1 \
      -g "$decode_prefix" -pix_fmt "$pix_fmt" "$base_source"
  fi
  source="$base_source"
fi

if [[ ! -f "$source" ]]; then
  printf 'FAIL: source does not exist: %s\n' "$source" >&2
  exit 1
fi

if [[ -n "$arbitrary_entry_offset" ]]; then
  arbitrary_entry_source=1
  arbitrary_stem="$(basename "$source")"
  arbitrary_stem="${arbitrary_stem%.*}"
  arbitrary_source="$generated_dir/${arbitrary_stem}-arbitrary-${arbitrary_entry_offset}s.webm"
  if [[ ! -s "$arbitrary_source" ]]; then
    ffmpeg -hide_banner -loglevel error -y \
      -i "$source" -ss "$arbitrary_entry_offset" \
      -c copy -copyinkf -avoid_negative_ts make_zero \
      "$arbitrary_source"
  fi
  source="$arbitrary_source"
  if [[ ! -s "$source" ]]; then
    printf 'FAIL: arbitrary-entry shifted source was not created: %s\n' "$source" >&2
    exit 1
  fi
  arbitrary_entry_probe_log="$report_dir/arbitrary-entry-ffprobe.log"
  arbitrary_entry_probe_frames="$report_dir/arbitrary-entry-frames.csv"
  set +e
  ffprobe -hide_banner -loglevel error \
    -select_streams v -show_frames \
    -show_entries frame=key_frame,best_effort_timestamp_time,pict_type \
    -of csv=p=0 "$source" >"$arbitrary_entry_probe_frames" 2>"$arbitrary_entry_probe_log"
  arbitrary_entry_probe_status=$?
  set -e
  arbitrary_entry_first_decodable_pts="$(awk -F, 'NF >= 2 { print $2; exit }' "$arbitrary_entry_probe_frames")"
  arbitrary_entry_first_key_pts="$(awk -F, '$1 == 1 && NF >= 2 { print $2; exit }' "$arbitrary_entry_probe_frames")"
  arbitrary_entry_first_decodable_pts="${arbitrary_entry_first_decodable_pts:-none}"
  arbitrary_entry_first_key_pts="${arbitrary_entry_first_key_pts:-none}"
  if awk -v pts="$arbitrary_entry_first_key_pts" 'BEGIN { exit !((pts + 0) > 0.001) }'; then
    arbitrary_entry_demux_dropped_prefix=1
  fi
fi
if [[ "$arbitrary_entry_source" -eq 1 && "$playback_frames" -gt "$decode_prefix" ]]; then
  require_loop_skip_replay=1
fi

cmd=(
  target/release/gilder-native-vulkan
  --run-av1-ready-prefix-video
  --source "$source"
  --video-codec "$video_codec"
  --width "$width"
  --height "$height"
  --target-fps "$target_fps"
  --layer "$layer"
  --fit "$fit"
  --decode-av1-ready-prefix "$decode_prefix"
  --playback-frames "$playback_frames"
)
if [[ -n "$output_name" ]]; then
  cmd+=(--output-name "$output_name")
fi

runtime_status=0
performance_status=0
runtime_env=(WAYLAND_DISPLAY="$display")
if [[ "$readback_frames" -gt 0 ]]; then
  runtime_env+=(GILDER_VULKAN_AV1_READBACK_FRAMES="$readback_frames")
fi
if [[ "$readback_hidden" -eq 1 ]]; then
  runtime_env+=(GILDER_VULKAN_AV1_READBACK_HIDDEN=1)
fi
if [[ "$performance_snapshot" -eq 1 ]]; then
  if [[ ! -x scripts/performance-snapshot.sh ]]; then
    printf 'FAIL: missing executable scripts/performance-snapshot.sh\n' | tee "$summary"
    exit 1
  fi
  set +e
  env "${runtime_env[@]}" "${cmd[@]}" >"$runtime_json" 2>"$stderr_log" &
  runtime_pid=$!
  scripts/performance-snapshot.sh \
    --pid "$runtime_pid" \
    --label "native-vulkan-av1-ready-prefix-video" \
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
  env "${runtime_env[@]}" "${cmd[@]}" >"$runtime_json" 2>"$stderr_log"
  runtime_status=$?
  set -e
fi

if [[ "$runtime_status" -ne 0 ]]; then
  {
    printf 'FAIL: native Vulkan AV1 ready-prefix visible runtime failed\n'
    printf 'source: %s\n' "$source"
    printf 'stderr:\n'
    sed -n '1,120p' "$stderr_log"
  } | tee "$summary"
  exit "$runtime_status"
fi
if [[ "$performance_snapshot" -eq 1 && "$performance_status" -ne 0 ]]; then
  {
    printf 'FAIL: native Vulkan AV1 performance snapshot failed\n'
    printf 'source: %s\n' "$source"
    printf 'performance log: %s\n' "$performance_log"
  } | tee "$summary"
  sed -n '1,200p' "$performance_log" >&2
  exit "$performance_status"
fi

requested_codec="$(jq -r '.requested_codec // "none"' "$runtime_json")"
picture_format="$(jq -r '.picture_format // "none"' "$runtime_json")"
decoded_count="$(jq -r '.decoded_frame_count // 0' "$runtime_json")"
hidden_decoded_count="$(jq -r '.hidden_decoded_frame_count // 0' "$runtime_json")"
total_decoded_count="$(jq -r '.total_decoded_frame_count // 0' "$runtime_json")"
handoff_count="$(jq -r '.displayed_handoff_frame_count // 0' "$runtime_json")"
presented_count="$(jq -r '.presented_frame_count // 0' "$runtime_json")"
requested_playback_count="$(jq -r '.requested_playback_frame_count // 0' "$runtime_json")"
processed_temporal_unit_count="$(jq -r '.processed_temporal_unit_count // 0' "$runtime_json")"
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
present_budget_us=$(((1000000 + target_fps - 1) / target_fps))
acquire_over_budget_count="$(jq -r --argjson budget "$present_budget_us" '[.frames[]?.acquire_elapsed_us // 0 | select(. > $budget)] | length' "$runtime_json")"
queue_present_over_budget_count="$(jq -r --argjson budget "$present_budget_us" '[.frames[]?.queue_present_elapsed_us // 0 | select(. > $budget)] | length' "$runtime_json")"
present_over_budget_count="$(jq -r --argjson budget "$present_budget_us" '[.frames[]?.present_elapsed_us // 0 | select(. > $budget)] | length' "$runtime_json")"
configured="$(jq -r '.configured // false' "$runtime_json")"
queue_capacity="$(jq -r '.av1_packet_queue_capacity // 0' "$runtime_json")"
queue_pulled_count="$(jq -r '.av1_packet_queue_pulled_count // 0' "$runtime_json")"
queue_eos_count="$(jq -r '.av1_packet_queue_eos_count // 0' "$runtime_json")"
queue_loop_count="$(jq -r '.av1_packet_queue_loop_count // 0' "$runtime_json")"
queue_loop_skip_temporal_units="$(jq -r '.av1_packet_queue_loop_skip_temporal_units // 0' "$runtime_json")"
queue_bootstrap_discarded_temporal_units="$(jq -r '.av1_packet_queue_bootstrap_discarded_temporal_units // 0' "$runtime_json")"
queue_retained_payload_bytes="$(jq -r '.av1_packet_queue_retained_payload_bytes // 0' "$runtime_json")"
display_handoff_strategy="$(jq -r '.av1_display_handoff_strategy // "none"' "$runtime_json")"
display_ring_slot_count="$(jq -r '.av1_display_ring_slot_count // 0' "$runtime_json")"
display_ring_memory_bytes="$(jq -r '.av1_display_ring_memory_bytes // 0' "$runtime_json")"
display_copy_count="$(jq -r '.av1_display_copy_count // 0' "$runtime_json")"
display_copy_elided_count="$(jq -r '.av1_display_copy_elided_count // 0' "$runtime_json")"
present_command_buffer_strategy="$(jq -r '.av1_present_command_buffer_strategy // "none"' "$runtime_json")"
async_present_depth="$(jq -r '.av1_async_present_depth // 0' "$runtime_json")"
present_frame_queue_depth="$(jq -r '.av1_present_frame_queue_depth // 0' "$runtime_json")"
present_frame_preroll_count="$(jq -r '.av1_present_frame_preroll_count // 0' "$runtime_json")"
present_frame_queue_submit_count="$(jq -r '.av1_present_frame_queue_submit_count // 0' "$runtime_json")"
present_frame_queue_acquire_elapsed_us="$(jq -r '.av1_present_frame_queue_acquire_elapsed_us // 0' "$runtime_json")"
present_frame_queue_acquire_max_us="$(jq -r '.av1_present_frame_queue_acquire_max_us // 0' "$runtime_json")"
present_frame_queue_record_elapsed_us="$(jq -r '.av1_present_frame_queue_record_elapsed_us // 0' "$runtime_json")"
present_frame_queue_record_max_us="$(jq -r '.av1_present_frame_queue_record_max_us // 0' "$runtime_json")"
present_result_wait_count="$(jq -r '.av1_present_result_wait_count // 0' "$runtime_json")"
present_result_wait_elapsed_us="$(jq -r '.av1_present_result_wait_elapsed_us // 0' "$runtime_json")"
present_result_wait_max_us="$(jq -r '.av1_present_result_wait_max_us // 0' "$runtime_json")"
frame_context_selection_strategy="$(jq -r '.av1_frame_context_selection_strategy // "none"' "$runtime_json")"
frame_context_ready_probe_count="$(jq -r '.av1_frame_context_ready_probe_count // 0' "$runtime_json")"
frame_context_ready_hit_count="$(jq -r '.av1_frame_context_ready_hit_count // 0' "$runtime_json")"
frame_context_fallback_count="$(jq -r '.av1_frame_context_fallback_count // 0' "$runtime_json")"
frame_context_fence_wait_count="$(jq -r '.av1_frame_context_fence_wait_count // 0' "$runtime_json")"
frame_context_fence_wait_elapsed_us="$(jq -r '.av1_frame_context_fence_wait_elapsed_us // 0' "$runtime_json")"
frame_context_fence_wait_max_us="$(jq -r '.av1_frame_context_fence_wait_max_us // 0' "$runtime_json")"
display_ring_slot_wait_count="$(jq -r '.av1_display_ring_slot_wait_count // 0' "$runtime_json")"
display_ring_slot_wait_elapsed_us="$(jq -r '.av1_display_ring_slot_wait_elapsed_us // 0' "$runtime_json")"
display_ring_slot_wait_max_us="$(jq -r '.av1_display_ring_slot_wait_max_us // 0' "$runtime_json")"
display_ring_slot_reuse_sync_strategy="$(jq -r '.av1_display_ring_slot_reuse_sync_strategy // "none"' "$runtime_json")"
display_ring_slot_gpu_wait_count="$(jq -r '.av1_display_ring_slot_gpu_wait_count // 0' "$runtime_json")"
display_ring_slot_gpu_signal_count="$(jq -r '.av1_display_ring_slot_gpu_signal_count // 0' "$runtime_json")"
display_ring_slot_selection_strategy="$(jq -r '.av1_display_ring_slot_selection_strategy // "none"' "$runtime_json")"
display_ring_ready_probe_count="$(jq -r '.av1_display_ring_ready_probe_count // 0' "$runtime_json")"
display_ring_ready_hit_count="$(jq -r '.av1_display_ring_ready_hit_count // 0' "$runtime_json")"
display_ring_ready_fallback_count="$(jq -r '.av1_display_ring_ready_fallback_count // 0' "$runtime_json")"
swapchain_image_wait_count="$(jq -r '.av1_swapchain_image_wait_count // 0' "$runtime_json")"
swapchain_image_wait_elapsed_us="$(jq -r '.av1_swapchain_image_wait_elapsed_us // 0' "$runtime_json")"
swapchain_image_wait_max_us="$(jq -r '.av1_swapchain_image_wait_max_us // 0' "$runtime_json")"
acquire_not_ready_count="$(jq -r '.av1_acquire_not_ready_count // 0' "$runtime_json")"
acquire_wait_present_result_count="$(jq -r '.av1_acquire_wait_present_result_count // 0' "$runtime_json")"
acquire_wait_present_result_elapsed_us="$(jq -r '.av1_acquire_wait_present_result_elapsed_us // 0' "$runtime_json")"
acquire_wait_present_result_max_us="$(jq -r '.av1_acquire_wait_present_result_max_us // 0' "$runtime_json")"
acquired_image_queue_slot_count="$(jq -r '.av1_acquired_image_queue_slot_count // 0' "$runtime_json")"
acquired_image_queue_target_depth="$(jq -r '.av1_acquired_image_queue_target_depth // 0' "$runtime_json")"
acquired_image_queue_attempt_count="$(jq -r '.av1_acquired_image_queue_attempt_count // 0' "$runtime_json")"
acquired_image_queue_hit_count="$(jq -r '.av1_acquired_image_queue_hit_count // 0' "$runtime_json")"
acquired_image_queue_miss_count="$(jq -r '.av1_acquired_image_queue_miss_count // 0' "$runtime_json")"
acquired_image_queue_wait_count="$(jq -r '.av1_acquired_image_queue_wait_count // 0' "$runtime_json")"
acquired_image_queue_wait_elapsed_us="$(jq -r '.av1_acquired_image_queue_wait_elapsed_us // 0' "$runtime_json")"
acquired_image_queue_wait_max_us="$(jq -r '.av1_acquired_image_queue_wait_max_us // 0' "$runtime_json")"
acquired_image_queue_acquire_elapsed_us="$(jq -r '.av1_acquired_image_queue_acquire_elapsed_us // 0' "$runtime_json")"
acquired_image_queue_acquire_max_us="$(jq -r '.av1_acquired_image_queue_acquire_max_us // 0' "$runtime_json")"
preacquire_target_depth="$(jq -r '.av1_preacquire_target_depth // 0' "$runtime_json")"
preacquire_grace_wait_us="$(jq -r '.av1_preacquire_grace_wait_us // 0' "$runtime_json")"
preacquire_attempt_count="$(jq -r '.av1_preacquire_attempt_count // 0' "$runtime_json")"
preacquire_hit_count="$(jq -r '.av1_preacquire_hit_count // 0' "$runtime_json")"
preacquire_not_ready_count="$(jq -r '.av1_preacquire_not_ready_count // 0' "$runtime_json")"
preacquire_elapsed_us="$(jq -r '.av1_preacquire_elapsed_us // 0' "$runtime_json")"
preacquire_max_us="$(jq -r '.av1_preacquire_max_us // 0' "$runtime_json")"
hidden_decode_sync_strategy="$(jq -r '.av1_hidden_decode_sync_strategy // "none"' "$runtime_json")"
hidden_decode_handoff_slot_count="$(jq -r '.av1_hidden_decode_handoff_slot_count // 0' "$runtime_json")"
hidden_decode_async_handoff_count="$(jq -r '.av1_hidden_decode_async_handoff_count // 0' "$runtime_json")"
hidden_decode_handoff_unavailable_count="$(jq -r '.av1_hidden_decode_handoff_unavailable_count // 0' "$runtime_json")"
hidden_decode_fence_wait_count="$(jq -r '.av1_hidden_decode_fence_wait_count // 0' "$runtime_json")"
hidden_decode_fence_wait_elapsed_us="$(jq -r '.av1_hidden_decode_fence_wait_elapsed_us // 0' "$runtime_json")"
hidden_decode_fence_wait_max_us="$(jq -r '.av1_hidden_decode_fence_wait_max_us // 0' "$runtime_json")"
hidden_decode_process_elapsed_us="$(jq -r '.av1_hidden_decode_process_elapsed_us // 0' "$runtime_json")"
hidden_decode_process_max_us="$(jq -r '.av1_hidden_decode_process_max_us // 0' "$runtime_json")"
hidden_decode_before_display_gap_count="$(jq -r '.av1_hidden_decode_before_display_gap_count // 0' "$runtime_json")"
hidden_decode_before_display_elapsed_us="$(jq -r '.av1_hidden_decode_before_display_elapsed_us // 0' "$runtime_json")"
hidden_decode_before_display_max_count="$(jq -r '.av1_hidden_decode_before_display_max_count // 0' "$runtime_json")"
hidden_decode_before_display_max_elapsed_us="$(jq -r '.av1_hidden_decode_before_display_max_elapsed_us // 0' "$runtime_json")"
hidden_lookahead_decode_count="$(jq -r '.av1_hidden_lookahead_decode_count // 0' "$runtime_json")"
hidden_lookahead_stop_present_count="$(jq -r '.av1_hidden_lookahead_stop_present_count // 0' "$runtime_json")"
hidden_lookahead_stop_output_hazard_count="$(jq -r '.av1_hidden_lookahead_stop_output_hazard_count // 0' "$runtime_json")"
hidden_lookahead_stop_unready_count="$(jq -r '.av1_hidden_lookahead_stop_unready_count // 0' "$runtime_json")"
visible_decode_ahead_strategy="$(jq -r '.av1_visible_decode_ahead_strategy // "none"' "$runtime_json")"
visible_decode_ahead_attempt_count="$(jq -r '.av1_visible_decode_ahead_attempt_count // 0' "$runtime_json")"
visible_decode_ahead_ready_count="$(jq -r '.av1_visible_decode_ahead_ready_count // 0' "$runtime_json")"
visible_decode_ahead_submit_count="$(jq -r '.av1_visible_decode_ahead_submit_count // 0' "$runtime_json")"
visible_decode_ahead_skip_show_existing_count="$(jq -r '.av1_visible_decode_ahead_skip_show_existing_count // 0' "$runtime_json")"
visible_decode_ahead_skip_unready_count="$(jq -r '.av1_visible_decode_ahead_skip_unready_count // 0' "$runtime_json")"
visible_decode_ahead_skip_output_hazard_count="$(jq -r '.av1_visible_decode_ahead_skip_output_hazard_count // 0' "$runtime_json")"
visible_decode_ahead_skip_bitstream_overlap_count="$(jq -r '.av1_visible_decode_ahead_skip_bitstream_overlap_count // 0' "$runtime_json")"
visible_decode_ahead_skip_display_slot_hazard_count="$(jq -r '.av1_visible_decode_ahead_skip_display_slot_hazard_count // 0' "$runtime_json")"
show_existing_display_cache_strategy="$(jq -r '.av1_show_existing_display_cache_strategy // "none"' "$runtime_json")"
show_existing_display_cache_lookup_count="$(jq -r '.av1_show_existing_display_cache_lookup_count // 0' "$runtime_json")"
show_existing_display_cache_hit_count="$(jq -r '.av1_show_existing_display_cache_hit_count // 0' "$runtime_json")"
show_existing_display_cache_miss_count="$(jq -r '.av1_show_existing_display_cache_miss_count // 0' "$runtime_json")"
show_existing_display_cache_busy_count="$(jq -r '.av1_show_existing_display_cache_busy_count // 0' "$runtime_json")"
show_existing_display_cache_stale_count="$(jq -r '.av1_show_existing_display_cache_stale_count // 0' "$runtime_json")"
show_existing_display_cache_update_count="$(jq -r '.av1_show_existing_display_cache_update_count // 0' "$runtime_json")"
show_existing_display_cache_invalidate_count="$(jq -r '.av1_show_existing_display_cache_invalidate_count // 0' "$runtime_json")"
show_existing_precopy_strategy="$(jq -r '.av1_show_existing_precopy_strategy // "none"' "$runtime_json")"
show_existing_precopy_attempt_count="$(jq -r '.av1_show_existing_precopy_attempt_count // 0' "$runtime_json")"
show_existing_precopy_submit_count="$(jq -r '.av1_show_existing_precopy_submit_count // 0' "$runtime_json")"
show_existing_precopy_hit_count="$(jq -r '.av1_show_existing_precopy_hit_count // 0' "$runtime_json")"
show_existing_precopy_miss_count="$(jq -r '.av1_show_existing_precopy_miss_count // 0' "$runtime_json")"
show_existing_precopy_skip_no_handoff_count="$(jq -r '.av1_show_existing_precopy_skip_no_handoff_count // 0' "$runtime_json")"
show_existing_precopy_skip_source_layout_count="$(jq -r '.av1_show_existing_precopy_skip_source_layout_count // 0' "$runtime_json")"
show_existing_precopy_skip_display_slot_busy_count="$(jq -r '.av1_show_existing_precopy_skip_display_slot_busy_count // 0' "$runtime_json")"
show_existing_precopy_stale_count="$(jq -r '.av1_show_existing_precopy_stale_count // 0' "$runtime_json")"
show_existing_precopy_invalidate_count="$(jq -r '.av1_show_existing_precopy_invalidate_count // 0' "$runtime_json")"
show_existing_direct_dpb_count="$(jq -r '.av1_show_existing_direct_dpb_count // 0' "$runtime_json")"
displayed_direct_dpb_count="$(jq -r '.av1_displayed_direct_dpb_count // 0' "$runtime_json")"
hidden_decode_queue_wait_count="$(jq -r '.av1_hidden_decode_queue_wait_count // 0' "$runtime_json")"
hidden_decode_queue_wait_elapsed_us="$(jq -r '.av1_hidden_decode_queue_wait_elapsed_us // 0' "$runtime_json")"
frame_context_count="$(jq -r '.av1_frame_context_count // 0' "$runtime_json")"
decode_command_ring_depth="$(jq -r '.av1_decode_command_ring_depth // 0' "$runtime_json")"
decode_pending_max_count="$(jq -r '.av1_decode_pending_max_count // 0' "$runtime_json")"
decode_deferred_hidden_count="$(jq -r '.av1_decode_deferred_hidden_count // 0' "$runtime_json")"
decode_slot_wait_count="$(jq -r '.av1_decode_slot_wait_count // 0' "$runtime_json")"
decode_slot_wait_elapsed_us="$(jq -r '.av1_decode_slot_wait_elapsed_us // 0' "$runtime_json")"
decode_slot_wait_max_us="$(jq -r '.av1_decode_slot_wait_max_us // 0' "$runtime_json")"
decode_hidden_slot_wait_count="$(jq -r '.av1_decode_hidden_slot_wait_count // 0' "$runtime_json")"
decode_hidden_slot_wait_elapsed_us="$(jq -r '.av1_decode_hidden_slot_wait_elapsed_us // 0' "$runtime_json")"
decode_hidden_slot_wait_max_us="$(jq -r '.av1_decode_hidden_slot_wait_max_us // 0' "$runtime_json")"
distinct_layers="$(jq -r '[.frames[]?.displayed_base_array_layer] | unique | length' "$runtime_json")"
bad_frames="$(jq -r '[.frames[]? | select((.show_existing_frame | not) and ((.tile_count <= 0) or (.src_buffer_range <= 0)))] | length' "$runtime_json")"
hidden_presented_frames="$(jq -r '[.frames[]? | select((.show_existing_frame | not) and (.show_frame == false))] | length' "$runtime_json")"
readback_frame_count="$(jq -r '[.frames[]? | select(.readback_y_hash != null and .readback_uv_hash != null)] | length' "$runtime_json")"
readback_y_distinct="$(jq -r '[.frames[]?.readback_y_hash | select(. != null)] | unique | length' "$runtime_json")"
readback_uv_distinct="$(jq -r '[.frames[]?.readback_uv_hash | select(. != null)] | unique | length' "$runtime_json")"
loop_boundary_reset_count="$(jq -r '.loop_boundary_reset_count // 0' "$runtime_json")"
playback_loop_count="$(jq -r '.playback_loop_count // 0' "$runtime_json")"
frame_count="$(jq -r '(.frames // []) | length' "$runtime_json")"
ready_prefix_count="$(jq -r '.ready_prefix_frame_count // 0' "$runtime_json")"
present_queue="$(jq -r '.present_queue_family_index // "none"' "$runtime_json")"
video_queue="$(jq -r '.video_decode_queue_family_index // "none"' "$runtime_json")"
sync_strategy="$(jq -r '.cross_queue_sync_strategy // "none"' "$runtime_json")"
driver_max_dpb_slots="$(jq -r '.driver_max_dpb_slots // "none"' "$runtime_json")"
stream_dpb_slots="$(jq -r '.stream_dpb_slots // 0' "$runtime_json")"
stream_max_active_reference_pictures="$(jq -r '.stream_max_active_reference_pictures // 0' "$runtime_json")"
session_max_dpb_slots="$(jq -r '.session_max_dpb_slots // 0' "$runtime_json")"
session_max_active_reference_pictures="$(jq -r '.session_max_active_reference_pictures // 0' "$runtime_json")"
present_mode="$(jq -r '.present_mode // "none"' "$runtime_json")"
pacing_strategy="$(jq -r '.pacing_strategy // "none"' "$runtime_json")"
expected_pacing_strategy="$(gilder_expected_pacing_strategy "$present_mode" "$target_fps")"
frame_sleep_count_value="$(jq -r '.frame_sleep_count // 0' "$runtime_json")"
bitstream_strategy="$(jq -r '.bitstream_buffer_strategy // "none"' "$runtime_json")"
bitstream_slot_count="$(jq -r '.bitstream_buffer_slot_count // 0' "$runtime_json")"
bitstream_slot_bytes="$(jq -r '.bitstream_buffer_slot_bytes // 0' "$runtime_json")"
bitstream_ring_capacity_bytes="$(jq -r '.bitstream_ring_capacity_bytes // 0' "$runtime_json")"
bitstream_ring_wrap_count="$(jq -r '.bitstream_ring_wrap_count // 0' "$runtime_json")"
bitstream_ring_allocation_count="$(jq -r '.bitstream_ring_allocation_count // 0' "$runtime_json")"
bitstream_window_payload_bytes="$(jq -r '.bitstream_window_payload_bytes // 0' "$runtime_json")"
bitstream_upload_count="$(jq -r '.bitstream_upload_count // 0' "$runtime_json")"
bitstream_uploaded_bytes="$(jq -r '.bitstream_uploaded_bytes // 0' "$runtime_json")"
queue_max_payload_bytes="$(jq -r '.av1_packet_queue_max_payload_bytes // 0' "$runtime_json")"
first_frame_key="$(jq -r '.frames[0].frame_type_label == "key"' "$runtime_json")"
key_frames="$(jq -r '[.frames[]? | select(.frame_type_label == "key")] | length' "$runtime_json")"
inter_frames="$(jq -r '[.frames[]? | select(.frame_type_label == "inter")] | length' "$runtime_json")"
show_existing_frames="$(jq -r '[.frames[]? | select(.show_existing_frame == true)] | length' "$runtime_json")"
max_reference_count="$(jq -r '[.frames[]? | .decode_reference_slot_count] | max // 0' "$runtime_json")"
loop_first_non_key_count="$(jq -r 'reduce (.frames // [])[] as $frame ({}; ($frame.playback_loop_index | tostring) as $loop | if has($loop) then . else .[$loop] = ($frame.frame_type_label == "key") end) | [to_entries[] | select(.value != true)] | length' "$runtime_json")"
swapchain_images="$(jq -r '.swapchain_image_count // 0' "$runtime_json")"
resource_bytes="$(jq -r '.video_resource_memory_bytes // 0' "$runtime_json")"

expected_frames="$playback_frames"
loop_gate_failed=0
if [[ "$expected_frames" -gt "$decode_prefix" && ( "$playback_loop_count" -le 1 || "$loop_boundary_reset_count" -lt 1 ) ]]; then
  loop_gate_failed=1
fi
bitstream_gate_failed=0
if [[ "$bitstream_strategy" != "fixed-capacity-persistent-mapped-ring" || "$bitstream_slot_count" -le 0 || "$bitstream_slot_bytes" -le 0 || "$bitstream_ring_capacity_bytes" -lt "$bitstream_slot_bytes" || "$bitstream_window_payload_bytes" -le 0 || "$bitstream_upload_count" -ne "$total_decoded_count" || "$bitstream_uploaded_bytes" -le 0 ]]; then
  bitstream_gate_failed=1
fi
if [[ "$decode_prefix" -gt 1 && ( "$bitstream_slot_count" -le 1 || "$bitstream_ring_capacity_bytes" -le "$bitstream_slot_bytes" ) ]]; then
  bitstream_gate_failed=1
fi
if [[ "$decode_prefix" -gt 8 && "$bitstream_slot_count" -ge "$decode_prefix" ]]; then
  bitstream_gate_failed=1
fi
input_gate_failed=0
if [[ "$queue_capacity" -le 0 || "$queue_pulled_count" -lt "$expected_frames" || "$queue_max_payload_bytes" -le 0 || "$queue_retained_payload_bytes" -ne 0 ]]; then
  input_gate_failed=1
fi
runtime_skipped_arbitrary_prefix=0
if [[ "$queue_bootstrap_discarded_temporal_units" -gt 0 || "$queue_loop_skip_temporal_units" -gt 0 ]]; then
  runtime_skipped_arbitrary_prefix=1
fi
arbitrary_prefix_handled=0
if [[ "$runtime_skipped_arbitrary_prefix" -eq 1 || "$arbitrary_entry_demux_dropped_prefix" -eq 1 ]]; then
  arbitrary_prefix_handled=1
fi
arbitrary_entry_gate_failed=0
if [[ "$arbitrary_entry_source" -eq 1 && ( "$arbitrary_prefix_handled" -ne 1 || "$first_frame_key" != "true" ) ]]; then
  arbitrary_entry_gate_failed=1
fi
loop_replay_gate_failed=0
if [[ "$require_loop_skip_replay" -eq 1 && ( "$queue_eos_count" -le 0 || "$queue_loop_count" -le 0 || "$playback_loop_count" -le 1 || "$loop_boundary_reset_count" -le 0 || "$arbitrary_prefix_handled" -ne 1 || "$first_frame_key" != "true" || "$loop_first_non_key_count" -ne 0 ) ]]; then
  loop_replay_gate_failed=1
fi

readback_gate_failed=0
if [[ "$readback_frame_count" -gt 1 && ( "$readback_y_distinct" -le 1 || "$readback_uv_distinct" -le 1 ) ]]; then
  readback_gate_failed=1
fi
if [[ "$require_readback_diversity" -eq 1 && ( "$readback_frame_count" -lt 2 || "$readback_y_distinct" -le 1 || "$readback_uv_distinct" -le 1 ) ]]; then
  readback_gate_failed=1
fi
pacing_gate_failed=0
if [[ "$pacing_strategy" != "$expected_pacing_strategy" ]]; then
  pacing_gate_failed=1
fi
dpb_gate_failed=0
if [[ "$driver_max_dpb_slots" == "none" || "$stream_dpb_slots" -le 0 || "$session_max_dpb_slots" -ne "$stream_dpb_slots" || "$session_max_active_reference_pictures" -le 0 || "$session_max_active_reference_pictures" -gt "$session_max_dpb_slots" || "$session_max_active_reference_pictures" -lt "$stream_max_active_reference_pictures" || "$distinct_layers" -gt "$session_max_dpb_slots" ]]; then
  dpb_gate_failed=1
fi
sync_strategy_gate_failed=0
case "$sync_strategy" in
  per-frame-binary-semaphore-decode-signal-present-wait|frame-context-ring-binary-semaphore-decode-signal-present-wait) ;;
  *) sync_strategy_gate_failed=1 ;;
esac
pts_delta_gate_failed=0
if [[ "$pts_delta_in_expected_range" != "true" || "$pts_delta_script_in_expected_range" != "true" || "$pts_delta_expected_min" != "$script_pts_delta_expected_min" || "$pts_delta_expected_max" != "$script_pts_delta_expected_max" ]]; then
  pts_delta_gate_failed=1
fi

if [[ "$requested_codec" != "$video_codec" || "$picture_format" != "$expected_picture_format" || "$presented_count" -ne "$expected_frames" || "$frame_count" -ne "$expected_frames" || "$ready_prefix_count" -ne "$decode_prefix" || "$requested_playback_count" -ne "$expected_frames" || $((decoded_count + handoff_count)) -ne "$expected_frames" || "$total_decoded_count" -ne $((decoded_count + hidden_decoded_count)) || "$configured" != "true" || "$processed_temporal_unit_count" -lt "$expected_frames" || "$distinct_layers" -le 1 || "$bad_frames" -ne 0 || "$hidden_presented_frames" -ne 0 || "$loop_gate_failed" -ne 0 || "$bitstream_gate_failed" -ne 0 || "$input_gate_failed" -ne 0 || "$arbitrary_entry_gate_failed" -ne 0 || "$loop_replay_gate_failed" -ne 0 || "$readback_gate_failed" -ne 0 || "$pacing_gate_failed" -ne 0 || "$dpb_gate_failed" -ne 0 || "$sync_strategy_gate_failed" -ne 0 || "$pts_delta_gate_failed" -ne 0 || "$present_queue" == "none" || "$video_queue" == "none" || "$swapchain_images" -lt 2 || "$resource_bytes" -le 0 ]]; then
  {
    printf 'FAIL: native Vulkan AV1 ready-prefix visible runtime output was not valid\n'
    printf 'requested_codec: %s\n' "$requested_codec"
    printf 'picture_format: %s\n' "$picture_format"
    printf 'expected_picture_format: %s\n' "$expected_picture_format"
    printf 'decoded_frame_count: %s\n' "$decoded_count"
    printf 'hidden_decoded_frame_count: %s\n' "$hidden_decoded_count"
    printf 'total_decoded_frame_count: %s\n' "$total_decoded_count"
    printf 'displayed_handoff_frame_count: %s\n' "$handoff_count"
    printf 'presented_frame_count: %s\n' "$presented_count"
    printf 'frame_count: %s\n' "$frame_count"
    printf 'ready_prefix_frame_count: %s\n' "$ready_prefix_count"
    printf 'requested_decode_prefix: %s\n' "$decode_prefix"
    printf 'expected_playback_frames: %s\n' "$expected_frames"
    printf 'ready_prefix_loop_period_ms: %s\n' "$ready_prefix_loop_period_ms"
    printf 'requested_playback_frame_count: %s\n' "$requested_playback_count"
    printf 'processed_temporal_unit_count: %s\n' "$processed_temporal_unit_count"
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
    printf 'pts_delta_min_ms: %s\n' "$pts_delta_min"
    printf 'pts_delta_max_ms: %s\n' "$pts_delta_max"
    printf 'pts_delta_expected_min_ms: %s\n' "$pts_delta_expected_min"
    printf 'pts_delta_expected_max_ms: %s\n' "$pts_delta_expected_max"
    printf 'pts_delta_in_expected_range: %s\n' "$pts_delta_in_expected_range"
    printf 'pts_delta_script_expected_min_ms: %s\n' "$script_pts_delta_expected_min"
    printf 'pts_delta_script_expected_max_ms: %s\n' "$script_pts_delta_expected_max"
    printf 'pts_delta_script_in_expected_range: %s\n' "$pts_delta_script_in_expected_range"
    printf 'pts_delta_gate_failed: %s\n' "$pts_delta_gate_failed"
    printf 'configured: %s\n' "$configured"
    printf 'av1_packet_queue_capacity: %s\n' "$queue_capacity"
    printf 'av1_packet_queue_pulled_count: %s\n' "$queue_pulled_count"
    printf 'av1_packet_queue_eos_count: %s\n' "$queue_eos_count"
    printf 'av1_packet_queue_loop_count: %s\n' "$queue_loop_count"
    printf 'av1_packet_queue_loop_skip_temporal_units: %s\n' "$queue_loop_skip_temporal_units"
    printf 'av1_packet_queue_bootstrap_discarded_temporal_units: %s\n' "$queue_bootstrap_discarded_temporal_units"
    printf 'av1_packet_queue_retained_payload_bytes: %s\n' "$queue_retained_payload_bytes"
    printf 'av1_display_handoff_strategy: %s\n' "$display_handoff_strategy"
    printf 'av1_display_ring_slot_count: %s\n' "$display_ring_slot_count"
    printf 'av1_display_ring_memory_bytes: %s\n' "$display_ring_memory_bytes"
    printf 'av1_display_copy_count: %s\n' "$display_copy_count"
    printf 'av1_display_copy_elided_count: %s\n' "$display_copy_elided_count"
    printf 'av1_present_command_buffer_strategy: %s\n' "$present_command_buffer_strategy"
    printf 'av1_async_present_depth: %s\n' "$async_present_depth"
    printf 'av1_present_frame_queue_depth: %s\n' "$present_frame_queue_depth"
    printf 'av1_present_frame_preroll_count: %s\n' "$present_frame_preroll_count"
    printf 'av1_present_frame_queue_submit_count: %s\n' "$present_frame_queue_submit_count"
    printf 'av1_present_frame_queue_acquire_elapsed_us: %s\n' "$present_frame_queue_acquire_elapsed_us"
    printf 'av1_present_frame_queue_acquire_max_us: %s\n' "$present_frame_queue_acquire_max_us"
    printf 'av1_present_frame_queue_record_elapsed_us: %s\n' "$present_frame_queue_record_elapsed_us"
    printf 'av1_present_frame_queue_record_max_us: %s\n' "$present_frame_queue_record_max_us"
    printf 'av1_present_result_wait_count: %s\n' "$present_result_wait_count"
    printf 'av1_present_result_wait_elapsed_us: %s\n' "$present_result_wait_elapsed_us"
    printf 'av1_present_result_wait_max_us: %s\n' "$present_result_wait_max_us"
    printf 'av1_frame_context_selection_strategy: %s\n' "$frame_context_selection_strategy"
    printf 'av1_frame_context_ready_probe_count: %s\n' "$frame_context_ready_probe_count"
    printf 'av1_frame_context_ready_hit_count: %s\n' "$frame_context_ready_hit_count"
    printf 'av1_frame_context_fallback_count: %s\n' "$frame_context_fallback_count"
    printf 'av1_frame_context_fence_wait_count: %s\n' "$frame_context_fence_wait_count"
    printf 'av1_frame_context_fence_wait_elapsed_us: %s\n' "$frame_context_fence_wait_elapsed_us"
    printf 'av1_frame_context_fence_wait_max_us: %s\n' "$frame_context_fence_wait_max_us"
    printf 'av1_display_ring_slot_wait_count: %s\n' "$display_ring_slot_wait_count"
    printf 'av1_display_ring_slot_wait_elapsed_us: %s\n' "$display_ring_slot_wait_elapsed_us"
    printf 'av1_display_ring_slot_wait_max_us: %s\n' "$display_ring_slot_wait_max_us"
    printf 'av1_display_ring_slot_reuse_sync_strategy: %s\n' "$display_ring_slot_reuse_sync_strategy"
    printf 'av1_display_ring_slot_gpu_wait_count: %s\n' "$display_ring_slot_gpu_wait_count"
    printf 'av1_display_ring_slot_gpu_signal_count: %s\n' "$display_ring_slot_gpu_signal_count"
    printf 'av1_display_ring_slot_selection_strategy: %s\n' "$display_ring_slot_selection_strategy"
    printf 'av1_display_ring_ready_probe_count: %s\n' "$display_ring_ready_probe_count"
    printf 'av1_display_ring_ready_hit_count: %s\n' "$display_ring_ready_hit_count"
    printf 'av1_display_ring_ready_fallback_count: %s\n' "$display_ring_ready_fallback_count"
    printf 'av1_swapchain_image_wait_count: %s\n' "$swapchain_image_wait_count"
    printf 'av1_swapchain_image_wait_elapsed_us: %s\n' "$swapchain_image_wait_elapsed_us"
    printf 'av1_swapchain_image_wait_max_us: %s\n' "$swapchain_image_wait_max_us"
    printf 'av1_acquire_not_ready_count: %s\n' "$acquire_not_ready_count"
    printf 'av1_acquire_wait_present_result_count: %s\n' "$acquire_wait_present_result_count"
    printf 'av1_acquire_wait_present_result_elapsed_us: %s\n' "$acquire_wait_present_result_elapsed_us"
    printf 'av1_acquire_wait_present_result_max_us: %s\n' "$acquire_wait_present_result_max_us"
    printf 'av1_acquired_image_queue_slot_count: %s\n' "$acquired_image_queue_slot_count"
    printf 'av1_acquired_image_queue_target_depth: %s\n' "$acquired_image_queue_target_depth"
    printf 'av1_acquired_image_queue_attempt_count: %s\n' "$acquired_image_queue_attempt_count"
    printf 'av1_acquired_image_queue_hit_count: %s\n' "$acquired_image_queue_hit_count"
    printf 'av1_acquired_image_queue_miss_count: %s\n' "$acquired_image_queue_miss_count"
    printf 'av1_acquired_image_queue_wait_count: %s\n' "$acquired_image_queue_wait_count"
    printf 'av1_acquired_image_queue_wait_elapsed_us: %s\n' "$acquired_image_queue_wait_elapsed_us"
    printf 'av1_acquired_image_queue_wait_max_us: %s\n' "$acquired_image_queue_wait_max_us"
    printf 'av1_acquired_image_queue_acquire_elapsed_us: %s\n' "$acquired_image_queue_acquire_elapsed_us"
    printf 'av1_acquired_image_queue_acquire_max_us: %s\n' "$acquired_image_queue_acquire_max_us"
    printf 'av1_preacquire_target_depth: %s\n' "$preacquire_target_depth"
    printf 'av1_preacquire_grace_wait_us: %s\n' "$preacquire_grace_wait_us"
    printf 'av1_preacquire_attempt_count: %s\n' "$preacquire_attempt_count"
    printf 'av1_preacquire_hit_count: %s\n' "$preacquire_hit_count"
    printf 'av1_preacquire_not_ready_count: %s\n' "$preacquire_not_ready_count"
    printf 'av1_preacquire_elapsed_us: %s\n' "$preacquire_elapsed_us"
    printf 'av1_preacquire_max_us: %s\n' "$preacquire_max_us"
    printf 'av1_hidden_decode_sync_strategy: %s\n' "$hidden_decode_sync_strategy"
    printf 'av1_hidden_decode_handoff_slot_count: %s\n' "$hidden_decode_handoff_slot_count"
    printf 'av1_hidden_decode_async_handoff_count: %s\n' "$hidden_decode_async_handoff_count"
    printf 'av1_hidden_decode_handoff_unavailable_count: %s\n' "$hidden_decode_handoff_unavailable_count"
    printf 'av1_hidden_decode_fence_wait_count: %s\n' "$hidden_decode_fence_wait_count"
    printf 'av1_hidden_decode_fence_wait_elapsed_us: %s\n' "$hidden_decode_fence_wait_elapsed_us"
    printf 'av1_hidden_decode_fence_wait_max_us: %s\n' "$hidden_decode_fence_wait_max_us"
    printf 'av1_hidden_decode_process_elapsed_us: %s\n' "$hidden_decode_process_elapsed_us"
    printf 'av1_hidden_decode_process_max_us: %s\n' "$hidden_decode_process_max_us"
    printf 'av1_hidden_decode_before_display_gap_count: %s\n' "$hidden_decode_before_display_gap_count"
    printf 'av1_hidden_decode_before_display_elapsed_us: %s\n' "$hidden_decode_before_display_elapsed_us"
    printf 'av1_hidden_decode_before_display_max_count: %s\n' "$hidden_decode_before_display_max_count"
    printf 'av1_hidden_decode_before_display_max_elapsed_us: %s\n' "$hidden_decode_before_display_max_elapsed_us"
    printf 'av1_hidden_lookahead_decode_count: %s\n' "$hidden_lookahead_decode_count"
    printf 'av1_hidden_lookahead_stop_present_count: %s\n' "$hidden_lookahead_stop_present_count"
    printf 'av1_hidden_lookahead_stop_output_hazard_count: %s\n' "$hidden_lookahead_stop_output_hazard_count"
    printf 'av1_hidden_lookahead_stop_unready_count: %s\n' "$hidden_lookahead_stop_unready_count"
    printf 'av1_visible_decode_ahead_strategy: %s\n' "$visible_decode_ahead_strategy"
    printf 'av1_visible_decode_ahead_attempt_count: %s\n' "$visible_decode_ahead_attempt_count"
    printf 'av1_visible_decode_ahead_ready_count: %s\n' "$visible_decode_ahead_ready_count"
    printf 'av1_visible_decode_ahead_submit_count: %s\n' "$visible_decode_ahead_submit_count"
    printf 'av1_visible_decode_ahead_skip_show_existing_count: %s\n' "$visible_decode_ahead_skip_show_existing_count"
    printf 'av1_visible_decode_ahead_skip_unready_count: %s\n' "$visible_decode_ahead_skip_unready_count"
    printf 'av1_visible_decode_ahead_skip_output_hazard_count: %s\n' "$visible_decode_ahead_skip_output_hazard_count"
    printf 'av1_visible_decode_ahead_skip_bitstream_overlap_count: %s\n' "$visible_decode_ahead_skip_bitstream_overlap_count"
    printf 'av1_visible_decode_ahead_skip_display_slot_hazard_count: %s\n' "$visible_decode_ahead_skip_display_slot_hazard_count"
    printf 'av1_show_existing_display_cache_strategy: %s\n' "$show_existing_display_cache_strategy"
    printf 'av1_show_existing_display_cache_lookup_count: %s\n' "$show_existing_display_cache_lookup_count"
    printf 'av1_show_existing_display_cache_hit_count: %s\n' "$show_existing_display_cache_hit_count"
    printf 'av1_show_existing_display_cache_miss_count: %s\n' "$show_existing_display_cache_miss_count"
    printf 'av1_show_existing_display_cache_busy_count: %s\n' "$show_existing_display_cache_busy_count"
    printf 'av1_show_existing_display_cache_stale_count: %s\n' "$show_existing_display_cache_stale_count"
    printf 'av1_show_existing_display_cache_update_count: %s\n' "$show_existing_display_cache_update_count"
    printf 'av1_show_existing_display_cache_invalidate_count: %s\n' "$show_existing_display_cache_invalidate_count"
    printf 'av1_show_existing_precopy_strategy: %s\n' "$show_existing_precopy_strategy"
    printf 'av1_show_existing_precopy_attempt_count: %s\n' "$show_existing_precopy_attempt_count"
    printf 'av1_show_existing_precopy_submit_count: %s\n' "$show_existing_precopy_submit_count"
    printf 'av1_show_existing_precopy_hit_count: %s\n' "$show_existing_precopy_hit_count"
    printf 'av1_show_existing_precopy_miss_count: %s\n' "$show_existing_precopy_miss_count"
    printf 'av1_show_existing_precopy_skip_no_handoff_count: %s\n' "$show_existing_precopy_skip_no_handoff_count"
    printf 'av1_show_existing_precopy_skip_source_layout_count: %s\n' "$show_existing_precopy_skip_source_layout_count"
    printf 'av1_show_existing_precopy_skip_display_slot_busy_count: %s\n' "$show_existing_precopy_skip_display_slot_busy_count"
    printf 'av1_show_existing_precopy_stale_count: %s\n' "$show_existing_precopy_stale_count"
    printf 'av1_show_existing_precopy_invalidate_count: %s\n' "$show_existing_precopy_invalidate_count"
    printf 'av1_show_existing_direct_dpb_count: %s\n' "$show_existing_direct_dpb_count"
    printf 'av1_displayed_direct_dpb_count: %s\n' "$displayed_direct_dpb_count"
    printf 'av1_hidden_decode_queue_wait_count: %s\n' "$hidden_decode_queue_wait_count"
    printf 'av1_hidden_decode_queue_wait_elapsed_us: %s\n' "$hidden_decode_queue_wait_elapsed_us"
    printf 'av1_frame_context_count: %s\n' "$frame_context_count"
    printf 'av1_decode_command_ring_depth: %s\n' "$decode_command_ring_depth"
    printf 'av1_decode_pending_max_count: %s\n' "$decode_pending_max_count"
    printf 'av1_decode_deferred_hidden_count: %s\n' "$decode_deferred_hidden_count"
    printf 'av1_decode_slot_wait_count: %s\n' "$decode_slot_wait_count"
    printf 'av1_decode_slot_wait_elapsed_us: %s\n' "$decode_slot_wait_elapsed_us"
    printf 'av1_decode_slot_wait_max_us: %s\n' "$decode_slot_wait_max_us"
    printf 'av1_decode_hidden_slot_wait_count: %s\n' "$decode_hidden_slot_wait_count"
    printf 'av1_decode_hidden_slot_wait_elapsed_us: %s\n' "$decode_hidden_slot_wait_elapsed_us"
    printf 'av1_decode_hidden_slot_wait_max_us: %s\n' "$decode_hidden_slot_wait_max_us"
    printf 'distinct_displayed_layers: %s\n' "$distinct_layers"
    printf 'bad_frames: %s\n' "$bad_frames"
    printf 'hidden_presented_frames: %s\n' "$hidden_presented_frames"
    printf 'loop_gate_failed: %s\n' "$loop_gate_failed"
    printf 'bitstream_gate_failed: %s\n' "$bitstream_gate_failed"
    printf 'input_gate_failed: %s\n' "$input_gate_failed"
    printf 'arbitrary_entry_source: %s\n' "$([[ "$arbitrary_entry_source" -eq 1 ]] && printf yes || printf no)"
    printf 'arbitrary_entry_offset: %s\n' "${arbitrary_entry_offset:-none}"
    printf 'arbitrary_entry_demux_dropped_prefix: %s\n' "$([[ "$arbitrary_entry_demux_dropped_prefix" -eq 1 ]] && printf yes || printf no)"
    printf 'arbitrary_entry_first_decodable_pts: %s\n' "$arbitrary_entry_first_decodable_pts"
    printf 'arbitrary_entry_first_key_pts: %s\n' "$arbitrary_entry_first_key_pts"
    printf 'arbitrary_entry_probe_status: %s\n' "$arbitrary_entry_probe_status"
    printf 'arbitrary_entry_probe_log: %s\n' "${arbitrary_entry_probe_log:-none}"
    printf 'generated_source_cache_dir: %s\n' "$generated_dir"
    printf 'runtime_skipped_arbitrary_prefix: %s\n' "$([[ "$runtime_skipped_arbitrary_prefix" -eq 1 ]] && printf yes || printf no)"
    printf 'arbitrary_prefix_handled: %s\n' "$([[ "$arbitrary_prefix_handled" -eq 1 ]] && printf yes || printf no)"
    printf 'arbitrary_entry_gate_failed: %s\n' "$arbitrary_entry_gate_failed"
    printf 'loop_replay_gate_failed: %s\n' "$loop_replay_gate_failed"
    printf 'require_loop_skip_replay: %s\n' "$([[ "$require_loop_skip_replay" -eq 1 ]] && printf yes || printf no)"
    printf 'first_frame_key: %s\n' "$first_frame_key"
    printf 'loop_first_non_key_count: %s\n' "$loop_first_non_key_count"
    printf 'readback_frame_count: %s\n' "$readback_frame_count"
    printf 'readback_y_distinct: %s\n' "$readback_y_distinct"
    printf 'readback_uv_distinct: %s\n' "$readback_uv_distinct"
    printf 'readback_gate_failed: %s\n' "$readback_gate_failed"
    printf 'require_readback_diversity: %s\n' "$([[ "$require_readback_diversity" -eq 1 ]] && printf yes || printf no)"
    printf 'key_frames: %s\n' "$key_frames"
    printf 'inter_frames: %s\n' "$inter_frames"
    printf 'show_existing_frames: %s\n' "$show_existing_frames"
    printf 'max_reference_count: %s\n' "$max_reference_count"
    printf 'present_queue: %s\n' "$present_queue"
    printf 'video_queue: %s\n' "$video_queue"
    printf 'cross_queue_sync_strategy: %s\n' "$sync_strategy"
    printf 'driver_max_dpb_slots: %s\n' "$driver_max_dpb_slots"
    printf 'stream_dpb_slots: %s\n' "$stream_dpb_slots"
    printf 'stream_max_active_reference_pictures: %s\n' "$stream_max_active_reference_pictures"
    printf 'session_max_dpb_slots: %s\n' "$session_max_dpb_slots"
    printf 'session_max_active_reference_pictures: %s\n' "$session_max_active_reference_pictures"
    printf 'present_mode: %s\n' "$present_mode"
    printf 'pacing_strategy: %s\n' "$pacing_strategy"
    printf 'expected_pacing_strategy: %s\n' "$expected_pacing_strategy"
    printf 'frame_sleep_count: %s\n' "$frame_sleep_count_value"
    printf 'pacing_gate_failed: %s\n' "$pacing_gate_failed"
    printf 'dpb_gate_failed: %s\n' "$dpb_gate_failed"
    printf 'bitstream_buffer_strategy: %s\n' "$bitstream_strategy"
    printf 'bitstream_buffer_slot_count: %s\n' "$bitstream_slot_count"
    printf 'bitstream_buffer_slot_bytes: %s\n' "$bitstream_slot_bytes"
    printf 'bitstream_ring_capacity_bytes: %s\n' "$bitstream_ring_capacity_bytes"
    printf 'bitstream_ring_wrap_count: %s\n' "$bitstream_ring_wrap_count"
    printf 'bitstream_ring_allocation_count: %s\n' "$bitstream_ring_allocation_count"
    printf 'bitstream_window_payload_bytes: %s\n' "$bitstream_window_payload_bytes"
    printf 'bitstream_upload_count: %s\n' "$bitstream_upload_count"
    printf 'bitstream_uploaded_bytes: %s\n' "$bitstream_uploaded_bytes"
    printf 'swapchain_images: %s\n' "$swapchain_images"
    printf 'video_resource_memory_bytes: %s\n' "$resource_bytes"
    printf 'runtime_json: %s\n' "$runtime_json"
  } | tee "$summary"
  exit 1
fi

{
  printf 'PASS: native Vulkan AV1 ready-prefix visible runtime passed\n'
  printf 'source: %s\n' "$source"
  printf 'generated_source: %s\n' "$([[ "$generated_source" -eq 1 ]] && printf yes || printf no)"
  printf 'generated_source_frames: %s\n' "$([[ "$generated_source" -eq 1 ]] && printf '%s' "$frames" || printf none)"
  printf 'generated_source_duration_seconds: %s\n' "$([[ "$generated_source" -eq 1 ]] && printf '%s' "$source_duration_seconds" || printf none)"
  printf 'generated_source_frames_explicit: %s\n' "$([[ "$frames_explicit" -eq 1 ]] && printf yes || printf no)"
  printf 'generated_source_pattern: %s\n' "$([[ "$generated_source" -eq 1 ]] && printf 'testsrc2-continuous-av1-main%s' "$bit_depth" || printf none)"
  printf 'generated_source_cache_dir: %s\n' "$generated_dir"
  printf 'arbitrary_entry_source: %s\n' "$([[ "$arbitrary_entry_source" -eq 1 ]] && printf yes || printf no)"
  printf 'arbitrary_entry_offset: %s\n' "${arbitrary_entry_offset:-none}"
  printf 'arbitrary_entry_demux_dropped_prefix: %s\n' "$([[ "$arbitrary_entry_demux_dropped_prefix" -eq 1 ]] && printf yes || printf no)"
  printf 'arbitrary_entry_first_decodable_pts: %s\n' "$arbitrary_entry_first_decodable_pts"
  printf 'arbitrary_entry_first_key_pts: %s\n' "$arbitrary_entry_first_key_pts"
  printf 'arbitrary_entry_probe_status: %s\n' "$arbitrary_entry_probe_status"
  printf 'arbitrary_entry_probe_log: %s\n' "${arbitrary_entry_probe_log:-none}"
  printf 'runtime_skipped_arbitrary_prefix: %s\n' "$([[ "$runtime_skipped_arbitrary_prefix" -eq 1 ]] && printf yes || printf no)"
  printf 'arbitrary_prefix_handled: %s\n' "$([[ "$arbitrary_prefix_handled" -eq 1 ]] && printf yes || printf no)"
  printf 'require_loop_skip_replay: %s\n' "$([[ "$require_loop_skip_replay" -eq 1 ]] && printf yes || printf no)"
  printf 'require_readback_diversity: %s\n' "$([[ "$require_readback_diversity" -eq 1 ]] && printf yes || printf no)"
  printf 'selected_device: %s\n' "$(jq -r '.selected_physical_device_name' "$runtime_json")"
  printf 'requested_output_name: %s\n' "${output_name:-auto}"
  printf 'surface_logical_size: %s\n' "$(jq -c '.wayland_surface_logical_size' "$runtime_json")"
  printf 'surface_buffer_size: %s\n' "$(jq -c '.wayland_surface_buffer_size' "$runtime_json")"
  printf 'source_extent: %s\n' "$(jq -c '.source_extent' "$runtime_json")"
  printf 'swapchain_extent: %s\n' "$(jq -c '.swapchain_extent' "$runtime_json")"
  printf 'swapchain_format: %s\n' "$(jq -r '.swapchain_format' "$runtime_json")"
  printf 'present_mode: %s\n' "$present_mode"
  printf 'runtime_elapsed_ms: %s\n' "$(jq -r '.runtime_elapsed_ms' "$runtime_json")"
  printf 'requested_codec: %s\n' "$requested_codec"
  printf 'picture_format: %s\n' "$picture_format"
  printf 'ready_prefix_frame_count: %s\n' "$ready_prefix_count"
  printf 'ready_prefix_loop_period_ms: %s\n' "$ready_prefix_loop_period_ms"
  printf 'requested_playback_frame_count: %s\n' "$requested_playback_count"
  printf 'decoded_frame_count: %s\n' "$decoded_count"
  printf 'hidden_decoded_frame_count: %s\n' "$hidden_decoded_count"
  printf 'total_decoded_frame_count: %s\n' "$total_decoded_count"
  printf 'displayed_handoff_frame_count: %s\n' "$handoff_count"
  printf 'presented_frame_count: %s\n' "$presented_count"
  printf 'processed_temporal_unit_count: %s\n' "$processed_temporal_unit_count"
  printf 'playback_loop_count: %s\n' "$playback_loop_count"
  printf 'loop_boundary_reset_count: %s\n' "$loop_boundary_reset_count"
  printf 'key_frames: %s\n' "$key_frames"
  printf 'inter_frames: %s\n' "$inter_frames"
  printf 'show_existing_frames: %s\n' "$show_existing_frames"
  printf 'max_reference_count: %s\n' "$max_reference_count"
  printf 'first_frame_key: %s\n' "$first_frame_key"
  printf 'loop_first_non_key_count: %s\n' "$loop_first_non_key_count"
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
  printf 'cross_queue_sync_strategy: %s\n' "$sync_strategy"
  printf 'driver_max_dpb_slots: %s\n' "$driver_max_dpb_slots"
  printf 'stream_dpb_slots: %s\n' "$stream_dpb_slots"
  printf 'stream_max_active_reference_pictures: %s\n' "$stream_max_active_reference_pictures"
  printf 'session_max_dpb_slots: %s\n' "$session_max_dpb_slots"
  printf 'session_max_active_reference_pictures: %s\n' "$session_max_active_reference_pictures"
  printf 'bitstream_buffer_strategy: %s\n' "$bitstream_strategy"
  printf 'bitstream_buffer_slot_count: %s\n' "$bitstream_slot_count"
  printf 'bitstream_buffer_slot_bytes: %s\n' "$bitstream_slot_bytes"
  printf 'bitstream_ring_capacity_bytes: %s\n' "$bitstream_ring_capacity_bytes"
  printf 'bitstream_ring_min_offset_alignment: %s\n' "$(jq -r '.bitstream_ring_min_offset_alignment // 0' "$runtime_json")"
  printf 'bitstream_ring_min_size_alignment: %s\n' "$(jq -r '.bitstream_ring_min_size_alignment // 0' "$runtime_json")"
  printf 'bitstream_ring_wrap_count: %s\n' "$bitstream_ring_wrap_count"
  printf 'bitstream_ring_allocation_count: %s\n' "$bitstream_ring_allocation_count"
  printf 'bitstream_window_payload_bytes: %s\n' "$bitstream_window_payload_bytes"
  printf 'bitstream_upload_count: %s\n' "$bitstream_upload_count"
  printf 'bitstream_uploaded_bytes: %s\n' "$bitstream_uploaded_bytes"
  printf 'av1_packet_queue_capacity: %s\n' "$queue_capacity"
  printf 'av1_packet_queue_pulled_count: %s\n' "$queue_pulled_count"
  printf 'readback_frame_count: %s\n' "$readback_frame_count"
  printf 'readback_y_distinct: %s\n' "$readback_y_distinct"
  printf 'readback_uv_distinct: %s\n' "$readback_uv_distinct"
  printf 'av1_packet_queue_eos_count: %s\n' "$queue_eos_count"
  printf 'av1_packet_queue_loop_count: %s\n' "$queue_loop_count"
  printf 'av1_packet_queue_loop_skip_temporal_units: %s\n' "$queue_loop_skip_temporal_units"
  printf 'av1_packet_queue_bootstrap_discarded_temporal_units: %s\n' "$queue_bootstrap_discarded_temporal_units"
  printf 'av1_packet_queue_max_payload_bytes: %s\n' "$queue_max_payload_bytes"
  printf 'av1_packet_queue_retained_payload_bytes: %s\n' "$queue_retained_payload_bytes"
  printf 'av1_display_handoff_strategy: %s\n' "$display_handoff_strategy"
  printf 'av1_display_ring_slot_count: %s\n' "$display_ring_slot_count"
  printf 'av1_display_ring_memory_bytes: %s\n' "$display_ring_memory_bytes"
  printf 'av1_display_copy_count: %s\n' "$display_copy_count"
  printf 'av1_display_copy_elided_count: %s\n' "$display_copy_elided_count"
  printf 'av1_present_command_buffer_strategy: %s\n' "$present_command_buffer_strategy"
  printf 'av1_async_present_depth: %s\n' "$async_present_depth"
  printf 'av1_present_frame_queue_depth: %s\n' "$present_frame_queue_depth"
  printf 'av1_present_frame_preroll_count: %s\n' "$present_frame_preroll_count"
  printf 'av1_present_frame_queue_submit_count: %s\n' "$present_frame_queue_submit_count"
  printf 'av1_present_frame_queue_acquire_elapsed_us: %s\n' "$present_frame_queue_acquire_elapsed_us"
  printf 'av1_present_frame_queue_acquire_max_us: %s\n' "$present_frame_queue_acquire_max_us"
  printf 'av1_present_frame_queue_record_elapsed_us: %s\n' "$present_frame_queue_record_elapsed_us"
  printf 'av1_present_frame_queue_record_max_us: %s\n' "$present_frame_queue_record_max_us"
  printf 'av1_present_result_wait_count: %s\n' "$present_result_wait_count"
  printf 'av1_present_result_wait_elapsed_us: %s\n' "$present_result_wait_elapsed_us"
  printf 'av1_present_result_wait_max_us: %s\n' "$present_result_wait_max_us"
  printf 'av1_frame_context_selection_strategy: %s\n' "$frame_context_selection_strategy"
  printf 'av1_frame_context_ready_probe_count: %s\n' "$frame_context_ready_probe_count"
  printf 'av1_frame_context_ready_hit_count: %s\n' "$frame_context_ready_hit_count"
  printf 'av1_frame_context_fallback_count: %s\n' "$frame_context_fallback_count"
  printf 'av1_frame_context_fence_wait_count: %s\n' "$frame_context_fence_wait_count"
  printf 'av1_frame_context_fence_wait_elapsed_us: %s\n' "$frame_context_fence_wait_elapsed_us"
  printf 'av1_frame_context_fence_wait_max_us: %s\n' "$frame_context_fence_wait_max_us"
  printf 'av1_display_ring_slot_wait_count: %s\n' "$display_ring_slot_wait_count"
  printf 'av1_display_ring_slot_wait_elapsed_us: %s\n' "$display_ring_slot_wait_elapsed_us"
  printf 'av1_display_ring_slot_wait_max_us: %s\n' "$display_ring_slot_wait_max_us"
  printf 'av1_display_ring_slot_reuse_sync_strategy: %s\n' "$display_ring_slot_reuse_sync_strategy"
  printf 'av1_display_ring_slot_gpu_wait_count: %s\n' "$display_ring_slot_gpu_wait_count"
  printf 'av1_display_ring_slot_gpu_signal_count: %s\n' "$display_ring_slot_gpu_signal_count"
  printf 'av1_display_ring_slot_selection_strategy: %s\n' "$display_ring_slot_selection_strategy"
  printf 'av1_display_ring_ready_probe_count: %s\n' "$display_ring_ready_probe_count"
  printf 'av1_display_ring_ready_hit_count: %s\n' "$display_ring_ready_hit_count"
  printf 'av1_display_ring_ready_fallback_count: %s\n' "$display_ring_ready_fallback_count"
  printf 'av1_swapchain_image_wait_count: %s\n' "$swapchain_image_wait_count"
  printf 'av1_swapchain_image_wait_elapsed_us: %s\n' "$swapchain_image_wait_elapsed_us"
  printf 'av1_swapchain_image_wait_max_us: %s\n' "$swapchain_image_wait_max_us"
  printf 'av1_acquire_not_ready_count: %s\n' "$acquire_not_ready_count"
  printf 'av1_acquire_wait_present_result_count: %s\n' "$acquire_wait_present_result_count"
  printf 'av1_acquire_wait_present_result_elapsed_us: %s\n' "$acquire_wait_present_result_elapsed_us"
  printf 'av1_acquire_wait_present_result_max_us: %s\n' "$acquire_wait_present_result_max_us"
  printf 'av1_acquired_image_queue_slot_count: %s\n' "$acquired_image_queue_slot_count"
  printf 'av1_acquired_image_queue_target_depth: %s\n' "$acquired_image_queue_target_depth"
  printf 'av1_acquired_image_queue_attempt_count: %s\n' "$acquired_image_queue_attempt_count"
  printf 'av1_acquired_image_queue_hit_count: %s\n' "$acquired_image_queue_hit_count"
  printf 'av1_acquired_image_queue_miss_count: %s\n' "$acquired_image_queue_miss_count"
  printf 'av1_acquired_image_queue_wait_count: %s\n' "$acquired_image_queue_wait_count"
  printf 'av1_acquired_image_queue_wait_elapsed_us: %s\n' "$acquired_image_queue_wait_elapsed_us"
  printf 'av1_acquired_image_queue_wait_max_us: %s\n' "$acquired_image_queue_wait_max_us"
  printf 'av1_acquired_image_queue_acquire_elapsed_us: %s\n' "$acquired_image_queue_acquire_elapsed_us"
  printf 'av1_acquired_image_queue_acquire_max_us: %s\n' "$acquired_image_queue_acquire_max_us"
  printf 'av1_preacquire_target_depth: %s\n' "$preacquire_target_depth"
  printf 'av1_preacquire_grace_wait_us: %s\n' "$preacquire_grace_wait_us"
  printf 'av1_preacquire_attempt_count: %s\n' "$preacquire_attempt_count"
  printf 'av1_preacquire_hit_count: %s\n' "$preacquire_hit_count"
  printf 'av1_preacquire_not_ready_count: %s\n' "$preacquire_not_ready_count"
  printf 'av1_preacquire_elapsed_us: %s\n' "$preacquire_elapsed_us"
  printf 'av1_preacquire_max_us: %s\n' "$preacquire_max_us"
  printf 'av1_hidden_decode_sync_strategy: %s\n' "$hidden_decode_sync_strategy"
  printf 'av1_hidden_decode_handoff_slot_count: %s\n' "$hidden_decode_handoff_slot_count"
  printf 'av1_hidden_decode_async_handoff_count: %s\n' "$hidden_decode_async_handoff_count"
  printf 'av1_hidden_decode_handoff_unavailable_count: %s\n' "$hidden_decode_handoff_unavailable_count"
  printf 'av1_hidden_decode_fence_wait_count: %s\n' "$hidden_decode_fence_wait_count"
  printf 'av1_hidden_decode_fence_wait_elapsed_us: %s\n' "$hidden_decode_fence_wait_elapsed_us"
  printf 'av1_hidden_decode_fence_wait_max_us: %s\n' "$hidden_decode_fence_wait_max_us"
  printf 'av1_hidden_decode_process_elapsed_us: %s\n' "$hidden_decode_process_elapsed_us"
  printf 'av1_hidden_decode_process_max_us: %s\n' "$hidden_decode_process_max_us"
  printf 'av1_hidden_decode_before_display_gap_count: %s\n' "$hidden_decode_before_display_gap_count"
  printf 'av1_hidden_decode_before_display_elapsed_us: %s\n' "$hidden_decode_before_display_elapsed_us"
  printf 'av1_hidden_decode_before_display_max_count: %s\n' "$hidden_decode_before_display_max_count"
  printf 'av1_hidden_decode_before_display_max_elapsed_us: %s\n' "$hidden_decode_before_display_max_elapsed_us"
  printf 'av1_hidden_lookahead_decode_count: %s\n' "$hidden_lookahead_decode_count"
  printf 'av1_hidden_lookahead_stop_present_count: %s\n' "$hidden_lookahead_stop_present_count"
  printf 'av1_hidden_lookahead_stop_output_hazard_count: %s\n' "$hidden_lookahead_stop_output_hazard_count"
  printf 'av1_hidden_lookahead_stop_unready_count: %s\n' "$hidden_lookahead_stop_unready_count"
  printf 'av1_visible_decode_ahead_strategy: %s\n' "$visible_decode_ahead_strategy"
  printf 'av1_visible_decode_ahead_attempt_count: %s\n' "$visible_decode_ahead_attempt_count"
  printf 'av1_visible_decode_ahead_ready_count: %s\n' "$visible_decode_ahead_ready_count"
  printf 'av1_visible_decode_ahead_submit_count: %s\n' "$visible_decode_ahead_submit_count"
  printf 'av1_visible_decode_ahead_skip_show_existing_count: %s\n' "$visible_decode_ahead_skip_show_existing_count"
  printf 'av1_visible_decode_ahead_skip_unready_count: %s\n' "$visible_decode_ahead_skip_unready_count"
  printf 'av1_visible_decode_ahead_skip_output_hazard_count: %s\n' "$visible_decode_ahead_skip_output_hazard_count"
  printf 'av1_visible_decode_ahead_skip_bitstream_overlap_count: %s\n' "$visible_decode_ahead_skip_bitstream_overlap_count"
  printf 'av1_visible_decode_ahead_skip_display_slot_hazard_count: %s\n' "$visible_decode_ahead_skip_display_slot_hazard_count"
  printf 'av1_show_existing_display_cache_strategy: %s\n' "$show_existing_display_cache_strategy"
  printf 'av1_show_existing_display_cache_lookup_count: %s\n' "$show_existing_display_cache_lookup_count"
  printf 'av1_show_existing_display_cache_hit_count: %s\n' "$show_existing_display_cache_hit_count"
  printf 'av1_show_existing_display_cache_miss_count: %s\n' "$show_existing_display_cache_miss_count"
  printf 'av1_show_existing_display_cache_busy_count: %s\n' "$show_existing_display_cache_busy_count"
  printf 'av1_show_existing_display_cache_stale_count: %s\n' "$show_existing_display_cache_stale_count"
  printf 'av1_show_existing_display_cache_update_count: %s\n' "$show_existing_display_cache_update_count"
  printf 'av1_show_existing_display_cache_invalidate_count: %s\n' "$show_existing_display_cache_invalidate_count"
  printf 'av1_show_existing_precopy_strategy: %s\n' "$show_existing_precopy_strategy"
  printf 'av1_show_existing_precopy_attempt_count: %s\n' "$show_existing_precopy_attempt_count"
  printf 'av1_show_existing_precopy_submit_count: %s\n' "$show_existing_precopy_submit_count"
  printf 'av1_show_existing_precopy_hit_count: %s\n' "$show_existing_precopy_hit_count"
  printf 'av1_show_existing_precopy_miss_count: %s\n' "$show_existing_precopy_miss_count"
  printf 'av1_show_existing_precopy_skip_no_handoff_count: %s\n' "$show_existing_precopy_skip_no_handoff_count"
  printf 'av1_show_existing_precopy_skip_source_layout_count: %s\n' "$show_existing_precopy_skip_source_layout_count"
  printf 'av1_show_existing_precopy_skip_display_slot_busy_count: %s\n' "$show_existing_precopy_skip_display_slot_busy_count"
  printf 'av1_show_existing_precopy_stale_count: %s\n' "$show_existing_precopy_stale_count"
  printf 'av1_show_existing_precopy_invalidate_count: %s\n' "$show_existing_precopy_invalidate_count"
  printf 'av1_show_existing_direct_dpb_count: %s\n' "$show_existing_direct_dpb_count"
  printf 'av1_displayed_direct_dpb_count: %s\n' "$displayed_direct_dpb_count"
  printf 'av1_hidden_decode_queue_wait_count: %s\n' "$hidden_decode_queue_wait_count"
  printf 'av1_hidden_decode_queue_wait_elapsed_us: %s\n' "$hidden_decode_queue_wait_elapsed_us"
  printf 'av1_frame_context_count: %s\n' "$frame_context_count"
  printf 'av1_decode_command_ring_depth: %s\n' "$decode_command_ring_depth"
  printf 'av1_decode_pending_max_count: %s\n' "$decode_pending_max_count"
  printf 'av1_decode_deferred_hidden_count: %s\n' "$decode_deferred_hidden_count"
  printf 'av1_decode_slot_wait_count: %s\n' "$decode_slot_wait_count"
  printf 'av1_decode_slot_wait_elapsed_us: %s\n' "$decode_slot_wait_elapsed_us"
  printf 'av1_decode_slot_wait_max_us: %s\n' "$decode_slot_wait_max_us"
  printf 'av1_decode_hidden_slot_wait_count: %s\n' "$decode_hidden_slot_wait_count"
  printf 'av1_decode_hidden_slot_wait_elapsed_us: %s\n' "$decode_hidden_slot_wait_elapsed_us"
  printf 'av1_decode_hidden_slot_wait_max_us: %s\n' "$decode_hidden_slot_wait_max_us"
  printf 'frame_layers_head: %s\n' "$(jq -c '[.frames[0:32][]?.displayed_base_array_layer]' "$runtime_json")"
  printf 'frame_layers_tail: %s\n' "$(jq -c '[.frames[-32:][]?.displayed_base_array_layer]' "$runtime_json")"
  printf 'frame_temporal_units_head: %s\n' "$(jq -c '[.frames[0:32][]?.temporal_unit_index]' "$runtime_json")"
  printf 'frame_temporal_units_tail: %s\n' "$(jq -c '[.frames[-32:][]?.temporal_unit_index]' "$runtime_json")"
  printf 'frame_loop_indices_head: %s\n' "$(jq -c '[.frames[0:32][]?.playback_loop_index]' "$runtime_json")"
  printf 'frame_loop_indices_tail: %s\n' "$(jq -c '[.frames[-32:][]?.playback_loop_index]' "$runtime_json")"
  printf 'frame_types_head: %s\n' "$(jq -c '[.frames[0:32][]?.frame_type_label]' "$runtime_json")"
  printf 'frame_types_tail: %s\n' "$(jq -c '[.frames[-32:][]?.frame_type_label]' "$runtime_json")"
  printf 'pts_delta_min_ms: %s\n' "$pts_delta_min"
  printf 'pts_delta_max_ms: %s\n' "$pts_delta_max"
  printf 'pts_delta_expected_min_ms: %s\n' "$pts_delta_expected_min"
  printf 'pts_delta_expected_max_ms: %s\n' "$pts_delta_expected_max"
  printf 'pts_delta_in_expected_range: %s\n' "$pts_delta_in_expected_range"
  printf 'pts_delta_script_expected_min_ms: %s\n' "$script_pts_delta_expected_min"
  printf 'pts_delta_script_expected_max_ms: %s\n' "$script_pts_delta_expected_max"
  printf 'max_bitstream_upload_elapsed_us: %s\n' "$(jq -r '[.frames[]?.bitstream_upload_elapsed_us] | max // 0' "$runtime_json")"
  printf 'max_decode_elapsed_us: %s\n' "$(jq -r '[.frames[]?.decode_elapsed_us] | max // 0' "$runtime_json")"
  printf 'avg_acquire_elapsed_us: %s\n' "$(jq -r '[.frames[]?.acquire_elapsed_us] | add / length' "$runtime_json")"
  printf 'max_acquire_elapsed_us: %s\n' "$(jq -r '[.frames[]?.acquire_elapsed_us] | max // 0' "$runtime_json")"
  printf 'present_budget_us: %s\n' "$present_budget_us"
  printf 'acquire_over_budget_count: %s\n' "$acquire_over_budget_count"
  printf 'avg_record_elapsed_us: %s\n' "$(jq -r '[.frames[]?.record_elapsed_us] | add / length' "$runtime_json")"
  printf 'max_record_elapsed_us: %s\n' "$(jq -r '[.frames[]?.record_elapsed_us] | max // 0' "$runtime_json")"
  printf 'avg_submit_elapsed_us: %s\n' "$(jq -r '[.frames[]?.submit_elapsed_us] | add / length' "$runtime_json")"
  printf 'max_submit_elapsed_us: %s\n' "$(jq -r '[.frames[]?.submit_elapsed_us] | max // 0' "$runtime_json")"
  printf 'avg_queue_present_elapsed_us: %s\n' "$(jq -r '[.frames[]?.queue_present_elapsed_us] | add / length' "$runtime_json")"
  printf 'max_queue_present_elapsed_us: %s\n' "$(jq -r '[.frames[]?.queue_present_elapsed_us] | max // 0' "$runtime_json")"
  printf 'queue_present_over_budget_count: %s\n' "$queue_present_over_budget_count"
  printf 'max_present_elapsed_us: %s\n' "$(jq -r '[.frames[]?.present_elapsed_us] | max // 0' "$runtime_json")"
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
  printf 'runtime_json: %s\n' "$runtime_json"
} | tee "$summary"
