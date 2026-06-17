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
allow_missing=0
keep=0

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

write_summary() {
  local csv="$1"
  local summary="$2"
  awk -F, '
    NR == 1 { next }
    {
      samples += 1
      cpu_sum += $4
      if ($5 + 0 > max_rss) { max_rss = $5 + 0 }
      if ($6 + 0 > max_vsz) { max_vsz = $6 + 0 }
    }
    END {
      printf "samples: %d\n", samples
      if (samples > 0) {
        printf "avg_cpu_percent: %.2f\n", cpu_sum / samples
        printf "max_rss_kib: %d\n", max_rss
        printf "max_vsz_kib: %d\n", max_vsz
      }
    }
  ' "$csv" > "$summary"
}

if ! is_positive_integer "$duration"; then
  echo "--duration must be a positive integer" >&2
  exit 2
fi
if ! is_positive_integer "$interval"; then
  echo "--interval must be a positive integer" >&2
  exit 2
fi

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

mkdir -p "$work_parent"
work_dir="$(mktemp -d "${work_parent%/}/gilder-performance.XXXXXX")"
if [[ "$keep" -eq 0 ]]; then
  trap 'rm -rf "$work_dir"' EXIT
fi

samples=$(( (duration + interval - 1) / interval ))
[[ "$samples" -ge 1 ]] || samples=1
csv_path="$work_dir/samples.csv"
metadata_path="$work_dir/metadata.txt"
summary_path="$work_dir/summary.txt"

cat > "$metadata_path" <<EOF
label: ${label}
pid: ${pid}
socket: ${socket:-default}
gilderctl: ${gilderctl:-unavailable}
duration_seconds: ${duration}
interval_seconds: ${interval}
samples: ${samples}
EOF

printf 'sample,elapsed_seconds,pid,cpu_percent,rss_kib,vsz_kib,stat,comm,status_file,status_error_file\n' > "$csv_path"

status_failures=0
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
  fi

  if [[ "$failures" -gt 0 ]]; then
    break
  fi

  printf '%s,%s,%s,%s,%s,%s,%s,%s,%s,%s\n' \
    "$sample" \
    "$elapsed" \
    "$sample_pid" \
    "$cpu_percent" \
    "$rss_kib" \
    "$vsz_kib" \
    "$stat" \
    "$comm" \
    "${status_file#$work_dir/}" \
    "${status_error_file#$work_dir/}" >> "$csv_path"

  if [[ "$sample" -lt "$samples" ]]; then
    sleep "$interval"
  fi
done

write_summary "$csv_path" "$summary_path"
pass "wrote process samples to $csv_path"
pass "wrote summary to $summary_path"
if [[ "$status_enabled" -eq 1 && "$status_failures" -eq 0 ]]; then
  pass "wrote status snapshots under $work_dir"
elif [[ "$status_enabled" -eq 1 ]]; then
  note "status snapshots had ${status_failures} failed samples"
else
  note "status snapshots skipped because gilderctl is unavailable"
fi

if [[ "$keep" -eq 1 ]]; then
  note "kept work dir: $work_dir"
else
  note "work dir will be removed; rerun with --keep to preserve evidence"
fi
note "metadata: $metadata_path"
note "samples:  $csv_path"
note "sample summary: $summary_path"
note "summary: ${passes} passed, ${skips} skipped, ${failures} failed"
if [[ "$failures" -gt 0 ]]; then
  exit 1
fi
