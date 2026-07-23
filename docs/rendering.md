# Rendering Contract

Tensor has one native renderer: Vulkanalia with `VK_EXT_descriptor_heap`. Descriptor sets,
descriptor buffers, GLES, and software composition are not compatibility backends.

## Native Device Gate

An eligible physical device must provide all of the following before ranking:

- Vulkan 1.4 and a graphics queue.
- `VK_EXT_descriptor_heap`, including its feature bit.
- Buffer device address support (the heap is bound as a device-address range; no descriptor-buffer
  or descriptor-set fallback is permitted).
- A usable resource heap: non-zero heap alignment, maximum size beyond the implementation's
  reserved range, and non-zero image descriptor size/alignment.
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

Client images are now accepted by the protocol boundary. Scene sampling and draw-pipeline wiring
are the next renderer step; until that is complete, the native output path remains the only path
that can put pixels on a KMS plane.

The protocol-to-scene handoff has an explicit value-only boundary. A
compositor-assigned `SurfaceBufferId` is registered after a successful
linux-dmabuf import; current surface content, revision, scale, transform, and
surface-local destination geometry are copied into ECS as `ViewContent`. The
scene flattens those values into a content table and builds a per-frame draw
plan that deduplicates image descriptor slots while preserving surface draw
order. Destroyed buffers remain renderer-live while any surface still refers to
them, and imported images are marked with the submission timeline used by that
plan.

## Frame Boundary Status

`render/frame.rs` is the renderer-to-scene boundary. It owns a bounded resource descriptor heap
allocator, retains the previous `SceneSnapshot` per output, computes damage, assigns one of three
native output image slots, and keeps descriptor ranges live until the Vulkan timeline value retires.
`render/vulkan/heap.rs` creates the actual device-addressable resource-heap buffer with
`VK_BUFFER_USAGE_DESCRIPTOR_HEAP_BIT_EXT`, a host-visible staging buffer, and the descriptor write
path. `render/vulkan/frame.rs` writes the native output and deduplicated client-image descriptors
into staging, copies them into the device-local heap, binds it with
`vkCmdBindResourceHeapEXT`, and submits through three resettable command buffers plus one timeline
semaphore. The frame allocation also reserves the future draw-record range; those bytes are
currently zeroed until the sampled-image pipeline consumes them. A lost device stops future frame
scheduling instead of recycling GPU-visible ranges.

The allocator starts after `minResourceHeapReservedRange`, rounds resource descriptors to the
reported image descriptor alignment, and adds the implementation's reserved range before capping
the configured usable budget at `maxResourceHeapSize`. The Vulkan heap uses the same capacity and
offset contract, so allocator ranges are now copied into the real heap rather than remaining a
simulation.

The current command stream is deliberately limited to:

1. upload native/client image descriptors and bind the resource heap;
2. acquire and clear the selected native output image;
3. release that image to `VK_QUEUE_FAMILY_FOREIGN_EXT` for Smithay/KMS.

The draw plan is ready for a sampled-image pipeline, but scene draw pipelines still need to sample
imported client images and emit real scene pixels; the clear is an intentional diagnostic frame,
not a claim that the compositor already renders application content. Debug diagnostics report draw
count, unique client-image descriptor count, surface-content count, and damage-region count for
each prepared frame.

## Synchronization

Internal frame scheduling uses Vulkan timeline semaphores. Timeline semaphores are not exported as
`SYNC_FD`: Linux sync-file interop uses binary semaphores. Each submitted output frame also signals
an exportable binary semaphore; the renderer exports its `SYNC_FD`, and the tty backend consumes it
as atomic KMS `IN_FENCE_FD`. Smithay owns commit/page-flip and vblank; the bounded repaint queue
waits for a free output slot before rendering another frame, so a current scanout buffer is never
reused while it is still displayed. Renderer timeline retirement and KMS release are separate gates.

Client acquire fences are imported as temporary binary semaphore payloads. Exporting or importing
a sync file is an explicit API boundary, not a reason to wait on the CPU. A device without both
importable and exportable `SYNC_FD` support fails selection; Tensor does not silently fall back to
blocking queue-idle synchronization.
