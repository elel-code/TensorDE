# Testing

Tensor borrows test strategy from Niri and Hyprland without copying implementation code.

- Pure layout/state tests cover empty, singleton, uneven, invalid, and boundary inputs.
- Scrolling tests cover focus visibility, persistent workspace offsets, oversized columns, and
  full-geometry versus visible-clip output. Grid and master-stack tests apply view min/max
  constraints after deterministic track allocation.
- ECS tests assert stable IDs, lifecycle transitions, workspace moves, focus uniqueness, and
  geometry ordering rather than Bevy internals.
- KDL tests separate valid documents, malformed syntax, schema errors, includes, and reload races.
- IPC tests cover fragmented reads, multiple frames per read, malformed/oversized input, request-ID
  round trips, permissions, and socket ownership.
- Nested Wayland tests are added when globals and dispatch state exist; pure geometry never requires
  a compositor session.
- Vulkan tests are capability-gated and must report a missing descriptor heap explicitly.
- Device-selection tests cover explicit DRM-node filtering, incomplete primary/render identities,
  and invalid configured node paths without requiring a Vulkan driver.
- Native interop tests reject each missing external-memory, dma-buf, modifier, foreign
  queue-family, external-semaphore, and bidirectional `SYNC_FD` capability independently.
- Native format tests keep Vulkan import and output-export roles distinct and reject unsupported
  fourccs, mismatched modifiers, non-exportable images, non-scanout GBM paths, and mismatched plane
  topology. Preference ordering must be deterministic regardless of probe order.
- Startup-gate tests prove that runtime preparation, process-environment publication, active
  user-manager publication, and readiness cannot be skipped or reordered before session autostart.
  Check and non-session modes must never receive an autostart permit.
- Output lifecycle tests drive synthetic connector events through Smithay `Output`/`Space` state;
  they must cover connect, mode change, deterministic reflow, and disconnect without real DRM.
- Output policy tests retain incomplete connector snapshots while excluding them from scanout, and
  verify deterministic planning and disconnect-before-connect reconciliation across DRM devices.
- Scene snapshots are appropriate when many coordinates or render decisions form one behavior.

Every change runs:

```sh
cargo fmt --all -- --check
uv run scripts/check_file_lines.py
cargo test --workspace --all-targets
```

The IPC tests cover fragmented and coalesced frames, multiple requests on one non-blocking client,
version rejection, layout mutation, and graceful shutdown. A running session can be queried with
`tensor-msg --socket "$TENSOR_IPC_SOCKET" get-state`; use `tensor-msg --socket "$TENSOR_IPC_SOCKET"
quit` for a manual smoke-test shutdown.
