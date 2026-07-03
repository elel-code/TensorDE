# Gilder

[中文说明](README.zh-CN.md)

Gilder is a native wallpaper engine for niri, Hyprland, and other independent
compositors. The current renderer direction is FFmpeg demux/parser and Vulkan
hardware decode feeding Gilder/Vulkanalia descriptor-heap render and Wayland
present.

Legacy GStreamer display-sink, decoded-frame CPU copy, descriptor-set fallback,
and old planning documents have been removed. Native video evidence must use
`VK_EXT_descriptor_heap`, report `descriptor_sets=0`, and include CPU, GPU,
memory, FPS, frame-count, descriptor-heap, and zero-copy fields when it is used
as a performance result.

## Current Status

- Daemon IPC, state persistence, package loading, and desktop-state policy are
  present.
- Native video targets FFmpeg `h264_vulkan`, `hevc_vulkan`, and `av1_vulkan`
  hardware decode producing `AV_PIX_FMT_VULKAN`/`AVVkFrame` frames.
- The active render path samples GPU Y/UV plane descriptors through
  `VK_EXT_descriptor_heap` and presents through Wayland without decoded-frame
  CPU copies.
- FFmpeg audio clock/output is modular and separate from video texture
  ownership; video consumes only compact audio-master-clock pacing state.
- Current 4K240 FFmpeg mainline evidence is roughly 240 fps with zero-copy
  present. H.264/H.265 use more host memory than legacy; that is tracked as an
  engineering cost unless FPS or bounded-retention telemetry regresses.

## Engineering Rule

All implementation work must optimize for the long-term native architecture.
Do not add short-term substitutes, sample-specific fixes, hidden compatibility
branches, or temporary render paths to cover missing behavior. When a gap comes
from an unsupported format, effect, material, interaction, renderer-quality, or
runtime subsystem, design and implement that first-class subsystem, and document
any remaining boundary explicitly.

## Next Work

1. FFmpeg video mainline: keep 4K240 H.264/H.265/AV1 10s matrices green, track
   dgop memory, retained `AVFrame` refs, descriptor heap bytes, and zero-copy
   state.
2. Modular platform boundaries: decouple media decode, decoded-image present,
   audio clock, surface host, and event-loop ownership so a future Win32 host
   can replace Wayland-facing pieces without rewriting the FFmpeg/Vulkan path.
3. Full scene wallpaper support: treat static wallpapers as a single-image
   scene case, then connect static image, video, properties, transforms, daemon
   output routing, pause/resume, and package state into one scene lifecycle.
4. Script hygiene: keep only codec smoke, real-source matrix, performance,
   packaging, workshop, and actively used diagnostic helpers. Remove one-off
   spike scripts instead of carrying compatibility wrappers.

## Repository Layout

- `src/bin/gilderd.rs`: daemon entry point.
- `src/bin/gilderctl.rs`: CLI client for daemon control.
- `src/bin/gilder-convert.rs`: Wallpaper Engine conversion and pack tool.
- `src/bin/gilder-native-vulkan.rs`: native Vulkan diagnostics and video smoke
  runner.
- `src/core/`: package and manifest primitives.
- `src/ipc/`: command, protocol, and socket helpers.
- `src/renderer/native_vulkan.rs`: native Vulkan facade and public contract.
- `src/renderer/native_vulkan/`: native Vulkan submodules and shared
  parser/snapshot code.
- `src/renderer/native_vulkan/video/`: FFmpeg demux, Vulkan HW decode boundary,
  pacing, timeline, route, and video evidence helpers.
- `src/renderer/native_vulkan/vulkan/`: the single Vulkanalia backend, split
  into `core/`, `present/`, `scene/`, and `video/`.
- `src/renderer/native_vulkan/present/`: clear/static image present and render
  item planning.
- `src/renderer/native_vulkan/scene/`: scene-lite runtime planning and native
  Vulkan present entry points.
- `src/renderer/native_vulkan/audio/`: FFmpeg audio clock/output policy and
  runtime helpers.
- `docs/native-vulkan-video-ffmpeg-mainline.md`: active FFmpeg Vulkan hardware
  decode mainline plan, memory evidence, and video validation commands.
- `docs/native-vulkan-scene-refactor-goals.md`: active native scene renderer
  architecture plan and evidence gates.
- `docs/packaging.md`: install and distribution notes.
- `docs/man/`: man pages.
- `scripts/native-vulkan-{h264,h265,av1}-ready-prefix-video-smoke.sh`: current
  legacy Vulkan Video compatibility evidence scripts.
- `scripts/ffmpeg-vulkan-hwdecode-4k240-matrix.sh`: FFmpeg Vulkan hardware
  decode 4K240 and real-source matrix runner.
- `scripts/native-vulkan-real-source-matrix.sh`: older real-source coverage
  runner for the native Vulkan Video path.
- `scripts/performance-snapshot.sh`: CPU/RSS/PSS/USS/Private_Dirty/GPU memory
  sampler.

## Commands

```sh
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

Performance evidence must be long enough for sampling. Functional-only smoke
output is not enough for CPU, GPU, memory, or zero-copy claims. For the FFmpeg
mainline, use dgop-backed matrix runs and retain the generated CSV/telemetry
paths in the report.

Example shape:

```sh
scripts/ffmpeg-vulkan-hwdecode-4k240-matrix.sh \
  --no-build \
  --label video-mainline-10s \
  --duration 10 \
  --target-fps source \
  --display wayland-1 \
  --output HDMI-A-1
```

The required fields are `average_present_fps`, `presented_frame_count`,
`all_zero_copy_presented`, dgop memory, smaps peak path,
`ffmpeg_retained_avframe_peak_count`,
`descriptor_sampler_cache_peak_entry_count`,
`descriptor_sampler_cache_total_heap_kb`, descriptor rewrite/recreate counts,
codec/source metadata, and the inferred codec host-memory model.
