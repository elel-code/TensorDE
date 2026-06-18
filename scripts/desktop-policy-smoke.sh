#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
usage: scripts/desktop-policy-smoke.sh [options]

Run a headless daemon smoke matrix for desktop-state performance policy. The
script uses validation overrides to create a virtual output, applies the static
example wallpaper, and samples decisions/telemetry for active, battery,
unfocused, fullscreen, hidden, inactive, locked, adaptive, and output override
scenarios.

Options:
  --output <name>       Virtual output name. Default: HEADLESS-1
  --work-dir <dir>      Parent directory for temporary smoke data
  --report-dir <dir>    Exact evidence directory. Created and kept
  --allow-missing       Report missing tools as skips instead of failures
  --no-build            Use existing target/debug binaries
  --sample-duration <s> Performance sampling duration. Default: 2
  --sample-interval <s> Performance sampling interval. Default: 1
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
  --expect-retained-private-delta-kib-at-most <kib>
                       Require sampled last-minus-first private memory delta to be at most this KiB value
  --expect-retained-uss-delta-kib-at-most <kib>
                       Require sampled last-minus-first USS/private delta to be at most this KiB value
  --expect-retained-pss-delta-kib-at-most <kib>
                       Require sampled last-minus-first PSS delta to be at most this KiB value
  --expect-peak-over-first-private-kib-at-most <kib>
                       Require sampled max-minus-first private memory delta to be at most this KiB value
  --expect-peak-over-first-uss-kib-at-most <kib>
                       Require sampled max-minus-first USS/private delta to be at most this KiB value
  --expect-peak-over-first-pss-kib-at-most <kib>
                       Require sampled max-minus-first PSS delta to be at most this KiB value
  --expect-render-sync-package-cache-entries-latest-at-most <count>
                       Require latest render_sync package cache entries to be at most count
  --expect-render-sync-planned-image-resource-references-latest-at-most <count>
                       Require latest planned image resource references to be at most count
  --expect-render-sync-planned-unique-image-resources-latest-at-most <count>
                       Require latest planned unique image resources to be at most count
  --expect-renderer-output-windows-latest-at-most <count>
                       Require latest renderer output window count to be at most count
  --expect-renderer-output-windows-max-at-most <count>
                       Require max sampled renderer output window count to be at most count
  --expect-renderer-static-surfaces-latest-at-most <count>
                       Require latest renderer static surface count to be at most count
  --expect-renderer-static-surfaces-max-at-most <count>
                       Require max sampled renderer static surface count to be at most count
  --expect-renderer-slideshow-surfaces-latest-at-most <count>
                       Require latest renderer slideshow surface count to be at most count
  --expect-renderer-slideshow-surfaces-max-at-most <count>
                       Require max sampled renderer slideshow surface count to be at most count
  --expect-renderer-video-surfaces-latest-at-most <count>
                       Require latest renderer video surface count to be at most count
  --expect-renderer-video-surfaces-max-at-most <count>
                       Require max sampled renderer video surface count to be at most count
  --expect-renderer-video-pipelines-latest-at-most <count>
                       Require latest renderer video pipeline count to be at most count
  --expect-renderer-video-pipelines-max-at-most <count>
                       Require max sampled renderer video pipeline count to be at most count
  --keep                Keep generated smoke data and logs
  -h, --help            Show this help text
EOF
}

output_name="HEADLESS-1"
work_parent="${TMPDIR:-/tmp}"
report_dir=""
allow_missing=0
build=1
keep=0
sample_duration=2
sample_interval=1
expect_max_rss_kib_at_most=""
expect_max_pss_kib_at_most=""
expect_max_private_kib_at_most=""
expect_max_uss_kib_at_most=""
expect_max_shared_kib_at_most=""
expect_retained_private_delta_kib_at_most=""
expect_retained_uss_delta_kib_at_most=""
expect_retained_pss_delta_kib_at_most=""
expect_peak_over_first_private_kib_at_most=""
expect_peak_over_first_uss_kib_at_most=""
expect_peak_over_first_pss_kib_at_most=""
expect_render_sync_package_cache_entries_latest_at_most=""
expect_render_sync_planned_image_resource_references_latest_at_most=""
expect_render_sync_planned_unique_image_resources_latest_at_most=""
expect_renderer_output_windows_latest_at_most=""
expect_renderer_output_windows_max_at_most=""
expect_renderer_static_surfaces_latest_at_most=""
expect_renderer_static_surfaces_max_at_most=""
expect_renderer_slideshow_surfaces_latest_at_most=""
expect_renderer_slideshow_surfaces_max_at_most=""
expect_renderer_video_surfaces_latest_at_most=""
expect_renderer_video_surfaces_max_at_most=""
expect_renderer_video_pipelines_latest_at_most=""
expect_renderer_video_pipelines_max_at_most=""

while [[ $# -gt 0 ]]; do
  case "$1" in
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
    --allow-missing)
      allow_missing=1
      shift
      ;;
    --no-build)
      build=0
      shift
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
    --expect-retained-pss-delta-kib-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-retained-pss-delta-kib-at-most requires a value" >&2; exit 2; }
      expect_retained_pss_delta_kib_at_most="$2"
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
    --expect-peak-over-first-pss-kib-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-peak-over-first-pss-kib-at-most requires a value" >&2; exit 2; }
      expect_peak_over_first_pss_kib_at_most="$2"
      shift 2
      ;;
    --expect-render-sync-package-cache-entries-latest-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-render-sync-package-cache-entries-latest-at-most requires a value" >&2; exit 2; }
      expect_render_sync_package_cache_entries_latest_at_most="$2"
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
cd "$repo_root"

failures=0
skips=0
passes=0
current_daemon_pid=""

note() {
  printf '%s\n' "$*"
}

csv_escape() {
  local value="$1"
  if [[ "$value" == *","* || "$value" == *"\""* || "$value" == *$'\n'* || "$value" == *$'\r'* ]]; then
    printf '"%s"' "${value//\"/\"\"}"
  else
    printf '%s' "$value"
  fi
}

pass() {
  passes=$((passes + 1))
  note "PASS: $*"
}

