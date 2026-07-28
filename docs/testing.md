# Testing

Tensor borrows test strategy from Niri and Hyprland without copying implementation code.
The local Nourish checkout is used for the ECS and retained-scene contracts. The converted
behavioral suite lives in [`tests/reference_contracts.rs`](../tests/reference_contracts.rs), with
one child module per reference project; it is an ordinary Tensor integration test and never links
or executes an upstream test fixture.

## Reference-to-Tensor test conversion

The local checkouts are treated as behavioral references. Their tests are translated into Tensor
contracts over stable IDs, ECS snapshots, TOML values, Vulkan capability records, and Tensor protocol state;
the reference projects are never linked into the build and their fixtures are not copied.

| Reference behavior | Tensor contract | Current tests |
| --- | --- | --- |
| Niri window opening, configure/ack and output removal | one configure size drives `WindowSpace`, ECS geometry, and output lifecycle | `tests/reference_contracts/niri.rs`, `protocol::runtime`, `ecs::world`, `protocol::state` |
| Niri XWayland fractional scaling | rootless X11 surfaces reuse integer client-buffer scale, fractional output conversion, and the linear surface sampler without a parallel X11 coordinate path | `protocol::runtime`, `protocol::handlers::xwayland`, `protocol::state::xwayland`, `render::frame::plan`, `render::vulkan::pipeline` |
| Niri dma-buf feedback and import failure paths | feedback exists only for a non-empty import contract; malformed explicit buffers fail before notifier success | `protocol::globals::dmabuf`, `render::vulkan::import` |
| Smithay/Niri explicit-sync lifecycle | syncobj is advertised only with a capable DRM owner; failed submits preserve acquire state and release follows the latest GPU read | `protocol::globals::syncobj`, `protocol::state::sync`, `render::vulkan::sync`, `render::vulkan::frame` |
| Niri/Nourish presentation lifecycle | frame callbacks follow accepted KMS commits while feedback follows the exact output/timeline flip; failures discard it and resume never reuses uncertain slots | `protocol::runtime`, `protocol::state::presentation`, `backend::tty::kms` |
| Smithay/Niri surface-tree transactions | subsurface order is stable, synchronized children defer to the parent transaction, and popups escape tile clipping without escaping output damage | `protocol::state::surfaces`, `protocol::state::tree`, `render::frame::plan`, `scene::damage` |
| Niri transaction/damage sequencing | first frame is full damage, movement damages old/new bounds, prepared frames can abort | `scene::damage`, `render::frame` |
| Niri/Hyprland/Nourish pointer and cursor paths | relative motion may cross adjacent outputs but not gaps; absolute coordinates and invalid axes stay bounded; the topmost software cursor damages both positions and a GPU-busy repaint remains pending | `protocol::input`, `protocol::cursor`, `protocol::state::output`, `render::frame::cursor_tests` |
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

## Native dma-buf presentation gate

From a Linux virtual terminal owned by the normal desktop user, run:

```sh
uv run scripts/tty.py --dmabuf-smoke
```

The launcher waits for the new Tensor socket and then starts
`tensor-dmabuf-smoke`. The client consumes Tensor's linux-dmabuf v4 default
feedback, resolves its advertised main device to the matching `renderD*` node,
and allocates only explicit-modifier GBM buffers on that node. It has no SHM,
implicit-modifier, or alternate-GPU fallback.

Success requires all of the following for the same native surface: accepted
dma-buf creation, XDG configure, frame callbacks, `wp_presentation` feedback,
and release of an older `wl_buffer`. Tensor itself writes its tracing stream
to `artifacts/logs/tensor-tty.log` through its bounded Compio asynchronous
drain; the launcher only tails new records for readiness and keeps control/client
diagnostics in `artifacts/logs/tensor-tty.launcher.log`. It returns the smoke
client's failure status, and neither logging path echoes compositor output onto
the graphics TTY, avoiding terminal-output backpressure during shutdown.
Each `tty.py` invocation also supplies a unique private IPC endpoint through
`TENSOR_IPC_SOCKET`. This prevents a suspended desktop compositor or a stale
socket from blocking the TTY smoke run; it does not weaken IPC's rule that an
ordinary compositor startup never removes an existing control socket.

## Ghostty native-client smoke

From a Linux virtual terminal, run:

```sh
uv run scripts/tty.py --ghostty --duration 30
```

