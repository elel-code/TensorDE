# TensorDE

TensorDE is a native, GPU-first desktop environment developed as one coherent
system. Product applications live under `apps/`; reusable protocol and rendering
standards live under `crates/`.

## Products

- `apps/tensorland` — the Tensorland Wayland compositor.
- `apps/tensor-shell` — Tensor Shell: panels, launcher, notifications and
  OSD, control center, overview, and lock surfaces.
- `apps/tensor-files` — the Tensor Files file manager.
- `apps/tensor-wallpaper` — the Tensor Wallpaper scene and wallpaper engine.

## Shared foundations

- `crates/vulkan-renderer` — Vulkanalia-based Vulkan 1.4 / Roadmap 2026 rendering
  standard, with descriptor-heap-first resource binding and FIFO latest-ready
  presentation.
- `crates/vulkan-renderer-build` — pinned Slang-to-SPIR-V generation,
  reflection contracts, Vulkan 1.4 validation, and reproducibility checks;
  applications do not link it at runtime.
- `crates/wayland-client-runtime` — native Wayland protocols and event-loop
  integration shared by applications and the shell.
- `crates/tensor-*` — value-only event, runtime, host, DRM, presentation,
  protocol, and geometry boundaries shared with the compositor.

Tensorland carries the compositor identity; Tensor Shell and Tensor Files form
the companion product family. TensorDE remains the repository and desktop-
environment brand.

Documentation and automation are indexed under [docs](docs/README.md) and
[scripts](scripts/README.md).
