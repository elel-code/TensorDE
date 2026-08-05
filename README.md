# TensorDE

TensorDE is a native, GPU-first desktop environment developed as one coherent
system. Product applications live under `apps/`; reusable protocol and rendering
standards live under `crates/`.

## Products

- `apps/tensor-wm` — the Tensorland Wayland compositor.
- `apps/tensor-shell` — Tensor Shell: panels, notifications and OSD, control
  center, overview, launcher/settings entries, and lock surfaces.
- `apps/tensor-launcher` — the retained, bounded Tensor application launcher.
- `apps/tensor-greeter` — the standalone greetd frontend for Tensor sessions.
- `apps/tensor-settings` — the standalone Tensor settings application.
- `apps/tensor-idle` — the independent idle, power and lock policy service.
- `apps/tensor-msg` — the independently installable Tensor product IPC client.
- `apps/tensor-files` — the Tensor Files file manager.
- `apps/tensor-wallpaper` — the Tensor Wallpaper scene and wallpaper engine.
- `apps/tensor-xdp` — the dedicated TensorDE xdg-desktop-portal backend.

## Shared foundations

- `crates/vulkan-renderer` — Vulkanalia-based Vulkan 1.4 / Roadmap 2026 rendering
  standard, with descriptor-heap-first resource binding and FIFO latest-ready
  presentation.
- `crates/vulkan-renderer-build` — pinned Slang-to-SPIR-V generation,
  reflection contracts, Vulkan 1.4 validation, and reproducibility checks;
  applications do not link it at runtime.
- `crates/wayland-client-runtime` — native Wayland protocols and event-loop
  integration shared by applications and the shell.
- `crates/tensor-*` — value-only event, runtime, D-Bus, host, DRM,
  presentation, protocol, and geometry boundaries shared across products.

Tensorland carries the compositor identity; Tensor Shell and Tensor Files form
the companion product family. TensorDE remains the repository and desktop-
environment brand.

Documentation and automation are indexed under [docs](docs/README.md) and
[scripts](scripts/README.md).