failure_status() {
  if [[ "$allow_missing" -eq 1 ]]; then
    printf '%s\n' "skip"
  else
    printf '%s\n' "fail"
  fi
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

require_file() {
  local file="$1"
  if [[ ! -x "$file" ]]; then
    skip_or_fail "missing executable $file"
    return 1
  fi
  return 0
}

stop_daemon() {
  if [[ -n "$current_daemon_pid" ]] && kill -0 "$current_daemon_pid" >/dev/null 2>&1; then
    kill "$current_daemon_pid" >/dev/null 2>&1 || true
    wait "$current_daemon_pid" >/dev/null 2>&1 || true
  fi
  current_daemon_pid=""
}

cleanup() {
  stop_daemon
  if [[ "${work_dir:-}" != "" && "$keep" -eq 0 && -z "$report_dir" ]]; then
    rm -rf "$work_dir"
  elif [[ "${work_dir:-}" != "" ]]; then
    printf 'kept work dir: %s\n' "$work_dir"
  fi
}
trap cleanup EXIT

if [[ ! "$sample_duration" =~ ^[1-9][0-9]*$ ]]; then
  echo "--sample-duration must be a positive integer" >&2
  exit 2
fi
if [[ ! "$sample_interval" =~ ^[1-9][0-9]*$ ]]; then
  echo "--sample-interval must be a positive integer" >&2
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
  "$expect_retained_private_delta_kib_at_most" \
  "$expect_retained_uss_delta_kib_at_most" \
  "$expect_retained_pss_delta_kib_at_most" \
  "$expect_peak_over_first_private_kib_at_most" \
  "$expect_peak_over_first_uss_kib_at_most" \
  "$expect_peak_over_first_pss_kib_at_most"
do
  if [[ -n "$memory_delta_expectation" && ! "$memory_delta_expectation" =~ ^[0-9]+$ ]]; then
    echo "memory delta KiB expectations must be non-negative integers" >&2
    exit 2
  fi
done
for render_sync_resource_expectation in \
  "$expect_render_sync_package_cache_entries_latest_at_most" \
  "$expect_render_sync_planned_image_resource_references_latest_at_most" \
  "$expect_render_sync_planned_unique_image_resources_latest_at_most"
do
  if [[ -n "$render_sync_resource_expectation" && ! "$render_sync_resource_expectation" =~ ^[0-9]+$ ]]; then
    echo "render sync resource expectations must be non-negative integers" >&2
    exit 2
  fi
done
for renderer_resource_expectation in \
  "$expect_renderer_output_windows_latest_at_most" \
  "$expect_renderer_output_windows_max_at_most" \
  "$expect_renderer_static_surfaces_latest_at_most" \
  "$expect_renderer_static_surfaces_max_at_most" \
  "$expect_renderer_slideshow_surfaces_latest_at_most" \
  "$expect_renderer_slideshow_surfaces_max_at_most" \
  "$expect_renderer_video_surfaces_latest_at_most" \
  "$expect_renderer_video_surfaces_max_at_most" \
  "$expect_renderer_video_pipelines_latest_at_most" \
  "$expect_renderer_video_pipelines_max_at_most"
do
  if [[ -n "$renderer_resource_expectation" && ! "$renderer_resource_expectation" =~ ^[0-9]+$ ]]; then
    echo "renderer resource expectations must be non-negative integers" >&2
    exit 2
  fi
done

require_command ps || true
require_command sed || true
require_command awk || true
if [[ "$build" -eq 1 ]]; then
  require_command cargo || true
fi

if [[ "$failures" -gt 0 || "$skips" -gt 0 ]]; then
  note "summary: ${passes} passed, ${skips} skipped, ${failures} failed"
  exit "$([[ "$failures" -gt 0 ]] && echo 1 || echo 0)"
fi

if [[ "$build" -eq 1 ]]; then
  cargo build
fi

gilderd="$repo_root/target/debug/gilderd"
gilderctl="$repo_root/target/debug/gilderctl"
performance_snapshot="$repo_root/scripts/performance-snapshot.sh"
wallpaper_path="$repo_root/examples/wallpapers/static-demo.gwpdir"
slideshow_wallpaper_path="$repo_root/examples/wallpapers/slideshow-demo.gwpdir"

require_file "$gilderd" || true
require_file "$gilderctl" || true
require_file "$performance_snapshot" || true
if [[ ! -d "$wallpaper_path" ]]; then
  skip_or_fail "missing example wallpaper $wallpaper_path"
fi
if [[ ! -d "$slideshow_wallpaper_path" ]]; then
  skip_or_fail "missing example wallpaper $slideshow_wallpaper_path"
fi
if [[ "$failures" -gt 0 || "$skips" -gt 0 ]]; then
  note "summary: ${passes} passed, ${skips} skipped, ${failures} failed"
  exit "$([[ "$failures" -gt 0 ]] && echo 1 || echo 0)"
fi

if [[ -n "$report_dir" ]]; then
  work_dir="$report_dir"
  mkdir -p "$work_dir"
else
  mkdir -p "$work_parent"
  work_dir="$(mktemp -d "${work_parent%/}/gilder-desktop-policy.XXXXXX")"
fi
metadata_path="$work_dir/metadata.txt"
matrix_path="$work_dir/matrix.csv"
resource_baseline_path="$work_dir/resource-baseline.csv"
summary_path="$work_dir/summary.txt"
cat > "$metadata_path" <<EOF
output: ${output_name}
sample_duration: ${sample_duration}
sample_interval: ${sample_interval}
expect_max_rss_kib_at_most: ${expect_max_rss_kib_at_most:-none}
expect_max_pss_kib_at_most: ${expect_max_pss_kib_at_most:-none}
expect_max_private_kib_at_most: ${expect_max_private_kib_at_most:-none}
expect_max_uss_kib_at_most: ${expect_max_uss_kib_at_most:-none}
expect_max_shared_kib_at_most: ${expect_max_shared_kib_at_most:-none}
expect_retained_private_delta_kib_at_most: ${expect_retained_private_delta_kib_at_most:-none}
expect_retained_uss_delta_kib_at_most: ${expect_retained_uss_delta_kib_at_most:-none}
expect_retained_pss_delta_kib_at_most: ${expect_retained_pss_delta_kib_at_most:-none}
expect_peak_over_first_private_kib_at_most: ${expect_peak_over_first_private_kib_at_most:-none}
expect_peak_over_first_uss_kib_at_most: ${expect_peak_over_first_uss_kib_at_most:-none}
expect_peak_over_first_pss_kib_at_most: ${expect_peak_over_first_pss_kib_at_most:-none}
expect_render_sync_package_cache_entries_latest_at_most: ${expect_render_sync_package_cache_entries_latest_at_most:-none}
expect_render_sync_planned_image_resource_references_latest_at_most: ${expect_render_sync_planned_image_resource_references_latest_at_most:-none}
expect_render_sync_planned_unique_image_resources_latest_at_most: ${expect_render_sync_planned_unique_image_resources_latest_at_most:-none}
expect_renderer_output_windows_latest_at_most: ${expect_renderer_output_windows_latest_at_most:-none}
expect_renderer_output_windows_max_at_most: ${expect_renderer_output_windows_max_at_most:-none}
expect_renderer_static_surfaces_latest_at_most: ${expect_renderer_static_surfaces_latest_at_most:-none}
expect_renderer_static_surfaces_max_at_most: ${expect_renderer_static_surfaces_max_at_most:-none}
expect_renderer_slideshow_surfaces_latest_at_most: ${expect_renderer_slideshow_surfaces_latest_at_most:-none}
expect_renderer_slideshow_surfaces_max_at_most: ${expect_renderer_slideshow_surfaces_max_at_most:-none}
expect_renderer_video_surfaces_latest_at_most: ${expect_renderer_video_surfaces_latest_at_most:-none}
expect_renderer_video_surfaces_max_at_most: ${expect_renderer_video_surfaces_max_at_most:-none}
expect_renderer_video_pipelines_latest_at_most: ${expect_renderer_video_pipelines_latest_at_most:-none}
expect_renderer_video_pipelines_max_at_most: ${expect_renderer_video_pipelines_max_at_most:-none}
wallpaper: ${wallpaper_path}
slideshow_wallpaper: ${slideshow_wallpaper_path}
EOF
printf 'scenario,status,expected_mode,expected_reason,expected_max_fps,expected_action,expected_plan_kind,power_state,output_state,session_state,adaptive_state,config_profile,status_before,status_after,performance_dir,daemon_log\n' > "$matrix_path"
printf 'scenario,status,expected_mode,expected_reason,expected_max_fps,expected_action,expected_plan_kind,power_state,output_state,session_state,adaptive_state,config_profile,performance_dir,samples,avg_cpu_percent,avg_rss_kib,max_rss_kib,first_rss_kib,last_rss_kib,retained_rss_delta_kib,peak_over_first_rss_kib,avg_pss_kib,max_pss_kib,first_pss_kib,last_pss_kib,retained_pss_delta_kib,peak_over_first_pss_kib,avg_private_kib,max_private_kib,first_private_kib,last_private_kib,retained_private_delta_kib,peak_over_first_private_kib,avg_uss_kib,max_uss_kib,first_uss_kib,last_uss_kib,retained_uss_delta_kib,peak_over_first_uss_kib,avg_shared_kib,max_shared_kib,first_shared_kib,last_shared_kib,retained_shared_delta_kib,peak_over_first_shared_kib,gpu_busy_samples,avg_gpu_busy_percent,max_gpu_busy_percent,decision_rows,decision_outputs,decision_samples,telemetry_rows,desktop_refreshes_delta,desktop_refresh_skips_delta,render_sync_cache_hits_delta,render_sync_cache_misses_delta,render_sync_cache_hit_ratio,render_sync_updates_queued_latest,render_sync_updates_skipped_latest,render_sync_package_cache_entries_latest,render_sync_package_cache_max_entries_latest,render_sync_package_cache_hits_latest,render_sync_package_cache_misses_latest,render_sync_package_cache_evictions_latest,render_sync_archive_cache_entries_latest,render_sync_archive_cache_max_entries_latest,render_sync_archive_cache_reuses_latest,render_sync_archive_cache_extractions_latest,render_sync_archive_cache_evictions_delta,render_sync_archive_cache_evictions_latest,render_sync_archive_cache_eviction_errors_delta,render_sync_archive_cache_eviction_errors_latest,render_sync_planned_static_image_resources_latest,render_sync_planned_video_poster_resources_latest,render_sync_planned_slideshow_image_resources_latest,render_sync_planned_image_resource_references_latest,render_sync_planned_unique_image_resources_latest,adaptive_action_types_latest,adaptive_action_max_fps_latest,renderer_output_windows_latest,renderer_output_windows_max,renderer_static_surfaces_latest,renderer_static_surfaces_max,renderer_slideshow_surfaces_latest,renderer_slideshow_surfaces_max,renderer_video_surfaces_latest,renderer_video_surfaces_max,renderer_video_pipelines_latest,renderer_video_pipelines_max,renderer_video_qos_messages_max,renderer_video_qos_dropped_max,renderer_video_gtk_frame_clock_ticks_max\n' > "$resource_baseline_path"

write_config_profile() {
  local config_file="$1"
  local profile="$2"
  [[ -n "$profile" ]] || return 0

  mkdir -p "${config_file%/*}"
  case "$profile" in
    output-active-42fps)
      cat > "$config_file" <<EOF
[outputs."${output_name}".performance]
interactive_max_fps = 42
EOF
      ;;
    output-unfocused-12fps)
      cat > "$config_file" <<EOF
