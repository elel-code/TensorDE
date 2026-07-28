# TensorDE

TensorDE is a native, GPU-first desktop environment developed as one coherent
system. Product applications live under `apps/`; reusable protocol and rendering
standards live under `crates/`.

## Products

- `apps/desktop-shell` — the desktop shell: panels, launcher, notifications and
  OSD, control center, overview, and lock surfaces.
- `apps/fika` — the Fika file manager.
- `apps/gilder` — the Gilder scene and wallpaper engine.

Tensor remains a separate repository for now and will be migrated in a later,
explicit step.

## Shared foundations

- `crates/vulkan-renderer` — Vulkanalia-based Vulkan 1.4 / Roadmap 2026 rendering
  standard, with descriptor-heap-first resource binding and FIFO latest-ready
  presentation.
- `crates/wayland-client-runtime` — native Wayland protocols and event-loop
  integration shared by applications and the shell.

`desktop-shell` remains a role-oriented crate name; TensorDE is the repository
and desktop-environment brand.
