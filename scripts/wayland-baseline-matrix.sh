#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
usage: scripts/wayland-baseline-matrix.sh [options]

Run a real Wayland GTK/video renderer baseline matrix and aggregate per-scenario
CPU, GPU, RSS, PSS, USS/private, shared-memory, render-sync, renderer resource,
decoder, caps, and frame timing evidence into one CSV.

Options:
  --output <name>       Output connector name passed to wayland-video-surface-smoke
  --all-outputs         Apply the generated video wallpaper to every output
  --expect-compositor <kind>
                       Require hyprland, niri, generic-wayland, or none
  --scenario <name>     Scenario to run. May be repeated. Defaults:
                       active, battery, unfocused, fullscreen, hidden,
                       session-inactive, session-locked
                       Optional: fullscreen-resume
  --work-dir <dir>      Parent directory for temporary baseline data
  --report-dir <dir>    Exact baseline evidence directory. Created and kept
  --sample-duration <s> Per-scenario sampling duration. Default: 30
  --sample-interval <s> Per-scenario sampling interval. Default: 1
  --allow-missing       Forward allow-missing to scenario smoke runs
  --no-build            Use existing target/debug binaries
  --keep                Keep generated baseline data when --report-dir is not set
  -h, --help            Show this help text

Each scenario writes its original wayland-video-surface-smoke evidence under
<report>/scenarios/<name>/ and the aggregate table to <report>/baseline.csv.
EOF
}

output_name=""
all_outputs=0
expect_compositor=""
work_parent="${TMPDIR:-/tmp}"
report_dir=""
sample_duration=30
sample_interval=1
allow_missing=0
build=1
keep=0
requested_scenarios=()

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
    --scenario)
      [[ $# -ge 2 ]] || { echo "--scenario requires a value" >&2; exit 2; }
      requested_scenarios+=("$2")
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
    --allow-missing)
      allow_missing=1
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

if [[ ! "$sample_duration" =~ ^[1-9][0-9]*$ ]]; then
  echo "--sample-duration must be a positive integer" >&2
  exit 2
fi
if [[ ! "$sample_interval" =~ ^[1-9][0-9]*$ ]]; then
  echo "--sample-interval must be a positive integer" >&2
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

default_scenarios=(
  active
  battery
  unfocused
  fullscreen
  hidden
  session-inactive
  session-locked
)
if [[ "${#requested_scenarios[@]}" -eq 0 ]]; then
  requested_scenarios=("${default_scenarios[@]}")
fi

validate_scenario() {
  case "$1" in
    active|battery|unfocused|fullscreen|hidden|session-inactive|session-locked|fullscreen-resume)
      return 0
      ;;
    *)
      echo "unknown scenario: $1" >&2
      return 2
      ;;
  esac
}

for scenario in "${requested_scenarios[@]}"; do
  validate_scenario "$scenario"
done

if [[ -n "$report_dir" ]]; then
  work_dir="$report_dir"
  mkdir -p "$work_dir"
else
  mkdir -p "$work_parent"
  work_dir="$(mktemp -d "${work_parent%/}/gilder-wayland-baseline.XXXXXX")"
fi
scenario_root="$work_dir/scenarios"
mkdir -p "$scenario_root"
baseline_csv="$work_dir/baseline.csv"
matrix_csv="$work_dir/matrix.csv"
summary_path="$work_dir/summary.txt"
metadata_path="$work_dir/metadata.txt"

cleanup() {
  if [[ "${work_dir:-}" != "" && "$keep" -eq 0 && -z "$report_dir" ]]; then
    rm -rf "$work_dir"
  elif [[ "${work_dir:-}" != "" ]]; then
    printf 'kept work dir: %s\n' "$work_dir"
  fi
}
trap cleanup EXIT

passes=0
skips=0
failures=0

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

