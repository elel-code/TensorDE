#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
usage: scripts/video-codec-smoke.sh [options]

Generate small MP4/WebM samples, verify that GStreamer can decode them through
playbin, and verify that gilder-convert can create first-frame previews.

Options:
  --work-dir <dir>    Parent directory for temporary smoke data
  --report-dir <dir>  Exact evidence directory. Created and kept
  --allow-missing     Report missing encoders/plugins as skips instead of failures
  --no-convert        Skip gilder-convert preview checks
  --keep              Keep generated smoke data
  -h, --help          Show this help text
EOF
}

work_parent="${TMPDIR:-/tmp}"
report_dir=""
allow_missing=0
run_convert=1
keep=0
size="96x54"
rate="6"
duration="0.5"

while [[ $# -gt 0 ]]; do
  case "$1" in
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
    --no-convert)
      run_convert=0
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

if [[ -n "$report_dir" ]]; then
  work_dir="$report_dir"
  mkdir -p "$work_dir"
else
  mkdir -p "$work_parent"
  work_dir="$(mktemp -d "${work_parent%/}/gilder-video-codecs.XXXXXX")"
fi
if [[ "$keep" -eq 0 && -z "$report_dir" ]]; then
  trap 'rm -rf "$work_dir"' EXIT
fi

failures=0
skips=0
passes=0
results_path="$work_dir/results.csv"
metadata_path="$work_dir/metadata.txt"
summary_path="$work_dir/summary.txt"

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

record_result() {
  local case_name="$1"
  local step="$2"
  local status="$3"
  local detail="$4"
  local artifact="$5"

  csv_escape "$case_name" >> "$results_path"
  printf ',' >> "$results_path"
  csv_escape "$step" >> "$results_path"
  printf ',' >> "$results_path"
  csv_escape "$status" >> "$results_path"
  printf ',' >> "$results_path"
  csv_escape "$detail" >> "$results_path"
  printf ',' >> "$results_path"
  csv_escape "${artifact#$work_dir/}" >> "$results_path"
  printf '\n' >> "$results_path"
}

record_failure() {
  failures=$((failures + 1))
  note "FAIL: $*"
}

record_skip() {
  skips=$((skips + 1))
  note "SKIP: $*"
}

record_pass() {
  passes=$((passes + 1))
  note "PASS: $*"
}

write_summary() {
  cat > "$summary_path" <<EOF
passed: ${passes}
skipped: ${skips}
failed: ${failures}
results: ${results_path}
EOF
}

tool_hint() {
  case "$1" in
    ffmpeg)
      printf '%s\n' "install the ffmpeg package"
      ;;
    gst-launch-1.0)
      printf '%s\n' "install the gstreamer1.0-tools package"
      ;;
    cargo)
      printf '%s\n' "install Rust/Cargo before running converter checks"
      ;;
  esac
}

missing_tool() {
  local message="$1 is not available"
  local hint
  hint="$(tool_hint "$1")"
  if [[ -n "$hint" ]]; then
    message="${message}; ${hint}"
  fi

  if [[ "$allow_missing" -eq 1 ]]; then
    record_skip "$message"
    record_result "environment" "tool-check" "skip" "$message" ""
    return 0
  fi
  record_failure "$message"
  record_result "environment" "tool-check" "fail" "$message" ""
  return 0
}

ffmpeg="$(command -v ffmpeg || true)"
gst_launch="$(command -v gst-launch-1.0 || true)"
cargo_bin="$(command -v cargo || true)"

printf 'case,step,status,detail,artifact\n' > "$results_path"

[[ -n "$ffmpeg" ]] || missing_tool ffmpeg
[[ -n "$gst_launch" ]] || missing_tool gst-launch-1.0
if [[ "$run_convert" -eq 1 ]]; then
  [[ -n "$cargo_bin" ]] || missing_tool cargo
fi

if [[ "$run_convert" -eq 1 && -z "$cargo_bin" ]]; then
  run_convert=0
fi

cat > "$metadata_path" <<EOF
size: ${size}
rate: ${rate}
duration_seconds: ${duration}
allow_missing: ${allow_missing}
run_convert: ${run_convert}
ffmpeg: ${ffmpeg:-unavailable}
gst_launch: ${gst_launch:-unavailable}
cargo: ${cargo_bin:-unavailable}
EOF

if [[ -z "$ffmpeg" ]]; then
  write_summary
  note "summary: ${passes} passed, ${skips} skipped, ${failures} failed"
  note "metadata: $metadata_path"
  note "results:  $results_path"
  note "report:   $summary_path"
  exit "$([[ "$failures" -gt 0 ]] && echo 1 || echo 0)"
fi

if [[ "$failures" -gt 0 ]]; then
  write_summary
  exit 1
fi

has_ffmpeg_encoder() {
  "$ffmpeg" -hide_banner -encoders 2>/dev/null | grep -Eq "[[:space:]]$1[[:space:]]"
}

run_with_timeout() {
  if command -v timeout >/dev/null 2>&1; then
    timeout 20s "$@"
  else
    "$@"
  fi
}

