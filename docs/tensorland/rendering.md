# Rendering Contract

Tensorland has one native renderer: the shared `vulkan-renderer`, backed by Vulkanalia, with
`VK_EXT_descriptor_heap`. Descriptor sets, descriptor buffers, GLES, and software composition are
not compatibility backends.

`vulkan-renderer` owns the loader, instance, selected adapter, logical device, graphics queue,
command pool, timeline semaphore, native dma-buf images, device-local descriptor heaps with their
retained staging buffers, shader modules, retained graphics pipelines, and binary sync-file
semaphores. It also owns Tensorland's typed command encoder, dynamic rendering, color upload, and
semantic synchronization primitives. Tensorland owns compositor/KMS policy, value-only scene
extraction, output-slot policy, and draw ordering; it has no borrowed native-device command path,
raw resource lifecycle, or parallel command-buffer pool.

## Native Device Gate

An eligible physical device must provide all of the following before ranking:

- Vulkan 1.4 and a graphics queue.
- `VK_EXT_descriptor_heap`, including its feature bit.
- Dynamic rendering, enabled from the Vulkan 1.3 feature chain for Tensorland's
  render-pass-free client-image pipeline.
- Vulkan 1.4's `maintenance5` feature, which is a required descriptor-heap dependency and is
  enabled in the device feature chain. When its promoted `VK_KHR_maintenance5` name is also
  advertised, Tensorland enables that name too, matching the descriptor-heap extension dependency.
- Buffer device address support (the heap is bound as a device-address range; no descriptor-buffer
  or descriptor-set fallback is permitted).
- A usable resource heap: non-zero heap alignment, maximum size beyond the implementation's
  reserved range, and non-zero image descriptor size/alignment.
- A sampler heap range large enough for `minSamplerHeapReservedRangeWithEmbedded`, plus one
  explicit linear-clamp sampler descriptor and 128 bytes of push data. Identity draws retain the
  original 64-byte geometry record; only managed-color draws append the fixed 64-byte color lane.
- `VK_EXT_physical_device_drm` with a complete primary/render node pair.
- `VK_KHR_external_memory_fd` and `VK_EXT_external_memory_dma_buf`.
- `VK_EXT_image_drm_format_modifier`.
- `VK_EXT_queue_family_foreign` for ownership transfers to and from non-Vulkan consumers.
- `VK_KHR_external_semaphore_fd`.
- Importable and exportable binary `SYNC_FD` semaphores, verified through
  `vkGetPhysicalDeviceExternalSemaphoreProperties`.

Extension availability alone does not prove that a usable image exists. Output initialization must
also intersect Vulkan's per-format external-image modifier properties with Tensorland's DRM plane and
GBM scanout capabilities. Readiness requires at least one renderable, exportable, explicit-modifier
format for every active output path.

## Native Format Gate

`render::format` is the value-only capability boundary between the shared renderer and the tty
adapter. Shared Vulkan probing enumerates `VkDrmFormatModifierPropertiesList2EXT`, rejects
modifiers without color-target support, and calls `vkGetPhysicalDeviceImageFormatProperties2` with the real sampled,
color-attachment, and transfer usage. `VkExternalImageFormatProperties` records dma-buf import and
export separately: client imports and compositor-owned output exports are different capability
roles and are never inferred from each other.

For each connector with a mode and mapped CRTC, the tty backend reads Tensorland's primary-plane
`FormatSet`, checks the matching GBM format/modifier plane topology and scanout/rendering usage, and
passes only value data into the deterministic intersection. Vendor modifiers precede linear;
implicit `DRM_FORMAT_MOD_INVALID` is rejected. XRGB8888 is the first format preference, followed by
the corresponding alpha, channel-order, and 10-bit candidates. An empty initial intersection is a
startup error, so systemd readiness and session autostart cannot run with an unusable output path.

