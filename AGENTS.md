# Tensor Engineering Rules

This file is the repository contract for agents and contributors. Make architectural changes
directly when the current shape is wrong; this project is pre-release and does not preserve legacy
APIs for their own sake.

## Core Direction

- Smithay `master` owns the Wayland protocol, input, session, calloop, DRM/KMS, GBM, connector,
  CRTC, page-flip, and scanout lifecycle.
- Vulkanalia is the renderer binding. The renderer requires Vulkan `VK_EXT_descriptor_heap` and
  models it as a first-class `DescriptorHeap`. Descriptor sets are not a backend and must not be
  added as a compatibility path. Vulkanalia owns rendering and synchronization, then exports
  dma-bufs and fences back to Smithay; it must not bypass Smithay to own KMS state.
- Physical-device selection is a renderer submodule and a configuration value. Default ranking
  prefers discrete GPUs, then integrated/virtual hardware, with CPU last; all candidates still need
  the descriptor-heap feature.
- `bevy_ecs` is the long-term ECS kernel. Use only the `bevy_ecs` crate, not Bevy's renderer or
  window stack. Smithay objects and other thread-affine handles stay in `NonSend` resources or the
  protocol layer; ECS components contain stable IDs, state, and renderable geometry.
- KDL parsed with `knus` is the user configuration format. Keep parsing, validation, defaults,
  includes, and file watching in the config boundary.
- IPC is a versioned Unix-socket protocol with request IDs, bounded length-prefixed frames, and
  structured errors. It is a new protocol surface; do not add compatibility shims prematurely.
- systemd readiness/activation is optional behind a Cargo feature. Core startup must work without
  systemd. Follow Niri's session lifecycle: a compiled `tensor-session` launcher, a user service,
  graphical-session targets, explicit environment publication, and readiness only after gates.
- XWayland supports individual legacy applications in rootless mode. Tensor never provides an X11
  session or an X11 compositor backend.
- XDP means the xdg-desktop-portal boundary in this project. Portal/D-Bus/PipeWire integration is
  an optional capability gate and must not receive direct renderer or Smithay object ownership.

Dependency declarations should use the broadest compatible major/minor constraint (`"1"`,
`"0.19"`, or `"4"` as appropriate), never an unconstrained `"*"`. Smithay intentionally follows
the upstream `master` branch.

Commit messages use a module-first scope: `where: imperative summary`, for example
`render: require descriptor heap`. Do not enforce `feat():`/Conventional Commits prefixes. Add a
short body when the change has a non-obvious architectural tradeoff, followed by test commands.

Repository automation under `scripts/` is Python run through `uv`. Do not grow long-lived policy in
shell scripts. Session-manager policy belongs in the compiled `tensor-session` binary.

## Module Layout

Use modern Rust's same-name file plus directory form:

```text
src/layout.rs              # parent module
src/layout/policy.rs       # child module
src/layout/geometry.rs     # child module
```

Never create `mod.rs`. Keep parent files focused on declarations and re-exports. Split hand-written
source before it reaches 800 lines; generated protocol bindings and deliberately data-heavy test
fixtures are exempt.

## Startup Order

Keep this order explicit and testable:

1. Parse CLI and environment overrides.
2. Resolve the KDL path and load/validate the complete configuration.
3. Initialize logging and diagnostics.
4. Create the calloop event loop and Smithay display/protocol state.
5. Probe Vulkan and require the descriptor-heap feature before allocating renderer state.
6. Bind the IPC socket and optional portal/systemd adapters.
7. Construct ECS resources/components and the initial scene.
8. Register watchers, signals, and event sources.
9. Publish the session environment, notify systemd after all required gates pass, then enter the
   compositor event loop.

Do not notify readiness before the Wayland socket, renderer, ECS, and IPC gates are complete.

## Verification

Run `cargo fmt --all`, `uv run scripts/check_file_lines.py`, and `cargo test --all-targets` for every
change. Changes to startup, IPC, ECS, or renderer contracts require focused tests for failure paths,
not only happy-path construction.