[outputs."${output_name}".performance]
background_max_fps = 12
EOF
      ;;
    output-battery-pause)
      cat > "$config_file" <<EOF
[outputs."${output_name}".performance]
battery = "pause"
EOF
      ;;
    adaptive-throttle)
      cat > "$config_file" <<EOF
[adaptive]
enabled = true
refresh_interval_ms = 250
cooldown_ms = 1000
throttle_max_fps = 11
action = "throttle"
EOF
      ;;
    adaptive-gpu-throttle)
      cat > "$config_file" <<EOF
[adaptive]
enabled = true
refresh_interval_ms = 250
cooldown_ms = 1000
throttle_max_fps = 11
action = "throttle"
cpu_pressure_threshold_percent = 0
memory_pressure_threshold_percent = 0
temperature_threshold_celsius = 0
gpu_busy_threshold_percent = 50
battery_capacity_threshold_percent = 0
EOF
      ;;
    adaptive-pause-unfocused)
      cat > "$config_file" <<EOF
[adaptive]
enabled = true
refresh_interval_ms = 250
cooldown_ms = 1000
throttle_max_fps = 11
action = "pause-unfocused"
EOF
      ;;
    adaptive-pause-dynamic)
      cat > "$config_file" <<EOF
[adaptive]
enabled = true
refresh_interval_ms = 250
cooldown_ms = 1000
throttle_max_fps = 11
action = "pause-dynamic"
EOF
      ;;
    adaptive-low-battery-pause-dynamic)
      cat > "$config_file" <<EOF
