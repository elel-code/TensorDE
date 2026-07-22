# Rendering Contract

Tensor has one native renderer: Vulkanalia with `VK_EXT_descriptor_heap`. Descriptor sets,
descriptor buffers, GLES, and software composition are not compatibility backends.

## Native Device Gate

An eligible physical device must provide all of the following before ranking:

- Vulkan 1.4 and a graphics queue.
- `VK_EXT_descriptor_heap`, including its feature bit.
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

Imported client dma-bufs and compositor-owned output images use separate lifetime caches. A cache
entry is keyed by stable buffer identity plus format, modifier, dimensions, plane offsets, and
strides; an fd number is never an identity. Buffer reuse waits for the KMS release path before
Vulkan writes the image again.

## Frame Boundary Status

`render/frame.rs` is the renderer-to-scene boundary. It owns a bounded resource descriptor heap
allocator, retains the previous `SceneSnapshot` per output, computes damage, and keeps descriptor
ranges live until the Vulkan timeline value retires them. `render/vulkan/frame.rs` uses three
resettable command buffers and one timeline semaphore to exercise that lifetime contract. A lost
device stops future frame scheduling instead of recycling GPU-visible ranges.

The allocator starts after `minResourceHeapReservedRange`, rounds resource descriptors to the
reported image descriptor alignment, and adds the implementation's reserved range before capping
the configured usable budget at `maxResourceHeapSize`. These are device properties, not a
substitute for the eventual Vulkan heap binding and descriptor writes.

The current boundary deliberately submits an empty command buffer. Native output target value
negotiation and validation are connected, but Vulkan image allocation, descriptor writes, dma-buf
export, queue-family release, and Smithay atomic KMS commit are the next required layer. Until those
handles and fences are connected, presentation-time, alpha-modifier, and background-effect globals
remain unadvertised; a timeline submission alone is not a displayed frame.

## Synchronization

Internal frame scheduling uses Vulkan timeline semaphores. Timeline semaphores are not exported as
`SYNC_FD`: Linux sync-file interop uses binary semaphores. Each submitted output frame signals an
exportable binary semaphore, whose `SYNC_FD` becomes the atomic KMS `IN_FENCE_FD`. Smithay owns the
commit and page-flip lifecycle and returns the release signal that permits image reuse.

Client acquire fences are imported as temporary binary semaphore payloads. Exporting or importing
a sync file is an explicit API boundary, not a reason to wait on the CPU. A device without both
importable and exportable `SYNC_FD` support fails selection; Tensor does not silently fall back to
blocking queue-idle synchronization.
