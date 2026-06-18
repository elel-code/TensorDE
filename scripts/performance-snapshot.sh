#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
usage: scripts/performance-snapshot.sh [options]

Sample a running gilderd process and save resource/status evidence for
active, paused, fullscreen, battery, or other desktop-state performance checks.

Options:
  --pid <pid>         gilderd process id. Default: first gilderd owned by user
  --socket <path>     IPC socket path passed to gilderctl as GILDER_SOCKET
  --gilderctl <path>  gilderctl binary. Default: target/debug/gilderctl or PATH
  --label <name>      Label written to metadata. Default: sample
  --duration <sec>    Sampling duration in whole seconds. Default: 10
  --interval <sec>    Sampling interval in whole seconds. Default: 1
  --work-dir <dir>    Parent directory for temporary evidence
  --output-dir <dir>  Exact evidence directory. Created if needed
  --expect-mode <mode>
                     Require at least one decision with this mode
  --expect-reason <reason>
                     Require at least one decision with this reason
  --expect-action <action>
                     Require at least one decision with this action
  --expect-max-fps <fps>
                     Require at least one decision with this max_fps
  --expect-plan-kind <kind>
                     Require at least one decision with this plan kind
  --expect-max-rss-kib-at-most <kib>
                     Require sampled max RSS to be at most this KiB value
  --expect-max-pss-kib-at-most <kib>
                     Require sampled max PSS to be at most this KiB value
  --expect-max-private-kib-at-most <kib>
                     Require sampled max private memory to be at most this KiB value
  --expect-max-uss-kib-at-most <kib>
                     Require sampled max USS/private memory to be at most this KiB value
  --expect-max-shared-kib-at-most <kib>
                     Require sampled max shared memory to be at most this KiB value
  --expect-retained-rss-delta-kib-at-most <kib>
                     Require last-minus-first RSS delta to be at most this KiB value
  --expect-retained-pss-delta-kib-at-most <kib>
                     Require last-minus-first PSS delta to be at most this KiB value
  --expect-retained-private-delta-kib-at-most <kib>
                     Require last-minus-first private memory delta to be at most this KiB value
  --expect-retained-uss-delta-kib-at-most <kib>
                     Require last-minus-first USS/private delta to be at most this KiB value
  --expect-retained-shared-delta-kib-at-most <kib>
                     Require last-minus-first shared memory delta to be at most this KiB value
  --expect-peak-over-first-rss-kib-at-most <kib>
                     Require max-minus-first RSS delta to be at most this KiB value
  --expect-peak-over-first-pss-kib-at-most <kib>
                     Require max-minus-first PSS delta to be at most this KiB value
  --expect-peak-over-first-private-kib-at-most <kib>
                     Require max-minus-first private memory delta to be at most this KiB value
  --expect-peak-over-first-uss-kib-at-most <kib>
                     Require max-minus-first USS/private delta to be at most this KiB value
  --expect-peak-over-first-shared-kib-at-most <kib>
                     Require max-minus-first shared memory delta to be at most this KiB value
  --expect-render-sync-cache-hit
                     Require render_sync cache hits to increase during sampling
  --expect-desktop-refresh-skip
                     Require read-request desktop refresh skips to increase during sampling
  --expect-render-sync-update-queued
                     Require renderer sync queued count to be non-zero
  --expect-render-sync-update-skipped
                     Require renderer sync skipped count to be non-zero
  --expect-render-sync-package-cache-entries-latest-at-most <count>
                     Require latest render_sync package cache entries to be at most count
  --expect-render-sync-package-cache-retained-resource-references-latest-at-most <count>
                     Require latest retained package-cache resource references to be at most count
  --expect-render-sync-package-cache-retained-unique-resources-latest-at-most <count>
                     Require latest retained package-cache unique resources to be at most count
  --expect-render-sync-package-cache-retained-resource-bytes-latest-at-most <bytes>
                     Require latest retained package-cache resource reference bytes to be at most bytes
  --expect-render-sync-package-cache-retained-unique-resource-bytes-latest-at-most <bytes>
                     Require latest retained package-cache unique resource bytes to be at most bytes
  --expect-render-sync-planned-image-resource-references-latest-at-most <count>
                     Require latest planned image resource references to be at most count
  --expect-render-sync-planned-unique-image-resources-latest-at-most <count>
                     Require latest planned unique image resources to be at most count
  --expect-render-sync-planned-image-resource-reference-bytes-latest-at-most <bytes>
                     Require latest planned image resource reference bytes to be at most bytes
  --expect-render-sync-planned-unique-image-resource-bytes-latest-at-most <bytes>
                     Require latest planned unique image resource bytes to be at most bytes
  --expect-renderer-output-windows-latest-at-least <count>
                     Require latest renderer output window count to be at least count
  --expect-renderer-output-windows-latest-at-most <count>
                     Require latest renderer output window count to be at most count
  --expect-renderer-output-windows-max-at-most <count>
                     Require max sampled renderer output window count to be at most count
  --expect-renderer-static-surfaces-latest-at-least <count>
                     Require latest renderer static surface count to be at least count
  --expect-renderer-static-surfaces-latest-at-most <count>
                     Require latest renderer static surface count to be at most count
  --expect-renderer-static-surfaces-max-at-most <count>
                     Require max sampled renderer static surface count to be at most count
  --expect-renderer-slideshow-surfaces-latest-at-least <count>
                     Require latest renderer slideshow surface count to be at least count
  --expect-renderer-slideshow-surfaces-latest-at-most <count>
                     Require latest renderer slideshow surface count to be at most count
  --expect-renderer-slideshow-surfaces-max-at-most <count>
                     Require max sampled renderer slideshow surface count to be at most count
  --expect-renderer-video-surfaces-latest-at-least <count>
                     Require latest renderer video surface count to be at least count
  --expect-renderer-video-surfaces-latest-at-most <count>
                     Require latest renderer video surface count to be at most count
  --expect-renderer-video-surfaces-max-at-most <count>
                     Require max sampled renderer video surface count to be at most count
  --expect-renderer-video-pipelines-latest-at-least <count>
                     Require latest renderer video pipeline count to be at least count
  --expect-renderer-video-pipelines-latest-at-most <count>
                     Require latest renderer video pipeline count to be at most count
  --expect-renderer-video-pipelines-max-at-most <count>
                     Require max sampled renderer video pipeline count to be at most count
  --expect-adaptive-action <type>
                     Require at least one telemetry row with this adaptive action type
  --expect-decoder-policy-status <status>
                     Require at least one video runtime row with this decoder policy status
  --expect-decoder-class <hardware|software|unknown>
                     Require at least one video runtime row with this decoder class
  --expect-memory-feature <feature>
                     Require at least one video runtime row with this caps memory feature
  --expect-sink-memory-feature <feature>
                     Require at least one video runtime row with this sink-side caps memory feature
  --expect-zero-copy-evidence <level>
                     Require at least one video runtime row with this zero-copy evidence level
  --expect-video-position-progress
                     Require sampled video position to advance on at least one output
  --expect-frame-limiter-enabled
                     Require at least one video runtime row with an enabled frame limiter
  --expect-frame-limiter-max-fps <fps>
                     Require at least one video runtime row with this frame limiter max_fps
  --expect-video-qos
                     Require at least one video runtime row with observed GStreamer QoS messages
  --expect-qos-dropped-max-at-most <count>
                     Require observed QoS dropped max to be at most count
  --expect-gtk-frame-clock
                     Require observed GTK frame clock ticks in video runtime rows
  --expect-gtk-frame-clock-phase <phase>
                     Require GTK frame clock phase ticks. Phase: before-paint, update, layout, paint, after-paint, or all
  --expect-gtk-frame-timings
                     Require observed completed GDK frame timings in video runtime rows
  --allow-missing     Report missing daemon/tools as skips instead of failures
  --keep              Keep generated evidence after the script exits
  -h, --help          Show this help text
EOF
}

