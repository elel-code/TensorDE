---
name: tensor-compositor
description: Use when changing the Tensor Wayland compositor, its Smithay/Vulkanalia renderer, Bevy ECS state, TOML configuration, startup sequence, IPC, or optional systemd and xdg-desktop-portal gates. Enforces the repository's descriptor-heap-only and modern Rust module contracts.
---

# Tensor Compositor Engineering

Treat Tensor as a pre-release compositor with a deliberate final architecture. Breaking refactors
are preferred over compatibility layers that would constrain the renderer or IPC design.

## Hard Contracts

- Use Smithay `master` for Wayland protocol, input, session, and calloop integration.
- Use Vulkanalia with `VK_EXT_descriptor_heap` as a required, first-class `DescriptorHeap`.
  Never add descriptor-set or descriptor-buffer fallback paths.
- Keep physical-device enumeration/ranking in the renderer `device` module. The default prefers a
  discrete GPU, then integrated/virtual hardware, with CPU last; every candidate must support the
  descriptor heap.
- Use `bevy_ecs` directly, not the Bevy engine. Keep Smithay/non-thread-safe handles outside normal
  components, in protocol-owned state or `NonSend` resources. Components should be IDs, lifecycle
  state, geometry, and render extraction data.
- Parse user configuration as TOML with `serde`/`toml`. Configuration loading and validation must be
  independent of renderer initialization and support an explicit path plus a deterministic default.
- Keep IPC versioned, request-ID based, length-prefixed, bounded, and structurally errorful over a
  private Unix socket. Do not invent compatibility shims for a new protocol.
- Make systemd notification and xdg-desktop-portal/PipeWire support optional feature boundaries.
  Portal code must not own renderer or Smithay internals.
- Keep dependency ranges broad but bounded by compatible major/minor versions; do not use `"*"`.

## Rust Layout

Use `src/foo.rs` as the parent and `src/foo/*.rs` for children. Parent files declare modules and
re-export the public surface. Do not use `mod.rs`. Split hand-written files before 800 lines;
generated bindings and explicit fixtures are the only exclusions.

## Startup Review

When modifying startup, preserve the gate order: CLI/environment, TOML load and validation,
logging, calloop/Smithay display, Vulkan descriptor-heap probe, IPC bind, ECS scene construction,
watchers/signals, then the event loop. `READY=1` is emitted only after every required gate passes.

## Validation

Run:

```sh
cargo fmt --all
./scripts/check-file-lines.sh
cargo test --all-targets
```

For IPC, renderer, or startup changes, add tests for malformed input, unavailable capabilities,
socket ownership, and partial initialization. Read `AGENTS.md` for the full repository contract.

See [references/testing.md](references/testing.md) for the Niri/Hyprland-derived test matrix.

Use module-first commit messages such as `ecs: stabilize workspace ordering`; do not require
`feat():` Conventional Commit prefixes.
