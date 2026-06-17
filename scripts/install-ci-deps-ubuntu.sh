#!/usr/bin/env bash
set -euo pipefail

sudo_cmd=()
if [[ "$(id -u)" -ne 0 ]]; then
  sudo_cmd=(sudo)
fi

"$(dirname "${BASH_SOURCE[0]}")/install-video-codec-smoke-deps-ubuntu.sh"

"${sudo_cmd[@]}" apt-get install -y \
  git \
  libgstreamer-plugins-base1.0-dev \
  libgstreamer1.0-dev \
  libgtk-4-dev \
  libwayland-bin \
  libwayland-dev \
  meson \
  ninja-build \
  pkg-config \
  wayland-protocols

gtk_layer_shell_root="$(mktemp -d "${TMPDIR:-/tmp}/gilder-gtk4-layer-shell.XXXXXX")"
trap 'rm -rf "$gtk_layer_shell_root"' EXIT

git clone --depth 1 https://github.com/wmww/gtk4-layer-shell.git "$gtk_layer_shell_root/src"
meson setup "$gtk_layer_shell_root/build" "$gtk_layer_shell_root/src" \
  --prefix=/usr \
  -Dexamples=false \
  -Ddocs=false \
  -Dintrospection=false \
  -Dvapi=false
"${sudo_cmd[@]}" meson install -C "$gtk_layer_shell_root/build"
"${sudo_cmd[@]}" ldconfig

required_tools=(
  ffmpeg
  gst-launch-1.0
  gst-inspect-1.0
  meson
  pkg-config
)

for tool in "${required_tools[@]}"; do
  command -v "$tool"
done
