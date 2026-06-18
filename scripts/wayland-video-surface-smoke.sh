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
  --work-dir <dir>    Parent directory for temporary smoke data
  --allow-missing     Report missing session/tools/plugins as skips instead of failures
  --no-build          Use existing target/debug binaries
  --sample-performance
                     Capture active-video CPU/RSS/status evidence after applying wallpaper
  --sample-paused    Also pause the output, capture paused-video evidence, then resume
  --sample-duration <sec>
                     Performance sampling duration. Default: 8
  --sample-interval <sec>
                     Performance sampling interval. Default: 1
  --simulate-power <state>
                     Start daemon with GILDER_POWER_STATE=ac|battery|unknown
  --simulate-output-state <state>
                     Start daemon with GILDER_OUTPUT_STATE=active|unfocused|fullscreen|hidden
  --simulate-session <state>
                     Start daemon with GILDER_SESSION_STATE=active|inactive|locked
  --keep              Keep generated smoke data and logs
  -h, --help          Show this help text
EOF
}

work_parent="${TMPDIR:-/tmp}"
output_name=""
allow_missing=0
build=1
keep=0
sample_performance=0
sample_paused=0
sample_duration=8
sample_interval=1
simulate_power=""
simulate_output_state=""
simulate_session=""

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

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

mkdir -p "$work_parent"
work_dir="$(mktemp -d "${work_parent%/}/gilder-wayland-video.XXXXXX")"
socket="$work_dir/runtime/gilder.sock"
daemon_log="$work_dir/gilderd.log"
status_before="$work_dir/status-before.json"
status_after="$work_dir/status-after.json"
status_paused="$work_dir/status-paused.json"
status_resumed="$work_dir/status-resumed.json"
wallpaper_dir="$work_dir/wallpaper.gwpdir"
source_dir="$work_dir/source"
performance_active_dir="$work_dir/performance-active"
performance_active_log="$work_dir/performance-active.log"
performance_paused_dir="$work_dir/performance-paused"
performance_paused_log="$work_dir/performance-paused.log"
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
daemon_pid=""

cleanup() {
  if [[ -n "$daemon_pid" ]] && kill -0 "$daemon_pid" >/dev/null 2>&1; then
    kill "$daemon_pid" >/dev/null 2>&1 || true
    wait "$daemon_pid" >/dev/null 2>&1 || true
  fi
  if [[ "$keep" -eq 0 ]]; then
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

capture_performance() {
  local label="$1"
  local output_dir="$2"
  local log_file="$3"
  local expected_mode="${4:-}"
  local expected_reason="${5:-}"
  local expected_action="${6:-}"
  local expected_plan_kind="${7:-}"
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
  )
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

if [[ -z "${WAYLAND_DISPLAY:-}" ]]; then
  skip_or_fail "WAYLAND_DISPLAY is not set; run this inside niri, Hyprland, or another Wayland session"
fi
if [[ -z "${XDG_RUNTIME_DIR:-}" ]]; then
  skip_or_fail "XDG_RUNTIME_DIR is not set"
fi
require_command ffmpeg || true
if [[ "$build" -eq 1 ]]; then
  require_command cargo || true
fi

if [[ "$failures" -gt 0 || "$skips" -gt 0 ]]; then
  note "summary: ${passes} passed, ${skips} skipped, ${failures} failed"
  exit "$([[ "$failures" -gt 0 ]] && echo 1 || echo 0)"
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
  note "summary: ${passes} passed, ${skips} skipped, ${failures} failed"
  exit "$([[ "$failures" -gt 0 ]] && echo 1 || echo 0)"
fi

mkdir -p "$work_dir/runtime" "$work_dir/config" "$work_dir/state" "$work_dir/cache" "$source_dir"
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
  daemon_env+=(GILDER_OUTPUT_STATE="$simulate_output_state")
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
    note "summary: ${passes} passed, ${skips} skipped, ${failures} failed"
    exit "$([[ "$failures" -gt 0 ]] && echo 1 || echo 0)"
  fi
  sleep 0.1
done
[[ -S "$socket" ]] || {
  note "daemon log:"
  sed -n '1,120p' "$daemon_log"
  skip_or_fail "gilderd did not create IPC socket"
  note "summary: ${passes} passed, ${skips} skipped, ${failures} failed"
  exit "$([[ "$failures" -gt 0 ]] && echo 1 || echo 0)"
}
pass "started isolated gilderd"

env GILDER_SOCKET="$socket" "$gilderctl" status > "$status_before"

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

if [[ -z "$output_name" ]]; then
  output_name="$(grep -o '"name":"[^"]*"' "$status_before" | head -n 1 | cut -d '"' -f 4 || true)"
fi
if [[ -z "$output_name" ]]; then
  skip_or_fail "daemon reported no output; pass --output <name> if compositor adapters are disabled"
  note "status evidence: $status_before"
  note "daemon log: $daemon_log"
  note "summary: ${passes} passed, ${skips} skipped, ${failures} failed"
  exit "$([[ "$failures" -gt 0 ]] && echo 1 || echo 0)"
fi
pass "selected output $output_name"

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
  note "summary: ${passes} passed, ${skips} skipped, ${failures} failed"
  exit "$([[ "$failures" -gt 0 ]] && echo 1 || echo 0)"
fi
pass "gtk4paintablesink is available"

env GILDER_SOCKET="$socket" "$gilderctl" set "$wallpaper_dir" --output "$output_name" >/dev/null
sleep 2
env GILDER_SOCKET="$socket" "$gilderctl" status > "$status_after"

if expects_active_video_plan; then
  if ! grep -q '"video_plans":\[' "$status_after" || grep -q '"video_plans":\[\]' "$status_after"; then
    skip_or_fail "status does not report an active video render plan"
  else
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

if [[ "$sample_performance" -eq 1 ]]; then
  expected_mode="$(expected_mode_for_reason "$expected_reason")"
  expected_action=""
  expected_plan_kind=""
  if [[ -n "$expected_reason" ]]; then
    if expects_active_video_plan; then
      expected_action="render"
      expected_plan_kind="video"
    else
      expected_action="remove"
    fi
  fi
  if capture_performance \
    "$performance_active_label" \
    "$performance_active_dir" \
    "$performance_active_log" \
    "$expected_mode" \
    "$expected_reason" \
    "$expected_action" \
    "$expected_plan_kind"; then
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
  if grep -q '"reason":"user-paused"' "$status_paused"; then
    pass "status reports user-paused decision"
  else
    skip_or_fail "status does not report user-paused decision after pause"
  fi

  if capture_performance \
    wayland-video-paused \
    "$performance_paused_dir" \
    "$performance_paused_log" \
    paused \
    user-paused \
    remove \
    ""; then
    pass "captured paused video performance evidence"
  else
    note "paused performance sample log:"
    sed -n '1,120p' "$performance_paused_log"
    skip_or_fail "paused performance sampling failed"
  fi

  env GILDER_SOCKET="$socket" "$gilderctl" resume --output "$output_name" >/dev/null
  sleep 1
  env GILDER_SOCKET="$socket" "$gilderctl" status > "$status_resumed"
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
if [[ "$sample_paused" -eq 1 ]]; then
  note "status paused: $status_paused"
  note "status resumed: $status_resumed"
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
note "Visually confirm that output '$output_name' shows the generated moving test video."
note "summary: ${passes} passed, ${skips} skipped, ${failures} failed"
if [[ "$failures" -gt 0 ]]; then
  exit 1
fi
