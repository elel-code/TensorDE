# Tensor Files

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)
[![Rust Edition](https://img.shields.io/badge/rust-2024-orange.svg)](https://doc.rust-lang.org/edition-guide/rust-2024/index.html)

Tensor Files is a Rust file manager for the current compositor stack. The UI mainline
is the default `tensor-files` binary, with Tensor Files-owned retained UI over a reusable native
client runtime; the previous UI runtimes have been removed from the source tree.

> [中文版 / Chinese](README.zh-CN.md)

## Current Runtime

- `tensor-files` is the default run target and the only in-tree file-manager UI.
- `wayland-client-runtime` is the reusable SCTK-based protocol, surface and
  event layer. Tensor Files itself has no direct winit or SCTK dependency.
- The complete file-manager controller renders through the shared native
  Vulkan 1.4 backend. It renders retained analytic chrome plus Places, location/filter,
  Details, file-item, and status labels through a device-local R8 glyph atlas,
  dynamic vertex buffers, descriptor heaps, and timeline-managed resources
  without a second rendering backend. External icon dma-bufs are imported directly
  into the same Vulkan resident cache with explicit foreign-queue ownership
  transfer and no CPU pixel readback.
- Main and detached-dialog frames describe scene-color dependencies, history,
  external consumers, async compute, and terminal transforms as semantic facts.
  The shared presentation planner defaults the current analytic UI to a direct
  single pass. A future bounded scene-color effect lowers to direct output plus
  local passes when the surface supports `COPY_SOURCE`; otherwise automatic
  policy selects retained offscreen composition. Explicit direct/offscreen
  selection remains available, and an unsupported forced-direct request fails.
- `tensor_files_core` stays UI-neutral and owns filesystem/domain behavior.
- Clipboard and DnD use Wayland `wl_data_device`; exported rendering handles
  remain native Vulkan dma-bufs, and KDE blur keeps full region semantics.
- Vulkan resources are created only by the persistent native renderer path.
- Parented dialogs, popup positioning/repositioning, cursor-shape fallback and
  drag icons are owned by the reusable Wayland layer.
- Local and inter-application drag-and-drop share the same Wayland
  source/offer, MIME-pipe and drop lifecycle after the local press threshold;
  scene state only owns the pre-protocol gesture, preview and target policy.
- Portal and privileged-helper binaries remain separate integration pieces.

## Source Layout

```text
src/
  lib.rs                         UI-neutral core exports
  main.rs                        File-manager UI entry point
  windowing.rs                   Window/input/clipboard integration
  windowing_event_loop.rs        Tensor Files scheduling and event translation
  windowing_types.rs             Tensor Files-owned window/input vocabulary
  core.rs                        Core module re-exports
  cli.rs                         Shared CLI parsing entry point
  cli/
    args.rs                      Manager/chooser argument parsing
  core/                          Directory, pane, operations, launcher,
                                 Places, devices, thumbnails, trash, D-Bus
  ui/                            Tensor Files-owned UI modules
  bin/
    tensor-files-xdp-filechooser.rs      XDG Desktop Portal FileChooser backend
    tensor-files-privileged-helper.rs    D-Bus helper for privileged operations
../../crates/
  vulkan-renderer/               Reusable Vulkan 1.4 rendering standard/backend
  wayland-client-runtime/        Reusable SCTK Wayland protocol/event crate
```

## Build And Run

```bash
cargo run -p tensor-files --bin tensor-files -- --view compact /etc
cargo test -p tensor-files --bin tensor-files
scripts/check-rust-file-lines.sh
```

Run these commands from the workspace root. Every Rust source file has a strict
800-line limit. The line gate has no legacy
exceptions and must pass before changes are merged.

Because `default-run` is `tensor-files`, this also starts the current shell:

```bash
cargo run -p tensor-files -- /etc
```

## Architecture Notes

- Pane state is routed by stable pane identity and stored through reusable pane
  containers, so split panes use the same state/projection/slot-pool path.
- Hot item views are retained and virtualized: visible-slot reuse, cached
  projection, cached text/icon atlas work, and explicit scroll metrics.
- The UI hot path uses MIME/icon role reuse by role + size, queued
  read-ahead, dirty-subrect atlas uploads, and tighter icon theme cache
  ownership.
- Core behavior follows Dolphin as the first reference for file-manager
  semantics, while the UI owns rendering, hit testing, DPI, input routing,
  overlays, and telemetry.

## Reference Docs

- [DEVICES_REFERENCE.md](../../docs/tensor-files/DEVICES_REFERENCE.md) — devices and Places
  behavior.
- [NETWORK_REFERENCE.md](../../docs/tensor-files/NETWORK_REFERENCE.md) — network locations
  behavior.
- [PERFORMANCE_ALIGNMENT.md](../../docs/tensor-files/PERFORMANCE_ALIGNMENT.md) — Dolphin-first
  performance reference policy.
- [TRASH_REFERENCE.md](../../docs/tensor-files/TRASH_REFERENCE.md) — trash behavior.