[adaptive]
enabled = true
refresh_interval_ms = 250
cooldown_ms = 1000
throttle_max_fps = 11
action = "pause-dynamic"
cpu_pressure_threshold_percent = 0
memory_pressure_threshold_percent = 0
temperature_threshold_celsius = 0
gpu_busy_threshold_percent = 0
battery_capacity_threshold_percent = 50
EOF
      ;;
    *)
      echo "unknown config profile: $profile" >&2
      return 2
      ;;
  esac
}

summary_value_or_empty() {
  local summary="$1"
  local key="$2"
  if [[ ! -f "$summary" ]]; then
    return 0
  fi
  awk -v key="$key" -F': ' '$1 == key { print $2; found = 1; exit } END { exit found ? 0 : 1 }' "$summary" || true
}

record_resource_baseline() {
  local scenario="$1"
  local status="$2"
  local expected_mode="$3"
  local expected_reason="$4"
  local expected_max_fps="$5"
  local expected_action="$6"
  local expected_plan_kind="$7"
  local power_state="$8"
  local output_state="$9"
  local session_state="${10}"
  local adaptive_state="${11}"
  local config_profile="${12}"
  local perf_dir="${13}"

  local process_summary="$perf_dir/summary.txt"
  local decision_summary="$perf_dir/decision-summary.txt"
  local telemetry_summary="$perf_dir/telemetry-summary.txt"
  local row=(
    "$scenario"
    "$status"
    "$expected_mode"
    "$expected_reason"
    "$expected_max_fps"
    "$expected_action"
    "$expected_plan_kind"
    "$power_state"
    "$output_state"
    "$session_state"
    "$adaptive_state"
    "$config_profile"
    "${perf_dir#$work_dir/}"
    "$(summary_value_or_empty "$process_summary" samples)"
    "$(summary_value_or_empty "$process_summary" avg_cpu_percent)"
    "$(summary_value_or_empty "$process_summary" avg_rss_kib)"
    "$(summary_value_or_empty "$process_summary" max_rss_kib)"
    "$(summary_value_or_empty "$process_summary" first_rss_kib)"
    "$(summary_value_or_empty "$process_summary" last_rss_kib)"
    "$(summary_value_or_empty "$process_summary" retained_rss_delta_kib)"
    "$(summary_value_or_empty "$process_summary" peak_over_first_rss_kib)"
    "$(summary_value_or_empty "$process_summary" avg_pss_kib)"
    "$(summary_value_or_empty "$process_summary" max_pss_kib)"
    "$(summary_value_or_empty "$process_summary" first_pss_kib)"
    "$(summary_value_or_empty "$process_summary" last_pss_kib)"
    "$(summary_value_or_empty "$process_summary" retained_pss_delta_kib)"
    "$(summary_value_or_empty "$process_summary" peak_over_first_pss_kib)"
    "$(summary_value_or_empty "$process_summary" avg_private_kib)"
    "$(summary_value_or_empty "$process_summary" max_private_kib)"
    "$(summary_value_or_empty "$process_summary" first_private_kib)"
    "$(summary_value_or_empty "$process_summary" last_private_kib)"
    "$(summary_value_or_empty "$process_summary" retained_private_delta_kib)"
    "$(summary_value_or_empty "$process_summary" peak_over_first_private_kib)"
    "$(summary_value_or_empty "$process_summary" avg_uss_kib)"
    "$(summary_value_or_empty "$process_summary" max_uss_kib)"
    "$(summary_value_or_empty "$process_summary" first_uss_kib)"
    "$(summary_value_or_empty "$process_summary" last_uss_kib)"
    "$(summary_value_or_empty "$process_summary" retained_uss_delta_kib)"
    "$(summary_value_or_empty "$process_summary" peak_over_first_uss_kib)"
    "$(summary_value_or_empty "$process_summary" avg_shared_kib)"
    "$(summary_value_or_empty "$process_summary" max_shared_kib)"
    "$(summary_value_or_empty "$process_summary" first_shared_kib)"
    "$(summary_value_or_empty "$process_summary" last_shared_kib)"
    "$(summary_value_or_empty "$process_summary" retained_shared_delta_kib)"
    "$(summary_value_or_empty "$process_summary" peak_over_first_shared_kib)"
    "$(summary_value_or_empty "$process_summary" gpu_busy_samples)"
    "$(summary_value_or_empty "$process_summary" avg_gpu_busy_percent)"
    "$(summary_value_or_empty "$process_summary" max_gpu_busy_percent)"
    "$(summary_value_or_empty "$decision_summary" decision_rows)"
    "$(summary_value_or_empty "$decision_summary" decision_outputs)"
    "$(summary_value_or_empty "$decision_summary" decision_samples)"
    "$(summary_value_or_empty "$telemetry_summary" telemetry_rows)"
    "$(summary_value_or_empty "$telemetry_summary" desktop_refreshes_delta)"
    "$(summary_value_or_empty "$telemetry_summary" desktop_refresh_skips_delta)"
    "$(summary_value_or_empty "$telemetry_summary" render_sync_cache_hits_delta)"
    "$(summary_value_or_empty "$telemetry_summary" render_sync_cache_misses_delta)"
    "$(summary_value_or_empty "$telemetry_summary" render_sync_cache_hit_ratio)"
    "$(summary_value_or_empty "$telemetry_summary" render_sync_updates_queued_latest)"
    "$(summary_value_or_empty "$telemetry_summary" render_sync_updates_skipped_latest)"
    "$(summary_value_or_empty "$telemetry_summary" render_sync_package_cache_entries_latest)"
    "$(summary_value_or_empty "$telemetry_summary" render_sync_package_cache_max_entries_latest)"
    "$(summary_value_or_empty "$telemetry_summary" render_sync_package_cache_hits_latest)"
    "$(summary_value_or_empty "$telemetry_summary" render_sync_package_cache_misses_latest)"
    "$(summary_value_or_empty "$telemetry_summary" render_sync_package_cache_evictions_latest)"
    "$(summary_value_or_empty "$telemetry_summary" render_sync_archive_cache_entries_latest)"
    "$(summary_value_or_empty "$telemetry_summary" render_sync_archive_cache_max_entries_latest)"
    "$(summary_value_or_empty "$telemetry_summary" render_sync_archive_cache_reuses_latest)"
    "$(summary_value_or_empty "$telemetry_summary" render_sync_archive_cache_extractions_latest)"
    "$(summary_value_or_empty "$telemetry_summary" render_sync_archive_cache_evictions_delta)"
    "$(summary_value_or_empty "$telemetry_summary" render_sync_archive_cache_evictions_latest)"
    "$(summary_value_or_empty "$telemetry_summary" render_sync_archive_cache_eviction_errors_delta)"
    "$(summary_value_or_empty "$telemetry_summary" render_sync_archive_cache_eviction_errors_latest)"
    "$(summary_value_or_empty "$telemetry_summary" render_sync_planned_static_image_resources_latest)"
    "$(summary_value_or_empty "$telemetry_summary" render_sync_planned_video_poster_resources_latest)"
    "$(summary_value_or_empty "$telemetry_summary" render_sync_planned_slideshow_image_resources_latest)"
    "$(summary_value_or_empty "$telemetry_summary" render_sync_planned_image_resource_references_latest)"
    "$(summary_value_or_empty "$telemetry_summary" render_sync_planned_unique_image_resources_latest)"
    "$(summary_value_or_empty "$telemetry_summary" adaptive_action_types_latest)"
    "$(summary_value_or_empty "$telemetry_summary" adaptive_action_max_fps_latest)"
    "$(summary_value_or_empty "$telemetry_summary" renderer_output_windows_latest)"
    "$(summary_value_or_empty "$telemetry_summary" renderer_output_windows_max)"
    "$(summary_value_or_empty "$telemetry_summary" renderer_static_surfaces_latest)"
    "$(summary_value_or_empty "$telemetry_summary" renderer_static_surfaces_max)"
    "$(summary_value_or_empty "$telemetry_summary" renderer_slideshow_surfaces_latest)"
    "$(summary_value_or_empty "$telemetry_summary" renderer_slideshow_surfaces_max)"
    "$(summary_value_or_empty "$telemetry_summary" renderer_video_surfaces_latest)"
    "$(summary_value_or_empty "$telemetry_summary" renderer_video_surfaces_max)"
    "$(summary_value_or_empty "$telemetry_summary" renderer_video_pipelines_latest)"
    "$(summary_value_or_empty "$telemetry_summary" renderer_video_pipelines_max)"
    "$(summary_value_or_empty "$telemetry_summary" renderer_video_qos_messages_max)"
    "$(summary_value_or_empty "$telemetry_summary" renderer_video_qos_dropped_max)"
    "$(summary_value_or_empty "$telemetry_summary" renderer_video_gtk_frame_clock_ticks_max)"
  )
  local index
  for index in "${!row[@]}"; do
    if [[ "$index" -gt 0 ]]; then
      printf ',' >> "$resource_baseline_path"
    fi
    csv_escape "${row[$index]}" >> "$resource_baseline_path"
  done
  printf '\n' >> "$resource_baseline_path"
}

