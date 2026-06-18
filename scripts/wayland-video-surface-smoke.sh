#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
usage: scripts/wayland-video-surface-smoke.sh [options]

Run an interactive Wayland-session smoke test for the GTK/layer-shell video
surface path. The script builds Gilder, generates a tiny video wallpaper,
starts an isolated daemon on a temporary socket, applies the wallpaper to an
output, and records status/log evidence.

Options:
  --output <name>     Output connector name. Default: first daemon-reported output
  --all-outputs       Apply the generated video wallpaper to every reported output
  --expect-compositor <kind>
                     Require desktop.compositor to be hyprland, niri, generic-wayland, or none
  --work-dir <dir>    Parent directory for temporary smoke data
  --report-dir <dir>  Exact evidence directory. Created and kept
  --preflight         Check session, tools, binaries, and GStreamer elements without applying wallpaper
  --allow-missing     Report missing session/tools/plugins as skips instead of failures
  --no-build          Use existing target/debug binaries
  --sample-performance
                     Capture active-video CPU/RSS/status evidence after applying wallpaper
  --sample-paused    Also pause the output, capture paused-video evidence, then resume
  --sample-duration <sec>
                     Performance sampling duration. Default: 8
  --sample-interval <sec>
                     Performance sampling interval. Default: 1
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
  --expect-decoder-policy-status <status>
                     Require sampled video runtime to report this decoder policy status
  --expect-decoder-class <hardware|software|unknown>
                     Require sampled video runtime to report this decoder class
  --expect-memory-feature <feature>
                     Require sampled video runtime to report this caps memory feature
  --expect-sink-memory-feature <feature>
                     Require sampled video runtime to report this sink-side caps memory feature
  --expect-zero-copy-evidence <level>
                     Require sampled video runtime to report this zero-copy evidence level
  --expect-video-position-progress
                     Require sampled video position to advance
  --expect-gtk-frame-clock
                     Require sampled GTK video frame clock ticks
  --expect-gtk-frame-clock-phase <phase>
                     Require GTK frame clock phase ticks. Phase: before-paint, update, layout, paint, after-paint, or all
  --expect-gtk-frame-timings
                     Require sampled completed GDK frame timings
  --expect-renderer-video-pipeline-lifecycle
                     Require active/resumed sampling to keep a video pipeline and paused/hidden/fullscreen sampling to release it
  --expect-render-sync-planned-image-resource-references-latest-at-most <count>
                     Require latest planned image resource references to be at most count
  --expect-render-sync-planned-unique-image-resources-latest-at-most <count>
                     Require latest planned unique image resources to be at most count
  --require-video-runtime-row
                     Require active video phases to record at least one video runtime row
  --visual-hold <sec>
                     Keep the applied video wallpaper visible before sampling/cleanup
  --simulate-power <state>
                     Start daemon with GILDER_POWER_STATE=ac|battery|unknown
  --simulate-output-state <state>
                     Start daemon with GILDER_OUTPUT_STATE=active|unfocused|fullscreen|hidden
  --measure-fullscreen-resume
                     Use a file-backed output-state override, switch fullscreen to active, and record resume latency
  --simulate-session <state>
                     Start daemon with GILDER_SESSION_STATE=active|inactive|locked
  --keep              Keep generated smoke data and logs
  -h, --help          Show this help text
EOF
}

work_parent="${TMPDIR:-/tmp}"
report_dir=""
output_name=""
all_outputs=0
expect_compositor=""
preflight=0
allow_missing=0
build=1
keep=0
sample_performance=0
sample_paused=0
sample_duration=8
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
expect_decoder_policy_status=""
expect_decoder_class=""
expect_memory_feature=""
expect_sink_memory_feature=""
expect_zero_copy_evidence=""
expect_video_position_progress=0
expect_gtk_frame_clock=0
expect_gtk_frame_clock_phases=()
expect_gtk_frame_timings=0
expect_renderer_video_pipeline_lifecycle=0
expect_render_sync_planned_image_resource_references_latest_at_most=""
expect_render_sync_planned_unique_image_resources_latest_at_most=""
require_video_runtime_row=0
visual_hold=0
simulate_power=""
simulate_output_state=""
simulate_session=""
measure_fullscreen_resume=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --output)
      [[ $# -ge 2 ]] || { echo "--output requires a value" >&2; exit 2; }
      output_name="$2"
      shift 2
      ;;
    --all-outputs)
      all_outputs=1
      shift
      ;;
    --expect-compositor)
      [[ $# -ge 2 ]] || { echo "--expect-compositor requires hyprland, niri, generic-wayland, or none" >&2; exit 2; }
      case "$2" in
        hyprland|niri|generic-wayland|generic|none)
          expect_compositor="$2"
          ;;
        *)
          echo "--expect-compositor requires hyprland, niri, generic-wayland, or none" >&2
          exit 2
          ;;
      esac
      if [[ "$expect_compositor" == "generic" ]]; then
        expect_compositor="generic-wayland"
      fi
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
    --preflight)
      preflight=1
      shift
      ;;
    --allow-missing)
      allow_missing=1
      shift
      ;;
    --no-build)
      build=0
      shift
      ;;
    --sample-performance)
      sample_performance=1
      shift
      ;;
    --sample-paused)
      sample_performance=1
      sample_paused=1
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
      sample_performance=1
      shift 2
      ;;
    --expect-max-pss-kib-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-max-pss-kib-at-most requires a value" >&2; exit 2; }
      expect_max_pss_kib_at_most="$2"
      sample_performance=1
      shift 2
      ;;
    --expect-max-private-kib-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-max-private-kib-at-most requires a value" >&2; exit 2; }
      expect_max_private_kib_at_most="$2"
      sample_performance=1
      shift 2
      ;;
    --expect-max-uss-kib-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-max-uss-kib-at-most requires a value" >&2; exit 2; }
      expect_max_uss_kib_at_most="$2"
      sample_performance=1
      shift 2
      ;;
    --expect-max-shared-kib-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-max-shared-kib-at-most requires a value" >&2; exit 2; }
      expect_max_shared_kib_at_most="$2"
      sample_performance=1
      shift 2
      ;;
    --expect-retained-private-delta-kib-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-retained-private-delta-kib-at-most requires a value" >&2; exit 2; }
      expect_retained_private_delta_kib_at_most="$2"
      sample_performance=1
      shift 2
      ;;
    --expect-retained-uss-delta-kib-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-retained-uss-delta-kib-at-most requires a value" >&2; exit 2; }
      expect_retained_uss_delta_kib_at_most="$2"
      sample_performance=1
      shift 2
      ;;
    --expect-retained-pss-delta-kib-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-retained-pss-delta-kib-at-most requires a value" >&2; exit 2; }
      expect_retained_pss_delta_kib_at_most="$2"
      sample_performance=1
      shift 2
      ;;
    --expect-peak-over-first-private-kib-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-peak-over-first-private-kib-at-most requires a value" >&2; exit 2; }
      expect_peak_over_first_private_kib_at_most="$2"
      sample_performance=1
      shift 2
      ;;
    --expect-peak-over-first-uss-kib-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-peak-over-first-uss-kib-at-most requires a value" >&2; exit 2; }
      expect_peak_over_first_uss_kib_at_most="$2"
      sample_performance=1
      shift 2
      ;;
    --expect-peak-over-first-pss-kib-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-peak-over-first-pss-kib-at-most requires a value" >&2; exit 2; }
      expect_peak_over_first_pss_kib_at_most="$2"
      sample_performance=1
      shift 2
      ;;
    --expect-decoder-policy-status)
      [[ $# -ge 2 ]] || { echo "--expect-decoder-policy-status requires a value" >&2; exit 2; }
      expect_decoder_policy_status="$2"
      sample_performance=1
      shift 2
      ;;
    --expect-decoder-class)
      [[ $# -ge 2 ]] || { echo "--expect-decoder-class requires hardware, software, or unknown" >&2; exit 2; }
      case "$2" in
        hardware|software|unknown)
          expect_decoder_class="$2"
          ;;
        *)
          echo "--expect-decoder-class requires hardware, software, or unknown" >&2
          exit 2
          ;;
      esac
      sample_performance=1
      shift 2
      ;;
    --expect-memory-feature)
      [[ $# -ge 2 ]] || { echo "--expect-memory-feature requires a value" >&2; exit 2; }
      expect_memory_feature="$2"
      sample_performance=1
      shift 2
      ;;
    --expect-sink-memory-feature)
      [[ $# -ge 2 ]] || { echo "--expect-sink-memory-feature requires a value" >&2; exit 2; }
      expect_sink_memory_feature="$2"
      sample_performance=1
      shift 2
      ;;
    --expect-zero-copy-evidence)
      [[ $# -ge 2 ]] || { echo "--expect-zero-copy-evidence requires a value" >&2; exit 2; }
      expect_zero_copy_evidence="$2"
      sample_performance=1
      shift 2
      ;;
    --expect-video-position-progress)
      expect_video_position_progress=1
      sample_performance=1
      shift
      ;;
    --expect-gtk-frame-clock)
      expect_gtk_frame_clock=1
      sample_performance=1
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
      sample_performance=1
      shift 2
      ;;
    --expect-gtk-frame-timings)
      expect_gtk_frame_timings=1
      sample_performance=1
      shift
      ;;
    --expect-renderer-video-pipeline-lifecycle)
      expect_renderer_video_pipeline_lifecycle=1
      sample_performance=1
      shift
      ;;
    --expect-render-sync-planned-image-resource-references-latest-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-render-sync-planned-image-resource-references-latest-at-most requires a value" >&2; exit 2; }
      expect_render_sync_planned_image_resource_references_latest_at_most="$2"
      sample_performance=1
      shift 2
      ;;
    --expect-render-sync-planned-unique-image-resources-latest-at-most)
      [[ $# -ge 2 ]] || { echo "--expect-render-sync-planned-unique-image-resources-latest-at-most requires a value" >&2; exit 2; }
      expect_render_sync_planned_unique_image_resources_latest_at_most="$2"
      sample_performance=1
      shift 2
      ;;
    --require-video-runtime-row)
      require_video_runtime_row=1
      shift
      ;;
    --visual-hold)
      [[ $# -ge 2 ]] || { echo "--visual-hold requires seconds" >&2; exit 2; }
      case "$2" in
        ''|*[!0-9]*)
          echo "--visual-hold requires a non-negative integer" >&2
          exit 2
          ;;
      esac
      visual_hold="$2"
      shift 2
      ;;
    --simulate-power)
      [[ $# -ge 2 ]] || { echo "--simulate-power requires ac, battery, or unknown" >&2; exit 2; }
      case "$2" in
        ac|battery|unknown)
          simulate_power="$2"
          ;;
        *)
          echo "--simulate-power requires ac, battery, or unknown" >&2
          exit 2
          ;;
      esac
      shift 2
      ;;
    --simulate-output-state)
      [[ $# -ge 2 ]] || { echo "--simulate-output-state requires active, unfocused, fullscreen, or hidden" >&2; exit 2; }
      case "$2" in
        active|unfocused|fullscreen|hidden)
          simulate_output_state="$2"
          ;;
        *)
          echo "--simulate-output-state requires active, unfocused, fullscreen, or hidden" >&2
          exit 2
          ;;
      esac
      shift 2
      ;;
    --measure-fullscreen-resume)
      measure_fullscreen_resume=1
      shift
      ;;
    --simulate-session)
      [[ $# -ge 2 ]] || { echo "--simulate-session requires active, inactive, or locked" >&2; exit 2; }
      case "$2" in
        active|inactive|locked)
          simulate_session="$2"
          ;;
        *)
          echo "--simulate-session requires active, inactive, or locked" >&2
          exit 2
          ;;
      esac
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

