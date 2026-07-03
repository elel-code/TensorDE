# Gilder

[English README](README.md)

Gilder 是面向 niri、Hyprland 等独立 compositor 的原生壁纸引擎。当前主线是
FFmpeg 负责 demux/parser/packet 和 Vulkan 硬件解码，Gilder/Vulkanalia 负责
`AV_PIX_FMT_VULKAN`/`AVVkFrame` 到 descriptor heap、渲染和 Wayland present。

旧的 GStreamer display-sink、decoded-frame CPU copy、descriptor set fallback 和历史迁移
文档已经删除。视频路径必须使用 `VK_EXT_descriptor_heap`，性能证据必须报告
`descriptor_sets=0`，并同时给出 CPU、GPU、内存、FPS、帧数、descriptor heap 和
zero-copy 状态。

## 当前状态

- 已有 daemon IPC、状态持久化、包加载和 desktop-state policy。
- 原生 video 主线目标是 FFmpeg `h264_vulkan`、`hevc_vulkan`、`av1_vulkan` 硬解输出
  `AV_PIX_FMT_VULKAN`/`AVVkFrame`。
- 当前渲染路径通过 `VK_EXT_descriptor_heap` 采样 GPU Y/UV plane descriptor，并通过
  Wayland present，不保留 decoded-frame CPU copy。
- FFmpeg audio clock/output 是独立模块，不绑定 video texture ownership；video 只消费很小的
  audio-master-clock pacing 状态。
- 当前 4K240 FFmpeg mainline 证据约为 240 fps、zero-copy present。H.264/H.265 的 host
  memory 比 legacy 高，这先作为工程代价跟踪；只有 FPS 或 bounded-retention telemetry 退化时
  才视为主线问题。

## 下一步计划

1. FFmpeg video 主线：保持 4K240 H.264/H.265/AV1 10s matrix 通过，持续跟踪 dgop memory、
   retained `AVFrame` refs、descriptor heap bytes 和 zero-copy 状态。
2. 模块化平台边界：解耦 media decode、decoded-image present、audio clock、surface host 和
   event-loop ownership，让未来 Win32 host 能替换 Wayland 相关部分，而不重写 FFmpeg/Vulkan
   路径。
3. 完整 scene 壁纸能力：把静态壁纸视为单 image layer 的 scene 特例，再把静态图、
   video、properties、transform、daemon output routing、pause/resume 和 package state
   接入统一 scene lifecycle。
4. 脚本清理：只保留 codec smoke、real-source matrix、performance、packaging、workshop
   和仍在使用的诊断 helper。一次性试验脚本直接删除，不做兼容 wrapper。

## 仓库结构

- `src/bin/gilderd.rs`：daemon 入口。
- `src/bin/gilderctl.rs`：daemon CLI 控制端。
- `src/bin/gilder-convert.rs`：Wallpaper Engine 转换和打包工具。
- `src/bin/gilder-native-vulkan.rs`：原生 Vulkan 诊断和视频 smoke runner。
- `src/core/`：包格式和 manifest 基础类型。
- `src/ipc/`：命令、协议和 socket helper。
- `src/renderer/native_vulkan.rs`：原生 Vulkan facade 和公开 contract。
- `src/renderer/native_vulkan/`：原生 Vulkan 子模块和共享 parser/snapshot 代码。
- `src/renderer/native_vulkan/video/`：FFmpeg demux、Vulkan HW decode 边界、pacing、
  timeline、route 和视频证据 helper。
- `src/renderer/native_vulkan/vulkan/`：唯一 Vulkanalia 后端，按 `core/`、`present/`、
  `scene/`、`video/` 拆分。
- `src/renderer/native_vulkan/present/`：clear/static image present 和 render item 规划。
- `src/renderer/native_vulkan/scene/`：scene-lite runtime 规划和原生 Vulkan present 入口。
- `src/renderer/native_vulkan/audio/`：FFmpeg audio clock/output policy 和 runtime helper。
- `docs/native-vulkan-video-ffmpeg-mainline.md`：当前 FFmpeg Vulkan 硬解主线计划、内存证据和
  video 验证命令。
- `docs/native-vulkan-scene-refactor-goals.md`：当前原生 scene renderer 架构计划和证据门槛。
- `docs/packaging.md`：安装和发行说明。
- `docs/man/`：man pages。
- `scripts/native-vulkan-{h264,h265,av1}-ready-prefix-video-smoke.sh`：旧 Vulkan Video 路径的
  兼容证据脚本。
- `scripts/ffmpeg-vulkan-hwdecode-4k240-matrix.sh`：FFmpeg Vulkan 硬解 4K240 和真实源矩阵。
- `scripts/native-vulkan-real-source-matrix.sh`：旧 native Vulkan Video 路径的真实源覆盖矩阵。
- `scripts/performance-snapshot.sh`：CPU/RSS/PSS/USS/Private_Dirty/GPU memory 采样。

## 常用命令

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

发行包由 `packaging/build-dist.sh` 生成。`.gwpdir` 可以使用 `manifest.gilder.json` 或便于
编辑的 `manifest.gilder.toml`；`.gwp` 归档使用 canonical `manifest.gilder.json`。

## 视频证据要求

性能证据必须播放足够长。只跑功能 smoke 不能用于说明 CPU、GPU、内存或 zero-copy。FFmpeg
主线使用 dgop-backed matrix，并在报告中保留生成的 CSV/telemetry 路径。

示例：

```sh
scripts/ffmpeg-vulkan-hwdecode-4k240-matrix.sh \
  --no-build \
  --label video-mainline-10s \
  --duration 10 \
  --target-fps source \
  --display wayland-1 \
  --output HDMI-A-1
```

必须保留的字段包括 `average_present_fps`、`presented_frame_count`、
`all_zero_copy_presented`、dgop memory、smaps peak path、
`ffmpeg_retained_avframe_peak_count`、`descriptor_sampler_cache_peak_entry_count`、
`descriptor_sampler_cache_total_heap_kb`、descriptor rewrite/recreate counts、codec/source
metadata，以及推断出的 codec host-memory model。
