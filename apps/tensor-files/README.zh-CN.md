# Tensor Files

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)
[![Rust Edition](https://img.shields.io/badge/rust-2024-orange.svg)](https://doc.rust-lang.org/edition-guide/rust-2024/index.html)

Tensor Files 是面向当前 compositor 栈的 Rust 文件管理器。当前 UI 主线是默认的 `tensor-files`
二进制：Tensor Files 自有 retained UI 构建在可复用的原生 client runtime 之上；之前的
UI runtime 已经从源码树移除。

> [English version](README.md)

## 当前 Runtime

- `tensor-files` 是默认运行目标，也是当前源码树里唯一的文件管理器 UI。
- `wayland-client-runtime` 是基于 SCTK 的通用协议、surface 和事件层；Tensor Files
  自身不再直接依赖 winit 或 SCTK。
- 完整文件管理器控制器统一使用共享的原生 Vulkan 1.4 后端，并通过
  device-local R8 glyph atlas、动态 vertex buffer、descriptor
  heap 和 timeline 资源生命周期渲染 retained analytic chrome，以及 Places、地址栏、
  Filter、Details、文件项和状态栏文字。外部 icon dma-buf 会经显式 foreign queue
  ownership transfer 直接进入同一套 Vulkan resident cache，不经过 CPU 像素回读。
- `tensor_files_core` 保持 UI-neutral，负责文件系统和领域行为。
- 剪贴板和 DnD 使用 Wayland `wl_data_device`；导出的渲染句柄保持为原生 Vulkan
  dma-buf，KDE blur 保留完整的 region 语义。
- Vulkan 资源只由持久原生 renderer 路径创建和持有。
- 父子 dialog、popup 定位/重定位、cursor-shape 回退和 drag icon 均由通用
  Wayland 层管理。
- Privileged helper 继续作为独立的系统集成二进制保留。
- XDG Desktop Portal 集成不再由 Tensor Files 持有；后续由整个 DE 共用的 portal
  服务统一负责。

## 源码布局

```text
src/
  lib.rs                         UI-neutral core 导出
  main.rs                        文件管理器 UI 入口
  windowing.rs                   窗口、输入与剪贴板集成
  windowing_event_loop.rs        Tensor Files 调度和事件翻译
  windowing_types.rs             Tensor Files 自有窗口与输入类型
  core.rs                        Core 模块重导出
  core/                          Directory、pane、operations、launcher、
                                 Places、devices、thumbnails、trash、D-Bus
  ui/                            Tensor Files 自有 UI 模块
  bin/
    tensor-files-privileged-helper.rs    特权操作 D-Bus helper
../../crates/
  vulkan-renderer/               可复用的 Vulkan 1.4 渲染标准与后端
  wayland-client-runtime/        可复用的 SCTK Wayland 协议/事件 crate
```

## 构建与运行

```bash
cargo run -p tensor-files --bin tensor-files -- --view compact /etc
cargo test -p tensor-files --bin tensor-files
scripts/check-rust-file-lines.sh
```

以上命令从 workspace 根目录执行。每个 Rust 源文件严格限制为最多 800 行。
门禁不设历史豁免，合并变更前必须通过。

因为 `default-run` 已经是 `tensor-files`，也可以直接运行：

```bash
cargo run -p tensor-files -- /etc
```

## 架构要点

- Pane state 按稳定 pane identity 路由，并通过可复用 pane container 存储；
  分屏 pane 走同一套 state/projection/slot-pool 路径。
- 热路径 item view 使用 retained + virtualization：visible-slot 复用、投影缓存、
  text/icon atlas 缓存和显式 scroll metrics。
- UI 热路径使用 MIME/icon role 按 role + size 复用、read-ahead 队列化、
  atlas 子矩形上传，并收紧 icon theme cache 边界。
- 文件管理器语义以 Dolphin 为第一参考；UI 层负责渲染、hit-test、DPI、输入路由、
  overlay 和 telemetry。

## 参考文档

- [DEVICES_REFERENCE.zh-CN.md](../../docs/tensor-files/DEVICES_REFERENCE.zh-CN.md) —
  设备和 Places 行为。
- [NETWORK_REFERENCE.zh-CN.md](../../docs/tensor-files/NETWORK_REFERENCE.zh-CN.md) —
  网络位置行为。
- [PERFORMANCE_ALIGNMENT.zh-CN.md](../../docs/tensor-files/PERFORMANCE_ALIGNMENT.zh-CN.md) —
  Dolphin-first 性能参考原则。
- [TRASH_REFERENCE.zh-CN.md](../../docs/tensor-files/TRASH_REFERENCE.zh-CN.md) —
  回收站行为。
