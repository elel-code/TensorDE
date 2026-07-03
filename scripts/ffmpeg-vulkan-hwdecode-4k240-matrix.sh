#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
usage: scripts/ffmpeg-vulkan-hwdecode-4k240-matrix.sh [options]

Run the FFmpeg Vulkan HW decode mainline against 4K/240 sources and collect
DMS/dgop memory plus present FPS telemetry.

Options:
  --display <name>             Wayland display. Default: WAYLAND_DISPLAY.
  --output-name <name>         Target Wayland output name.
  --output <name>              Alias for --output-name.
  --label <name>               Result label. Default: matrix.
  --work-dir <dir>             Result directory. Default: /tmp.
  --codecs <list>              Comma list. Default: h264,h265-main8,h265-main10,av1-main8,av1-main10.
  --frames <count>             Playback frames. Default: 2400.
  --target-fps <fps>           Target present FPS. Default: 240.
  --present-mode-policy <mode> Set GILDER_VULKAN_PRESENT_MODE_POLICY for trials:
                               default, fifo, fifo-relaxed, fifo-latest-ready, mailbox, immediate.
  --wait-after-present         Set GILDER_VULKAN_PRESENT_WAIT_AFTER_PRESENT=1.
  --sample-interval <sec>      dgop sampling interval. Default: 0.1.
  --no-build                   Reuse existing target/release/gilder-native-vulkan.
  -h, --help                   Show this help.
EOF
}

display="${WAYLAND_DISPLAY:-}"
output_name=""
label="matrix"
work_dir="${TMPDIR:-/tmp}"
codec_list="h264,h265-main8,h265-main10,av1-main8,av1-main10"
frames=2400
target_fps=240
present_mode_policy=""
wait_after_present=0
sample_interval="0.1"
no_build=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --display)
      display="${2:-}"
      shift 2
      ;;
    --output-name|--output)
      output_name="${2:-}"
      shift 2
      ;;
    --label)
      label="${2:-}"
      shift 2
      ;;
    --work-dir)
      work_dir="${2:-}"
      shift 2
      ;;
    --codecs)
      codec_list="${2:-}"
      shift 2
      ;;
    --frames)
      frames="${2:-}"
      shift 2
      ;;
    --target-fps)
      target_fps="${2:-}"
      shift 2
      ;;
    --present-mode-policy)
      present_mode_policy="${2:-}"
      shift 2
      ;;
    --wait-after-present)
      wait_after_present=1
      shift
      ;;
    --sample-interval)
      sample_interval="${2:-}"
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
      printf 'unknown option: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "$display" ]]; then
  printf 'FAIL: WAYLAND_DISPLAY is empty; pass --display\n' >&2
  exit 2
fi
for number in "$frames" "$target_fps"; do
  if [[ ! "$number" =~ ^[0-9]+$ || "$number" -lt 1 ]]; then
    printf 'FAIL: --frames and --target-fps must be positive integers\n' >&2
    exit 2
  fi
done

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
cd "$repo_root"

if [[ "$no_build" -ne 1 ]]; then
  cargo build --release --features native-vulkan-video --bin gilder-native-vulkan
fi

mkdir -p "$work_dir"
matrix_csv="${work_dir}/gilder-ffmpeg-vulkan-4k240-${label}-matrix.csv"
printf 'codec,status,max_memory_kb,last_memory_kb,average_present_fps,average_present_teardown_inclusive_fps,presented_frame_count,all_zero_copy_presented,present_mode,present_delta_over_6250us_count,present_delta_over_8334us_count,frame_sleep_count,total_pacing_sleep_micros,telemetry\n' > "$matrix_csv"

codec_source() {
  case "$1" in
    h264)
      printf '%s\n' 'artifacts/video-sources/h264/h264-high-b0-ref2-weightp0-weightb0-3840x2160-240fps-2402frames-g2401-d2400.mp4'
      ;;
    h265-main8)
      printf '%s\n' 'artifacts/video-sources/h265/h265-main-8-b0-ref1-3840x2160-240fps-2402frames-g240-d2400.mp4'
      ;;
    h265-main10)
      printf '%s\n' 'artifacts/video-sources/h265/h265-main-10-b0-ref1-3840x2160-240fps-566frames-g240-d240.mp4'
      ;;
    av1-main8)
      printf '%s\n' 'artifacts/video-sources/av1/av1-main8-3840x2160-240fps-566frames-g240.webm'
      ;;
    av1-main10)
      printf '%s\n' 'artifacts/video-sources/av1/av1-main10-3840x2160-240fps-566frames-g240.webm'
      ;;
    *)
      return 1
      ;;
  esac
}

