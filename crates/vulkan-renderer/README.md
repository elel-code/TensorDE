# vulkan-renderer

`vulkan-renderer` is a standalone Vulkan 1.4 backend foundation built directly
on `vulkanalia`. It has no dependency on a consumer project, wgpu, a scene
format, or a CPU rasterizer. Its manifest uses no parent-workspace dependency
inheritance, so the crate directory can move into an independent repository.

The crate implements its own [native rendering standard](docs/rendering-standard.md).
It uses WebGPU's strict discovery/descriptor/validation experience as a base,
then deliberately exposes native descriptor heaps, explicit synchronization,
timeline lifetime, memory residency, and Vulkan 1.4/2026 capabilities that a
browser portability contract cannot express.

The public model follows WebGPU/wgpu's strict separation of discovery and
enablement:

```text
InstanceDescriptor -> Instance -> RequestAdapterOptions -> Adapter
Adapter + DeviceDescriptor -> Device + Queue
```

- `Adapter::features()` and `Adapter::limits()` report physical-device support.
- `DeviceDescriptor.required_features` is a hard requirement. Missing bits
  fail the request; they are never silently disabled.
- `DeviceDescriptor.required_limits` is checked before `vkCreateDevice`.
- `Device::features()` reports the enabled contract, not every adapter feature.
- `Queue` owns the logical-device lifetime independently, like `wgpu::Queue`.
- `Backend::new` is only a convenience path over the same validation rules.

The default device contract requires and enables `VK_EXT_descriptor_heap` and
`VK_KHR_present_mode_fifo_latest_ready`. Applications that construct a custom
descriptor may request a stricter superset, but the standard constructors do
not silently fall back to legacy descriptor sets or another present policy.

## Profiles

| Profile | API floor | Validation |
|---|---:|---|
| `Vulkan14` | 1.4.0 | timeline semaphore, synchronization2, dynamic rendering, maintenance5, graphics queue, requested features and limits |
| `Roadmap2026` | 1.4.328 | exact `VP_KHR_roadmap_2026` revision 11 extension, core-feature, extension-feature, and property requirements from Khronos' 2026-01-28 profile |

The 2026 profile additionally enables FIFO latest-ready when creating its
logical device. Platform surface extensions such as
`VK_KHR_wayland_surface` remain an explicit requirement of the embedding
application.

## `VK_EXT_descriptor_heap`

`Features::DESCRIPTOR_HEAP` maps to both the extension name and
`PhysicalDeviceDescriptorHeapFeaturesEXT::descriptor_heap`. Adapter probing
also records the complete heap property block:

- sampler/resource heap alignments and maximum sizes;
- implementation-reserved ranges, including the embedded-sampler variant;
- sampler/image/buffer descriptor sizes and alignments;
- push-data size and embedded-sampler count.

A device request fails if the feature bit is absent, the extension is absent,
the requested limits exceed the adapter, an alignment is invalid, or no usable
payload remains beside the aligned reserved range. Extension-name presence alone is
not treated as support.

`Device::create_descriptor_heap` creates a host-visible heap for cold tables.
`Device::create_descriptor_heap_with_memory` additionally offers explicit
`HostVisible` and `DeviceLocal` placement: a device-local heap retains one
persistently mapped staging buffer, while shader descriptor reads stay in the
target device-local buffer. The application descriptor region comes first and
the aligned implementation-reserved range is appended; both values are copied
into `VkBindHeapInfoEXT`.

Resource and sampler descriptor types are size/alignment checked against the
queried properties before `vkWriteResourceDescriptorsEXT` or
`vkWriteSamplerDescriptorsEXT`. Non-coherent writes flush atom-aligned ranges.
For a device-local heap, reusable `DescriptorHeapUploadBatch` scratch records
the host-write visibility barrier, compact staged copies, the transfer-to-heap
read barrier, and the resource/sampler heap bind in one composable command
sequence. Host-visible heaps use the same recording API but skip the copy.
`CommandEncoder::bind_descriptor_heap` dispatches to the resource or sampler
heap command without a legacy descriptor-set path.

`SampledImageDescriptor` is the typed batched descriptor source for owned
`ImageView` objects and imported/exported dma-buf images. It keeps
`VkImageViewCreateInfo` construction inside the renderer; callers provide
retained renderer resources plus an explicit `TextureLayout` instead of raw
descriptor image structs.

