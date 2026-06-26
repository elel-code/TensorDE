# Gilder

[English README](README.md)

Gilder 是面向 niri、Hyprland 等独立 Wayland compositor 的原生壁纸引擎。当前主线是
FFmpeg 负责 demux/parser/packet 语义，Vulkanalia/Vulkan Video 负责 GPU 解码、渲染和
Wayland present。

旧的 GStreamer display-sink、decoded-frame CPU copy、descriptor set fallback 和历史迁移
文档已经删除。视频路径必须使用 `VK_EXT_descriptor_heap`，性能证据必须报告
`descriptor_sets=0`，并同时给出 CPU、GPU、内存、FPS、帧数、descriptor heap 和
zero-copy 状态。

## 当前状态

- 已有 daemon IPC、状态持久化、包加载和 desktop-state policy。
- 原生 Vulkan video 支持 H.264、H.265 Main8/Main10、AV1 Main8/Main10。
- 当前渲染路径通过 `VK_EXT_descriptor_heap` 采样 GPU Y/UV plane descriptor，并通过
  Wayland present，不保留 decoded-frame CPU copy。
- 4K240 当前达标门槛和证据记录在 `docs/native-vulkan-video.md`。

## 下一步计划

1. Audio 集成：按 FFmpeg 对齐 audio demux、clock、loop 语义，再接入 muted clock-only
   和有声输出模式。
2. 完整 scene 壁纸能力：把 native Vulkan video、静态图、属性、scene transform 和 daemon
   output routing 接入正常壁纸生命周期。
3. 更多码流覆盖：扩展 H.264、H.265、AV1 的真实源和生成源矩阵，覆盖 profile、bit depth、
   reference pattern、任意入口、loop boundary 和长跑资源稳定性。
4. 脚本清理：只保留当前 CI、codec smoke、real-source matrix、performance、packaging 和
   workshop helper。迁移期/试验性脚本直接删除，不做兼容 wrapper。

## 仓库结构

- `src/bin/gilderd.rs`：daemon 入口。
- `src/bin/gilderctl.rs`：daemon CLI 控制端。
- `src/bin/gilder-convert.rs`：Wallpaper Engine 转换和打包工具。
- `src/bin/gilder-native-vulkan.rs`：原生 Vulkan 诊断和视频 smoke runner。
- `src/core/`：包格式和 manifest 基础类型。
- `src/ipc/`：命令、协议和 socket helper。
- `src/renderer/native_vulkan/`：原生 Vulkan 渲染、FFmpeg demux、video 和 present 代码。
- `docs/native-vulkan-video.md`：当前 FFmpeg/Vulkan Video 门槛、证据和验证规则。
- `docs/packaging.md`：安装和发行说明。
- `docs/man/`：man pages。
- `scripts/native-vulkan-{h264,h265,av1}-ready-prefix-video-smoke.sh`：当前三种格式证据脚本。
- `scripts/native-vulkan-real-source-matrix.sh`：真实源覆盖矩阵。
- `scripts/performance-snapshot.sh`：CPU/RSS/PSS/USS/Private_Dirty/GPU memory 采样。
- `scripts/desktop-policy-smoke.sh`：CI desktop-policy smoke。

## 常用命令

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

发行包由 `packaging/build-dist.sh` 生成。`.gwpdir` 可以使用 `manifest.gilder.json` 或便于
编辑的 `manifest.gilder.toml`；`.gwp` 归档使用 canonical `manifest.gilder.json`。

## 视频证据要求

性能证据必须播放足够长，且必须开启 `--performance-snapshot`。只跑功能 smoke 不能用于说明
CPU、GPU 或内存占用。codec smoke 默认使用 `--allocator-profile system`，也就是发行环境口径，
启动视频进程前会清掉已知 glibc/malloc 调参变量。

示例：

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
  --allocator-profile system \
  --performance-snapshot \
  --performance-duration 6 \
  --performance-interval 1 \
  --report-dir /tmp/gilder-h264-real-source
```

必须保留的字段包括 `average_present_fps`、decoded/presented 帧数、平均 CPU、RSS/PSS/USS、
`Private_Dirty`、进程 GPU memory、`descriptor_sets`、`descriptor_heap_only` 和 zero-copy
状态。
