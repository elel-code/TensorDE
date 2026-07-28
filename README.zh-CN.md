# Fika

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust Edition](https://img.shields.io/badge/rust-2024-orange.svg)](https://doc.rust-lang.org/edition-guide/rust-2024/index.html)

Fika 是一个面向 Wayland 桌面的 Rust 文件管理器。当前 UI 主线是默认的 `fika`
二进制：Fika 专用的 wgpu shell 构建在可复用的原生 Wayland runtime 之上；
之前的 UI runtime 已经从源码树移除。

> [English version](README.md)

## 当前 Runtime

- `fika` 是默认运行目标，也是当前源码树里唯一的文件管理器 UI。
- `wayland-client-runtime` 是基于 SCTK 的通用协议、surface 和事件层；Fika
  自身不再直接依赖 winit 或 SCTK。
- `wgpu` 使用官方 crates.io 版本。
- `FIKA_VULKAN_RENDERER=1` 选择直接 Vulkan 1.4 主窗口路径；该路径已不创建
  wgpu 对象，并通过 device-local R8 glyph atlas、动态 vertex buffer、descriptor
  heap 和 timeline 资源生命周期渲染 retained analytic chrome，以及 Places、地址栏、
  Filter、Details、文件项和状态栏文字。
- `fika-core` 保持 UI-neutral，负责文件系统和领域行为。
- 剪贴板和 DnD 使用 Wayland `wl_data_device`；渲染句柄可供 wgpu 或直接
  Vulkan 使用，KDE blur 保留完整的 region 语义。
- 原生 Vulkan 迁移链可用 `FIKA_VULKAN_PROBE=1` 验证：它在 wgpu 初始化前通过
  `crates/vulkan-renderer` 完成 swapchain clear、device-local quad 上传和真实 draw，
  不在同一帧混用两个逻辑设备。
- 父子 dialog、popup 定位/重定位、cursor-shape 回退和 drag icon 均由通用
  Wayland 层管理。
- Portal 与 privileged helper 继续作为独立集成二进制保留。

## 源码布局

```text
src/
  lib.rs                         UI-neutral core 导出
  main.rs                        Wayland/wgpu shell 入口
  windowing.rs                   窗口、输入与剪贴板集成
  windowing_event_loop.rs        Fika 调度和事件翻译
  windowing_types.rs             Fika 自有窗口与输入类型
  core.rs                        Core 模块重导出
  cli.rs                         共享 CLI 解析入口
  cli/
    args.rs                      Manager/chooser 参数解析
  core/                          Directory、pane、operations、launcher、
                                 Places、devices、thumbnails、trash、D-Bus
  shell/                         已拆出的 shell 模块
  bin/
    fika-xdp-filechooser.rs      XDG Desktop Portal FileChooser 后端
    fika-privileged-helper.rs    特权操作 D-Bus helper
crates/
  vulkan-renderer/               可复用的 Vulkan 1.4 渲染标准与后端
  wayland-client-runtime/        可复用的 SCTK Wayland 协议/事件 crate
```

## 构建与运行

```bash
cargo run --bin fika -- --view compact /etc
cargo test --bin fika
scripts/check-rust-file-lines.sh
```

每个 Rust 源文件严格限制为最多 1000 行。门禁不设历史豁免，合并变更前必须通过。

因为 `default-run` 已经是 `fika`，也可以直接运行：

```bash
cargo run -- /etc
```

## 架构要点

- Pane state 按稳定 pane identity 路由，并通过可复用 pane container 存储；
  分屏 pane 走同一套 state/projection/slot-pool 路径。
- 热路径 item view 使用 retained + virtualization：visible-slot 复用、投影缓存、
  text/icon atlas 缓存和显式 scroll metrics。
- Shell 热路径使用 MIME/icon role 按 role + size 复用、read-ahead 队列化、
  atlas 子矩形上传，并收紧 icon theme cache 边界。
- 文件管理器语义以 Dolphin 为第一参考；shell 层负责渲染、hit-test、DPI、输入路由、
  overlay 和 telemetry。

## 参考文档

- [docs/DEVICES_REFERENCE.zh-CN.md](docs/DEVICES_REFERENCE.zh-CN.md) —
  设备和 Places 行为。
- [docs/NETWORK_REFERENCE.zh-CN.md](docs/NETWORK_REFERENCE.zh-CN.md) —
  网络位置行为。
- [docs/PERFORMANCE_ALIGNMENT.zh-CN.md](docs/PERFORMANCE_ALIGNMENT.zh-CN.md) —
  Dolphin-first 性能参考原则。
- [docs/TRASH_REFERENCE.zh-CN.md](docs/TRASH_REFERENCE.zh-CN.md) —
  回收站行为。
