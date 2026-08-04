# TensorDE

TensorDE 以一个统一系统的形式开发原生、GPU-first 的桌面环境。产品程序位于
`apps/`，可复用的协议与渲染标准位于 `crates/`。

## 产品

- `apps/tensorland`：Tensorland Wayland 合成器。
- `apps/tensor-shell`：Tensor Shell，包括顶栏、启动器、通知与 OSD、控制中心、
  概览和锁屏 surface。
- `apps/tensor-files`：Tensor Files 文件管理器。
- `apps/gilder`：Gilder 场景与壁纸引擎。

## 共享基础设施

- `crates/vulkan-renderer`：基于 Vulkanalia 的 Vulkan 1.4 / Roadmap 2026 渲染
  标准，默认 descriptor heap 与 FIFO latest-ready。
- `crates/wayland-client-runtime`：由应用和桌面 Shell 共用的原生 Wayland
  协议及事件循环实现。
- `crates/tensor-*`：与 compositor 共用的 value-only 事件、runtime、host、
  DRM、present、protocol 和 geometry 边界。

Tensorland 承载合成器名称，Tensor Shell 与 Tensor Files 是同品牌配套产品；
TensorDE 仍是仓库与桌面环境品牌。

文档与自动化入口分别统一收录在 [docs](docs/README.md) 和
[scripts](scripts/README.md)。