if [[ "$all_outputs" -eq 1 && -n "$output_name" ]]; then
  echo "--all-outputs cannot be combined with --output" >&2
  exit 2
fi
if [[ "$measure_fullscreen_resume" -eq 1 && -n "$simulate_output_state" ]]; then
  echo "--measure-fullscreen-resume cannot be combined with --simulate-output-state" >&2
  exit 2
fi
if [[ "$measure_fullscreen_resume" -eq 1 ]]; then
  simulate_output_state="fullscreen"
fi
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
  "$expect_render_sync_planned_image_resource_references_latest_at_most" \
  "$expect_render_sync_planned_unique_image_resources_latest_at_most"
do
  if [[ -n "$render_sync_resource_expectation" && ! "$render_sync_resource_expectation" =~ ^[0-9]+$ ]]; then
    echo "render sync resource expectations must be non-negative integers" >&2
    exit 2
  fi
done

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if [[ -n "$report_dir" ]]; then
  work_dir="$report_dir"
  mkdir -p "$work_dir"
else
  mkdir -p "$work_parent"
  work_dir="$(mktemp -d "${work_parent%/}/gilder-wayland-video.XXXXXX")"
fi
socket="$work_dir/runtime/gilder.sock"
daemon_log="$work_dir/gilderd.log"
status_before="$work_dir/status-before.json"
status_after="$work_dir/status-after.json"
status_paused="$work_dir/status-paused.json"
status_resumed="$work_dir/status-resumed.json"
wallpaper_dir="$work_dir/wallpaper.gwpdir"
source_dir="$work_dir/source"
output_state_override_file="$work_dir/output-state.override"
resume_latency_csv="$work_dir/fullscreen-resume-latency.csv"
resume_latency_summary="$work_dir/fullscreen-resume-latency.txt"
video_runtime_path="$work_dir/video-runtime.csv"
performance_active_dir="$work_dir/performance-active"
performance_active_log="$work_dir/performance-active.log"
performance_paused_dir="$work_dir/performance-paused"
performance_paused_log="$work_dir/performance-paused.log"
checks_path="$work_dir/checks.csv"
metadata_path="$work_dir/metadata.txt"
summary_path="$work_dir/summary.txt"
validation_report_path="$work_dir/validation-report.txt"
performance_active_label="wayland-video-active"
scenario_suffix=""
if [[ -n "$simulate_power" ]]; then
  scenario_suffix="$simulate_power"
fi
if [[ -n "$simulate_output_state" ]]; then
  if [[ -n "$scenario_suffix" ]]; then
    scenario_suffix="${scenario_suffix}-${simulate_output_state}"
  else
    scenario_suffix="$simulate_output_state"
  fi
fi
if [[ -n "$simulate_session" ]]; then
  if [[ -n "$scenario_suffix" ]]; then
    scenario_suffix="${scenario_suffix}-${simulate_session}"
  else
    scenario_suffix="$simulate_session"
  fi
fi
if [[ -n "$scenario_suffix" ]]; then
  performance_active_dir="$work_dir/performance-${scenario_suffix}"
  performance_active_log="$work_dir/performance-${scenario_suffix}.log"
  performance_active_label="wayland-video-${scenario_suffix}"
fi
if [[ "$measure_fullscreen_resume" -eq 1 ]]; then
  performance_active_dir="$work_dir/performance-resumed"
  performance_active_log="$work_dir/performance-resumed.log"
  performance_active_label="wayland-video-resumed"
fi
daemon_pid=""
actual_compositor="not-sampled"
target_outputs=()

