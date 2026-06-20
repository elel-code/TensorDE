#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
usage: scripts/native-vulkan-surface-video-queue-smoke.sh [options]

Create a real native Wayland Vulkan background surface and assert that the
selected present device can also support H.265 Vulkan Video decode on the same
physical device. The smoke records whether decode can happen on the selected
present queue itself or requires same-device cross-queue synchronization.

Options:
  --display <name>      Wayland display name. Default: WAYLAND_DISPLAY.
  --output-name <name>  Target Wayland output name, for example HDMI-A-1.
  --layer <layer>       Wayland layer. Default: background.
  --report-dir <dir>    Exact evidence directory. Created and kept.
  --work-dir <dir>      Parent directory for generated evidence. Default: /tmp.
  --no-build            Reuse existing target/release/gilder-native-vulkan.
  -h, --help            Show this help text.
EOF
}

display="${WAYLAND_DISPLAY:-}"
output_name=""
layer="background"
report_dir=""
work_parent="${TMPDIR:-/tmp}"
no_build=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --display)
      display="${2:-}"
      shift 2
      ;;
    --output-name)
      output_name="${2:-}"
      shift 2
      ;;
    --layer)
      layer="${2:-}"
      shift 2
      ;;
    --report-dir)
      report_dir="${2:-}"
      shift 2
      ;;
    --work-dir)
      work_parent="${2:-}"
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
  exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
  printf 'FAIL: missing required tool: jq\n' >&2
  exit 1
fi

case "$layer" in
  background|bottom) ;;
  top|overlay)
    printf 'FAIL: foreground layer "%s" is not allowed by this smoke\n' "$layer" >&2
    exit 1
    ;;
  *)
    printf 'FAIL: unsupported layer: %s\n' "$layer" >&2
    exit 1
    ;;
esac

if [[ -z "$report_dir" ]]; then
  report_dir="$(mktemp -d "${work_parent%/}/gilder-vulkan-surface-video-queue.XXXXXX")"
else
  mkdir -p "$report_dir"
fi
mkdir -p "$report_dir"

if [[ "$no_build" -eq 0 ]]; then
  cargo build --release --features native-vulkan-gst-video --bin gilder-native-vulkan
fi

probe_json="$report_dir/probe-surface.json"
probe_stderr="$report_dir/probe-surface.stderr"
summary="$report_dir/summary.txt"
args=(--probe-surface --layer "$layer" --wait-roundtrips 1)
if [[ -n "$output_name" ]]; then
  args+=(--output-name "$output_name")
fi

set +e
env WAYLAND_DISPLAY="$display" \
  target/release/gilder-native-vulkan \
  "${args[@]}" \
  >"$probe_json" 2>"$probe_stderr"
probe_status=$?
set -e

if [[ "$probe_status" -ne 0 ]]; then
  printf 'FAIL: native Vulkan surface/video queue probe failed\n' | tee "$summary"
  printf 'stderr: %s\n' "$probe_stderr" | tee -a "$summary"
  sed -n '1,120p' "$probe_stderr" >&2
  exit "$probe_status"
fi

selected_device="$(jq -r '.selected_physical_device_name // "none"' "$probe_json")"
present_queue="$(jq -r '.selected_queue_family_index // "none"' "$probe_json")"
present_graphics="$(jq -r '.selected_queue_supports_graphics // false' "$probe_json")"
has_video_queue_ext="$(jq -r '.selected_device_has_video_queue_extension // false' "$probe_json")"
has_video_decode_ext="$(jq -r '.selected_device_has_video_decode_queue_extension // false' "$probe_json")"
has_h265_ext="$(jq -r '.selected_device_has_h265_decode_extension // false' "$probe_json")"
selected_h265="$(jq -r '.selected_queue_supports_h265_decode // false' "$probe_json")"
h265_queue="$(jq -r '.same_device_h265_decode_queue_family_index // "none"' "$probe_json")"
cross_queue="$(jq -r '.h265_decode_requires_cross_queue_sync // false' "$probe_json")"

if [[ "$selected_device" == "none" || "$present_queue" == "none" || "$present_graphics" != "true" ]]; then
  {
    printf 'FAIL: native Vulkan surface did not select a graphics/present queue\n'
    printf 'selected_device: %s\n' "$selected_device"
    printf 'present_queue: %s\n' "$present_queue"
    printf 'present_graphics: %s\n' "$present_graphics"
    printf 'probe JSON: %s\n' "$probe_json"
  } | tee "$summary"
  exit 1
fi

if [[ "$has_video_queue_ext" != "true" || "$has_video_decode_ext" != "true" || "$has_h265_ext" != "true" || "$h265_queue" == "none" ]]; then
  {
    printf 'FAIL: selected present device does not expose same-device H.265 Vulkan Video decode\n'
    printf 'selected_device: %s\n' "$selected_device"
    printf 'has_video_queue_extension: %s\n' "$has_video_queue_ext"
    printf 'has_video_decode_queue_extension: %s\n' "$has_video_decode_ext"
    printf 'has_h265_decode_extension: %s\n' "$has_h265_ext"
    printf 'same_device_h265_decode_queue: %s\n' "$h265_queue"
    printf 'probe JSON: %s\n' "$probe_json"
  } | tee "$summary"
  exit 1
fi

if [[ "$selected_h265" != "true" && "$cross_queue" != "true" ]]; then
  {
    printf 'FAIL: H.265 decode is not on the selected queue and cross-queue sync was not detected\n'
    printf 'present_queue: %s\n' "$present_queue"
    printf 'same_device_h265_decode_queue: %s\n' "$h265_queue"
    printf 'selected_queue_supports_h265_decode: %s\n' "$selected_h265"
    printf 'h265_decode_requires_cross_queue_sync: %s\n' "$cross_queue"
    printf 'probe JSON: %s\n' "$probe_json"
  } | tee "$summary"
  exit 1
fi

{
  printf 'selected_device: %s\n' "$selected_device"
  printf 'selected_device_type: %s\n' "$(jq -r '.selected_physical_device_type // "none"' "$probe_json")"
  printf 'requested_output_name: %s\n' "${output_name:-auto}"
  printf 'surface_logical_size: %s\n' "$(jq -c '.wayland_surface_logical_size' "$probe_json")"
  printf 'surface_buffer_size: %s\n' "$(jq -c '.wayland_surface_buffer_size' "$probe_json")"
  printf 'present_queue_family_index: %s\n' "$present_queue"
  printf 'present_queue_flags: %s\n' "$(jq -c '.selected_queue_flags' "$probe_json")"
  printf 'present_queue_video_codec_operations: %s\n' "$(jq -c '.selected_queue_video_codec_operations' "$probe_json")"
  printf 'selected_queue_supports_h265_decode: %s\n' "$selected_h265"
  printf 'same_device_h265_decode_queue_family_index: %s\n' "$h265_queue"
  printf 'same_device_h265_decode_queue_flags: %s\n' "$(jq -c '.same_device_h265_decode_queue_flags' "$probe_json")"
  printf 'same_device_h265_decode_queue_video_codec_operations: %s\n' "$(jq -c '.same_device_h265_decode_queue_video_codec_operations' "$probe_json")"
  printf 'h265_decode_requires_cross_queue_sync: %s\n' "$cross_queue"
  printf 'selected_device_decode_codec_extensions: %s\n' "$(jq -c '.selected_device_decode_codec_extensions' "$probe_json")"
} >"$summary"

printf 'PASS: native Vulkan surface/video queue smoke passed\n'
printf 'summary: %s\n' "$summary"
printf 'probe JSON: %s\n' "$probe_json"
