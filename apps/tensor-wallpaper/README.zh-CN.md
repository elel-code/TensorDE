# Tensor Wallpaper

Tensor Wallpaper 是面向 niri、Hyprland 等独立 compositor 的 Wayland 动态壁纸引擎。当前主线是：

- Tensor Wallpaper 只提供 typed media source、codec requirement 与时钟策略；
- `vulkan-renderer` 负责 FFmpeg demux/parser/packet、Vulkan 硬件解码、retained
  `AV_PIX_FMT_VULKAN`/`AVVkFrame` plane lease、descriptor heap、渲染和 Wayland present；
- scene 正在按 Godot 风格边界全面重写，WE 语义只以 `reverse-engineered/tensor-wallpaper/` 为准。

## 当前原则

- 不保留旧 GStreamer display-sink、decoded-frame CPU copy、旧式 Vulkan 绑定表 fallback。
- scene runtime、binary ingest、draw-pass/effect graph、旧 shader wrapper 和旧 smoke 脚本已删除。
- 0 兼容旧代码和旧字段；不接受 CPU compatibility renderer、mesh blocker、隐藏 fallback、
  样本特化修复或临时补丁。
- 4K 240Hz 和 10 秒 host dirty/retained memory < 40 MiB 是性能硬门槛。
- 能用 GPU 的工作必须上 GPU；能用 GPU 却留在 CPU 热路径是不可接受实现。
- 无法确认的 WE 语义必须继续反汇编并更新 `reverse-engineered/tensor-wallpaper/` 后再实现。
- 架构必须继续对齐 Godot 的 RenderingServer、RendererSceneRender、RenderingDevice、storage
  和 Vulkan driver 边界。
- 严格模块化是硬约束；禁止新增堆叠文件或继续往历史超大文件里追加 scene 主线逻辑。
- 脚本入口使用 Python，并通过 `uv run python ...` 调度；旧 shell smoke 已删除。

## 仓库结构

- `src/bin/tensor-wallpaperd.rs`：daemon 入口。
- `../tensor-msg`：可独立打包的控制 CLI（`tensor-msg wallpaper`）。
- `src/bin/tensor-wallpaper-convert.rs`：Wallpaper Engine 转换和打包工具。
- `src/bin/tensor-wallpaper.rs`：场景、视频渲染与指令诊断入口。
- `src/convert/we_ingest/`：WE project、scene.pkg、tex、material、effect、mdl 冷路径 ingest。
- `src/engine/scene/`：新 scene engine ABI、binary、storage 和 RenderingServer 边界。
- `src/renderer/rendering_device/scene/`：typed scene descriptor heap/render graph 计划边界。
- `src/renderer/rendering_device/video/`：typed media source、PTS pacing 与 direct/scene plane binding。
- `src/renderer/rendering_device/scene_present/`：Tensor Wallpaper 的 typed scene policy/command-plan integration；
  `vulkan-renderer` 是唯一 Vulkan owner。
- `docs/tensor-wallpaper/tensor-wallpaper-scene-engine-architecture.md`：scene 全面重写架构。

## 常用命令

```text
cargo check -p tensor-wallpaper
cargo check -p tensor-wallpaper --features rendering-device --bin tensor-wallpaper
cargo check -p tensor-wallpaper --features video --bin tensor-wallpaper
cargo test -p tensor-wallpaper --features video
cargo run -p tensor-wallpaper --bin tensor-wallpaperd
cargo run -p tensor-msg -- wallpaper ping
cargo run -p tensor-msg -- wallpaper outputs
cargo run -p tensor-wallpaper --bin tensor-wallpaper-convert -- wallpaper-engine /path/to/we/project ./out.gscene
```

## Python 入口

```text
uv run python scripts/tensor-wallpaper/video_decode_matrix.py --no-build --label video-mainline-10s --duration 10 --target-fps source --display wayland-1
uv run python scripts/tensor-wallpaper/performance_snapshot.py --duration 10 --pid <pid>
uv run python scripts/tensor-wallpaper/wallpaper_engine_workshop_download.py --item-id <id>
uv run python apps/tensor-wallpaper/packaging/build_dist.py
```

性能证据必须包含足够长的采样窗口，并保留 `binding`/`route`、
`requested_present_frame_count`/`frames_presented`、`decoded_frame_count`、
`repeated_presentation_count`、`video_loop_index`、`frame_slot_count`、`pacing`、
`present_mode`、`decoded_image_zero_copy_presented`/`zero_copy_scope`、
`runtime_elapsed_ms`/`average_present_fps`、dgop/smaps memory 和 codec/source metadata。