codec_cli_name() {
  case "$1" in
    h264) printf '%s\n' 'h264' ;;
    h265-main8) printf '%s\n' 'h265' ;;
    h265-main10) printf '%s\n' 'h265-main-10' ;;
    av1-main8) printf '%s\n' 'av1' ;;
    av1-main10) printf '%s\n' 'av1-main-10' ;;
    *) return 1 ;;
  esac
}

run_one() {
  local codec="$1"
  local source cli prefix out err csv rollup smaps summary status sample captured start_ns pid
  source="$(codec_source "$codec")" || {
    printf 'FAIL: unsupported codec label: %s\n' "$codec" >&2
    return 2
  }
  cli="$(codec_cli_name "$codec")"
  if [[ ! -f "$source" ]]; then
    printf 'FAIL: source missing for %s: %s\n' "$codec" "$source" >&2
    return 1
  fi

  prefix="${work_dir}/gilder-ffmpeg-vulkan-4k240-${label}-${codec}"
  out="${prefix}-telemetry.json"
  err="${prefix}.stderr"
  csv="${prefix}-dgop.csv"
  rollup="${prefix}-smaps-rollup.txt"
  smaps="${prefix}-smaps.txt"
  summary="${prefix}-summary.txt"
  rm -f "$out" "$err" "$csv" "$rollup" "$smaps" "$summary"
  printf "sample,elapsed_ms,pid,memory_kb,memory_calculation,rss_kb,pss_kb,pss_dirty_kb,anonymous_kb,command\n" > "$csv"

  local cmd=(target/release/gilder-native-vulkan
    --run-video
    --source "$source"
    --video-codec "$cli"
    --width 3840
    --height 2160
    --target-fps "$target_fps"
    --playback-frames "$frames"
    --layer bottom
    --wait-roundtrips 2)
  if [[ -n "$output_name" ]]; then
    cmd+=(--output-name "$output_name")
  fi

  start_ns=$(date +%s%N)
  (
    export WAYLAND_DISPLAY="$display"
    export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
    if [[ -n "$present_mode_policy" && "$present_mode_policy" != "default" ]]; then
      export GILDER_VULKAN_PRESENT_MODE_POLICY="$present_mode_policy"
    fi
    if [[ "$wait_after_present" -eq 1 ]]; then
      export GILDER_VULKAN_PRESENT_WAIT_AFTER_PRESENT=1
    fi
    "${cmd[@]}"
  ) > "$out" 2> "$err" &
  pid=$!
  sample=0
  captured=0
  sleep "$sample_interval"
  while kill -0 "$pid" 2>/dev/null; do
    local now_ns elapsed_ms row
    now_ns=$(date +%s%N)
    elapsed_ms=$(((now_ns - start_ns) / 1000000))
    row=""
    if command -v dgop >/dev/null 2>&1; then
      row=$(
        dgop processes --json --no-cpu --limit 0 --sort memory 2>/dev/null |
          jq -r --argjson pid "$pid" '
            .processes[]
            | select(.pid == $pid or (.executablePath // "" | endswith("/target/release/gilder-native-vulkan")))
            | [
                .memoryKB,
                .memoryCalculation,
                .rssKB,
                .pssKB,
                (.pssDirtyKB // .pss_dirty_kb // .pssDirtyKb // 0),
                (.anonymousKB // .anonymous_kb // .anonymousKb // 0),
                .command
              ]
            | @csv
          ' |
          head -n 1
      )
    fi
    if [[ -n "$row" ]]; then
      printf "%s,%s,%s,%s\n" "$sample" "$elapsed_ms" "$pid" "$row" >> "$csv"
    fi
    if (( captured == 0 && elapsed_ms > 3000 )); then
      [[ -r "/proc/${pid}/smaps_rollup" ]] && sed -n '1,200p' "/proc/${pid}/smaps_rollup" > "$rollup" || true
      [[ -r "/proc/${pid}/smaps" ]] && sed -n '1,20000p' "/proc/${pid}/smaps" > "$smaps" || true
      captured=1
    fi
    sample=$((sample + 1))
    sleep "$sample_interval"
  done

  set +e
  wait "$pid"
  status=$?
  set -e

  awk -F, '
    NR == 1 { next }
    {
      gsub(/"/, "", $5)
      samples++
      memory = $4 + 0
      if (samples == 1 || memory > max_memory) max_memory = memory
      last_memory = memory
      calc = $5
    }
    END {
      printf "status: %d\n", status
      printf "codec: %s\n", codec
      printf "source: %s\n", source
      printf "samples: %d\n", samples
      printf "memory_calculation: %s\n", calc
      printf "max_memory_kb: %d\n", max_memory
      printf "last_memory_kb: %d\n", last_memory
    }
  ' status="$status" codec="$codec" source="$source" "$csv" > "$summary"

  if [[ -s "$out" ]]; then
    jq -r '
      .decoded_image_present_sequence as $seq
      | .device.swapchain as $swap
      | [
          "average_present_fps: \($seq.average_present_fps // 0)",
          "average_present_teardown_inclusive_fps: \($seq.average_present_teardown_inclusive_fps // 0)",
          "presented_frame_count: \($seq.presented_frame_count // 0)",
          "all_zero_copy_presented: \($seq.all_zero_copy_presented // false)",
          "present_mode: \($swap.present_mode // "unknown")",
          "present_delta_over_6250us_count: \($seq.present_delta_over_6250us_count // 0)",
          "present_delta_over_8334us_count: \($seq.present_delta_over_8334us_count // 0)",
          "frame_sleep_count: \($seq.frame_sleep_count // 0)",
          "total_pacing_sleep_micros: \($seq.total_pacing_sleep_micros // 0)"
        ]
      | .[]
    ' "$out" >> "$summary" || true
    if ! jq -e '.decoded_image_present_sequence.presented_frame_count == .requested_present_frame_count and .decoded_image_zero_copy_presented == true' "$out" >/dev/null 2>&1; then
      status=1
      printf 'matrix_status: failed-zero-copy-present-contract\n' >> "$summary"
    fi
  else
    status=1
    printf 'matrix_status: missing-runtime-json\n' >> "$summary"
  fi

  local max_memory last_memory fps fps_inclusive presented zero_copy present_mode over6250 over8334 sleep_count sleep_micros
  max_memory=$(awk -F': ' '$1 == "max_memory_kb" { print $2 }' "$summary")
  last_memory=$(awk -F': ' '$1 == "last_memory_kb" { print $2 }' "$summary")
  fps=$(awk -F': ' '$1 == "average_present_fps" { print $2 }' "$summary")
  fps_inclusive=$(awk -F': ' '$1 == "average_present_teardown_inclusive_fps" { print $2 }' "$summary")
  presented=$(awk -F': ' '$1 == "presented_frame_count" { print $2 }' "$summary")
  zero_copy=$(awk -F': ' '$1 == "all_zero_copy_presented" { print $2 }' "$summary")
  present_mode=$(awk -F': ' '$1 == "present_mode" { print $2 }' "$summary")
  over6250=$(awk -F': ' '$1 == "present_delta_over_6250us_count" { print $2 }' "$summary")
  over8334=$(awk -F': ' '$1 == "present_delta_over_8334us_count" { print $2 }' "$summary")
  sleep_count=$(awk -F': ' '$1 == "frame_sleep_count" { print $2 }' "$summary")
  sleep_micros=$(awk -F': ' '$1 == "total_pacing_sleep_micros" { print $2 }' "$summary")
  printf '%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s\n' \
    "$codec" "$status" "${max_memory:-0}" "${last_memory:-0}" \
    "${fps:-0}" "${fps_inclusive:-0}" "${presented:-0}" "${zero_copy:-false}" \
    "${present_mode:-unknown}" "${over6250:-0}" "${over8334:-0}" \
    "${sleep_count:-0}" "${sleep_micros:-0}" "$out" >> "$matrix_csv"
  printf '%s\n' "$summary"
  return "$status"
}

IFS=',' read -r -a codecs <<< "$codec_list"
overall_status=0
for codec in "${codecs[@]}"; do
  codec="${codec// /}"
  [[ -z "$codec" ]] && continue
  if ! run_one "$codec"; then
    overall_status=1
  fi
done

printf 'matrix: %s\n' "$matrix_csv"
exit "$overall_status"
