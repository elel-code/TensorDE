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
  --expect-render-sync-cache-hit
                     Require render_sync cache hits to increase during sampling
  --expect-desktop-refresh-skip
                     Require read-request desktop refresh skips to increase during sampling
  --expect-render-sync-update-queued
                     Require renderer sync queued count to be non-zero
  --expect-render-sync-update-skipped
                     Require renderer sync skipped count to be non-zero
  --expect-decoder-policy-status <status>
                     Require at least one video runtime row with this decoder policy status
  --expect-decoder-class <hardware|software|unknown>
                     Require at least one video runtime row with this decoder class
  --expect-memory-feature <feature>
                     Require at least one video runtime row with this caps memory feature
  --expect-sink-memory-feature <feature>
                     Require at least one video runtime row with this sink-side caps memory feature
  --expect-video-position-progress
                     Require sampled video position to advance on at least one output
  --expect-frame-limiter-enabled
                     Require at least one video runtime row with an enabled frame limiter
  --expect-frame-limiter-max-fps <fps>
                     Require at least one video runtime row with this frame limiter max_fps
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
expect_render_sync_cache_hit=0
expect_desktop_refresh_skip=0
expect_render_sync_update_queued=0
expect_render_sync_update_skipped=0
expect_decoder_policy_status=""
expect_decoder_class=""
expect_memory_feature=""
expect_sink_memory_feature=""
expect_video_position_progress=0
expect_frame_limiter_enabled=0
expect_frame_limiter_max_fps=""

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
        printf "avg_rss_kib: %.0f\n", rss_sum / samples
        printf "max_rss_kib: %d\n", max_rss
        printf "min_vsz_kib: %d\n", min_vsz
        printf "avg_vsz_kib: %.0f\n", vsz_sum / samples
        printf "max_vsz_kib: %d\n", max_vsz
        printf "min_pss_kib: %d\n", min_pss
        printf "avg_pss_kib: %.0f\n", pss_sum / samples
        printf "max_pss_kib: %d\n", max_pss
        printf "min_private_kib: %d\n", min_private
        printf "avg_private_kib: %.0f\n", private_sum / samples
        printf "max_private_kib: %d\n", max_private
        printf "min_uss_kib: %d\n", min_uss
        printf "avg_uss_kib: %.0f\n", uss_sum / samples
        printf "max_uss_kib: %d\n", max_uss
        printf "min_shared_kib: %d\n", min_shared
        printf "avg_shared_kib: %.0f\n", shared_sum / samples
        printf "max_shared_kib: %d\n", max_shared
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
      adaptive_refreshes = $11 + 0
      adaptive_skips = $12 + 0
      adaptive_triggers = $13 + 0
      cpu_pressure = $14 + 0
      memory_pressure = $15 + 0
      temperature = $16 + 0
      external_online = $17
      battery_present = $18
      battery_discharging = $19
      battery_capacity = $20 + 0
      battery_power = $21 + 0
      daemon_gpu_avg = $22
      daemon_gpu_max_sample = $23
      daemon_gpu_sources = $24

      if (rows == 1) {
        first_refreshes = refreshes
        first_skips = skips
        first_changes = changes
        first_hits = hits
        first_misses = misses
        first_queued = queued
        first_update_skips = update_skips
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
    }
    END {
      refresh_delta = last_refreshes - first_refreshes
      skip_delta = last_skips - first_skips
      change_delta = last_changes - first_changes
      hit_delta = last_hits - first_hits
      miss_delta = last_misses - first_misses
      queued_delta = last_queued - first_queued
      update_skip_delta = last_update_skips - first_update_skips
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
      }
    }
  ' "$telemetry_csv" > "$summary"
}

