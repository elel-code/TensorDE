# Rendering Contract

Tensor has one native renderer: Vulkanalia with `VK_EXT_descriptor_heap`. Descriptor sets,
descriptor buffers, GLES, and software composition are not compatibility backends.

## Native Device Gate

An eligible physical device must provide all of the following before ranking:

- Vulkan 1.4 and a graphics queue.
- `VK_EXT_descriptor_heap`, including its feature bit.
- Dynamic rendering, enabled from the Vulkan 1.3 feature chain for Tensor's
  render-pass-free client-image pipeline.
- Vulkan 1.4's `maintenance5` feature, which is a required descriptor-heap dependency and is
  enabled in the device feature chain. When its promoted `VK_KHR_maintenance5` name is also
  advertised, Tensor enables that name too, matching the descriptor-heap extension dependency.
- Buffer device address support (the heap is bound as a device-address range; no descriptor-buffer
  or descriptor-set fallback is permitted).
- A usable resource heap: non-zero heap alignment, maximum size beyond the implementation's
  reserved range, and non-zero image descriptor size/alignment.
- A sampler heap range large enough for `minSamplerHeapReservedRangeWithEmbedded`, plus at least one
  embedded sampler and enough push data for Tensor's 64-byte draw record.
- `VK_EXT_physical_device_drm` with a complete primary/render node pair.
- `VK_KHR_external_memory_fd` and `VK_EXT_external_memory_dma_buf`.
- `VK_EXT_image_drm_format_modifier`.
- `VK_EXT_queue_family_foreign` for ownership transfers to and from non-Vulkan consumers.
- `VK_KHR_external_semaphore_fd`.
- Importable and exportable binary `SYNC_FD` semaphores, verified through
  `vkGetPhysicalDeviceExternalSemaphoreProperties`.

Extension availability alone does not prove that a usable image exists. Output initialization must
also intersect Vulkan's per-format external-image modifier properties with Smithay's DRM plane and
GBM scanout capabilities. Readiness requires at least one renderable, exportable, explicit-modifier
format for every active output path.

## Native Format Gate

`render::format` is the value-only boundary between Vulkanalia and the Smithay tty backend. Vulkan
probing enumerates `VkDrmFormatModifierPropertiesList2EXT`, rejects modifiers without color-target
support, and calls `vkGetPhysicalDeviceImageFormatProperties2` with the real sampled,
color-attachment, and transfer usage. `VkExternalImageFormatProperties` records dma-buf import and
export separately: client imports and compositor-owned output exports are different capability
roles and are never inferred from each other.

For each connector with a mode and mapped CRTC, the tty backend reads Smithay's primary-plane
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
plane is exported with its offset and row pitch. The tty boundary imports those dma-bufs through
Smithay's GBM device and validates dimensions, fourcc, modifier, and plane count before retaining
the GBM objects. Vulkan image resources replaced by a mode or format change remain in a retired
queue until their last renderer timeline value completes.

GBM remains owned by Smithay and does not become a renderer. Its check validates the allocation and
KMS-facing boundary; Vulkanalia remains the only component that creates and renders native output
images.

## Buffer Ownership

Vulkanalia owns render images and their memory. A native render target is allocated with an
explicit DRM modifier and exportable dma-buf memory. Tensor exports the dma-buf planes to Smithay;
Smithay owns DRM/KMS, its GBM device, framebuffer creation, atomic commits, page flips, and direct
scanout decisions. Vulkan handles and DRM surface handles never cross into ECS or IPC.

Before an image leaves Vulkan for KMS or another API, Tensor releases it to
`VK_QUEUE_FAMILY_FOREIGN_EXT`; imported images are acquired back from the same external owner.
This is mandatory for multi-plane and driver-compressed modifiers and is never replaced with a
queue-idle compatibility path.

Imported client dma-bufs and compositor-owned output images use separate lifetime caches. A client
cache entry is keyed by the compositor-assigned stable buffer identity and retains the validated
format, modifier, dimensions, plane offsets, and strides from the Smithay dma-buf object; an fd
number is never an identity. Buffer reuse waits for the renderer timeline and Wayland release path
before the Vulkan image is destroyed.

## Client linux-dmabuf

The `zwp_linux_dmabuf_v1` global is created only after the selected Vulkan device provides a
non-empty client-import format list. Feedback is built from that device's explicit modifiers and
render-node identity; it is not copied from a KMS-only list. The initial import contract is
deliberately narrow and honest: explicit-modifier, single-plane RGB buffers whose fd memory type
is accepted by the selected Vulkan device.

