use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

pub(super) struct SceneColorImageBarrierBatch {
    barriers: [vk::ImageMemoryBarrier2; 4],
    len: usize,
}

impl SceneColorImageBarrierBatch {
    pub(super) fn new() -> Self {
        Self {
            barriers: [vk::ImageMemoryBarrier2::default(); 4],
            len: 0,
        }
    }

    pub(super) fn push(&mut self, barrier: vk::ImageMemoryBarrier2) {
        debug_assert!(self.len < self.barriers.len());
        if self.len < self.barriers.len() {
            self.barriers[self.len] = barrier;
            self.len += 1;
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub(super) fn as_slice(&self) -> &[vk::ImageMemoryBarrier2] {
        &self.barriers[..self.len]
    }
}

pub(super) fn scene_color_image_barrier(
    image: vk::Image,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
    src_stage_mask: vk::PipelineStageFlags2,
    src_access_mask: vk::AccessFlags2,
    dst_stage_mask: vk::PipelineStageFlags2,
    dst_access_mask: vk::AccessFlags2,
) -> vk::ImageMemoryBarrier2 {
    vk::ImageMemoryBarrier2::builder()
        .src_stage_mask(src_stage_mask)
        .src_access_mask(src_access_mask)
        .dst_stage_mask(dst_stage_mask)
        .dst_access_mask(dst_access_mask)
        .old_layout(old_layout)
        .new_layout(new_layout)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(native_vulkan_vulkanalia_scene_color_subresource_range())
        .build()
}

pub(super) fn scene_color_image_barriers(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    barriers: &[vk::ImageMemoryBarrier2],
) {
    let dependency = vk::DependencyInfo::builder()
        .image_memory_barriers(barriers)
        .build();
    unsafe {
        device.cmd_pipeline_barrier2(command_buffer, &dependency);
    }
}

pub(super) fn scene_color_image_transition(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    image: vk::Image,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
    src_stage_mask: vk::PipelineStageFlags2,
    src_access_mask: vk::AccessFlags2,
    dst_stage_mask: vk::PipelineStageFlags2,
    dst_access_mask: vk::AccessFlags2,
) {
    let barriers = [scene_color_image_barrier(
        image,
        old_layout,
        new_layout,
        src_stage_mask,
        src_access_mask,
        dst_stage_mask,
        dst_access_mask,
    )];
    scene_color_image_barriers(device, command_buffer, &barriers);
}

pub(super) fn native_vulkan_vulkanalia_scene_color_subresource_range() -> vk::ImageSubresourceRange
{
    vk::ImageSubresourceRange::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1)
        .build()
}