record_scenario() {
  local scenario="$1"
  local status="$2"
  local expected_mode="$3"
  local expected_reason="$4"
  local expected_max_fps="$5"
  local expected_action="$6"
  local expected_plan_kind="$7"
  local power_state="$8"
  local output_state="$9"
  local session_state="${10}"
  local adaptive_state="${11}"
  local config_profile="${12}"
  local status_before="${13}"
  local status_after="${14}"
  local perf_dir="${15}"
  local daemon_log="${16}"

  local row=(
    "$scenario"
    "$status"
    "$expected_mode"
    "$expected_reason"
    "$expected_max_fps"
    "$expected_action"
    "$expected_plan_kind"
    "$power_state"
    "$output_state"
    "$session_state"
    "$adaptive_state"
    "$config_profile"
    "${status_before#$work_dir/}"
    "${status_after#$work_dir/}"
    "${perf_dir#$work_dir/}"
    "${daemon_log#$work_dir/}"
  )
  local index
  for index in "${!row[@]}"; do
    if [[ "$index" -gt 0 ]]; then
      printf ',' >> "$matrix_path"
    fi
    csv_escape "${row[$index]}" >> "$matrix_path"
  done
  printf '\n' >> "$matrix_path"
  record_resource_baseline "$scenario" "$status" "$expected_mode" "$expected_reason" "$expected_max_fps" "$expected_action" "$expected_plan_kind" "$power_state" "$output_state" "$session_state" "$adaptive_state" "$config_profile" "$perf_dir"
}

write_summary() {
  cat > "$summary_path" <<EOF
passed: ${passes}
skipped: ${skips}
failed: ${failures}
metadata: ${metadata_path}
matrix: ${matrix_path}
resource_baseline: ${resource_baseline_path}
EOF
}

expected_adaptive_action_for_scenario() {
  local config_profile="$1"
  local output_state="$2"

  case "$config_profile" in
    adaptive-throttle)
      printf '%s\n' "throttle"
      ;;
    adaptive-gpu-throttle)
      printf '%s\n' "throttle"
      ;;
    adaptive-pause-unfocused)
      if [[ "$output_state" == "unfocused" ]]; then
        printf '%s\n' "pause-unfocused"
      else
        printf '%s\n' "throttle"
      fi
      ;;
    adaptive-pause-dynamic)
      printf '%s\n' "pause-dynamic"
      ;;
    adaptive-low-battery-pause-dynamic)
      printf '%s\n' "pause-dynamic"
      ;;
  esac
}