For each `params` request Smithay validates the protocol shape, then Vulkan creates an explicit
modifier image, intersects image and dma-buf fd memory-type masks, binds imported memory, and
creates a view. Only a completed image/view import calls `ImportNotifier::successful`; malformed
planes, implicit modifiers, unsupported formats, and Vulkan failures call `failed` instead. The
image cache is keyed by `SurfaceBufferId` and retires resources after the renderer timeline, so a
duplicated fd or recycled Wayland object ID cannot alias a live scene image accidentally.

Client images are now accepted by the protocol boundary and have a first real sampling path. A
client image is acquired from the foreign queue family, selected through a descriptor-heap push
index, sampled with an embedded linear sampler, and composited by a dynamic-rendering pipeline with
premultiplied-alpha blending. The first acquire uses `UNDEFINED + FOREIGN` to preserve the
producer's explicit-modifier contents; only a successful queue submission advances the cache to the
subsequent `GENERAL + FOREIGN` path. Resource and sampler heap ranges share one device allocation but
are disjoint, including the implementation-reserved sampler range required by embedded samplers.
The path is intentionally limited to one-plane RGB today. Explicit producer/consumer
synchronization for these buffers is provided through `wp_linux_drm_syncobj_v1`; implicit dma-buf
reservation-fence interop remains a separate gate.

The protocol-to-scene handoff has an explicit value-only boundary. A
compositor-assigned `SurfaceBufferId` is registered after a successful
linux-dmabuf import. Smithay's toplevel, synchronized/asynchronous subsurface,
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
use the same scene conversion. Legacy clients that only observe `wl_output.scale` receive Smithay's
rounded-up integer value, so a 1.25 output lets XWayland render a 2x client buffer before the
compositor downsamples it to the fractional physical destination. The embedded surface sampler is
linear for this final conversion; Tensor must not introduce a default nearest-neighbor XWayland
branch. Tensor does not add an X11-session renderer, a second X11 coordinate space, or an
X11-specific damage path.

This follows the quality-producing invariant observed in Niri's xwayland-satellite integration:
X11 windows become ordinary Wayland surfaces and inherit the normal fractional-scale pipeline.
Hyprland's separate XWayland monitor positions, `force_zero_scaling` coordinate conversions, and
default XWayland nearest-neighbor switch are useful reference failure modes, not Tensor APIs to
copy. A future per-window pixel-art filter may be explicit policy, but X11 provenance alone must
never reduce sampling quality.

## Frame Boundary Status

`render/frame.rs` is the renderer-to-scene boundary. It owns a bounded resource descriptor heap
allocator, retains the previous `SceneSnapshot` per output, computes damage, assigns one of three
native output image slots, and keeps descriptor ranges live until the Vulkan timeline value retires.
`render/vulkan/heap.rs` creates the actual device-addressable resource and sampler heap ranges with
`VK_BUFFER_USAGE_DESCRIPTOR_HEAP_BIT_EXT`, a host-visible staging buffer, and the descriptor write
path. `render/vulkan/frame.rs` writes the native output and deduplicated client-image descriptors
into staging, copies them into the device-local resource range, binds both heaps, and submits through
three resettable command buffers plus one timeline semaphore. The sampled-image pipeline pushes a
64-byte draw record whose first word is the descriptor index relative to the resource heap's user
range, while the pipeline mapping supplies the implementation-reserved byte offset. A lost device
stops future frame scheduling instead of recycling GPU-visible ranges.

The allocator starts after `minResourceHeapReservedRange`, rounds resource descriptors to the
reported image descriptor alignment, and adds the implementation's reserved range before capping
the configured usable budget at `maxResourceHeapSize`. The Vulkan heap uses the same capacity and
offset contract, so allocator ranges are now copied into the real heap rather than remaining a
simulation.

Pointer visibility is a compositor overlay, not client-scene state. Smithay's
`CursorImageStatus` and any client cursor surface stay in the protocol owner; at frame submission
they become a value-only, output-local physical `CursorOverlay`. The current visible fallback is a
small vector arrow, including for a named or client-provided cursor image until cursor-raster
upload is added. It draws after all client content, is clipped to the native target, and damages
both its old and new physical bounds on motion. Its pipeline has no sampled resources and uses only
a 16-byte vertex push constant with a zero-set layout; it is not a descriptor-set rendering path.

This is a deliberately distilled contract from the local Niri, Hyprland, and Nourish references:
Niri establishes pointer-as-topmost-render-element and output-aware relative motion; Hyprland
establishes old/new software-cursor damage and rejects invalid input coordinates; Nourish
establishes an explicit logical-to-output-physical cursor boundary. Tensor keeps the resulting
state and geometry value-only at the renderer boundary, without adopting any reference project's
renderer or KMS ownership model.

