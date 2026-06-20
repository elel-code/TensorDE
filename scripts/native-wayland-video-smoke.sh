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
  --sink-throttle       Also enforce target with sink throttle-time.
  --pipeline <name>     appsink-mmap-probe, appsink-probe,
                        appsink-dmabuf-present, or explicit-h264-gl.
                        Legacy playbin/playbin3 require
                        --allow-legacy-waylandsink.
                        Default: appsink-mmap-probe.
  --allow-unsafe-dmabuf-present
                        Permit appsink-dmabuf-present, the manual linux-dmabuf
                        attach prototype that can destabilize compositors.
  --allow-legacy-waylandsink
                        Permit the deprecated playbin+waylandsink path only
                        for explicit comparison runs.
  --fit <mode>          cover, contain, stretch, or center. Default: cover.
  --layer <name>        background, bottom, top, or overlay. Default: bottom.
  --allow-foreground-layer
                        Permit top/overlay layers for short visual debugging.
  --output-name <name>  Bind the layer-shell surface to a wl_output name such
                        as HDMI-A-1. Required for >=200fps smoke runs unless
                        --allow-compositor-output is passed.
  --allow-compositor-output
                        Permit compositor-selected output for high-fps smoke.
  --decoder <policy>    auto, hardware-preferred, hardware-required, or software.
                        Default: hardware-preferred.
  --runtime-interval-ms <ms>
                        Native runtime JSON sample interval. Default: 1000.
  --debug-visible-frame
                        Present generated XRGB color bars through the same
                        dmabuf attach path to diagnose layer visibility.
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
pipeline="appsink-mmap-probe"
allow_legacy_waylandsink=0
allow_unsafe_dmabuf_present=0
fit="cover"
layer="bottom"
output_name=""
allow_compositor_output=0
allow_foreground_layer=0
decoder="hardware-preferred"
runtime_json_enabled=1
runtime_interval_ms=1000
debug_visible_frame=0
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
    --allow-legacy-waylandsink)
      allow_legacy_waylandsink=1
      shift
      ;;
    --allow-unsafe-dmabuf-present)
      allow_unsafe_dmabuf_present=1
      shift
      ;;
    --fit)
      [[ $# -ge 2 ]] || { echo "--fit requires a value" >&2; exit 2; }
      fit="$2"
      shift 2
      ;;
    --layer)
      [[ $# -ge 2 ]] || { echo "--layer requires a value" >&2; exit 2; }
      layer="$2"
      shift 2
      ;;
    --allow-foreground-layer)
      allow_foreground_layer=1
      shift
      ;;
    --output-name)
      [[ $# -ge 2 ]] || { echo "--output-name requires a value" >&2; exit 2; }
      output_name="$2"
      shift 2
      ;;
    --allow-compositor-output)
      allow_compositor_output=1
      shift
      ;;
    --decoder)
      [[ $# -ge 2 ]] || { echo "--decoder requires a value" >&2; exit 2; }
      decoder="$2"
      shift 2
      ;;
    --runtime-interval-ms)
      [[ $# -ge 2 ]] || { echo "--runtime-interval-ms requires milliseconds" >&2; exit 2; }
      runtime_interval_ms="$2"
      shift 2
      ;;
    --debug-visible-frame)
      debug_visible_frame=1
      shift
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

summarize_native_runtime_json() {
  local runtime_json_path="$1"
  local csv_path="$2"
  local summary_path="$3"

  if [[ ! -s "$runtime_json_path" ]]; then
    {
      printf 'native_runtime_rows: 0\n'
      printf 'actual_rendered_fps: none\n'
      printf 'actual_rendered_fps_status: unavailable\n'
    } > "$summary_path"
    printf 'sample,gst_state,rendered,dropped,qos_messages,qos_dropped_max,position_ms,video_fit,render_rectangle,linux_dmabuf_supported,linux_dmabuf_version,linux_dmabuf_modifier_count,linux_dmabuf_feedback_received,linux_dmabuf_feedback_source,linux_dmabuf_feedback_format_count,linux_dmabuf_feedback_tranche_count,appsink_pulled_samples,appsink_last_memory_count,appsink_last_memory_types,appsink_last_buffer_size,appsink_cuda_alloc_method,appsink_cuda_export_fd,appsink_cuda_export_successes,appsink_cuda_export_failures,appsink_allocation_queries,appsink_allocation_need_pool,appsink_allocation_pool_size,appsink_allocation_mmap_pool_proposals,appsink_allocation_mmap_pool_failures,appsink_allocation_last_result,runtime_elapsed_ms,selected_output_name,selected_output_logical_size,selected_output_current_mode,selected_output_refresh_millihertz,surface_logical_size,surface_scale,appsink_cuda_export_fds_closed,appsink_video_meta_format,appsink_video_meta_size,appsink_video_meta_planes,appsink_video_meta_offsets,appsink_video_meta_strides,appsink_video_meta_drm_fourcc,appsink_video_meta_drm_modifier,appsink_video_meta_attach_ready,appsink_video_meta_attach_blockers,dmabuf_feedback_supports_app_fourcc\n' > "$csv_path"
    return
  fi

  jq -r '
    (.appsink_probe.last_video_meta.drm_fourcc // null) as $appsink_drm_fourcc |
    [
      .pipeline.gst_state,
      (.sink_stats.rendered // ""),
      (.sink_stats.dropped // ""),
      (.pipeline.frame_stats.qos_messages // ""),
      (.pipeline.frame_stats.qos_dropped_max // ""),
      (.pipeline.position_ms // ""),
      (.video_fit // ""),
      (if .render_rectangle then
        "\(.render_rectangle.x):\(.render_rectangle.y):\(.render_rectangle.width):\(.render_rectangle.height)"
       else "" end),
      (.surface.dmabuf.supports_linux_dmabuf_protocol // ""),
      (.surface.dmabuf.linux_dmabuf_version // ""),
      (.surface.dmabuf.linux_dmabuf_modifier_count // ""),
      (.surface.dmabuf.linux_dmabuf_feedback_received // ""),
      (.surface.dmabuf.linux_dmabuf_feedback.source // ""),
      (.surface.dmabuf.linux_dmabuf_feedback.format_count // ""),
      (.surface.dmabuf.linux_dmabuf_feedback.tranche_count // ""),
      (.appsink_probe.pulled_samples // ""),
      (.appsink_probe.last_memory_count // ""),
      ((.appsink_probe.last_memory_types // []) | join("|")),
      (.appsink_probe.last_buffer_size // ""),
      (.appsink_probe.last_cuda_alloc_method // ""),
      (.appsink_probe.last_cuda_export_fd // ""),
      (.appsink_probe.cuda_export_successes // ""),
      (.appsink_probe.cuda_export_failures // ""),
      (.appsink_probe.allocation_queries // ""),
      (.appsink_probe.allocation_need_pool // ""),
      (.appsink_probe.allocation_pool_size // ""),
      (.appsink_probe.allocation_mmap_pool_proposals // ""),
      (.appsink_probe.allocation_mmap_pool_failures // ""),
      (.appsink_probe.allocation_last_result // ""),
      (.runtime_elapsed_ms // ""),
      (.surface.selected_output.name // ""),
      (if .surface.selected_output.logical_size then
        "\(.surface.selected_output.logical_size[0])x\(.surface.selected_output.logical_size[1])"
       else "" end),
      (if .surface.selected_output.current_mode then
        "\(.surface.selected_output.current_mode.width)x\(.surface.selected_output.current_mode.height)"
       else "" end),
      (.surface.selected_output.current_mode.refresh_millihertz // ""),
      (if .surface.logical_size then "\(.surface.logical_size[0])x\(.surface.logical_size[1])" else "" end),
      "\(.surface.scale_num // "")/\(.surface.scale_den // "")",
      (.appsink_probe.cuda_export_fds_closed // ""),
      (.appsink_probe.last_video_meta.format // ""),
      (if .appsink_probe.last_video_meta then
        "\(.appsink_probe.last_video_meta.width)x\(.appsink_probe.last_video_meta.height)"
       else "" end),
      (.appsink_probe.last_video_meta.n_planes // ""),
      ((.appsink_probe.last_video_meta.offsets // []) | join("|")),
      ((.appsink_probe.last_video_meta.strides // []) | join("|")),
      ($appsink_drm_fourcc // ""),
      (.appsink_probe.last_video_meta.drm_modifier // ""),
      (.appsink_probe.last_video_meta.dmabuf_attach_ready // ""),
      ((.appsink_probe.last_video_meta.dmabuf_attach_blockers // []) | join("|")),
      (if $appsink_drm_fourcc == null then ""
       else ((.surface.dmabuf.linux_dmabuf_feedback.format_fourccs // []) | index($appsink_drm_fourcc) != null)
       end)
    ] | @tsv
  ' "$runtime_json_path" | awk -F '\t' -v csv_path="$csv_path" -v summary_path="$summary_path" '
    BEGIN {
      print "sample,gst_state,rendered,dropped,qos_messages,qos_dropped_max,position_ms,video_fit,render_rectangle,linux_dmabuf_supported,linux_dmabuf_version,linux_dmabuf_modifier_count,linux_dmabuf_feedback_received,linux_dmabuf_feedback_source,linux_dmabuf_feedback_format_count,linux_dmabuf_feedback_tranche_count,appsink_pulled_samples,appsink_last_memory_count,appsink_last_memory_types,appsink_last_buffer_size,appsink_cuda_alloc_method,appsink_cuda_export_fd,appsink_cuda_export_successes,appsink_cuda_export_failures,appsink_allocation_queries,appsink_allocation_need_pool,appsink_allocation_pool_size,appsink_allocation_mmap_pool_proposals,appsink_allocation_mmap_pool_failures,appsink_allocation_last_result,runtime_elapsed_ms,selected_output_name,selected_output_logical_size,selected_output_current_mode,selected_output_refresh_millihertz,surface_logical_size,surface_scale,appsink_cuda_export_fds_closed,appsink_video_meta_format,appsink_video_meta_size,appsink_video_meta_planes,appsink_video_meta_offsets,appsink_video_meta_strides,appsink_video_meta_drm_fourcc,appsink_video_meta_drm_modifier,appsink_video_meta_attach_ready,appsink_video_meta_attach_blockers,dmabuf_feedback_supports_app_fourcc" > csv_path
    }
    {
      sample += 1
      state = $1
      rendered = $2
      dropped = $3
      qos_messages = $4
      qos_dropped = $5
      position = $6
      fit = $7
      rectangle = $8
      dmabuf_supported = $9
      dmabuf_version = $10
      dmabuf_modifier_count = $11
      dmabuf_feedback_received = $12
      dmabuf_feedback_source = $13
      dmabuf_feedback_format_count = $14
      dmabuf_feedback_tranche_count = $15
      appsink_pulled = $16
      appsink_memory_count = $17
      appsink_memory_types = $18
      appsink_buffer_size = $19
      appsink_cuda_alloc_method = $20
      appsink_cuda_export_fd = $21
      appsink_cuda_export_successes = $22
      appsink_cuda_export_failures = $23
      appsink_allocation_queries = $24
      appsink_allocation_need_pool = $25
      appsink_allocation_pool_size = $26
      appsink_allocation_mmap_pool_proposals = $27
      appsink_allocation_mmap_pool_failures = $28
      appsink_allocation_last_result = $29
      runtime_elapsed_ms = $30
      selected_output_name = $31
      selected_output_logical_size = $32
      selected_output_current_mode = $33
      selected_output_refresh_millihertz = $34
      surface_logical_size = $35
      surface_scale = $36
      appsink_cuda_export_fds_closed = $37
      appsink_video_meta_format = $38
      appsink_video_meta_size = $39
      appsink_video_meta_planes = $40
      appsink_video_meta_offsets = $41
      appsink_video_meta_strides = $42
      appsink_video_meta_drm_fourcc = $43
      appsink_video_meta_drm_modifier = $44
      appsink_video_meta_attach_ready = $45
      appsink_video_meta_attach_blockers = $46
      dmabuf_feedback_supports_app_fourcc = $47
      printf "%d,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s\n", sample, state, rendered, dropped, qos_messages, qos_dropped, position, fit, rectangle, dmabuf_supported, dmabuf_version, dmabuf_modifier_count, dmabuf_feedback_received, dmabuf_feedback_source, dmabuf_feedback_format_count, dmabuf_feedback_tranche_count, appsink_pulled, appsink_memory_count, appsink_memory_types, appsink_buffer_size, appsink_cuda_alloc_method, appsink_cuda_export_fd, appsink_cuda_export_successes, appsink_cuda_export_failures, appsink_allocation_queries, appsink_allocation_need_pool, appsink_allocation_pool_size, appsink_allocation_mmap_pool_proposals, appsink_allocation_mmap_pool_failures, appsink_allocation_last_result, runtime_elapsed_ms, selected_output_name, selected_output_logical_size, selected_output_current_mode, selected_output_refresh_millihertz, surface_logical_size, surface_scale, appsink_cuda_export_fds_closed, appsink_video_meta_format, appsink_video_meta_size, appsink_video_meta_planes, appsink_video_meta_offsets, appsink_video_meta_strides, appsink_video_meta_drm_fourcc, appsink_video_meta_drm_modifier, appsink_video_meta_attach_ready, appsink_video_meta_attach_blockers, dmabuf_feedback_supports_app_fourcc >> csv_path

      rows += 1
      last_state = state
      if (runtime_elapsed_ms != "") {
        last_runtime_elapsed_ms = runtime_elapsed_ms + 0
        seen_runtime_elapsed = 1
      }
      if (state == "PLAYING") {
        playing_rows += 1
      }
      if (selected_output_name != "") {
        last_selected_output_name = selected_output_name
      }
      if (selected_output_logical_size != "") {
        last_selected_output_logical_size = selected_output_logical_size
      }
      if (selected_output_current_mode != "") {
        last_selected_output_current_mode = selected_output_current_mode
      }
      if (selected_output_refresh_millihertz != "") {
        last_selected_output_refresh_millihertz = selected_output_refresh_millihertz
      }
      if (surface_logical_size != "") {
        last_surface_logical_size = surface_logical_size
      }
      if (surface_scale != "") {
        last_surface_scale = surface_scale
      }
      if (fit != "") {
        last_fit = fit
      }
      if (rectangle != "") {
        last_rectangle = rectangle
      }
      if (dmabuf_supported != "") {
        last_dmabuf_supported = dmabuf_supported
      }
      if (dmabuf_version != "") {
        last_dmabuf_version = dmabuf_version
      }
      if (dmabuf_modifier_count != "") {
        last_dmabuf_modifier_count = dmabuf_modifier_count
      }
      if (dmabuf_feedback_received != "") {
        last_dmabuf_feedback_received = dmabuf_feedback_received
      }
      if (dmabuf_feedback_source != "") {
        last_dmabuf_feedback_source = dmabuf_feedback_source
      }
      if (dmabuf_feedback_format_count != "") {
        last_dmabuf_feedback_format_count = dmabuf_feedback_format_count
      }
      if (dmabuf_feedback_tranche_count != "") {
        last_dmabuf_feedback_tranche_count = dmabuf_feedback_tranche_count
      }
      if (appsink_pulled != "") {
        appsink_pulled_numeric = appsink_pulled + 0
        if (!seen_appsink_pulled) {
          first_appsink_pulled = appsink_pulled_numeric
          first_appsink_sample = sample
          first_appsink_elapsed_ms = runtime_elapsed_ms + 0
          seen_appsink_pulled = 1
        }
        if (appsink_pulled_numeric > 0 && !seen_appsink_active) {
          first_appsink_active = appsink_pulled_numeric
          first_appsink_active_sample = sample
          first_appsink_active_elapsed_ms = runtime_elapsed_ms + 0
          first_appsink_active_position = position + 0
          seen_appsink_active = 1
        }
        last_appsink_pulled = appsink_pulled_numeric
        last_appsink_sample = sample
        last_appsink_elapsed_ms = runtime_elapsed_ms + 0
        last_appsink_position = position + 0
      }
      if (appsink_memory_count != "") {
        last_appsink_memory_count = appsink_memory_count
      }
      if (appsink_memory_types != "") {
        last_appsink_memory_types = appsink_memory_types
      }
      if (appsink_buffer_size != "") {
        last_appsink_buffer_size = appsink_buffer_size
      }
      if (appsink_cuda_alloc_method != "") {
        last_appsink_cuda_alloc_method = appsink_cuda_alloc_method
      }
      if (appsink_cuda_export_fd != "") {
        last_appsink_cuda_export_fd = appsink_cuda_export_fd
      }
      if (appsink_cuda_export_successes != "") {
        last_appsink_cuda_export_successes = appsink_cuda_export_successes
      }
      if (appsink_cuda_export_failures != "") {
        last_appsink_cuda_export_failures = appsink_cuda_export_failures
      }
      if (appsink_cuda_export_fds_closed != "") {
        last_appsink_cuda_export_fds_closed = appsink_cuda_export_fds_closed
      }
      if (appsink_video_meta_format != "") {
        last_appsink_video_meta_format = appsink_video_meta_format
      }
      if (appsink_video_meta_size != "") {
        last_appsink_video_meta_size = appsink_video_meta_size
      }
      if (appsink_video_meta_planes != "") {
        last_appsink_video_meta_planes = appsink_video_meta_planes
      }
      if (appsink_video_meta_offsets != "") {
        last_appsink_video_meta_offsets = appsink_video_meta_offsets
      }
      if (appsink_video_meta_strides != "") {
        last_appsink_video_meta_strides = appsink_video_meta_strides
      }
      if (appsink_video_meta_drm_fourcc != "") {
        last_appsink_video_meta_drm_fourcc = appsink_video_meta_drm_fourcc
      }
      if (appsink_video_meta_drm_modifier != "") {
        last_appsink_video_meta_drm_modifier = appsink_video_meta_drm_modifier
      }
      if (appsink_video_meta_attach_ready != "") {
        last_appsink_video_meta_attach_ready = appsink_video_meta_attach_ready
      }
      if (appsink_video_meta_attach_blockers != "") {
        last_appsink_video_meta_attach_blockers = appsink_video_meta_attach_blockers
      }
      if (dmabuf_feedback_supports_app_fourcc != "") {
        last_dmabuf_feedback_supports_app_fourcc = dmabuf_feedback_supports_app_fourcc
      }
      if (appsink_allocation_queries != "") {
        last_appsink_allocation_queries = appsink_allocation_queries
      }
      if (appsink_allocation_need_pool != "") {
        last_appsink_allocation_need_pool = appsink_allocation_need_pool
      }
      if (appsink_allocation_pool_size != "") {
        last_appsink_allocation_pool_size = appsink_allocation_pool_size
      }
      if (appsink_allocation_mmap_pool_proposals != "") {
        last_appsink_allocation_mmap_pool_proposals = appsink_allocation_mmap_pool_proposals
      }
      if (appsink_allocation_mmap_pool_failures != "") {
        last_appsink_allocation_mmap_pool_failures = appsink_allocation_mmap_pool_failures
      }
      if (appsink_allocation_last_result != "") {
        last_appsink_allocation_last_result = appsink_allocation_last_result
      }
      if (rendered != "") {
        rendered_numeric = rendered + 0
        if (!seen_rendered) {
          first_rendered = rendered_numeric
          first_rendered_sample = sample
          first_rendered_elapsed_ms = runtime_elapsed_ms + 0
          seen_rendered = 1
        }
        if (rendered_numeric > 0 && !seen_rendered_active) {
          first_rendered_active = rendered_numeric
          first_rendered_active_sample = sample
          first_rendered_active_elapsed_ms = runtime_elapsed_ms + 0
          first_rendered_active_position = position + 0
          seen_rendered_active = 1
        }
        last_rendered = rendered_numeric
        last_rendered_sample = sample
        last_rendered_elapsed_ms = runtime_elapsed_ms + 0
        last_rendered_position = position + 0
      }
      if (dropped != "") {
        dropped_numeric = dropped + 0
        if (!seen_dropped) {
          first_dropped = dropped_numeric
          seen_dropped = 1
        }
        last_dropped = dropped_numeric
      }
      if (qos_messages != "" && qos_messages + 0 > max_qos_messages) {
        max_qos_messages = qos_messages + 0
      }
      if (qos_dropped != "" && qos_dropped + 0 > max_qos_dropped) {
        max_qos_dropped = qos_dropped + 0
      }
      if (position != "") {
        if (seen_position) {
          delta = position + 0 - previous_position
          if (delta > max_position_delta) {
            max_position_delta = delta
          }
        }
        previous_position = position + 0
        seen_position = 1
      }
    }
    END {
      print "native_runtime_rows: " rows > summary_path
      print "native_runtime_playing_rows: " playing_rows >> summary_path
      if (seen_runtime_elapsed) {
        print "native_runtime_elapsed_ms_latest: " last_runtime_elapsed_ms >> summary_path
      }
      if (last_state != "") {
        print "native_runtime_state_latest: " last_state >> summary_path
      }
      if (last_selected_output_name != "") {
        print "native_selected_output_name_latest: " last_selected_output_name >> summary_path
      }
      if (last_selected_output_logical_size != "") {
        print "native_selected_output_logical_size_latest: " last_selected_output_logical_size >> summary_path
      }
      if (last_selected_output_current_mode != "") {
        print "native_selected_output_current_mode_latest: " last_selected_output_current_mode >> summary_path
      }
      if (last_selected_output_refresh_millihertz != "") {
        print "native_selected_output_refresh_millihertz_latest: " last_selected_output_refresh_millihertz >> summary_path
        printf "native_selected_output_refresh_hz_latest: %.3f\n", (last_selected_output_refresh_millihertz + 0) / 1000 >> summary_path
      }
      if (last_surface_logical_size != "") {
        print "native_surface_logical_size_latest: " last_surface_logical_size >> summary_path
      }
      if (last_surface_scale != "") {
        print "native_surface_scale_latest: " last_surface_scale >> summary_path
      }
      if (last_fit != "") {
        print "native_video_fit_latest: " last_fit >> summary_path
      }
      if (last_rectangle != "") {
        print "native_render_rectangle_latest: " last_rectangle >> summary_path
      }
      if (last_dmabuf_supported != "") {
        print "native_linux_dmabuf_supported_latest: " last_dmabuf_supported >> summary_path
      }
      if (last_dmabuf_version != "") {
        print "native_linux_dmabuf_version_latest: " last_dmabuf_version >> summary_path
      }
      if (last_dmabuf_modifier_count != "") {
        print "native_linux_dmabuf_modifier_count_latest: " last_dmabuf_modifier_count >> summary_path
      }
      if (last_dmabuf_feedback_received != "") {
        print "native_linux_dmabuf_feedback_received_latest: " last_dmabuf_feedback_received >> summary_path
      }
      if (last_dmabuf_feedback_source != "") {
        print "native_linux_dmabuf_feedback_source_latest: " last_dmabuf_feedback_source >> summary_path
      }
      if (last_dmabuf_feedback_format_count != "") {
        print "native_linux_dmabuf_feedback_format_count_latest: " last_dmabuf_feedback_format_count >> summary_path
      }
      if (last_dmabuf_feedback_tranche_count != "") {
        print "native_linux_dmabuf_feedback_tranche_count_latest: " last_dmabuf_feedback_tranche_count >> summary_path
      }
      if (seen_appsink_pulled) {
        appsink_delta = last_appsink_pulled - first_appsink_pulled
        appsink_elapsed = last_appsink_sample - first_appsink_sample
        appsink_elapsed_ms = last_appsink_elapsed_ms - first_appsink_elapsed_ms
        print "native_appsink_pulled_first: " first_appsink_pulled >> summary_path
        print "native_appsink_pulled_last: " last_appsink_pulled >> summary_path
        print "native_appsink_pulled_delta: " appsink_delta >> summary_path
        print "native_appsink_pulled_elapsed_seconds_estimate: " appsink_elapsed >> summary_path
        if (appsink_elapsed_ms > 0) {
          print "native_appsink_pulled_elapsed_ms: " appsink_elapsed_ms >> summary_path
          printf "actual_appsink_pulled_fps: %.3f\n", appsink_delta * 1000 / appsink_elapsed_ms >> summary_path
          print "actual_appsink_pulled_fps_basis: runtime-elapsed-ms" >> summary_path
        } else if (appsink_elapsed > 0) {
          printf "actual_appsink_pulled_fps: %.3f\n", appsink_delta / appsink_elapsed >> summary_path
          print "actual_appsink_pulled_fps_basis: sample-index" >> summary_path
        } else {
          print "actual_appsink_pulled_fps: none" >> summary_path
        }
      }
      if (seen_appsink_active) {
        appsink_active_delta = last_appsink_pulled - first_appsink_active
        appsink_active_elapsed = last_appsink_sample - first_appsink_active_sample
        appsink_active_elapsed_ms = last_appsink_elapsed_ms - first_appsink_active_elapsed_ms
        print "native_appsink_pulled_active_first: " first_appsink_active >> summary_path
        print "native_appsink_pulled_active_delta: " appsink_active_delta >> summary_path
        print "native_appsink_pulled_active_elapsed_seconds_estimate: " appsink_active_elapsed >> summary_path
        if (appsink_active_elapsed_ms > 0) {
          print "native_appsink_pulled_active_elapsed_ms: " appsink_active_elapsed_ms >> summary_path
          printf "actual_appsink_pulled_active_fps: %.3f\n", appsink_active_delta * 1000 / appsink_active_elapsed_ms >> summary_path
          print "actual_appsink_pulled_active_fps_basis: runtime-elapsed-ms" >> summary_path
        } else if (appsink_active_elapsed > 0) {
          printf "actual_appsink_pulled_active_fps: %.3f\n", appsink_active_delta / appsink_active_elapsed >> summary_path
          print "actual_appsink_pulled_active_fps_basis: sample-index" >> summary_path
        }
        appsink_position_delta = last_appsink_position - first_appsink_active_position
        if (appsink_position_delta > 0) {
          printf "actual_appsink_pulled_media_fps: %.3f\n", appsink_active_delta * 1000 / appsink_position_delta >> summary_path
        }
      }
      if (last_appsink_memory_count != "") {
        print "native_appsink_last_memory_count_latest: " last_appsink_memory_count >> summary_path
      }
      if (last_appsink_memory_types != "") {
        print "native_appsink_last_memory_types_latest: " last_appsink_memory_types >> summary_path
      }
      if (last_appsink_buffer_size != "") {
        print "native_appsink_last_buffer_size_latest: " last_appsink_buffer_size >> summary_path
      }
      if (last_appsink_cuda_alloc_method != "") {
        print "native_appsink_cuda_alloc_method_latest: " last_appsink_cuda_alloc_method >> summary_path
      }
      if (last_appsink_cuda_export_fd != "") {
        print "native_appsink_cuda_export_fd_latest: " last_appsink_cuda_export_fd >> summary_path
      }
      if (last_appsink_cuda_export_successes != "") {
        print "native_appsink_cuda_export_successes_latest: " last_appsink_cuda_export_successes >> summary_path
      }
      if (last_appsink_cuda_export_failures != "") {
        print "native_appsink_cuda_export_failures_latest: " last_appsink_cuda_export_failures >> summary_path
      }
      if (last_appsink_cuda_export_fds_closed != "") {
        print "native_appsink_cuda_export_fds_closed_latest: " last_appsink_cuda_export_fds_closed >> summary_path
      }
      if (last_appsink_video_meta_format != "") {
        print "native_appsink_video_meta_format_latest: " last_appsink_video_meta_format >> summary_path
      }
      if (last_appsink_video_meta_size != "") {
        print "native_appsink_video_meta_size_latest: " last_appsink_video_meta_size >> summary_path
      }
      if (last_appsink_video_meta_planes != "") {
        print "native_appsink_video_meta_planes_latest: " last_appsink_video_meta_planes >> summary_path
      }
      if (last_appsink_video_meta_offsets != "") {
        print "native_appsink_video_meta_offsets_latest: " last_appsink_video_meta_offsets >> summary_path
      }
      if (last_appsink_video_meta_strides != "") {
        print "native_appsink_video_meta_strides_latest: " last_appsink_video_meta_strides >> summary_path
      }
      if (last_appsink_video_meta_drm_fourcc != "") {
        print "native_appsink_video_meta_drm_fourcc_latest: " last_appsink_video_meta_drm_fourcc >> summary_path
      }
      if (last_appsink_video_meta_drm_modifier != "") {
        print "native_appsink_video_meta_drm_modifier_latest: " last_appsink_video_meta_drm_modifier >> summary_path
      }
      if (last_appsink_video_meta_attach_ready != "") {
        print "native_appsink_video_meta_attach_ready_latest: " last_appsink_video_meta_attach_ready >> summary_path
      }
      if (last_appsink_video_meta_attach_blockers != "") {
        print "native_appsink_video_meta_attach_blockers_latest: " last_appsink_video_meta_attach_blockers >> summary_path
      }
      if (last_dmabuf_feedback_supports_app_fourcc != "") {
        print "native_dmabuf_feedback_supports_app_fourcc_latest: " last_dmabuf_feedback_supports_app_fourcc >> summary_path
      }
      if (last_appsink_allocation_queries != "") {
        print "native_appsink_allocation_queries_latest: " last_appsink_allocation_queries >> summary_path
      }
      if (last_appsink_allocation_need_pool != "") {
        print "native_appsink_allocation_need_pool_latest: " last_appsink_allocation_need_pool >> summary_path
      }
      if (last_appsink_allocation_pool_size != "") {
        print "native_appsink_allocation_pool_size_latest: " last_appsink_allocation_pool_size >> summary_path
      }
      if (last_appsink_allocation_mmap_pool_proposals != "") {
        print "native_appsink_allocation_mmap_pool_proposals_latest: " last_appsink_allocation_mmap_pool_proposals >> summary_path
      }
      if (last_appsink_allocation_mmap_pool_failures != "") {
        print "native_appsink_allocation_mmap_pool_failures_latest: " last_appsink_allocation_mmap_pool_failures >> summary_path
      }
      if (last_appsink_allocation_last_result != "") {
        print "native_appsink_allocation_last_result_latest: " last_appsink_allocation_last_result >> summary_path
      }
      if (seen_rendered) {
        rendered_delta = last_rendered - first_rendered
        rendered_elapsed = last_rendered_sample - first_rendered_sample
        rendered_elapsed_ms = last_rendered_elapsed_ms - first_rendered_elapsed_ms
        print "native_sink_rendered_first: " first_rendered >> summary_path
        print "native_sink_rendered_last: " last_rendered >> summary_path
        print "native_sink_rendered_delta: " rendered_delta >> summary_path
        print "native_sink_rendered_elapsed_seconds_estimate: " rendered_elapsed >> summary_path
        if (rendered_elapsed_ms > 0) {
          print "native_sink_rendered_elapsed_ms: " rendered_elapsed_ms >> summary_path
          actual_fps = rendered_delta * 1000 / rendered_elapsed_ms
          printf "actual_rendered_fps: %.3f\n", actual_fps >> summary_path
          print "actual_rendered_fps_basis: runtime-elapsed-ms" >> summary_path
          if (actual_fps >= 230) {
            print "actual_rendered_fps_status: near-240" >> summary_path
          } else {
            print "actual_rendered_fps_status: below-240" >> summary_path
          }
        } else if (rendered_elapsed > 0) {
          actual_fps = rendered_delta / rendered_elapsed
          printf "actual_rendered_fps: %.3f\n", actual_fps >> summary_path
          print "actual_rendered_fps_basis: sample-index" >> summary_path
          if (actual_fps >= 230) {
            print "actual_rendered_fps_status: near-240" >> summary_path
          } else {
            print "actual_rendered_fps_status: below-240" >> summary_path
          }
        } else {
          print "actual_rendered_fps: none" >> summary_path
          print "actual_rendered_fps_status: unavailable" >> summary_path
        }
      } else {
        print "actual_rendered_fps: none" >> summary_path
        print "actual_rendered_fps_status: unavailable" >> summary_path
      }
      if (seen_rendered_active) {
        rendered_active_delta = last_rendered - first_rendered_active
        rendered_active_elapsed = last_rendered_sample - first_rendered_active_sample
        rendered_active_elapsed_ms = last_rendered_elapsed_ms - first_rendered_active_elapsed_ms
        print "native_sink_rendered_active_first: " first_rendered_active >> summary_path
        print "native_sink_rendered_active_delta: " rendered_active_delta >> summary_path
        print "native_sink_rendered_active_elapsed_seconds_estimate: " rendered_active_elapsed >> summary_path
        if (rendered_active_elapsed_ms > 0) {
          print "native_sink_rendered_active_elapsed_ms: " rendered_active_elapsed_ms >> summary_path
          printf "actual_rendered_active_fps: %.3f\n", rendered_active_delta * 1000 / rendered_active_elapsed_ms >> summary_path
          print "actual_rendered_active_fps_basis: runtime-elapsed-ms" >> summary_path
        } else if (rendered_active_elapsed > 0) {
          printf "actual_rendered_active_fps: %.3f\n", rendered_active_delta / rendered_active_elapsed >> summary_path
          print "actual_rendered_active_fps_basis: sample-index" >> summary_path
        }
        rendered_position_delta = last_rendered_position - first_rendered_active_position
        if (rendered_position_delta > 0) {
          printf "actual_rendered_media_fps: %.3f\n", rendered_active_delta * 1000 / rendered_position_delta >> summary_path
        }
      }
      if (seen_dropped) {
        print "native_sink_dropped_first: " first_dropped >> summary_path
        print "native_sink_dropped_last: " last_dropped >> summary_path
        print "native_sink_dropped_delta: " last_dropped - first_dropped >> summary_path
      }
      print "native_qos_messages_max: " max_qos_messages >> summary_path
      print "native_qos_dropped_max: " max_qos_dropped >> summary_path
      print "native_position_delta_ms_max: " max_position_delta >> summary_path
    }
  '
}

append_native_dmabuf_runtime_summary() {
  local runtime_json_path="$1"
  local summary_path="$2"

  [[ -s "$runtime_json_path" ]] || return 0

  jq -sr '
    .[-1] |
    [
      "native_dmabuf_buffers_created_latest: \(.surface.dmabuf.dmabuf_buffers_created // "")",
      "native_dmabuf_buffer_create_failures_latest: \(.surface.dmabuf.dmabuf_buffer_create_failures // "")",
      "native_dmabuf_buffers_released_latest: \(.surface.dmabuf.dmabuf_buffers_released // "")",
      "native_dmabuf_frames_submitted_latest: \(.surface.dmabuf.dmabuf_frames_submitted // "")",
      "native_dmabuf_frames_attached_latest: \(.surface.dmabuf.dmabuf_frames_attached // "")",
      "native_dmabuf_frame_attach_failures_latest: \(.surface.dmabuf.dmabuf_frame_attach_failures // "")",
      "native_dmabuf_frame_attach_skips_latest: \(.surface.dmabuf.dmabuf_frame_attach_skips // "")",
      "native_dmabuf_buffers_pending_latest: \(.surface.dmabuf.dmabuf_buffers_pending // "")",
      "native_dmabuf_buffers_in_flight_latest: \(.surface.dmabuf.dmabuf_buffers_in_flight // "")",
      "native_dmabuf_last_frame_format_latest: \(.surface.dmabuf.dmabuf_last_frame_format // "")",
      "native_dmabuf_last_frame_modifier_latest: \(.surface.dmabuf.dmabuf_last_frame_modifier // "")",
      "native_dmabuf_last_attach_error_latest: \(.surface.dmabuf.dmabuf_last_attach_error // "")",
      "native_appsink_dmabuf_export_source_latest: \(.appsink_probe.last_dmabuf_export_source // "")",
      "native_appsink_dmabuf_export_fd_count_latest: \(.appsink_probe.last_dmabuf_export_fd_count // "")",
      "native_appsink_dmabuf_export_plane_count_latest: \(.appsink_probe.last_dmabuf_export_plane_count // "")",
      "native_appsink_dmabuf_export_error_latest: \(.appsink_probe.last_dmabuf_export_error // "")",
      "native_appsink_dmabuf_copy_fallback_error_latest: \(.appsink_probe.last_dmabuf_copy_fallback_error // "")",
      "native_appsink_dmabuf_export_successes_latest: \(.appsink_probe.dmabuf_export_successes // "")",
      "native_appsink_dmabuf_export_failures_latest: \(.appsink_probe.dmabuf_export_failures // "")",
      "native_appsink_video_meta_dmabuf_export_source_latest: \(.appsink_probe.last_video_meta.dmabuf_export_source // "")",
      "native_appsink_video_meta_dmabuf_export_error_latest: \(.appsink_probe.last_video_meta.dmabuf_export_error // "")"
    ] | .[]
  ' "$runtime_json_path" >> "$summary_path"
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
[[ "$runtime_interval_ms" =~ ^[0-9]+$ && "$runtime_interval_ms" -ge 100 ]] || {
  echo "--runtime-interval-ms must be an integer >= 100" >&2
  exit 2
}
[[ "$video_size" =~ ^[0-9]+x[0-9]+$ ]] || {
  echo "--video-size must look like WxH" >&2
  exit 2
}
case "$pipeline" in
  playbin|playbin3|explicit-h264-gl|h264-gl|gl-h264|appsink-probe|appsink|probe|appsink-mmap-probe|appsink-mmap|mmap-probe|appsink-dmabuf-present|dmabuf-present|present) ;;
  *) echo "--pipeline must be appsink-dmabuf-present, appsink-mmap-probe, appsink-probe, explicit-h264-gl, playbin, or playbin3" >&2; exit 2 ;;
esac
case "$pipeline" in
  playbin|playbin3)
    if [[ "$allow_legacy_waylandsink" -eq 0 ]]; then
      echo "--pipeline ${pipeline} is the deprecated playbin+waylandsink path; pass --allow-legacy-waylandsink only for explicit comparison runs" >&2
      exit 2
    fi
    ;;
esac
case "$pipeline" in
  appsink-dmabuf-present|dmabuf-present|present)
    if [[ "$allow_unsafe_dmabuf_present" -eq 0 ]]; then
      echo "--pipeline ${pipeline} manually attaches linux-dmabuf buffers and has crashed niri/NVIDIA during stress tests; pass --allow-unsafe-dmabuf-present only for isolated debugging" >&2
      exit 2
    fi
    ;;
esac
case "$fit" in
  cover|contain|stretch|center) ;;
  *) echo "--fit must be cover, contain, stretch, or center" >&2; exit 2 ;;
esac
case "$layer" in
  background|bottom|top|overlay) ;;
  *) echo "--layer must be background, bottom, top, or overlay" >&2; exit 2 ;;
esac
if [[ "$allow_foreground_layer" -eq 0 ]]; then
  case "$layer" in
    top|overlay)
      echo "--layer ${layer} covers normal application windows; pass --allow-foreground-layer for foreground debug" >&2
      exit 2
      ;;
  esac
fi
if [[ "$fps_limit" -eq 1 && "$target_max_fps" -ge 200 && -z "$output_name" && "$allow_compositor_output" -eq 0 ]]; then
  echo "--output-name is required for >=200fps native Wayland smoke; pass --allow-compositor-output to let the compositor choose" >&2
  exit 2
fi
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
if [[ "$runtime_json_enabled" -eq 1 ]]; then
  require_command jq
fi

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
native_rendered_fps_path="$work_dir/native-rendered-fps.csv"
native_runtime_summary_path="$work_dir/native-runtime-summary.txt"

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
  --fit "$fit"
  --layer "$layer"
)
if [[ "$allow_foreground_layer" -eq 1 ]]; then
  native_args+=(--allow-foreground-layer)
fi
if [[ "$allow_legacy_waylandsink" -eq 1 ]]; then
  native_args+=(--allow-legacy-waylandsink)
fi
if [[ "$allow_unsafe_dmabuf_present" -eq 1 ]]; then
  native_args+=(--allow-unsafe-dmabuf-present)
fi
if [[ -n "$output_name" ]]; then
  native_args+=(--output-name "$output_name")
fi
if [[ "$runtime_json_enabled" -eq 1 ]]; then
  native_args+=(--runtime-json "$runtime_json")
  native_args+=(--runtime-interval-ms "$runtime_interval_ms")
fi
if [[ "$debug_visible_frame" -eq 1 ]]; then
  native_args+=(--debug-visible-frame)
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
allow_legacy_waylandsink: $([[ "$allow_legacy_waylandsink" -eq 1 ]] && printf yes || printf no)
allow_unsafe_dmabuf_present: $([[ "$allow_unsafe_dmabuf_present" -eq 1 ]] && printf yes || printf no)
fit: ${fit}
layer: ${layer}
allow_foreground_layer: $([[ "$allow_foreground_layer" -eq 1 ]] && printf yes || printf no)
output_name: ${output_name:-compositor-selected}
allow_compositor_output: $([[ "$allow_compositor_output" -eq 1 ]] && printf yes || printf no)
debug_visible_frame: $([[ "$debug_visible_frame" -eq 1 ]] && printf yes || printf no)
opaque_region: $([[ "$opaque_region" -eq 1 ]] && printf yes || printf no)
input_passthrough: $([[ "$input_passthrough" -eq 1 ]] && printf yes || printf no)
decoder: ${decoder}
runtime_json: ${runtime_json}
runtime_json_enabled: $([[ "$runtime_json_enabled" -eq 1 ]] && printf yes || printf no)
runtime_interval_ms: ${runtime_interval_ms}
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

if [[ "$runtime_json_enabled" -eq 1 ]]; then
  summarize_native_runtime_json "$runtime_json" "$native_rendered_fps_path" "$native_runtime_summary_path"
  append_native_dmabuf_runtime_summary "$runtime_json" "$native_runtime_summary_path"
fi

{
  printf 'work_dir: %s\n' "$work_dir"
  printf 'metadata: %s\n' "$metadata_path"
  printf 'video_info: %s\n' "$video_info_path"
  printf 'native_log: %s\n' "$native_log"
  if [[ "$runtime_json_enabled" -eq 1 ]]; then
    printf 'native_runtime_json: %s\n' "$runtime_json"
    printf 'native_rendered_fps_csv: %s\n' "$native_rendered_fps_path"
    printf 'native_runtime_summary: %s\n' "$native_runtime_summary_path"
  else
    printf 'native_runtime_json: none\n'
    printf 'native_rendered_fps_csv: none\n'
    printf 'native_runtime_summary: none\n'
  fi
  printf 'performance_summary: %s\n' "$performance_dir/summary.txt"
  printf 'performance_memory_mapping: %s\n' "$performance_dir/memory-mapping-summary.txt"
  printf 'kept: %s\n' "$([[ "$keep" -eq 1 || -n "$report_dir" ]] && printf yes || printf no)"
} > "$summary_path"

printf 'native Wayland video evidence: %s\n' "$work_dir"
printf 'summary: %s\n' "$summary_path"
if [[ "$runtime_json_enabled" -eq 1 ]]; then
  printf 'native runtime JSONL: %s\n' "$runtime_json"
  printf 'native runtime summary: %s\n' "$native_runtime_summary_path"
  printf 'native rendered fps CSV: %s\n' "$native_rendered_fps_path"
else
  printf 'native runtime JSONL: none\n'
fi
printf 'performance summary: %s\n' "$performance_dir/summary.txt"