write_video_runtime_summary() {
  local video_runtime_csv="$1"
  local summary="$2"
  awk -F, '
    NR == 1 { next }
    {
      rows += 1
      sample = $1
      output = $3
      position = $15
      duration = $16
      limiter_enabled = $17
      limiter_fps = $18

      if (output != "" && !(output in seen_output)) {
        seen_output[output] = 1
        outputs += 1
      }
      if (sample != "" && !(sample in seen_sample)) {
        seen_sample[sample] = 1
        samples += 1
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

has_telemetry_expectations() {
  [[ "$expect_render_sync_cache_hit" -eq 1 ||
    "$expect_desktop_refresh_skip" -eq 1 ||
    "$expect_render_sync_update_queued" -eq 1 ||
    "$expect_render_sync_update_skipped" -eq 1 ]]
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
}

has_video_runtime_expectations() {
  [[ -n "$expect_decoder_policy_status" ||
    -n "$expect_decoder_class" ||
    -n "$expect_memory_feature" ||
    -n "$expect_sink_memory_feature" ||
    "$expect_video_position_progress" -eq 1 ||
    "$expect_frame_limiter_enabled" -eq 1 ||
    -n "$expect_frame_limiter_max_fps" ]]
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
  if [[ "$expect_video_position_progress" -eq 1 ]]; then
    expect_video_runtime_summary_minimum "video_position_moving_outputs" 1 "moving video output count"
  fi
  if [[ "$expect_frame_limiter_enabled" -eq 1 ]]; then
    expect_video_runtime_field 17 "true" "frame limiter enabled"
  fi
  if [[ -n "$expect_frame_limiter_max_fps" ]]; then
    expect_video_runtime_field 18 "$expect_frame_limiter_max_fps" "frame limiter max_fps"
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
expect_render_sync_cache_hit: ${expect_render_sync_cache_hit}
expect_desktop_refresh_skip: ${expect_desktop_refresh_skip}
expect_render_sync_update_queued: ${expect_render_sync_update_queued}
expect_render_sync_update_skipped: ${expect_render_sync_update_skipped}
expect_decoder_policy_status: ${expect_decoder_policy_status:-none}
expect_decoder_class: ${expect_decoder_class:-none}
expect_memory_feature: ${expect_memory_feature:-none}
expect_sink_memory_feature: ${expect_sink_memory_feature:-none}
expect_video_position_progress: ${expect_video_position_progress}
expect_frame_limiter_enabled: ${expect_frame_limiter_enabled}
expect_frame_limiter_max_fps: ${expect_frame_limiter_max_fps:-none}
gpu_busy_sources: drm gpu_busy_percent sysfs when readable; nvidia-smi utilization.gpu when available
EOF

printf 'sample,elapsed_seconds,pid,cpu_percent,rss_kib,vsz_kib,pss_kib,private_clean_kib,private_dirty_kib,private_kib,uss_kib,shared_clean_kib,shared_dirty_kib,shared_kib,stat,comm,status_file,status_error_file,gpu_busy_percent_avg,gpu_busy_percent_max,gpu_busy_sources\n' > "$csv_path"
printf 'sample,elapsed_seconds,output_name,action,mode,reason,max_fps,wallpaper,plan_kind,source,fit,target_max_fps,muted\n' > "$decisions_path"
printf 'sample,elapsed_seconds,desktop_refreshes,desktop_refresh_skips,desktop_changes,last_desktop_refresh_age_ms,render_sync_cache_hits,render_sync_cache_misses,render_sync_updates_queued,render_sync_updates_skipped,adaptive_refreshes,adaptive_refresh_skips,adaptive_active_triggers,cpu_pressure_some_avg10_x100,memory_pressure_some_avg10_x100,temperature_max_millicelsius,power_external_online,power_system_battery_present,power_battery_discharging,power_battery_capacity_percent,power_battery_power_microwatts,gpu_busy_percent_avg,gpu_busy_percent_max,gpu_busy_sources\n' > "$telemetry_path"
printf 'sample,elapsed_seconds,output_name,mode,gst_state,decoder_policy,decoder_policy_status,actual_decoders,decoder_classes,caps_report_count,memory_features,sink_memory_features,media_types,caps_paths,position_ms,duration_ms,frame_limiter_enabled,frame_limiter_max_fps,source\n' > "$video_runtime_path"

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
