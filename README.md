# Gilder

[中文说明](README.zh-CN.md)

Gilder is a native Wayland wallpaper engine for niri, Hyprland, and other
independent compositors. The current renderer direction is FFmpeg demux/parser
frontends feeding a Vulkanalia/Vulkan Video GPU path for decode, render, and
Wayland present.

Legacy GStreamer display-sink, decoded-frame CPU copy, descriptor-set fallback,
and old planning documents have been removed. Native video evidence must use
`VK_EXT_descriptor_heap`, report `descriptor_sets=0`, and include CPU, GPU,
memory, FPS, frame-count, descriptor-heap, and zero-copy fields when it is used
as a performance result.

## Current Status

- Daemon IPC, state persistence, package loading, and desktop-state policy are
  present.
- Native Vulkan video supports H.264, H.265 Main8/Main10, and AV1 Main8/Main10
  through FFmpeg packet/parsing semantics and Vulkan Video decode.
- The active render path samples GPU Y/UV plane descriptors through
  `VK_EXT_descriptor_heap` and presents through Wayland without decoded-frame
  CPU copies.
- Current validated 4K240 gates are recorded in
  `docs/native-vulkan-video.md`.

## Next Work

1. Audio integration: align audio demux/clock/loop semantics with FFmpeg, then
   wire muted clock-only and audible output modes into the daemon/runtime path.
2. Full scene wallpaper support: connect native Vulkan video, static images,
   properties, scene transforms, and daemon output routing into the normal
   wallpaper lifecycle.
3. Broader bitstream coverage: expand real-source and generated matrices for
   H.264, H.265, and AV1 profiles, bit depths, reference patterns, arbitrary
   entry points, loop boundaries, and long-run resource stability.
4. Script hygiene: keep only current CI, codec smoke, real-source matrix,
   performance, packaging, and workshop helpers. Remove one-off migration or
   spike scripts instead of carrying compatibility wrappers.

## Repository Layout

- `src/bin/gilderd.rs`: daemon entry point.
- `src/bin/gilderctl.rs`: CLI client for daemon control.
- `src/bin/gilder-convert.rs`: Wallpaper Engine conversion and pack tool.
- `src/bin/gilder-native-vulkan.rs`: native Vulkan diagnostics and video smoke
  runner.
- `src/core/`: package and manifest primitives.
- `src/ipc/`: command, protocol, and socket helpers.
- `src/renderer/native_vulkan/`: native Vulkan render, FFmpeg demux, video, and
  present code.
- `docs/native-vulkan-video.md`: current FFmpeg/Vulkan Video gates, evidence,
  and next validation rules.
- `docs/packaging.md`: install and distribution notes.
- `docs/man/`: man pages.
- `scripts/native-vulkan-{h264,h265,av1}-ready-prefix-video-smoke.sh`: current
  codec evidence scripts.
- `scripts/native-vulkan-real-source-matrix.sh`: real-source coverage runner.
- `scripts/performance-snapshot.sh`: CPU/RSS/PSS/USS/Private_Dirty/GPU memory
  sampler.
- `scripts/desktop-policy-smoke.sh`: CI desktop-policy smoke.

## Commands

```sh
scripts/install-ci-deps-ubuntu.sh
cargo check
cargo check --features native-vulkan-renderer
cargo check --features native-vulkan-video --bin gilder-native-vulkan
cargo test --features native-vulkan-video
cargo run --bin gilderd
cargo run --bin gilderctl -- ping
cargo run --bin gilderctl -- outputs
cargo run --bin gilderctl -- watch
cargo run --bin gilderctl -- set ./examples/wallpapers/static-demo.gwpdir --output eDP-1
cargo run --bin gilder-convert -- wallpaper-engine /path/to/we/project ./out.gwpdir
cargo run --bin gilder-convert -- pack ./examples/wallpapers/static-demo.gwpdir ./static-demo.gwp
```

Distribution assets are staged by `packaging/build-dist.sh`. `.gwpdir`
packages can use `manifest.gilder.json` or authoring-friendly
`manifest.gilder.toml`; `.gwp` archives are packed with canonical
`manifest.gilder.json`.

## Video Evidence

Performance evidence must be long enough for sampling and must pass
`--performance-snapshot`. Functional-only smoke output is not enough for CPU,
GPU, or memory claims. Codec smoke scripts do not provide allocator tuning
profiles; they clear known glibc/malloc tuning variables before launching the
video process so evidence matches untuned distribution behavior.
Current 4K240 video performance gates are `average_present_fps >= 239.999` and
`performance_max_private_dirty_kib < 25000`.

Example shape:

```sh
scripts/native-vulkan-h264-ready-prefix-video-smoke.sh \
  --no-build \
  --display wayland-1 \
  --output HDMI-A-1 \
  --source /path/to/source.mp4 \
  --target-fps 60 \
  --decode-prefix 600 \
  --playback-frames 600 \
  --arbitrary-entry-offset 2.3 \
  --performance-snapshot \
  --performance-duration 6 \
  --performance-interval 1 \
  --report-dir /tmp/gilder-h264-real-source
```

The required fields are `average_present_fps`, decoded/presented counts,
average CPU percent, RSS/PSS/USS, `Private_Dirty`, process GPU memory,
`descriptor_sets`, `descriptor_heap_only`, and zero-copy state.
