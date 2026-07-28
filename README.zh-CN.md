# TensorDE

TensorDE 以一个统一系统的形式开发原生、GPU-first 的桌面环境。产品程序位于
`apps/`，可复用的协议与渲染标准位于 `crates/`。

## 产品

- `apps/desktop-shell`：桌面 Shell，包括顶栏、启动器、通知与 OSD、控制中心、
  概览和锁屏 surface。
- `apps/fika`：Fika 文件管理器。
- `apps/gilder`：Gilder 场景与壁纸引擎。

Tensor 暂时保留为独立仓库，后续再单独迁移。

## 共享基础设施

- `crates/vulkan-renderer`：基于 Vulkanalia 的 Vulkan 1.4 / Roadmap 2026 渲染
  标准，默认 descriptor heap 与 FIFO latest-ready。
- `crates/wayland-client-runtime`：由应用和桌面 Shell 共用的原生 Wayland
  协议及事件循环实现。

`desktop-shell` 保持职责导向的 crate 名；TensorDE 是仓库和桌面环境品牌。
