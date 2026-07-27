# Smithay exit plan

Tensor currently uses Smithay as a **mature adapter** for remaining Wayland
protocol objects, XWayland, and a residual calloop aggregate. Tensor owns the
DRM/KMS and GBM path. The product goal is to delete Smithay once Tensor-owned
crates cover the same **semantic** surface with equal or better performance.

This document is the exit contract. Work that does not shrink Smithay surface
area or move policy behind these crates is not exit work.

## Non-goals

- Rewrite every protocol handler in one PR.
- Run Vulkan or KMS on Compio worker threads.
- Preserve compatibility or dual implementations during replacement. The new Tensor path and
  deletion of the old adapter land together.

## Target crate map

| Crate | Owns | Must not own |
|-------|------|----------------|
| `tensor-event` | Value events, phases, coalesce, fixed queues | FDs, Wayland objects |
| `tensor-runtime` | Compio workers, bounded bridges | DRM master, WlSurface |
| `tensor-host` | Mode, connector id, format codes, input samples, present intent | libdrm, wayland-server |
| `tensor-drm` | Topology plan, output rules, mode select | Open DRM nodes, atomic |
| `tensor-present` | Slot readiness, present intent queue | GBM bo, CRTC commit |
| `tensor-input` | Device caps + `Sample` → bus events | Seat protocol objects, libinput |
| `tensor-protocol` | Stable surface/buffer IDs, attachment lifecycle, tier policy | Wire objects, ECS entities |
| `tensor-compositor` | Policy, ECS, Vulkan, adapters | — |
| `tensor-smithay` *(future optional)* | Temporary adapter only | Product policy |

## Completion gates (delete Smithay only when all pass)

1. **Policy purity**
   - Output plan / mode / scale / enable live in `tensor-drm` and are unit-tested
     without Smithay.
   - Present slot readiness / intent queue live in `tensor-present`.
   - Event bus never carries `WlSurface`, `DrmDevice`, or Vulkan handles.

2. **Type boundary**
   - `BackendOutputId` ≡ `tensor_host::ConnectorId`.
   - Physical modes are `tensor_host::PhysicalMode` (not `smithay::output::Mode`)
     inside policy modules.
   - DRM fourcc/modifier for negotiation are `tensor_host::{Fourcc,Modifier}`
     (or thin newtypes) outside the Smithay adapter.
   - Renderer import/export uses Tensor-owned `Dmabuf` / `ExportedDmabuf`
     descriptions; Smithay conversion occurs only at protocol and tty edges.

3. **Reactor**
   - Semantic loop is only `tensor-event` drain order.
   - After **Compio completions** (io_uring driver), inject values into
     `tensor-event` (calloop readiness only during migration); policy does not
     depend on readiness callback order.
   - Spawn / log / IPC workers already use Compio bridges.

4. **Present path**
   - Compositor thread: Vulkan record → export dma-buf + SYNC_FD →
     `PresentIntent` → adapter atomic commit.
   - The tty adapter alone owns Tensor's atomic KMS surface, DRM framebuffers,
     and GBM imports.
   - VBlank → `Event::Output(VBlank)` → redraw latch (already started).

5. **Protocol surface**
   - Tensor-local Dispatch2 extensions (gamma, virtual-pointer, workspace,
     output-management, capture) remain value-only at ECS/render edges.
   - Core shell either stays on a thin wayland-server stack **or** moves to a
     Tensor protocol crate. Tensor `WindowSpace` now owns protocol-window
     mapping, stacking, hit testing, output overlap, and enter/leave; ECS +
     scene own policy and rendering order.

6. **Input / session**
   - libinput / libseat behind `tensor-input` / session adapter emitting
     `tensor_host` samples and `tensor_event::InputEvent`.
   - Seat protocol state is protocol-layer only.

7. **Verification**
   - `cargo test --all-targets` with Smithay feature **off** still builds host
     crates and policy tests.
   - Full session feature may keep Smithay until the last adapter lands.
   - Line limits, fmt, and no new Smithay imports outside `src/protocol/` +
     `src/backend/` adapter modules.

## Migration stages