`DescriptorHeap::allocator` returns a cloneable handle to the heap's single
allocation state. Products may keep their own frame or render-graph policy,
but ranges remain owned by the actual heap and must be released before use or
retired against the shared device timeline after submission. This permits
independent single-pass and multi-pass clients to compose allocation policy
without inventing parallel offset or retirement namespaces.

Sampled images, storage images, input attachments, uniform/storage buffers,
and samplers use the corresponding descriptor-heap representations. Small
graphics/compute parameters use bounded `vkCmdPushDataEXT`; standard pipelines
never reintroduce a legacy pipeline layout merely to push constants.

`SampledTextureBinding` is the standard path for a separately declared SPIR-V
sampled image plus sampler. It atomically allocates/writes the resource and
sampler heap descriptors for an `ImageView`, produces a checked
`ShaderBindingMap` from explicit set/binding locations, and has explicit
`release` (never submitted) and `retire` (timeline-safe reuse) transitions.
The image view implements `SubmissionResource`, so command recording can retain
both the `VkImageView` and its parent image without an application-owned
per-frame lifetime list.
`SampledTextureShaderBindings::push_index_shader_binding_map` instead keeps the
pipeline independent of concrete heap slots; `SampledTextureBinding` supplies
the checked image/sampler byte offsets to write into push data after an atlas
replacement.

Shader modules accept owned, structurally validated SPIR-V 1.0–1.6 words.
`ShaderBindingMap` canonicalizes set/binding ranges and rejects empty,
overflowing, or overlapping mappings before it constructs the borrowed
`VkShaderDescriptorSetAndBindingMappingInfoEXT` chain. Its tagged
`ShaderBindingSource` exposes constant-offset, push-index, and indirect-index
heap addressing without exposing Vulkan's untagged union. Dynamic sources
enforce their 4/8-byte offset rules immediately; pipeline creation additionally
checks `maxPushDataSize` and descriptor-class alignment against the selected
adapter. Graphics pipeline creation always sets
`VK_PIPELINE_CREATE_2_DESCRIPTOR_HEAP_BIT_EXT`, always uses a null pipeline
layout, and always uses dynamic rendering. Pipeline caches are host-synchronized
and checked against the creating device.

`CommandEncoder::begin_rendering` creates a borrowed rendering scope whose drop
records `vkCmdEndRendering`. Attachment ownership, layouts, resolve contracts,
formats, and sample counts are validated before recording. Pipeline binding
then requires an exact match with the active rendering scope; viewport and
scissor are mandatory dynamic state before drawing. Resource lifetime and
render-graph state transitions remain explicit obligations; callers can attach
type-erased ownership directly to a submission instead of maintaining a
separate per-project frame-retirement queue.
The primitives do not impose a fixed frame template: one encoder may compose
upload, transfer, graphics, compute, external-image, and presentation work in
the order declared by the consumer's render graph.

## FIFO latest-ready

`Features::FIFO_LATEST_READY` maps to
`VK_KHR_present_mode_fifo_latest_ready` and
`PhysicalDevicePresentModeFifoLatestReadyFeaturesKHR`.
It also enables the required `VK_KHR_swapchain` device extension; the standard
instance contract enables `VK_KHR_surface`, while the embedding project adds
its platform surface extension.

Using `VK_PRESENT_MODE_FIFO_LATEST_READY_KHR` has three independent gates:

1. the adapter advertises the device extension;
2. the feature bit is supported and enabled at device creation;
3. `vkGetPhysicalDeviceSurfacePresentModesKHR` reports the mode for the
   concrete surface.

`SurfacePresentCapabilities::choose` checks all three and never invents an
implicit fallback. Include `PresentMode::Fifo` in the preference list when FIFO
fallback is acceptable.

