# Gilder

Gilder 是面向 niri、Hyprland 等独立 compositor 的原生壁纸引擎。当前主线是：

- FFmpeg 负责 demux/parser/packet 和 Vulkan 硬件解码；
- Gilder/Vulkanalia 负责 `AV_PIX_FMT_VULKAN`/`AVVkFrame` 到 descriptor heap、渲染和
  Wayland present；
- scene 正在按 Godot 风格边界全面重写，WE 语义只以 `reverse-engineered/gilder/` 为准。

## 当前原则

- 不保留旧 GStreamer display-sink、decoded-frame CPU copy、旧式 Vulkan 绑定表 fallback。
- scene runtime、binary ingest、draw-pass/effect graph、旧 shader wrapper 和旧 smoke 脚本已删除。
- 0 兼容旧代码和旧字段；不接受 CPU compatibility renderer、mesh blocker、隐藏 fallback、
  样本特化修复或临时补丁。
- 4K 240Hz 和 10 秒 host dirty/retained memory < 40 MiB 是性能硬门槛。
- 能用 GPU 的工作必须上 GPU；能用 GPU 却留在 CPU 热路径是不可接受实现。
- 无法确认的 WE 语义必须继续反汇编并更新 `reverse-engineered/gilder/` 后再实现。
- 架构必须继续对齐 Godot 的 RenderingServer、RendererSceneRender、RenderingDevice、storage
  和 Vulkan driver 边界。
- 严格模块化是硬约束；禁止新增堆叠文件或继续往历史超大文件里追加 scene 主线逻辑。
- 脚本入口使用 Python，并通过 `uv run python ...` 调度；旧 shell smoke 已删除。

## 仓库结构

- `src/bin/gilderd.rs`：daemon 入口。
- `src/bin/gilderctl.rs`：daemon CLI。
- `src/bin/gilder-convert.rs`：Wallpaper Engine 转换和打包工具。
- `src/bin/gilder-native-vulkan.rs`：原生 Vulkan 诊断入口。
- `src/convert/we_ingest/`：WE project、scene.pkg、tex、material、effect、mdl 冷路径 ingest。
- `src/engine/scene/`：新 scene engine ABI、binary、storage 和 RenderingServer 边界。
- `src/renderer/native_vulkan/scene/`：native Vulkan scene descriptor heap/render graph 计划边界。
- `src/renderer/native_vulkan/video/`：FFmpeg demux、Vulkan HW decode、pacing 和 evidence helper。
- `src/renderer/native_vulkan/vulkan/`：Vulkanalia backend。
- `docs/gilder/gilder-scene-engine-architecture.md`：scene 全面重写架构。

## 常用命令

```text
cargo check -p gilder
cargo check -p gilder --features native-vulkan-renderer --bin gilder-native-vulkan
cargo check -p gilder --features native-vulkan-video --bin gilder-native-vulkan
cargo test -p gilder --features native-vulkan-video
cargo run -p gilder --bin gilderd
cargo run -p gilder --bin gilderctl -- ping
cargo run -p gilder --bin gilderctl -- outputs
cargo run -p gilder --bin gilder-convert -- wallpaper-engine /path/to/we/project ./out.gscene
```

## Python 入口

```text
uv run python scripts/gilder/ffmpeg_vulkan_hwdecode_matrix.py --no-build --label video-mainline-10s --duration 10 --target-fps source --display wayland-1
uv run python scripts/gilder/performance_snapshot.py --duration 10 --pid <pid>
uv run python scripts/gilder/wallpaper_engine_workshop_download.py --item-id <id>
uv run python apps/gilder/packaging/build_dist.py
```

性能证据必须包含足够长的采样窗口，并保留 `average_present_fps`、`presented_frame_count`、
`all_zero_copy_presented`、dgop/smaps memory、descriptor heap telemetry 和 codec/source metadata。