append_process_memory_expectations() {
  local -n args_ref="$1"
  if [[ -n "$expect_max_rss_kib_at_most" ]]; then
    args_ref+=(--expect-max-rss-kib-at-most "$expect_max_rss_kib_at_most")
  fi
  if [[ -n "$expect_max_pss_kib_at_most" ]]; then
    args_ref+=(--expect-max-pss-kib-at-most "$expect_max_pss_kib_at_most")
  fi
  if [[ -n "$expect_max_private_kib_at_most" ]]; then
    args_ref+=(--expect-max-private-kib-at-most "$expect_max_private_kib_at_most")
  fi
  if [[ -n "$expect_max_uss_kib_at_most" ]]; then
    args_ref+=(--expect-max-uss-kib-at-most "$expect_max_uss_kib_at_most")
  fi
  if [[ -n "$expect_max_shared_kib_at_most" ]]; then
    args_ref+=(--expect-max-shared-kib-at-most "$expect_max_shared_kib_at_most")
  fi
  if [[ -n "$expect_retained_private_delta_kib_at_most" ]]; then
    args_ref+=(--expect-retained-private-delta-kib-at-most "$expect_retained_private_delta_kib_at_most")
  fi
  if [[ -n "$expect_retained_uss_delta_kib_at_most" ]]; then
    args_ref+=(--expect-retained-uss-delta-kib-at-most "$expect_retained_uss_delta_kib_at_most")
  fi
  if [[ -n "$expect_retained_pss_delta_kib_at_most" ]]; then
    args_ref+=(--expect-retained-pss-delta-kib-at-most "$expect_retained_pss_delta_kib_at_most")
  fi
  if [[ -n "$expect_peak_over_first_private_kib_at_most" ]]; then
    args_ref+=(--expect-peak-over-first-private-kib-at-most "$expect_peak_over_first_private_kib_at_most")
  fi
  if [[ -n "$expect_peak_over_first_uss_kib_at_most" ]]; then
    args_ref+=(--expect-peak-over-first-uss-kib-at-most "$expect_peak_over_first_uss_kib_at_most")
  fi
  if [[ -n "$expect_peak_over_first_pss_kib_at_most" ]]; then
    args_ref+=(--expect-peak-over-first-pss-kib-at-most "$expect_peak_over_first_pss_kib_at_most")
  fi
  if [[ -n "$expect_render_sync_package_cache_entries_latest_at_most" ]]; then
    args_ref+=(--expect-render-sync-package-cache-entries-latest-at-most "$expect_render_sync_package_cache_entries_latest_at_most")
  fi
  if [[ -n "$expect_renderer_output_windows_latest_at_most" ]]; then
    args_ref+=(--expect-renderer-output-windows-latest-at-most "$expect_renderer_output_windows_latest_at_most")
  fi
  if [[ -n "$expect_renderer_output_windows_max_at_most" ]]; then
    args_ref+=(--expect-renderer-output-windows-max-at-most "$expect_renderer_output_windows_max_at_most")
  fi
  if [[ -n "$expect_renderer_static_surfaces_latest_at_most" ]]; then
    args_ref+=(--expect-renderer-static-surfaces-latest-at-most "$expect_renderer_static_surfaces_latest_at_most")
  fi
  if [[ -n "$expect_renderer_static_surfaces_max_at_most" ]]; then
    args_ref+=(--expect-renderer-static-surfaces-max-at-most "$expect_renderer_static_surfaces_max_at_most")
  fi
  if [[ -n "$expect_renderer_slideshow_surfaces_latest_at_most" ]]; then
    args_ref+=(--expect-renderer-slideshow-surfaces-latest-at-most "$expect_renderer_slideshow_surfaces_latest_at_most")
  fi
  if [[ -n "$expect_renderer_slideshow_surfaces_max_at_most" ]]; then
    args_ref+=(--expect-renderer-slideshow-surfaces-max-at-most "$expect_renderer_slideshow_surfaces_max_at_most")
  fi
  if [[ -n "$expect_renderer_video_surfaces_latest_at_most" ]]; then
    args_ref+=(--expect-renderer-video-surfaces-latest-at-most "$expect_renderer_video_surfaces_latest_at_most")
  fi
  if [[ -n "$expect_renderer_video_surfaces_max_at_most" ]]; then
    args_ref+=(--expect-renderer-video-surfaces-max-at-most "$expect_renderer_video_surfaces_max_at_most")
  fi
  if [[ -n "$expect_renderer_video_pipelines_latest_at_most" ]]; then
    args_ref+=(--expect-renderer-video-pipelines-latest-at-most "$expect_renderer_video_pipelines_latest_at_most")
  fi
  if [[ -n "$expect_renderer_video_pipelines_max_at_most" ]]; then
    args_ref+=(--expect-renderer-video-pipelines-max-at-most "$expect_renderer_video_pipelines_max_at_most")
  fi
}