The preferred intersection result is part of `OutputDescriptor`, so mode, CRTC, fourcc, modifier,
and plane-count changes share one topology generation and one hotplug diff. The protocol boundary
turns that descriptor into a `NativeOutputTarget` containing stable output identity, pixel extent,
fourcc, explicit modifier, and plane count. The renderer checks the target against the selected
Vulkan device again before registering it. A target change clears scene-damage history but retains
in-flight timeline ownership until the old submission retires.

Registration now allocates three native output slots per target. Each slot is a Vulkan 2D image
created with `DRM_FORMAT_MODIFIER_EXT`, dedicated exportable dma-buf memory, and an image view. The
created modifier is queried back and must equal the negotiated modifier; every reported memory
plane is exported with its offset and row pitch as Tensorland's value-only `ExportedDmabuf`.
The tty boundary imports that description directly through GBM and validates dimensions, fourcc,
modifier, and plane count before retaining the GBM objects. Vulkan image resources replaced by a
mode or format change remain in a retired queue until their last renderer timeline value completes.

GBM remains owned by the tty adapter and does not become a renderer. Its check validates the
allocation and KMS-facing boundary; `vulkan-renderer` remains the only component that creates and
renders native output images.

## Buffer Ownership

`vulkan-renderer` owns Vulkan render images and their memory. A native render target is allocated
with an explicit DRM modifier and exportable dma-buf memory. The renderer returns Tensorland-owned
`ExportedDmabuf` plane descriptions directly to the tty adapter. The tty adapter owns GBM import,
DRM framebuffer lifetime,
primary-plane selection, atomic commits, and page flips. Vulkan handles and DRM/KMS handles never
cross into ECS or IPC.

Before an image leaves Vulkan for KMS or another API, Tensorland releases it to
`VK_QUEUE_FAMILY_FOREIGN_EXT`; imported images are acquired back from the same external owner.
This is mandatory for multi-plane and driver-compressed modifiers and is never replaced with a
queue-idle compatibility path.

Imported client dma-bufs and compositor-owned output images use separate lifetime caches. The
protocol owner builds Tensorland's generic `Dmabuf` description directly before calling the renderer.
A client cache entry is keyed by the compositor-assigned stable buffer
identity and retains the validated format, modifier, dimensions, plane offsets, and strides from
that value description; an fd number is never an identity. Frame-local sampled descriptors use
the stricter `(buffer identity, view encoding)` key, so one buffer may safely appear through both
an encoded/UNORM view and a hardware-sRGB-decoding view. Compatible views are created once on the
import cold path through the shared renderer's mutable-format image contract. Buffer reuse waits
for the renderer timeline and Wayland release path before the Vulkan image is destroyed.

SHM client snapshots use the same shared allocator rather than a Tensorland-local Vulkan image,
memory, view, staging-buffer, or mapping lifecycle. Their optimal sampled image and persistently
mapped upload buffer are retained through the submission timeline; the shared mapped-write API
flushes non-coherent ranges. Tensorland uses small 2 MiB retained pools for this UI-specific cache and
a 2 MiB exact-allocation cutoff, so idle client-surface churn cannot reserve the scene renderer's
large default blocks or retain an oversized empty pool. Empty pools are trimmed after timeline
retirement.

## Client linux-dmabuf

The `zwp_linux_dmabuf_v1` global is created only after the selected Vulkan device provides a
non-empty client-import format list. Feedback is built from that device's explicit modifiers and
render-node identity; it is not copied from a KMS-only list. The initial import contract is
deliberately narrow and honest: explicit-modifier, single-plane RGB buffers whose fd memory type
is accepted by the selected Vulkan device.

For each `params` request Tensorland validates the protocol shape, then `vulkan-renderer` creates an
`ImportedDmaBufImage` with the explicit modifier, intersects image and dma-buf fd memory-type
masks, binds imported memory, and creates a view. Only a completed image/view import calls
`ImportNotifier::successful`; malformed
planes, implicit modifiers, unsupported formats, and Vulkan failures call `failed` instead. The
image cache is keyed by `SurfaceBufferId` and retires resources after the renderer timeline, so a
duplicated fd or recycled Wayland object ID cannot alias a live scene image accidentally.

