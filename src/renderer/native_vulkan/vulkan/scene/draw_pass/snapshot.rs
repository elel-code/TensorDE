use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

use super::super::scene_sampled_image::VulkanaliaSceneSampledImageResources;
use super::sync::{scene_color_image_barrier, scene_color_image_barriers};

pub(super) fn copy_scene_framebuffer_to_snapshot(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    swapchain_image: vk::Image,
    snapshot: &VulkanaliaSceneSampledImageResources,
    extent: vk::Extent2D,
    snapshot_old_layout: vk::ImageLayout,
) {
    let copy_src_dst_barriers = [
        scene_color_image_barrier(
            swapchain_image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags2::COLOR_ATTACHMENT_READ | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            vk::PipelineStageFlags2::ALL_TRANSFER,
            vk::AccessFlags2::TRANSFER_READ,
        ),
        scene_color_image_barrier(
            snapshot.image,
            snapshot_old_layout,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            if snapshot_old_layout == vk::ImageLayout::UNDEFINED {
                vk::PipelineStageFlags2::TOP_OF_PIPE
            } else {
                vk::PipelineStageFlags2::FRAGMENT_SHADER
            },
            if snapshot_old_layout == vk::ImageLayout::UNDEFINED {
                vk::AccessFlags2::empty()
            } else {
                vk::AccessFlags2::SHADER_SAMPLED_READ
            },
            vk::PipelineStageFlags2::ALL_TRANSFER,
            vk::AccessFlags2::TRANSFER_WRITE,
        ),
    ];
    scene_color_image_barriers(device, command_buffer, &copy_src_dst_barriers);

    let subresource = vk::ImageSubresourceLayers::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .mip_level(0)
        .base_array_layer(0)
        .layer_count(1)
        .build();
    let copy = vk::ImageCopy2::builder()
        .src_subresource(subresource)
        .src_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
        .dst_subresource(subresource)
        .dst_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
        .extent(vk::Extent3D {
            width: extent.width,
            height: extent.height,
            depth: 1,
        })
        .build();
    let regions = [copy];
    let copy_info = vk::CopyImageInfo2::builder()
        .src_image(swapchain_image)
        .src_image_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
        .dst_image(snapshot.image)
        .dst_image_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .regions(&regions)
        .build();
    unsafe {
        device.cmd_copy_image2(command_buffer, &copy_info);
    }

    let shader_read_resume_barriers = [
        scene_color_image_barrier(
            snapshot.image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            vk::PipelineStageFlags2::ALL_TRANSFER,
            vk::AccessFlags2::TRANSFER_WRITE,
            vk::PipelineStageFlags2::FRAGMENT_SHADER,
            vk::AccessFlags2::SHADER_SAMPLED_READ,
        ),
        scene_color_image_barrier(
            swapchain_image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::PipelineStageFlags2::ALL_TRANSFER,
            vk::AccessFlags2::TRANSFER_READ,
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags2::COLOR_ATTACHMENT_READ | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
        ),
    ];
    scene_color_image_barriers(device, command_buffer, &shader_read_resume_barriers);
}