cleanup() {
  if [[ -n "$daemon_pid" ]] && kill -0 "$daemon_pid" >/dev/null 2>&1; then
    kill "$daemon_pid" >/dev/null 2>&1 || true
    wait "$daemon_pid" >/dev/null 2>&1 || true
  fi
  if [[ "$keep" -eq 0 && -z "$report_dir" ]]; then
    rm -rf "$work_dir"
  else
    printf 'kept work dir: %s\n' "$work_dir"
  fi
}
trap cleanup EXIT

failures=0
skips=0
passes=0

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

record_check() {
  local kind="$1"
  local name="$2"
  local status="$3"
  local detail="$4"

  csv_escape "$kind" >> "$checks_path"
  printf ',' >> "$checks_path"
  csv_escape "$name" >> "$checks_path"
  printf ',' >> "$checks_path"
  csv_escape "$status" >> "$checks_path"
  printf ',' >> "$checks_path"
  csv_escape "$detail" >> "$checks_path"
  printf '\n' >> "$checks_path"
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
    record_check "command" "$command" "missing" "not found in PATH"
    skip_or_fail "$command is not available"
    return 1
  fi
  record_check "command" "$command" "available" "$(command -v "$command")"
  return 0
}

require_file() {
  local file="$1"
  if [[ ! -x "$file" ]]; then
    record_check "file" "$file" "missing" "not executable"
    skip_or_fail "missing executable $file"
    return 1
  fi
  record_check "file" "$file" "available" "executable"
  return 0
}

check_env_var() {
  local name="$1"
  local value="${!name:-}"
  if [[ -z "$value" ]]; then
    record_check "environment" "$name" "missing" "not set"
    skip_or_fail "$name is not set"
    return 1
  fi
  record_check "environment" "$name" "available" "$value"
  return 0
}

gst_element_available() {
  command -v gst-inspect-1.0 >/dev/null 2>&1 && gst-inspect-1.0 "$1" >/dev/null 2>&1
}

gst_element_hint() {
  case "$1" in
    gtk4paintablesink)
      printf '%s\n' "install gst-plugin-gtk4 on Arch-like systems; Ubuntu may need gst-plugin-gtk4 from a newer package source or source build"
      ;;
    qtdemux)
      printf '%s\n' "install the GStreamer good plugins package"
      ;;
    playbin)
      printf '%s\n' "install GStreamer playback plugins and tools"
      ;;
  esac
}

check_gst_element() {
  local role="$1"
  local element="$2"
  if gst_element_available "$element"; then
    record_check "gstreamer-${role}" "$element" "available" "gst-inspect-1.0 found element"
    return 0
  fi
  local detail="gst-inspect-1.0 did not find element"
  local hint
  hint="$(gst_element_hint "$element")"
  if [[ -n "$hint" ]]; then
    detail="${detail}; ${hint}"
  fi
  record_check "gstreamer-${role}" "$element" "missing" "$detail"
  skip_or_fail "GStreamer element ${element} is not available${hint:+; ${hint}}"
  return 1
}

check_any_gst_element() {
  local role="$1"
  shift
  local element
  local available=0
  for element in "$@"; do
    if gst_element_available "$element"; then
      available=1
      record_check "gstreamer-${role}-candidate" "$element" "available" "gst-inspect-1.0 found element"
    else
      record_check "gstreamer-${role}-candidate" "$element" "missing" "gst-inspect-1.0 did not find element"
    fi
  done
  if [[ "$available" -eq 1 ]]; then
    record_check "gstreamer-${role}" "$*" "available" "at least one candidate is available"
    return 0
  fi
  record_check "gstreamer-${role}" "$*" "missing" "no candidate element is available"
  skip_or_fail "no GStreamer ${role} candidate is available: $*"
  return 1
}

write_metadata() {
  cat > "$metadata_path" <<EOF
mode: $([[ "$preflight" -eq 1 ]] && printf 'preflight' || printf 'smoke')
output: ${output_name:-auto}
all_outputs: ${all_outputs}
build: ${build}
sample_performance: ${sample_performance}
sample_paused: ${sample_paused}
measure_fullscreen_resume: ${measure_fullscreen_resume}
expect_compositor: ${expect_compositor:-none}
actual_compositor: ${actual_compositor}
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
expect_decoder_policy_status: ${expect_decoder_policy_status:-none}
expect_decoder_class: ${expect_decoder_class:-none}
expect_memory_feature: ${expect_memory_feature:-none}
expect_sink_memory_feature: ${expect_sink_memory_feature:-none}
expect_zero_copy_evidence: ${expect_zero_copy_evidence:-none}
expect_video_position_progress: ${expect_video_position_progress}
expect_gtk_frame_clock: ${expect_gtk_frame_clock}
expect_gtk_frame_clock_phases: ${expect_gtk_frame_clock_phases[*]:-none}
expect_gtk_frame_timings: ${expect_gtk_frame_timings}
expect_renderer_video_pipeline_lifecycle: ${expect_renderer_video_pipeline_lifecycle}
expect_render_sync_planned_image_resource_references_latest_at_most: ${expect_render_sync_planned_image_resource_references_latest_at_most:-none}
expect_render_sync_planned_unique_image_resources_latest_at_most: ${expect_render_sync_planned_unique_image_resources_latest_at_most:-none}
require_video_runtime_row: ${require_video_runtime_row}
visual_hold: ${visual_hold}
simulate_power: ${simulate_power:-none}
simulate_output_state: ${simulate_output_state:-none}
simulate_session: ${simulate_session:-none}
wayland_display: ${WAYLAND_DISPLAY:-unset}
xdg_runtime_dir: ${XDG_RUNTIME_DIR:-unset}
checks: ${checks_path}
output_state_override_file: $([[ "$measure_fullscreen_resume" -eq 1 ]] && printf '%s' "$output_state_override_file" || printf 'none')
resume_latency_csv: $([[ "$measure_fullscreen_resume" -eq 1 ]] && printf '%s' "$resume_latency_csv" || printf 'none')
video_runtime_csv: ${video_runtime_path}
validation_report: ${validation_report_path}
EOF
}

write_summary() {
  cat > "$summary_path" <<EOF
passed: ${passes}
skipped: ${skips}
failed: ${failures}
expect_compositor: ${expect_compositor:-none}
actual_compositor: ${actual_compositor}
metadata: ${metadata_path}
checks: ${checks_path}
validation_report: ${validation_report_path}
fullscreen_resume_latency: $([[ "$measure_fullscreen_resume" -eq 1 ]] && printf '%s' "$resume_latency_summary" || printf 'none')
EOF
}

count_csv_data_rows() {
  local file="$1"
  if [[ ! -f "$file" ]]; then
    printf '0\n'
    return
  fi
  awk 'NR > 1 { count++ } END { print count + 0 }' "$file"
}

selected_outputs_text() {
  if [[ "${#target_outputs[@]}" -eq 0 ]]; then
    printf 'none\n'
    return
  fi
  printf '%s\n' "${target_outputs[*]}"
}

active_video_plan_expectation_text() {
  if [[ "$preflight" -eq 1 ]]; then
    printf 'not-run\n'
  elif expects_active_video_plan; then
    printf 'yes\n'
  else
    printf 'no\n'
  fi
}

performance_artifact_text() {
  local enabled="$1"
  local path="$2"
  local relative_summary="$3"
  if [[ "$enabled" -eq 1 ]]; then
    printf '%s\n' "${path}/${relative_summary}"
  else
    printf 'none\n'
  fi
}

summary_value_or_none() {
  local summary="$1"
  local key="$2"
  if [[ ! -f "$summary" ]]; then
    printf 'none\n'
    return
  fi
  awk -v key="$key" -F': ' '$1 == key { print $2; found = 1; exit } END { exit found ? 0 : 1 }' "$summary" \
    || printf 'none\n'
}

