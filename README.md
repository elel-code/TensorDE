# Gilder

Gilder is a native Wayland wallpaper engine for independent compositors such as
niri and Hyprland. The project aims to cover the gap
between simple static wallpaper tools and richer Wallpaper Engine style
packages on Linux.

Current status: daemon IPC, state persistence, wallpaper planning, and
feature-gated native Wayland/Vulkan renderer paths. Video work uses an FFmpeg
demux/bitstream frontend feeding Vulkanalia/Vulkan Video decode/render/present;
legacy decoded-frame and display-sink paths have been removed.

## Project Layout

- `src/core.rs`: core module entry and re-exports.
- `src/core/`: wallpaper package format primitives.
- `src/ipc.rs`: IPC module entry and re-exports.
- `src/ipc/`: command, protocol, and socket helpers.
- `src/bin/gilderd.rs`: daemon entry point for IPC, state, and renderer updates.
- `src/bin/gilderctl.rs`: CLI client for daemon control.
- `src/bin/gilder-convert.rs`: conversion tool for Wallpaper Engine projects.
- `docs/native-vulkan-video.md`: current FFmpeg/Vulkan Video status,
  hard gates, and H.264/H.265/AV1 evidence.
- `docs/packaging.md`: packaging asset install notes.
- `docs/man/`: man pages for the command line tools.
- `completions/`: bash and zsh shell completions.
- `packaging/systemd/`: systemd user service example.
- `scripts/native-vulkan-h264-ready-prefix-video-smoke.sh`: real Wayland native
  Vulkan H.264 decode/present evidence helper.
- `scripts/native-vulkan-av1-ready-prefix-video-smoke.sh`: real Wayland native
  Vulkan AV1 decode/present evidence helper.
- `scripts/native-vulkan-h265-ready-prefix-video-smoke.sh`: real Wayland native
  Vulkan H.265 decode/present evidence helper.
- `scripts/native-vulkan-surface-video-queue-smoke.sh`: native Vulkan surface
  queue helper.
- `scripts/performance-snapshot.sh`: daemon CPU/RSS/PSS/USS/status sampling
  helper.
- `scripts/desktop-policy-smoke.sh`: headless desktop-state performance policy
  validation helper.

## Early commands

```sh
cargo check
cargo check --features native-vulkan-renderer
cargo check --features native-vulkan-video --bin gilder-native-vulkan
cargo run --bin gilderd
cargo run --bin gilderctl -- ping
cargo run --bin gilderctl -- outputs
cargo run --bin gilderctl -- watch
cargo run --bin gilderctl -- set ./examples/wallpapers/static-demo.gwpdir --output eDP-1
cargo run --bin gilderctl -- properties set speed 0.5 --output eDP-1
cargo run --bin gilder-convert -- wallpaper-engine /path/to/we/project ./out.gwpdir
cargo run --bin gilder-convert -- pack ./examples/wallpapers/static-demo.gwpdir ./static-demo.gwp
```

Distribution assets are documented in `docs/packaging.md`. The repository ships
example systemd user service, man pages, and shell completions, but install
paths are intentionally left to distro/user packaging. A tarball staging helper
is available at `packaging/build-dist.sh`.

`.gwpdir` packages can use either `manifest.gilder.json` or authoring-friendly
`manifest.gilder.toml`; `.gwp` archives are packed with canonical
`manifest.gilder.json`.

The daemon currently provides JSON-RPC over a Unix socket, persistent state, and
policy decisions for desktop-state-based throttling. Native Vulkan video status
and evidence are tracked in `docs/native-vulkan-video.md`.

The optional `native-vulkan-video` feature builds the native Wayland/Vulkan
video helper. FFmpeg owns container parsing and parser-normalized bitstream
handoff; Vulkanalia/native Vulkan owns decode, render and present.
