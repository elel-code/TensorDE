# Tensor Compositor

Tensor is an early-stage Wayland compositor written in Rust. Its intended stack is:

- Smithay for Wayland protocol state, input, DRM/KMS integration, and the event loop.
- Vulkanalia for a custom Vulkan renderer.
- A bindless descriptor heap backed by `VK_EXT_descriptor_heap`.
- Pluggable layouts: scrolling 1D, spatial 2D, and master-stack.

Design records live in `docs/`: [architecture](docs/architecture.md),
[rendering](docs/rendering.md), [startup](docs/startup.md), [configuration](docs/configuration.md),
[IPC/portal gates](docs/ipc-and-portal.md), [testing](docs/testing.md), and
[contributing](docs/contributing.md).

The repository currently contains the long-lived Smithay protocol state machine (compositor,
xdg-shell, SHM, output, seat, selection, and data-device globals), rootless XWayland process
startup, bounded IPC framing, a Bevy ECS scene world, tested layout geometry, tty device/session
ownership, and a Vulkanalia device gate. KMS outputs and Vulkan frame submission are not connected
yet; their fourcc/modifier path is already validated across Vulkan, GBM, and primary KMS planes.

## Requirements

- Rust 1.97 or newer.
- Linux for the DRM/KMS backend. Protocol and layout tests remain platform-light.
- A Vulkan 1.4 loader and driver providing descriptor heap, dma-buf, DRM modifier, foreign
  queue-family, and sync-file interop for the renderer target.

The renderer targets Vulkan 1.4 and requires `VK_EXT_descriptor_heap`, external dma-buf memory,
explicit DRM modifiers, `VK_EXT_queue_family_foreign`, and importable/exportable `SYNC_FD`
semaphores. Descriptor sets are not a supported backend and are intentionally not planned as a
future compatibility path. Device selection fails before logical-device creation when any native
renderer capability is unavailable, including the lack of a renderable and dma-buf-exportable
explicit DRM modifier.

## Quick start

```sh
cargo test
cargo run -- --check
TENSOR_LAYOUT=spatial-2d cargo run -- --check
cargo run -- --config examples/config.kdl --check
```

`TENSOR_LAYOUT` accepts `scrolling-1d`, `spatial-2d`, and `master-stack`.
`TENSOR_GPU` accepts `discrete` (default), `integrated`, and `any`; every choice still requires
the complete native renderer gate.
`TENSOR_RENDER_DEVICE` optionally pins the common Vulkan/Smithay DRM primary or render node;
without it Vulkan capability filtering and GPU ranking select the pair.
The file format is KDL parsed by `knus`; `TENSOR_CONFIG` and `--config` select a file, with
`$XDG_CONFIG_HOME/tensor/config.kdl` as the default.

## Architecture

```text
Smithay protocol/input events
             |
             v
       Bevy ECS state
        /          \
 layout systems  scene extraction
                       |
                       v
          Vulkanalia renderer
       descriptor heap -> frame graph
                       |
                dma-buf + fence
                       |
               Smithay DRM/KMS
```

The module boundaries are deliberate ownership boundaries:

- `src/protocol.rs`: Smithay/calloop ownership; `RuntimeState` serializes protocol, input, popup,
  and ECS lifecycle transitions.
- `src/ipc.rs`: versioned compositor control protocol over a bounded Unix-socket framing layer.
- `src/ecs.rs`: Bevy ECS components and deterministic scene/layout state.
- `src/scene.rs`: one-shot render scene extraction, effect bounds, and damage snapshots.
- `src/render.rs`: Vulkan target capabilities and, later, device/swapchain lifetime.
- `src/layout.rs`: constrained geometry, per-workspace scrolling state, and layout snapshots shared
  by scene extraction, damage, effects, and input policy.
- `src/config.rs`: process configuration.
- `src/startup.rs`: CLI, KDL load, capability gates, and startup sequencing.
- `src/compositor.rs`: top-level composition root.
- `crates/tensor-util`: dependency-light geometry primitives shared across modules.

## Roadmap

1. Create Smithay DRM surfaces from the existing output plan and negotiated native format lists.
2. Allocate Vulkanalia descriptor-heap output images, export their dma-buf planes, and attach them
   to Smithay-owned KMS framebuffers.
3. Import client dma-bufs, handle explicit synchronization and damage, and submit direct-scanout
   candidates through Smithay's DRM/KMS backend.
4. Connect output/workspace policy and the three layouts to persistent scene extraction and IPC
   commands.
5. Complete rootless XWayland surface association using the same protocol-owned lifecycle and
   stable ECS view IDs.
6. Add the dedicated xdg-desktop-portal/PipeWire gate for screencasting without leaking internal
   handles into IPC or ECS.

## IPC contract

The control socket uses a little-endian `u32` length prefix followed by one JSON envelope. Frames
are capped at 1 MiB, carry a protocol version and request ID, and return structured errors. The
server never removes an existing socket when binding fails, and its destructor removes only the
socket inode it created. This is intentionally a new protocol surface; compatibility shims are not
part of the initial design.

`tensor-msg` is the matching CLI for `ping`, `get-state`, `set-layout`, and graceful `quit`. It uses
the same framed protocol and never shells out.

## systemd (optional)

Enable `--features systemd` at build time to compile environment publication and `sd_notify`.
`tensor-session` is a Rust equivalent of Niri's `niri-session`: it detects a working user manager,
starts `tensor.service`, waits for the compositor, drives `tensor-shutdown.target`, and clears the
published environment. `systemd "auto"` is the default; `enabled` and `disabled` override detection.
The compositor still runs directly without systemd, and `--check` never sends `READY=1`.
Display managers should install `contrib/wayland-sessions/tensor.desktop`; no entry is provided under
`xsessions`, because Tensor is never an X11 session.

Client startup is shell-free. The compositor's `ProcessLauncher` accepts an executable plus an
argument list and uses a double-fork; active systemd sessions place both forked PIDs in a transient
scope through D-Bus. `systemd "enabled"` fails closed if that scope cannot be created, while `auto`
can keep the client direct after reporting the scope failure.

## XWayland, not an X11 session

The default build enables Smithay's rootless XWayland process and event-source bootstrap.
`xwayland false` or `TENSOR_XWAYLAND=off` disables it. X11 window-manager policy and surface
association will use the protocol owner's stable view index; it is not emulated by a second
backend. Tensor has no X11 compositor backend and refuses `--session` startup when it detects an
inherited X11-only session.

## XDP portal gate (reserved boundary)

XDP here means xdg-desktop-portal integration, following Niri's dedicated screencast feature. The
future portal adapter will be an optional D-Bus/PipeWire boundary: it can request a controlled
scene capture, but it cannot receive renderer handles, Smithay objects, or unrestricted ECS access.
