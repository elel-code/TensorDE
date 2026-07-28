# vulkan-renderer

`vulkan-renderer` is a reusable Vulkan 1.4 backend foundation built directly
on `vulkanalia`. It has no dependency on Fika, Gilder, Tensor, wgpu, a window
system, or a scene format.

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
- implementation-reserved prefixes, including the embedded-sampler variant;
- sampler/image/buffer descriptor sizes and alignments;
- push-data size and embedded-sampler count.

A device request fails if the feature bit is absent, the extension is absent,
the requested limits exceed the adapter, an alignment is invalid, or no usable
payload remains after the reserved prefix. Extension-name presence alone is
not treated as support.

## FIFO latest-ready

`Features::FIFO_LATEST_READY` maps to
`VK_KHR_present_mode_fifo_latest_ready` and
`PhysicalDevicePresentModeFifoLatestReadyFeaturesKHR`.

Using `VK_PRESENT_MODE_FIFO_LATEST_READY_KHR` has three independent gates:

1. the adapter advertises the device extension;
2. the feature bit is supported and enabled at device creation;
3. `vkGetPhysicalDeviceSurfacePresentModesKHR` reports the mode for the
   concrete surface.

`SurfacePresentCapabilities::choose` checks all three and never invents an
implicit fallback. Include `PresentMode::Fifo` in the preference list when FIFO
fallback is acceptable.

## Submission and resource lifetime

The device owns a resettable graphics command pool and a timeline semaphore.
Every submission signals a monotonic `FrameToken`. Resource owners place
objects into `RetirementQueue<T>` and destroy them only after the completed
timeline reaches the token. Submission bookkeeping is committed only after
`vkQueueSubmit2` succeeds.

The render graph uses explicit resource states (stage mask, access mask, image
layout, queue family), derives write/layout/ownership dependencies, rejects
cycles, and emits Vulkan synchronization2 barrier plans without CPU readback.
