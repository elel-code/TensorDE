#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
usage: scripts/native-wgpu-smoke.sh [options]

Run the native wgpu/Vulkan layer-shell helper on a real Wayland display and
sample process memory/CPU. This does not use playbin, waylandsink, or the
manual linux-dmabuf attach prototype.

Options:
  --display <name>      Wayland display name. Default: WAYLAND_DISPLAY.
  --report-dir <dir>    Exact evidence directory. Created and kept.
  --work-dir <dir>      Parent directory for temporary data. Default: /tmp.
  --sample-duration <s> Run/sample duration. Default: 5.
  --sample-interval <s> Sampling interval in whole seconds. Default: 1.
  --runtime-interval-ms <ms>
                        Native runtime JSONL sample interval. Default: 1000.
  --target-fps <n>      Render loop target. Default: 240.
  --no-fps-limit        Disable render loop sleep.
  --layer <name>        background, bottom, top, or overlay. Default: bottom.
  --allow-foreground-layer
                        Permit top/overlay layers for short visual debugging.
  --output-name <name>  Bind the layer-shell surface to a wl_output name such
                        as HDMI-A-1. Required for >=200fps smoke runs unless
                        --allow-compositor-output is passed.
  --allow-compositor-output
                        Permit compositor-selected output for high-fps smoke.
  --color <value>       #rrggbb or r,g,b clear color. Default: #0b5cff.
  --no-build            Reuse existing target/release/gilder-native-wgpu.
  -h, --help            Show this help text.
EOF
}

