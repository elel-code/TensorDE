//! Retained main scene-color multisample attachments and resolve policy.
//!
//! WE renders the shared scene parent target at 4x. Effect targets keep their
//! own typed sample policy and are not promoted by this module.

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::renderer::native_vulkan::{
    NativeVulkanVulkanaliaImage,
    native_vulkan_vulkanalia_create_multisampled_color_attachment_image,
    native_vulkan_vulkanalia_destroy_image,
};

pub(super) fn create_scene_color_msaa_targets(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    format: vk::Format,
    extent: vk::Extent2D,
    frame_slot_count: usize,
    scene_color_msaa_enabled: bool,
    multisampled_render_to_single_sampled_enabled: bool,
) -> Result<Vec<NativeVulkanVulkanaliaImage>, String> {
    if !scene_color_msaa_enabled || multisampled_render_to_single_sampled_enabled {
        return Ok(Vec::new());
    }
    let mut targets = Vec::with_capacity(frame_slot_count);
    for _ in 0..frame_slot_count {
        match native_vulkan_vulkanalia_create_multisampled_color_attachment_image(
            device,
            memory_properties,
            "scene-color-msaa-frame-attachment",
            format,
            extent.width,
            extent.height,
            vk::SampleCountFlags::_4,
        ) {
            Ok(target) => targets.push(target),
            Err(err) => {
                destroy_scene_color_msaa_targets(device, targets);
                return Err(err);
            }
        }
    }
    Ok(targets)
}

pub(super) fn destroy_scene_color_msaa_targets(
    device: &Device,
    targets: Vec<NativeVulkanVulkanaliaImage>,
) {
    for target in targets {
        native_vulkan_vulkanalia_destroy_image(device, target);
    }
}

pub(super) fn scene_color_msaa_memory_bytes(targets: &[NativeVulkanVulkanaliaImage]) -> u64 {
    targets
        .iter()
        .map(|target| target.snapshot.memory_size)
        .sum()
}