append_video_runtime_evidence_summary() {
  local prefix="$1"
  local summary="$2"
  local key

  if [[ ! -f "$summary" ]]; then
    printf '%s_video_runtime_summary_present: no\n' "$prefix"
    return
  fi

  printf '%s_video_runtime_summary_present: yes\n' "$prefix"
  for key in \
    video_runtime_rows \
    video_runtime_samples \
    video_runtime_outputs \
    video_mode_latest \
    video_gst_state_latest \
    video_decoder_policy_latest \
    video_decoder_policy_status_latest \
    video_actual_decoders_latest \
    video_decoder_classes_latest \
    video_caps_report_count_max \
    video_memory_features_latest \
    video_sink_memory_features_latest \
    video_zero_copy_evidence_latest \
    video_zero_copy_evidence_notes_latest \
    video_position_moving_outputs \
    video_position_delta_ms_max \
    video_frame_limiter_enabled_rows \
    video_frame_limiter_fps_min \
    video_frame_limiter_fps_max \
    video_qos_messages_max \
    video_qos_dropped_max \
    video_gtk_frame_clock_ticks_max \
    video_gtk_frame_clock_before_paint_ticks_max \
    video_gtk_frame_clock_update_ticks_max \
    video_gtk_frame_clock_layout_ticks_max \
    video_gtk_frame_clock_paint_ticks_max \
    video_gtk_frame_clock_after_paint_ticks_max \
    video_gtk_frame_timings_complete_max \
    video_gtk_frame_timings_presentation_interval_us_max
  do
    printf '%s_%s: %s\n' "$prefix" "$key" "$(summary_value_or_none "$summary" "$key")"
  done

  awk -v prefix="$prefix" -F': ' '
    $1 ~ /^video_decoder_policy_status\./ ||
    $1 ~ /^video_actual_decoder\./ ||
    $1 ~ /^video_decoder_class\./ ||
    $1 ~ /^video_memory_feature\./ ||
    $1 ~ /^video_sink_memory_feature\./ ||
    $1 ~ /^video_zero_copy_evidence\./ {
      printf "%s_%s: %s\n", prefix, $1, $2
    }
  ' "$summary"
}

append_process_memory_evidence_summary() {
  local prefix="$1"
  local summary="$2"
  local key

  if [[ ! -f "$summary" ]]; then
    printf '%s_process_summary_present: no\n' "$prefix"
    return
  fi

  printf '%s_process_summary_present: yes\n' "$prefix"
  for key in \
    samples \
    avg_cpu_percent \
    avg_rss_kib \
    max_rss_kib \
    first_rss_kib \
    last_rss_kib \
    retained_rss_delta_kib \
    peak_over_first_rss_kib \
    avg_pss_kib \
    max_pss_kib \
    first_pss_kib \
    last_pss_kib \
    retained_pss_delta_kib \
    peak_over_first_pss_kib \
    avg_private_kib \
    max_private_kib \
    first_private_kib \
    last_private_kib \
    retained_private_delta_kib \
    peak_over_first_private_kib \
    avg_uss_kib \
    max_uss_kib \
    first_uss_kib \
    last_uss_kib \
    retained_uss_delta_kib \
    peak_over_first_uss_kib \
    avg_shared_kib \
    max_shared_kib \
    first_shared_kib \
    last_shared_kib \
    retained_shared_delta_kib \
    peak_over_first_shared_kib \
    avg_gpu_busy_percent \
    max_gpu_busy_percent
  do
    printf '%s_%s: %s\n' "$prefix" "$key" "$(summary_value_or_none "$summary" "$key")"
  done
}

append_telemetry_evidence_summary() {
  local prefix="$1"
  local summary="$2"
  local key

  if [[ ! -f "$summary" ]]; then
    printf '%s_telemetry_summary_present: no\n' "$prefix"
    return
  fi

  printf '%s_telemetry_summary_present: yes\n' "$prefix"
  for key in \
    telemetry_rows \
    desktop_refresh_skips_delta \
    render_sync_cache_hits_delta \
    render_sync_cache_misses_delta \
    render_sync_updates_queued_latest \
    render_sync_updates_skipped_latest \
    render_sync_package_cache_entries_latest \
    render_sync_archive_cache_entries_latest \
    render_sync_planned_static_image_resources_latest \
    render_sync_planned_video_poster_resources_latest \
    render_sync_planned_slideshow_image_resources_latest \
    render_sync_planned_image_resource_references_latest \
    render_sync_planned_unique_image_resources_latest \
    renderer_output_windows_latest \
    renderer_output_windows_max \
    renderer_static_surfaces_latest \
    renderer_static_surfaces_max \
    renderer_slideshow_surfaces_latest \
    renderer_slideshow_surfaces_max \
    renderer_video_surfaces_latest \
    renderer_video_surfaces_max \
    renderer_video_pipelines_latest \
    renderer_video_pipelines_max
  do
    printf '%s_%s: %s\n' "$prefix" "$key" "$(summary_value_or_none "$summary" "$key")"
  done
}

write_validation_report() {
  local active_summary
  local active_telemetry_summary
  local active_video_runtime_summary
  local paused_summary
  local paused_telemetry_summary
  local paused_video_runtime_summary
  active_summary="$(performance_artifact_text "$sample_performance" "$performance_active_dir" "summary.txt")"
  active_telemetry_summary="$(performance_artifact_text "$sample_performance" "$performance_active_dir" "telemetry-summary.txt")"
  active_video_runtime_summary="$(performance_artifact_text "$sample_performance" "$performance_active_dir" "video-runtime-summary.txt")"
  paused_summary="$(performance_artifact_text "$sample_paused" "$performance_paused_dir" "summary.txt")"
  paused_telemetry_summary="$(performance_artifact_text "$sample_paused" "$performance_paused_dir" "telemetry-summary.txt")"
  paused_video_runtime_summary="$(performance_artifact_text "$sample_paused" "$performance_paused_dir" "video-runtime-summary.txt")"

  cat > "$validation_report_path" <<EOF
validation: wayland-video-surface-smoke
mode: $([[ "$preflight" -eq 1 ]] && printf 'preflight' || printf 'smoke')
result_passed: ${passes}
result_skipped: ${skips}
result_failed: ${failures}
expect_compositor: ${expect_compositor:-none}
actual_compositor: ${actual_compositor}
output_request: ${output_name:-auto}
all_outputs: ${all_outputs}
selected_outputs: $(selected_outputs_text)
expects_active_video_plan: $(active_video_plan_expectation_text)
sample_performance: ${sample_performance}
sample_paused: ${sample_paused}
measure_fullscreen_resume: ${measure_fullscreen_resume}
require_video_runtime_row: ${require_video_runtime_row}
expect_renderer_video_pipeline_lifecycle: ${expect_renderer_video_pipeline_lifecycle}
expect_gtk_frame_clock_phases: ${expect_gtk_frame_clock_phases[*]:-none}
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
expect_render_sync_planned_image_resource_references_latest_at_most: ${expect_render_sync_planned_image_resource_references_latest_at_most:-none}
expect_render_sync_planned_unique_image_resources_latest_at_most: ${expect_render_sync_planned_unique_image_resources_latest_at_most:-none}
zero_copy_audit_note: hardware decoder evidence alone is not zero-copy proof; prefer sink DMABuf/GLMemory caps plus compositor presentation evidence
video_runtime_rows: $(count_csv_data_rows "$video_runtime_path")
video_runtime_csv: ${video_runtime_path}
performance_active_summary: ${active_summary}
performance_active_telemetry_summary: ${active_telemetry_summary}
performance_active_video_runtime_summary: ${active_video_runtime_summary}
performance_paused_summary: ${paused_summary}
performance_paused_telemetry_summary: ${paused_telemetry_summary}
performance_paused_video_runtime_summary: ${paused_video_runtime_summary}
fullscreen_resume_latency: $([[ "$measure_fullscreen_resume" -eq 1 ]] && printf '%s' "$resume_latency_summary" || printf 'none')
metadata: ${metadata_path}
checks: ${checks_path}
summary: ${summary_path}
status_before: $([[ -f "$status_before" ]] && printf '%s' "$status_before" || printf 'none')
status_after: $([[ -f "$status_after" ]] && printf '%s' "$status_after" || printf 'none')
status_paused: $([[ -f "$status_paused" ]] && printf '%s' "$status_paused" || printf 'none')
status_resumed: $([[ -f "$status_resumed" ]] && printf '%s' "$status_resumed" || printf 'none')
daemon_log: ${daemon_log}
EOF

  append_process_memory_evidence_summary "performance_active" "$active_summary" >> "$validation_report_path"
  append_process_memory_evidence_summary "performance_paused" "$paused_summary" >> "$validation_report_path"
  append_telemetry_evidence_summary "performance_active" "$active_telemetry_summary" >> "$validation_report_path"
  append_telemetry_evidence_summary "performance_paused" "$paused_telemetry_summary" >> "$validation_report_path"
  append_video_runtime_evidence_summary "performance_active" "$active_video_runtime_summary" >> "$validation_report_path"
  append_video_runtime_evidence_summary "performance_paused" "$paused_video_runtime_summary" >> "$validation_report_path"

  if [[ -f "$video_runtime_path" ]]; then
    awk -F, '
      NR > 1 && $1 != "" { count[$1]++ }
      END {
        for (phase in count) {
          printf "video_runtime_phase.%s: %d\n", phase, count[phase]
        }
      }
    ' "$video_runtime_path" | sort >> "$validation_report_path"
  fi
}

