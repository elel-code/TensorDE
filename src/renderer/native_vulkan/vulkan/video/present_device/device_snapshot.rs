
pub(in crate::renderer::native_vulkan::vulkan) fn device_snapshot_from_selection(
    vulkan: &NativeVulkanVulkanaliaInstance,
    selection: &NativeVulkanVulkanaliaVideoPresentPhysicalDeviceSelection,
    context: &NativeVulkanVulkanaliaVideoPresentDeviceContext,
    codec: NativeVulkanVideoSessionCodec,
    swapchain: NativeVulkanVulkanaliaSwapchainSnapshot,
) -> NativeVulkanVulkanaliaVideoPresentDeviceProbeSnapshot {
    let same_queue_family =
        selection.video_queue_family_index == selection.present_queue_family_index;
    let same_queue_handle =
        same_queue_family && context.video_queue_index == context.present_queue_index;
    NativeVulkanVulkanaliaVideoPresentDeviceProbeSnapshot {
        binding: "vulkanalia",
        route: "video-present-device",
        loader: vulkan.loader_name.to_owned(),
        requested_api_version: Version::V1_4_0.to_string(),
        codec,
        physical_device_index: selection.physical_device_index,
        physical_device_name: selection
            .properties
            .device_name
            .to_string_lossy()
            .into_owned(),
        physical_device_type: format!("{:?}", selection.properties.device_type),
        api_version: Version::from(selection.properties.api_version).to_string(),
        vendor_id: selection.properties.vendor_id,
        device_id: selection.properties.device_id,
        driver_version: selection.properties.driver_version,
        single_logical_device_created: true,
        surface_host: None,
        enabled_device_extensions: context.enabled_device_extensions.clone(),
        video_enabled_device_extensions: context.video_enabled_device_extensions.clone(),
        present_enabled_device_extensions: context.present_enabled_device_extensions.clone(),
        feature_selection: feature_snapshot_from_context(context),
        video_queue: queue_snapshot(
            selection.video_queue_family_index,
            context.video_queue_index,
            selection.video_queue_count,
            selection.video_queue_flags,
            true,
            same_queue_family,
            same_queue_family && selection.present_supports_wayland,
        ),
        present_queue: queue_snapshot(
            selection.present_queue_family_index,
            context.present_queue_index,
            selection.present_queue_count,
            selection.present_queue_flags,
            selection
                .present_queue_flags
                .contains(vk::QueueFlags::VIDEO_DECODE_KHR),
            true,
            selection.present_supports_wayland,
        ),
        same_queue_family,
        same_queue_handle,
        queue_family_model: video_present_queue_family_model(same_queue_family, same_queue_handle),
        decoded_image_resource_sharing_model: decoded_image_resource_sharing_model(
            same_queue_family,
        ),
        swapchain,
        present_backend: "vulkanalia-single-device-video-decode-graphics-present",
        decoded_image_present_boundary: "same logical device now owns video-decode and graphics/present queues; next gate records decoded DPB/output image sampling into swapchain instead of clear placeholder",
        ffmpeg_reference: FFMPEG_VULKAN_DECODE_REFERENCE,
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn feature_snapshot_from_context(
    context: &NativeVulkanVulkanaliaVideoPresentDeviceContext,
) -> NativeVulkanVulkanaliaVideoPresentFeatureSnapshot {
    NativeVulkanVulkanaliaVideoPresentFeatureSnapshot {
        core_features: context.video_feature_selection.core_features,
        synchronization2_enabled: context.video_feature_selection.synchronization2_enabled
            && context.present_feature_selection.synchronization2_enabled,
        dynamic_rendering_enabled: context.video_feature_selection.dynamic_rendering_enabled,
        descriptor_heap_enabled: context
            .video_feature_selection
            .core_features
            .descriptor_heap,
        descriptor_heap_capture_replay_enabled: context
            .video_feature_selection
            .core_features
            .descriptor_heap_capture_replay,
        descriptor_heap_properties: context.video_feature_selection.descriptor_heap_properties,
        video_maintenance1_enabled: context.video_feature_selection.video_maintenance1_enabled,
        video_maintenance2_enabled: context.video_feature_selection.video_maintenance2_enabled,
        inline_session_parameters_enabled: context
            .video_feature_selection
            .inline_session_parameters_enabled,
        present_id2_enabled: context.present_feature_selection.present_id2_enabled,
        present_wait2_enabled: context.present_feature_selection.present_wait2_enabled,
        swapchain_maintenance1_enabled: context
            .present_feature_selection
            .swapchain_maintenance1_enabled,
        present_mode_fifo_latest_ready_enabled: context
            .present_feature_selection
            .present_mode_fifo_latest_ready_enabled,
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn swapchain_plan_snapshot(
    swapchain_plan: &super::swapchain::NativeVulkanVulkanaliaSwapchainPlan,
    image_count: usize,
) -> NativeVulkanVulkanaliaSwapchainSnapshot {
    NativeVulkanVulkanaliaSwapchainSnapshot {
        created: true,
        format: format!("{:?}", swapchain_plan.format.format),
        color_space: format!("{:?}", swapchain_plan.format.color_space),
        present_mode: present_mode_label(swapchain_plan.present_mode),
        extent: (swapchain_plan.extent.width, swapchain_plan.extent.height),
        extent_selection: swapchain_plan.extent_selection,
        image_count,
        min_image_count: swapchain_plan.image_count,
        composite_alpha: composite_alpha_label(swapchain_plan.composite_alpha),
        image_usage: vec!["transfer-src", "transfer-dst", "color-attachment"],
        create_flags: super::swapchain::swapchain_create_flag_labels(swapchain_plan.create_flags),
        present_id2_enabled: swapchain_plan.present_id2_enabled,
        present_wait2_enabled: swapchain_plan.present_wait2_enabled,
    }
}

fn queue_snapshot(
    queue_family_index: u32,
    queue_index: u32,
    queue_count: u32,
    queue_flags: vk::QueueFlags,
    supports_video_decode: bool,
    supports_present: bool,
    supports_wayland_presentation: bool,
) -> NativeVulkanVulkanaliaVideoPresentQueueSnapshot {
    NativeVulkanVulkanaliaVideoPresentQueueSnapshot {
        queue_family_index,
        queue_index,
        queue_count,
        queue_flags: queue_flag_labels(queue_flags),
        supports_video_decode,
        supports_graphics: queue_flags.contains(vk::QueueFlags::GRAPHICS),
        supports_present,
        supports_wayland_presentation,
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn video_present_queue_family_indices(
    video: u32,
    present: u32,
) -> Vec<u32> {
    if video == present {
        vec![video]
    } else {
        vec![video, present]
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn video_present_queue_indices(
    same_queue_family: bool,
    queue_count: u32,
) -> (u32, u32) {
    if same_queue_family && queue_count > 1 {
        (0, 1)
    } else {
        (0, 0)
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn video_present_queue_family_model(
    same_queue_family: bool,
    same_queue_handle: bool,
) -> &'static str {
    if same_queue_handle {
        "single-video-graphics-present-queue-family-single-queue"
    } else if same_queue_family {
        "single-video-graphics-present-queue-family-split-queue-indices"
    } else {
        "dedicated-video-decode-queue-plus-graphics-present-queue"
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn decoded_image_resource_sharing_model(
    same_queue_family: bool,
) -> &'static str {
    if same_queue_family {
        "exclusive-image-ownership-on-single-queue-family"
    } else {
        "concurrent-image-sharing-or-explicit-ownership-transfer-between-video-and-present-queue-families"
    }
}

fn dedup_static_extensions(
    extensions: impl IntoIterator<Item = &'static str>,
) -> Vec<&'static str> {
    let mut deduped = Vec::new();
    for extension in extensions {
        if !deduped.contains(&extension) {
            deduped.push(extension);
        }
    }
    deduped
}

#[cfg(test)]
mod tests;