Client images are now accepted by the protocol boundary and have a first real sampling path. A
client image is acquired from the foreign queue family, selected through a descriptor-heap push
index, sampled with a shared linear-clamp sampler from the sampler heap, and composited by a
dynamic-rendering pipeline with
premultiplied-alpha blending. The first acquire uses `UNDEFINED + FOREIGN` to preserve the
producer's explicit-modifier contents; only a successful queue submission advances the cache to the
subsequent `GENERAL + FOREIGN` path. Resource and sampler descriptors occupy separate shared,
device-local heap buffers, each with its own aligned implementation-reserved suffix; the sampler
heap retains one linear-clamp descriptor. The path is intentionally limited to one-plane RGB today.
Explicit producer/consumer
synchronization for these buffers is provided through `wp_linux_drm_syncobj_v1`; implicit dma-buf
reservation-fence interop remains a separate gate.

The protocol-to-scene handoff has an explicit value-only boundary. A
compositor-assigned `SurfaceBufferId` is registered after a successful
linux-dmabuf import. Tensorland's toplevel, synchronized/asynchronous subsurface,
and popup trees are traversed in draw order; content revision, scale, transform,
surface-local destination geometry, and `View`/`Popup` clip policy are copied
into ECS as `ViewContent`. No `WlSurface`, popup handle, or renderer state enters
ECS. Synchronized child commits and their explicit sync points remain pending
until the non-synchronized ancestor applies the complete transaction.

Rootless XWayland override-redirect surfaces follow the same value-only handoff
only after they resolve through a managed `WM_TRANSIENT_FOR` ancestor. Their
X11 coordinates produce a logical offset from that root; they do not create an
independent X11 layout coordinate system or ECS view.

The scene builds a per-frame draw plan that deduplicates image descriptor slots
while preserving the flattened surface order. View content is intersected with
the layout-visible tile and output; popup content is intersected only with the
output. Scene visual bounds include popup content so popup motion or destruction
damages both the old and new regions outside the tile. Destroyed buffers remain
renderer-live while any surface still refers to them, and imported images are
marked with the submission timeline used by that plan.

Fractional output scale has one coordinate conversion boundary. `OutputScale` stores the exact
`N/120` protocol value and the scene remains in logical coordinates. The native output target keeps
the DRM mode's physical dimensions. Draw destinations transform both logical edges with identical
nearest-pixel rounding so adjacent surfaces retain a common edge; clip and damage rectangles use
floor/ceil coverage and are intersected with the physical target. Corner radii are converted to
physical pixels in Vulkan push data. A 1920×1080 output at scale 1.25 therefore lays out as
1536×864, still renders and scans out 1920×1080, and advertises preferred scale `150`.

This also defines the scaling contract for rootless XWayland windows: they are surface content and
use the same scene conversion. Legacy clients that only observe `wl_output.scale` receive Tensorland's
rounded-up integer value, so a 1.25 output lets XWayland render a 2x client buffer before the
compositor downsamples it to the fractional physical destination. The embedded surface sampler is
linear for this final conversion; Tensorland must not introduce a default nearest-neighbor XWayland
branch. Tensorland does not add an X11-session renderer, a second X11 coordinate space, or an
X11-specific damage path.

This follows the quality-producing invariant observed in Niri's xwayland-satellite integration:
X11 windows become ordinary Wayland surfaces and inherit the normal fractional-scale pipeline.
Hyprland's separate XWayland monitor positions, `force_zero_scaling` coordinate conversions, and
default XWayland nearest-neighbor switch are useful reference failure modes, not Tensorland APIs to
copy. A future per-window pixel-art filter may be explicit policy, but X11 provenance alone must
never reduce sampling quality.

## Frame Boundary Status

