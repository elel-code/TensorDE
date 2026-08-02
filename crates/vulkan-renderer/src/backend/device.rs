use vulkanalia::{Device, Instance, prelude::v1_4::*, vk};

use super::{Candidate, c_strings};
use crate::{Error, Features, Result};

pub(super) fn create_device(
    instance: &Instance,
    candidate: &Candidate,
    extension_names: &[String],
    required_features: Features,
) -> Result<Device> {
    let priorities = [1.0f32];
    let queue_infos = candidate
        .info
        .queues
        .unique_families()
        .into_iter()
        .map(|family| {
            vk::DeviceQueueCreateInfo::builder()
                .queue_family_index(family)
                .queue_priorities(&priorities)
                .build()
        })
        .collect::<Vec<_>>();
    let enabled_extension_names =
        enabled_device_extensions(candidate, extension_names, required_features);

    let extension_names = c_strings(&enabled_extension_names)?;
    let extension_pointers = extension_names
        .iter()
        .map(|name| name.as_ptr())
        .collect::<Vec<_>>();
    let core10 = vk::PhysicalDeviceFeatures::builder()
        .sampler_anisotropy(required_features.contains(Features::SAMPLER_ANISOTROPY))
        .sample_rate_shading(required_features.contains(Features::SAMPLE_RATE_SHADING))
        .texture_compression_bc(required_features.contains(Features::TEXTURE_COMPRESSION_BC))
        .build();
    let mut vulkan11 = vk::PhysicalDeviceVulkan11Features::builder()
        .shader_draw_parameters(required_features.contains(Features::SHADER_DRAW_PARAMETERS))
        .build();
    let mut vulkan12 = vk::PhysicalDeviceVulkan12Features::builder()
        .timeline_semaphore(true)
        .scalar_block_layout(required_features.contains(Features::SCALAR_BLOCK_LAYOUT))
        .descriptor_indexing(required_features.contains(Features::DESCRIPTOR_INDEXING))
        .runtime_descriptor_array(required_features.contains(Features::RUNTIME_DESCRIPTOR_ARRAY))
        .buffer_device_address(required_features.contains(Features::BUFFER_DEVICE_ADDRESS))
        .build();
    let mut vulkan13 = vk::PhysicalDeviceVulkan13Features::builder()
        .synchronization2(true)
        .dynamic_rendering(true)
        .shader_demote_to_helper_invocation(
            required_features.contains(Features::SHADER_DEMOTE_TO_HELPER_INVOCATION),
        )
        .maintenance4(required_features.contains(Features::MAINTENANCE4))
        .build();
    let mut vulkan14 = vk::PhysicalDeviceVulkan14Features::builder()
        .maintenance5(true)
        .maintenance6(required_features.contains(Features::MAINTENANCE6))
        .dynamic_rendering_local_read(
            required_features.contains(Features::DYNAMIC_RENDERING_LOCAL_READ),
        )
        .host_image_copy(required_features.contains(Features::HOST_IMAGE_COPY))
        .build();
    let mut descriptor_heap = vk::PhysicalDeviceDescriptorHeapFeaturesEXT::builder()
        .descriptor_heap(required_features.contains(Features::DESCRIPTOR_HEAP))
        .build();
    let mut fifo_latest_ready = vk::PhysicalDevicePresentModeFifoLatestReadyFeaturesKHR::builder()
        .present_mode_fifo_latest_ready(required_features.contains(Features::FIFO_LATEST_READY))
        .build();
    let mut pipeline_binary = vk::PhysicalDevicePipelineBinaryFeaturesKHR::builder()
        .pipeline_binaries(required_features.contains(Features::PIPELINE_BINARIES))
        .build();
    let mut shader_untyped_pointers = vk::PhysicalDeviceShaderUntypedPointersFeaturesKHR::builder()
        .shader_untyped_pointers(required_features.contains(Features::SHADER_UNTYPED_POINTERS))
        .build();
    let mut present_id2 = vk::PhysicalDevicePresentId2FeaturesKHR::builder()
        .present_id2(required_features.contains(Features::PRESENT_ID2))
        .build();
    let mut present_wait2 = vk::PhysicalDevicePresentWait2FeaturesKHR::builder()
        .present_wait2(required_features.contains(Features::PRESENT_WAIT2))
        .build();
    let mut swapchain_maintenance1 = vk::PhysicalDeviceSwapchainMaintenance1FeaturesKHR::builder()
        .swapchain_maintenance1(required_features.contains(Features::SWAPCHAIN_MAINTENANCE1))
        .build();
    let mut advanced_blend = vk::PhysicalDeviceBlendOperationAdvancedFeaturesEXT::builder()
        .advanced_blend_coherent_operations(
            required_features.contains(Features::ADVANCED_BLEND_COHERENT),
        )
        .build();
    let mut multisampled_render_to_single_sampled =
        vk::PhysicalDeviceMultisampledRenderToSingleSampledFeaturesEXT::builder()
            .multisampled_render_to_single_sampled(
                required_features.contains(Features::MULTISAMPLED_RENDER_TO_SINGLE_SAMPLED),
            )
            .build();
    let mut maintenance7 = vk::PhysicalDeviceMaintenance7FeaturesKHR::builder()
        .maintenance7(required_features.contains(Features::MAINTENANCE7))
        .build();
    let mut maintenance8 = vk::PhysicalDeviceMaintenance8FeaturesKHR::builder()
        .maintenance8(required_features.contains(Features::MAINTENANCE8))
        .build();
    let mut maintenance9 = vk::PhysicalDeviceMaintenance9FeaturesKHR::builder()
        .maintenance9(required_features.contains(Features::MAINTENANCE9))
        .build();
    let mut maintenance10 = vk::PhysicalDeviceMaintenance10FeaturesKHR::builder()
        .maintenance10(required_features.contains(Features::MAINTENANCE10))
        .build();
    let mut info = vk::DeviceCreateInfo::builder()
        .queue_create_infos(&queue_infos)
        .enabled_extension_names(&extension_pointers)
        .enabled_features(&core10)
        .push_next(&mut vulkan11)
        .push_next(&mut vulkan12)
        .push_next(&mut vulkan13)
        .push_next(&mut vulkan14);
    if extension_names
        .iter()
        .any(|name| name.as_bytes() == b"VK_EXT_descriptor_heap")
    {
        info = info.push_next(&mut descriptor_heap);
    }
    if extension_names
        .iter()
        .any(|name| name.as_bytes() == b"VK_KHR_present_mode_fifo_latest_ready")
    {
        info = info.push_next(&mut fifo_latest_ready);
    }
    if extension_names
        .iter()
        .any(|name| name.as_bytes() == b"VK_KHR_pipeline_binary")
    {
        info = info.push_next(&mut pipeline_binary);
    }
    if required_features.contains(Features::SHADER_UNTYPED_POINTERS) {
        info = info.push_next(&mut shader_untyped_pointers);
    }
    if required_features.contains(Features::PRESENT_ID2) {
        info = info.push_next(&mut present_id2);
    }
    if required_features.contains(Features::PRESENT_WAIT2) {
        info = info.push_next(&mut present_wait2);
    }
    if required_features.contains(Features::SWAPCHAIN_MAINTENANCE1) {
        info = info.push_next(&mut swapchain_maintenance1);
    }
    if required_features.contains(Features::ADVANCED_BLEND) {
        info = info.push_next(&mut advanced_blend);
    }
    if required_features.contains(Features::MULTISAMPLED_RENDER_TO_SINGLE_SAMPLED) {
        info = info.push_next(&mut multisampled_render_to_single_sampled);
    }
    if required_features.contains(Features::MAINTENANCE7) {
        info = info.push_next(&mut maintenance7);
    }
    if required_features.contains(Features::MAINTENANCE8) {
        info = info.push_next(&mut maintenance8);
    }
    if required_features.contains(Features::MAINTENANCE9) {
        info = info.push_next(&mut maintenance9);
    }
    if required_features.contains(Features::MAINTENANCE10) {
        info = info.push_next(&mut maintenance10);
    }
    unsafe { instance.create_device(candidate.handle, &info, None) }
        .map_err(|source| Error::vulkan("vkCreateDevice", source))
}

pub(super) fn enabled_device_extensions(
    candidate: &Candidate,
    extension_names: &[String],
    required_features: Features,
) -> Vec<String> {
    let mut enabled_extension_names = extension_names.to_vec();
    if required_features.contains(Features::DESCRIPTOR_HEAP)
        && candidate.info.extensions.contains("VK_KHR_maintenance5")
    {
        enabled_extension_names.push("VK_KHR_maintenance5".into());
        enabled_extension_names.sort();
        enabled_extension_names.dedup();
    }
    enabled_extension_names
}