| Stage | Status | Work |
|-------|--------|------|
| 0 | Done | `tensor-event`, `tensor-runtime` |
| 1 | Done | Event idle turn: inject → drain → redraw latch |
| 2 | **Done** | `tensor-host` / `tensor-drm` / `tensor-present` scaffolded + tests |
| 3 | **Done** | `backend/output` policy uses host types; Smithay maps only in `host_map` / protocol advertise |
| 4 | **Done** | Submit path: policy readiness + `PresentIntent` push/pop before KMS; format negotiation and renderer dma-buf descriptions are Smithay-free. `smithay-drm-extras`, Smithay's `backend_gbm`, and Smithay's `backend_drm` feature are removed. Tensor imports renderer-owned dma-bufs directly through GBM, creates explicit-modifier DRM framebuffers, strictly parses primary-plane `IN_FORMATS`, and owns atomic TEST_ONLY/modeset/page-flip/clear/resume requests. Steady-state present submits only `FB_ID + IN_FENCE_FD` from fixed stack arrays; it does not allocate, clone a request, stage geometry, or lock. Tensor owns the shared DRM fd, master lifecycle, startup property snapshot, tty restoration, and bidirectional syncobj/SYNC_FD protocol timeline operations. DRM nodes, connector modes/subpixels, event decode, scanning, and gamma use drm-rs directly. Smithay no longer compiles any DRM backend code |
| 5 | **In progress** | `run_turn` + `EventfdWake` + `CompletionDriver::IoUring`; the compositor thread now runs a Compio completion loop directly, with no shared relay thread/channel. IPC, signalfd, GPU sync-file, security-context, Wayland listener/display, XWayland displayfd startup, udev hotplug, libinput, Tensor-owned libseat session waits, commit-timing timerfd deadlines, and per-device DRM page-flip waits are Compio-completed operations. The timerfd has one submitted `IORING_OP_POLL_ADD`; its CQE is consumed before the earliest absolute monotonic deadline is rearmed, with no timer polling or readiness registry. DRM waits stay on the compositor thread, use one submitted op per device, decode one fixed stack batch after the CQE, and rearm explicitly. Libseat, udev, and libinput cursors apply one event at a time without per-completion staging vectors. Session-resume repaint is a completion-turn tail after DRM CQEs; tty no longer owns a calloop handle. The dma-buf client smoke also submits its Wayland socket operations directly through Compio, so `calloop-wayland-source` is gone. The aggregate remains only for Smithay's internal XWM event channel and focus-release ping |
| 6 | **In progress** | Libinput CQEs normalize keyboard, relative/absolute pointer, button, axis, activity, device ID, and capabilities directly into compact `tensor-input` values. The hot path no longer carries Smithay backend events or allocates device-name keys. Virtual-pointer requests enter the same Tensor value path without per-event Wayland resource clones; its allocation-free axis accumulator preserves source-before-value ordering, v120, and explicit stops. Smithay's `backend_libinput` and `backend_session` features and session wrapper are removed; raw tablet tools terminate at the protocol adapter, whose allocating device descriptor is cached once per hotplug rather than rebuilt per motion |
| 7 | **In progress** | See the Stage 7 protocol ledger below |
| 8 | Exit | Optional `tensor-smithay` removed; dependency deleted |

### Stage 7 protocol ledger

