#!/usr/bin/env bash
set -euo pipefail

sudo_cmd=()
if [[ "$(id -u)" -ne 0 ]]; then
  sudo_cmd=(sudo)
fi

"${sudo_cmd[@]}" apt-get install -y \
  ffmpeg \
  libwayland-bin \
  libwayland-dev \
  libxkbcommon-dev \
  pkg-config \
  wayland-protocols

required_tools=(
  ffmpeg
  pkg-config
)

for tool in "${required_tools[@]}"; do
  command -v "$tool"
done