Do not add `--no-xwayland`: the default configuration starts rootless XWayland
as part of this session. The launcher waits until Tensor reports that it has
entered its compositor event loop, then starts a new Ghostty with
`WAYLAND_DISPLAY` set to Tensor's socket. Ghostty retains its normal backend
selection; Tensor does not set `GDK_BACKEND`. The launcher's parent process is
not part of the new session, so it removes the host session's stale `DISPLAY`,
just as Tensor's `ProcessLauncher` clears managed session values before
installing Tensor's published environment. `--gtk-single-instance=false` only
ensures that an existing host Ghostty cannot receive the request over D-Bus;
it does not select a client backend.

This is a native Wayland rendering and input test, not an X11-client test. The
Tensor log should contain `entering compositor event loop`; the launcher log
should contain `client launch gate opened` and `starting Ghostty with its
normal backend selection`. The terminal must appear and accept input before
the bounded launch restores the previous session. Ghostty's stdout and stderr
are retained in the launcher log for diagnosis.

The compositor-owned arrow must be visible as soon as libinput publishes a
pointer capability, including when that device appears after the first output
frame. Moving it must erase the previous arrow location as well as draw the new
one; named and client cursor-image requests currently retain this visible vector
fallback until cursor raster upload is implemented.

An X11-only application is still required to exercise XWayland client mapping
and rendering itself.

- Pure layout/state tests cover empty, singleton, uneven, invalid, and boundary inputs.
- Scene tests cover stable node ordering, independent draw order, effect-bound expansion, rounded
  focus-ring inner/outer physical geometry (including fractional scale and output clipping), first
  frame/full damage, old/new movement damage, popup bounds outside layout tiles, region coalescing,
  and blur dependency propagation.
- Scrolling tests cover focus visibility, persistent workspace offsets, oversized columns, and
  full-geometry versus visible-clip output. Grid and master-stack tests apply view min/max
  constraints after deterministic track allocation.
- ECS tests assert stable IDs, lifecycle transitions, workspace moves, focus uniqueness, and
  geometry ordering rather than Bevy internals.
- TOML tests separate valid documents, malformed syntax, schema errors, and reload races.
- IPC tests cover fragmented reads, multiple frames per read, malformed/oversized input, request-ID
  round trips, permissions, socket ownership, fixed connection capacity, real Compio socket
  completions, and response-before-shutdown ordering.
- Signal tests direct a blocked termination signal at the runtime thread and require a submitted
  Compio signalfd read to return its value.
- GPU fence tests require a submitted one-shot io_uring wait to remain pending until the sync-file
  test fd signals, then preserve its output and timeline value.
- Nested Wayland tests submit real XDG min/max constraints and assert that one layout result drives
  the configure size, Tensor `WindowSpace` location, and retained ECS snapshot. Pure geometry never
  requires a compositor session. The same client lifecycle exercises Tensor `ProtocolWindow`
  commit-bbox caching, preferred output state traversal, frame callbacks, presentation feedback,
  activation, and teardown through Tensor `ProtocolWindow` state.
- Popup lifecycle coverage creates a real two-level XDG tree, establishes nested explicit grabs,
  verifies Tensor's child-first borrowed iteration order, and tears it down without frame staging.
  A second tree destroys its parent first and must receive `not_the_topmost_popup` while Tensor
  removes the complete descendant topology immediately.
- Stable XDG-shell wire coverage checks Tensor-owned error attribution for unconfigured buffers,
  invalid configure serials, defunct role objects, and incomplete positioners. A remap test retains
  an in-flight configure across detach, proves detach emits no replacement configure, then verifies
  that the old-generation ACK cannot authorize a buffer after the required new empty commit.
- Layer-shell lifecycle coverage creates a real top-anchored client surface, verifies its configure
  uses the fractional-scale logical output width, asserts its exclusive zone reshapes the workspace,
  and confirms protocol destruction removes the Tensor layer map and restores the full output zone.
  Failure coverage rejects a buffer before the first configure ACK, zero dimensions without
  opposite anchors, invalid exclusive edges, and zones below `-1` on the owning wire object. A
  remap test proves an old-generation ACK cannot authorize the new buffer and that detach itself
  does not emit the next initial configure. Output removal sends `closed`, and a real parentless
  xdg-popup is transferred into the Tensor layer tree before its initial commit.
- Protocol-global tests bind the full `ProtocolCapabilities` set (core shell extensions plus
  pointer-constraints, idle-inhibit, single-pixel-buffer, keyboard-shortcuts-inhibit, tablet,
  text-input, input-method, virtual-keyboard, session-lock, security-context,
  foreign-toplevel-list, xdg-foreign, system-bell, pointer-warp, content-type, alpha-modifier,
  toplevel-icon/tag, and wlr-layer-shell). They assert preferred-scale (including `150/120`),
  decoration configure, monotonic clock, and discarded-feedback events, including
  protocol-correct child-object destruction order. Layer draw-order helpers assert Overlay sits
  above Top and Bottom in the value-only scene merge.