report_value_or_none() {
  local report="$1"
  local key="$2"
  if [[ ! -f "$report" ]]; then
    printf 'none\n'
    return
  fi
  awk -v key="$key" -F': ' '$1 == key { print $2; found = 1; exit } END { exit found ? 0 : 1 }' "$report" \
    || printf 'none\n'
}

numeric_report_value_or_zero() {
  local value
  value="$(report_value_or_none "$1" "$2")"
  if [[ "$value" =~ ^[0-9]+$ ]]; then
    printf '%s\n' "$value"
  else
    printf '0\n'
  fi
}

scenario_status() {
  local report="$1"
  local command_status="$2"
  local failed
  local skipped
  if [[ "$command_status" -ne 0 ]]; then
    printf 'failed\n'
    return
  fi
  failed="$(numeric_report_value_or_zero "$report" result_failed)"
  skipped="$(numeric_report_value_or_zero "$report" result_skipped)"
  if (( failed > 0 )); then
    printf 'failed\n'
  elif (( skipped > 0 )); then
    printf 'skipped\n'
  else
    printf 'passed\n'
  fi
}

process_keys=(
  samples
  avg_cpu_percent
  avg_rss_kib
  max_rss_kib
  first_rss_kib
  last_rss_kib
  retained_rss_delta_kib
  peak_over_first_rss_kib
  avg_pss_kib
  max_pss_kib
  first_pss_kib
  last_pss_kib
  retained_pss_delta_kib
  peak_over_first_pss_kib
  avg_private_kib
  max_private_kib
  first_private_kib
  last_private_kib
  retained_private_delta_kib
  peak_over_first_private_kib
  avg_uss_kib
  max_uss_kib
  first_uss_kib
  last_uss_kib
  retained_uss_delta_kib
  peak_over_first_uss_kib
  avg_shared_kib
  max_shared_kib
  first_shared_kib
  last_shared_kib
  retained_shared_delta_kib
  peak_over_first_shared_kib
  avg_gpu_busy_percent
  max_gpu_busy_percent
)

telemetry_keys=(
  telemetry_rows
  render_sync_cache_hits_delta
  render_sync_cache_misses_delta
  render_sync_updates_queued_latest
  render_sync_updates_skipped_latest
  render_sync_package_cache_entries_latest
  render_sync_archive_cache_entries_latest
  render_sync_planned_image_resource_references_latest
  render_sync_planned_unique_image_resources_latest
  render_sync_planned_image_resource_reference_bytes_latest
  render_sync_planned_unique_image_resource_bytes_latest
  renderer_output_windows_latest
  renderer_output_windows_max
  renderer_static_surfaces_latest
  renderer_static_surfaces_max
  renderer_slideshow_surfaces_latest
  renderer_slideshow_surfaces_max
  renderer_video_surfaces_latest
  renderer_video_surfaces_max
  renderer_video_pipelines_latest
  renderer_video_pipelines_max
)

video_keys=(
  video_runtime_rows
  video_runtime_samples
  video_runtime_outputs
  video_decoder_policy_status_latest
  video_actual_decoders_latest
  video_decoder_classes_latest
  video_memory_features_latest
  video_sink_memory_features_latest
  video_zero_copy_evidence_latest
  video_position_delta_ms_max
  video_qos_messages_max
  video_qos_dropped_max
  video_gtk_frame_clock_ticks_max
  video_gtk_frame_clock_before_paint_ticks_max
  video_gtk_frame_clock_update_ticks_max
  video_gtk_frame_clock_layout_ticks_max
  video_gtk_frame_clock_paint_ticks_max
  video_gtk_frame_clock_after_paint_ticks_max
  video_gtk_frame_timings_complete_max
  video_gtk_frame_timings_presentation_interval_us_max
)