`tensor-protocol` owns IDs, scene values, attachment lifetime, and tier policy. All wire types now
come directly from `wayland-server`, `wayland-protocols`, and `wayland-protocols-wlr`; Smithay
reexports are gone. Tensor `WindowSpace` and compositor-thread `ProtocolWindow` have replaced
Smithay's desktop space/window types without per-refresh snapshots, query vectors, or shared
locking. Tensor also owns popup topology, descendant cleanup, dynamic root-relative placement,
explicit-grab policy, per-output layer maps/surfaces, allocation-free surface traversal, frame
callback dispatch, presentation capture, and X11 hit testing. Tensor now owns `wl_output` v4,
xdg-output v3, output identity/lifetime, enter/leave, `wp_presentation`, `wp_fractional_scale_v1`,
`wp_content_type_v1`, `wp_alpha_modifier_v1`, `wp_fifo_v1`, `wp_commit_timing_v1`, and
`ext-background-effect-v1` wire state. Fractional scale publishes Tensor's exact 120-based units
without a float round trip. Content type, exact 32-bit alpha, and background area share one O(1)
double-buffered metadata table and one post-commit hook installed only on opted-in surfaces, so
ordinary commits gain no work. FIFO preserves synchronized-subsurface and queued-transaction
ordering through the transitional core compositor cache, but capture scans only Tensor's
active-barrier set; atomic KMS acceptance releases submitted barriers, while failed or off-screen
submissions fail forward at the idle boundary. Commit timing keeps full unsigned wire seconds,
rejects invalid nanoseconds, and advertises only when its monotonic timerfd exists. Alpha remains an
integer through `SurfaceContent` and the draw plan, converts only for the final Vulkan push
constant, and fully transparent surfaces allocate neither descriptors nor draws. Background-effect
reduces the transient region to the area predicate consumed by current scene policy instead of
retaining another region-vector copy. Mode, location, scale, transform, and physical state are
coherent lock-free snapshots; image capture and present read them without locking. Wayland output
resource lists publish an RCU snapshot only on bind/destroy, so presentation completion neither
locks nor copies the list. Tensor owns the live renderer-facing surface buffer, size, viewport,
transform, scale, damage revision, release, and synchronized-subsurface application state. Scene
extraction consumes those values in the existing single surface-tree traversal, without a damage
staging allocation or buffer-content copy. Tensor also owns the linux-dmabuf v6 global, immutable
sealed feedback table, params validation, `wl_buffer` userdata, renderer-registration lifetime,
inline single-pixel buffer userdata, stable viewporter wire, core `wl_shm`/pool/buffer wire, staging
`ext-foreign-toplevel-list`, and the complete staging image-capture-source/image-copy-capture wire.
SHM pool validation, grow-only remapping, truncated-file SIGBUS recovery, and exact-subrange pixel
access are Tensor-owned; size lookup borrows inline userdata without mapping or locking, while
surface and capture pixel consumers borrow the mmap directly without staging copies. Capture
sessions carry their source directly instead of scanning session tables, constraints use fixed
arrays, and frame fills write into the client mapping on the bounded idle path. Foreign-toplevel
capture sources retain a weak Tensor handle and resolve directly to the stable surface key; closing
a toplevel invalidates that identity with no title lookup or session scan. Metadata resource locks
and string allocation remain confined to map/title/app-id protocol events, outside input, vblank,
and present paths. Viewport source crop remains exact 24.8 fixed-point state through the registry,
then composes with buffer scale and transform once at commit; Vulkan recording reads the six
precomputed UV values without buffer copies, allocation, traversal, or division. Viewport commit
validation takes the existing surface traversal and performs no allocation on valid input; only
protocol-error formatting allocates. Client dma-buf import retains one plane vector and uniquely
owned FDs from wire decode through Vulkan import; there is no Smithay conversion or second plane
collection, while single-pixel lookup is a zero-copy userdata borrow. Tensor now owns
`xdg_system_bell_v1`, `wp_pointer_warp_v1`, `xdg_toplevel_tag_v1`, and `xdg_toplevel_icon_v1`
directly, with no Smithay handler or parallel compatibility path. Pointer warps validate the enter
serial, focused surface, finite in-bounds coordinates, and client scale exactly once. Toplevel icons
freeze once into shared `Arc` snapshots; pixel buffers remain zero-copy mmap leases after the icon
and `wl_buffer` resources are destroyed, while destroying a buffer before its live icon raises
`no_buffer`. A buffer-to-icon reverse index makes unrelated SHM buffer destruction one hash lookup
instead of an icon scan. Icon commit hooks are installed only on surfaces that opt in, so ordinary
commit, input, vblank, and present paths gain no work. Tensor also owns `zxdg_decoration_v1`,
`zwp_idle_inhibit_v1`, and `zwp_keyboard_shortcuts_inhibit_v1` directly. Idle inhibition preserves
multiple live inhibitors per surface without scanning them on input activity. Shortcut inhibitors
are keyed by stable surface identity, reject duplicates, and add one allocation-free hash lookup to
the key path only while resolving the focused surface; VT recovery remains compositor-owned. Only
the core compositor cached-state boundary remains Smithay-backed for surface commits. Tensor-owned
gamma, virtual-pointer, workspace,
output-management, and security-context protocols use a local zero-cost `wayland-server` dispatch
delegate and no longer import Smithay. Gamma-control lifetime is keyed by stable `ConnectorId`
rather than Smithay `Output`, and ramp ingestion uses one final allocation without a full-size
staging copy. The Smithay `desktop` feature is removed; Dispatch1 shell state remains in the
protocol adapter

## Performance rules (unchanged)

- Fixed-capacity rings; no alloc on input/vblank/present queue push.
- Coalesce pointer motion and per-output vblank.
- Present and Vulkan record stay on the compositor thread.
- Topology-rate work (output-management advertise, connector rescan) never on
  page-flip.

## How to review exit PRs

1. Does this PR **reduce** `smithay::` imports outside adapter modules?
2. Are new types in `tensor-host` / `tensor-drm` / `tensor-present` pure values?
3. Are hot paths still O(1) / budgeted idle work?
4. Is there a test that runs **without** opening DRM or a Wayland display?

If the answer to (1) or (2) is no, it is not exit work.
