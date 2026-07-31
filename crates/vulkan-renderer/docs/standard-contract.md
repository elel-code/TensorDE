# Standard API contract

The words **MUST**, **MUST NOT**, **SHOULD**, and **MAY** are normative.

## Object model

1. `Instance::new(InstanceDescriptor)` MUST validate the loader API version and
   MUST create exactly one Vulkan instance with the profile and caller-required
   instance extensions.
2. `Instance::enumerate_adapters()` reports physical-device capabilities. It
   MUST NOT create a logical device or imply that every returned adapter is
   compatible with the active profile.
3. `Instance::request_adapter(RequestAdapterOptions)` MUST apply every hard
   profile requirement before applying `PowerPreference`. Preference only
   ranks compatible devices.
4. `Adapter::request_device(DeviceDescriptor)` MUST fail when any requested
   feature, limit, or extension is unavailable. It MUST NOT silently remove a
   request or select a different physical device.
5. The returned `Device` and `Queue` independently retain logical-device and
   instance ownership. Destruction occurs after the final owner is dropped.

## Feature enablement

| Public feature | Extension gate | Vulkan feature field |
|---|---|---|
| `TIMELINE_SEMAPHORE` | core 1.2 | `timelineSemaphore` |
| `BUFFER_DEVICE_ADDRESS` | core 1.2 | `bufferDeviceAddress` |
| `SYNCHRONIZATION2` | core 1.3 | `synchronization2` |
| `DYNAMIC_RENDERING` | core 1.3 | `dynamicRendering` |
| `MAINTENANCE5` | core 1.4 | `maintenance5` |
| `MAINTENANCE6` | core 1.4 | `maintenance6` |
| `DYNAMIC_RENDERING_LOCAL_READ` | core 1.4 | `dynamicRenderingLocalRead` |
| `DESCRIPTOR_HEAP` | `VK_EXT_descriptor_heap` | `descriptorHeap` |
| `PIPELINE_BINARIES` | `VK_KHR_pipeline_binary` | `pipelineBinaries` |
| `FIFO_LATEST_READY` | `VK_KHR_present_mode_fifo_latest_ready` | `presentModeFifoLatestReady` |
| `EXTERNAL_MEMORY_DMA_BUF` | `VK_KHR_external_memory_fd`, `VK_EXT_external_memory_dma_buf`, `VK_EXT_image_drm_format_modifier`, `VK_EXT_queue_family_foreign` | format/modifier-specific query |
| `EXTERNAL_SEMAPHORE_SYNC_FD` | `VK_KHR_external_semaphore_fd` | `IMPORTABLE | EXPORTABLE` for `SYNC_FD` |

Adapter support and device enablement are separate bitsets. A feature MUST be
present in the adapter bitset and requested in `DeviceDescriptor` before the
device contract exposes it.

`Features::STANDARD_DEFAULTS` MUST include the Vulkan 1.4 renderer baseline,
`DESCRIPTOR_HEAP`, `PIPELINE_BINARIES`, and `FIFO_LATEST_READY`. The standard
instance/adapter/device path uses this set as a hard minimum.

`DESCRIPTOR_HEAP` transitively requires and enables `BUFFER_DEVICE_ADDRESS` and
`MAINTENANCE5`. Dependency expansion is part of the returned device contract;
the backend MUST NOT create a descriptor-heap device with either prerequisite
disabled.

`FIFO_LATEST_READY` additionally requires `VK_KHR_swapchain` on the device and
`VK_KHR_surface` on the instance. A platform extension such as
`VK_KHR_wayland_surface` remains caller-selected. For `DESCRIPTOR_HEAP`, Vulkan
1.4 core `maintenance5` is sufficient; `VK_KHR_maintenance5` is enabled only
when the selected driver still advertises that promoted extension name.

## Limit comparison

Maximum-capacity fields use `requested <= supported`. Alignment fields use
`supported <= requested` when the requested value is nonzero because a lower
required Vulkan alignment is more capable. A zero requested field means the
caller imposes no additional constraint beyond the selected profile.

For descriptor heaps, device creation additionally requires:

- every reported alignment is a nonzero power of two;
- sampler, image, and buffer descriptor sizes are nonzero;
- push-data size and embedded-sampler count are nonzero;
- aligned implementation-reserved ranges leave nonzero sampler/resource
  payload ranges.