finish_with_summary() {
  write_summary
  write_validation_report
  note "metadata: $metadata_path"
  note "checks:   $checks_path"
  note "validation report: $validation_report_path"
  note "report:   $summary_path"
  note "summary: ${passes} passed, ${skips} skipped, ${failures} failed"
  exit "$([[ "$failures" -gt 0 ]] && echo 1 || echo 0)"
}

has_video_runtime_expectations() {
  [[ -n "$expect_decoder_policy_status" ||
    -n "$expect_decoder_class" ||
    -n "$expect_memory_feature" ||
    -n "$expect_sink_memory_feature" ||
    -n "$expect_zero_copy_evidence" ||
    "$expect_video_position_progress" -eq 1 ||
    "$expect_gtk_frame_clock" -eq 1 ||
    "${#expect_gtk_frame_clock_phases[@]}" -gt 0 ||
    "$expect_gtk_frame_timings" -eq 1 ]]
}

append_video_runtime_expectations() {
  local -n args_ref="$1"
  if [[ -n "$expect_decoder_policy_status" ]]; then
    args_ref+=(--expect-decoder-policy-status "$expect_decoder_policy_status")
  fi
  if [[ -n "$expect_decoder_class" ]]; then
    args_ref+=(--expect-decoder-class "$expect_decoder_class")
  fi
  if [[ -n "$expect_memory_feature" ]]; then
    args_ref+=(--expect-memory-feature "$expect_memory_feature")
  fi
  if [[ -n "$expect_sink_memory_feature" ]]; then
    args_ref+=(--expect-sink-memory-feature "$expect_sink_memory_feature")
  fi
  if [[ -n "$expect_zero_copy_evidence" ]]; then
    args_ref+=(--expect-zero-copy-evidence "$expect_zero_copy_evidence")
  fi
  if [[ "$expect_video_position_progress" -eq 1 ]]; then
    args_ref+=(--expect-video-position-progress)
  fi
  if [[ "$expect_gtk_frame_clock" -eq 1 ]]; then
    args_ref+=(--expect-gtk-frame-clock)
  fi
  local phase
  for phase in "${expect_gtk_frame_clock_phases[@]}"; do
    args_ref+=(--expect-gtk-frame-clock-phase "$phase")
  done
  if [[ "$expect_gtk_frame_timings" -eq 1 ]]; then
    args_ref+=(--expect-gtk-frame-timings)
  fi
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

capture_performance() {
  local label="$1"
  local output_dir="$2"
  local log_file="$3"
  local expected_mode="${4:-}"
  local expected_reason="${5:-}"
  local expected_action="${6:-}"
  local expected_plan_kind="${7:-}"
  local include_video_runtime_expectations="${8:-0}"
  local expected_pipeline_latest_at_least="${9:-}"
  local expected_pipeline_latest_at_most="${10:-}"
  local expected_planned_image_references="${11:-}"
  local expected_planned_unique_resources="${12:-}"
  local -a sample_args
  sample_args=(
    --pid "$daemon_pid"
    --socket "$socket"
    --gilderctl "$gilderctl"
    --label "$label"
    --duration "$sample_duration"
    --interval "$sample_interval"
    --output-dir "$output_dir"
    --keep
    --expect-render-sync-cache-hit
    --expect-desktop-refresh-skip
    --expect-render-sync-update-queued
  )
  append_process_memory_expectations sample_args
  append_render_sync_resource_expectations \
    sample_args \
    "$expected_planned_image_references" \
    "$expected_planned_unique_resources"
  if [[ -n "$expected_mode" ]]; then
    sample_args+=(--expect-mode "$expected_mode")
  fi
  if [[ -n "$expected_reason" ]]; then
    sample_args+=(--expect-reason "$expected_reason")
  fi
  if [[ -n "$expected_action" ]]; then
    sample_args+=(--expect-action "$expected_action")
  fi
  if [[ -n "$expected_plan_kind" ]]; then
    sample_args+=(--expect-plan-kind "$expected_plan_kind")
  fi
  if [[ "$include_video_runtime_expectations" -eq 1 ]]; then
    append_video_runtime_expectations sample_args
  fi
  if [[ -n "$expected_pipeline_latest_at_least" ]]; then
    sample_args+=(--expect-renderer-output-windows-latest-at-least "$expected_pipeline_latest_at_least")
    sample_args+=(--expect-renderer-video-surfaces-latest-at-least "$expected_pipeline_latest_at_least")
    sample_args+=(--expect-renderer-video-pipelines-latest-at-least "$expected_pipeline_latest_at_least")
  fi
  if [[ -n "$expected_pipeline_latest_at_most" ]]; then
    sample_args+=(--expect-renderer-output-windows-latest-at-most "$expected_pipeline_latest_at_most")
    sample_args+=(--expect-renderer-static-surfaces-latest-at-most "$expected_pipeline_latest_at_most")
    sample_args+=(--expect-renderer-slideshow-surfaces-latest-at-most "$expected_pipeline_latest_at_most")
    sample_args+=(--expect-renderer-video-surfaces-latest-at-most "$expected_pipeline_latest_at_most")
    if [[ "$expected_pipeline_latest_at_most" == "0" ]]; then
      sample_args+=(--expect-renderer-output-windows-max-at-most 0)
      sample_args+=(--expect-renderer-static-surfaces-max-at-most 0)
      sample_args+=(--expect-renderer-slideshow-surfaces-max-at-most 0)
      sample_args+=(--expect-renderer-video-surfaces-max-at-most 0)
    fi
    sample_args+=(--expect-renderer-video-pipelines-latest-at-most "$expected_pipeline_latest_at_most")
  fi
  if [[ "$allow_missing" -eq 1 ]]; then
    sample_args+=(--allow-missing)
  fi
  "$performance_snapshot" "${sample_args[@]}" > "$log_file" 2>&1
}

expected_performance_reason() {
  if [[ "$simulate_session" == "inactive" ]]; then
    printf '%s\n' "session-inactive"
  elif [[ "$simulate_session" == "locked" ]]; then
    printf '%s\n' "session-locked"
  elif [[ "$simulate_output_state" == "hidden" ]]; then
    printf '%s\n' "output-hidden"
  elif [[ "$simulate_output_state" == "fullscreen" ]]; then
    printf '%s\n' "fullscreen"
  elif [[ "$simulate_power" == "battery" ]]; then
    printf '%s\n' "battery"
  elif [[ "$simulate_output_state" == "unfocused" ]]; then
    printf '%s\n' "unfocused"
  else
    printf '%s\n' ""
  fi
}

expects_active_video_plan() {
  [[ "$simulate_session" != "inactive" && "$simulate_session" != "locked" && "$simulate_output_state" != "fullscreen" && "$simulate_output_state" != "hidden" ]]
}

expected_mode_for_reason() {
  case "$1" in
    battery|unfocused)
      printf '%s\n' "throttled"
      ;;
    fullscreen|output-hidden|user-paused|session-inactive|session-locked)
      printf '%s\n' "paused"
      ;;
    interactive)
      printf '%s\n' "active"
      ;;
    *)
      printf '%s\n' ""
      ;;
  esac
}