`render/frame.rs` is the renderer-to-scene boundary. It retains the latest `SceneSnapshot` per
output plus the scene last written to each native image slot, computes both damage domains, assigns
one of three native output slots, and holds a clone of the resource heap's shared allocator. Thus
Tensorland's frame policy allocates and retires exactly the
same direct-heap ranges that `vulkan-renderer::DescriptorHeap` validates and encodes; it does not
maintain a parallel offset allocator. `vulkan-renderer` creates the device-local resource and
sampler heaps with retained persistently mapped staging buffers. `render/vulkan/frame.rs` retains
image-resolution and batch scratch storage, then encodes the native output and deduplicated client
images in one descriptor write/flush, copies just the allocated resource range, binds both heaps,
and submits through the shared bounded command-buffer recycler plus the shared timeline. The sampler
descriptor is copied only on its first submitted frame. The sampled-image identity pipeline pushes
a 64-byte draw record whose first and fourth words are absolute resource- and sampler-heap element
indices. Non-identity per-surface color plans select a separately retained pipeline and append a
64-byte transfer/matrix/tone-map lane; ordinary SDR draws execute the original shader without a
managed-color branch. Slang emits
`SPV_EXT_descriptor_heap` direct accesses, so pipeline creation needs no descriptor binding mapping.
A lost device stops future frame scheduling instead of recycling GPU-visible ranges.

### Single-pass and multi-pass policy

Tensorland does not expose a global pass-count switch. Frame extraction produces a value-only
`FramePassPlan` from scene dependencies:

- Client surfaces, alpha, rounded corners, analytic rounded Gaussian shadows, focus rings, and
  software cursors stay in the direct single output pass. Shadows and rings are descriptor-free
  draws; they do not allocate offscreen images. This is the default because it minimizes attachment
  traffic, synchronization, and device-memory bandwidth.
- A backdrop effect that reads already-composited pixels selects a region-local multi-pass path.
  The direct pass must end before that region is sampled; filtering uses retained intermediates,
  then output composition resumes. Capture taps and future color post-processing use the same
  explicit dependency rule rather than forcing every frame through an offscreen image.

The shared renderer also exposes Vulkan 1.4 dynamic-rendering local read through typed attachment
location/input-index mappings. Tensorland may select it for same-pixel input-attachment dependencies;
it is not a blur shortcut because convolution requires sampler access to neighboring pixels. Such
neighborhood reads therefore continue to lower to bounded region-local intermediates.

Capture readback is split at the shared-renderer boundary. TTY output capture and toplevels wholly
contained by one output copy only their requested physical region into a four-entry bounded
retained device-local target pool before foreign/KMS release. Toplevel global logical geometry is
converted once through the owning output's `OutputScale`; cross-output geometry fails explicitly
until multi-output assembly exists. A later same-queue timeline submission transitions that
retained image and uses the typed image-to-readback-buffer command; the compositor reads and
converts the mapped BGRA/RGBA/10-bit result only after timeline completion, then publishes the
protocol SHM frame. `PaintCursors` selects whether the tap is placed before or after software
cursor composition. For an excluding capture, frame planning injects only the captured portions of
old/current cursor footprints into native-slot `render_damage`, restoring the retained scene below
them before the tap. A second `Load` rendering scope opens only when current cursor draws actually
exist; ordinary frames and captures including cursors retain the original damage and scope count.
This keeps host reads and SHM writes out of page-flip submission, does not poll the GPU, and does
not expose Vulkanalia or protocol objects across the renderer boundary. Headless protocol tests
retain the bounded idle SHM path.

The direct path tracks two damage domains deliberately. Semantic `damage` compares against the
latest committed scene for protocol scheduling and diagnostics. `render_damage` compares against
the scene and overlays last written to the exact native output slot. The latter is the only valid
source for partial rendering with triple buffering: an unused slot receives a full attachment
clear, while a retained slot uses `Load`, clears only accumulated damaged rectangles, and clips
every draw to those rectangles. This avoids assuming that the newest scene is also the content of
the next KMS buffer.

Both domains retain at most 64 rectangles. Crossing that bound no longer
unconditionally damages the complete output: dense rectangles collapse to
their local extent only when that extent covers at most twice their summed
area, while sparse rectangles merge the pair with the least added area until
the fixed bound is restored. This keeps command/scissor work bounded without
turning scattered window updates into full-output fragment shading. The rule
is deterministic, allocation-bounded, and conservative: compaction may add
overdraw but never removes a damaged pixel.