## Pipeline machine code

1. Distributed shader assets MUST be validated SPIR-V. Device-native pipeline
   binaries MUST NOT be distributed as portable assets.
2. `VK_KHR_pipeline_binary` and `pipelineBinaries` MUST be enabled on the
   logical device. Extension-name presence alone is not support.
3. A cold-cache pipeline MUST first be created with
   `CAPTURE_DATA_KHR`. Every returned binary key and data payload MUST be
   materialized before the pipeline can become visible to frame recording.
4. The provisional pipeline MUST be destroyed and recreated from the complete,
   ordered binary set through `VkPipelineBinaryInfoKHR`. Supplying binary
   handles is the no-compile contract; Vulkan forbids combining a non-empty
   binary set with `FAIL_ON_PIPELINE_COMPILE_REQUIRED`. A non-success
   status is a startup failure; it MUST NOT fall through to a visible submit
   that compiles in the driver.
5. Persistent archives MUST be bound to the physical-device UUID, driver UUID
   and version, complete pipeline key, and renderer binary-format version.
   Any mismatch invalidates the archive before Vulkan sees it.
6. `PipelineCache` data does not prove machine-code readiness and MUST NOT be
   used as a substitute for this contract.
7. The shared machine-code pipeline retains its logical device, destroys its
   final `VkPipeline` through RAII, and implements `SubmissionResource` for
   timeline retirement. Product code MUST NOT own or manually destroy that
   final handle. Raw create-info callbacks are migration interop only.

## Descriptor heap storage

1. A heap buffer MUST use `DESCRIPTOR_HEAP_EXT` and
   `SHADER_DEVICE_ADDRESS`. Its memory allocation MUST enable
   `DEVICE_ADDRESS`, be host visible, and remain persistently mapped while the
   heap exists.
2. The heap layout MUST be `[application descriptor bytes][aligned reserved
   range]`. `VkBindHeapInfoEXT.reservedRangeOffset` MUST point to the end of the
   application region; the allocator MUST NOT return bytes from the reserved
   range.
3. Resource, image, and sampler writes MUST use the exact queried descriptor
   size and alignment. A write for one heap kind MUST NOT target the other.
   Sampled, storage, and input-attachment images share the queried image
   descriptor representation; storage images MUST declare `GENERAL` layout.
4. Non-coherent writes MUST be flushed with offsets and sizes satisfying
   `nonCoherentAtomSize`.
5. Binding MUST use `vkCmdBindResourceHeapEXT` or
   `vkCmdBindSamplerHeapEXT` according to heap kind. There is no descriptor-set
   fallback in the standard backend.
6. Descriptor storage and every referenced Vulkan resource MUST remain live
   and unmodified until the submission timeline completes.

### Separately sampled textures

`SampledTextureBinding` is the standard convenience object for a SPIR-V
sampled-image binding plus a separate sampler binding. It MUST allocate one
`SAMPLED_IMAGE` descriptor from a resource heap and one `SAMPLER` descriptor
from a sampler heap owned by the same logical device as its `ImageView`.

1. `SampledTextureShaderBindings` MUST use distinct bindings for image and
   sampler. The generated `ShaderBindingMap` MUST retain exact heap offsets,
   reject offsets that do not fit Vulkan's `u32` mapping fields, and use zero
   array stride for its one-element mappings. A separate `OpTypeSampler` MUST
   use `VkDescriptorMappingSourceConstantOffsetEXT.heapOffset`;
   `samplerHeapOffset` is only the sampler half of an `OpTypeSampledImage`.
2. The caller MUST retain the `ImageView` through every command submission
   that samples it. `ImageView` implements `SubmissionResource` for this
   purpose; descriptor heaps themselves remain explicit application-owned
   storage.
3. A binding abandoned before command submission MUST be released immediately.
   A binding visible to GPU work MUST be retired using the final frame token;
   it MUST NOT be reused merely because its Rust owner was dropped.
4. The declared descriptor image layout MUST match the render graph's sampled
   image state. The helper does not insert an implicit layout transition or
   create a descriptor-set compatibility path.
5. A push-index sampled-texture map MUST interpret each pushed `u32` as an
   exact descriptor byte offset (`heapIndexStride = 1`). The value MUST come
   from `SampledTextureBinding::push_index_heap_offsets` so an offset beyond
   the mapping's 32-bit representation fails instead of truncating.

