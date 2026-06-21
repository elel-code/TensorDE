# Gilder

Gilder is a native Wayland wallpaper engine for independent compositors such as
niri and Hyprland. The project aims to cover the gap
between simple static wallpaper tools and richer Wallpaper Engine style
packages on Linux.

Current status: daemon IPC, state persistence, wallpaper planning, and
feature-gated native Wayland/Vulkan renderer paths. Video work is converging on
GStreamer demux/parser/appsink feeding native Vulkan import/decode/render; the
old GTK and native waylandsink/playbin display paths have been removed.

## Project Layout

- `src/core.rs`: core module entry and re-exports.
- `src/core/`: wallpaper package format primitives.
- `src/ipc.rs`: IPC module entry and re-exports.
- `src/ipc/`: command, protocol, and socket helpers.
- `src/bin/gilderd.rs`: daemon entry point for IPC, state, and renderer updates.
- `src/bin/gilderctl.rs`: CLI client for daemon control.
- `src/bin/gilder-convert.rs`: conversion tool for Wallpaper Engine projects.
- `docs/design.md`: system design.
- `docs/format.md`: Gilder wallpaper package format.
- `docs/conversion.md`: Wallpaper Engine conversion plan.
- `docs/ipc.md`: local IPC protocol.
- `docs/packaging.md`: packaging asset install notes.
- `docs/video-validation.md`: video codec smoke validation notes.
- `docs/wallpaper-types.md`: wallpaper type support matrix and runtime gaps.
- `docs/vulkan-migration.md`: renderer backend boundaries and Vulkan migration
  preparation plan.
- `docs/todo.md`: staged implementation checklist.
- `docs/man/`: man pages for the command line tools.
- `completions/`: bash and zsh shell completions.
- `packaging/systemd/`: systemd user service example.
- `scripts/video-codec-smoke.sh`: MP4/WebM codec smoke validation helper.
- `scripts/install-video-codec-smoke-deps-ubuntu.sh`: Ubuntu/Debian dependency
  helper for codec smoke validation.
- `scripts/install-video-codec-smoke-deps-arch.sh`: Arch-like dependency
  helper for codec smoke validation.
- `scripts/native-vulkan-h265-ready-prefix-video-smoke.sh`: real Wayland native
  Vulkan H.265 decode/present evidence helper.
- `scripts/native-vulkan-h265-first-frame-video-smoke.sh`: real Wayland native
  Vulkan first-frame video helper.
- `scripts/native-vulkan-surface-video-queue-smoke.sh`: native Vulkan surface
  queue helper.
- `scripts/performance-snapshot.sh`: daemon CPU/RSS/PSS/USS/status sampling
  helper.
- `scripts/desktop-policy-smoke.sh`: headless desktop-state performance policy
  validation helper.

## Early commands

```sh
cargo check
cargo check --features video-renderer
cargo check --features native-vulkan-renderer
cargo check --features native-vulkan-gst-video --bin gilder-native-vulkan
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
policy decisions for desktop-state-based throttling. Rendering, native Vulkan
integration, and Hyprland/niri output discovery are tracked in `docs/todo.md`.

The optional `video-renderer` feature builds the GStreamer controller for video
wallpaper pipeline lifecycle. It expects GStreamer 1.0 development files and
plugins from the host system.

The optional `native-vulkan-gst-video` feature builds the native Wayland/Vulkan
video helper. GStreamer owns container parsing and appsink handoff; native
Vulkan owns the GPU/display side.
