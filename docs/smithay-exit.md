# Smithay exit plan

Tensor currently uses Smithay as a **mature adapter** for Wayland protocol
objects, libinput, libseat, DRM/GBM, and calloop. The product goal is to delete
that dependency once Tensor-owned crates cover the same **semantic** surface
with equal or better performance.

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
     Tensor protocol crate; Smithay `desktop::Space` is replaced by ECS + scene.

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
| 4 | **Done (partial)** | Submit path: policy readiness + `PresentIntent` push/pop before KMS; format negotiation and renderer dma-buf descriptions are Smithay-free |
| 5 | **In progress** | `run_turn` + `EventfdWake` + submitted `EventfdCompletion` read + Compio-completed IPC, signalfd, GPU sync-file, and security-context accept/close operations + `CompletionDriver::IoUring`; calloop readiness remains only for transitional Smithay/Wayland/backend sources — replace it with completed ops, not epoll-as-goal |
| 6 | **In progress** | `tensor-input` samples (motion/button/axis/key) + caps; libinput adapter-only |
| 7 | **In progress** | `tensor-protocol` owns IDs, scene values, attachment lifetime, and tier policy; wire shell still needs removal of Smithay `desktop` / Dispatch1 |
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