### Shader binding mapping

1. `ShaderBindingSource` MUST be the sole safe representation of Vulkan's
   `source`/`sourceData` tagged union. The source tag and union member MUST be
   generated from the same enum variant.
2. `PushIndexMapping.push_offset` MUST be 4-byte aligned and address a complete
   `u32` inside `maxPushDataSize`. `IndirectIndexMapping.push_offset` MUST be
   8-byte aligned and address a complete `VkDeviceAddress`; its
   `address_offset` MUST be 4-byte aligned.
3. Heap offsets and array strides MUST satisfy every image, buffer, or sampler
   descriptor alignment selected by the mapping's resource mask. Pipeline
   creation MUST validate these device-dependent rules before Vulkan sees the
   mapping.
4. Memory read by an indirect-index mapping MUST remain immutable while any
   consuming shader invocation is in flight and MUST be synchronized for
   `VK_ACCESS_2_UNIFORM_READ_BIT`.
5. The standard path declares sampled images and samplers separately. A safe
   dynamic mapping MUST reject `OpTypeSampledImage` until both its resource and
   sampler calculations are represented explicitly; it MUST NOT manufacture
   a zeroed sampler mapping.

## FIFO latest-ready

The device-level feature is necessary but insufficient. A surface configuration
MUST also contain `VK_PRESENT_MODE_FIFO_LATEST_READY_KHR` in the present modes
returned for that physical-device/surface pair. `SurfacePresentCapabilities`
therefore never derives support from adapter state alone.

Present-mode selection is ordered and explicit. If FIFO is an acceptable
fallback, the preference list MUST contain `PresentMode::Fifo`.

## Surface and swapchain

1. `InstanceDescriptor::for_window` MUST derive only the platform extension
   required by the supplied handle. The current implementation accepts Wayland
   and MUST fail explicitly for every other handle kind.
2. `Instance::create_surface` MUST retain the host display/window lease through
   every `Surface` and `Swapchain` owner. It MUST NOT leave a Vulkan surface
   referencing a destroyed `wl_display` or `wl_surface`.
3. When `RequestAdapterOptions.compatible_surface` is present, adapter
   selection MUST choose a queue family supporting both graphics and present
   for that surface. Separate hidden present queues are not selected.
4. Surface configuration MUST validate the exact format/color-space pair,
   extent, image count, usage, transform, composite alpha, present mode, and
   enabled feature set immediately before swapchain creation.
5. Present preferences are caller ordered. FIFO latest-ready MUST NOT silently
   become FIFO; the caller explicitly lists both when either is acceptable.
6. The standard frame chain is acquire to binary semaphore, synchronization2
   submit waiting on that semaphore, timeline plus binary signal, then present
   waiting on the signalled binary semaphore. Queue submit and present MUST use
   the same host synchronization domain.
7. Reconfiguration MUST create the replacement with `oldSwapchain` and MUST
   retain the old swapchain until the replacement operation has returned.

## Offscreen presentation

Applications MUST acquire Vulkan capabilities through `vulkan-renderer`; they
MUST NOT select an independent `vulkanalia` version or create a parallel
instance/device ownership stack. `vulkan_renderer::vk` and the raw re-export
remain explicit migration/interop surfaces, not a reason to duplicate resource,
submission, retirement, or presentation implementations in a product.

1. `PresentationPathPlan` MUST permit a direct surface target only when the
   frame has exactly one physical pass, no post-write sampling, no history,
   no external consumer, no async-compute access, matching target/surface
   extent and format, and no required terminal transform. Forced direct mode
   MUST fail with the complete blocker set instead of weakening the graph.
2. Automatic mode MUST select an independently allocated offscreen target
   whenever any direct-surface condition is unmet. Explicit offscreen mode MAY
   be selected even when direct mode would be legal.
3. Offscreen presentation MUST allocate one single-sampled color-attachment +
   sampled image per in-flight frame slot through the shared image allocator.
   These images and views use shared RAII/timeline ownership; a product-local
   `vkAllocateMemory` path is not an alternative implementation.
4. All immutable offscreen sampled-image descriptors MUST share one resource
   heap, and every slot MUST reuse one sampler descriptor in one sampler heap.
   The selected filter, address and alpha policy remain explicit product input.