extract_desktop_output_names() {
  local status_file="$1"
  local desktop_outputs
  desktop_outputs="$(sed -n 's/.*"desktop":{"compositor":"[^"]*","outputs":\[\(.*\)\],"power".*/\1/p' "$status_file")"
  if [[ -z "$desktop_outputs" ]]; then
    return 0
  fi
  printf '%s\n' "$desktop_outputs" | grep -o '"name":"[^"]*"' | cut -d '"' -f 4 || true
}

extract_desktop_compositor() {
  local status_file="$1"
  local raw
  raw="$(grep -o '"compositor":[^,}]*' "$status_file" | head -n 1 | cut -d ':' -f 2- || true)"
  case "$raw" in
    \"*\")
      raw="${raw#\"}"
      raw="${raw%\"}"
      ;;
    null|"")
      raw="none"
      ;;
  esac
  printf '%s\n' "$raw"
}

check_expected_compositor() {
  local status_file="$1"
  actual_compositor="$(extract_desktop_compositor "$status_file")"
  record_check "desktop-compositor" "actual" "observed" "$actual_compositor"
  write_metadata

  if [[ -z "$expect_compositor" ]]; then
    pass "status reports compositor ${actual_compositor}"
    return 0
  fi

  if [[ "$actual_compositor" == "$expect_compositor" ]]; then
    record_check "desktop-compositor" "$expect_compositor" "matched" "actual ${actual_compositor}"
    pass "status reports expected compositor ${expect_compositor}"
  else
    record_check "desktop-compositor" "$expect_compositor" "mismatch" "actual ${actual_compositor}"
    skip_or_fail "status reports compositor ${actual_compositor}, expected ${expect_compositor}"
  fi
}

status_has_video_plan_for_output() {
  local status_file="$1"
  local output="$2"
  local video_plans
  video_plans="$(sed -n 's/.*"video_plans":\[\(.*\)\]},"renderer".*/\1/p' "$status_file")"
  [[ -n "$video_plans" ]] && grep -Fq "\"output_name\":\"${output}\"" <<< "$video_plans"
}

now_millis() {
  date +%s%3N
}

write_resume_latency_summary() {
  local status="$1"
  local latency_ms="$2"
  local status_file="$3"
  cat > "$resume_latency_summary" <<EOF
status: ${status}
latency_ms: ${latency_ms}
from_state: fullscreen
to_state: active
output: ${output_name}
status_file: ${status_file}
csv: ${resume_latency_csv}
EOF
}

measure_fullscreen_resume_latency() {
  local start_ms
  local end_ms
  local latency_ms
  local attempt
  local status_file="$status_resumed"

  printf 'from_state,to_state,output,start_ms,end_ms,latency_ms,status_file\n' > "$resume_latency_csv"
  printf 'active\n' > "$output_state_override_file"
  start_ms="$(now_millis)"

  for attempt in $(seq 1 100); do
    env GILDER_SOCKET="$socket" "$gilderctl" status > "$status_file"
    if status_has_video_plan_for_output "$status_file" "$output_name" \
      && grep -q '"reason":"interactive"' "$status_file"; then
      end_ms="$(now_millis)"
      latency_ms=$((end_ms - start_ms))
      printf 'fullscreen,active,%s,%s,%s,%s,%s\n' \
        "$output_name" \
        "$start_ms" \
        "$end_ms" \
        "$latency_ms" \
        "${status_file#$work_dir/}" >> "$resume_latency_csv"
      write_resume_latency_summary "ok" "$latency_ms" "$status_file"
      return 0
    fi
    sleep 0.1
  done

  end_ms="$(now_millis)"
  latency_ms=$((end_ms - start_ms))
  printf 'fullscreen,active,%s,%s,%s,%s,%s\n' \
    "$output_name" \
    "$start_ms" \
    "$end_ms" \
    "$latency_ms" \
    "${status_file#$work_dir/}" >> "$resume_latency_csv"
  write_resume_latency_summary "timeout" "$latency_ms" "$status_file"
  return 1
}

append_video_runtime_evidence() {
  local phase="$1"
  local status_file="$2"
  local require_row="${3:-0}"
  local temp_video_runtime="$work_dir/video-runtime-${phase}.tmp"
  local video_runtime_error_file="$work_dir/video-runtime-${phase}.err"
  local row_count

  if ! "$gilderctl" status --video-runtime-csv --from-file "$status_file" > "$temp_video_runtime" 2> "$video_runtime_error_file"; then
    rm -f "$temp_video_runtime"
    skip_or_fail "failed to extract video runtime evidence for ${phase}"
    return 1
  fi
  if [[ ! -s "$video_runtime_error_file" ]]; then
    rm -f "$video_runtime_error_file"
  fi
  row_count="$(count_csv_data_rows "$temp_video_runtime")"

  awk -v phase="$phase" '
    NR == 1 { next }
    {
      print phase "," $0
    }
  ' "$temp_video_runtime" >> "$video_runtime_path"
  rm -f "$temp_video_runtime"
  if [[ "$row_count" -gt 0 ]]; then
    record_check "video-runtime" "$phase" "observed" "${row_count} row(s)"
    pass "recorded video runtime evidence for ${phase} (${row_count} row(s))"
    return 0
  fi

  record_check "video-runtime" "$phase" "missing" "0 rows"
  if [[ "$require_row" -eq 1 ]]; then
    skip_or_fail "video runtime evidence for ${phase} has no rows"
    return 1
  fi
  note "video runtime evidence for ${phase} has no rows"
  return 0
}

printf 'kind,name,status,detail\n' > "$checks_path"
write_metadata

check_env_var WAYLAND_DISPLAY || true
check_env_var XDG_RUNTIME_DIR || true
require_command ffmpeg || true
require_command gst-inspect-1.0 || true
if command -v gst-inspect-1.0 >/dev/null 2>&1; then
  check_gst_element "playback" playbin || true
  check_gst_element "paintable-sink" gtk4paintablesink || true
  check_gst_element "mp4-demuxer" qtdemux || true
  check_any_gst_element "h264-decoder" avdec_h264 openh264dec || true
fi
if [[ "$build" -eq 1 ]]; then
  require_command cargo || true
fi

if [[ "$failures" -gt 0 || "$skips" -gt 0 ]]; then
  finish_with_summary
fi

if [[ "$build" -eq 1 ]]; then
  cargo build --features gtk-renderer,video-renderer
fi

gilderd="target/debug/gilderd"
gilderctl="target/debug/gilderctl"
gilder_convert="target/debug/gilder-convert"
performance_snapshot="scripts/performance-snapshot.sh"
require_file "$gilderd" || true
require_file "$gilderctl" || true
require_file "$gilder_convert" || true
if [[ "$sample_performance" -eq 1 ]]; then
  require_file "$performance_snapshot" || true
