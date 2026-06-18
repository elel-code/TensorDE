#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
usage: scripts/desktop-policy-smoke.sh [options]

Run a headless daemon smoke matrix for desktop-state performance policy. The
script uses validation overrides to create a virtual output, applies the static
example wallpaper, and samples decisions/telemetry for active, battery,
unfocused, fullscreen, hidden, inactive, and locked scenarios.

Options:
  --output <name>       Virtual output name. Default: HEADLESS-1
  --work-dir <dir>      Parent directory for temporary smoke data
  --allow-missing       Report missing tools as skips instead of failures
  --no-build            Use existing target/debug binaries
  --sample-duration <s> Performance sampling duration. Default: 2
  --sample-interval <s> Performance sampling interval. Default: 1
  --keep                Keep generated smoke data and logs
  -h, --help            Show this help text
EOF
}

output_name="HEADLESS-1"
work_parent="${TMPDIR:-/tmp}"
allow_missing=0
build=1
keep=0
sample_duration=2
sample_interval=1

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
  if [[ "${work_dir:-}" != "" && "$keep" -eq 0 ]]; then
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

require_file "$gilderd" || true
require_file "$gilderctl" || true
require_file "$performance_snapshot" || true
if [[ ! -d "$wallpaper_path" ]]; then
  skip_or_fail "missing example wallpaper $wallpaper_path"
fi
if [[ "$failures" -gt 0 || "$skips" -gt 0 ]]; then
  note "summary: ${passes} passed, ${skips} skipped, ${failures} failed"
  exit "$([[ "$failures" -gt 0 ]] && echo 1 || echo 0)"
fi

mkdir -p "$work_parent"
work_dir="$(mktemp -d "${work_parent%/}/gilder-desktop-policy.XXXXXX")"
metadata_path="$work_dir/metadata.txt"
cat > "$metadata_path" <<EOF
output: ${output_name}
sample_duration: ${sample_duration}
sample_interval: ${sample_interval}
wallpaper: ${wallpaper_path}
EOF

run_scenario() {
  local name="$1"
  local expected_mode="$2"
  local expected_reason="$3"
  local expected_action="$4"
  local expected_plan_kind="$5"
  local power_state="$6"
  local output_state="$7"
  local session_state="$8"

  local scenario_dir="$work_dir/$name"
  local socket="$scenario_dir/runtime/gilder.sock"
  local daemon_log="$scenario_dir/gilderd.log"
  local status_before="$scenario_dir/status-before.json"
  local status_after="$scenario_dir/status-after.json"
  local perf_dir="$scenario_dir/performance"
  local perf_log="$scenario_dir/performance.log"

  mkdir -p "$scenario_dir/runtime" "$scenario_dir/config" "$scenario_dir/state" "$scenario_dir/cache"
  chmod 700 "$scenario_dir/runtime"

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

  "${daemon_env[@]}" "$gilderd" >"$daemon_log" 2>&1 &
  current_daemon_pid=$!

  for _ in $(seq 1 80); do
    if [[ -S "$socket" ]]; then
      break
    fi
    if ! kill -0 "$current_daemon_pid" >/dev/null 2>&1; then
      note "daemon log for ${name}:"
      sed -n '1,120p' "$daemon_log"
      skip_or_fail "${name}: gilderd exited before creating IPC socket"
      stop_daemon
      return 0
    fi
    sleep 0.1
  done
  if [[ ! -S "$socket" ]]; then
    note "daemon log for ${name}:"
    sed -n '1,120p' "$daemon_log"
    skip_or_fail "${name}: gilderd did not create IPC socket"
    stop_daemon
    return 0
  fi
  pass "${name}: started isolated daemon"

  if ! env GILDER_SOCKET="$socket" "$gilderctl" status > "$status_before"; then
    skip_or_fail "${name}: failed to capture initial status"
    stop_daemon
    return 0
  fi
  if grep -Fq "\"name\":\"${output_name}\"" "$status_before"; then
    pass "${name}: status reports virtual output"
  else
    skip_or_fail "${name}: status does not report virtual output"
  fi

  if ! env GILDER_SOCKET="$socket" "$gilderctl" set "$wallpaper_path" --output "$output_name" >/dev/null; then
    skip_or_fail "${name}: failed to set wallpaper"
    stop_daemon
    return 0
  fi
  if ! env GILDER_SOCKET="$socket" "$gilderctl" set "$wallpaper_path" --output "$output_name" >/dev/null; then
    skip_or_fail "${name}: failed to repeat wallpaper set"
    stop_daemon
    return 0
  fi
  if ! env GILDER_SOCKET="$socket" "$gilderctl" status > "$status_after"; then
    skip_or_fail "${name}: failed to capture status after set"
    stop_daemon
    return 0
  fi
  if grep -q "\"reason\":\"${expected_reason}\"" "$status_after"; then
    pass "${name}: status reports ${expected_reason} decision"
  else
    skip_or_fail "${name}: status does not report ${expected_reason} decision"
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
  if [[ -n "$expected_plan_kind" ]]; then
    sample_args+=(--expect-plan-kind "$expected_plan_kind")
  fi
  if [[ "$allow_missing" -eq 1 ]]; then
    sample_args+=(--allow-missing)
  fi

  if "$performance_snapshot" "${sample_args[@]}" >"$perf_log" 2>&1; then
    pass "${name}: captured policy performance evidence"
  else
    note "performance log for ${name}:"
    sed -n '1,160p' "$perf_log"
    skip_or_fail "${name}: performance sampling failed"
  fi

  note "${name}: status before: $status_before"
  note "${name}: status after:  $status_after"
  note "${name}: performance:   $perf_dir"
  note "${name}: daemon log:    $daemon_log"
  stop_daemon
}

run_scenario active active interactive render static-image ac active active
run_scenario battery throttled battery render static-image battery active active
run_scenario unfocused throttled unfocused render static-image ac unfocused active
run_scenario fullscreen paused fullscreen remove "" ac fullscreen active
run_scenario hidden paused output-hidden remove "" ac hidden active
run_scenario session-inactive paused session-inactive remove "" ac active inactive
run_scenario session-locked paused session-locked remove "" ac active locked

note "metadata: $metadata_path"
note "summary: ${passes} passed, ${skips} skipped, ${failures} failed"
if [[ "$failures" -gt 0 ]]; then
  exit 1
fi