Wayland surfaces are created directly from raw-window-handle 0.6 with
`vkCreateWaylandSurfaceKHR`; no `vulkanalia/window`, Cocoa, or Metal support
package is pulled into this Linux backend. `Surface` retains the supplied host
lease, compatible-adapter selection requires a graphics/present queue, and
swapchain configuration validates the complete surface capability snapshot.
Acquire, synchronization2 submit, timeline/binary signal, and present are
exposed as one explicit chain with queue-wide host synchronization.
Direct single-pass and offscreen multi-pass presentation are peer modes; the
renderer imposes no architectural primary path. Target choice, acquire timing,
and terminal policy are separate, explicit inputs. Products select direct,
offscreen, or automatic fact-based resolution for their own workload.
Acquire, command encoding, barriers, queue submission, timeline retirement and
present remain independently composable shared primitives;
`PresentationTransaction` is an optional retained orchestration layer.
For multi-pass rendering, `PresentationTransaction` can submit retained
offscreen work before a policy-driven late acquire, then records the terminal
surface command buffer with exact `ATTACHMENT_OPTIMAL` and `PRESENT_SRC_KHR`
transitions. The terminal shader writes the acquired swapchain image directly;
no final copy or blit is inserted. Acquire semaphores are frame-slot owned,
while render-finished semaphores are swapchain-image owned for safe WSI reuse.

Region-local dependencies can use `RetainedColorTargetPool` independently of
surface presentation. The pool has explicit target-count, byte, and extent
limits; keys contain only extent, format, and image usage. A target remains
busy until its retirement timeline completes. Matching retired targets are
reused, pressure evicts only retired least-recently-used entries, and a fully
busy pool fails before invoking the image allocator. Acquisition returns an
explicit reservation: successful submission retires it at the frame timeline,
while abandoned recording releases it immediately without inventing GPU work.
Fixed-size batch acquisition is rollback-safe: if a later lane cannot be
reserved, every earlier lane is released, and batch retire/release validates
all tokens before mutating any entry.
`CommandEncoder::copy_exported_color_image_to_image` provides the matching
typed external/output-to-retained region copy without exposing Vulkan handles
or product graph semantics. Its WSI peer,
`copy_surface_color_image_to_image`, validates the acquired swapchain usage and
supports the same bounded dependency for direct-surface products.
`copy_color_image_to_buffer` completes the typed
retained-image-to-readback leg; it retains both operands through submission,
while `Buffer::read` remains valid only after the caller has observed the
submission timeline. The shared API contains no compositor capture/session
types and is equally usable by UI export, diagnostics, and scene products.

## Submission and resource lifetime

The device owns a resettable graphics command pool and a timeline semaphore.
`Device::create_command_encoder` begins a primary one-time command buffer;
`CommandEncoder::finish` is the only transition to the executable state.
`Queue::submit` consumes finished buffers, signals a monotonic `FrameToken`,
and recycles their Vulkan handles only after the completed timeline reaches
that token. The recycler retains at most 64 primary buffers; excess buffers
are freed. Command-pool allocation and reclamation are host-synchronized, and
timeline allocation plus `vkQueueSubmit2` form one serialized transaction so
concurrent callers cannot submit value N+1 before value N. Raw command-buffer
submission remains available only as an explicitly unsafe interoperability
path.

Other resource owners place objects into `RetirementQueue<T>` and destroy them
only after the completed timeline reaches the token. Submission bookkeeping is
committed only after `vkQueueSubmit2` succeeds. `Queue::submit_retained` and the
matching upload/binary-signal forms accept `SubmissionLease` values for decoder
frames, dma-buf owners, transient resources, or any other `Arc<T: Send + Sync>`.
Leases are installed only after successful submission,
are reclaimed by `completed_timeline` or `wait_for`, and keep the logical device
alive until resource destruction has completed without making the device own a
cyclic retirement queue.

`CommandEncoder::retain` and `retain_resource` move that ownership into the
finished command buffer, so ordinary `Queue::submit` performs the same timeline
retirement automatically. `SubmissionResource` is implemented by buffers, owned images,
graphics/compute pipelines, retained decoder images/views and timelines, and
imported/exported dma-buf images. Image copy and pipeline binding register their
owned operands automatically. Buffer update/copy and vertex/index binding also
register their buffers automatically, so growing a persistent geometry buffer
cannot destroy storage still referenced by an older frame. Descriptor heaps,
borrowed attachments, and raw render-graph bindings remain explicit because
their mutation and host-ownership contracts are application-defined.