minimum_optional_limit() {
  local left="$1"
  local right="$2"
  if [[ -z "$left" ]]; then
    printf '%s\n' "$right"
  elif [[ -z "$right" ]]; then
    printf '%s\n' "$left"
  elif (( 10#$left < 10#$right )); then
    printf '%s\n' "$left"
  else
    printf '%s\n' "$right"
  fi
}

append_render_sync_resource_expectations() {
  local -n args_ref="$1"
  local expected_planned_references="$2"
  local expected_unique_resources="$3"
  local effective_planned_references
  local effective_unique_resources
  effective_planned_references="$(minimum_optional_limit \
    "$expect_render_sync_planned_image_resource_references_latest_at_most" \
    "$expected_planned_references")"
  effective_unique_resources="$(minimum_optional_limit \
    "$expect_render_sync_planned_unique_image_resources_latest_at_most" \
    "$expected_unique_resources")"

  if [[ -n "$effective_planned_references" ]]; then
    args_ref+=(--expect-render-sync-planned-image-resource-references-latest-at-most "$effective_planned_references")
  fi
  if [[ -n "$effective_unique_resources" ]]; then
    args_ref+=(--expect-render-sync-planned-unique-image-resources-latest-at-most "$effective_unique_resources")
  fi
}

run_scenario() {
  local name="$1"
  local expected_mode="$2"
  local expected_reason="$3"
  local expected_max_fps="$4"
  local expected_action="$5"
  local expected_plan_kind="$6"
  local power_state="$7"
  local output_state="$8"
  local session_state="$9"
  local config_profile="${10:-}"
  local adaptive_state="${11:-}"
  local expected_planned_image_references="${12:-}"
  local expected_planned_unique_resources="${13:-}"

  local scenario_dir="$work_dir/$name"
  local socket="$scenario_dir/runtime/gilder.sock"
  local config_file="$scenario_dir/config/gilder/config.toml"
  local daemon_log="$scenario_dir/gilderd.log"
  local status_before="$scenario_dir/status-before.json"
  local status_after="$scenario_dir/status-after.json"
  local perf_dir="$scenario_dir/performance"
  local perf_log="$scenario_dir/performance.log"
  local scenario_wallpaper_path="$wallpaper_path"
  local expected_adaptive_action
  expected_adaptive_action="$(expected_adaptive_action_for_scenario "$config_profile" "$output_state")"
  local scenario_status="pass"
  if [[ "$name" == *"pause-dynamic-slideshow" ]]; then
    scenario_wallpaper_path="$slideshow_wallpaper_path"
  fi

  mkdir -p "$scenario_dir/runtime" "$scenario_dir/config" "$scenario_dir/state" "$scenario_dir/cache"
  chmod 700 "$scenario_dir/runtime"
  if ! write_config_profile "$config_file" "$config_profile"; then
    scenario_status="$(failure_status)"
    skip_or_fail "${name}: failed to write config profile ${config_profile}"
    record_scenario "$name" "$scenario_status" "$expected_mode" "$expected_reason" "$expected_max_fps" "$expected_action" "$expected_plan_kind" "$power_state" "$output_state" "$session_state" "$adaptive_state" "$config_profile" "$status_before" "$status_after" "$perf_dir" "$daemon_log"
    return 0
  fi

  local -a daemon_env
  daemon_env=(
    env
    GILDER_SOCKET="$socket"
    XDG_RUNTIME_DIR="$scenario_dir/runtime"
    XDG_CONFIG_HOME="$scenario_dir/config"
    XDG_STATE_HOME="$scenario_dir/state"
    XDG_CACHE_HOME="$scenario_dir/cache"
    GILDER_DESKTOP_OUTPUTS="${output_name}:1280x720@1"
    GILDER_POWER_STATE="$power_state"
    GILDER_OUTPUT_STATE="$output_state"
    GILDER_SESSION_STATE="$session_state"
  )
  if [[ -n "$adaptive_state" ]]; then
    daemon_env+=(GILDER_ADAPTIVE_STATE="$adaptive_state")
  fi

  "${daemon_env[@]}" "$gilderd" >"$daemon_log" 2>&1 &
  current_daemon_pid=$!

  for _ in $(seq 1 80); do
    if [[ -S "$socket" ]]; then
      break
    fi
    if ! kill -0 "$current_daemon_pid" >/dev/null 2>&1; then
      note "daemon log for ${name}:"
      sed -n '1,120p' "$daemon_log"
      scenario_status="$(failure_status)"
      skip_or_fail "${name}: gilderd exited before creating IPC socket"
      record_scenario "$name" "$scenario_status" "$expected_mode" "$expected_reason" "$expected_max_fps" "$expected_action" "$expected_plan_kind" "$power_state" "$output_state" "$session_state" "$adaptive_state" "$config_profile" "$status_before" "$status_after" "$perf_dir" "$daemon_log"
      stop_daemon
      return 0
    fi
    sleep 0.1
  done
  if [[ ! -S "$socket" ]]; then
    note "daemon log for ${name}:"
    sed -n '1,120p' "$daemon_log"
    scenario_status="$(failure_status)"
    skip_or_fail "${name}: gilderd did not create IPC socket"
    record_scenario "$name" "$scenario_status" "$expected_mode" "$expected_reason" "$expected_max_fps" "$expected_action" "$expected_plan_kind" "$power_state" "$output_state" "$session_state" "$adaptive_state" "$config_profile" "$status_before" "$status_after" "$perf_dir" "$daemon_log"
    stop_daemon
    return 0
  fi
  pass "${name}: started isolated daemon"

  if ! env GILDER_SOCKET="$socket" "$gilderctl" status > "$status_before"; then
    scenario_status="$(failure_status)"
    skip_or_fail "${name}: failed to capture initial status"
    record_scenario "$name" "$scenario_status" "$expected_mode" "$expected_reason" "$expected_max_fps" "$expected_action" "$expected_plan_kind" "$power_state" "$output_state" "$session_state" "$adaptive_state" "$config_profile" "$status_before" "$status_after" "$perf_dir" "$daemon_log"
    stop_daemon
    return 0
  fi
  if grep -Fq "\"name\":\"${output_name}\"" "$status_before"; then
    pass "${name}: status reports virtual output"
  else
    scenario_status="$(failure_status)"
    skip_or_fail "${name}: status does not report virtual output"
  fi

  if ! env GILDER_SOCKET="$socket" "$gilderctl" set "$scenario_wallpaper_path" --output "$output_name" >/dev/null; then
    scenario_status="$(failure_status)"
    skip_or_fail "${name}: failed to set wallpaper"
    record_scenario "$name" "$scenario_status" "$expected_mode" "$expected_reason" "$expected_max_fps" "$expected_action" "$expected_plan_kind" "$power_state" "$output_state" "$session_state" "$adaptive_state" "$config_profile" "$status_before" "$status_after" "$perf_dir" "$daemon_log"
    stop_daemon
    return 0
  fi
  if ! env GILDER_SOCKET="$socket" "$gilderctl" set "$scenario_wallpaper_path" --output "$output_name" >/dev/null; then
    scenario_status="$(failure_status)"
    skip_or_fail "${name}: failed to repeat wallpaper set"
    record_scenario "$name" "$scenario_status" "$expected_mode" "$expected_reason" "$expected_max_fps" "$expected_action" "$expected_plan_kind" "$power_state" "$output_state" "$session_state" "$adaptive_state" "$config_profile" "$status_before" "$status_after" "$perf_dir" "$daemon_log"
    stop_daemon
    return 0
  fi
  if ! env GILDER_SOCKET="$socket" "$gilderctl" status > "$status_after"; then
    scenario_status="$(failure_status)"
    skip_or_fail "${name}: failed to capture status after set"
    record_scenario "$name" "$scenario_status" "$expected_mode" "$expected_reason" "$expected_max_fps" "$expected_action" "$expected_plan_kind" "$power_state" "$output_state" "$session_state" "$adaptive_state" "$config_profile" "$status_before" "$status_after" "$perf_dir" "$daemon_log"
    stop_daemon
    return 0
  fi
  if grep -q "\"reason\":\"${expected_reason}\"" "$status_after"; then
    pass "${name}: status reports ${expected_reason} decision"
  else
    scenario_status="$(failure_status)"
    skip_or_fail "${name}: status does not report ${expected_reason} decision"
  fi
  if [[ -n "$expected_max_fps" ]]; then
    if grep -q "\"max_fps\":${expected_max_fps}" "$status_after"; then
      pass "${name}: status reports max_fps ${expected_max_fps}"
    else
      scenario_status="$(failure_status)"
      skip_or_fail "${name}: status does not report max_fps ${expected_max_fps}"
    fi
  fi

  local -a sample_args
  sample_args=(
    --pid "$current_daemon_pid"
    --socket "$socket"
    --gilderctl "$gilderctl"
    --label "desktop-policy-${name}"
    --duration "$sample_duration"
    --interval "$sample_interval"
    --output-dir "$perf_dir"
    --keep
    --expect-mode "$expected_mode"
    --expect-reason "$expected_reason"
    --expect-action "$expected_action"
    --expect-render-sync-cache-hit
    --expect-desktop-refresh-skip
    --expect-render-sync-update-queued
    --expect-render-sync-update-skipped
  )
  append_process_memory_expectations sample_args
  append_render_sync_resource_expectations \
    sample_args \
    "$expected_planned_image_references" \
    "$expected_planned_unique_resources"
  if [[ -n "$expected_plan_kind" ]]; then
    sample_args+=(--expect-plan-kind "$expected_plan_kind")
  fi
  if [[ -n "$expected_max_fps" ]]; then
    sample_args+=(--expect-max-fps "$expected_max_fps")
  fi
  if [[ -n "$expected_adaptive_action" ]]; then
    sample_args+=(--expect-adaptive-action "$expected_adaptive_action")
  fi
  if [[ "$allow_missing" -eq 1 ]]; then
    sample_args+=(--allow-missing)
  fi

  if "$performance_snapshot" "${sample_args[@]}" >"$perf_log" 2>&1; then
    pass "${name}: captured policy performance evidence"
  else
    note "performance log for ${name}:"
    sed -n '1,160p' "$perf_log"
    scenario_status="$(failure_status)"
    skip_or_fail "${name}: performance sampling failed"
  fi

  note "${name}: status before: $status_before"
  note "${name}: status after:  $status_after"
  note "${name}: performance:   $perf_dir"
  note "${name}: daemon log:    $daemon_log"
  record_scenario "$name" "$scenario_status" "$expected_mode" "$expected_reason" "$expected_max_fps" "$expected_action" "$expected_plan_kind" "$power_state" "$output_state" "$session_state" "$adaptive_state" "$config_profile" "$status_before" "$status_after" "$perf_dir" "$daemon_log"
  stop_daemon
}

run_scenario active active interactive 60 render static-image ac active active "" "" 1 1
run_scenario battery throttled battery 24 render static-image battery active active "" "" 1 1
run_scenario unfocused throttled unfocused 30 render static-image ac unfocused active "" "" 1 1
run_scenario fullscreen paused fullscreen "" remove "" ac fullscreen active "" "" 0 0
run_scenario hidden paused output-hidden "" remove "" ac hidden active "" "" 0 0
run_scenario session-inactive paused session-inactive "" remove "" ac active inactive "" "" 0 0
run_scenario session-locked paused session-locked "" remove "" ac active locked "" "" 0 0
run_scenario output-active-42fps active interactive 42 render static-image ac active active output-active-42fps "" 1 1
run_scenario output-unfocused-12fps throttled unfocused 12 render static-image ac unfocused active output-unfocused-12fps "" 1 1
run_scenario output-battery-pause paused battery "" remove "" battery active active output-battery-pause "" 0 0
run_scenario adaptive-throttle throttled adaptive 11 render static-image ac active active adaptive-throttle cpu-pressure 1 1
run_scenario adaptive-gpu-throttle throttled adaptive 11 render static-image ac active active adaptive-gpu-throttle gpu-busy 1 1
run_scenario adaptive-pause-unfocused paused adaptive "" remove "" ac unfocused active adaptive-pause-unfocused cpu-pressure 0 0
run_scenario adaptive-pause-focused-fallback throttled adaptive 11 render static-image ac active active adaptive-pause-unfocused cpu-pressure 1 1
run_scenario adaptive-pause-dynamic-static active interactive 60 render static-image ac active active adaptive-pause-dynamic cpu-pressure 1 1
run_scenario adaptive-pause-dynamic-slideshow paused adaptive "" remove "" ac active active adaptive-pause-dynamic cpu-pressure 0 0
run_scenario adaptive-low-battery-pause-dynamic-slideshow paused adaptive "" remove "" ac active active adaptive-low-battery-pause-dynamic low-battery 0 0

write_summary
note "metadata: $metadata_path"
note "matrix:   $matrix_path"
note "resources: $resource_baseline_path"
note "report:   $summary_path"
note "summary: ${passes} passed, ${skips} skipped, ${failures} failed"
if [[ "$failures" -gt 0 ]]; then
  exit 1
fi
