# Cursor architecture and work plan

This document is the authoritative status and follow-up list for pointer-adjacent rendering. It
covers the core pointer, tablet tools, named cursor shapes, and drag icons. Protocol support and
renderer/KMS optimization use one Tensor-owned cursor model; a hardware cursor plane must not grow
a second protocol implementation or a compatibility adapter.

## Completed baseline

The software-composited path is the correctness baseline and is complete for the current native
renderer:

- Core `wl_pointer.set_cursor`, `wp_cursor_shape_manager_v1` version 2, and tablet-v2 tool cursors
  are dispatched directly through `wayland-server`.
- Core-pointer and cursor-shape devices validate the latest enter or proximity serial against the
  concrete input device rather than shared seat-wide authority.
- Client cursor surfaces preserve committed hotspots, `wl_surface.offset`, viewport state, buffer
  transform, integer scale, and fractional preferred scale. A surface without committed image
  content is invisible.
- Named XCursor images are selected at each output's physical scale. Every animation frame for the
  chosen nominal size is uploaded during cold loading, and a one-shot timerfd advances active
  sequences through a submitted Compio/io_uring completion.
- Pointer and tablet motion, image replacement, animation, surface commit/destruction, and device
  removal damage the complete old/new cursor extents. A cursor straddling two outputs damages both.
- Cursor surfaces track output-instance membership and emit exact `wl_surface.enter` and
  `wl_surface.leave` events. Retired surfaces leave their outputs before redraw.
- Core data-device drag icons use the same cached image path, committed offset, output membership,
  frame callback, and presentation rules. They are drawn below the pointer cursor.
- The renderer boundary carries at most one pointer, 64 tablet tools, and one drag icon in fixed
  66-slot storage. Overlay and descriptor preparation performs no per-frame cursor allocation.
- Named and client cursors use stable `SurfaceBufferId` values and the shared descriptor heap.
  Pixel data is not recopied during ordinary motion or animation.

The corresponding implementation is concentrated in `src/protocol/cursor.rs`,
`src/protocol/globals/cursor_shape.rs`, `src/protocol/globals/seat`,
`src/protocol/globals/tablet`, `src/protocol/state/output/cursor.rs`, and
`src/render/frame/plan.rs`.

## Non-negotiable contracts

- Wayland resources and cursor roles remain on the compositor thread. Compio workers receive only
  value messages and never own Wayland or DRM/KMS objects.
- Animation and fence waits are submit-to-complete operations. Do not introduce a readiness loop,
  Compio's `polling` backend, or a periodic cursor wakeup.
- There is one cursor state, image, geometry, damage, and presentation model. Hardware-plane
  assignment is a late native presentation decision, not a parallel cursor backend.
- The normal Vulkan composition pass remains capable of drawing every overlay. An overlay not
  assigned to a KMS plane follows that same path; this is not a legacy compatibility
  implementation.
- Cursor motion must not allocate, read pixels back to the CPU, rebuild protocol surface lists, or
  repaint unrelated outputs.
- Vulkanalia owns rendering and synchronization. Tensor owns framebuffer import and atomic KMS
  state. Binary `SYNC_FD` is the only synchronization boundary between them.
- Smithay types, adapters, features, and fallback implementations remain prohibited.

## Next milestone: atomic hardware cursor planes

Hardware cursor support is the next cursor milestone. It is complete only when all of the following
gates are implemented; merely discovering a plane or setting `FB_ID` is not completion.

The capability-discovery foundation is now implemented on the output cold path. It queries cursor
dimensions from the already Vulkan-selected DRM device, orders compatible cursor planes by object
ID, and retains at most eight candidates with at most 128 sorted explicit format/modifier pairs
per plane. Candidates lacking `IN_FORMATS`, any required atomic geometry/object property, or
`IN_FENCE_FD` are diagnosed and excluded; an unspecified modifier is never promoted to an explicit
pair. The first candidate with a strict Vulkan intersection is selected deterministically: only
alpha-bearing formats with an exact explicit modifier and a renderable, exportable Vulkan
capability are eligible, tiled modifiers precede linear ones, and the selected Vulkan plane count
is preserved for dma-buf allocation. Output registration now allocates three fixed device-local
cursor image slots with that exact modifier and plane count. Their dma-bufs are retained by the
renderer and imported into Tensor-owned DRM framebuffers without stripping the alpha fourcc;
planes already claimed by another output are skipped deterministically. The slots are not yet
rendered or attached to atomic cursor-plane commits. The selected plane is nevertheless owned by
the output's atomic modeset lifecycle: test-only modesets and session-resume rebuilds explicitly
disable it, teardown clears its `CRTC_ID` and `FB_ID` in the same request as the primary plane, and
every steady-state primary page flip carries the same explicit disabled cursor state. The
steady-state request is a typed fixed-capacity stack value and performs no frame-time allocation.
The slots still need GPU rendering, exported binary fences, active geometry attachment, and joint
retirement before the plane can be enabled. The milestone remains open and software composition
remains the correctness path.

