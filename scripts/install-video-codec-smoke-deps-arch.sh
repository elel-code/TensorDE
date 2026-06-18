#!/usr/bin/env bash
set -euo pipefail

sudo_cmd=()
if [[ "$(id -u)" -ne 0 ]]; then
  sudo_cmd=(sudo)
fi

if ! command -v pacman >/dev/null 2>&1; then
  echo "pacman is not available; this helper is for Arch-like systems" >&2
  exit 1
fi

"${sudo_cmd[@]}" pacman -S --needed --noconfirm \
  ffmpeg \
  gstreamer \
  gst-libav \
  gst-plugin-dav1d \
  gst-plugins-bad \
  gst-plugins-base \
  gst-plugins-good \
  gst-plugins-ugly

required_tools=(
  ffmpeg
  gst-inspect-1.0
  gst-launch-1.0
)

for tool in "${required_tools[@]}"; do
  command -v "$tool"
done

required_gstreamer_elements=(
  fakesink
  matroskademux
  playbin
  qtdemux
)

for element in "${required_gstreamer_elements[@]}"; do
  gst-inspect-1.0 "$element" >/dev/null
done

if ! gst-inspect-1.0 avdec_h264 >/dev/null 2>&1 && ! gst-inspect-1.0 openh264dec >/dev/null 2>&1; then
  echo "no H.264 decoder candidate found: expected avdec_h264 or openh264dec" >&2
  exit 1
fi

if ! gst-inspect-1.0 avdec_vp9 >/dev/null 2>&1 && ! gst-inspect-1.0 vp9dec >/dev/null 2>&1; then
  echo "no VP9 decoder candidate found: expected avdec_vp9 or vp9dec" >&2
  exit 1
fi

if ! gst-inspect-1.0 avdec_av1 >/dev/null 2>&1 && ! gst-inspect-1.0 dav1ddec >/dev/null 2>&1; then
  echo "no AV1 decoder candidate found: expected avdec_av1 or dav1ddec" >&2
  exit 1
fi
