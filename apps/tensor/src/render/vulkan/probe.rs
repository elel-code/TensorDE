use tensor_host::{DrmFormat, Modifier};
use vulkan_renderer::{
    Adapter, DrmDeviceIdentity as RendererDrmDeviceIdentity, Instance as RendererInstance, vk,
};

use crate::render::{
    DescriptorHeapProperties, DeviceCandidate, DrmDeviceIdentity, DrmNodeId, VulkanFormatCapability,
};

use super::{OUTPUT_FORMATS, RendererError, native_image_usage};

pub(super) struct ProbedDevice {
    pub(super) adapter: Adapter,
    pub(super) candidate: DeviceCandidate,
    pub(super) formats: Vec<VulkanFormatCapability>,
}

/// Builds Tensor's value-only device-selection candidates from the shared
/// renderer's adapter/probe API. Tensor retains its DRM-node preference and
/// KMS policy, while Vulkan extension, sync-file, modifier, and descriptor
/// heap probing live in `vulkan-renderer`.
pub(super) fn probe_devices(
    instance: &RendererInstance,
) -> Result<Vec<ProbedDevice>, RendererError> {
    instance
        .enumerate_adapters()
        .map_err(|source| RendererError::Probe(source.to_string()))?
        .into_iter()
        .map(probe_adapter)
        .collect()
}

fn probe_adapter(adapter: Adapter) -> Result<ProbedDevice, RendererError> {
    let info = adapter.info().clone();
    let descriptor_heap = descriptor_heap_properties(info.limits.descriptor_heap);
    let descriptor_heap_supported = info.features.descriptor_heap;
    let buffer_device_address_supported = info.features.buffer_device_address;
    let timeline_semaphore_supported = info.features.timeline_semaphore;
    let dynamic_rendering_supported = info.features.dynamic_rendering;
    let maintenance5_supported = info.features.maintenance5;
    let graphics_queue_family = (info.queues.graphics != u32::MAX).then_some(info.queues.graphics);
    let drm = adapter.drm_device_identity().map(drm_device_identity);
    let interop = adapter.linux_dma_buf_capabilities();
    let formats = if descriptor_heap_supported
        && descriptor_heap.is_usable()
        && buffer_device_address_supported
        && timeline_semaphore_supported
        && dynamic_rendering_supported
        && maintenance5_supported
        && graphics_queue_family.is_some()
        && drm.and_then(DrmDeviceIdentity::node_pair).is_some()
        && interop.is_complete()
    {
        probe_format_capabilities(&adapter)?
    } else {
        Vec::new()
    };
    let native_output_format_count = formats
        .iter()
        .filter(|format| format.supports_output_export())
        .count();
    Ok(ProbedDevice {
        adapter,
        candidate: DeviceCandidate {
            ordinal: info.ordinal,
            name: info.name.clone(),
            device_type: info.device_type,
            api_version: info.api_version,
            descriptor_heap_supported,
            descriptor_heap,
            buffer_device_address_supported,
            timeline_semaphore_supported,
            dynamic_rendering_supported,
            maintenance5_supported,
            graphics_queue_family,
            drm,
            interop,
            native_output_format_count,
        },
        formats,
    })
}

fn drm_device_identity(identity: RendererDrmDeviceIdentity) -> DrmDeviceIdentity {
    DrmDeviceIdentity::new(
        identity
            .primary()
            .map(|node| DrmNodeId::new(node.major(), node.minor())),
        identity
            .render()
            .map(|node| DrmNodeId::new(node.major(), node.minor())),
    )
}

fn probe_format_capabilities(
    adapter: &Adapter,
) -> Result<Vec<VulkanFormatCapability>, RendererError> {
    let mut capabilities = Vec::new();
    for &(fourcc, vulkan_format) in OUTPUT_FORMATS {
        let modifiers = adapter
            .drm_format_modifier_capabilities(vulkan_format, native_image_usage())
            .map_err(|source| RendererError::ProbeFormat {
                format: fourcc,
                details: source.to_string(),
            })?;
        capabilities.extend(
            modifiers
                .into_iter()
                .filter(|modifier| {
                    modifier
                        .tiling_features
                        .contains(vk::FormatFeatureFlags2::COLOR_ATTACHMENT)
                })
                .map(|modifier| VulkanFormatCapability {
                    format: DrmFormat::new(fourcc, Modifier::from_raw(modifier.modifier)),
                    plane_count: modifier.plane_count,
                    renderable: true,
                    importable: modifier.importable,
                    exportable: modifier.exportable,
                }),
        );
    }
    Ok(capabilities)
}

fn descriptor_heap_properties(
    limits: vulkan_renderer::DescriptorHeapLimits,
) -> DescriptorHeapProperties {
    DescriptorHeapProperties {
        sampler_heap_alignment: limits.sampler_heap_alignment,
        resource_heap_alignment: limits.resource_heap_alignment,
        max_sampler_heap_size: limits.max_sampler_heap_size,
        max_resource_heap_size: limits.max_resource_heap_size,
        min_sampler_heap_reserved_range: limits.min_sampler_heap_reserved_range,
        min_sampler_heap_reserved_range_with_embedded: limits
            .min_sampler_heap_reserved_range_with_embedded,
        min_resource_heap_reserved_range: limits.min_resource_heap_reserved_range,
        sampler_descriptor_size: limits.sampler_descriptor_size,
        buffer_descriptor_alignment: limits.buffer_descriptor_alignment,
        image_descriptor_size: limits.image_descriptor_size,
        sampler_descriptor_alignment: limits.sampler_descriptor_alignment,
        image_descriptor_alignment: limits.image_descriptor_alignment,
        max_push_data_size: limits.max_push_data_size,
        max_descriptor_heap_embedded_samplers: limits.max_embedded_samplers,
    }
}