### 1. Capability and identity discovery

- Enumerate DRM planes and select only `DRM_PLANE_TYPE_CURSOR` planes compatible with a planned
  CRTC through `possible_crtcs`.
- Parse each plane's `IN_FORMATS` blob and retain explicit fourcc/modifier pairs. Do not assume
  linear ARGB8888.
- Query cursor width and height limits and the required atomic properties: `FB_ID`, `CRTC_ID`,
  CRTC position/size, 16.16 source position/size, and `IN_FENCE_FD`. A plane without the required
  explicit-fence property is ineligible.
- Keep plane capability in the Vulkan-selected DRM primary/render identity. Cursor discovery must
  never independently select another GPU.
- Model plane availability and assignment as fixed-capacity per-output state with deterministic
  ordering.

### 2. Vulkan/KMS image contract

- Allocate a small explicit-modifier dma-buf image compatible with both Vulkan rendering and the
  selected cursor plane, then import the same planes into a Tensor-owned DRM framebuffer.
- Render named, SHM, or dma-buf client cursor content into that image on the GPU. Never introduce a
  GPU-to-CPU readback or a motion-time pixel copy.
- Export a binary semaphore as `SYNC_FD` after rendering and pass it to the cursor plane's atomic
  `IN_FENCE_FD`. Internal timeline semaphores do not cross this boundary.
- Retain every image, framebuffer, descriptor range, and sync object until both Vulkan completion
  and the replacing KMS page flip make retirement safe.
- Reuse bounded per-output cursor image slots. A busy slot delays or coalesces the next cursor
  presentation; it must not allocate an unbounded replacement.

### 3. Atomic presentation policy

- Submit primary and cursor plane changes in the same Tensor-owned atomic commit. Page-flip and KMS
  completion remain on the compositor thread.
- Express hotspots, output-local fractional scaling, clipping, and negative CRTC positions through
  one tested logical-to-physical conversion. A cursor crossing outputs may use each output's own
  plane and cropped source rectangle.
- Keep drag icons in the primary Vulkan composition while assigning the pointer cursor above them.
  This preserves the required drag-icon-under-pointer order.
- When several tablet tools are visible, assign planes deterministically and compose unassigned
  overlays in the same Vulkan pass. Plane count must never alter protocol state or cursor identity.
- Plane assignment changes must damage the previous Vulkan-composited extent and the new extent
  exactly once, preventing both stale pixels and redundant full-output redraws.
- Session pause, modeset, hot-unplug, GPU loss, and failed atomic commits must quarantine in-flight
  cursor slots using the same fail-closed lifetime rules as primary scanout.

### 4. Protocol and presentation closure

- Add a real tablet-v2 `set_cursor` wire test covering role exclusivity, valid and stale proximity
  serials, hotspot updates, detach, destruction, and output membership.
- Verify frame callbacks and presentation feedback are released only for cursor surfaces included
  in an accepted atomic submission, whether drawn into the primary image or assigned to a plane.
- Verify pointer, tablet, and drag-icon surface destruction removes all output memberships without
  scanning or redrawing unrelated heads.
- Document the precise behavior for current tablet cursor surfaces separately from cursor-shape
  serial validation; do not infer core-pointer wording where tablet-v2 differs.

## Performance acceptance gates

The hardware milestone is rejected unless evidence demonstrates all of these properties:

- Pointer motion performs zero heap allocations and zero pixel copies.
- A stable cursor image performs no descriptor/image upload and no dma-buf or framebuffer creation.
- Animation changes only the stable image/slot identity and redraws only outputs touched by the
  complete old/new frame extents.
- No cursor operation causes an unrelated CRTC atomic commit unless the debug full-redraw policy is
  explicitly enabled.
- Cross-output motion, fractional scaling, and a busy GPU remain bounded by fixed cursor slots and
  coalesced redraw state.
- There is no CPU readback, readiness polling, implicit modifier selection, or unbounded retirement
  queue.

## Verification plan

Every cursor change runs the repository-wide gates in `AGENTS.md`. Focused evidence must also cover:

- Pure geometry tests for hotspot, scale, crop, negative position, and cross-output extents.
- Plane-property parser and deterministic assignment tests without DRM hardware.
- Renderer tests proving descriptor/image reuse and timeline-safe slot retirement.
- Real Wayland wire tests for core pointer, cursor-shape v1/v2, tablet-v2, and drag icons.
- An optional TTY smoke test that records selected plane, format/modifier, slot reuse, exported
  fence, atomic commit, page flip, and retirement without relying on log polling for correctness.

The milestone may be marked complete only after this document moves every item above into the
completed baseline and the full verification suite passes.