encode_sample() {
  local name="$1"
  local output="$2"
  local input="testsrc2=size=${size}:rate=${rate}:duration=${duration}"

  case "$name" in
    mp4-h264)
      has_ffmpeg_encoder libx264 || return 10
      run_with_timeout "$ffmpeg" -hide_banner -loglevel error -y \
        -f lavfi -i "$input" \
        -an -c:v libx264 -preset ultrafast -tune zerolatency -pix_fmt yuv420p \
        "$output"
      ;;
    webm-vp9)
      has_ffmpeg_encoder libvpx-vp9 || return 10
      run_with_timeout "$ffmpeg" -hide_banner -loglevel error -y \
        -f lavfi -i "$input" \
        -an -c:v libvpx-vp9 -deadline realtime -cpu-used 8 -b:v 0 -crf 40 -pix_fmt yuv420p \
        "$output"
      ;;
    webm-av1)
      has_ffmpeg_encoder libaom-av1 || return 10
      run_with_timeout "$ffmpeg" -hide_banner -loglevel error -y \
        -f lavfi -i "$input" \
        -an -c:v libaom-av1 -cpu-used 8 -crf 45 -b:v 0 -row-mt 1 -pix_fmt yuv420p \
        "$output"
      ;;
    *)
      echo "unknown codec smoke case: $name" >&2
      return 2
      ;;
  esac
}

play_sample() {
  local sample="$1"
  run_with_timeout "$gst_launch" -q playbin \
    uri="file://${sample}" \
    video-sink=fakesink \
    audio-sink=fakesink
}

convert_sample() {
  local name="$1"
  local sample="$2"
  local ext="$3"
  local source_dir="$work_dir/${name}-source"
  local output_dir="$work_dir/${name}.gwpdir"

  mkdir -p "$source_dir"
  cp "$sample" "$source_dir/loop.${ext}"
  cat > "$source_dir/project.json" <<EOF
{
  "type": "video",
  "title": "Gilder ${name} Smoke",
  "file": "loop.${ext}"
}
EOF

  cargo run --quiet --bin gilder-convert -- wallpaper-engine "$source_dir" "$output_dir" >/dev/null
  [[ -s "$output_dir/previews/poster.jpg" ]]
  [[ -s "$output_dir/previews/thumbnail.jpg" ]]
  grep -q '"poster": "previews/poster.jpg"' "$output_dir/manifest.gilder.json"
  grep -q '"thumbnail": "previews/thumbnail.jpg"' "$output_dir/manifest.gilder.json"
}

run_case() {
  local name="$1"
  local ext="$2"
  local sample="$work_dir/${name}.${ext}"

  note "case ${name}"
  local status=0
  encode_sample "$name" "$sample" || status=$?
  if [[ "$status" -ne 0 ]]; then
    if [[ "$status" -eq 10 && "$allow_missing" -eq 1 ]]; then
      record_skip "$name encoder is not available in ffmpeg"
      record_result "$name" "sample-generation" "skip" "encoder is not available in ffmpeg" ""
      return 0
    fi
    record_failure "$name sample generation failed"
    record_result "$name" "sample-generation" "fail" "ffmpeg sample generation failed" "$sample"
    return 0
  fi
  record_pass "$name sample generation"
  record_result "$name" "sample-generation" "pass" "generated synthetic ${ext} sample" "$sample"

  local gst_log="$work_dir/${name}.gst.log"
  if [[ -z "$gst_launch" ]]; then
    record_skip "$name GStreamer decode skipped because gst-launch-1.0 is unavailable"
    record_result "$name" "gstreamer-decode" "skip" "gst-launch-1.0 is unavailable" ""
  elif ! play_sample "$sample" >"$gst_log" 2>&1; then
    if [[ "$allow_missing" -eq 1 ]]; then
      record_skip "$name GStreamer decode failed or required plugin is missing"
      record_result "$name" "gstreamer-decode" "skip" "decode failed or required plugin is missing" "$gst_log"
    else
      record_failure "$name GStreamer decode failed"
      record_result "$name" "gstreamer-decode" "fail" "GStreamer playbin decode failed" "$gst_log"
      sed -n '1,40p' "$gst_log"
    fi
  else
    record_pass "$name GStreamer decode"
    record_result "$name" "gstreamer-decode" "pass" "GStreamer playbin decoded through fakesink" "$gst_log"
  fi

  if [[ "$run_convert" -eq 1 ]]; then
    if convert_sample "$name" "$sample" "$ext"; then
      record_pass "$name gilder-convert first-frame preview"
      record_result "$name" "gilder-convert-preview" "pass" "generated poster and thumbnail previews" "$work_dir/${name}.gwpdir"
    else
      record_failure "$name gilder-convert first-frame preview"
      record_result "$name" "gilder-convert-preview" "fail" "failed to generate poster or thumbnail previews" "$work_dir/${name}.gwpdir"
    fi
  else
    record_skip "$name gilder-convert preview skipped because --no-convert was passed"
    record_result "$name" "gilder-convert-preview" "skip" "converter check disabled by --no-convert" ""
  fi
}

note "work dir: $work_dir"
run_case mp4-h264 mp4
run_case webm-vp9 webm
run_case webm-av1 webm

write_summary
if [[ "$keep" -eq 1 || -n "$report_dir" ]]; then
  note "kept work dir: $work_dir"
else
  note "work dir will be removed; rerun with --keep or --report-dir to preserve evidence"
fi
note "metadata: $metadata_path"
note "results:  $results_path"
note "report:   $summary_path"
note "summary: ${passes} passed, ${skips} skipped, ${failures} failed"
if [[ "$failures" -gt 0 ]]; then
  exit 1
fi
