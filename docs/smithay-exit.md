# Smithay exit plan

Tensor currently uses Smithay as a **mature adapter** for Wayland protocol and
desktop objects, DRM/GBM, XWayland, and a residual calloop aggregate. The
product goal is to delete that dependency once Tensor-owned crates cover the
same **semantic** surface with equal or better performance.

This document is the exit contract. Work that does not shrink Smithay surface
area or move policy behind these crates is not exit work.

## Non-goals

- Rewrite every protocol handler in one PR.
- Run Vulkan or KMS on Compio worker threads.
- Keep dual implementations forever (adapter → delete, not dual forever).

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
   - Adapter is the only place that touches `DrmSurface` / GBM.
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
| 4 | **Done (partial)** | Submit path: policy readiness + `PresentIntent` push/pop before KMS; format negotiation and renderer dma-buf descriptions are Smithay-free. `smithay-drm-extras` is removed: the tty adapter now refreshes its connector/CRTC tables in place without constructing unused scan-event vectors or cloning connector snapshots |
| 5 | **In progress** | `run_turn` + `EventfdWake` + `CompletionDriver::IoUring`; the compositor thread now runs a Compio completion loop directly, with no shared relay thread/channel. IPC, signalfd, GPU sync-file, security-context, Wayland listener/display, XWayland displayfd startup, udev hotplug, libinput, Tensor-owned libseat session waits, and per-device DRM page-flip waits are Compio-completed operations. DRM waits stay on the compositor thread, use one submitted op per device, decode one fixed stack batch after the CQE, and rearm explicitly. Libseat, udev, and libinput cursors apply one event at a time without per-completion staging vectors. Session-resume repaint is a completion-turn tail after DRM CQEs; tty no longer owns a calloop handle. The dma-buf client smoke also submits its Wayland socket operations directly through Compio, so `calloop-wayland-source` is gone. The aggregate remains only for Smithay's internal XWM event channel and focus-release ping |
| 6 | **In progress** | Libinput CQEs normalize keyboard, relative/absolute pointer, button, axis, activity, device ID, and capabilities directly into compact `tensor-input` values. The hot path no longer carries Smithay backend events or allocates device-name keys. Smithay's `backend_libinput` and `backend_session` features and session wrapper are removed; raw tablet tools terminate at the protocol adapter, whose allocating device descriptor is cached once per hotplug rather than rebuilt per motion |
| 7 | **In progress** | `tensor-protocol` owns IDs, scene values, attachment lifetime, and tier policy. All wire types now come directly from `wayland-server`, `wayland-protocols`, and `wayland-protocols-wlr`; Smithay reexports are gone. Tensor `WindowSpace` and compositor-thread `ProtocolWindow` have replaced Smithay's desktop space/window types without per-refresh snapshots, query vectors, or shared locking. Tensor also owns popup topology, descendant cleanup, dynamic root-relative placement, and explicit-grab policy. Topology changes rebuild compact parent/order indices; frame traversal is borrowed and allocation-free. Smithay desktop use is now limited to layer map/surface handling, generic surface/presentation helpers, and the X11 XDND hit-test adapter; Dispatch1 shell state also remains |
| 8 | Exit | Optional `tensor-smithay` removed; dependency deleted |

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
