# Gilder

Gilder is a planned GTK-rs based Wayland wallpaper engine for independent
compositors such as niri and Hyprland. The project aims to cover the gap
between simple static wallpaper tools and richer Wallpaper Engine style
packages on Linux.

Current status: daemon IPC, state persistence, static GTK renderer planning, and
early feature-gated GTK/GStreamer renderer paths.

## Project Layout

- `src/core.rs`: core module entry and re-exports.
- `src/core/`: wallpaper package format primitives.
- `src/ipc.rs`: IPC module entry and re-exports.
- `src/ipc/`: command, protocol, and socket helpers.
- `src/bin/gilderd.rs`: daemon entry point; later owns GTK/Wayland rendering.
- `src/bin/gilderctl.rs`: CLI client for daemon control.
- `src/bin/gilder-convert.rs`: conversion tool for Wallpaper Engine projects.
- `docs/design.md`: system design.
- `docs/format.md`: Gilder wallpaper package format.
- `docs/conversion.md`: Wallpaper Engine conversion plan.
- `docs/ipc.md`: local IPC protocol.
- `docs/todo.md`: staged implementation checklist.

## Early commands

```sh
cargo check
cargo check --features gtk-renderer
cargo check --features video-renderer
cargo check --features gtk-renderer,video-renderer
cargo run --bin gilderd
cargo run --bin gilderctl -- ping
cargo run --bin gilderctl -- outputs
cargo run --bin gilderctl -- watch
cargo run --bin gilderctl -- set ./examples/wallpapers/static-demo.gwpdir --output eDP-1
cargo run --bin gilderctl -- properties set speed 0.5 --output eDP-1
cargo run --bin gilder-convert -- wallpaper-engine /path/to/we/project ./out.gwpdir
cargo run --bin gilder-convert -- pack ./examples/wallpapers/static-demo.gwpdir ./static-demo.gwp
```

The daemon currently provides JSON-RPC over a Unix socket, persistent state, and
policy decisions for desktop-state-based throttling. Rendering,
GTK-layer-shell integration, and Hyprland/niri output discovery are tracked in
`docs/todo.md`.

The optional `gtk-renderer` feature builds the GTK 4 + gtk4-layer-shell static
renderer path. It expects system GTK 4 and gtk4-layer-shell development files;
CI builds gtk4-layer-shell from source because Ubuntu Noble does not ship a
`libgtk4-layer-shell-dev` package.

The optional `video-renderer` feature builds the GStreamer controller for video
wallpaper pipeline lifecycle. It expects GStreamer 1.0 development files and
plugins from the host system.