This follows the useful parts of the local references without copying their backend ownership.
Niri retains offscreen textures and an independent damage tracker only for effects and animations
that need them; its normal output remains a direct composition. Nourish likewise keeps ordinary
output composition direct, while its named scene/lock/picker passes describe semantic ordering and
its overview blur is a separately cached multi-stage operation. Tensorland keeps Vulkanalia inside
`vulkan-renderer`; the compositor owns only pass selection, slot history, and scene ordering.

Backdrop multi-pass execution is connected as one explicit graphics submission. Tensorland ends the
output attachment scope immediately before an affected view, copies only its radius-expanded sample
region into lane 0, filters lane 0 → lane 1 horizontally and lane 1 → lane 0 vertically, composites
the affected subregion back into the output, then resumes scene drawing. Missing or reordered view
boundaries fail before target acquisition instead of silently moving the effect in scene order.

The value lowering now computes both the effect output region and its radius-expanded sampling
region in physical output pixels. All backdrop operations execute serially in scene order and share
one two-lane ping-pong requirement sized to the largest expanded local region; they do not reserve a
full-output image per effect. The frame allocator already reserves the corresponding two sampled
image descriptor slots only for multi-pass frames, while a direct frame retains its exact previous
descriptor count. Damage propagation uses the expanded logical filter footprint, so a background
change just outside the visible effect still redraws the pixels whose convolution depends on it.
After that dependency propagation crosses into physical output pixels, pass lowering drops every
backdrop whose visible region does not intersect the exact native-slot `render_damage`. This is safe
for cursor-only damage because cursors compose after the client scene: a cursor update outside an
effect leaves its retained output pixels untouched. If only one of several effects is affected,
only that scene-ordered pass contributes descriptors, workload counters, and intermediate extent.
For an affected effect, Tensorland retains every exact non-overlapping composite rectangle produced by
the committed protocol region and native-slot damage. It takes only their bounding rectangle,
inflates that by the physical filter radius, and performs one copy plus two filters over the
resulting sample region. Final composition draws the exact rectangle span, so protocol subtraction
holes are never filled back in. This keeps pass count bounded to the effect count while shrinking
work for small or fragmented damage. The complete radius remains present on every non-output edge,
so the nine-tap separable convolution never samples outside the copied dependency footprint.
Output-edge clipping matches the source image boundary, and every final rectangle remains
damage-scissored.

The shared allocation slice is now present. `RetainedColorTargetPool` owns allocator-backed image/
view pairs behind explicit target-count, retained-byte, and maximum-extent limits. It reuses a
matching entry only after its retirement timeline completes, evicts only retired LRU entries under
pressure, and fails before allocation when every slot is busy. Pre-submit acquisition produces an
explicit reservation so Tensorland can retire it only after successful queue submission or release it
immediately after abandoned recording. Tensorland lowers its maximum local extent and output format to
an exact two-element generic request batch with `COPY_DESTINATION | STORAGE` usage; no `ViewId` or
blur type crosses the shared boundary. The executor now owns an allocation-lazy pool bounded to six
targets (two lanes across three in-flight frames), 768 MiB retained storage, and an 8192×8192 local
extent. Direct frames neither acquire from it nor allocate intermediate images. Batch acquisition
rolls back the first lane if the second cannot be reserved, and retirement must advance beyond the
completed timeline observed at acquisition.

The retained separable filter pipeline is also materialized on the cold path. Its Slang shader uses
the required direct descriptor heap, performs a normalized nine-tap horizontal or vertical pass,
and varies radius by sample spacing while retaining a fixed command and texture-fetch count. The
pipeline disables blending because each pass overwrites its destination lane. Shader modules are
validated during renderer startup and every output format is prepared during output registration;
a multi-pass frame cannot trigger shader compilation or silently use an unprepared format.

