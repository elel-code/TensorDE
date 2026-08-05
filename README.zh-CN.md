# TensorDE

TensorDE 以一个统一系统的形式开发原生、GPU-first 的桌面环境。产品程序位于
`apps/`，可复用的协议与渲染标准位于 `crates/`。

## 产品

- `apps/tensor-wm`：Tensorland Wayland 合成器。
- `apps/tensor-shell`：Tensor Shell，包括顶栏、通知与 OSD、控制中心、概览、
  启动器/设置入口和锁屏 surface。
- `apps/tensor-launcher`：独立的 Tensor 应用启动器。
- `apps/tensor-greeter`：独立的 greetd 登录前端。
- `apps/tensor-settings`：独立的 Tensor 设置应用。
- `apps/tensor-idle`：独立的 idle、电源与锁定策略服务。
- `apps/tensor-msg`：可独立安装的 Tensor 产品 IPC 客户端。
- `apps/tensor-files`：Tensor Files 文件管理器。
- `apps/tensor-wallpaper`：Tensor Wallpaper 场景与壁纸引擎。
- `apps/tensor-xdp`：TensorDE 专用 xdg-desktop-portal 后端。

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
