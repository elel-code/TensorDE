# Tensor Compositor

Tensor is an early-stage Wayland compositor written in Rust. Its intended stack is:

- Tensor-owned `wayland-server` protocol state, rootless XWayland/XWM, input/session ownership,
  and atomic DRM/KMS submission.
- Compio completion dispatch with an io_uring-only Linux product driver.
- Vulkanalia for a custom Vulkan renderer.
- A bindless descriptor heap backed by `VK_EXT_descriptor_heap`.
- Pluggable layouts: scrolling 1D, spatial 2D, and master-stack.

Design records live in `docs/`: [architecture](docs/architecture.md),
[rendering](docs/rendering.md), [startup](docs/startup.md), [configuration](docs/configuration.md),
[IPC/portal gates](docs/ipc-and-portal.md), [testing](docs/testing.md), and
[contributing](docs/contributing.md).

The repository currently contains Tensor-owned protocol state (compositor, xdg-shell, SHM, output,
seat, selection, and data-device globals), a direct rootless XWayland process and XWM,
startup, bounded IPC framing, a Bevy ECS scene world, tested layout geometry, tty device/session
ownership, Tensor-owned atomic KMS output submission, and a Vulkanalia renderer. The renderer
allocates explicit-modifier output dma-bufs, samples imported one-plane RGB client buffers through
the descriptor heap, and integrates `wp_linux_drm_syncobj_v1` acquire/release fences without a CPU
wait or descriptor-set fallback. Toplevel, subsurface, and popup trees are flattened into the
value-only ECS scene with transaction-aware synchronized commits and popup-aware damage.
Frame callbacks are released once atomic KMS accepts the frame; `wp_presentation` feedback completes
only against the matching page flip.
Multi-plane formats, implicit-sync clients, and damage-driven partial rendering remain open renderer
gates.

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
cargo run -- --config examples/config.toml --check
```

For a direct DRM/KMS smoke test, switch to a Linux virtual terminal, log in as
the normal desktop user, and run `uv run scripts/tty.py`. The launcher builds
the compositor, keeps automatic GPU selection intact, and starts the compiled
`tensor-compositor --session` binary. Hardware smoke tests stop after 20
seconds by default: they request orderly shutdown with `SIGTERM`, then force
shutdown only after a grace period. Use `--forever` only for an intentional
persistent session; `Ctrl`+`Alt`+`F1` through `F12` are always compositor-owned
VT recovery keys. The launcher refuses to enter the KMS event loop from a
terminal emulator. Use `--render-device /dev/dri/renderD*` only to pin a
hybrid-GPU test, and `--no-xwayland` to isolate the native Wayland path.
`--check` remains safe to run from an ordinary terminal. In a TTY launch,
Tensor itself appends tracing records to `artifacts/logs/tensor-tty.log` through
its bounded Compio asynchronous drain; the launcher only watches that file for readiness
and never relays compositor output through a terminal pipe. Its small control
and client diagnostic log is kept separately in
`artifacts/logs/tensor-tty.launcher.log`, so a smoke-test result can be
inspected after returning to the development session without terminal-output
backpressure stalling shutdown.

For the native presentation gate, use `uv run scripts/tty.py --dmabuf-smoke`.
After Tensor creates its socket, the launcher runs a GBM client on the exact
render node advertised by Tensor's linux-dmabuf feedback. It succeeds only
after explicit-modifier dma-buf import, XDG configure, frame callbacks,
`wp_presentation` completion, and release of an older `wl_buffer`; it has no
SHM or implicit-modifier fallback.

For the first interactive client test, leave XWayland enabled (the default) and
run `uv run scripts/tty.py --ghostty --duration 30` from a virtual terminal.
The launcher waits until Tensor has entered its compositor event loop before
starting a fresh Ghostty. It supplies Tensor's `WAYLAND_DISPLAY` session value
and lets Ghostty choose its usual backend; it does not set `GDK_BACKEND`. A
stale `DISPLAY` belonging to the suspended host session is removed, matching
Tensor's own autostart environment hygiene. `--gtk-single-instance=false` only
prevents an already-running host Ghostty from receiving the request over D-Bus;
it does not force the client backend. The bounded run returns to the prior
session automatically; use `--forever` only after the first visual check
succeeds.

Add `--fcitx` to make the TTY launcher attach the existing Fcitx daemon to
Tensor before Ghostty starts, and use `--application /absolute/path/to/app` to
exercise a different native Wayland client. Both routes remove the suspended
host `DISPLAY`; neither turns the test into an X11 fallback.

`TENSOR_LAYOUT` accepts `scrolling-1d`, `spatial-2d`, and `master-stack`.
`TENSOR_GPU` accepts `discrete` (default), `integrated`, and `any`; every choice still requires
the complete native renderer gate.
`TENSOR_RENDER_DEVICE` optionally pins the common Vulkan/tty DRM primary or render node;
without it Vulkan capability filtering and GPU ranking select the pair.
The file format is TOML parsed by `serde`/`toml`; `TENSOR_CONFIG` and `--config` select a file, with
`$XDG_CONFIG_HOME/tensor/config.toml` as the default. Runtime control remains the versioned IPC
protocol rather than a second configuration dialect.

## Architecture

```text
Compio CQEs + direct Wayland/input dispatch
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
               Tensor DRM/KMS