5. A late acquire is valid only when swapchain-independent offscreen work can
   be submitted first. The terminal composite is a separate submission after
   acquire. Same-queue order MAY carry the offscreen dependency; cross-queue
   execution MUST use an explicit timeline dependency.
6. `BeforeFrame` and `AfterOffscreenSubmit` are both supported policies.
   Neither is declared universally faster: a product MUST choose from measured
   acquire wait, submit overhead and GPU overlap, then verify a paired formal
   A/B without changing graph semantics.
7. A non-surface target extent is an explicit quality/render-scale policy. It
   MUST NOT silently replace the surface physical extent or be reported as
   native-resolution rendering.
8. Future transient memory aliasing MUST require non-overlapping first/last-use
   intervals plus equal format, sample count, usage compatibility and queue
   ownership. It MAY reuse backing memory but MUST NOT fuse passes or erase
   barriers, copies, swaps, descriptor identity, history or external liveness.

## Submission transaction

1. `Device::create_command_encoder` MUST allocate and begin exactly one primary
   command buffer. `CommandEncoder::finish` MUST consume the encoder and end
   recording exactly once.
2. Standard `Queue::submit` MUST consume every finished command buffer, so the
   caller cannot resubmit or prematurely free it.
3. A `FrameToken` MUST be allocated monotonically while the same host lock that
   serializes the queue submission is held. Concurrent callers MUST NOT be able
   to submit token N+1 before token N.
4. The backend signals its timeline semaphore with the token through
   `vkQueueSubmit2`.
5. Higher-level frame/resource state MUST be committed only after submission
   succeeds. Failed submission retains acquire/resource ownership for retry or
   explicit cancellation; its allocated timeline value MUST NOT be reused.
   `SubmissionLease` arguments are transferred by value and MUST enter
   retirement bookkeeping only after `vkQueueSubmit2` succeeds. A failed
   retained submission MUST release those arguments immediately.
6. A command buffer or resource MUST NOT be recycled or destroyed until the
   completed timeline is at least its retirement token. Command-pool allocate
   and free operations MUST be externally synchronized.
7. Submission of externally owned raw Vulkan command buffers MUST be an unsafe
   interoperability operation with an explicit lifetime contract.
8. `Queue::submit_retained` and its binary-signal form MUST hold every supplied
   lease until the completed timeline is at least the returned `FrameToken`.
   Completion polling and waiting MUST reclaim eligible leases. Retirement
   state MUST keep the logical device alive until leases are dropped, and MUST
   NOT be owned by the device in a way that forms a cycle with device-owned
   resources inside a lease.
9. `UploadBatch::submit_retained` MUST preserve the same transactional cursor
   rules as ordinary upload submission: staging cursors commit only after a
   successful queue submit and roll back on every earlier failure.
10. `CommandEncoder::retain`, `retain_lease`, and `retain_resource` MUST attach
    ownership to the recording transaction. `finish` MUST transfer those
    leases into its `CommandBuffer`; a successful managed `Queue` submission
    MUST transfer them into timeline retirement without requiring the caller
    to select a retained-submit overload. Dropping an unfinished, unsubmitted,
    validation-failed, or submit-failed managed command buffer MUST release its
    attached leases.
11. `SubmissionResource` MUST produce a lease which owns the concrete Vulkan
    resource through destruction. It MUST NOT be a marker detached from the
    object's actual RAII owner. Buffers, owned images, graphics/compute pipelines,
    retained decoder images and timelines, and imported/exported dma-buf images
    implement this contract.
12. Buffer update/copy and vertex/index binding MUST attach every referenced
    buffer. Image copy recording MUST attach owned source/destination images.
    Graphics and compute pipeline binding MUST attach the bound pipeline. Raw
    descriptor heap, attachment, and barrier bindings remain explicit lifetime
    obligations until their resource types use the same shared-ownership contract.

## Memory allocation

0. `DynamicBuffer` MUST allocate device-local memory, MUST add
   `TRANSFER_DST`, MUST reject non-four-byte uploads before recording Vulkan,
   and MUST use the caller's existing `UploadBatch`. It MUST NOT create a CPU
   drawing fallback or an implicit second queue submission. Unchanged content
   MAY skip a transfer only after comparing both byte length and content hash.

1. Small buffers SHOULD be suballocated from reusable blocks selected by
   memory location and exact Vulkan memory type. The implementation MUST NOT
   perform one `vkAllocateMemory` per ordinary buffer.