- XWayland scaling tests keep rootless X11 buffers on the ordinary surface path: integer client
  buffer scale, `N/120` output geometry, outward damage coverage, and linear final sampling are one
  contract. X11 provenance must not enable a nearest-neighbor default or a second coordinate model.
- XWayland lifecycle tests cover the two-signal mapped-window association gate, logical configure
  conversion, and teardown before Tensor removes the X11-to-Wayland association. Runtime wiring
  creates the XWM after XWayland readiness; hardware execution remains a TTY smoke test.
  Override-redirect coverage additionally requires map and association state, a managed
  `WM_TRANSIENT_FOR` ancestor, relative logical offsets, and X11 stacking. It verifies that such
  windows add no ECS view or independent X11 coordinate path, and that owner or popup teardown
  detaches their protocol/input/render state safely.
- Normal X11 `WM_TRANSIENT_FOR` dialog coverage verifies the separate attached-view model: dialogs
  do not consume tiled placements, retain independent scene/input/synchronization state, inherit
  focused-owner scrolling, and move or disappear safely with their owner. Tests reject attachment
  cycles, cross-workspace parents, missing owners, and accidental global-X11-position fallbacks.
- Input lifecycle coverage verifies that mapping selects an ECS root before an input device exists,
  then a late keyboard capability restores the Tensor keyboard target only after the initial XDG
  configure. Focus transitions deactivate the old toplevel, activate and raise the new attachment
  family, and closing the active view restores a live successor without an empty-focus gap. A late
  pointer capability schedules its first software-cursor frame; its removal schedules an overlay-free
  frame, and cursor motion damages the old and new physical bounds while drawing above client content.
  X11 activation routes through the `X11Surface` keyboard target so its ICCCM focus handshake is
  retained without changing the Wayland logical pointer-coordinate path.
- Focus-ring frame-plan and Vulkan-record tests verify the same back-to-front contract as Niri's
  element list: a focused view's rounded ring precedes its client content and popup tree, and later
  stacking entries cover the complete earlier view. The cursor remains a final compositor overlay.
- Presentation tests cover output/timeline identity, primary-output intersection selection, refresh
  conversion, hardware-clock flags, surface destruction, output/session discard, and scanout-slot
  quarantine across session resume.
- Vulkan tests are capability-gated and must report a missing descriptor heap explicitly.
- Device-selection tests cover explicit DRM-node filtering, incomplete primary/render identities,
  and invalid configured node paths without requiring a Vulkan driver.
- Native interop tests reject each missing external-memory, dma-buf, modifier, foreign
  queue-family, external-semaphore, and bidirectional `SYNC_FD` capability independently.
- Descriptor-heap renderer tests cover resource/sampler heap limits, embedded-sampler reservation,
  user-range-relative push-index arithmetic, first-use `UNDEFINED + FOREIGN` acquisition, and the
  retained `GENERAL + FOREIGN` path after a successful submission.
- Explicit-sync tests cover acquire import ownership, fragment-stage waits, retry after failed
  submission, latest-repaint release fences, and the no-early-release timeline fallback when
  binary completion export fails.
- Native format tests keep Vulkan import and output-export roles distinct and reject unsupported
  fourccs, mismatched modifiers, non-exportable images, non-scanout GBM paths, and mismatched plane
  topology. Preference ordering must be deterministic regardless of probe order.
- Startup-gate tests prove that runtime preparation, process-environment publication, active
  user-manager publication, and readiness cannot be skipped or reordered before session autostart.
  Check and non-session modes must never receive an autostart permit.
- Output lifecycle tests drive synthetic connector events through Tensor output and `WindowSpace`
  state;
  they must cover connect, mode change, deterministic reflow, and disconnect without real DRM.
- Output policy tests retain incomplete connector snapshots while excluding them from scanout, and
  verify deterministic planning and disconnect-before-connect reconciliation across DRM devices.
- Scene snapshots are appropriate when many coordinates or render decisions form one behavior.

Every change runs:

```sh
cargo fmt --all -- --check
uv run scripts/check_file_lines.py
uv run scripts/check_crate_boundaries.py
cargo test --workspace --all-targets
cargo test --workspace --all-targets --features systemd
cargo test --workspace --all-targets --no-default-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

The IPC tests cover fragmented and coalesced frames, multiple requests on one Compio-completed
client, version rejection, layout mutation, and graceful response-before-stop shutdown. A running
session can be queried with
`tensor-msg --socket "$TENSOR_IPC_SOCKET" get-state`; use `tensor-msg --socket "$TENSOR_IPC_SOCKET"
quit` for a manual smoke-test shutdown.
