#!/usr/bin/env bash
set -euo pipefail

sudo_cmd=()
if [[ "$(id -u)" -ne 0 ]]; then
  sudo_cmd=(sudo)
fi

"${sudo_cmd[@]}" apt-get update
"${sudo_cmd[@]}" apt-get install -y \
  ffmpeg \
  gstreamer1.0-libav \
  gstreamer1.0-plugins-bad \
  gstreamer1.0-plugins-base \
  gstreamer1.0-plugins-good \
  gstreamer1.0-plugins-ugly \
  gstreamer1.0-tools

required_tools=(
  ffmpeg
  gst-launch-1.0
  gst-inspect-1.0
)

for tool in "${required_tools[@]}"; do
  command -v "$tool"
done
