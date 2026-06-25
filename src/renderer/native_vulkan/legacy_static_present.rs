use ash::vk;

use super::present::native_vulkan_color_subresource_range;
use super::static_image_upload::NativeVulkanStaticImageUpload;
use super::{NativeVulkanClearColor, NativeVulkanError};

pub(super) const NATIVE_VULKAN_LEGACY_STATIC_RENDERER_STATUS: &str =
    "legacy-ash-cpu-fit-staging-copy";

pub(super) fn native_vulkan_record_legacy_static_frame(
    device: &ash::Device,
    command_buffer: vk::CommandBuffer,
    image: vk::Image,
    old_layout: vk::ImageLayout,
    static_upload: Option<&NativeVulkanStaticImageUpload>,
    clear_color: NativeVulkanClearColor,
) -> Result<vk::ImageLayout, NativeVulkanError> {
    let range = native_vulkan_color_subresource_range();
    let to_transfer = vk::ImageMemoryBarrier::default()
        .old_layout(old_layout)
        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(range)
        .src_access_mask(vk::AccessFlags::empty())
        .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);
    unsafe {
        device.cmd_pipeline_barrier(
            command_buffer,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[to_transfer],
        );
    }

    if let Some(static_upload) = static_upload {
        let copy = static_upload.buffer_image_copy;
        unsafe {
            device.cmd_copy_buffer_to_image(
                command_buffer,
                static_upload.buffer,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[copy],
            );
        }
    } else {
        let clear_color = vk::ClearColorValue::from(clear_color);
        unsafe {
            device.cmd_clear_color_image(
                command_buffer,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &clear_color,
                &[range],
            );
        }
    }

    let to_present = vk::ImageMemoryBarrier::default()
        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(range)
        .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
        .dst_access_mask(vk::AccessFlags::empty());
    unsafe {
        device.cmd_pipeline_barrier(
            command_buffer,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::BOTTOM_OF_PIPE,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[to_present],
        );
    }

    Ok(vk::ImageLayout::PRESENT_SRC_KHR)
}

pub(super) fn native_vulkan_legacy_static_wait_stage() -> vk::PipelineStageFlags {
    vk::PipelineStageFlags::TRANSFER
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_static_status_names_ash_copy_boundary() {
        assert_eq!(
            NATIVE_VULKAN_LEGACY_STATIC_RENDERER_STATUS,
            "legacy-ash-cpu-fit-staging-copy"
        );
    }

    #[test]
    fn legacy_static_present_waits_on_transfer_stage() {
        assert_eq!(
            native_vulkan_legacy_static_wait_stage(),
            vk::PipelineStageFlags::TRANSFER
        );
    }
}
