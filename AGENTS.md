# Tensor Engineering Rules

This file is the repository contract for agents and contributors. Make architectural changes
directly when the current shape is wrong; this project is pre-release and does not preserve legacy
APIs for their own sake.

## Core Direction

- **Event layer (Tensor-owned):** `tensor-event` owns value-only events, fixed-capacity
  phase-bucketed queues, coalescing (pointer motion / per-output vblank), and dispatch order.
  Policy must not depend on calloop callback order. See `docs/event-layer.md`.
- **Runtime (Compio = completion model, io_uring driver):** `tensor-runtime` owns Compio workers
  and turn contracts (`run_turn`, `EventfdWake`, `CompletionDriver::IoUring`). Compio is
  **submit → complete**, not a readiness poll loop. On Linux the product driver is **io_uring**;
  Compio's `polling` feature is only an automatic host fallback when io_uring cannot be created —
  never the architecture we design for. Async workers exchange value-only messages only and never
  own Wayland objects or DRM/KMS file descriptors. Do not force native dependencies such as zbus
  onto blocking APIs merely to avoid async, and do not add an unused runtime as a marker dependency.
- **Smithay (mature adapter, transitional):** Smithay `master` still provides Wayland protocol
  objects, input/session/DRM bindings, and (today) a calloop readiness loop. Borrow its patterns
  (sources, bounded channels, idle-after-wait) but route semantics through `tensor-event`.
  Page-flip and KMS submission stay on the compositor thread; Compio does not own scanout.
  Replacing calloop means expressing the same work as **Compio-completed ops** (io_uring driver),
  not re-homing a poll/epoll readiness registry.
  **Exit path:** policy and value types live in `tensor-host` / `tensor-drm` /
  `tensor-present` / `tensor-input` (and later `tensor-protocol`). Smithay may only appear in
  adapter modules that map to those crates. See `docs/smithay-exit.md`. Do not add new
  compositor policy that depends on `smithay::` types outside adapters.
- Vulkanalia is the renderer binding. The renderer requires Vulkan `VK_EXT_descriptor_heap` and
  models it as a first-class `DescriptorHeap`. Descriptor sets are not a backend and must not be
  added as a compatibility path. Native devices also require external dma-buf memory, explicit DRM
  modifiers, foreign queue-family ownership transfers, external semaphore FDs, and bidirectional
  binary `SYNC_FD` support. Vulkanalia owns rendering and synchronization, then exports dma-bufs and
  sync-file fences back to Smithay; it must not bypass Smithay to own KMS state. Internal timeline
  semaphores never cross the sync-file boundary, which uses binary semaphores.
- Physical-device selection is a renderer submodule and a configuration value. Default ranking
  prefers discrete GPUs, then integrated/virtual hardware, with CPU last; all candidates still need
  the descriptor-heap feature. An eligible native device must expose a complete DRM primary/render
  node pair through `VK_EXT_physical_device_drm`. The Vulkan-selected identity is authoritative for
  the Smithay tty backend; `render-device` constrains both sides and they must never select GPUs
  independently.
- `bevy_ecs` is the long-term ECS kernel. Use only the `bevy_ecs` crate, not Bevy's renderer or
  window stack. Smithay objects and other thread-affine handles stay in `NonSend` resources or the
  protocol layer; ECS components contain stable IDs, state, and renderable geometry.
- TOML parsed with `serde`/`toml` is the user configuration format. Keep parsing, validation,
  defaults, and file watching in the config boundary. Runtime control stays on the versioned IPC
  surface rather than growing a second hot-reload dialect.
- IPC is a versioned Unix-socket protocol with request IDs, bounded length-prefixed frames, and
  structured errors. It is a new protocol surface; do not add compatibility shims prematurely.
- Wayland protocol work follows **wayland-protocols / Smithay-style tiers** (see
  `docs/protocol-surface.md`). Prefer higher tiers; never invent a twin in a lower tier for
  compositor-parity alone:
  1. **Core** — `wayland.xml` (compositor, seat, shm, …)
  2. **Stable standard** — `xdg-*`, mature `wp-*` (viewporter, presentation-time, linux-dmabuf, …)
  3. **Staging / `ext-*`** — ready for adoption; implement when the feature is needed (session-lock,
     foreign-toplevel-list, image-copy-capture, background-effect, …)
  4. **Unstable (`z*`)** — legacy only; prefer a staging/stable replacement when one exists
  5. **Community** — `wayland-protocols-wlr` / plasma / misc; use only when no tier-2/3 equivalent
     exists (e.g. `wlr-layer-shell` today) or a critical client cannot bind the standard global
  6. **Proprietary** (hyprland-*, kde server-decoration as policy, …) — out of scope unless
     explicitly productized
  Within the same capability, **`ext-*` / staging beats `zwlr_*`**. Capture targets
  `ext-image-copy-capture` / `ext-image-capture-source`, not `zwlr-screencopy`. Smithay modules and
  Dispatch2 are the preferred implementation vehicle; Tensor-owned protocol code stays value-only
  at the ECS/render boundary.
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
2. Resolve the TOML path and load/validate the complete configuration.
3. Initialize logging and diagnostics.
4. Create the event reactor (calloop today; Compio-oriented `tensor-runtime` workers already)
   and Smithay display/protocol state; compositor turns should drain into `tensor-event`.
5. Probe Vulkan and require the descriptor-heap feature before allocating renderer state.
6. Bind the IPC socket and optional portal/systemd adapters.
7. Construct ECS resources/components and the initial scene.
8. Register watchers, signals, and event sources.
9. Publish the session environment, synchronously update an active systemd user manager, notify
   readiness after all required gates pass, then authorize session autostart and enter the
   compositor event loop.

Do not notify readiness before the Wayland socket, renderer, ECS, and IPC gates are complete.
`spawn-at-startup` must require the one-shot startup permit; control-flow ordering alone is not a
gate. `--check`, non-session startup, failed environment publication, and failed readiness must
never launch configured commands.

## Verification

Run `cargo fmt --all`, `uv run scripts/check_file_lines.py`, and `cargo test --all-targets` for every
change. Changes to startup, IPC, ECS, or renderer contracts require focused tests for failure paths,
not only happy-path construction.