Descriptor indices and scene-order ranges are validated before pool acquisition. A descriptor,
encoder, recording, or rejected-submit failure releases the complete lane batch immediately; an
accepted submission retires both lanes at the frame timeline before completion export. The shared
renderer exposes only the generic exported-color-image → retained-color-image region copy and typed
image states. `ViewId`, blur radius, and scene ranges remain Tensorland-owned. A direct frame takes the
existing `record_scene` branch, writes no lane descriptors, acquires no intermediate, and emits the
same single dynamic-rendering command stream as before.

Frame diagnostics expose fixed-width workload counters instead of formatting the complete backdrop
pass vector. `shadow_draws` counts analytic shadow quads and `shadow_pixel_upper_bound` sums their
output-clipped bounds before damage scissoring. `backdrop_sample_pixels` is the sum of
radius-expanded copy regions;
`backdrop_filter_pixels` counts both separable passes;
`backdrop_filter_texture_samples` multiplies that value by the shader's fixed nine taps;
`backdrop_composite_pixel_upper_bound` sums the exact active composite-rectangle areas before final
damage scissoring; and `backdrop_retained_intermediate_pixels` is the largest active local
extent times the two retained lanes. The counters are accumulated with saturating integer
arithmetic while the pass plan is already being built. They allocate nothing, query no Vulkan
state, and remain zero for direct frames. This keeps hardware measurements comparable while
distinguishing command topology and theoretical pixel work from driver-dependent elapsed time or
memory bandwidth.

The remaining acceptance work is hardware evidence rather than an architectural fallback: verify
region edges and fractional-scale radii on supported DRM/Vulkan devices, then measure direct-frame
CPU/GPU deltas and backdrop bandwidth before considering cached or downsampled quality modes.

The shared allocator reserves the aligned implementation range as a suffix, so Tensorland application
descriptors start at byte zero and remain densely compiler-strided. Tensorland caps the requested
resource payload below `maxResourceHeapSize` after that suffix, then uses the shared image/buffer
stride and allocation alignment. This makes device-local, host-visible, single-pass, and future
multi-pass users share one explicit heap contract without imposing Tensorland's frame policy on them.

Pointer visibility is a compositor overlay, not client-scene state. Tensorland cursor state and any
client cursor surface stay in the protocol owner; at frame submission
they become a value-only, output-local physical `CursorOverlay`. Named `wp_cursor_shape` images are
loaded lazily from the configured XCursor theme at the output's fractional scale. Client
`wl_pointer.set_cursor` and tablet cursor surfaces reuse the normal SHM image cache and preserve
their viewport/buffer transform. Both paths enter the same deduplicated descriptor heap as scene
images and draw after all client content. Core and cursor-shape requests require the latest
pointer-enter or tablet-proximity serial even when updating the current cursor surface. Pixel data
is copied only once into the persistent Vulkan
staging allocation when an image version is imported; normal cursor frames carry only stable buffer
IDs and transforms. Cursor overlays and their descriptor indices share fixed 66-slot inline
storage, so pointer/tablet/DnD descriptor preparation does not allocate per frame. Animated XCursor
frames of the selected nominal size are all uploaded during
that cold load, then a dedicated one-shot timerfd advances their IDs through Compio io_uring
completions; there is no readiness loop or periodic polling. A completion advances only active
named cursors and queues the outputs intersecting their complete old/new extents. The small
descriptor-free vector arrow remains only for a missing theme image. Hotspots include committed `wl_surface.offset`
deltas, cursor image or geometry changes damage the old and new physical bounds, and successfully
submitted cursor surfaces receive frame callbacks so client-driven animation can advance. Cursor
role state also tracks output-instance membership: crossing a head emits `wl_surface.enter/leave`
and publishes that head's integer and fractional preferred scale. Stable membership is cached, so
ordinary high-rate motion does not lock or rescan each output's protocol surface list.
Pointer and tablet damage is selected from the complete old/new cursor rectangles rather than the
hotspot alone. A cursor can therefore straddle differently scaled CRTCs without either clipping its
adjacent-output pixels or leaving stale pixels behind. A client cursor surface with no committed
buffer is invisible; it never falls through to the compositor's vector cursor.
Tablet device removal snapshots at most 64 active tool positions in fixed-capacity storage, damages
only their old extents, and retires cursor-surface output membership before one batched redraw.
Presentation capture keeps the ordinary pointer cursor surface inline, so a pointer-only frame
does not allocate a cursor-surface vector; storage grows only when tablet cursor surfaces are
actually visible.
Core `wl_data_device` drag icons use the same cached surface-image path and an inline presentation
slot. Their committed `wl_surface.offset` moves the icon relative to the pointer hotspot, output
enter/leave and preferred scale follow the visible icon rectangle, and accepted KMS submissions
release their frame callbacks. The icon is a cursor underlay, so the pointer remains legible above
it; old and new icon memberships queue only the affected outputs.

