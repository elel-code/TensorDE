---
name: tensor-compositor
description: Use when changing the Tensor Wayland compositor, its direct Wayland and shared vulkan-renderer integration, Bevy ECS state, KDL configuration, startup sequence, IPC, or optional systemd and xdg-desktop-portal gates. Enforces the repository's completion-only, descriptor-heap-only, and modern Rust module contracts.
---

# Tensor Compositor Engineering

Treat Tensor as a pre-release compositor with a deliberate final architecture. Breaking refactors
are preferred over compatibility layers that would constrain the renderer or IPC design.

## Hard Contracts

- Do not depend on or import Smithay and do not add an adapter, compatibility feature, or fallback
  for it. Use direct `wayland-server` dispatch and Tensor-owned input/session/DRM/XWayland state.
- Compio is submit-to-complete with io_uring. Keep defaults and `polling` disabled; do not add a
  readiness reactor or marker runtime.
- Use Vulkanalia only through the shared `vulkan-renderer`, with `VK_EXT_descriptor_heap` as a
  required, first-class `DescriptorHeap`. Tensor must not depend on Vulkanalia directly. Never add
  descriptor-set or descriptor-buffer fallback paths.
- Keep physical-device enumeration/ranking in the renderer `device` module. The default prefers a
  discrete GPU, then integrated/virtual hardware, with CPU last; every candidate must support the
  descriptor heap.
- Use `bevy_ecs` directly, not the Bevy engine. Keep Wayland/non-thread-safe handles outside normal
  components, in protocol-owned state or `NonSend` resources. Components should be IDs, lifecycle
  state, geometry, and render extraction data.
- Parse user configuration through the shared `tensor-kdl` crate. Configuration loading and
  validation must be independent of renderer initialization and support an explicit path plus a
  deterministic default. Preserve structured named-source diagnostics through the config boundary;
  Tensor Shell owns visual/accessibility notification. Do not add a TOML compatibility parser.
- Keep IPC versioned, request-ID based, length-prefixed, bounded, and structurally errorful over a
  private Unix socket. Do not invent compatibility shims for a new protocol.
- Make systemd notification and xdg-desktop-portal/PipeWire support optional feature boundaries.
  Portal code must not own renderer or Wayland internals.
- Keep dependency ranges broad but bounded by compatible major/minor versions; do not use `"*"`.

## Rust Layout

Use `src/foo.rs` as the parent and `src/foo/*.rs` for children. Parent files declare modules and
re-export the public surface. Do not use `mod.rs`. Split hand-written files before 800 lines;
generated bindings and explicit fixtures are the only exclusions.

## Startup Review

When modifying startup, preserve the gate order: CLI/environment, KDL load and validation,
logging, Compio completion loop/direct Wayland display, Vulkan descriptor-heap probe, IPC bind,
ECS scene construction,
watchers/signals, then the event loop. `READY=1` is emitted only after every required gate passes.

## Validation

Run:

```sh
cargo fmt --all
uv run scripts/tensor/check_file_lines.py
uv run scripts/tensor/check_crate_boundaries.py
cargo test -p tensorland --all-targets
```

For IPC, renderer, or startup changes, add tests for malformed input, unavailable capabilities,
socket ownership, and partial initialization. Read `AGENTS.md` for the full repository contract.

See [references/testing.md](references/testing.md) for the Niri/Hyprland-derived test matrix.

Use module-first commit messages such as `ecs: stabilize workspace ordering`; do not require
`feat():` Conventional Commit prefixes.