2. Device memory SHOULD prefer non-host-visible device-local heaps. Upload
   memory MUST be host visible and SHOULD be coherent. Readback memory MUST be
   host visible and SHOULD be cached.
3. Upload and readback blocks MUST remain persistently mapped. Non-coherent
   CPU writes MUST flush, and CPU reads MUST invalidate, ranges aligned to
   `nonCoherentAtomSize`.
4. Large allocations MAY use isolated blocks at a documented configurable
   threshold. Unused isolated and excess pooled blocks MUST be reclaimable
   without invalidating live resources.
5. Buffers requesting `SHADER_DEVICE_ADDRESS` MUST be allocated from memory
   with `DEVICE_ADDRESS` enabled and MUST fail creation if Vulkan returns zero.
6. Buffers, linear images, and optimal-tiled images MUST use distinct
   suballocation classes unless `bufferImageGranularity` separation is
   explicitly implemented. The standard allocator pools each class separately
   and honors required or preferred dedicated allocation metadata.

## Upload staging

1. An `UploadBelt` MUST use persistently mapped `MemoryLocation::Upload`
   buffers with `TRANSFER_SRC`; ordinary writes MUST NOT allocate or map one
   Vulkan memory object per upload.
2. Every staged range MUST satisfy the configured power-of-two offset
   alignment and Vulkan's four-byte buffer-copy alignment. Non-coherent writes
   MUST be flushed by the underlying allocator.
3. A touched staging chunk MUST NOT be reused until the submission timeline
   returned for its batch has completed. Abandoned or failed submissions MUST
   roll their cursor reservations back.
4. Both retained bytes and chunk count MUST have explicit hard bounds.
   Exhaustion MUST fail visibly instead of growing process memory without a
   bound or waiting for queue idle.
5. Upload copies and following rendering MAY share one command buffer. The
   caller remains responsible for exact synchronization2 transitions and
   texel/block packing; the implementation MUST NOT introduce CPU readback.
6. Trimming MUST destroy only completed chunks. Destroying an upload belt MUST
   wait for its greatest outstanding timeline value before releasing staging
   buffers.

## Texture upload layout

1. Texture copies MUST derive their footprint from an explicit texel-block
   layout. R8 and RGBA/BGRA formats use one-texel blocks; BC1 through BC7 use
   their exact 4x4 compressed block byte sizes.
2. Origin, extent, mip level, array layer, and row/image strides MUST be
   validated before staging allocation or command recording. Integer overflow,
   a stride smaller than one block row, and misaligned compressed origins are
   validation errors.
3. A compressed copy MAY end at a mip edge without block-multiple dimensions;
   every non-edge compressed extent MUST be block aligned.
4. Staging MUST reserve and copy only the required footprint. Padding after the
   final copied row or image MUST NOT be counted as required source data.
5. Image upload MUST remain a buffer-to-image GPU copy and MUST NOT invoke a
   CPU raster, format-conversion, or readback fallback.

## Render graph

Semantic `ResourceState` constructors MUST expand to their documented Vulkan
stage/access/layout tuple and MUST remain optional conveniences over the fully
explicit `buffer` and `image` constructors. Consumers MUST be able to mix
semantic and explicit states in one graph.

Each resource use specifies pipeline stages, access mask, image layout, and
queue-family ownership. The compiler MUST add an ordering edge for writes,
layout transitions, and ownership transfers; read/read uses in an identical
state MAY remain parallel. Duplicate pass IDs, duplicate uses of one resource
inside a pass, resource-kind changes, unknown dependencies, and cycles are
validation errors.

Before a pass, each abstract graph barrier MUST resolve to a live binding of
the same resource kind. Buffer resources produce `VkBufferMemoryBarrier2`;
image resources produce `VkImageMemoryBarrier2`, including the exact old/new
layouts and subresource range. Equal source/destination queue families MUST be
encoded as `VK_QUEUE_FAMILY_IGNORED`; distinct families MUST remain explicit.
The batch is recorded using `vkCmdPipelineBarrier2`, and an empty batch MUST
NOT emit a Vulkan command.