```

The module boundaries are deliberate ownership boundaries:

- `src/protocol.rs`: direct Wayland protocol ownership; `RuntimeState` serializes protocol, input, popup,
  and ECS lifecycle transitions.
- `src/ipc.rs`: versioned compositor control protocol over a bounded Unix-socket framing layer.
- `src/ecs.rs`: Bevy ECS components and deterministic scene/layout state.
- `src/scene.rs`: one-shot render scene extraction, effect bounds, and damage snapshots.
- `src/render.rs`: Vulkan device selection, descriptor heaps, imported client images, native output
  image lifetime, explicit synchronization, and frame submission.
- `src/layout.rs`: constrained geometry, per-workspace scrolling state, and layout snapshots shared
  by scene extraction, damage, effects, and input policy.
- `src/config.rs`: process configuration.
- `src/startup.rs`: CLI, TOML load, capability gates, and startup sequencing.
- `src/compositor.rs`: top-level composition root.
- `crates/tensor-util`: dependency-light geometry primitives shared across modules.

## Roadmap

1. Add multi-plane/YUV client import and an explicit policy for implicit dma-buf reservation fences.
2. Add damage-driven partial rendering and per-output redraw scheduling around the existing
   timeline/KMS presentation model.
3. Add direct-scanout candidate selection inside the Tensor tty/KMS adapter.
4. Complete rootless XWayland surface association using the same protocol-owned lifecycle and
   stable ECS view IDs.
5. Add the dedicated xdg-desktop-portal/PipeWire gate for screencasting without leaking internal
   handles into IPC or ECS.

## IPC contract

The control socket uses a little-endian `u32` length prefix followed by one JSON envelope. Frames
are capped at 1 MiB, carry a protocol version and request ID, and return structured errors. The
server never removes an existing socket when binding fails, and its destructor removes only the
socket inode it created. This is intentionally a new protocol surface; compatibility shims are not
part of the initial design.

`tensor-msg` is the matching CLI for `ping`, `get-state`, `set-layout`, `spawn`, and graceful
`quit`. Server accept/read/write operations complete on the dedicated Compio runtime; only decoded
values cross to the compositor thread. The CLI uses the same framed protocol and never shells out.

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

The default build enables Tensor's rootless XWayland process, direct XWM, and Compio-completed X11
socket operations.
`xwayland false` or `TENSOR_XWAYLAND=off` disables it. X11 window-manager policy and surface
association will use the protocol owner's stable view index; it is not emulated by a second
backend. Tensor has no X11 compositor backend and refuses `--session` startup when it detects an
inherited X11-only session.

## XDP portal gate (reserved boundary)

XDP here means xdg-desktop-portal integration, following Niri's dedicated screencast feature. The
future portal adapter will be an optional D-Bus/PipeWire boundary: it can request a controlled
scene capture, but it cannot receive renderer handles, Wayland resources, or unrestricted ECS access.