`MemoryAllocator` suballocates buffers from reusable blocks keyed by memory
location and Vulkan memory type. Device-local blocks default to 64 MiB;
persistently mapped upload/readback blocks default to 16 MiB; allocations at
or above the configurable 32 MiB threshold use isolated blocks. Upload writes
flush non-coherent atom-aligned ranges, readback invalidates them, and `trim`
releases unused dedicated/excess pooled blocks. Buffer, linear-image, and
optimal-image memory classes are kept separate, so `bufferImageGranularity`
cannot be violated by accidental mixed suballocation. Images and image views
use shared RAII ownership, and dynamic-rendering compatibility can query their
format and sample count without raw Vulkan calls.
`Buffer::write_with` exposes a bounded, persistently mapped upload slice to a
host producer and performs the required non-coherent flush without exposing a
device-memory handle.

`UploadBelt` replaces repeated queue-side write allocations with bounded,
persistently mapped staging chunks. A batch can stage buffer and image copies,
record subsequent barriers/rendering into the same encoder, and submit once.
Touched chunks are reused only after the returned timeline completes; failed
or abandoned batches roll their cursors back. A cold producer whose complete
resource set exceeds that bound can explicitly `flush_for_reuse` without
blocking, then call `wait_for_oldest_reuse` only if a later reservation still
has no capacity; neither path uses queue/device idle. The default policy
retains at most eight chunks and 32 MiB, and `trim` releases completed excess
chunks.

`UploadBatch::write_image_data` validates texel-block geometry, mip/array/3D
footprints, row and image strides, compressed-format edge blocks, and exact
source lengths for R8, RGBA/BGRA, and BC1-BC7 uploads. The upload remains a GPU
buffer-to-image copy; no CPU raster or conversion path is present.

The render graph uses explicit resource states (stage mask, access mask, image
layout, queue family), derives write/layout/ownership dependencies, rejects
cycles, and resolves abstract resources into owned
`VkBufferMemoryBarrier2`/`VkImageMemoryBarrier2` batches. Recording executes
one `vkCmdPipelineBarrier2` at the corresponding pass boundary without CPU
readback. Equal queue families are encoded as `VK_QUEUE_FAMILY_IGNORED`;
different families remain explicit ownership transfers.
For dynamic compositor-style streams, reusable `BarrierBatch` scratch can add
the same typed image transition directly from a retained `ResourceBinding` and
two `ResourceState` values, without allocating a graph or exposing raw
synchronization2 flags.

## Linux compositor interop

`Features::EXTERNAL_MEMORY_DMA_BUF` is a complete compositor capability, not a
single extension alias. It requires external-memory fd, dma-buf, explicit DRM
modifier, and FOREIGN queue-family extensions. Adapter/device modifier queries
validate the exact Vulkan format and image usage and report per-modifier plane
count, tiling features, importability, and exportability.

`Device::import_dma_buf_image` accepts one to four explicit memory planes in a
single fd, duplicates that fd, creates a dedicated imported allocation, and
exposes descriptor-heap, render-graph, and attachment views.
`Device::import_disjoint_dma_buf_image` handles separate per-plane fds with
per-plane memory-requirement queries and `MEMORY_PLANE_i_EXT` bindings when the
modifier advertises `DISJOINT` support.
`Device::create_exportable_dma_buf_image`
creates a dedicated output image from an explicit modifier list, reports the
driver-selected modifier and every memory-plane layout, and duplicates its
dma-buf fd for Wayland or DRM ownership transfer. Neither object enters the
ordinary suballocator.

`Features::EXTERNAL_SEMAPHORE_SYNC_FD` separately verifies importable and
exportable Linux `SYNC_FD` support. Temporary semaphore import, semaphore fd
export, FOREIGN acquire/release barriers, and timeline retirement form the
standard external-compute compositor path without a CPU copy or descriptor-set
fallback.

`Device::retain_external_image` adapts decoder/host-owned images from the same
logical device, including FFmpeg `AVVkFrame` multiplanar images. Selected
plane/layer metadata can be written directly to descriptor heaps without
allocating a `VkImageView`; a real retained view is materialized only when an
API such as dynamic rendering needs its handle. Both forms retain the supplied
host lease and never copy or map decoded pixels.

`Device::retain_external_timeline_semaphore` similarly adapts the
`AVVkFrame.sem[]` timeline semaphore without taking destruction ownership. Its
wait descriptors preserve FFmpeg's exact timeline value and Vulkan stage mask;
the same retained AVFrame owner can back both image and semaphore objects.