fi
if [[ "$failures" -gt 0 || "$skips" -gt 0 ]]; then
  finish_with_summary
fi

if [[ "$preflight" -eq 1 ]]; then
  pass "preflight checks passed"
  finish_with_summary
fi

printf 'phase,output_name,mode,gst_state,decoder_policy,decoder_policy_status,actual_decoders,decoder_classes,caps_report_count,memory_features,sink_memory_features,zero_copy_evidence_level,zero_copy_evidence_notes,media_types,caps_paths,position_ms,duration_ms,frame_limiter_enabled,frame_limiter_max_fps,qos_messages,qos_processed_max,qos_dropped_max,qos_stats_format,qos_jitter_ns_latest,qos_jitter_ns_abs_max,qos_proportion_x1000_latest,gtk_frame_clock_ticks,gtk_frame_clock_counter_latest,gtk_frame_clock_time_us_latest,gtk_frame_clock_interval_us_latest,gtk_frame_clock_interval_us_max,gtk_frame_clock_fps_x1000_latest,gtk_frame_clock_refresh_interval_us_latest,gtk_frame_clock_predicted_presentation_time_us_latest,gtk_frame_timings_observed,gtk_frame_timings_complete,gtk_frame_timings_counter_latest,gtk_frame_timings_complete_counter_latest,gtk_frame_timings_frame_time_us_latest,gtk_frame_timings_predicted_presentation_time_us_latest,gtk_frame_timings_presentation_time_us_latest,gtk_frame_timings_presentation_interval_us_latest,gtk_frame_timings_presentation_interval_us_max,gtk_frame_timings_refresh_interval_us_latest,source,gtk_frame_clock_before_paint_ticks,gtk_frame_clock_update_ticks,gtk_frame_clock_layout_ticks,gtk_frame_clock_paint_ticks,gtk_frame_clock_after_paint_ticks\n' > "$video_runtime_path"

mkdir -p "$work_dir/runtime" "$work_dir/config" "$work_dir/state" "$work_dir/cache" "$source_dir"
if [[ "$measure_fullscreen_resume" -eq 1 ]]; then
  printf 'fullscreen\n' > "$output_state_override_file"
fi
ffmpeg -hide_banner -loglevel error -y \
  -f lavfi -i testsrc2=size=128x72:rate=12:duration=2 \
  -an -c:v libx264 -preset ultrafast -tune zerolatency -pix_fmt yuv420p \
  "$source_dir/loop.mp4"
cat > "$source_dir/project.json" <<'EOF'
{
  "type": "video",
  "title": "Gilder Wayland Video Smoke",
  "file": "loop.mp4"
}
EOF
"$gilder_convert" wallpaper-engine "$source_dir" "$wallpaper_dir" >/dev/null
pass "generated video wallpaper package"

daemon_env=(
  env
  GILDER_SOCKET="$socket"
  XDG_CONFIG_HOME="$work_dir/config"
  XDG_STATE_HOME="$work_dir/state"
  XDG_CACHE_HOME="$work_dir/cache"
)
if [[ -n "$simulate_power" ]]; then
  daemon_env+=(GILDER_POWER_STATE="$simulate_power")
fi
if [[ -n "$simulate_output_state" ]]; then
  if [[ "$measure_fullscreen_resume" -eq 1 ]]; then
    daemon_env+=(GILDER_OUTPUT_STATE_FILE="$output_state_override_file")
  else
    daemon_env+=(GILDER_OUTPUT_STATE="$simulate_output_state")
  fi
fi
if [[ -n "$simulate_session" ]]; then
  daemon_env+=(GILDER_SESSION_STATE="$simulate_session")
fi
"${daemon_env[@]}" "$gilderd" >"$daemon_log" 2>&1 &
daemon_pid=$!

for _ in $(seq 1 80); do
  if [[ -S "$socket" ]]; then
    break
  fi
  if ! kill -0 "$daemon_pid" >/dev/null 2>&1; then
    note "daemon log:"
    sed -n '1,120p' "$daemon_log"
    skip_or_fail "gilderd exited before creating IPC socket"
    finish_with_summary
  fi
  sleep 0.1
done
[[ -S "$socket" ]] || {
  note "daemon log:"
  sed -n '1,120p' "$daemon_log"
  skip_or_fail "gilderd did not create IPC socket"
  finish_with_summary
}
pass "started isolated gilderd"

env GILDER_SOCKET="$socket" "$gilderctl" status > "$status_before"
check_expected_compositor "$status_before"

if [[ -n "$simulate_power" ]]; then
  if grep -q "\"power\":\"${simulate_power}\"" "$status_before"; then
    pass "status reports simulated power state ${simulate_power}"
  else
    skip_or_fail "status does not report simulated power state ${simulate_power}"
  fi
fi

if [[ -n "$simulate_session" ]]; then
  case "$simulate_session" in
    active)
      session_pattern='"session_active":true.*"session_locked":false|"session_locked":false.*"session_active":true'
      ;;
    inactive)
      session_pattern='"session_active":false.*"session_locked":false|"session_locked":false.*"session_active":false'
      ;;
    locked)
      session_pattern='"session_active":true.*"session_locked":true|"session_locked":true.*"session_active":true'
      ;;
  esac
  if grep -Eq "$session_pattern" "$status_before"; then
    pass "status reports simulated session state ${simulate_session}"
  else
    skip_or_fail "status does not report simulated session state ${simulate_session}"
  fi
fi

target_outputs=()
if [[ "$all_outputs" -eq 1 ]]; then
  mapfile -t target_outputs < <(extract_desktop_output_names "$status_before")
else
  if [[ -z "$output_name" ]]; then
    output_name="$(extract_desktop_output_names "$status_before" | head -n 1 || true)"
  fi
  if [[ -n "$output_name" ]]; then
    target_outputs=("$output_name")
  fi
fi
if [[ "${#target_outputs[@]}" -eq 0 ]]; then
  skip_or_fail "daemon reported no output; pass --output <name> if compositor adapters are disabled"
  note "status evidence: $status_before"
  note "daemon log: $daemon_log"
  finish_with_summary
fi
output_name="${target_outputs[0]}"
if [[ "$all_outputs" -eq 1 ]]; then
  pass "selected ${#target_outputs[@]} outputs: ${target_outputs[*]}"
else
  pass "selected output $output_name"
fi

if [[ -n "$simulate_output_state" ]]; then
  case "$simulate_output_state" in
    active)
      state_pattern='"focused":true.*"visible":true.*"has_fullscreen":false|"has_fullscreen":false.*"focused":true.*"visible":true'
      ;;
    unfocused)
      state_pattern='"focused":false.*"visible":true.*"has_fullscreen":false|"has_fullscreen":false.*"focused":false.*"visible":true'
      ;;
    fullscreen)
      state_pattern='"focused":true.*"visible":true.*"has_fullscreen":true|"has_fullscreen":true.*"focused":true.*"visible":true'
      ;;
    hidden)
      state_pattern='"focused":false.*"visible":false.*"has_fullscreen":false|"has_fullscreen":false.*"focused":false.*"visible":false'
      ;;
  esac
  if grep -Eq "$state_pattern" "$status_before"; then
    pass "status reports simulated output state ${simulate_output_state}"
  else
    skip_or_fail "status does not report simulated output state ${simulate_output_state}"
  fi
fi

if ! grep -Eq '"name":"gtk4paintablesink","available":true|"available":true,"name":"gtk4paintablesink"' "$status_before"; then
  skip_or_fail "gtk4paintablesink is not available according to renderer_capabilities"
  note "status evidence: $status_before"
  note "daemon log: $daemon_log"
  finish_with_summary