This is a deliberately distilled contract from the local Niri, Hyprland, and Nourish references:
Niri establishes pointer-as-topmost-render-element and output-aware relative motion; Hyprland
establishes old/new software-cursor damage and rejects invalid input coordinates; Nourish
establishes an explicit logical-to-output-physical cursor boundary. Tensorland keeps the resulting
state and geometry value-only at the renderer boundary, without adopting any reference project's
renderer or KMS ownership model.

The completed software-composition baseline and the gated atomic hardware cursor-plane work are
tracked in [`cursor.md`](cursor.md). That document is authoritative for cursor follow-up work,
performance acceptance, and completion criteria.

The current command stream is deliberately limited to:

1. upload typed native/client image descriptors and bind the resource and sampler heaps;
2. use shared semantic barrier batches to acquire the selected output and imported client images
   from `VK_QUEUE_FAMILY_FOREIGN_EXT` (or upload a changed SHM snapshot);
3. run dynamic rendering and draw sampled client rectangles with transform, opacity, clip, and
   corner-radius data;
4. draw sampled named/client cursors, or the descriptor-free fallback arrow, over client content;
5. use the same semantic barrier batch to release client images and the output to
   `VK_QUEUE_FAMILY_FOREIGN_EXT` for Tensorland KMS.

This is a real client-image sampling slice, not a descriptor-only diagnostic clear. It is not yet a
complete Wayland renderer: implicit-sync dma-bufs, multi-plane YUV, and damage-driven partial
rendering remain separate gates. Debug diagnostics report draw count, unique client-image descriptor
count, surface-content count, and damage-region count for each prepared frame.

`wp_presentation` v2 uses `CLOCK_MONOTONIC`. Before Vulkan submission, the protocol layer takes
feedback only from surfaces intersecting the submitted scene and whose largest output intersection
selects that output as primary. This is geometry-aware but not yet opaque-region occlusion-aware.
The resulting owner is keyed by both stable backend output ID and renderer timeline value. Renderer
failure, missing `SYNC_FD`, atomic KMS failure, output replacement, disconnect, or session pause
drops that owner and therefore sends `discarded`. Once atomic KMS accepts the frame, frame callbacks
are released immediately so clients can prepare the next commit; only the matching DRM vblank sends
`presented`. Monotonic DRM metadata carries `HwClock`; realtime, zero, or missing metadata falls back
to the compositor's monotonic clock without claiming hardware clock accuracy.

## Synchronization