For Linux dma-buf resources, a fresh import enters with layout `UNDEFINED` and
queue family `VK_QUEUE_FAMILY_FOREIGN_EXT`. A successful first submission MAY
commit a preserved host-selected layout such as `GENERAL`; failed recording or
submission MUST NOT commit that state. Every compositor use MUST acquire from
FOREIGN before GPU access and release back to FOREIGN afterward. The graph MUST
preserve those queue-family indices rather than translating them to
`VK_QUEUE_FAMILY_IGNORED`.

## Shader, graphics, and compute pipelines

Applications embedding precompiled shaders SHOULD use
`vulkan_renderer::include_spirv!`. The macro includes a little-endian SPIR-V
asset as an aligned `&[u32]` and rejects byte lengths that are not multiples of
four at compile time. It is the standard asset-inclusion surface; consumers do
not need a direct `vulkanalia` dependency for this operation.

Authored Slang is a cold-path source format. Standard builds compile it through
`vulkan-renderer-build` with a pinned `slangc`, strict reflection contracts,
and external Vulkan 1.4 `spirv-val` validation. Applications distribute the
resulting SPIR-V, not Slang, LLVM, or a runtime shader compiler. Runtime shader
compilation is a separate opt-in product capability and MUST NOT enter the
default frame path.

1. Shader modules MUST reject malformed SPIR-V before calling Vulkan. The core
   accepts SPIR-V versions through 1.6 and MUST retain owned, aligned words for
   the duration of `vkCreateShaderModule`.
2. Descriptor set/binding mappings MUST be sorted canonically. Zero-sized,
   overflowing, and overlapping binding ranges MUST fail validation.
3. The mapping pNext chain MUST remain live for the complete pipeline-creation
   call and MUST NOT escape references to temporary mapping arrays.
4. Every standard graphics pipeline MUST use
   `VK_PIPELINE_CREATE_2_DESCRIPTOR_HEAP_BIT_EXT`. Consequently its
   `VkGraphicsPipelineCreateInfo.layout` MUST be `VK_NULL_HANDLE`; creating a
   legacy descriptor-set pipeline is not an implementation fallback.
5. Every standard graphics pipeline MUST use dynamic rendering with a null
   render pass and an explicit ordered color-format list plus depth/stencil
   formats. An inactive color slot is represented by `VK_FORMAT_UNDEFINED` and
   remains positionally significant.
6. Shader modules and pipeline caches from another logical device MUST be
   rejected before pipeline creation. Pipeline cache host access MUST be
   externally synchronized by the implementation.
7. Viewport and scissor are mandatory dynamic state. A standard command encoder
   MUST reject a draw until both have been set after a compatible pipeline is
   bound.
8. Every standard compute pipeline MUST also set
   `VK_PIPELINE_CREATE_2_DESCRIPTOR_HEAP_BIT_EXT`, use a null pipeline layout,
   and use the same canonical shader binding mappings as graphics stages.
9. Compute dispatch dimensions MUST be nonzero. Direct and indirect dispatch,
   direct and indirect draw, buffer update, and image-to-image copy MUST retain
   explicit unsafe lifetime and render-graph synchronization obligations.
10. Small shader parameters for descriptor-heap pipelines MUST use
   `vkCmdPushDataEXT`, not a hidden legacy pipeline layout. Offset and byte
   length MUST be four-byte aligned, non-empty, overflow checked, and bounded
   by the adapter's `maxPushDataSize`.
11. Descriptor-bearing Slang MUST be compiled with the
    `spvDescriptorHeapEXT` capability. Its SPIR-V MUST declare
    `DescriptorHeapEXT` and `SPV_EXT_descriptor_heap` and MUST NOT contain
    `Binding` or `DescriptorSet` decorations. Descriptor-free stages MUST NOT
    declare the heap extension.

## Linux dma-buf and explicit sync

1. `EXTERNAL_MEMORY_DMA_BUF` MUST require all four extensions in its feature
   row. In particular, dma-buf fd import without
   `VK_EXT_queue_family_foreign` is not the compositor interoperability
   capability defined by this standard.
2. Modifier support MUST be queried for the exact Vulkan format and image usage
   through `vkGetPhysicalDeviceImageFormatProperties2`. Extension-name
   presence MUST NOT imply that a modifier is importable or exportable.
3. An imported image MUST use
   `VkImageDrmFormatModifierExplicitCreateInfoEXT` and
   `VkExternalMemoryImageCreateInfo`. The import contract accepts one to four
   explicit DRM memory-plane layouts backed either by one fd or by one disjoint
   fd per plane. Implicit modifiers and plane-count mismatches fail before
   allocation.
