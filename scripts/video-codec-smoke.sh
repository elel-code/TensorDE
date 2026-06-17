#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
usage: scripts/video-codec-smoke.sh [options]

Generate small MP4/WebM samples, verify that GStreamer can decode them through
playbin, and verify that gilder-convert can create first-frame previews.

Options:
  --work-dir <dir>    Parent directory for temporary smoke data
  --allow-missing     Report missing encoders/plugins as skips instead of failures
  --no-convert        Skip gilder-convert preview checks
  --keep              Keep generated smoke data
  -h, --help          Show this help text
EOF
}

work_parent="${TMPDIR:-/tmp}"
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

mkdir -p "$work_parent"
work_dir="$(mktemp -d "${work_parent%/}/gilder-video-codecs.XXXXXX")"
if [[ "$keep" -eq 0 ]]; then
  trap 'rm -rf "$work_dir"' EXIT
fi

failures=0
skips=0
passes=0

note() {
  printf '%s\n' "$*"
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

missing_tool() {
  if [[ "$allow_missing" -eq 1 ]]; then
    record_skip "$1 is not available"
    return 0
  fi
  record_failure "$1 is not available"
  return 0
}

ffmpeg="$(command -v ffmpeg || true)"
gst_launch="$(command -v gst-launch-1.0 || true)"
cargo_bin="$(command -v cargo || true)"

[[ -n "$ffmpeg" ]] || missing_tool ffmpeg
[[ -n "$gst_launch" ]] || missing_tool gst-launch-1.0
if [[ "$run_convert" -eq 1 ]]; then
  [[ -n "$cargo_bin" ]] || missing_tool cargo
fi

if [[ -z "$ffmpeg" ]]; then
  note "summary: ${passes} passed, ${skips} skipped, ${failures} failed"
  exit 0
fi

if [[ "$run_convert" -eq 1 && -z "$cargo_bin" ]]; then
  run_convert=0
fi

if [[ "$failures" -gt 0 ]]; then
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
      return 0
    fi
    record_failure "$name sample generation failed"
    return 0
  fi
  record_pass "$name sample generation"

  local gst_log="$work_dir/${name}.gst.log"
  if [[ -z "$gst_launch" ]]; then
    record_skip "$name GStreamer decode skipped because gst-launch-1.0 is unavailable"
  elif ! play_sample "$sample" >"$gst_log" 2>&1; then
    if [[ "$allow_missing" -eq 1 ]]; then
      record_skip "$name GStreamer decode failed or required plugin is missing"
    else
      record_failure "$name GStreamer decode failed"
      sed -n '1,40p' "$gst_log"
    fi
  else
    record_pass "$name GStreamer decode"
  fi

  if [[ "$run_convert" -eq 1 ]]; then
    if convert_sample "$name" "$sample" "$ext"; then
      record_pass "$name gilder-convert first-frame preview"
    else
      record_failure "$name gilder-convert first-frame preview"
    fi
  fi
}

note "work dir: $work_dir"
run_case mp4-h264 mp4
run_case webm-vp9 webm
run_case webm-av1 webm

note "summary: ${passes} passed, ${skips} skipped, ${failures} failed"
if [[ "$failures" -gt 0 ]]; then
  exit 1
fi