write_baseline_header() {
  local key
  printf 'scenario,phase,status,result_passed,result_skipped,result_failed,report_dir,validation_report' > "$baseline_csv"
  for key in "${process_keys[@]}"; do
    printf ',%s' "$key" >> "$baseline_csv"
  done
  for key in "${telemetry_keys[@]}"; do
    printf ',%s' "$key" >> "$baseline_csv"
  done
  for key in "${video_keys[@]}"; do
    printf ',%s' "$key" >> "$baseline_csv"
  done
  printf '\n' >> "$baseline_csv"
}

append_baseline_row() {
  local scenario="$1"
  local phase="$2"
  local status="$3"
  local report_dir_path="$4"
  local report="$5"
  local prefix="$6"
  local key
  local value
  local row=(
    "$scenario"
    "$phase"
    "$status"
    "$(report_value_or_none "$report" result_passed)"
    "$(report_value_or_none "$report" result_skipped)"
    "$(report_value_or_none "$report" result_failed)"
    "${report_dir_path#$work_dir/}"
    "${report#$work_dir/}"
  )

  for key in "${process_keys[@]}"; do
    row+=("$(report_value_or_none "$report" "${prefix}_${key}")")
  done
  for key in "${telemetry_keys[@]}"; do
    row+=("$(report_value_or_none "$report" "${prefix}_${key}")")
  done
  for key in "${video_keys[@]}"; do
    row+=("$(report_value_or_none "$report" "${prefix}_${key}")")
  done

  for index in "${!row[@]}"; do
    if [[ "$index" -gt 0 ]]; then
      printf ',' >> "$baseline_csv"
    fi
    value="${row[$index]}"
    csv_escape "$value" >> "$baseline_csv"
  done
  printf '\n' >> "$baseline_csv"
}

write_matrix_header() {
  printf 'scenario,status,report_dir,validation_report,log\n' > "$matrix_csv"
}

append_matrix_row() {
  local scenario="$1"
  local status="$2"
  local report_dir_path="$3"
  local report="$4"
  local log_file="$5"
  local row=(
    "$scenario"
    "$status"
    "${report_dir_path#$work_dir/}"
    "${report#$work_dir/}"
    "${log_file#$work_dir/}"
  )
  for index in "${!row[@]}"; do
    if [[ "$index" -gt 0 ]]; then
      printf ',' >> "$matrix_csv"
    fi
    csv_escape "${row[$index]}" >> "$matrix_csv"
  done
  printf '\n' >> "$matrix_csv"
}

write_metadata() {
  cat > "$metadata_path" <<EOF
sample_duration: ${sample_duration}
sample_interval: ${sample_interval}
output: ${output_name:-auto}
all_outputs: ${all_outputs}
expect_compositor: ${expect_compositor:-none}
allow_missing: ${allow_missing}
build: ${build}
scenarios: ${requested_scenarios[*]}
baseline_csv: ${baseline_csv}
matrix_csv: ${matrix_csv}
scenario_root: ${scenario_root}
EOF
}

write_summary() {
  cat > "$summary_path" <<EOF
passed: ${passes}
skipped: ${skips}
failed: ${failures}
metadata: ${metadata_path}
baseline: ${baseline_csv}
matrix: ${matrix_csv}
scenario_root: ${scenario_root}
EOF
}