Internal frame scheduling uses the shared renderer's Vulkan timeline semaphore. Tensorland reserves a
timeline value from that device before frame allocation; it never creates a second timeline or
command pool. Timeline semaphores are not exported as `SYNC_FD`: Linux sync-file interop uses
binary semaphores. Each submitted output frame also signals an exportable shared
`BinarySemaphore`; the renderer exports its `SYNC_FD`, and the tty backend consumes it as atomic
KMS `IN_FENCE_FD`. Tensorland's tty adapter owns commit/page-flip submission; steady-state
flips update only `FB_ID` and `IN_FENCE_FD` through a fixed stack request. One per-device operation
on the compositor-thread Compio runtime completes for vblank, then drm-rs decodes one fixed stack
batch before explicit rearm. The bounded repaint queue
waits for a free output slot before rendering another frame, so a current scanout buffer is never
reused while it is still displayed. Renderer timeline retirement and KMS release are separate gates.
An input-driven compositor overlay repaint remains pending when its only blocked gate is a Vulkan
timeline submission that has not retired yet. Tensorland duplicates that submission's exported
`SYNC_FD` and submits one io_uring `PollAdd` through a dedicated Compio runtime. The kernel request
completes only when the sync-file fence signals, then a bounded `{output, timeline}` value reaches
the compositor and retires renderer resources. This is a one-shot fence completion, not a timer
poll, epoll registry, or generic readiness loop. KMS receives the original `SYNC_FD`, and KMS-owned
slot waits remain driven by the next page flip.
After VT/session resume, Tensorland rebuilds each property cache and mode blob, runs TEST_ONLY, and
marks the next submit as a modeset while quarantining previously current and pending slots. A
completion-turn tail requests repaint only after already-completed DRM events drain; the first new
page flip releases the quarantine. If the old Vulkan submission is still completing, its
submitted sync-file wait triggers recovery only after the GPU fence signals. If repeated
interruption leaves every slot uncertain, Tensorland resets that DRM device instead of risking writes
into a scanned-out dma-buf.

Tensorland directly implements the modern `wp_linux_drm_syncobj_v1` path. The global
is created only when the Vulkan-selected primary DRM device supports `drmSyncobjEventfd`; hot-unplug
or session pause closes the import device, and hotplug/session recovery updates the same protocol
owner. Tensorland does not publish the older `zwp_linux_explicit_synchronization_v1` as a parallel
compatibility surface.
Once a surface binds the syncobj add-on, every non-null buffer attach must provide both points;
missing, conflicting, or non-dma-buf commits are rejected and unmapped rather than sampled through
an implicit fallback.

On commit, the protocol owner removes the acquire/release points from Tensorland's cached surface state
before applying the buffer attachment. This prevents buffer-drop release from signalling a point
while Vulkan still samples that dma-buf. The acquire point is exported to a sync file, imported into
a temporary shared `BinarySemaphore` payload, and waited at the fragment-shader stage. A failed
`queue_submit2` leaves
that imported semaphore pending for retry and does not advance imported-image state or the client
release point.

Tensorland applies synchronized subsurfaces before invoking their non-synchronized ancestor commit
path. It records those child sync points without changing ECS or renderer attachment state;
the ancestor callback rebuilds the complete tree first and then reconciles every deferred point
against the newly active `SurfaceBufferId`. This preserves transaction atomicity and prevents a
release point from being associated with the child's previous buffer.

After a successful submission, the acquire semaphore is retired by the internal Vulkan timeline.
The exported binary completion sync file is retained per explicitly synchronized surface while a
duplicate is handed to KMS. Each repaint replaces the retained completion with the latest GPU read,
so detaching or replacing the surface attachment imports the newest fence into the client's release
timeline point. Tensorland never signals release merely because the first frame completed: the scene may
reuse that dma-buf on a later repaint. An attachment that was never submitted can be released
immediately. Sync-file import failures are retained for retry; if queue submission succeeded but
completion export failed, the release point remains gated by the renderer timeline instead of being
signalled early.

Exporting or importing a sync file is an explicit API boundary, not a reason to wait on the CPU.
Timeline semaphores never cross it, descriptor sets are not introduced for this path, and a device
without both importable and exportable binary `SYNC_FD` support fails selection. Implicit-sync
clients still need a defined dma-buf reservation-fence policy before Tensorland can claim the full
linux-dmabuf ecosystem is complete.

Color/HDR, per-surface tearing eligibility, live mode replacement, multi-output
capture assembly, cursor-only GPU taps, multi-plane import, and implicit-fence
closure follow the explicit completion gates in
[`protocol-roadmap.md`](protocol-roadmap.md). These features lower into generic
renderer color/target/synchronization descriptors and Tensorland-owned KMS
plans; the shared renderer does not receive Wayland protocol types or output
policy.