pid=""
socket="${GILDER_SOCKET:-}"
gilderctl=""
label="sample"
duration=10
interval=1
work_parent="${TMPDIR:-/tmp}"
output_dir=""
allow_missing=0
keep=0
expect_mode=""
expect_reason=""
expect_action=""
expect_max_fps=""
expect_plan_kind=""
expect_max_rss_kib_at_most=""
expect_max_pss_kib_at_most=""
expect_max_private_kib_at_most=""
expect_max_uss_kib_at_most=""
expect_max_shared_kib_at_most=""
expect_retained_rss_delta_kib_at_most=""
expect_retained_pss_delta_kib_at_most=""
expect_retained_private_delta_kib_at_most=""
expect_retained_uss_delta_kib_at_most=""
expect_retained_shared_delta_kib_at_most=""
expect_peak_over_first_rss_kib_at_most=""
expect_peak_over_first_pss_kib_at_most=""
expect_peak_over_first_private_kib_at_most=""
expect_peak_over_first_uss_kib_at_most=""
expect_peak_over_first_shared_kib_at_most=""
expect_render_sync_cache_hit=0
expect_desktop_refresh_skip=0
expect_render_sync_update_queued=0
expect_render_sync_update_skipped=0
expect_render_sync_package_cache_entries_latest_at_most=""
expect_render_sync_package_cache_retained_resource_references_latest_at_most=""
expect_render_sync_package_cache_retained_unique_resources_latest_at_most=""
expect_render_sync_package_cache_retained_resource_bytes_latest_at_most=""
expect_render_sync_package_cache_retained_unique_resource_bytes_latest_at_most=""
expect_render_sync_planned_image_resource_references_latest_at_most=""
expect_render_sync_planned_unique_image_resources_latest_at_most=""
expect_render_sync_planned_image_resource_reference_bytes_latest_at_most=""
expect_render_sync_planned_unique_image_resource_bytes_latest_at_most=""
expect_renderer_output_windows_latest_at_least=""
expect_renderer_output_windows_latest_at_most=""
expect_renderer_output_windows_max_at_most=""
expect_renderer_static_surfaces_latest_at_least=""
expect_renderer_static_surfaces_latest_at_most=""
expect_renderer_static_surfaces_max_at_most=""
expect_renderer_slideshow_surfaces_latest_at_least=""
expect_renderer_slideshow_surfaces_latest_at_most=""
expect_renderer_slideshow_surfaces_max_at_most=""
expect_renderer_video_surfaces_latest_at_least=""
expect_renderer_video_surfaces_latest_at_most=""
expect_renderer_video_surfaces_max_at_most=""
expect_renderer_video_pipelines_latest_at_least=""
expect_renderer_video_pipelines_latest_at_most=""
expect_renderer_video_pipelines_max_at_most=""
expect_adaptive_action=""
expect_decoder_policy_status=""
expect_decoder_class=""
expect_memory_feature=""
expect_sink_memory_feature=""
expect_zero_copy_evidence=""
expect_video_position_progress=0
expect_frame_limiter_enabled=0
expect_frame_limiter_max_fps=""
expect_video_qos=0
expect_qos_dropped_max_at_most=""
expect_gtk_frame_clock=0
expect_gtk_frame_clock_phases=()
expect_gtk_frame_timings=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --pid)
      [[ $# -ge 2 ]] || { echo "--pid requires a value" >&2; exit 2; }
      pid="$2"
      shift 2
      ;;
    --socket)
      [[ $# -ge 2 ]] || { echo "--socket requires a path" >&2; exit 2; }
      socket="$2"
      shift 2
      ;;
    --gilderctl)
      [[ $# -ge 2 ]] || { echo "--gilderctl requires a path" >&2; exit 2; }
      gilderctl="$2"
      shift 2
      ;;
    --label)
      [[ $# -ge 2 ]] || { echo "--label requires a value" >&2; exit 2; }
      label="$2"
      shift 2
      ;;
    --duration)
      [[ $# -ge 2 ]] || { echo "--duration requires seconds" >&2; exit 2; }
      duration="$2"
      shift 2
      ;;
    --interval)
      [[ $# -ge 2 ]] || { echo "--interval requires seconds" >&2; exit 2; }
      interval="$2"
      shift 2
      ;;
    --work-dir)
      [[ $# -ge 2 ]] || { echo "--work-dir requires a directory" >&2; exit 2; }
      work_parent="$2"
      shift 2
      ;;
    --output-dir)
      [[ $# -ge 2 ]] || { echo "--output-dir requires a directory" >&2; exit 2; }
      output_dir="$2"
      shift 2
      ;;
    --expect-mode)
      [[ $# -ge 2 ]] || { echo "--expect-mode requires a value" >&2; exit 2; }
      expect_mode="$2"
      shift 2
      ;;
    --expect-reason)
      [[ $# -ge 2 ]] || { echo "--expect-reason requires a value" >&2; exit 2; }
      expect_reason="$2"
      shift 2
      ;;
    --expect-action)
      [[ $# -ge 2 ]] || { echo "--expect-action requires a value" >&2; exit 2; }
      expect_action="$2"
      shift 2
      ;;
    --expect-max-fps)
      [[ $# -ge 2 ]] || { echo "--expect-max-fps requires a value" >&2; exit 2; }
      expect_max_fps="$2"
      shift 2
      ;;
    --expect-plan-kind)
      [[ $# -ge 2 ]] || { echo "--expect-plan-kind requires a value" >&2; exit 2; }
      expect_plan_kind="$2"
      shift 2
      ;;
    --expect-max-rss-kib-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-max-rss-kib-at-most requires a value" >&2; exit 2; }
      expect_max_rss_kib_at_most="$2"
      shift 2
      ;;
    --expect-max-pss-kib-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-max-pss-kib-at-most requires a value" >&2; exit 2; }
      expect_max_pss_kib_at_most="$2"
      shift 2
      ;;
    --expect-max-private-kib-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-max-private-kib-at-most requires a value" >&2; exit 2; }
      expect_max_private_kib_at_most="$2"
      shift 2
      ;;
    --expect-max-uss-kib-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-max-uss-kib-at-most requires a value" >&2; exit 2; }
      expect_max_uss_kib_at_most="$2"
      shift 2
      ;;
    --expect-max-shared-kib-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-max-shared-kib-at-most requires a value" >&2; exit 2; }
      expect_max_shared_kib_at_most="$2"
      shift 2
      ;;
    --expect-retained-rss-delta-kib-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-retained-rss-delta-kib-at-most requires a value" >&2; exit 2; }
      expect_retained_rss_delta_kib_at_most="$2"
      shift 2
      ;;
    --expect-retained-pss-delta-kib-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-retained-pss-delta-kib-at-most requires a value" >&2; exit 2; }
      expect_retained_pss_delta_kib_at_most="$2"
      shift 2
      ;;
    --expect-retained-private-delta-kib-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-retained-private-delta-kib-at-most requires a value" >&2; exit 2; }
      expect_retained_private_delta_kib_at_most="$2"
      shift 2
      ;;
    --expect-retained-uss-delta-kib-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-retained-uss-delta-kib-at-most requires a value" >&2; exit 2; }
      expect_retained_uss_delta_kib_at_most="$2"
      shift 2
      ;;
    --expect-retained-shared-delta-kib-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-retained-shared-delta-kib-at-most requires a value" >&2; exit 2; }
      expect_retained_shared_delta_kib_at_most="$2"
      shift 2
      ;;
    --expect-peak-over-first-rss-kib-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-peak-over-first-rss-kib-at-most requires a value" >&2; exit 2; }
      expect_peak_over_first_rss_kib_at_most="$2"
      shift 2
      ;;
    --expect-peak-over-first-pss-kib-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-peak-over-first-pss-kib-at-most requires a value" >&2; exit 2; }
      expect_peak_over_first_pss_kib_at_most="$2"
      shift 2
      ;;
    --expect-peak-over-first-private-kib-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-peak-over-first-private-kib-at-most requires a value" >&2; exit 2; }
      expect_peak_over_first_private_kib_at_most="$2"
      shift 2
      ;;
    --expect-peak-over-first-uss-kib-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-peak-over-first-uss-kib-at-most requires a value" >&2; exit 2; }
      expect_peak_over_first_uss_kib_at_most="$2"
      shift 2
      ;;
    --expect-peak-over-first-shared-kib-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-peak-over-first-shared-kib-at-most requires a value" >&2; exit 2; }
      expect_peak_over_first_shared_kib_at_most="$2"
      shift 2
      ;;
    --expect-render-sync-cache-hit)
      expect_render_sync_cache_hit=1
      shift
      ;;
    --expect-desktop-refresh-skip)
      expect_desktop_refresh_skip=1
      shift
      ;;
    --expect-render-sync-update-queued)
      expect_render_sync_update_queued=1
      shift
      ;;
    --expect-render-sync-update-skipped)
      expect_render_sync_update_skipped=1
      shift
      ;;
    --expect-render-sync-package-cache-entries-latest-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-render-sync-package-cache-entries-latest-at-most requires a value" >&2; exit 2; }
      expect_render_sync_package_cache_entries_latest_at_most="$2"
      shift 2
      ;;
    --expect-render-sync-package-cache-retained-resource-references-latest-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-render-sync-package-cache-retained-resource-references-latest-at-most requires a value" >&2; exit 2; }
      expect_render_sync_package_cache_retained_resource_references_latest_at_most="$2"
      shift 2
      ;;
    --expect-render-sync-package-cache-retained-unique-resources-latest-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-render-sync-package-cache-retained-unique-resources-latest-at-most requires a value" >&2; exit 2; }
      expect_render_sync_package_cache_retained_unique_resources_latest_at_most="$2"
      shift 2
      ;;
    --expect-render-sync-package-cache-retained-resource-bytes-latest-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-render-sync-package-cache-retained-resource-bytes-latest-at-most requires a value" >&2; exit 2; }
      expect_render_sync_package_cache_retained_resource_bytes_latest_at_most="$2"
      shift 2
      ;;
    --expect-render-sync-package-cache-retained-unique-resource-bytes-latest-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-render-sync-package-cache-retained-unique-resource-bytes-latest-at-most requires a value" >&2; exit 2; }
      expect_render_sync_package_cache_retained_unique_resource_bytes_latest_at_most="$2"
      shift 2
      ;;
    --expect-render-sync-planned-image-resource-references-latest-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-render-sync-planned-image-resource-references-latest-at-most requires a value" >&2; exit 2; }
      expect_render_sync_planned_image_resource_references_latest_at_most="$2"
      shift 2
      ;;
    --expect-render-sync-planned-unique-image-resources-latest-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-render-sync-planned-unique-image-resources-latest-at-most requires a value" >&2; exit 2; }
      expect_render_sync_planned_unique_image_resources_latest_at_most="$2"
      shift 2
      ;;
    --expect-render-sync-planned-image-resource-reference-bytes-latest-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-render-sync-planned-image-resource-reference-bytes-latest-at-most requires a value" >&2; exit 2; }
      expect_render_sync_planned_image_resource_reference_bytes_latest_at_most="$2"
      shift 2
      ;;
    --expect-render-sync-planned-unique-image-resource-bytes-latest-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-render-sync-planned-unique-image-resource-bytes-latest-at-most requires a value" >&2; exit 2; }
      expect_render_sync_planned_unique_image_resource_bytes_latest_at_most="$2"
      shift 2
      ;;
    --expect-renderer-output-windows-latest-at-least)
      [[ $# -ge 2 ]] || { echo "--expect-renderer-output-windows-latest-at-least requires a value" >&2; exit 2; }
      expect_renderer_output_windows_latest_at_least="$2"
      shift 2
      ;;
    --expect-renderer-output-windows-latest-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-renderer-output-windows-latest-at-most requires a value" >&2; exit 2; }
      expect_renderer_output_windows_latest_at_most="$2"
      shift 2
      ;;
    --expect-renderer-output-windows-max-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-renderer-output-windows-max-at-most requires a value" >&2; exit 2; }
      expect_renderer_output_windows_max_at_most="$2"
      shift 2
      ;;
    --expect-renderer-static-surfaces-latest-at-least)
      [[ $# -ge 2 ]] || { echo "--expect-renderer-static-surfaces-latest-at-least requires a value" >&2; exit 2; }
      expect_renderer_static_surfaces_latest_at_least="$2"
      shift 2
      ;;
    --expect-renderer-static-surfaces-latest-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-renderer-static-surfaces-latest-at-most requires a value" >&2; exit 2; }
      expect_renderer_static_surfaces_latest_at_most="$2"
      shift 2
      ;;
    --expect-renderer-static-surfaces-max-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-renderer-static-surfaces-max-at-most requires a value" >&2; exit 2; }
      expect_renderer_static_surfaces_max_at_most="$2"
      shift 2
      ;;
    --expect-renderer-slideshow-surfaces-latest-at-least)
      [[ $# -ge 2 ]] || { echo "--expect-renderer-slideshow-surfaces-latest-at-least requires a value" >&2; exit 2; }
      expect_renderer_slideshow_surfaces_latest_at_least="$2"
      shift 2
      ;;
    --expect-renderer-slideshow-surfaces-latest-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-renderer-slideshow-surfaces-latest-at-most requires a value" >&2; exit 2; }
      expect_renderer_slideshow_surfaces_latest_at_most="$2"
      shift 2
      ;;
    --expect-renderer-slideshow-surfaces-max-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-renderer-slideshow-surfaces-max-at-most requires a value" >&2; exit 2; }
      expect_renderer_slideshow_surfaces_max_at_most="$2"
      shift 2
      ;;
    --expect-renderer-video-surfaces-latest-at-least)
      [[ $# -ge 2 ]] || { echo "--expect-renderer-video-surfaces-latest-at-least requires a value" >&2; exit 2; }
      expect_renderer_video_surfaces_latest_at_least="$2"
      shift 2
      ;;
    --expect-renderer-video-surfaces-latest-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-renderer-video-surfaces-latest-at-most requires a value" >&2; exit 2; }
      expect_renderer_video_surfaces_latest_at_most="$2"
      shift 2
      ;;
    --expect-renderer-video-surfaces-max-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-renderer-video-surfaces-max-at-most requires a value" >&2; exit 2; }
      expect_renderer_video_surfaces_max_at_most="$2"
      shift 2
      ;;
    --expect-renderer-video-pipelines-latest-at-least)
      [[ $# -ge 2 ]] || { echo "--expect-renderer-video-pipelines-latest-at-least requires a value" >&2; exit 2; }
      expect_renderer_video_pipelines_latest_at_least="$2"
      shift 2
      ;;
    --expect-renderer-video-pipelines-latest-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-renderer-video-pipelines-latest-at-most requires a value" >&2; exit 2; }
      expect_renderer_video_pipelines_latest_at_most="$2"
      shift 2
      ;;
    --expect-renderer-video-pipelines-max-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-renderer-video-pipelines-max-at-most requires a value" >&2; exit 2; }
      expect_renderer_video_pipelines_max_at_most="$2"
      shift 2
      ;;
    --expect-adaptive-action)
      [[ $# -ge 2 ]] || { echo "--expect-adaptive-action requires a value" >&2; exit 2; }
      expect_adaptive_action="$2"
      shift 2
      ;;
    --expect-decoder-policy-status)
      [[ $# -ge 2 ]] || { echo "--expect-decoder-policy-status requires a value" >&2; exit 2; }
      expect_decoder_policy_status="$2"
      shift 2
      ;;
    --expect-decoder-class)
      [[ $# -ge 2 ]] || { echo "--expect-decoder-class requires a value" >&2; exit 2; }
      expect_decoder_class="$2"
      shift 2
      ;;
    --expect-memory-feature)
      [[ $# -ge 2 ]] || { echo "--expect-memory-feature requires a value" >&2; exit 2; }
      expect_memory_feature="$2"
      shift 2
      ;;
    --expect-sink-memory-feature)
      [[ $# -ge 2 ]] || { echo "--expect-sink-memory-feature requires a value" >&2; exit 2; }
      expect_sink_memory_feature="$2"
      shift 2
      ;;
    --expect-zero-copy-evidence)
      [[ $# -ge 2 ]] || { echo "--expect-zero-copy-evidence requires a value" >&2; exit 2; }
      expect_zero_copy_evidence="$2"
      shift 2
      ;;
    --expect-video-position-progress)
      expect_video_position_progress=1
      shift
      ;;
    --expect-frame-limiter-enabled)
      expect_frame_limiter_enabled=1
      shift
      ;;
    --expect-frame-limiter-max-fps)
      [[ $# -ge 2 ]] || { echo "--expect-frame-limiter-max-fps requires a value" >&2; exit 2; }
      expect_frame_limiter_max_fps="$2"
      shift 2
      ;;
    --expect-video-qos)
      expect_video_qos=1
      shift
      ;;
    --expect-qos-dropped-max-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-qos-dropped-max-at-most requires a value" >&2; exit 2; }
      expect_qos_dropped_max_at_most="$2"
      shift 2
      ;;
    --expect-gtk-frame-clock)
      expect_gtk_frame_clock=1
      shift
      ;;
    --expect-gtk-frame-clock-phase)
      [[ $# -ge 2 ]] || { echo "--expect-gtk-frame-clock-phase requires a value" >&2; exit 2; }
      case "$2" in
        before-paint|update|layout|paint|after-paint)
          expect_gtk_frame_clock_phases+=("$2")
          ;;
        all)
          expect_gtk_frame_clock_phases+=(before-paint update layout paint after-paint)
          ;;
        *)
          echo "--expect-gtk-frame-clock-phase must be one of before-paint, update, layout, paint, after-paint, all" >&2
          exit 2
          ;;
      esac
      shift 2
      ;;
    --expect-gtk-frame-timings)
      expect_gtk_frame_timings=1
      shift
      ;;
    --allow-missing)
      allow_missing=1
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

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
decision_summary_awk="$repo_root/scripts/performance-decision-summary.awk"
cd "$repo_root"

failures=0
skips=0
passes=0

note() {
  printf '%s\n' "$*"
}

pass() {
  passes=$((passes + 1))
  note "PASS: $*"
}

skip_or_fail() {
  if [[ "$allow_missing" -eq 1 ]]; then
    skips=$((skips + 1))
    note "SKIP: $*"
  else
    failures=$((failures + 1))
    note "FAIL: $*"
  fi
}

require_command() {
  local command="$1"
  if ! command -v "$command" >/dev/null 2>&1; then
    skip_or_fail "$command is not available"
    return 1
  fi
  return 0
}

is_positive_integer() {
  [[ "$1" =~ ^[1-9][0-9]*$ ]]
}

find_gilderd_pid() {
  local current_user="${USER:-$(id -un 2>/dev/null || true)}"
  while read -r candidate_pid candidate_user candidate_comm; do
    if [[ "$candidate_comm" == "gilderd" && ( -z "$current_user" || "$candidate_user" == "$current_user" ) ]]; then
      printf '%s\n' "$candidate_pid"
      return 0
    fi
  done < <(ps -eo pid=,user=,comm=)
  return 1
}

resolve_gilderctl() {
  if [[ -n "$gilderctl" ]]; then
    [[ -x "$gilderctl" ]] && return 0
    skip_or_fail "missing executable $gilderctl"
    return 1
  fi
  if [[ -x target/debug/gilderctl ]]; then
    gilderctl="target/debug/gilderctl"
    return 0
  fi
  if gilderctl_path="$(command -v gilderctl 2>/dev/null)"; then
    gilderctl="$gilderctl_path"
    return 0
  fi
  skip_or_fail "gilderctl is not available"
  return 1
}

sample_process() {
  local target_pid="$1"
  ps -p "$target_pid" -o pid=,pcpu=,rss=,vsz=,stat=,comm= | sed -n '1p'
}

sample_smaps_rollup() {
  local target_pid="$1"
  local rollup="/proc/${target_pid}/smaps_rollup"
  if [[ ! -r "$rollup" ]]; then
    printf '0 0 0 0 0 0 0 0\n'
    return 0
  fi

  awk '
    /^Pss:/ { pss = $2 + 0 }
    /^Private_Clean:/ { private_clean = $2 + 0 }
    /^Private_Dirty:/ { private_dirty = $2 + 0 }
    /^Shared_Clean:/ { shared_clean = $2 + 0 }
    /^Shared_Dirty:/ { shared_dirty = $2 + 0 }
    END {
      private_total = private_clean + private_dirty
      uss = private_total
      shared_total = shared_clean + shared_dirty
      printf "%d %d %d %d %d %d %d %d\n", pss, private_clean, private_dirty, private_total, uss, shared_clean, shared_dirty, shared_total
    }
  ' "$rollup"
}

sample_gpu_busy() {
  local values=()
  local sources=()
  local path
  local value
  local source

  for path in /sys/class/drm/card*/device/gpu_busy_percent /sys/class/drm/renderD*/device/gpu_busy_percent; do
    [[ -r "$path" ]] || continue
    value="$(sed -n '1p' "$path" 2>/dev/null | tr -d '[:space:]')"
    [[ "$value" =~ ^[0-9]+$ ]] || continue
    source="${path#/sys/class/drm/}"
    source="${source%/device/gpu_busy_percent}"
    values+=("$value")
    sources+=("$source")
  done

  if command -v nvidia-smi >/dev/null 2>&1; then
    local index=0
    while IFS= read -r value; do
      value="${value//[[:space:]]/}"
      [[ "$value" =~ ^[0-9]+$ ]] || continue
      values+=("$value")
      sources+=("nvidia-smi:${index}")
      index=$((index + 1))
    done < <(nvidia-smi --query-gpu=utilization.gpu --format=csv,noheader,nounits 2>/dev/null || true)
  fi

  if [[ "${#values[@]}" -eq 0 ]]; then
    printf '||\n'
    return 0
  fi

  local sum=0
  local max=0
  for value in "${values[@]}"; do
    sum=$((sum + value))
    if [[ "$value" -gt "$max" ]]; then
      max="$value"
    fi
  done
  local avg=$((sum / ${#values[@]}))
  local joined_sources
  joined_sources="$(IFS=';'; printf '%s' "${sources[*]}")"
  printf '%s|%s|%s\n' "$avg" "$max" "$joined_sources"
}

write_summary() {
  local csv="$1"
  local summary="$2"
  awk -F, '
    NR == 1 { next }
    {
      samples += 1
      cpu_sum += $4
      rss = $5 + 0
      vsz = $6 + 0
      pss = $7 + 0
      private = $10 + 0
      uss = $11 + 0
      shared = $14 + 0
      gpu_avg = $19
      gpu_max_sample = $20
      if (samples == 1) {
        first_rss = rss
        first_vsz = vsz
        first_pss = pss
        first_private = private
        first_uss = uss
        first_shared = shared
      }
      last_rss = rss
      last_vsz = vsz
      last_pss = pss
      last_private = private
      last_uss = uss
      last_shared = shared
      rss_sum += rss
      vsz_sum += vsz
      pss_sum += pss
      private_sum += private
      uss_sum += uss
      shared_sum += shared
      if (gpu_avg != "") {
        gpu_samples += 1
        gpu_sum += gpu_avg + 0
      }
      if (samples == 1 || rss < min_rss) { min_rss = rss }
      if (samples == 1 || vsz < min_vsz) { min_vsz = vsz }
      if (samples == 1 || pss < min_pss) { min_pss = pss }
      if (samples == 1 || private < min_private) { min_private = private }
      if (samples == 1 || uss < min_uss) { min_uss = uss }
      if (samples == 1 || shared < min_shared) { min_shared = shared }
      if ($5 + 0 > max_rss) { max_rss = $5 + 0 }
      if ($6 + 0 > max_vsz) { max_vsz = $6 + 0 }
      if (pss > max_pss) { max_pss = pss }
      if (private > max_private) { max_private = private }
      if (uss > max_uss) { max_uss = uss }
      if (shared > max_shared) { max_shared = shared }
      if (gpu_max_sample != "" && gpu_max_sample + 0 > max_gpu_busy) { max_gpu_busy = gpu_max_sample + 0 }
    }
    END {
      printf "samples: %d\n", samples
      if (samples > 0) {
        printf "avg_cpu_percent: %.2f\n", cpu_sum / samples
        printf "min_rss_kib: %d\n", min_rss
        printf "first_rss_kib: %d\n", first_rss
        printf "avg_rss_kib: %.0f\n", rss_sum / samples
        printf "last_rss_kib: %d\n", last_rss
        printf "max_rss_kib: %d\n", max_rss
        printf "retained_rss_delta_kib: %d\n", last_rss - first_rss
        printf "peak_over_first_rss_kib: %d\n", max_rss - first_rss
        printf "min_vsz_kib: %d\n", min_vsz
        printf "first_vsz_kib: %d\n", first_vsz
        printf "avg_vsz_kib: %.0f\n", vsz_sum / samples
        printf "last_vsz_kib: %d\n", last_vsz
        printf "max_vsz_kib: %d\n", max_vsz
        printf "retained_vsz_delta_kib: %d\n", last_vsz - first_vsz
        printf "peak_over_first_vsz_kib: %d\n", max_vsz - first_vsz
        printf "min_pss_kib: %d\n", min_pss
        printf "first_pss_kib: %d\n", first_pss
        printf "avg_pss_kib: %.0f\n", pss_sum / samples
        printf "last_pss_kib: %d\n", last_pss
        printf "max_pss_kib: %d\n", max_pss
        printf "retained_pss_delta_kib: %d\n", last_pss - first_pss
        printf "peak_over_first_pss_kib: %d\n", max_pss - first_pss
        printf "min_private_kib: %d\n", min_private
        printf "first_private_kib: %d\n", first_private
        printf "avg_private_kib: %.0f\n", private_sum / samples
        printf "last_private_kib: %d\n", last_private
        printf "max_private_kib: %d\n", max_private
        printf "retained_private_delta_kib: %d\n", last_private - first_private
        printf "peak_over_first_private_kib: %d\n", max_private - first_private
        printf "min_uss_kib: %d\n", min_uss
        printf "first_uss_kib: %d\n", first_uss
        printf "avg_uss_kib: %.0f\n", uss_sum / samples
        printf "last_uss_kib: %d\n", last_uss
        printf "max_uss_kib: %d\n", max_uss
        printf "retained_uss_delta_kib: %d\n", last_uss - first_uss
        printf "peak_over_first_uss_kib: %d\n", max_uss - first_uss
        printf "min_shared_kib: %d\n", min_shared
        printf "first_shared_kib: %d\n", first_shared
        printf "avg_shared_kib: %.0f\n", shared_sum / samples
        printf "last_shared_kib: %d\n", last_shared
        printf "max_shared_kib: %d\n", max_shared
        printf "retained_shared_delta_kib: %d\n", last_shared - first_shared
        printf "peak_over_first_shared_kib: %d\n", max_shared - first_shared
        printf "gpu_busy_samples: %d\n", gpu_samples
        if (gpu_samples > 0) {
          printf "avg_gpu_busy_percent: %.0f\n", gpu_sum / gpu_samples
          printf "max_gpu_busy_percent: %d\n", max_gpu_busy
        }
      }
    }
  ' "$csv" > "$summary"
}

append_status_decisions() {
  local sample="$1"
  local elapsed="$2"
  local status_file="$3"
  local decisions_csv="$4"
  local decision_error_file="$5"
  local temp_decisions="$work_dir/decisions-$(printf '%03d' "$sample").tmp"

  if ! "$gilderctl" status --decisions-csv --from-file "$status_file" > "$temp_decisions" 2> "$decision_error_file"; then
    rm -f "$temp_decisions"
    return 1
  fi
  if [[ ! -s "$decision_error_file" ]]; then
    rm -f "$decision_error_file"
  fi

  awk -v sample="$sample" -v elapsed="$elapsed" '
    NR == 1 { next }
    {
      print sample "," elapsed "," $0
    }
  ' "$temp_decisions" >> "$decisions_csv"
  rm -f "$temp_decisions"
  return 0
}

append_status_telemetry() {
  local sample="$1"
  local elapsed="$2"
  local status_file="$3"
  local telemetry_csv="$4"
  local telemetry_error_file="$5"
  local temp_telemetry="$work_dir/telemetry-$(printf '%03d' "$sample").tmp"

  if ! "$gilderctl" status --telemetry-csv --from-file "$status_file" > "$temp_telemetry" 2> "$telemetry_error_file"; then
    rm -f "$temp_telemetry"
    return 1
  fi
  if [[ ! -s "$telemetry_error_file" ]]; then
    rm -f "$telemetry_error_file"
  fi

  awk -v sample="$sample" -v elapsed="$elapsed" '
    NR == 1 { next }
    {
      print sample "," elapsed "," $0
    }
  ' "$temp_telemetry" >> "$telemetry_csv"
  rm -f "$temp_telemetry"
  return 0
}

append_status_video_runtime() {
  local sample="$1"
  local elapsed="$2"
  local status_file="$3"
  local video_runtime_csv="$4"
  local video_runtime_error_file="$5"
  local temp_video_runtime="$work_dir/video-runtime-$(printf '%03d' "$sample").tmp"

  if ! "$gilderctl" status --video-runtime-csv --from-file "$status_file" > "$temp_video_runtime" 2> "$video_runtime_error_file"; then
    rm -f "$temp_video_runtime"
    return 1
  fi
  if [[ ! -s "$video_runtime_error_file" ]]; then
    rm -f "$video_runtime_error_file"
  fi

  awk -v sample="$sample" -v elapsed="$elapsed" '
    NR == 1 { next }
    {
      print sample "," elapsed "," $0
    }
  ' "$temp_video_runtime" >> "$video_runtime_csv"
  rm -f "$temp_video_runtime"
  return 0
}

write_decision_summary() {
  local decisions_csv="$1"
  local summary="$2"
  awk -f "$decision_summary_awk" "$decisions_csv" > "$summary"
}

write_telemetry_summary() {
  local telemetry_csv="$1"
  local summary="$2"
  awk -F, '
    NR == 1 { next }
    {
      rows += 1
      refreshes = $3 + 0
      skips = $4 + 0
      changes = $5 + 0
      age = $6 + 0
      hits = $7 + 0
      misses = $8 + 0
      queued = $9 + 0
      update_skips = $10 + 0
      package_cache_entries = $11 + 0
      package_cache_max_entries = $12 + 0
      package_cache_hits = $13 + 0
      package_cache_misses = $14 + 0
      package_cache_evictions = $15 + 0
      archive_cache_entries = $16 + 0
      archive_cache_max_entries = $17 + 0
      archive_cache_reuses = $18 + 0
      archive_cache_extractions = $19 + 0
      archive_cache_evictions = $20 + 0
      archive_cache_evictions_latest = $21 + 0
      archive_cache_eviction_errors = $22 + 0
      archive_cache_eviction_errors_latest = $23 + 0
      planned_static_image_resources = $24 + 0
      planned_video_poster_resources = $25 + 0
      planned_slideshow_image_resources = $26 + 0
      planned_image_resource_references = $27 + 0
      planned_unique_image_resources = $28 + 0
      adaptive_refreshes = $29 + 0
      adaptive_skips = $30 + 0
      adaptive_triggers = $31 + 0
      cpu_pressure = $32 + 0
      memory_pressure = $33 + 0
      temperature = $34 + 0
      external_online = $35
      battery_present = $36
      battery_discharging = $37
      battery_capacity = $38 + 0
      battery_power = $39 + 0
      daemon_gpu_avg = $40
      daemon_gpu_max_sample = $41
      daemon_gpu_sources = $42
      adaptive_action_types = $43
      adaptive_action_scopes = $44
      adaptive_action_configured_actions = $45
      adaptive_action_max_fps = $46
      renderer_output_windows = $47 + 0
      renderer_static_surfaces = $48 + 0
      renderer_slideshow_surfaces = $49 + 0
      renderer_video_surfaces = $50 + 0
      renderer_video_pipelines = $51 + 0
      renderer_video_qos_messages = $52 + 0
      renderer_video_qos_dropped_max = $53
      renderer_video_gtk_frame_clock_ticks = $54 + 0
      renderer_video_gtk_frame_clock_interval_us_max = $55
      renderer_video_gtk_frame_clock_fps_x1000_max = $56
      renderer_video_gtk_frame_timings_complete = $57 + 0
      renderer_video_gtk_frame_timings_presentation_interval_us_max = $58
      renderer_video_gtk_frame_timings_presentation_time_us_max = $59
      renderer_video_gtk_frame_clock_before_paint_ticks = $60 + 0
      renderer_video_gtk_frame_clock_update_ticks = $61 + 0
      renderer_video_gtk_frame_clock_layout_ticks = $62 + 0
      renderer_video_gtk_frame_clock_paint_ticks = $63 + 0
      renderer_video_gtk_frame_clock_after_paint_ticks = $64 + 0
      planned_static_image_resource_bytes = $65 + 0
      planned_video_poster_resource_bytes = $66 + 0
      planned_slideshow_image_resource_bytes = $67 + 0
      planned_image_resource_reference_bytes = $68 + 0
      planned_unique_image_resource_bytes = $69 + 0
      package_cache_retained_resource_references = $70 + 0
      package_cache_retained_unique_resources = $71 + 0
      package_cache_retained_resource_bytes = $72 + 0
      package_cache_retained_unique_resource_bytes = $73 + 0

      if (rows == 1) {
        first_refreshes = refreshes
        first_skips = skips
        first_changes = changes
        first_hits = hits
        first_misses = misses
        first_queued = queued
        first_update_skips = update_skips
        first_archive_cache_evictions = archive_cache_evictions
        first_archive_cache_eviction_errors = archive_cache_eviction_errors
        first_adaptive_refreshes = adaptive_refreshes
        first_adaptive_skips = adaptive_skips
      }
      last_refreshes = refreshes
      last_skips = skips
      last_changes = changes
      last_hits = hits
      last_misses = misses
      last_queued = queued
      last_update_skips = update_skips
      last_package_cache_entries = package_cache_entries
      last_package_cache_max_entries = package_cache_max_entries
      last_package_cache_hits = package_cache_hits
      last_package_cache_misses = package_cache_misses
      last_package_cache_evictions = package_cache_evictions
      last_archive_cache_entries = archive_cache_entries
      last_archive_cache_max_entries = archive_cache_max_entries
      last_archive_cache_reuses = archive_cache_reuses
      last_archive_cache_extractions = archive_cache_extractions
      last_archive_cache_evictions = archive_cache_evictions
      last_archive_cache_evictions_latest = archive_cache_evictions_latest
      last_archive_cache_eviction_errors = archive_cache_eviction_errors
      last_archive_cache_eviction_errors_latest = archive_cache_eviction_errors_latest
      last_planned_static_image_resources = planned_static_image_resources
      last_planned_video_poster_resources = planned_video_poster_resources
      last_planned_slideshow_image_resources = planned_slideshow_image_resources
      last_planned_image_resource_references = planned_image_resource_references
      last_planned_unique_image_resources = planned_unique_image_resources
      last_planned_static_image_resource_bytes = planned_static_image_resource_bytes
      last_planned_video_poster_resource_bytes = planned_video_poster_resource_bytes
      last_planned_slideshow_image_resource_bytes = planned_slideshow_image_resource_bytes
      last_planned_image_resource_reference_bytes = planned_image_resource_reference_bytes
      last_planned_unique_image_resource_bytes = planned_unique_image_resource_bytes
      last_package_cache_retained_resource_references = package_cache_retained_resource_references
      last_package_cache_retained_unique_resources = package_cache_retained_unique_resources
      last_package_cache_retained_resource_bytes = package_cache_retained_resource_bytes
      last_package_cache_retained_unique_resource_bytes = package_cache_retained_unique_resource_bytes
      last_adaptive_refreshes = adaptive_refreshes
      last_adaptive_skips = adaptive_skips
      last_adaptive_triggers = adaptive_triggers
      if (age > max_age) { max_age = age }
      if (cpu_pressure > max_cpu_pressure) { max_cpu_pressure = cpu_pressure }
      if (memory_pressure > max_memory_pressure) { max_memory_pressure = memory_pressure }
      if (temperature > max_temperature) { max_temperature = temperature }
      last_external_online = external_online
      last_battery_present = battery_present
      last_battery_discharging = battery_discharging
      last_battery_capacity = battery_capacity
      last_battery_power = battery_power
      if (daemon_gpu_avg != "") {
        daemon_gpu_samples += 1
        daemon_gpu_sum += daemon_gpu_avg + 0
      }
      if (daemon_gpu_max_sample != "" && daemon_gpu_max_sample + 0 > max_daemon_gpu_busy) {
        max_daemon_gpu_busy = daemon_gpu_max_sample + 0
      }
      if (daemon_gpu_sources != "") {
        last_daemon_gpu_sources = daemon_gpu_sources
      }
      if (adaptive_action_types != "") {
        last_adaptive_action_types = adaptive_action_types
        split(adaptive_action_types, action_parts, "|")
        for (action_index in action_parts) {
          if (action_parts[action_index] != "") {
            adaptive_action_seen[action_parts[action_index]] = 1
          }
        }
      }
      if (adaptive_action_scopes != "") {
        last_adaptive_action_scopes = adaptive_action_scopes
      }
      if (adaptive_action_configured_actions != "") {
        last_adaptive_action_configured_actions = adaptive_action_configured_actions
      }
      if (adaptive_action_max_fps != "") {
        last_adaptive_action_max_fps = adaptive_action_max_fps
      }
      last_renderer_output_windows = renderer_output_windows
      if (renderer_output_windows > max_renderer_output_windows) {
        max_renderer_output_windows = renderer_output_windows
      }
      last_renderer_static_surfaces = renderer_static_surfaces
      if (renderer_static_surfaces > max_renderer_static_surfaces) {
        max_renderer_static_surfaces = renderer_static_surfaces
      }
      last_renderer_slideshow_surfaces = renderer_slideshow_surfaces
      if (renderer_slideshow_surfaces > max_renderer_slideshow_surfaces) {
        max_renderer_slideshow_surfaces = renderer_slideshow_surfaces
      }
      last_renderer_video_surfaces = renderer_video_surfaces
      if (renderer_video_surfaces > max_renderer_video_surfaces) {
        max_renderer_video_surfaces = renderer_video_surfaces
      }
      last_renderer_video_pipelines = renderer_video_pipelines
      if (renderer_video_pipelines > max_renderer_video_pipelines) {
        max_renderer_video_pipelines = renderer_video_pipelines
      }
      last_renderer_video_qos_messages = renderer_video_qos_messages
      if (renderer_video_qos_messages > max_renderer_video_qos_messages) {
        max_renderer_video_qos_messages = renderer_video_qos_messages
      }
      if (renderer_video_qos_dropped_max != "") {
        last_renderer_video_qos_dropped_max = renderer_video_qos_dropped_max
        if (renderer_video_qos_dropped_max + 0 > max_renderer_video_qos_dropped) {
          max_renderer_video_qos_dropped = renderer_video_qos_dropped_max + 0
        }
      }
      last_renderer_video_gtk_frame_clock_ticks = renderer_video_gtk_frame_clock_ticks
      if (renderer_video_gtk_frame_clock_ticks > max_renderer_video_gtk_frame_clock_ticks) {
        max_renderer_video_gtk_frame_clock_ticks = renderer_video_gtk_frame_clock_ticks
      }
      last_renderer_video_gtk_frame_clock_before_paint_ticks = renderer_video_gtk_frame_clock_before_paint_ticks
      if (renderer_video_gtk_frame_clock_before_paint_ticks > max_renderer_video_gtk_frame_clock_before_paint_ticks) {
        max_renderer_video_gtk_frame_clock_before_paint_ticks = renderer_video_gtk_frame_clock_before_paint_ticks
      }
      last_renderer_video_gtk_frame_clock_update_ticks = renderer_video_gtk_frame_clock_update_ticks
      if (renderer_video_gtk_frame_clock_update_ticks > max_renderer_video_gtk_frame_clock_update_ticks) {
        max_renderer_video_gtk_frame_clock_update_ticks = renderer_video_gtk_frame_clock_update_ticks
      }
      last_renderer_video_gtk_frame_clock_layout_ticks = renderer_video_gtk_frame_clock_layout_ticks
      if (renderer_video_gtk_frame_clock_layout_ticks > max_renderer_video_gtk_frame_clock_layout_ticks) {
        max_renderer_video_gtk_frame_clock_layout_ticks = renderer_video_gtk_frame_clock_layout_ticks
      }
      last_renderer_video_gtk_frame_clock_paint_ticks = renderer_video_gtk_frame_clock_paint_ticks
      if (renderer_video_gtk_frame_clock_paint_ticks > max_renderer_video_gtk_frame_clock_paint_ticks) {
        max_renderer_video_gtk_frame_clock_paint_ticks = renderer_video_gtk_frame_clock_paint_ticks
      }
      last_renderer_video_gtk_frame_clock_after_paint_ticks = renderer_video_gtk_frame_clock_after_paint_ticks
      if (renderer_video_gtk_frame_clock_after_paint_ticks > max_renderer_video_gtk_frame_clock_after_paint_ticks) {
        max_renderer_video_gtk_frame_clock_after_paint_ticks = renderer_video_gtk_frame_clock_after_paint_ticks
      }
      if (renderer_video_gtk_frame_clock_interval_us_max != "") {
        if (renderer_video_gtk_frame_clock_interval_us_max + 0 > max_renderer_video_gtk_frame_clock_interval_us) {
          max_renderer_video_gtk_frame_clock_interval_us = renderer_video_gtk_frame_clock_interval_us_max + 0
        }
      }
      if (renderer_video_gtk_frame_clock_fps_x1000_max != "") {
        if (renderer_video_gtk_frame_clock_fps_x1000_max + 0 > max_renderer_video_gtk_frame_clock_fps_x1000) {
          max_renderer_video_gtk_frame_clock_fps_x1000 = renderer_video_gtk_frame_clock_fps_x1000_max + 0
        }
      }
      last_renderer_video_gtk_frame_timings_complete = renderer_video_gtk_frame_timings_complete
      if (renderer_video_gtk_frame_timings_complete > max_renderer_video_gtk_frame_timings_complete) {
        max_renderer_video_gtk_frame_timings_complete = renderer_video_gtk_frame_timings_complete
      }
      if (renderer_video_gtk_frame_timings_presentation_interval_us_max != "") {
        if (renderer_video_gtk_frame_timings_presentation_interval_us_max + 0 > max_renderer_video_gtk_frame_timings_presentation_interval_us) {
          max_renderer_video_gtk_frame_timings_presentation_interval_us = renderer_video_gtk_frame_timings_presentation_interval_us_max + 0
        }
      }
      if (renderer_video_gtk_frame_timings_presentation_time_us_max != "") {
        last_renderer_video_gtk_frame_timings_presentation_time_us_max = renderer_video_gtk_frame_timings_presentation_time_us_max
      }
    }
    END {
      refresh_delta = last_refreshes - first_refreshes
      skip_delta = last_skips - first_skips
      change_delta = last_changes - first_changes
      hit_delta = last_hits - first_hits
      miss_delta = last_misses - first_misses
      queued_delta = last_queued - first_queued
      update_skip_delta = last_update_skips - first_update_skips
      archive_cache_eviction_delta = last_archive_cache_evictions - first_archive_cache_evictions
      archive_cache_eviction_error_delta = last_archive_cache_eviction_errors - first_archive_cache_eviction_errors
      adaptive_refresh_delta = last_adaptive_refreshes - first_adaptive_refreshes
      adaptive_skip_delta = last_adaptive_skips - first_adaptive_skips
      total_cache_delta = hit_delta + miss_delta

      printf "telemetry_rows: %d\n", rows
      if (rows > 0) {
        printf "desktop_refreshes_delta: %d\n", refresh_delta
        printf "desktop_refresh_skips_delta: %d\n", skip_delta
        printf "desktop_changes_delta: %d\n", change_delta
        printf "render_sync_cache_hits_delta: %d\n", hit_delta
        printf "render_sync_cache_misses_delta: %d\n", miss_delta
        printf "render_sync_updates_queued_delta: %d\n", queued_delta
        printf "render_sync_updates_skipped_delta: %d\n", update_skip_delta
        printf "render_sync_updates_queued_latest: %d\n", last_queued
        printf "render_sync_updates_skipped_latest: %d\n", last_update_skips
        printf "render_sync_package_cache_entries_latest: %d\n", last_package_cache_entries
        printf "render_sync_package_cache_max_entries_latest: %d\n", last_package_cache_max_entries
        printf "render_sync_package_cache_hits_latest: %d\n", last_package_cache_hits
        printf "render_sync_package_cache_misses_latest: %d\n", last_package_cache_misses
        printf "render_sync_package_cache_evictions_latest: %d\n", last_package_cache_evictions
        printf "render_sync_archive_cache_entries_latest: %d\n", last_archive_cache_entries
        printf "render_sync_archive_cache_max_entries_latest: %d\n", last_archive_cache_max_entries
        printf "render_sync_archive_cache_reuses_latest: %d\n", last_archive_cache_reuses
        printf "render_sync_archive_cache_extractions_latest: %d\n", last_archive_cache_extractions
        printf "render_sync_archive_cache_evictions_delta: %d\n", archive_cache_eviction_delta
        printf "render_sync_archive_cache_evictions_latest: %d\n", last_archive_cache_evictions_latest
        printf "render_sync_archive_cache_eviction_errors_delta: %d\n", archive_cache_eviction_error_delta
        printf "render_sync_archive_cache_eviction_errors_latest: %d\n", last_archive_cache_eviction_errors_latest
        printf "render_sync_planned_static_image_resources_latest: %d\n", last_planned_static_image_resources
        printf "render_sync_planned_video_poster_resources_latest: %d\n", last_planned_video_poster_resources
        printf "render_sync_planned_slideshow_image_resources_latest: %d\n", last_planned_slideshow_image_resources
        printf "render_sync_planned_image_resource_references_latest: %d\n", last_planned_image_resource_references
        printf "render_sync_planned_unique_image_resources_latest: %d\n", last_planned_unique_image_resources
        printf "render_sync_planned_static_image_resource_bytes_latest: %d\n", last_planned_static_image_resource_bytes
        printf "render_sync_planned_video_poster_resource_bytes_latest: %d\n", last_planned_video_poster_resource_bytes
        printf "render_sync_planned_slideshow_image_resource_bytes_latest: %d\n", last_planned_slideshow_image_resource_bytes
        printf "render_sync_planned_image_resource_reference_bytes_latest: %d\n", last_planned_image_resource_reference_bytes
        printf "render_sync_planned_unique_image_resource_bytes_latest: %d\n", last_planned_unique_image_resource_bytes
        printf "render_sync_package_cache_retained_resource_references_latest: %d\n", last_package_cache_retained_resource_references
        printf "render_sync_package_cache_retained_unique_resources_latest: %d\n", last_package_cache_retained_unique_resources
        printf "render_sync_package_cache_retained_resource_bytes_latest: %d\n", last_package_cache_retained_resource_bytes
        printf "render_sync_package_cache_retained_unique_resource_bytes_latest: %d\n", last_package_cache_retained_unique_resource_bytes
        printf "adaptive_refreshes_delta: %d\n", adaptive_refresh_delta
        printf "adaptive_refresh_skips_delta: %d\n", adaptive_skip_delta
        printf "adaptive_active_triggers_latest: %d\n", last_adaptive_triggers
        if (total_cache_delta > 0) {
          printf "render_sync_cache_hit_ratio: %.4f\n", hit_delta / total_cache_delta
        }
        printf "last_desktop_refresh_age_ms_max: %d\n", max_age
        printf "cpu_pressure_some_avg10_x100_max: %d\n", max_cpu_pressure
        printf "memory_pressure_some_avg10_x100_max: %d\n", max_memory_pressure
        printf "temperature_max_millicelsius_max: %d\n", max_temperature
        printf "power_external_online_latest: %s\n", last_external_online
        printf "power_system_battery_present_latest: %s\n", last_battery_present
        printf "power_battery_discharging_latest: %s\n", last_battery_discharging
        printf "power_battery_capacity_percent_latest: %d\n", last_battery_capacity
        printf "power_battery_power_microwatts_latest: %d\n", last_battery_power
        printf "daemon_gpu_busy_samples: %d\n", daemon_gpu_samples
        if (daemon_gpu_samples > 0) {
          printf "daemon_avg_gpu_busy_percent: %.0f\n", daemon_gpu_sum / daemon_gpu_samples
          printf "daemon_max_gpu_busy_percent: %d\n", max_daemon_gpu_busy
          printf "daemon_gpu_busy_sources_latest: %s\n", last_daemon_gpu_sources
        }
        printf "adaptive_action_types_latest: %s\n", last_adaptive_action_types
        printf "adaptive_action_scopes_latest: %s\n", last_adaptive_action_scopes
        printf "adaptive_action_configured_actions_latest: %s\n", last_adaptive_action_configured_actions
        printf "adaptive_action_max_fps_latest: %s\n", last_adaptive_action_max_fps
        for (action in adaptive_action_seen) {
          printf "adaptive_action.%s: 1\n", action
        }
        printf "renderer_output_windows_latest: %d\n", last_renderer_output_windows
        printf "renderer_output_windows_max: %d\n", max_renderer_output_windows
        printf "renderer_static_surfaces_latest: %d\n", last_renderer_static_surfaces
        printf "renderer_static_surfaces_max: %d\n", max_renderer_static_surfaces
        printf "renderer_slideshow_surfaces_latest: %d\n", last_renderer_slideshow_surfaces
        printf "renderer_slideshow_surfaces_max: %d\n", max_renderer_slideshow_surfaces
        printf "renderer_video_surfaces_latest: %d\n", last_renderer_video_surfaces
        printf "renderer_video_surfaces_max: %d\n", max_renderer_video_surfaces
        printf "renderer_video_pipelines_latest: %d\n", last_renderer_video_pipelines
        printf "renderer_video_pipelines_max: %d\n", max_renderer_video_pipelines
        printf "renderer_video_qos_messages_latest: %d\n", last_renderer_video_qos_messages
        printf "renderer_video_qos_messages_max: %d\n", max_renderer_video_qos_messages
        printf "renderer_video_qos_dropped_max_latest: %s\n", last_renderer_video_qos_dropped_max
        printf "renderer_video_qos_dropped_max: %d\n", max_renderer_video_qos_dropped
        printf "renderer_video_gtk_frame_clock_ticks_latest: %d\n", last_renderer_video_gtk_frame_clock_ticks
        printf "renderer_video_gtk_frame_clock_ticks_max: %d\n", max_renderer_video_gtk_frame_clock_ticks
        printf "renderer_video_gtk_frame_clock_before_paint_ticks_latest: %d\n", last_renderer_video_gtk_frame_clock_before_paint_ticks
        printf "renderer_video_gtk_frame_clock_before_paint_ticks_max: %d\n", max_renderer_video_gtk_frame_clock_before_paint_ticks
        printf "renderer_video_gtk_frame_clock_update_ticks_latest: %d\n", last_renderer_video_gtk_frame_clock_update_ticks
        printf "renderer_video_gtk_frame_clock_update_ticks_max: %d\n", max_renderer_video_gtk_frame_clock_update_ticks
        printf "renderer_video_gtk_frame_clock_layout_ticks_latest: %d\n", last_renderer_video_gtk_frame_clock_layout_ticks
        printf "renderer_video_gtk_frame_clock_layout_ticks_max: %d\n", max_renderer_video_gtk_frame_clock_layout_ticks
        printf "renderer_video_gtk_frame_clock_paint_ticks_latest: %d\n", last_renderer_video_gtk_frame_clock_paint_ticks
        printf "renderer_video_gtk_frame_clock_paint_ticks_max: %d\n", max_renderer_video_gtk_frame_clock_paint_ticks
        printf "renderer_video_gtk_frame_clock_after_paint_ticks_latest: %d\n", last_renderer_video_gtk_frame_clock_after_paint_ticks
        printf "renderer_video_gtk_frame_clock_after_paint_ticks_max: %d\n", max_renderer_video_gtk_frame_clock_after_paint_ticks
        printf "renderer_video_gtk_frame_clock_interval_us_max: %d\n", max_renderer_video_gtk_frame_clock_interval_us
        printf "renderer_video_gtk_frame_clock_fps_x1000_max: %d\n", max_renderer_video_gtk_frame_clock_fps_x1000
        printf "renderer_video_gtk_frame_timings_complete_latest: %d\n", last_renderer_video_gtk_frame_timings_complete
        printf "renderer_video_gtk_frame_timings_complete_max: %d\n", max_renderer_video_gtk_frame_timings_complete
        printf "renderer_video_gtk_frame_timings_presentation_interval_us_max: %d\n", max_renderer_video_gtk_frame_timings_presentation_interval_us
        printf "renderer_video_gtk_frame_timings_presentation_time_us_latest: %s\n", last_renderer_video_gtk_frame_timings_presentation_time_us_max
      }
    }
  ' "$telemetry_csv" > "$summary"
}

write_video_runtime_summary() {
  local video_runtime_csv="$1"
  local summary="$2"
  awk -F, '
    function record_pipe_values(value, counts,    item, part_count, parts, part_index) {
      if (value == "") {
        return
      }
      part_count = split(value, parts, /\|/)
      for (part_index = 1; part_index <= part_count; part_index++) {
        item = parts[part_index]
        if (item != "") {
          counts[item] += 1
        }
      }
    }
    NR == 1 { next }
    {
      rows += 1
      sample = $1
      output = $3
      mode = $4
      gst_state = $5
      decoder_policy = $6
      decoder_policy_status = $7
      actual_decoders = $8
      decoder_classes = $9
      caps_report_count = $10
      memory_features = $11
      sink_memory_features = $12
      zero_copy_level = $13
      zero_copy_notes = $14
      position = $17
      duration = $18
      limiter_enabled = $19
      limiter_fps = $20
      qos_messages = $21
      qos_processed = $22
      qos_dropped = $23
      qos_format = $24
      qos_jitter = $25
      qos_jitter_abs = $26
      qos_proportion = $27
      gtk_clock_ticks = $28
      gtk_clock_counter = $29
      gtk_clock_time = $30
      gtk_clock_interval = $31
      gtk_clock_interval_max = $32
      gtk_clock_fps = $33
      gtk_clock_refresh = $34
      gtk_clock_presentation = $35
      gtk_timings_observed = $36
      gtk_timings_complete = $37
      gtk_timings_counter = $38
      gtk_timings_complete_counter = $39
      gtk_timings_frame_time = $40
      gtk_timings_predicted_presentation = $41
      gtk_timings_presentation = $42
      gtk_timings_presentation_interval = $43
      gtk_timings_presentation_interval_max = $44
      gtk_timings_refresh = $45
      gtk_clock_before_paint_ticks = $47
      gtk_clock_update_ticks = $48
      gtk_clock_layout_ticks = $49
      gtk_clock_paint_ticks = $50
      gtk_clock_after_paint_ticks = $51

      if (output != "" && !(output in seen_output)) {
        seen_output[output] = 1
        outputs += 1
      }
      if (sample != "" && !(sample in seen_sample)) {
        seen_sample[sample] = 1
        samples += 1
      }
      if (mode != "") {
        video_modes[mode] += 1
        last_mode = mode
      }
      if (gst_state != "") {
        gst_states[gst_state] += 1
        last_gst_state = gst_state
      }
      if (decoder_policy != "") {
        decoder_policies[decoder_policy] += 1
        last_decoder_policy = decoder_policy
      }
      if (decoder_policy_status != "") {
        decoder_policy_statuses[decoder_policy_status] += 1
        last_decoder_policy_status = decoder_policy_status
      }
      if (actual_decoders != "") {
        last_actual_decoders = actual_decoders
        record_pipe_values(actual_decoders, actual_decoder_counts)
      }
      if (decoder_classes != "") {
        last_decoder_classes = decoder_classes
        record_pipe_values(decoder_classes, decoder_class_counts)
      }
      if (caps_report_count != "" && caps_report_count + 0 > max_caps_report_count) {
        max_caps_report_count = caps_report_count + 0
      }
      if (memory_features != "") {
        last_memory_features = memory_features
        record_pipe_values(memory_features, memory_feature_counts)
      }
      if (sink_memory_features != "") {
        last_sink_memory_features = sink_memory_features
        record_pipe_values(sink_memory_features, sink_memory_feature_counts)
      }
      if (zero_copy_level != "") {
        zero_copy_rows += 1
        zero_copy_levels[zero_copy_level] += 1
        last_zero_copy_level = zero_copy_level
      }
      if (zero_copy_notes != "") {
        last_zero_copy_notes = zero_copy_notes
      }
      if (position != "") {
        position_samples += 1
        if (!(output in first_position)) {
          first_position[output] = position + 0
        }
        last_position[output] = position + 0
      }
      if (duration != "") {
        duration_samples += 1
        if (duration + 0 > max_duration) { max_duration = duration + 0 }
      }
      if (limiter_enabled == "true") {
        frame_limiter_enabled_rows += 1
      }
      if (limiter_fps != "") {
        limiter_fps_samples += 1
        if (limiter_fps_samples == 1 || limiter_fps + 0 < min_limiter_fps) {
          min_limiter_fps = limiter_fps + 0
        }
        if (limiter_fps + 0 > max_limiter_fps) {
          max_limiter_fps = limiter_fps + 0
        }
      }
      if (qos_messages != "") {
        qos_rows += 1
        if (qos_messages + 0 > max_qos_messages) { max_qos_messages = qos_messages + 0 }
      }
      if (qos_processed != "" && qos_processed + 0 > max_qos_processed) {
        max_qos_processed = qos_processed + 0
      }
      if (qos_dropped != "" && qos_dropped + 0 > max_qos_dropped) {
        max_qos_dropped = qos_dropped + 0
      }
      if (qos_format != "") {
        last_qos_format = qos_format
      }
      if (qos_jitter != "") {
        last_qos_jitter = qos_jitter
      }
      if (qos_jitter_abs != "" && qos_jitter_abs + 0 > max_qos_jitter_abs) {
        max_qos_jitter_abs = qos_jitter_abs + 0
      }
      if (qos_proportion != "") {
        last_qos_proportion = qos_proportion
      }
      if (gtk_clock_ticks != "") {
        if (gtk_clock_ticks + 0 > 0) { gtk_clock_rows += 1 }
        if (gtk_clock_ticks + 0 > max_gtk_clock_ticks) {
          max_gtk_clock_ticks = gtk_clock_ticks + 0
        }
      }
      if (gtk_clock_before_paint_ticks != "" && gtk_clock_before_paint_ticks + 0 > max_gtk_clock_before_paint_ticks) {
        max_gtk_clock_before_paint_ticks = gtk_clock_before_paint_ticks + 0
      }
      if (gtk_clock_update_ticks != "" && gtk_clock_update_ticks + 0 > max_gtk_clock_update_ticks) {
        max_gtk_clock_update_ticks = gtk_clock_update_ticks + 0
      }
      if (gtk_clock_layout_ticks != "" && gtk_clock_layout_ticks + 0 > max_gtk_clock_layout_ticks) {
        max_gtk_clock_layout_ticks = gtk_clock_layout_ticks + 0
      }
      if (gtk_clock_paint_ticks != "" && gtk_clock_paint_ticks + 0 > max_gtk_clock_paint_ticks) {
        max_gtk_clock_paint_ticks = gtk_clock_paint_ticks + 0
      }
      if (gtk_clock_after_paint_ticks != "" && gtk_clock_after_paint_ticks + 0 > max_gtk_clock_after_paint_ticks) {
        max_gtk_clock_after_paint_ticks = gtk_clock_after_paint_ticks + 0
      }
      if (gtk_clock_counter != "") {
        last_gtk_clock_counter = gtk_clock_counter
      }
      if (gtk_clock_time != "") {
        last_gtk_clock_time = gtk_clock_time
      }
      if (gtk_clock_interval != "") {
        last_gtk_clock_interval = gtk_clock_interval
      }
      if (gtk_clock_interval_max != "" && gtk_clock_interval_max + 0 > max_gtk_clock_interval) {
        max_gtk_clock_interval = gtk_clock_interval_max + 0
      }
      if (gtk_clock_fps != "") {
        last_gtk_clock_fps = gtk_clock_fps
      }
      if (gtk_clock_refresh != "") {
        last_gtk_clock_refresh = gtk_clock_refresh
      }
      if (gtk_clock_presentation != "") {
        last_gtk_clock_presentation = gtk_clock_presentation
      }
      if (gtk_timings_observed != "") {
        if (gtk_timings_observed + 0 > 0) { gtk_timings_rows += 1 }
        if (gtk_timings_observed + 0 > max_gtk_timings_observed) {
          max_gtk_timings_observed = gtk_timings_observed + 0
        }
      }
      if (gtk_timings_complete != "") {
        if (gtk_timings_complete + 0 > max_gtk_timings_complete) {
          max_gtk_timings_complete = gtk_timings_complete + 0
        }
      }
      if (gtk_timings_counter != "") {
        last_gtk_timings_counter = gtk_timings_counter
      }
      if (gtk_timings_complete_counter != "") {
        last_gtk_timings_complete_counter = gtk_timings_complete_counter
      }
      if (gtk_timings_frame_time != "") {
        last_gtk_timings_frame_time = gtk_timings_frame_time
      }
      if (gtk_timings_predicted_presentation != "") {
        last_gtk_timings_predicted_presentation = gtk_timings_predicted_presentation
      }
      if (gtk_timings_presentation != "") {
        last_gtk_timings_presentation = gtk_timings_presentation
      }
      if (gtk_timings_presentation_interval != "") {
        last_gtk_timings_presentation_interval = gtk_timings_presentation_interval
      }
      if (gtk_timings_presentation_interval_max != "" && gtk_timings_presentation_interval_max + 0 > max_gtk_timings_presentation_interval) {
        max_gtk_timings_presentation_interval = gtk_timings_presentation_interval_max + 0
      }
      if (gtk_timings_refresh != "") {
        last_gtk_timings_refresh = gtk_timings_refresh
      }
    }
    END {
      for (output in last_position) {
        delta = last_position[output] - first_position[output]
        if (delta > 0) {
          moving_outputs += 1
        }
        if (delta > max_position_delta) {
          max_position_delta = delta
        }
      }

      printf "video_runtime_rows: %d\n", rows
      printf "video_runtime_samples: %d\n", samples
      printf "video_runtime_outputs: %d\n", outputs
      if (last_mode != "") {
        printf "video_mode_latest: %s\n", last_mode
      }
      for (mode in video_modes) {
        printf "video_mode.%s: %d\n", mode, video_modes[mode]
      }
      if (last_gst_state != "") {
        printf "video_gst_state_latest: %s\n", last_gst_state
      }
      for (gst_state in gst_states) {
        printf "video_gst_state.%s: %d\n", gst_state, gst_states[gst_state]
      }
      if (last_decoder_policy != "") {
        printf "video_decoder_policy_latest: %s\n", last_decoder_policy
      }
      for (decoder_policy in decoder_policies) {
        printf "video_decoder_policy.%s: %d\n", decoder_policy, decoder_policies[decoder_policy]
      }
      if (last_decoder_policy_status != "") {
        printf "video_decoder_policy_status_latest: %s\n", last_decoder_policy_status
      }
      for (decoder_policy_status in decoder_policy_statuses) {
        printf "video_decoder_policy_status.%s: %d\n", decoder_policy_status, decoder_policy_statuses[decoder_policy_status]
      }
      if (last_actual_decoders != "") {
        printf "video_actual_decoders_latest: %s\n", last_actual_decoders
      }
      for (actual_decoder in actual_decoder_counts) {
        printf "video_actual_decoder.%s: %d\n", actual_decoder, actual_decoder_counts[actual_decoder]
      }
      if (last_decoder_classes != "") {
        printf "video_decoder_classes_latest: %s\n", last_decoder_classes
      }
      for (decoder_class in decoder_class_counts) {
        printf "video_decoder_class.%s: %d\n", decoder_class, decoder_class_counts[decoder_class]
      }
      printf "video_caps_report_count_max: %d\n", max_caps_report_count
      if (last_memory_features != "") {
        printf "video_memory_features_latest: %s\n", last_memory_features
      }
      for (memory_feature in memory_feature_counts) {
        printf "video_memory_feature.%s: %d\n", memory_feature, memory_feature_counts[memory_feature]
      }
      if (last_sink_memory_features != "") {
        printf "video_sink_memory_features_latest: %s\n", last_sink_memory_features
      }
      for (sink_memory_feature in sink_memory_feature_counts) {
        printf "video_sink_memory_feature.%s: %d\n", sink_memory_feature, sink_memory_feature_counts[sink_memory_feature]
      }
      printf "video_zero_copy_evidence_rows: %d\n", zero_copy_rows
      if (last_zero_copy_level != "") {
        printf "video_zero_copy_evidence_latest: %s\n", last_zero_copy_level
      }
      if (last_zero_copy_notes != "") {
        printf "video_zero_copy_evidence_notes_latest: %s\n", last_zero_copy_notes
      }
      for (level in zero_copy_levels) {
        printf "video_zero_copy_evidence.%s: %d\n", level, zero_copy_levels[level]
      }
      printf "video_position_samples: %d\n", position_samples
      printf "video_position_moving_outputs: %d\n", moving_outputs
      printf "video_position_delta_ms_max: %d\n", max_position_delta
      printf "video_duration_samples: %d\n", duration_samples
      if (duration_samples > 0) {
        printf "video_duration_ms_max: %d\n", max_duration
      }
      printf "video_frame_limiter_enabled_rows: %d\n", frame_limiter_enabled_rows
      printf "video_frame_limiter_fps_samples: %d\n", limiter_fps_samples
      if (limiter_fps_samples > 0) {
        printf "video_frame_limiter_fps_min: %d\n", min_limiter_fps
        printf "video_frame_limiter_fps_max: %d\n", max_limiter_fps
      }
      printf "video_qos_rows: %d\n", qos_rows
      printf "video_qos_messages_max: %d\n", max_qos_messages
      printf "video_qos_processed_max: %d\n", max_qos_processed
      printf "video_qos_dropped_max: %d\n", max_qos_dropped
      if (last_qos_format != "") {
        printf "video_qos_stats_format_latest: %s\n", last_qos_format
      }
      if (last_qos_jitter != "") {
        printf "video_qos_jitter_ns_latest: %s\n", last_qos_jitter
      }
      printf "video_qos_jitter_ns_abs_max: %d\n", max_qos_jitter_abs
      if (last_qos_proportion != "") {
        printf "video_qos_proportion_x1000_latest: %s\n", last_qos_proportion
      }
      printf "video_gtk_frame_clock_rows: %d\n", gtk_clock_rows
      printf "video_gtk_frame_clock_ticks_max: %d\n", max_gtk_clock_ticks
      printf "video_gtk_frame_clock_before_paint_ticks_max: %d\n", max_gtk_clock_before_paint_ticks
      printf "video_gtk_frame_clock_update_ticks_max: %d\n", max_gtk_clock_update_ticks
      printf "video_gtk_frame_clock_layout_ticks_max: %d\n", max_gtk_clock_layout_ticks
      printf "video_gtk_frame_clock_paint_ticks_max: %d\n", max_gtk_clock_paint_ticks
      printf "video_gtk_frame_clock_after_paint_ticks_max: %d\n", max_gtk_clock_after_paint_ticks
      if (last_gtk_clock_counter != "") {
        printf "video_gtk_frame_clock_counter_latest: %s\n", last_gtk_clock_counter
      }
      if (last_gtk_clock_time != "") {
        printf "video_gtk_frame_clock_time_us_latest: %s\n", last_gtk_clock_time
      }
      if (last_gtk_clock_interval != "") {
        printf "video_gtk_frame_clock_interval_us_latest: %s\n", last_gtk_clock_interval
      }
      printf "video_gtk_frame_clock_interval_us_max: %d\n", max_gtk_clock_interval
      if (last_gtk_clock_fps != "") {
        printf "video_gtk_frame_clock_fps_x1000_latest: %s\n", last_gtk_clock_fps
      }
      if (last_gtk_clock_refresh != "") {
        printf "video_gtk_frame_clock_refresh_interval_us_latest: %s\n", last_gtk_clock_refresh
      }
      if (last_gtk_clock_presentation != "") {
        printf "video_gtk_frame_clock_predicted_presentation_time_us_latest: %s\n", last_gtk_clock_presentation
      }
      printf "video_gtk_frame_timings_rows: %d\n", gtk_timings_rows
      printf "video_gtk_frame_timings_observed_max: %d\n", max_gtk_timings_observed
      printf "video_gtk_frame_timings_complete_max: %d\n", max_gtk_timings_complete
      if (last_gtk_timings_counter != "") {
        printf "video_gtk_frame_timings_counter_latest: %s\n", last_gtk_timings_counter
      }
      if (last_gtk_timings_complete_counter != "") {
        printf "video_gtk_frame_timings_complete_counter_latest: %s\n", last_gtk_timings_complete_counter
      }
      if (last_gtk_timings_frame_time != "") {
        printf "video_gtk_frame_timings_frame_time_us_latest: %s\n", last_gtk_timings_frame_time
      }
      if (last_gtk_timings_predicted_presentation != "") {
        printf "video_gtk_frame_timings_predicted_presentation_time_us_latest: %s\n", last_gtk_timings_predicted_presentation
      }
      if (last_gtk_timings_presentation != "") {
        printf "video_gtk_frame_timings_presentation_time_us_latest: %s\n", last_gtk_timings_presentation
      }
      if (last_gtk_timings_presentation_interval != "") {
        printf "video_gtk_frame_timings_presentation_interval_us_latest: %s\n", last_gtk_timings_presentation_interval
      }
      printf "video_gtk_frame_timings_presentation_interval_us_max: %d\n", max_gtk_timings_presentation_interval
      if (last_gtk_timings_refresh != "") {
        printf "video_gtk_frame_timings_refresh_interval_us_latest: %s\n", last_gtk_timings_refresh
      }
    }
  ' "$video_runtime_csv" > "$summary"
}

has_expectations() {
  [[ -n "$expect_mode" ||
    -n "$expect_reason" ||
    -n "$expect_action" ||
    -n "$expect_max_fps" ||
    -n "$expect_plan_kind" ]]
}

summary_value() {
  local key="$1"
  local summary="$2"
  awk -v key="$key" -F': ' '$1 == key { print $2; found = 1; exit } END { exit found ? 0 : 1 }' "$summary"
}

expect_summary_key() {
  local key="$1"
  local description="$2"
  local value
  if value="$(summary_value "$key" "$decision_summary_path")"; then
    pass "decision expectation matched ${description}: ${value}"
  else
    skip_or_fail "decision expectation not met: ${description}"
  fi
}

validate_decision_expectations() {
  has_expectations || return 0
  if [[ "$status_enabled" -ne 1 || "$decision_failures" -gt 0 ]]; then
    skip_or_fail "cannot validate decision expectations without complete decision samples"
    return 0
  fi

  if ! summary_value "decision_rows" "$decision_summary_path" >/dev/null; then
    skip_or_fail "cannot validate decision expectations because decision summary is missing"
    return 0
  fi
  local rows
  rows="$(summary_value "decision_rows" "$decision_summary_path")"
  if [[ "$rows" == "0" ]]; then
    skip_or_fail "cannot validate decision expectations because no decision rows were sampled"
    return 0
  fi

  if [[ -n "$expect_mode" && -n "$expect_reason" ]]; then
    expect_summary_key "mode_reason.${expect_mode}/${expect_reason}" "${expect_mode}/${expect_reason}"
  elif [[ -n "$expect_mode" ]]; then
    expect_summary_key "mode.${expect_mode}" "mode ${expect_mode}"
  elif [[ -n "$expect_reason" ]]; then
    expect_summary_key "reason.${expect_reason}" "reason ${expect_reason}"
  fi
  if [[ -n "$expect_action" ]]; then
    expect_summary_key "action.${expect_action}" "action ${expect_action}"
  fi
  if [[ -n "$expect_max_fps" ]]; then
    expect_summary_key "max_fps.${expect_max_fps}" "max_fps ${expect_max_fps}"
  fi
  if [[ -n "$expect_plan_kind" ]]; then
    expect_summary_key "plan_kind.${expect_plan_kind}" "plan kind ${expect_plan_kind}"
  fi
}

has_process_memory_expectations() {
  [[ -n "$expect_max_rss_kib_at_most" ||
    -n "$expect_max_pss_kib_at_most" ||
    -n "$expect_max_private_kib_at_most" ||
    -n "$expect_max_uss_kib_at_most" ||
    -n "$expect_max_shared_kib_at_most" ||
    -n "$expect_retained_rss_delta_kib_at_most" ||
    -n "$expect_retained_pss_delta_kib_at_most" ||
    -n "$expect_retained_private_delta_kib_at_most" ||
    -n "$expect_retained_uss_delta_kib_at_most" ||
    -n "$expect_retained_shared_delta_kib_at_most" ||
    -n "$expect_peak_over_first_rss_kib_at_most" ||
    -n "$expect_peak_over_first_pss_kib_at_most" ||
    -n "$expect_peak_over_first_private_kib_at_most" ||
    -n "$expect_peak_over_first_uss_kib_at_most" ||
    -n "$expect_peak_over_first_shared_kib_at_most" ]]
}

expect_process_summary_maximum() {
  local key="$1"
  local maximum="$2"
  local description="$3"
  local require_observed="$4"
  local value

  if value="$(summary_value "$key" "$summary_path")" && [[ "$value" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
    if [[ "$require_observed" -eq 1 ]] && awk -v value="$value" 'BEGIN { exit (value + 0 > 0) ? 0 : 1 }'; then
      :
    elif [[ "$require_observed" -eq 1 ]]; then
      skip_or_fail "process memory expectation not met: ${description} was not observed; /proc/<pid>/smaps_rollup may be unavailable"
      return 0
    fi
    if awk -v value="$value" -v maximum="$maximum" 'BEGIN { exit (value + 0 <= maximum + 0) ? 0 : 1 }'; then
      pass "process memory expectation matched ${description}: ${value} KiB"
    else
      skip_or_fail "process memory expectation not met: ${description} was ${value} KiB, expected at most ${maximum} KiB"
    fi
  else
    skip_or_fail "process memory expectation not met: missing ${description}"
  fi
}

expect_process_summary_delta_maximum() {
  local key="$1"
  local maximum="$2"
  local description="$3"
  local observed_key="$4"
  local value
  local observed

  if value="$(summary_value "$key" "$summary_path")" && [[ "$value" =~ ^-?[0-9]+([.][0-9]+)?$ ]]; then
    if [[ -n "$observed_key" ]]; then
      if observed="$(summary_value "$observed_key" "$summary_path")" && [[ "$observed" =~ ^[0-9]+([.][0-9]+)?$ ]] && awk -v value="$observed" 'BEGIN { exit (value + 0 > 0) ? 0 : 1 }'; then
        :
      else
        skip_or_fail "process memory expectation not met: ${description} base memory was not observed; /proc/<pid>/smaps_rollup may be unavailable"
        return 0
      fi
    fi
    if awk -v value="$value" -v maximum="$maximum" 'BEGIN { exit (value + 0 <= maximum + 0) ? 0 : 1 }'; then
      pass "process memory expectation matched ${description}: ${value} KiB"
    else
      skip_or_fail "process memory expectation not met: ${description} was ${value} KiB, expected at most ${maximum} KiB"
    fi
  else
    skip_or_fail "process memory expectation not met: missing ${description}"
  fi
}

validate_process_memory_expectations() {
  has_process_memory_expectations || return 0
  local rows
  if ! rows="$(summary_value "samples" "$summary_path")" || [[ "$rows" == "0" ]]; then
    skip_or_fail "cannot validate process memory expectations because no process samples were recorded"
    return 0
  fi

  if [[ -n "$expect_max_rss_kib_at_most" ]]; then
    expect_process_summary_maximum "max_rss_kib" "$expect_max_rss_kib_at_most" "max RSS" 0
  fi
  if [[ -n "$expect_max_pss_kib_at_most" ]]; then
    expect_process_summary_maximum "max_pss_kib" "$expect_max_pss_kib_at_most" "max PSS" 1
  fi
  if [[ -n "$expect_max_private_kib_at_most" ]]; then
    expect_process_summary_maximum "max_private_kib" "$expect_max_private_kib_at_most" "max private memory" 1
  fi
  if [[ -n "$expect_max_uss_kib_at_most" ]]; then
    expect_process_summary_maximum "max_uss_kib" "$expect_max_uss_kib_at_most" "max USS" 1
  fi
  if [[ -n "$expect_max_shared_kib_at_most" ]]; then
    expect_process_summary_maximum "max_shared_kib" "$expect_max_shared_kib_at_most" "max shared memory" 1
  fi
  if [[ -n "$expect_retained_rss_delta_kib_at_most" ]]; then
    expect_process_summary_delta_maximum "retained_rss_delta_kib" "$expect_retained_rss_delta_kib_at_most" "retained RSS delta" ""
  fi
  if [[ -n "$expect_retained_pss_delta_kib_at_most" ]]; then
    expect_process_summary_delta_maximum "retained_pss_delta_kib" "$expect_retained_pss_delta_kib_at_most" "retained PSS delta" "first_pss_kib"
  fi
  if [[ -n "$expect_retained_private_delta_kib_at_most" ]]; then
    expect_process_summary_delta_maximum "retained_private_delta_kib" "$expect_retained_private_delta_kib_at_most" "retained private memory delta" "first_private_kib"
  fi
  if [[ -n "$expect_retained_uss_delta_kib_at_most" ]]; then
    expect_process_summary_delta_maximum "retained_uss_delta_kib" "$expect_retained_uss_delta_kib_at_most" "retained USS delta" "first_uss_kib"
  fi
  if [[ -n "$expect_retained_shared_delta_kib_at_most" ]]; then
    expect_process_summary_delta_maximum "retained_shared_delta_kib" "$expect_retained_shared_delta_kib_at_most" "retained shared memory delta" "first_shared_kib"
  fi
  if [[ -n "$expect_peak_over_first_rss_kib_at_most" ]]; then
    expect_process_summary_delta_maximum "peak_over_first_rss_kib" "$expect_peak_over_first_rss_kib_at_most" "peak-over-first RSS" ""
  fi
  if [[ -n "$expect_peak_over_first_pss_kib_at_most" ]]; then
    expect_process_summary_delta_maximum "peak_over_first_pss_kib" "$expect_peak_over_first_pss_kib_at_most" "peak-over-first PSS" "first_pss_kib"
  fi
  if [[ -n "$expect_peak_over_first_private_kib_at_most" ]]; then
    expect_process_summary_delta_maximum "peak_over_first_private_kib" "$expect_peak_over_first_private_kib_at_most" "peak-over-first private memory" "first_private_kib"
  fi
  if [[ -n "$expect_peak_over_first_uss_kib_at_most" ]]; then
    expect_process_summary_delta_maximum "peak_over_first_uss_kib" "$expect_peak_over_first_uss_kib_at_most" "peak-over-first USS" "first_uss_kib"
  fi
  if [[ -n "$expect_peak_over_first_shared_kib_at_most" ]]; then
    expect_process_summary_delta_maximum "peak_over_first_shared_kib" "$expect_peak_over_first_shared_kib_at_most" "peak-over-first shared memory" "first_shared_kib"
  fi
}

has_telemetry_expectations() {
  [[ "$expect_render_sync_cache_hit" -eq 1 ||
    "$expect_desktop_refresh_skip" -eq 1 ||
    "$expect_render_sync_update_queued" -eq 1 ||
    "$expect_render_sync_update_skipped" -eq 1 ||
    -n "$expect_render_sync_package_cache_entries_latest_at_most" ||
    -n "$expect_render_sync_package_cache_retained_resource_references_latest_at_most" ||
    -n "$expect_render_sync_package_cache_retained_unique_resources_latest_at_most" ||
    -n "$expect_render_sync_package_cache_retained_resource_bytes_latest_at_most" ||
    -n "$expect_render_sync_package_cache_retained_unique_resource_bytes_latest_at_most" ||
    -n "$expect_render_sync_planned_image_resource_references_latest_at_most" ||
    -n "$expect_render_sync_planned_unique_image_resources_latest_at_most" ||
    -n "$expect_render_sync_planned_image_resource_reference_bytes_latest_at_most" ||
    -n "$expect_render_sync_planned_unique_image_resource_bytes_latest_at_most" ||
    -n "$expect_renderer_output_windows_latest_at_least" ||
    -n "$expect_renderer_output_windows_latest_at_most" ||
    -n "$expect_renderer_output_windows_max_at_most" ||
    -n "$expect_renderer_static_surfaces_latest_at_least" ||
    -n "$expect_renderer_static_surfaces_latest_at_most" ||
    -n "$expect_renderer_static_surfaces_max_at_most" ||
    -n "$expect_renderer_slideshow_surfaces_latest_at_least" ||
    -n "$expect_renderer_slideshow_surfaces_latest_at_most" ||
    -n "$expect_renderer_slideshow_surfaces_max_at_most" ||
    -n "$expect_renderer_video_surfaces_latest_at_least" ||
    -n "$expect_renderer_video_surfaces_latest_at_most" ||
    -n "$expect_renderer_video_surfaces_max_at_most" ||
    -n "$expect_renderer_video_pipelines_latest_at_least" ||
    -n "$expect_renderer_video_pipelines_latest_at_most" ||
    -n "$expect_renderer_video_pipelines_max_at_most" ||
    -n "$expect_adaptive_action" ]]
}

expect_telemetry_minimum() {
  local key="$1"
  local minimum="$2"
  local description="$3"
  local value
  if value="$(summary_value "$key" "$telemetry_summary_path")" && [[ "$value" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
    if awk -v value="$value" -v minimum="$minimum" 'BEGIN { exit (value + 0 >= minimum + 0) ? 0 : 1 }'; then
      pass "telemetry expectation matched ${description}: ${value}"
    else
      skip_or_fail "telemetry expectation not met: ${description} was ${value}, expected at least ${minimum}"
    fi
  else
    skip_or_fail "telemetry expectation not met: missing ${description}"
  fi
}

expect_telemetry_maximum() {
  local key="$1"
  local maximum="$2"
  local description="$3"
  local value
  if value="$(summary_value "$key" "$telemetry_summary_path")" && [[ "$value" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
    if awk -v value="$value" -v maximum="$maximum" 'BEGIN { exit (value + 0 <= maximum + 0) ? 0 : 1 }'; then
      pass "telemetry expectation matched ${description}: ${value}"
    else
      skip_or_fail "telemetry expectation not met: ${description} was ${value}, expected at most ${maximum}"
    fi
  else
    skip_or_fail "telemetry expectation not met: missing ${description}"
  fi
}

validate_telemetry_expectations() {
  has_telemetry_expectations || return 0
  if [[ "$status_enabled" -ne 1 || "$telemetry_failures" -gt 0 ]]; then
    skip_or_fail "cannot validate telemetry expectations without complete telemetry samples"
    return 0
  fi

  local rows
  if ! rows="$(summary_value "telemetry_rows" "$telemetry_summary_path")" || [[ "$rows" == "0" ]]; then
    skip_or_fail "cannot validate telemetry expectations because no telemetry rows were sampled"
    return 0
  fi

  if [[ "$expect_render_sync_cache_hit" -eq 1 ]]; then
    expect_telemetry_minimum "render_sync_cache_hits_delta" 1 "render sync cache hit delta"
  fi
  if [[ "$expect_desktop_refresh_skip" -eq 1 ]]; then
    expect_telemetry_minimum "desktop_refresh_skips_delta" 1 "desktop refresh skip delta"
  fi
  if [[ "$expect_render_sync_update_queued" -eq 1 ]]; then
    expect_telemetry_minimum "render_sync_updates_queued_latest" 1 "renderer sync queued latest count"
  fi
  if [[ "$expect_render_sync_update_skipped" -eq 1 ]]; then
    expect_telemetry_minimum "render_sync_updates_skipped_latest" 1 "renderer sync skipped latest count"
  fi
  if [[ -n "$expect_render_sync_package_cache_entries_latest_at_most" ]]; then
    expect_telemetry_maximum "render_sync_package_cache_entries_latest" "$expect_render_sync_package_cache_entries_latest_at_most" "latest render sync package cache entries"
  fi
  if [[ -n "$expect_render_sync_package_cache_retained_resource_references_latest_at_most" ]]; then
    expect_telemetry_maximum "render_sync_package_cache_retained_resource_references_latest" "$expect_render_sync_package_cache_retained_resource_references_latest_at_most" "latest retained package-cache resource references"
  fi
  if [[ -n "$expect_render_sync_package_cache_retained_unique_resources_latest_at_most" ]]; then
    expect_telemetry_maximum "render_sync_package_cache_retained_unique_resources_latest" "$expect_render_sync_package_cache_retained_unique_resources_latest_at_most" "latest retained package-cache unique resources"
  fi
  if [[ -n "$expect_render_sync_package_cache_retained_resource_bytes_latest_at_most" ]]; then
    expect_telemetry_maximum "render_sync_package_cache_retained_resource_bytes_latest" "$expect_render_sync_package_cache_retained_resource_bytes_latest_at_most" "latest retained package-cache resource bytes"
  fi
  if [[ -n "$expect_render_sync_package_cache_retained_unique_resource_bytes_latest_at_most" ]]; then
    expect_telemetry_maximum "render_sync_package_cache_retained_unique_resource_bytes_latest" "$expect_render_sync_package_cache_retained_unique_resource_bytes_latest_at_most" "latest retained package-cache unique resource bytes"
  fi
  if [[ -n "$expect_render_sync_planned_image_resource_references_latest_at_most" ]]; then
    expect_telemetry_maximum "render_sync_planned_image_resource_references_latest" "$expect_render_sync_planned_image_resource_references_latest_at_most" "latest planned image resource references"
  fi
  if [[ -n "$expect_render_sync_planned_unique_image_resources_latest_at_most" ]]; then
    expect_telemetry_maximum "render_sync_planned_unique_image_resources_latest" "$expect_render_sync_planned_unique_image_resources_latest_at_most" "latest planned unique image resources"
  fi
  if [[ -n "$expect_render_sync_planned_image_resource_reference_bytes_latest_at_most" ]]; then
    expect_telemetry_maximum "render_sync_planned_image_resource_reference_bytes_latest" "$expect_render_sync_planned_image_resource_reference_bytes_latest_at_most" "latest planned image resource reference bytes"
  fi
  if [[ -n "$expect_render_sync_planned_unique_image_resource_bytes_latest_at_most" ]]; then
    expect_telemetry_maximum "render_sync_planned_unique_image_resource_bytes_latest" "$expect_render_sync_planned_unique_image_resource_bytes_latest_at_most" "latest planned unique image resource bytes"
  fi
  if [[ -n "$expect_renderer_output_windows_latest_at_least" ]]; then
    expect_telemetry_minimum "renderer_output_windows_latest" "$expect_renderer_output_windows_latest_at_least" "latest renderer output window count"
  fi
  if [[ -n "$expect_renderer_output_windows_latest_at_most" ]]; then
    expect_telemetry_maximum "renderer_output_windows_latest" "$expect_renderer_output_windows_latest_at_most" "latest renderer output window count"
  fi
  if [[ -n "$expect_renderer_output_windows_max_at_most" ]]; then
    expect_telemetry_maximum "renderer_output_windows_max" "$expect_renderer_output_windows_max_at_most" "max renderer output window count"
  fi
  if [[ -n "$expect_renderer_static_surfaces_latest_at_least" ]]; then
    expect_telemetry_minimum "renderer_static_surfaces_latest" "$expect_renderer_static_surfaces_latest_at_least" "latest renderer static surface count"
  fi
  if [[ -n "$expect_renderer_static_surfaces_latest_at_most" ]]; then
    expect_telemetry_maximum "renderer_static_surfaces_latest" "$expect_renderer_static_surfaces_latest_at_most" "latest renderer static surface count"
  fi
  if [[ -n "$expect_renderer_static_surfaces_max_at_most" ]]; then
    expect_telemetry_maximum "renderer_static_surfaces_max" "$expect_renderer_static_surfaces_max_at_most" "max renderer static surface count"
  fi
  if [[ -n "$expect_renderer_slideshow_surfaces_latest_at_least" ]]; then
    expect_telemetry_minimum "renderer_slideshow_surfaces_latest" "$expect_renderer_slideshow_surfaces_latest_at_least" "latest renderer slideshow surface count"
  fi
  if [[ -n "$expect_renderer_slideshow_surfaces_latest_at_most" ]]; then
    expect_telemetry_maximum "renderer_slideshow_surfaces_latest" "$expect_renderer_slideshow_surfaces_latest_at_most" "latest renderer slideshow surface count"
  fi
  if [[ -n "$expect_renderer_slideshow_surfaces_max_at_most" ]]; then
    expect_telemetry_maximum "renderer_slideshow_surfaces_max" "$expect_renderer_slideshow_surfaces_max_at_most" "max renderer slideshow surface count"
  fi
  if [[ -n "$expect_renderer_video_surfaces_latest_at_least" ]]; then
    expect_telemetry_minimum "renderer_video_surfaces_latest" "$expect_renderer_video_surfaces_latest_at_least" "latest renderer video surface count"
  fi
  if [[ -n "$expect_renderer_video_surfaces_latest_at_most" ]]; then
    expect_telemetry_maximum "renderer_video_surfaces_latest" "$expect_renderer_video_surfaces_latest_at_most" "latest renderer video surface count"
  fi
  if [[ -n "$expect_renderer_video_surfaces_max_at_most" ]]; then
    expect_telemetry_maximum "renderer_video_surfaces_max" "$expect_renderer_video_surfaces_max_at_most" "max renderer video surface count"
  fi
  if [[ -n "$expect_renderer_video_pipelines_latest_at_least" ]]; then
    expect_telemetry_minimum "renderer_video_pipelines_latest" "$expect_renderer_video_pipelines_latest_at_least" "latest renderer video pipeline count"
  fi
  if [[ -n "$expect_renderer_video_pipelines_latest_at_most" ]]; then
    expect_telemetry_maximum "renderer_video_pipelines_latest" "$expect_renderer_video_pipelines_latest_at_most" "latest renderer video pipeline count"
  fi
  if [[ -n "$expect_renderer_video_pipelines_max_at_most" ]]; then
    expect_telemetry_maximum "renderer_video_pipelines_max" "$expect_renderer_video_pipelines_max_at_most" "max renderer video pipeline count"
  fi
  if [[ -n "$expect_adaptive_action" ]]; then
    expect_telemetry_minimum "adaptive_action.${expect_adaptive_action}" 1 "adaptive action ${expect_adaptive_action}"
  fi
}

has_video_runtime_expectations() {
  [[ -n "$expect_decoder_policy_status" ||
    -n "$expect_decoder_class" ||
    -n "$expect_memory_feature" ||
    -n "$expect_sink_memory_feature" ||
    -n "$expect_zero_copy_evidence" ||
    "$expect_video_position_progress" -eq 1 ||
    "$expect_frame_limiter_enabled" -eq 1 ||
    -n "$expect_frame_limiter_max_fps" ||
    "$expect_video_qos" -eq 1 ||
    -n "$expect_qos_dropped_max_at_most" ||
    "$expect_gtk_frame_clock" -eq 1 ||
    "${#expect_gtk_frame_clock_phases[@]}" -gt 0 ||
    "$expect_gtk_frame_timings" -eq 1 ]]
}

expect_video_runtime_field() {
  local column="$1"
  local expected="$2"
  local description="$3"

  if awk -F, -v column="$column" -v expected="$expected" '
    NR == 1 { next }
    {
      count = split($column, values, /\|/)
      for (i = 1; i <= count; i += 1) {
        if (values[i] == expected) {
          found = 1
          exit
        }
      }
    }
    END { exit found ? 0 : 1 }
  ' "$video_runtime_path"; then
    pass "video runtime expectation matched ${description}: ${expected}"
  else
    skip_or_fail "video runtime expectation not met: ${description} ${expected}"
  fi
}

expect_video_runtime_summary_minimum() {
  local key="$1"
  local minimum="$2"
  local description="$3"
  local value
  if value="$(summary_value "$key" "$video_runtime_summary_path")" && [[ "$value" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
    if awk -v value="$value" -v minimum="$minimum" 'BEGIN { exit (value + 0 >= minimum + 0) ? 0 : 1 }'; then
      pass "video runtime expectation matched ${description}: ${value}"
    else
      skip_or_fail "video runtime expectation not met: ${description} was ${value}, expected at least ${minimum}"
    fi
  else
    skip_or_fail "video runtime expectation not met: missing ${description}"
  fi
}

expect_video_runtime_summary_maximum() {
  local key="$1"
  local maximum="$2"
  local description="$3"
  local value
  if value="$(summary_value "$key" "$video_runtime_summary_path")" && [[ "$value" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
    if awk -v value="$value" -v maximum="$maximum" 'BEGIN { exit (value + 0 <= maximum + 0) ? 0 : 1 }'; then
      pass "video runtime expectation matched ${description}: ${value}"
    else
      skip_or_fail "video runtime expectation not met: ${description} was ${value}, expected at most ${maximum}"
    fi
  else
    skip_or_fail "video runtime expectation not met: missing ${description}"
  fi
}

gtk_frame_clock_phase_summary_key() {
  case "$1" in
    before-paint)
      printf '%s\n' "video_gtk_frame_clock_before_paint_ticks_max"
      ;;
    update)
      printf '%s\n' "video_gtk_frame_clock_update_ticks_max"
      ;;
    layout)
      printf '%s\n' "video_gtk_frame_clock_layout_ticks_max"
      ;;
    paint)
      printf '%s\n' "video_gtk_frame_clock_paint_ticks_max"
      ;;
    after-paint)
      printf '%s\n' "video_gtk_frame_clock_after_paint_ticks_max"
      ;;
  esac
}

validate_video_runtime_expectations() {
  has_video_runtime_expectations || return 0
  if [[ "$status_enabled" -ne 1 || "$video_runtime_failures" -gt 0 ]]; then
    skip_or_fail "cannot validate video runtime expectations without complete video runtime samples"
    return 0
  fi

  local rows
  rows="$(awk 'NR > 1 { rows += 1 } END { print rows + 0 }' "$video_runtime_path")"
  if [[ "$rows" == "0" ]]; then
    skip_or_fail "cannot validate video runtime expectations because no video runtime rows were sampled"
    return 0
  fi

  if [[ -n "$expect_decoder_policy_status" ]]; then
    expect_video_runtime_field 7 "$expect_decoder_policy_status" "decoder policy status"
  fi
  if [[ -n "$expect_decoder_class" ]]; then
    expect_video_runtime_field 9 "$expect_decoder_class" "decoder class"
  fi
  if [[ -n "$expect_memory_feature" ]]; then
    expect_video_runtime_field 11 "$expect_memory_feature" "caps memory feature"
  fi
  if [[ -n "$expect_sink_memory_feature" ]]; then
    expect_video_runtime_field 12 "$expect_sink_memory_feature" "sink caps memory feature"
  fi
  if [[ -n "$expect_zero_copy_evidence" ]]; then
    expect_video_runtime_field 13 "$expect_zero_copy_evidence" "zero-copy evidence level"
  fi
  if [[ "$expect_video_position_progress" -eq 1 ]]; then
    expect_video_runtime_summary_minimum "video_position_moving_outputs" 1 "moving video output count"
  fi
  if [[ "$expect_frame_limiter_enabled" -eq 1 ]]; then
    expect_video_runtime_field 19 "true" "frame limiter enabled"
  fi
  if [[ -n "$expect_frame_limiter_max_fps" ]]; then
    expect_video_runtime_field 20 "$expect_frame_limiter_max_fps" "frame limiter max_fps"
  fi
  if [[ "$expect_video_qos" -eq 1 ]]; then
    expect_video_runtime_summary_minimum "video_qos_messages_max" 1 "QoS message max count"
  fi
  if [[ -n "$expect_qos_dropped_max_at_most" ]]; then
    expect_video_runtime_summary_maximum "video_qos_dropped_max" "$expect_qos_dropped_max_at_most" "QoS dropped max count"
  fi
  if [[ "$expect_gtk_frame_clock" -eq 1 ]]; then
    expect_video_runtime_summary_minimum "video_gtk_frame_clock_ticks_max" 1 "GTK frame clock tick max count"
  fi
  local phase
  local phase_key
  for phase in "${expect_gtk_frame_clock_phases[@]}"; do
    phase_key="$(gtk_frame_clock_phase_summary_key "$phase")"
    expect_video_runtime_summary_minimum "$phase_key" 1 "GTK frame clock ${phase} phase tick max count"
  done
  if [[ "$expect_gtk_frame_timings" -eq 1 ]]; then
    expect_video_runtime_summary_minimum "video_gtk_frame_timings_complete_max" 1 "GDK frame timings complete max count"
  fi
}

if ! is_positive_integer "$duration"; then
  echo "--duration must be a positive integer" >&2
  exit 2
fi
if ! is_positive_integer "$interval"; then
  echo "--interval must be a positive integer" >&2
  exit 2
fi
if [[ -n "$expect_max_fps" && ! "$expect_max_fps" =~ ^[0-9]+$ ]]; then
  echo "--expect-max-fps must be a non-negative integer" >&2
  exit 2
fi
if [[ -n "$expect_frame_limiter_max_fps" && ! "$expect_frame_limiter_max_fps" =~ ^[0-9]+$ ]]; then
  echo "--expect-frame-limiter-max-fps must be a non-negative integer" >&2
  exit 2
fi
if [[ -n "$expect_qos_dropped_max_at_most" && ! "$expect_qos_dropped_max_at_most" =~ ^[0-9]+$ ]]; then
  echo "--expect-qos-dropped-max-at-most must be a non-negative integer" >&2
  exit 2
fi
for memory_expectation in \
  "$expect_max_rss_kib_at_most" \
  "$expect_max_pss_kib_at_most" \
  "$expect_max_private_kib_at_most" \
  "$expect_max_uss_kib_at_most" \
  "$expect_max_shared_kib_at_most"
do
  if [[ -n "$memory_expectation" && ! "$memory_expectation" =~ ^[1-9][0-9]*$ ]]; then
    echo "memory KiB expectations must be positive integers" >&2
    exit 2
  fi
done
for memory_delta_expectation in \
  "$expect_retained_rss_delta_kib_at_most" \
  "$expect_retained_pss_delta_kib_at_most" \
  "$expect_retained_private_delta_kib_at_most" \
  "$expect_retained_uss_delta_kib_at_most" \
  "$expect_retained_shared_delta_kib_at_most" \
  "$expect_peak_over_first_rss_kib_at_most" \
  "$expect_peak_over_first_pss_kib_at_most" \
  "$expect_peak_over_first_private_kib_at_most" \
  "$expect_peak_over_first_uss_kib_at_most" \
  "$expect_peak_over_first_shared_kib_at_most"
do
  if [[ -n "$memory_delta_expectation" && ! "$memory_delta_expectation" =~ ^[0-9]+$ ]]; then
    echo "memory delta KiB expectations must be non-negative integers" >&2
    exit 2
  fi
done
case "$expect_decoder_policy_status" in
  ""|not-applicable|not-observed|satisfied|software-fallback|violated|unknown-decoder)
    ;;
  *)
    echo "--expect-decoder-policy-status must be one of not-applicable, not-observed, satisfied, software-fallback, violated, unknown-decoder" >&2
    exit 2
    ;;
esac
case "$expect_decoder_class" in
  ""|hardware|software|unknown)
    ;;
  *)
    echo "--expect-decoder-class must be one of hardware, software, unknown" >&2
    exit 2
    ;;
esac
case "$expect_adaptive_action" in
  ""|throttle|pause-unfocused|pause-dynamic)
    ;;
  *)
    echo "--expect-adaptive-action must be one of throttle, pause-unfocused, pause-dynamic" >&2
    exit 2
    ;;
esac
for renderer_resource_expectation in \
  "$expect_renderer_output_windows_latest_at_least" \
  "$expect_renderer_output_windows_latest_at_most" \
  "$expect_renderer_output_windows_max_at_most" \
  "$expect_renderer_static_surfaces_latest_at_least" \
  "$expect_renderer_static_surfaces_latest_at_most" \
  "$expect_renderer_static_surfaces_max_at_most" \
  "$expect_renderer_slideshow_surfaces_latest_at_least" \
  "$expect_renderer_slideshow_surfaces_latest_at_most" \
  "$expect_renderer_slideshow_surfaces_max_at_most" \
  "$expect_renderer_video_surfaces_latest_at_least" \
  "$expect_renderer_video_surfaces_latest_at_most" \
  "$expect_renderer_video_surfaces_max_at_most" \
  "$expect_renderer_video_pipelines_latest_at_least" \
  "$expect_renderer_video_pipelines_latest_at_most" \
  "$expect_renderer_video_pipelines_max_at_most"
do
  if [[ -n "$renderer_resource_expectation" && ! "$renderer_resource_expectation" =~ ^[0-9]+$ ]]; then
    echo "renderer resource expectations must be non-negative integers" >&2
    exit 2
  fi
done
for render_sync_resource_expectation in \
  "$expect_render_sync_package_cache_entries_latest_at_most" \
  "$expect_render_sync_package_cache_retained_resource_references_latest_at_most" \
  "$expect_render_sync_package_cache_retained_unique_resources_latest_at_most" \
  "$expect_render_sync_package_cache_retained_resource_bytes_latest_at_most" \
  "$expect_render_sync_package_cache_retained_unique_resource_bytes_latest_at_most" \
  "$expect_render_sync_planned_image_resource_references_latest_at_most" \
  "$expect_render_sync_planned_unique_image_resources_latest_at_most" \
  "$expect_render_sync_planned_image_resource_reference_bytes_latest_at_most" \
  "$expect_render_sync_planned_unique_image_resource_bytes_latest_at_most"
do
  if [[ -n "$render_sync_resource_expectation" && ! "$render_sync_resource_expectation" =~ ^[0-9]+$ ]]; then
    echo "render sync resource expectations must be non-negative integers" >&2
    exit 2
  fi
done

essential_missing=0
require_command ps || essential_missing=1
require_command sed || essential_missing=1
require_command awk || essential_missing=1
if [[ -z "$pid" ]]; then
  pid="$(find_gilderd_pid || true)"
fi
if [[ -z "$pid" ]]; then
  skip_or_fail "no running gilderd process found; pass --pid <pid>"
fi
if [[ -n "$pid" ]] && ! kill -0 "$pid" >/dev/null 2>&1; then
  skip_or_fail "process $pid is not running"
fi
status_enabled=1
resolve_gilderctl || status_enabled=0

if [[ "$failures" -gt 0 ]]; then
  note "summary: ${passes} passed, ${skips} skipped, ${failures} failed"
  exit 1
fi
if [[ "$essential_missing" -eq 1 || -z "$pid" ]]; then
  note "summary: ${passes} passed, ${skips} skipped, ${failures} failed"
  exit 0
fi

if [[ -n "$output_dir" ]]; then
  work_dir="$output_dir"
  mkdir -p "$work_dir"
else
  mkdir -p "$work_parent"
  work_dir="$(mktemp -d "${work_parent%/}/gilder-performance.XXXXXX")"
fi
if [[ "$keep" -eq 0 ]]; then
  trap 'rm -rf "$work_dir"' EXIT
fi

samples=$(( (duration + interval - 1) / interval ))
[[ "$samples" -ge 1 ]] || samples=1
csv_path="$work_dir/samples.csv"
metadata_path="$work_dir/metadata.txt"
summary_path="$work_dir/summary.txt"
decisions_path="$work_dir/decisions.csv"
decision_summary_path="$work_dir/decision-summary.txt"
telemetry_path="$work_dir/telemetry.csv"
telemetry_summary_path="$work_dir/telemetry-summary.txt"
video_runtime_path="$work_dir/video-runtime.csv"
video_runtime_summary_path="$work_dir/video-runtime-summary.txt"

cat > "$metadata_path" <<EOF
label: ${label}
pid: ${pid}
socket: ${socket:-default}
gilderctl: ${gilderctl:-unavailable}
duration_seconds: ${duration}
interval_seconds: ${interval}
samples: ${samples}
expect_mode: ${expect_mode:-none}
expect_reason: ${expect_reason:-none}
expect_action: ${expect_action:-none}
expect_max_fps: ${expect_max_fps:-none}
expect_plan_kind: ${expect_plan_kind:-none}
expect_max_rss_kib_at_most: ${expect_max_rss_kib_at_most:-none}
expect_max_pss_kib_at_most: ${expect_max_pss_kib_at_most:-none}
expect_max_private_kib_at_most: ${expect_max_private_kib_at_most:-none}
expect_max_uss_kib_at_most: ${expect_max_uss_kib_at_most:-none}
expect_max_shared_kib_at_most: ${expect_max_shared_kib_at_most:-none}
expect_retained_rss_delta_kib_at_most: ${expect_retained_rss_delta_kib_at_most:-none}
expect_retained_pss_delta_kib_at_most: ${expect_retained_pss_delta_kib_at_most:-none}
expect_retained_private_delta_kib_at_most: ${expect_retained_private_delta_kib_at_most:-none}
expect_retained_uss_delta_kib_at_most: ${expect_retained_uss_delta_kib_at_most:-none}
expect_retained_shared_delta_kib_at_most: ${expect_retained_shared_delta_kib_at_most:-none}
expect_peak_over_first_rss_kib_at_most: ${expect_peak_over_first_rss_kib_at_most:-none}
expect_peak_over_first_pss_kib_at_most: ${expect_peak_over_first_pss_kib_at_most:-none}
expect_peak_over_first_private_kib_at_most: ${expect_peak_over_first_private_kib_at_most:-none}
expect_peak_over_first_uss_kib_at_most: ${expect_peak_over_first_uss_kib_at_most:-none}
expect_peak_over_first_shared_kib_at_most: ${expect_peak_over_first_shared_kib_at_most:-none}
expect_render_sync_cache_hit: ${expect_render_sync_cache_hit}
expect_desktop_refresh_skip: ${expect_desktop_refresh_skip}
expect_render_sync_update_queued: ${expect_render_sync_update_queued}
expect_render_sync_update_skipped: ${expect_render_sync_update_skipped}
expect_render_sync_package_cache_entries_latest_at_most: ${expect_render_sync_package_cache_entries_latest_at_most:-none}
expect_render_sync_package_cache_retained_resource_references_latest_at_most: ${expect_render_sync_package_cache_retained_resource_references_latest_at_most:-none}
expect_render_sync_package_cache_retained_unique_resources_latest_at_most: ${expect_render_sync_package_cache_retained_unique_resources_latest_at_most:-none}
expect_render_sync_package_cache_retained_resource_bytes_latest_at_most: ${expect_render_sync_package_cache_retained_resource_bytes_latest_at_most:-none}
expect_render_sync_package_cache_retained_unique_resource_bytes_latest_at_most: ${expect_render_sync_package_cache_retained_unique_resource_bytes_latest_at_most:-none}
expect_render_sync_planned_image_resource_references_latest_at_most: ${expect_render_sync_planned_image_resource_references_latest_at_most:-none}
expect_render_sync_planned_unique_image_resources_latest_at_most: ${expect_render_sync_planned_unique_image_resources_latest_at_most:-none}
expect_render_sync_planned_image_resource_reference_bytes_latest_at_most: ${expect_render_sync_planned_image_resource_reference_bytes_latest_at_most:-none}
expect_render_sync_planned_unique_image_resource_bytes_latest_at_most: ${expect_render_sync_planned_unique_image_resource_bytes_latest_at_most:-none}
expect_renderer_output_windows_latest_at_least: ${expect_renderer_output_windows_latest_at_least:-none}
expect_renderer_output_windows_latest_at_most: ${expect_renderer_output_windows_latest_at_most:-none}
expect_renderer_output_windows_max_at_most: ${expect_renderer_output_windows_max_at_most:-none}
expect_renderer_static_surfaces_latest_at_least: ${expect_renderer_static_surfaces_latest_at_least:-none}
expect_renderer_static_surfaces_latest_at_most: ${expect_renderer_static_surfaces_latest_at_most:-none}
expect_renderer_static_surfaces_max_at_most: ${expect_renderer_static_surfaces_max_at_most:-none}
expect_renderer_slideshow_surfaces_latest_at_least: ${expect_renderer_slideshow_surfaces_latest_at_least:-none}
expect_renderer_slideshow_surfaces_latest_at_most: ${expect_renderer_slideshow_surfaces_latest_at_most:-none}
expect_renderer_slideshow_surfaces_max_at_most: ${expect_renderer_slideshow_surfaces_max_at_most:-none}
expect_renderer_video_surfaces_latest_at_least: ${expect_renderer_video_surfaces_latest_at_least:-none}
expect_renderer_video_surfaces_latest_at_most: ${expect_renderer_video_surfaces_latest_at_most:-none}
expect_renderer_video_surfaces_max_at_most: ${expect_renderer_video_surfaces_max_at_most:-none}
expect_renderer_video_pipelines_latest_at_least: ${expect_renderer_video_pipelines_latest_at_least:-none}
expect_renderer_video_pipelines_latest_at_most: ${expect_renderer_video_pipelines_latest_at_most:-none}
expect_renderer_video_pipelines_max_at_most: ${expect_renderer_video_pipelines_max_at_most:-none}
expect_adaptive_action: ${expect_adaptive_action:-none}
expect_decoder_policy_status: ${expect_decoder_policy_status:-none}
expect_decoder_class: ${expect_decoder_class:-none}
expect_memory_feature: ${expect_memory_feature:-none}
expect_sink_memory_feature: ${expect_sink_memory_feature:-none}
expect_zero_copy_evidence: ${expect_zero_copy_evidence:-none}
expect_video_position_progress: ${expect_video_position_progress}
expect_frame_limiter_enabled: ${expect_frame_limiter_enabled}
expect_frame_limiter_max_fps: ${expect_frame_limiter_max_fps:-none}
expect_video_qos: ${expect_video_qos}
expect_qos_dropped_max_at_most: ${expect_qos_dropped_max_at_most:-none}
expect_gtk_frame_clock: ${expect_gtk_frame_clock}
expect_gtk_frame_clock_phases: ${expect_gtk_frame_clock_phases[*]:-none}
expect_gtk_frame_timings: ${expect_gtk_frame_timings}
gpu_busy_sources: drm gpu_busy_percent sysfs when readable; nvidia-smi utilization.gpu when available
EOF

printf 'sample,elapsed_seconds,pid,cpu_percent,rss_kib,vsz_kib,pss_kib,private_clean_kib,private_dirty_kib,private_kib,uss_kib,shared_clean_kib,shared_dirty_kib,shared_kib,stat,comm,status_file,status_error_file,gpu_busy_percent_avg,gpu_busy_percent_max,gpu_busy_sources\n' > "$csv_path"
printf 'sample,elapsed_seconds,output_name,action,mode,reason,max_fps,wallpaper,plan_kind,source,fit,target_max_fps,muted\n' > "$decisions_path"
printf 'sample,elapsed_seconds,desktop_refreshes,desktop_refresh_skips,desktop_changes,last_desktop_refresh_age_ms,render_sync_cache_hits,render_sync_cache_misses,render_sync_updates_queued,render_sync_updates_skipped,render_sync_package_cache_entries,render_sync_package_cache_max_entries,render_sync_package_cache_hits,render_sync_package_cache_misses,render_sync_package_cache_evictions,render_sync_archive_cache_entries,render_sync_archive_cache_max_entries,render_sync_archive_cache_reuses,render_sync_archive_cache_extractions,render_sync_archive_cache_evictions,render_sync_archive_cache_evictions_latest,render_sync_archive_cache_eviction_errors,render_sync_archive_cache_eviction_errors_latest,render_sync_planned_static_image_resources,render_sync_planned_video_poster_resources,render_sync_planned_slideshow_image_resources,render_sync_planned_image_resource_references,render_sync_planned_unique_image_resources,adaptive_refreshes,adaptive_refresh_skips,adaptive_active_triggers,cpu_pressure_some_avg10_x100,memory_pressure_some_avg10_x100,temperature_max_millicelsius,power_external_online,power_system_battery_present,power_battery_discharging,power_battery_capacity_percent,power_battery_power_microwatts,gpu_busy_percent_avg,gpu_busy_percent_max,gpu_busy_sources,adaptive_action_types,adaptive_action_scopes,adaptive_action_configured_actions,adaptive_action_max_fps,renderer_output_windows,renderer_static_surfaces,renderer_slideshow_surfaces,renderer_video_surfaces,renderer_video_pipelines,renderer_video_qos_messages,renderer_video_qos_dropped_max,renderer_video_gtk_frame_clock_ticks,renderer_video_gtk_frame_clock_interval_us_max,renderer_video_gtk_frame_clock_fps_x1000_max,renderer_video_gtk_frame_timings_complete,renderer_video_gtk_frame_timings_presentation_interval_us_max,renderer_video_gtk_frame_timings_presentation_time_us_max,renderer_video_gtk_frame_clock_before_paint_ticks,renderer_video_gtk_frame_clock_update_ticks,renderer_video_gtk_frame_clock_layout_ticks,renderer_video_gtk_frame_clock_paint_ticks,renderer_video_gtk_frame_clock_after_paint_ticks,render_sync_planned_static_image_resource_bytes,render_sync_planned_video_poster_resource_bytes,render_sync_planned_slideshow_image_resource_bytes,render_sync_planned_image_resource_reference_bytes,render_sync_planned_unique_image_resource_bytes,render_sync_package_cache_retained_resource_references,render_sync_package_cache_retained_unique_resources,render_sync_package_cache_retained_resource_bytes,render_sync_package_cache_retained_unique_resource_bytes\n' > "$telemetry_path"
printf 'sample,elapsed_seconds,output_name,mode,gst_state,decoder_policy,decoder_policy_status,actual_decoders,decoder_classes,caps_report_count,memory_features,sink_memory_features,zero_copy_evidence_level,zero_copy_evidence_notes,media_types,caps_paths,position_ms,duration_ms,frame_limiter_enabled,frame_limiter_max_fps,qos_messages,qos_processed_max,qos_dropped_max,qos_stats_format,qos_jitter_ns_latest,qos_jitter_ns_abs_max,qos_proportion_x1000_latest,gtk_frame_clock_ticks,gtk_frame_clock_counter_latest,gtk_frame_clock_time_us_latest,gtk_frame_clock_interval_us_latest,gtk_frame_clock_interval_us_max,gtk_frame_clock_fps_x1000_latest,gtk_frame_clock_refresh_interval_us_latest,gtk_frame_clock_predicted_presentation_time_us_latest,gtk_frame_timings_observed,gtk_frame_timings_complete,gtk_frame_timings_counter_latest,gtk_frame_timings_complete_counter_latest,gtk_frame_timings_frame_time_us_latest,gtk_frame_timings_predicted_presentation_time_us_latest,gtk_frame_timings_presentation_time_us_latest,gtk_frame_timings_presentation_interval_us_latest,gtk_frame_timings_presentation_interval_us_max,gtk_frame_timings_refresh_interval_us_latest,source,gtk_frame_clock_before_paint_ticks,gtk_frame_clock_update_ticks,gtk_frame_clock_layout_ticks,gtk_frame_clock_paint_ticks,gtk_frame_clock_after_paint_ticks\n' > "$video_runtime_path"

status_failures=0
decision_failures=0
telemetry_failures=0
video_runtime_failures=0
for sample in $(seq 1 "$samples"); do
  if ! kill -0 "$pid" >/dev/null 2>&1; then
    skip_or_fail "process $pid exited during sampling"
    break
  fi

  elapsed=$(( (sample - 1) * interval ))
  ps_line="$(sample_process "$pid" || true)"
  if [[ -z "$ps_line" ]]; then
    skip_or_fail "failed to sample process $pid"
    break
  fi
  read -r sample_pid cpu_percent rss_kib vsz_kib stat comm <<< "$ps_line"
  read -r pss_kib private_clean_kib private_dirty_kib private_kib uss_kib shared_clean_kib shared_dirty_kib shared_kib < <(sample_smaps_rollup "$pid")
  IFS='|' read -r gpu_busy_percent_avg gpu_busy_percent_max gpu_busy_sources < <(sample_gpu_busy)

  status_file=""
  status_error_file=""
  if [[ "$status_enabled" -eq 1 ]]; then
    status_file="$work_dir/status-$(printf '%03d' "$sample").json"
    status_error_file="$work_dir/status-$(printf '%03d' "$sample").err"
    if [[ -n "$socket" ]]; then
      if ! GILDER_SOCKET="$socket" "$gilderctl" status > "$status_file" 2> "$status_error_file"; then
        status_failures=$((status_failures + 1))
        skip_or_fail "gilderctl status failed for sample $sample"
        rm -f "$status_file"
        status_file=""
      elif [[ ! -s "$status_error_file" ]]; then
        rm -f "$status_error_file"
        status_error_file=""
      fi
    else
      if ! "$gilderctl" status > "$status_file" 2> "$status_error_file"; then
        status_failures=$((status_failures + 1))
        skip_or_fail "gilderctl status failed for sample $sample"
        rm -f "$status_file"
        status_file=""
      elif [[ ! -s "$status_error_file" ]]; then
        rm -f "$status_error_file"
        status_error_file=""
      fi
    fi
    if [[ -n "$status_file" ]]; then
      decision_error_file="$work_dir/decisions-$(printf '%03d' "$sample").err"
      if ! append_status_decisions "$sample" "$elapsed" "$status_file" "$decisions_path" "$decision_error_file"; then
        decision_failures=$((decision_failures + 1))
        skip_or_fail "failed to extract render decisions for sample $sample"
      fi
      telemetry_error_file="$work_dir/telemetry-$(printf '%03d' "$sample").err"
      if ! append_status_telemetry "$sample" "$elapsed" "$status_file" "$telemetry_path" "$telemetry_error_file"; then
        telemetry_failures=$((telemetry_failures + 1))
        skip_or_fail "failed to extract daemon telemetry for sample $sample"
      fi
      video_runtime_error_file="$work_dir/video-runtime-$(printf '%03d' "$sample").err"
      if ! append_status_video_runtime "$sample" "$elapsed" "$status_file" "$video_runtime_path" "$video_runtime_error_file"; then
        video_runtime_failures=$((video_runtime_failures + 1))
        skip_or_fail "failed to extract video runtime for sample $sample"
      fi
    fi
  fi

  if [[ "$failures" -gt 0 ]]; then
    break
  fi

  printf '%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s\n' \
    "$sample" \
    "$elapsed" \
    "$sample_pid" \
    "$cpu_percent" \
    "$rss_kib" \
    "$vsz_kib" \
    "$pss_kib" \
    "$private_clean_kib" \
    "$private_dirty_kib" \
    "$private_kib" \
    "$uss_kib" \
    "$shared_clean_kib" \
    "$shared_dirty_kib" \
    "$shared_kib" \
    "$stat" \
    "$comm" \
    "${status_file#$work_dir/}" \
    "${status_error_file#$work_dir/}" \
    "$gpu_busy_percent_avg" \
    "$gpu_busy_percent_max" \
    "$gpu_busy_sources" >> "$csv_path"

  if [[ "$sample" -lt "$samples" ]]; then
    sleep "$interval"
  fi
done

write_summary "$csv_path" "$summary_path"
write_decision_summary "$decisions_path" "$decision_summary_path"
write_telemetry_summary "$telemetry_path" "$telemetry_summary_path"
write_video_runtime_summary "$video_runtime_path" "$video_runtime_summary_path"
pass "wrote process samples to $csv_path"
pass "wrote summary to $summary_path"
if [[ "$status_enabled" -eq 1 && "$status_failures" -eq 0 ]]; then
  pass "wrote status snapshots under $work_dir"
elif [[ "$status_enabled" -eq 1 ]]; then
  note "status snapshots had ${status_failures} failed samples"
else
  note "status snapshots skipped because gilderctl is unavailable"
fi
if [[ "$status_enabled" -eq 1 && "$decision_failures" -eq 0 ]]; then
  pass "wrote render decision samples to $decisions_path"
  pass "wrote render decision summary to $decision_summary_path"
elif [[ "$status_enabled" -eq 1 ]]; then
  note "render decision extraction had ${decision_failures} failed samples"
fi
if [[ "$status_enabled" -eq 1 && "$telemetry_failures" -eq 0 ]]; then
  pass "wrote daemon telemetry samples to $telemetry_path"
  pass "wrote daemon telemetry summary to $telemetry_summary_path"
elif [[ "$status_enabled" -eq 1 ]]; then
  note "daemon telemetry extraction had ${telemetry_failures} failed samples"
fi
if [[ "$status_enabled" -eq 1 && "$video_runtime_failures" -eq 0 ]]; then
  pass "wrote video runtime samples to $video_runtime_path"
  pass "wrote video runtime summary to $video_runtime_summary_path"
elif [[ "$status_enabled" -eq 1 ]]; then
  note "video runtime extraction had ${video_runtime_failures} failed samples"
fi
validate_decision_expectations
validate_process_memory_expectations
validate_telemetry_expectations
validate_video_runtime_expectations

if [[ "$keep" -eq 1 ]]; then
  note "kept work dir: $work_dir"
else
  note "work dir will be removed; rerun with --keep to preserve evidence"
fi
note "metadata: $metadata_path"
note "samples:  $csv_path"
note "sample summary: $summary_path"
note "decisions: $decisions_path"
note "decision summary: $decision_summary_path"
note "telemetry: $telemetry_path"
note "telemetry summary: $telemetry_summary_path"
note "video runtime: $video_runtime_path"
note "video runtime summary: $video_runtime_summary_path"
note "summary: ${passes} passed, ${skips} skipped, ${failures} failed"
if [[ "$failures" -gt 0 ]]; then
  exit 1
fi
