#!/usr/bin/env bash
set -euo pipefail

sudo_cmd=()
if [[ "$(id -u)" -ne 0 ]]; then
  sudo_cmd=(sudo)
fi

"$(dirname "${BASH_SOURCE[0]}")/install-video-codec-smoke-deps-ubuntu.sh"

"${sudo_cmd[@]}" apt-get install -y \
  libgstreamer-plugins-base1.0-dev \
  libgstreamer1.0-dev \
  libwayland-bin \
  libwayland-dev \
  libxkbcommon-dev \
  pkg-config \
  wayland-protocols

required_tools=(
  ffmpeg
  gst-launch-1.0
  gst-inspect-1.0
  pkg-config
)

for tool in "${required_tools[@]}"; do
  command -v "$tool"
done