work_parent="${TMPDIR:-/tmp}"
report_dir=""
wayland_display="${WAYLAND_DISPLAY:-}"
sample_duration=5
sample_interval=1
runtime_interval_ms=1000
target_fps=240
fps_limit=1
layer="bottom"
allow_foreground_layer=0
output_name=""
allow_compositor_output=0
color="#0b5cff"
no_build=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --display)
      [[ $# -ge 2 ]] || { echo "--display requires a value" >&2; exit 2; }
      wayland_display="$2"
      shift 2
      ;;
    --report-dir)
      [[ $# -ge 2 ]] || { echo "--report-dir requires a value" >&2; exit 2; }
      report_dir="$2"
      shift 2
      ;;
    --work-dir)
      [[ $# -ge 2 ]] || { echo "--work-dir requires a value" >&2; exit 2; }
      work_parent="$2"
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
    --target-fps)
      [[ $# -ge 2 ]] || { echo "--target-fps requires a value" >&2; exit 2; }
      target_fps="$2"
      fps_limit=1
      shift 2
      ;;
    --runtime-interval-ms)
      [[ $# -ge 2 ]] || { echo "--runtime-interval-ms requires milliseconds" >&2; exit 2; }
      runtime_interval_ms="$2"
      shift 2
      ;;
    --no-fps-limit)
      fps_limit=0
      shift
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
    --color)
      [[ $# -ge 2 ]] || { echo "--color requires a value" >&2; exit 2; }
      color="$2"
      shift 2
      ;;
    --no-build)
      no_build=1
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

[[ -n "$wayland_display" ]] || { echo "WAYLAND_DISPLAY is empty; pass --display" >&2; exit 2; }
[[ "$sample_duration" =~ ^[0-9]+$ && "$sample_duration" -gt 0 ]] || {
  echo "--sample-duration must be a positive integer" >&2
  exit 2
}
[[ "$sample_interval" =~ ^[0-9]+$ && "$sample_interval" -gt 0 ]] || {
  echo "--sample-interval must be a positive integer" >&2
  exit 2
}
[[ "$runtime_interval_ms" =~ ^[0-9]+$ && "$runtime_interval_ms" -ge 100 ]] || {
  echo "--runtime-interval-ms must be an integer >= 100" >&2
  exit 2
}
if [[ "$fps_limit" -eq 1 ]]; then
  [[ "$target_fps" =~ ^[0-9]+$ && "$target_fps" -gt 0 ]] || {
    echo "--target-fps must be a positive integer" >&2
    exit 2
  }
fi
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
if [[ "$fps_limit" -eq 1 && "$target_fps" -ge 200 && -z "$output_name" && "$allow_compositor_output" -eq 0 ]]; then
  echo "--output-name is required for >=200fps wgpu smoke runs; pass --allow-compositor-output only if compositor-selected output is intentional" >&2
  exit 2
fi

if [[ -n "$report_dir" ]]; then
  work_dir="$report_dir"
  mkdir -p "$work_dir"
else
  mkdir -p "$work_parent"
  work_dir="$(mktemp -d "${work_parent%/}/gilder-native-wgpu.XXXXXX")"
fi

exe="target/release/gilder-native-wgpu"
if [[ "$no_build" -eq 0 ]]; then
  cargo build --release --features native-wgpu-renderer --bin gilder-native-wgpu
fi
if [[ ! -x "$exe" ]]; then
  echo "missing executable $exe; build it or omit --no-build" >&2
  exit 1
fi

runtime_json="$work_dir/runtime.json"
runtime_jsonl="$work_dir/runtime.jsonl"
samples_csv="$work_dir/samples.csv"
summary_txt="$work_dir/summary.txt"
runtime_summary_txt="$work_dir/runtime-summary.txt"
metadata_txt="$work_dir/metadata.txt"
stdout_log="$work_dir/stdout.log"
stderr_log="$work_dir/stderr.log"

native_args=(
  --duration "$sample_duration"
  --layer "$layer"
  --color "$color"
  --runtime-json "$runtime_json"
  --runtime-jsonl "$runtime_jsonl"
  --runtime-interval-ms "$runtime_interval_ms"
)
if [[ "$fps_limit" -eq 1 ]]; then
  native_args+=(--target-fps "$target_fps")
else
  native_args+=(--no-fps-limit)
fi
if [[ "$allow_foreground_layer" -eq 1 ]]; then
  native_args+=(--allow-foreground-layer)
fi
if [[ -n "$output_name" ]]; then
  native_args+=(--output-name "$output_name")
fi

cat >"$metadata_txt" <<EOF
display: ${wayland_display}
sample_duration: ${sample_duration}
sample_interval: ${sample_interval}
runtime_interval_ms: ${runtime_interval_ms}
target_fps: $([[ "$fps_limit" -eq 1 ]] && printf '%s' "$target_fps" || printf unlimited)
layer: ${layer}
output_name: ${output_name:-compositor-selected}
color: ${color}
manual_linux_dmabuf_attach: no
legacy_waylandsink: no
EOF

env WAYLAND_DISPLAY="$wayland_display" "$exe" "${native_args[@]}" >"$stdout_log" 2>"$stderr_log" &
app_pid=$!

printf 'elapsed_s,pid,rss_kib,ps_rss_kib,vsz_kib,private_dirty_kib,shared_dirty_kib,private_clean_kib,shared_clean_kib,swap_kib,cpu_percent\n' >"$samples_csv"

elapsed=0
while kill -0 "$app_pid" 2>/dev/null; do
  rss=0
  private_dirty=0
  shared_dirty=0
  private_clean=0
  shared_clean=0
  swap=0
  if [[ -r "/proc/$app_pid/smaps_rollup" ]]; then
    read -r rss private_dirty shared_dirty private_clean shared_clean swap < <(
      awk '
        /^Rss:/ { rss=$2 }
        /^Private_Dirty:/ { private_dirty=$2 }
        /^Shared_Dirty:/ { shared_dirty=$2 }
        /^Private_Clean:/ { private_clean=$2 }
        /^Shared_Clean:/ { shared_clean=$2 }
        /^Swap:/ { swap=$2 }
        END { printf "%d %d %d %d %d %d\n", rss, private_dirty, shared_dirty, private_clean, shared_clean, swap }
      ' "/proc/$app_pid/smaps_rollup"
    )
  fi
  ps_line="$(ps -o rss=,vsz=,%cpu= -p "$app_pid" 2>/dev/null || true)"
  ps_rss=0
  vsz=0
  cpu=0
  if [[ -n "$ps_line" ]]; then
    read -r ps_rss vsz cpu <<<"$ps_line"
  fi
  printf '%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s\n' \
    "$elapsed" "$app_pid" "$rss" "$ps_rss" "$vsz" \
    "$private_dirty" "$shared_dirty" "$private_clean" "$shared_clean" "$swap" "$cpu" \
    >>"$samples_csv"
  sleep "$sample_interval"
  elapsed=$((elapsed + sample_interval))
done

set +e
wait "$app_pid"
app_status=$?
set -e

awk -F, '
  NR > 1 {
    count++
    rss_sum += $3
    dirty_sum += $6
    cpu_sum += $11
    if ($3 > rss_max) rss_max = $3
    if ($6 > dirty_max) dirty_max = $6
  }
  END {
    printf "samples: %d\n", count
    if (count > 0) {
      printf "rss_kib_avg: %.0f\n", rss_sum / count
      printf "rss_kib_max: %.0f\n", rss_max
      printf "private_dirty_kib_avg: %.0f\n", dirty_sum / count
      printf "private_dirty_kib_max: %.0f\n", dirty_max
      printf "cpu_percent_avg: %.2f\n", cpu_sum / count
    }
  }
' "$samples_csv" >"$summary_txt"

if [[ -s "$runtime_json" ]]; then
  awk '
    function value() {
      line = $0
      sub(/^[[:space:]]*"[^"]+":[[:space:]]*/, "", line)
      gsub(/[",]/, "", line)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", line)
      return line
    }
    /"runtime_elapsed_ms":/ { print "runtime_elapsed_ms: " value() }
    /"render_calls":/ { print "render_calls: " value() }
    /"frames_rendered":/ { print "frames_rendered: " value() }
    /"frames_skipped":/ { print "frames_skipped: " value() }
    /"average_render_fps":/ { print "average_render_fps: " value() }
    /"render_duration_us_avg":/ { print "render_duration_us_avg: " value() }
    /"render_duration_us_max":/ { print "render_duration_us_max: " value() }
    /"last_render_duration_us":/ { print "last_render_duration_us: " value() }
    /"surface_suboptimal_frames":/ { print "surface_suboptimal_frames: " value() }
    /"surface_lost_skips":/ { print "surface_lost_skips: " value() }
    /"surface_outdated_skips":/ { print "surface_outdated_skips: " value() }
    /"surface_timeout_skips":/ { print "surface_timeout_skips: " value() }
    /"surface_occluded_skips":/ { print "surface_occluded_skips: " value() }
    /"surface_validation_skips":/ { print "surface_validation_skips: " value() }
    /"surface_format":/ { print "surface_format: " value() }
    /"present_mode":/ { print "present_mode: " value() }
    /"last_render_error":/ { print "last_render_error: " value() }
  ' "$runtime_json" >"$runtime_summary_txt"
fi

if [[ "$app_status" -ne 0 ]]; then
  echo "FAIL: gilder-native-wgpu exited with $app_status" >&2
  echo "stderr: $stderr_log" >&2
  echo "evidence: $work_dir" >&2
  exit "$app_status"
fi

echo "PASS: native wgpu smoke completed"
echo "evidence: $work_dir"
echo "metadata: $metadata_txt"
echo "samples:  $samples_csv"
echo "summary:  $summary_txt"
echo "runtime:  $runtime_json"
echo "runtime samples: $runtime_jsonl"
echo "runtime summary: $runtime_summary_txt"