4. The backend MUST duplicate an input dma-buf fd. Vulkan receives ownership of
   that duplicate only after successful `vkAllocateMemory`; failure paths MUST
   close it. The original caller fd is never consumed.
   Disjoint import MUST additionally require the modifier's `DISJOINT` format
   feature, query memory requirements separately with
   `VkImagePlaneMemoryRequirementsInfo`, and bind every allocation with the
   corresponding `MEMORY_PLANE_i_EXT` aspect.
5. Imported and exportable dma-buf memory MUST be a dedicated allocation and
   MUST NOT enter the ordinary image suballocator. Image view, image, and memory
   destruction MUST follow Vulkan lifetime order after GPU retirement.
6. Exportable images MUST use an explicit modifier list, query the
   actual selected modifier, expose every DRM memory-plane offset and row pitch,
   and return duplicated fds to host integration. Plane counts outside one to
   four are unsupported.
7. `SYNC_FD` import uses a temporary binary-semaphore payload. Vulkan consumes
   the fd only on successful import; failure MUST reclaim it. Export requires a
   semaphore created with the corresponding external handle type and a pending
   or completed signal operation.
8. Imported/exported images expose the same descriptor-heap view-create info,
   render-graph resource binding, and dynamic-rendering attachment contract as
   ordinary images. No descriptor-set or CPU-copy interop fallback exists.

## Decoder and host-owned Vulkan images

1. A decoder-owned `VkImage`, including an FFmpeg `AVVkFrame` image, MUST be
   retained through a host lease whose lifetime covers every created image
   view and GPU submission referencing it.
2. Construction is unsafe because the implementation cannot independently
   prove logical-device identity, raw-image metadata, or lease ownership. It
   MUST nevertheless validate format, extent, mip/layer counts, sample count,
   usage, and exact view subresources before calling Vulkan.
3. The renderer owns and destroys the `VkImageView` but MUST NOT destroy the
   external image or its memory. The view MUST be destroyed before the final
   host lease is released.
4. Plane views of multiplanar decoder images MAY use `PLANE_0`, `PLANE_1`, or
   other valid aspects and a selected array layer. They expose the same
   descriptor-heap metadata and render-graph bindings as owned images.
5. Retention MUST preserve zero-copy sampling. Creating a retained external
   view MUST NOT copy pixels, map image memory, or introduce CPU readback.
6. A decoder-owned timeline semaphore MAY be adapted through a retained host
   lease. The renderer MUST NOT destroy it. The raw handle MUST belong to the
   same logical device, wait values MUST be positive, and stage masks MUST be
   non-empty.
7. An FFmpeg frame lease SHOULD back both its retained image views and retained
   timeline semaphore. It MUST remain alive until all submissions waiting on
   or sampling that frame have completed.
8. Descriptor-heap-only sampling SHOULD retain view-create metadata without
   allocating a `VkImageView`. A real view SHOULD be materialized only for an
   API that consumes a view handle, such as dynamic rendering. Both forms MUST
   retain the same host image lease.

## Dynamic rendering

1. A rendering descriptor MUST contain a non-empty render area, a nonzero layer
   count, and at least one color, depth, or stencil attachment slot.
2. Every non-resolve attachment MUST belong to the encoder's device and use the
   same sample count. Undefined and preinitialized attachment layouts are
   invalid; the render graph is responsible for recording the declared layout
   transition before the scope.
3. A resolve target requires exactly one resolve mode, a multisampled source, a
   single-sampled destination, and equal formats. A resolve mode without a
   resolve target MUST fail validation.
4. Graphics pipeline binding MUST compare the full ordered color-format list,
   depth format, stencil format, and sample count with the active rendering
   scope. Compatibility MUST NOT be inferred from a render-pass object.
5. A rendering encoder exclusively borrows its command encoder. Ending or
   dropping the rendering encoder MUST record exactly one `vkCmdEndRendering`,
   and the parent command buffer cannot finish while the rendering scope lives.
6. Pipeline binding retains its shared pipeline owner automatically. Until
   command buffers also retain every attachment, descriptor heap, buffer, and
   indirect resource, commands that rely on those post-recording Vulkan
   lifetimes remain explicitly `unsafe`. A conforming safe layer MUST NOT hide
   that obligation.
