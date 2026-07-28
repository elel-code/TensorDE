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
| `FIFO_LATEST_READY` | `VK_KHR_present_mode_fifo_latest_ready` | `presentModeFifoLatestReady` |

Adapter support and device enablement are separate bitsets. A feature MUST be
present in the adapter bitset and requested in `DeviceDescriptor` before the
device contract exposes it.

## Limit comparison

Maximum-capacity fields use `requested <= supported`. Alignment fields use
`supported <= requested` when the requested value is nonzero because a lower
required Vulkan alignment is more capable. A zero requested field means the
caller imposes no additional constraint beyond the selected profile.

For descriptor heaps, device creation additionally requires:

- every reported alignment is a nonzero power of two;
- sampler, image, and buffer descriptor sizes are nonzero;
- push-data size and embedded-sampler count are nonzero;
- aligned implementation-reserved prefixes leave nonzero sampler/resource
  payload ranges.

## FIFO latest-ready

The device-level feature is necessary but insufficient. A surface configuration
MUST also contain `VK_PRESENT_MODE_FIFO_LATEST_READY_KHR` in the present modes
returned for that physical-device/surface pair. `SurfacePresentCapabilities`
therefore never derives support from adapter state alone.

Present-mode selection is ordered and explicit. If FIFO is an acceptable
fallback, the preference list MUST contain `PresentMode::Fifo`.

## Submission transaction

1. A `FrameToken` is allocated monotonically.
2. Command buffers and wait semaphores are passed to `Queue::submit`.
3. The backend signals its timeline semaphore with the token through
   `vkQueueSubmit2`.
4. Higher-level frame/resource state MUST be committed only after submission
   succeeds. Failed submission retains acquire/resource ownership for retry or
   explicit cancellation.
5. A resource MUST NOT be recycled or destroyed until the completed timeline
   is at least its retirement token.

## Render graph

Each resource use specifies pipeline stages, access mask, image layout, and
queue-family ownership. The compiler MUST add an ordering edge for writes,
layout transitions, and ownership transfers; read/read uses in an identical
state MAY remain parallel. Duplicate pass IDs, duplicate uses of one resource
inside a pass, resource-kind changes, unknown dependencies, and cycles are
validation errors.
