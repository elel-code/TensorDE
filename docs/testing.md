# Testing

Tensor borrows test strategy from Niri and Hyprland without copying implementation code.
The local Nourish checkout is used for the ECS and retained-scene contracts. The converted
behavioral suite lives in [`tests/reference_contracts.rs`](../tests/reference_contracts.rs), with
one child module per reference project; it is an ordinary Tensor integration test and never links
or executes an upstream test fixture.

## Reference-to-Tensor test conversion

The local checkouts are treated as behavioral references. Their tests are translated into Tensor
contracts over stable IDs, ECS snapshots, KDL values, Vulkan capability records, and Smithay state;
the reference projects are never linked into the build and their fixtures are not copied.

| Reference behavior | Tensor contract | Current tests |
| --- | --- | --- |
| Niri window opening, configure/ack and output removal | one configure size drives `Space`, ECS geometry, and output lifecycle | `tests/reference_contracts/niri.rs`, `protocol::runtime`, `ecs::world`, `protocol::state` |
| Niri dma-buf feedback and import failure paths | feedback exists only for a non-empty import contract; malformed explicit buffers fail before notifier success | `protocol::globals::dmabuf`, `render::vulkan::import` |
| Niri transaction/damage sequencing | first frame is full damage, movement damages old/new bounds, prepared frames can abort | `scene::damage`, `render::frame` |
| Hyprland layout, workspace and multi-output regressions | deterministic layout names, track constraints, output-plan ordering and disconnect-before-connect diff | `tests/reference_contracts/hyprland.rs`, `layout::policy`, `layout::scrolling`, `backend::output` |
| Hyprland IPC and client protocol checks | bounded framed requests, request IDs, version errors, and protocol-global ownership | `tests/reference_contracts/hyprland.rs`, `ipc`, `compositor::root`, `protocol::globals` |
| Nourish 2-D scene/ECS invariants | stable view IDs, unique focus, lifecycle invalidation, geometry independent of draw order | `tests/reference_contracts/nourish.rs`, `ecs::world`, `scene::model` |
| Nourish Vulkan memory/target boundaries | explicit modifier, fd-memory compatibility, plane topology and deferred resource retirement | `render::format`, `render::vulkan::target`, `render::vulkan::import` |

The reference modules deliberately assert Tensor invariants rather than upstream implementation
details: Niri's configure/ack behavior becomes a geometry-and-scene contract, Hyprland's monitor
and control tests become deterministic layout/IPC contracts, and Nourish's world tests become ECS
lifecycle plus retained-scene contracts. When a reference behavior is not implemented yet (for
example layer-shell or multi-plane client imports), it remains a documented gap instead of being
represented by a vacuous passing test.

Hardware-dependent tests remain split into a deterministic state-machine layer and an optional TTY
smoke layer. A missing Vulkan descriptor heap or a missing native dma-buf capability is a reported
selection result, never a silently skipped compatibility path.

- Pure layout/state tests cover empty, singleton, uneven, invalid, and boundary inputs.
- Scene tests cover stable node ordering, independent draw order, effect-bound expansion, first
  frame/full damage, old/new movement damage, region coalescing, and blur dependency propagation.
- Scrolling tests cover focus visibility, persistent workspace offsets, oversized columns, and
  full-geometry versus visible-clip output. Grid and master-stack tests apply view min/max
  constraints after deterministic track allocation.
- ECS tests assert stable IDs, lifecycle transitions, workspace moves, focus uniqueness, and
  geometry ordering rather than Bevy internals.
- KDL tests separate valid documents, malformed syntax, schema errors, includes, and reload races.
- IPC tests cover fragmented reads, multiple frames per read, malformed/oversized input, request-ID
  round trips, permissions, and socket ownership.
- Nested Wayland tests submit real XDG min/max constraints and assert that one layout result drives
  the configure size, Smithay `Space` location, and retained ECS snapshot. Pure geometry never
  requires a compositor session.
- Protocol-global tests bind viewporter, fractional-scale, xdg-decoration, primary selection,
  relative pointer, and pointer gestures from a real client. They assert preferred-scale and
  decoration configure events, including protocol-correct child-object destruction order.
- Vulkan tests are capability-gated and must report a missing descriptor heap explicitly.
- Device-selection tests cover explicit DRM-node filtering, incomplete primary/render identities,
  and invalid configured node paths without requiring a Vulkan driver.
- Native interop tests reject each missing external-memory, dma-buf, modifier, foreign
  queue-family, external-semaphore, and bidirectional `SYNC_FD` capability independently.
- Descriptor-heap renderer tests cover resource/sampler heap limits, embedded-sampler reservation,
  user-range-relative push-index arithmetic, first-use `UNDEFINED + FOREIGN` acquisition, and the
  retained `GENERAL + FOREIGN` path after a successful submission.
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
cargo test --workspace --all-targets --features systemd
cargo test --workspace --all-targets --no-default-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

The IPC tests cover fragmented and coalesced frames, multiple requests on one non-blocking client,
version rejection, layout mutation, and graceful shutdown. A running session can be queried with
`tensor-msg --socket "$TENSOR_IPC_SOCKET" get-state`; use `tensor-msg --socket "$TENSOR_IPC_SOCKET"
quit` for a manual smoke-test shutdown.
