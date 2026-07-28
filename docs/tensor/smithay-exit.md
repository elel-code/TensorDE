# Smithay removal contract

Smithay was removed from Tensor's dependency graph on 2026-07-27. This is a completed
architectural break, not a feature configuration: there is no Smithay adapter crate, disabled
Smithay feature, compatibility implementation, or fallback path.

`LICENSES/Smithay-MIT.txt` remains because several Tensor-owned implementations were derived from
or validated against upstream Smithay code. Those source files retain explicit attribution. A
license notice does not grant Smithay runtime ownership and must not be used to justify restoring
the dependency.

## Current ownership

| Boundary | Owner |
|----------|-------|
| Core and extension wire resources | `wayland-server`, `wayland-protocols`, and Tensor dispatch state |
| Surface/subsurface transactions, SHM, XDG, seat, selection, output, capture | `src/protocol` and `tensor-protocol` |
| Geometry and scaling | `tensor-util` |
| Input normalization, device capabilities, and semantic event order | `tensor-event` |
| Session, libinput, udev, DRM scanout, atomic KMS, GBM | Tensor tty backend using native crates |
| Output planning and present policy | `tensor-host`, `tensor-drm`, `tensor-present` |
| Rootless XWayland process, shell association, XWM | Tensor protocol state plus `x11rb` |
| Renderer and synchronization | Vulkanalia renderer; Tensor atomic KMS consumes exported dma-bufs and sync files |
| Asynchronous I/O | Compio completion operations with the io_uring driver |

Thread-affine Wayland, X11, Vulkan, and KMS objects remain on their owning compositor boundary.
Workers publish bounded value messages; they do not acquire Wayland objects or DRM/KMS ownership.

## Completion model

The Linux product path disables Compio defaults and its `polling` fallback. Runtime construction
fails if io_uring cannot be created. `PollFd` in Compio source is a wrapper name for one submitted
`IORING_OP_POLL_ADD`; Tensor consumes its CQE and explicitly rearms the next one-shot operation.
Tensor owns no poll/epoll readiness registry and has no calloop dependency.

Wayland listener accepts and display dispatch, IPC, signalfd, GPU fences, security-context sockets,
udev, libinput, libseat, timerfds, DRM page flips, XWayland displayfd, and X11 event notification all
enter compositor turns through completed operations. X11 property reads use a dedicated
Compio/io_uring connection: `MapRequest` submits a fixed-capacity batch for `WM_TRANSIENT_FOR`,
`WM_NORMAL_HINTS`, and `_NET_WM_STATE`; only the returned pure-value completion can publish the map
request to compositor policy. Property changes use the same channel. Queue failure terminates the
XWayland subsystem; it never falls back to synchronous property reads.

X11 event ordering is authoritative. A `MapRequest` or `ConfigureRequest` without the preceding
`CreateNotify` is an invariant error; Tensor does not issue synchronous attribute/geometry queries
to repair it. The XWM keeps one persistent fixed-capacity event deque, `_NET_WM_STATE` uses a
fixed-capacity atom array, and EWMH client-list publication borrows the stacking store directly.
Either capacity being exceeded fails the XWayland session closed instead of reallocating the hot
path.

## No-reintroduction rules

- No manifest may declare `smithay` or `smithay-drm-extras`.
- No source file may import a `smithay::` path.
- No `tensor-smithay` or equivalent adapter crate may be added.
- No legacy API wrapper, dual implementation, feature-gated fallback, or marker dependency may be
  retained for compatibility.
- Direct native ownership must replace an obsolete boundary in the same change that deletes it.
- Performance-sensitive paths use fixed capacities, borrowed slices, stable IDs, and explicit
  overflow/failure behavior; removal is not permission to add copies or unbounded queues.
- Rustix is preferred for Linux syscalls. Do not add direct `libc` use when rustix exposes the
  operation.

`uv run scripts/tensor/check_crate_boundaries.py` enforces the dependency and import bans for the root
package and every workspace crate.

## Verification

The removal gate is:

```sh
rg -n "smithay::|use smithay|extern crate smithay|smithay\\s*=" \
  Cargo.toml Cargo.lock src crates
cargo tree -i smithay
cargo tree -i calloop
cargo fmt --all -- --check
uv run scripts/tensor/check_file_lines.py
uv run scripts/tensor/check_crate_boundaries.py
cargo test -p tensor-compositor --all-targets
```

The two `cargo tree -i` commands are expected to report that no matching package exists.