fi
pass "gtk4paintablesink is available"

for target_output in "${target_outputs[@]}"; do
  env GILDER_SOCKET="$socket" "$gilderctl" set "$wallpaper_dir" --output "$target_output" >/dev/null
done
if [[ "$all_outputs" -eq 1 ]]; then
  pass "applied video wallpaper to ${#target_outputs[@]} outputs"
fi
sleep 2
env GILDER_SOCKET="$socket" "$gilderctl" status > "$status_after"
require_after_set_runtime=0
if [[ "$require_video_runtime_row" -eq 1 ]] && expects_active_video_plan; then
  require_after_set_runtime=1
fi
append_video_runtime_evidence "after-set" "$status_after" "$require_after_set_runtime" || true

if expects_active_video_plan; then
  missing_video_plan=0
  for target_output in "${target_outputs[@]}"; do
    if ! status_has_video_plan_for_output "$status_after" "$target_output"; then
      missing_video_plan=1
      skip_or_fail "status does not report an active video render plan for ${target_output}"
    fi
  done
  if [[ "$missing_video_plan" -eq 0 ]]; then
    pass "status reports active video render plan"
  fi
else
  if grep -q '"video_plans":\[\]' "$status_after"; then
    pass "status omits active video plan for paused simulated output state"
  else
    skip_or_fail "status reports video plans for paused simulated output state"
  fi
fi
expected_reason="$(expected_performance_reason)"
if [[ -n "$expected_reason" ]]; then
  if grep -q "\"reason\":\"${expected_reason}\"" "$status_after"; then
    pass "status reports ${expected_reason} performance decision"
  else
    skip_or_fail "status does not report ${expected_reason} performance decision"
  fi
fi

if [[ "$visual_hold" -gt 0 ]]; then
  if [[ "$all_outputs" -eq 1 ]]; then
    note "visual hold: outputs '${target_outputs[*]}' should show the generated moving test video for ${visual_hold}s"
  else
    note "visual hold: output '$output_name' should show the generated moving test video for ${visual_hold}s"
  fi
  sleep "$visual_hold"
  pass "held video wallpaper for visual confirmation window"
fi

if [[ "$measure_fullscreen_resume" -eq 1 ]]; then
  if measure_fullscreen_resume_latency; then
    pass "measured fullscreen resume latency"
    append_video_runtime_evidence "fullscreen-resumed" "$status_resumed" "$require_video_runtime_row" || true
  else
    note "resume latency summary:"
    sed -n '1,120p' "$resume_latency_summary"
    skip_or_fail "fullscreen resume latency measurement timed out"
  fi
  simulate_output_state=""
  expected_reason=""
fi

if [[ "$sample_performance" -eq 1 ]]; then
  expected_mode="$(expected_mode_for_reason "$expected_reason")"
  expected_action=""
  expected_plan_kind=""
  include_video_runtime_expectations=0
  expected_pipeline_latest_at_least=""
  expected_pipeline_latest_at_most=""
  expected_planned_image_references=""
  expected_planned_unique_resources=""
  if [[ -n "$expected_reason" ]]; then
    if expects_active_video_plan; then
      expected_action="render"
      expected_plan_kind="video"
    else
      expected_action="remove"
    fi
  fi
  if [[ "$expect_renderer_video_pipeline_lifecycle" -eq 1 ]]; then
    if expects_active_video_plan; then
      expected_pipeline_latest_at_least="${#target_outputs[@]}"
      expected_planned_image_references="${#target_outputs[@]}"
      expected_planned_unique_resources=1
    else
      expected_pipeline_latest_at_most=0
      expected_planned_image_references=0
      expected_planned_unique_resources=0
    fi
  fi
  if has_video_runtime_expectations; then
    if expects_active_video_plan; then
      include_video_runtime_expectations=1
    else
      skip_or_fail "video runtime expectations require an active video plan in the sampled scenario"
    fi
  fi
  if capture_performance \
    "$performance_active_label" \
    "$performance_active_dir" \
    "$performance_active_log" \
    "$expected_mode" \
    "$expected_reason" \
    "$expected_action" \
    "$expected_plan_kind" \
    "$include_video_runtime_expectations" \
    "$expected_pipeline_latest_at_least" \
    "$expected_pipeline_latest_at_most" \
    "$expected_planned_image_references" \
    "$expected_planned_unique_resources"; then
    pass "captured ${performance_active_label} performance evidence"
  else
    note "performance sample log:"
    sed -n '1,120p' "$performance_active_log"
    skip_or_fail "performance sampling failed"
  fi
fi

if [[ "$sample_paused" -eq 1 ]]; then
  env GILDER_SOCKET="$socket" "$gilderctl" pause --output "$output_name" >/dev/null
  sleep 2
  env GILDER_SOCKET="$socket" "$gilderctl" status > "$status_paused"
  append_video_runtime_evidence "paused" "$status_paused" 0 || true
  if grep -q '"reason":"user-paused"' "$status_paused"; then
    pass "status reports user-paused decision"
  else
    skip_or_fail "status does not report user-paused decision after pause"
  fi

  paused_pipeline_latest_at_most=""
  paused_planned_image_references=""
  paused_planned_unique_resources=""
  if [[ "$expect_renderer_video_pipeline_lifecycle" -eq 1 ]]; then
    paused_pipeline_latest_at_most=0
    paused_planned_image_references=0
    paused_planned_unique_resources=0
  fi
  if capture_performance \
    wayland-video-paused \
    "$performance_paused_dir" \
    "$performance_paused_log" \
    paused \
    user-paused \
    remove \
    "" \
    0 \
    "" \
    "$paused_pipeline_latest_at_most" \
    "$paused_planned_image_references" \
    "$paused_planned_unique_resources"; then
    pass "captured paused video performance evidence"
  else
    note "paused performance sample log:"
    sed -n '1,120p' "$performance_paused_log"
    skip_or_fail "paused performance sampling failed"
  fi

  env GILDER_SOCKET="$socket" "$gilderctl" resume --output "$output_name" >/dev/null
  sleep 1
  env GILDER_SOCKET="$socket" "$gilderctl" status > "$status_resumed"
  append_video_runtime_evidence "resumed" "$status_resumed" "$require_video_runtime_row" || true
  if grep -q '"reason":"user-paused"' "$status_resumed"; then
    skip_or_fail "status still reports user-paused after resume"
  else
    pass "resumed output after paused performance sample"
  fi
fi

if ! kill -0 "$daemon_pid" >/dev/null 2>&1; then
  skip_or_fail "gilderd exited during video surface smoke"
else
  pass "gilderd remained running after applying video wallpaper"
fi

note "status before: $status_before"
note "status after:  $status_after"
note "video runtime: $video_runtime_path"
if [[ "$sample_paused" -eq 1 ]]; then
  note "status paused: $status_paused"
  note "status resumed: $status_resumed"
fi
if [[ "$measure_fullscreen_resume" -eq 1 ]]; then
  note "fullscreen resume latency: $resume_latency_summary"
  note "fullscreen resume latency csv: $resume_latency_csv"
fi
note "daemon log:    $daemon_log"
if [[ "$sample_performance" -eq 1 ]]; then
  note "performance ${performance_active_label}: $performance_active_dir"
  note "performance ${performance_active_label} log: $performance_active_log"
fi
if [[ "$sample_paused" -eq 1 ]]; then
  note "performance paused: $performance_paused_dir"
  note "performance paused log: $performance_paused_log"
fi
if [[ "$all_outputs" -eq 1 ]]; then
  note "Visually confirm that outputs '${target_outputs[*]}' show the generated moving test video."
else
  note "Visually confirm that output '$output_name' shows the generated moving test video."
fi
finish_with_summary