The current command stream is deliberately limited to:

1. upload native/client image descriptors and bind the resource and sampler heaps;
2. acquire the selected output and imported client images from `VK_QUEUE_FAMILY_FOREIGN_EXT`;
3. run dynamic rendering and draw sampled client rectangles with transform, opacity, clip, and
   corner-radius data;
4. draw the compositor-owned vector cursor over client content when the pointer is visible;
5. release client images and the output to `VK_QUEUE_FAMILY_FOREIGN_EXT` for Smithay/KMS.

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

Internal frame scheduling uses Vulkan timeline semaphores. Timeline semaphores are not exported as
`SYNC_FD`: Linux sync-file interop uses binary semaphores. Each submitted output frame also signals
an exportable binary semaphore; the renderer exports its `SYNC_FD`, and the tty backend consumes it
as atomic KMS `IN_FENCE_FD`. Smithay owns commit/page-flip and vblank; the bounded repaint queue
waits for a free output slot before rendering another frame, so a current scanout buffer is never
reused while it is still displayed. Renderer timeline retirement and KMS release are separate gates.
An input-driven compositor overlay repaint remains pending when its only blocked gate is a Vulkan
timeline submission that has not retired yet. A low-frequency one-shot calloop timer then polls
completion until it can submit; KMS-owned slot waits remain driven by the next page flip, so Tensor
does not spin or treat an old page flip as retirement of new GPU work.
After VT/session resume, Smithay refreshes each DRM surface while Tensor quarantines the previously
current and pending slots. A calloop idle repaint runs only after already-ready DRM events drain; the
first new page flip releases the quarantine. If the old Vulkan submission is still completing, a
low-frequency one-shot calloop timer polls its timeline until the recovery frame can be selected;
it stops before KMS slot waiting and never blocks the event loop. If repeated interruption leaves
every slot uncertain, Tensor resets that DRM device instead of risking writes into a scanned-out
dma-buf.

Tensor implements the modern `wp_linux_drm_syncobj_v1` path supplied by Smithay master. The global
is created only when the Vulkan-selected primary DRM device supports `drmSyncobjEventfd`; hot-unplug
or session pause closes the import device, and hotplug/session recovery updates the same protocol
owner. Tensor does not publish the older `zwp_linux_explicit_synchronization_v1` as a parallel
compatibility surface.
Once a surface binds the syncobj add-on, every non-null buffer attach must provide both points;
missing, conflicting, or non-dma-buf commits are rejected and unmapped rather than sampled through
an implicit fallback.

On commit, the protocol owner removes the acquire/release points from Smithay's renderer cache
before `on_commit_buffer_handler` can consume them. This is deliberate: Smithay objects remain in
the protocol layer, and its default buffer-drop release must not signal a point while Vulkan still
samples that dma-buf. The acquire point is exported to a sync file, imported into a temporary binary
Vulkan semaphore payload, and waited at the fragment-shader stage. A failed `queue_submit2` leaves
that imported semaphore pending for retry and does not advance imported-image state or the client
release point.

Smithay applies synchronized subsurfaces before invoking their non-synchronized ancestor commit
callback. Tensor records those child sync points without changing ECS or renderer attachment state;
the ancestor callback rebuilds the complete tree first and then reconciles every deferred point
against the newly active `SurfaceBufferId`. This preserves transaction atomicity and prevents a
release point from being associated with the child's previous buffer.

After a successful submission, the acquire semaphore is retired by the internal Vulkan timeline.
The exported binary completion sync file is retained per explicitly synchronized surface while a
duplicate is handed to KMS. Each repaint replaces the retained completion with the latest GPU read,
so detaching or replacing the surface attachment imports the newest fence into the client's release
timeline point. Tensor never signals release merely because the first frame completed: the scene may
reuse that dma-buf on a later repaint. An attachment that was never submitted can be released
immediately. Sync-file import failures are retained for retry; if queue submission succeeded but
completion export failed, the release point remains gated by the renderer timeline instead of being
signalled early.

Exporting or importing a sync file is an explicit API boundary, not a reason to wait on the CPU.
Timeline semaphores never cross it, descriptor sets are not introduced for this path, and a device
without both importable and exportable binary `SYNC_FD` support fails selection. Implicit-sync
clients still need a defined dma-buf reservation-fence policy before Tensor can claim the full
linux-dmabuf ecosystem is complete.
