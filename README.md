# Tensor Compositor

Tensor is an early-stage Wayland compositor written in Rust. Its intended stack is:

- Smithay for Wayland protocol state, input, DRM/KMS integration, and the event loop.
- Vulkanalia for a custom Vulkan renderer.
- A bindless descriptor heap backed by `VK_EXT_descriptor_heap`.
- Pluggable layouts: scrolling 1D, spatial 2D, and classic master-stack.

Design records live in `docs/`: [architecture](docs/architecture.md),
[startup](docs/startup.md), [configuration](docs/configuration.md),
[IPC/portal gates](docs/ipc-and-portal.md), [testing](docs/testing.md), and
[contributing](docs/contributing.md).

The repository currently contains the architectural skeleton, a real Smithay display/socket
bootstrap, bounded IPC framing, a Bevy ECS scene world, and tested layout geometry. It does not yet
acquire DRM devices or submit Vulkan commands.

## Requirements

- Rust 1.97 or newer.
- Linux for the eventual DRM/KMS backend. The current skeleton and tests are platform-light.
- A Vulkan 1.4 loader and driver advertising `VK_EXT_descriptor_heap` for the renderer target.

The renderer targets Vulkan 1.4 and requires `VK_EXT_descriptor_heap`. Descriptor sets are not a
supported backend and are intentionally not planned as a future compatibility path. Device
selection must fail early when the heap feature is unavailable.

## Quick start

```sh
cargo test
cargo run -- --check
TENSOR_LAYOUT=nourish-2d cargo run -- --check
cargo run -- --config examples/config.kdl --check
```

`TENSOR_LAYOUT` accepts `niri-1d`, `nourish-2d`, and `classic`.
The file format is KDL; `TENSOR_CONFIG` and `--config` select a file, with
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
     descriptor heap -> frame graph -> KMS
```

The initial module boundaries are deliberately narrow:

- `src/protocol.rs`: Smithay/calloop ownership and, later, protocol dispatch.
- `src/ipc.rs`: versioned compositor control protocol over a bounded Unix-socket framing layer.
- `src/ecs.rs`: Bevy ECS components and deterministic scene/layout state.
- `src/render.rs`: Vulkan target capabilities and, later, device/swapchain lifetime.
- `src/layout.rs`: platform-independent layout policy and geometry.
- `src/config.rs`: process configuration.
- `src/startup.rs`: CLI, KDL load, capability gates, and startup sequencing.
- `src/compositor.rs`: top-level composition root.
- `crates/tensor-util`: dependency-light geometry primitives shared across modules.

## Roadmap

1. Add a nested winit backend for fast development and validation.
2. Build Smithay compositor, xdg-shell, seat, output, and data-device state.
3. Add Vulkan instance/device selection with `VK_EXT_descriptor_heap` feature probing.
4. Import dmabufs, handle explicit synchronization, and submit direct-scanout candidates to KMS.
5. Connect the three layout policies to a persistent workspace/view model and IPC commands.
6. Add a libinput + udev + libseat DRM session backend and XWayland support where explicitly desired.
7. Add the dedicated xdg-desktop-portal/PipeWire gate for screencasting without leaking internal
   handles into IPC or ECS.

## IPC contract

The control socket uses a little-endian `u32` length prefix followed by one JSON envelope. Frames
are capped at 1 MiB, carry a protocol version and request ID, and return structured errors. The
server never removes an existing socket when binding fails, and its destructor removes only the
socket inode it created. This is intentionally a new protocol surface; compatibility shims are not
part of the initial design.

## systemd (optional)

Enable `--features systemd` at build time to compile the `sd_notify` adapter. The bootstrap binary
does not claim readiness until the real compositor event-loop handoff is implemented; `--check`
always exits without sending `READY=1`. The compositor does not require systemd and runs without
the feature. A service template is provided in `contrib/systemd/tensor-compositor.service` for
installations that choose to use it.

## XDP portal gate (reserved boundary)

XDP here means xdg-desktop-portal integration, following Niri's dedicated screencast feature. The
future portal adapter will be an optional D-Bus/PipeWire boundary: it can request a controlled
scene capture, but it cannot receive renderer handles, Smithay objects, or unrestricted ECS access.