scenario_smoke_args() {
  local scenario="$1"
  local -n args_ref="$2"
  args_ref+=(--sample-performance)
  args_ref+=(--expect-renderer-video-pipeline-lifecycle)
  case "$scenario" in
    active)
      args_ref+=(--sample-paused)
      args_ref+=(--simulate-power ac)
      args_ref+=(--simulate-output-state active)
      args_ref+=(--simulate-session active)
      args_ref+=(--require-video-runtime-row)
      ;;
    battery)
      args_ref+=(--simulate-power battery)
      args_ref+=(--simulate-output-state active)
      args_ref+=(--simulate-session active)
      args_ref+=(--require-video-runtime-row)
      ;;
    unfocused)
      args_ref+=(--simulate-power ac)
      args_ref+=(--simulate-output-state unfocused)
      args_ref+=(--simulate-session active)
      args_ref+=(--require-video-runtime-row)
      ;;
    fullscreen)
      args_ref+=(--simulate-power ac)
      args_ref+=(--simulate-output-state fullscreen)
      args_ref+=(--simulate-session active)
      ;;
    hidden)
      args_ref+=(--simulate-power ac)
      args_ref+=(--simulate-output-state hidden)
      args_ref+=(--simulate-session active)
      ;;
    session-inactive)
      args_ref+=(--simulate-power ac)
      args_ref+=(--simulate-output-state active)
      args_ref+=(--simulate-session inactive)
      ;;
    session-locked)
      args_ref+=(--simulate-power ac)
      args_ref+=(--simulate-output-state active)
      args_ref+=(--simulate-session locked)
      ;;
    fullscreen-resume)
      args_ref+=(--simulate-power ac)
      args_ref+=(--simulate-session active)
      args_ref+=(--measure-fullscreen-resume)
      args_ref+=(--require-video-runtime-row)
      ;;
  esac
}

append_rows_for_scenario() {
  local scenario="$1"
  local status="$2"
  local scenario_dir="$3"
  local report="$4"
  case "$scenario" in
    active)
      append_baseline_row "$scenario" active "$status" "$scenario_dir" "$report" performance_active
      append_baseline_row "$scenario" user-paused "$status" "$scenario_dir" "$report" performance_paused
      ;;
    fullscreen-resume)
      append_baseline_row "$scenario" resumed "$status" "$scenario_dir" "$report" performance_active
      ;;
    *)
      append_baseline_row "$scenario" "$scenario" "$status" "$scenario_dir" "$report" performance_active
      ;;
  esac
}

write_metadata
write_baseline_header
write_matrix_header

if [[ "$build" -eq 1 ]]; then
  cargo build --features gtk-renderer,video-renderer
fi

wayland_smoke="$repo_root/scripts/wayland-video-surface-smoke.sh"
for scenario in "${requested_scenarios[@]}"; do
  scenario_dir="$scenario_root/$scenario"
  scenario_log="$work_dir/${scenario}.log"
  validation_report="$scenario_dir/validation-report.txt"
  mkdir -p "$scenario_dir"

  command=(
    "$wayland_smoke"
    --report-dir "$scenario_dir"
    --sample-duration "$sample_duration"
    --sample-interval "$sample_interval"
    --no-build
  )
  if [[ -n "$output_name" ]]; then
    command+=(--output "$output_name")
  fi
  if [[ "$all_outputs" -eq 1 ]]; then
    command+=(--all-outputs)
  fi
  if [[ -n "$expect_compositor" ]]; then
    command+=(--expect-compositor "$expect_compositor")
  fi
  if [[ "$allow_missing" -eq 1 ]]; then
    command+=(--allow-missing)
  fi
  scenario_smoke_args "$scenario" command

  set +e
  "${command[@]}" > "$scenario_log" 2>&1
  command_status=$?
  set -e
  status="$(scenario_status "$validation_report" "$command_status")"
  case "$status" in
    passed)
      passes=$((passes + 1))
      note "PASS: ${scenario}"
      ;;
    skipped)
      skips=$((skips + 1))
      note "SKIP: ${scenario}"
      ;;
    failed)
      failures=$((failures + 1))
      note "FAIL: ${scenario}"
      ;;
  esac
  append_matrix_row "$scenario" "$status" "$scenario_dir" "$validation_report" "$scenario_log"
  append_rows_for_scenario "$scenario" "$status" "$scenario_dir" "$validation_report"
done

write_summary
note "metadata: $metadata_path"
note "baseline: $baseline_csv"
note "matrix:   $matrix_csv"
note "report:   $summary_path"
note "summary: ${passes} passed, ${skips} skipped, ${failures} failed"
exit "$([[ "$failures" -gt 0 ]] && echo 1 || echo 0)"
