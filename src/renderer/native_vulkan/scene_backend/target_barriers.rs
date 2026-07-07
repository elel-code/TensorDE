//! Scene graph target barrier lowering for Vulkan synchronization2.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene_engine::{
    SceneGraphTarget, SceneGraphTargetBarrier, SceneGraphTargetBarrierReason, SceneGraphTargetUsage,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneTargetBarrierPlan {
    pub target: SceneGraphTarget,
    pub before_pass: usize,
    pub after_pass: usize,
    pub reason: SceneGraphTargetBarrierReason,
    pub previous_usage: SceneGraphTargetUsage,
    pub next_usage: SceneGraphTargetUsage,
    pub old_layout: &'static str,
    pub new_layout: &'static str,
    pub src_stage: &'static str,
    pub dst_stage: &'static str,
    pub src_access: &'static str,
    pub dst_access: &'static str,
    pub command_order: [&'static str; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneTargetBarrierImage {
    pub target: SceneGraphTarget,
    pub image: vk::Image,
}

#[derive(Debug, Clone, Copy)]
struct NativeVulkanSceneTargetUsageBarrier {
    stage: vk::PipelineStageFlags2,
    access: vk::AccessFlags2,
    layout: vk::ImageLayout,
    stage_label: &'static str,
    access_label: &'static str,
    layout_label: &'static str,
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_target_barrier_plan(
    barrier: &SceneGraphTargetBarrier,
) -> NativeVulkanSceneTargetBarrierPlan {
    let previous = native_vulkan_scene_target_usage_barrier(barrier.previous_usage);
    let next = native_vulkan_scene_target_usage_barrier(barrier.next_usage);
    NativeVulkanSceneTargetBarrierPlan {
        target: barrier.target,
        before_pass: barrier.before_pass,
        after_pass: barrier.after_pass,
        reason: barrier.reason,
        previous_usage: barrier.previous_usage,
        next_usage: barrier.next_usage,
        old_layout: previous.layout_label,
        new_layout: next.layout_label,
        src_stage: previous.stage_label,
        dst_stage: next.stage_label,
        src_access: previous.access_label,
        dst_access: next.access_label,
        command_order: [
            "map_scene_graph_target_usage_to_vk_sync2",
            "cmd_pipeline_barrier2_target_dependency",
        ],
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_target_barrier(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    barrier: &SceneGraphTargetBarrier,
    image: NativeVulkanSceneTargetBarrierImage,
) -> Result<NativeVulkanSceneTargetBarrierPlan, String> {
    if command_buffer == vk::CommandBuffer::null() {
        return Err("scene target barrier requires a valid command buffer".to_owned());
    }
    if image.image == vk::Image::null() {
        return Err(format!(
            "scene target barrier for {:?} requires a valid image",
            barrier.target
        ));
    }
    if image.target != barrier.target {
        return Err(format!(
            "scene target barrier image target mismatch: barrier {:?}, image {:?}",
            barrier.target, image.target
        ));
    }

    let previous = native_vulkan_scene_target_usage_barrier(barrier.previous_usage);
    let next = native_vulkan_scene_target_usage_barrier(barrier.next_usage);
    let image_barrier = vk::ImageMemoryBarrier2::builder()
        .src_stage_mask(previous.stage)
        .src_access_mask(previous.access)
        .dst_stage_mask(next.stage)
        .dst_access_mask(next.access)
        .old_layout(previous.layout)
        .new_layout(next.layout)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image.image)
        .subresource_range(scene_target_color_subresource_range())
        .build();
    let image_barriers = [image_barrier];
    let dependency = vk::DependencyInfo::builder()
        .image_memory_barriers(&image_barriers)
        .build();
    unsafe {
        device.cmd_pipeline_barrier2(command_buffer, &dependency);
    }

    Ok(native_vulkan_scene_target_barrier_plan(barrier))
}

fn native_vulkan_scene_target_usage_barrier(
    usage: SceneGraphTargetUsage,
) -> NativeVulkanSceneTargetUsageBarrier {
    match usage {
        SceneGraphTargetUsage::ShaderSampledRead => NativeVulkanSceneTargetUsageBarrier {
            stage: vk::PipelineStageFlags2::FRAGMENT_SHADER,
            access: vk::AccessFlags2::SHADER_SAMPLED_READ,
            layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            stage_label: "fragment-shader",
            access_label: "shader-sampled-read",
            layout_label: "shader-read-only-optimal",
        },
        SceneGraphTargetUsage::ColorAttachmentWrite => NativeVulkanSceneTargetUsageBarrier {
            stage: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            access: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            stage_label: "color-attachment-output",
            access_label: "color-attachment-write",
            layout_label: "color-attachment-optimal",
        },
        SceneGraphTargetUsage::Present => NativeVulkanSceneTargetUsageBarrier {
            stage: vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
            access: vk::AccessFlags2::empty(),
            layout: vk::ImageLayout::PRESENT_SRC_KHR,
            stage_label: "bottom-of-pipe",
            access_label: "none",
            layout_label: "present-src-khr",
        },
    }
}

fn scene_target_color_subresource_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_barrier_plan_maps_read_after_write_to_shader_sample() {
        let barrier = SceneGraphTargetBarrier {
            target: SceneGraphTarget::ImageLocalMain(0),
            before_pass: 0,
            after_pass: 1,
            previous_usage: SceneGraphTargetUsage::ColorAttachmentWrite,
            next_usage: SceneGraphTargetUsage::ShaderSampledRead,
            reason: SceneGraphTargetBarrierReason::ReadAfterWrite,
        };

        let plan = native_vulkan_scene_target_barrier_plan(&barrier);

        assert_eq!(plan.old_layout, "color-attachment-optimal");
        assert_eq!(plan.new_layout, "shader-read-only-optimal");
        assert_eq!(plan.src_stage, "color-attachment-output");
        assert_eq!(plan.dst_stage, "fragment-shader");
        assert_eq!(plan.src_access, "color-attachment-write");
        assert_eq!(plan.dst_access, "shader-sampled-read");
        assert_eq!(
            plan.command_order,
            [
                "map_scene_graph_target_usage_to_vk_sync2",
                "cmd_pipeline_barrier2_target_dependency"
            ]
        );
    }

    #[test]
    fn target_barrier_plan_maps_write_after_read_to_attachment_write() {
        let barrier = SceneGraphTargetBarrier {
            target: SceneGraphTarget::ImageLocalMain(0),
            before_pass: 1,
            after_pass: 2,
            previous_usage: SceneGraphTargetUsage::ShaderSampledRead,
            next_usage: SceneGraphTargetUsage::ColorAttachmentWrite,
            reason: SceneGraphTargetBarrierReason::WriteAfterRead,
        };

        let plan = native_vulkan_scene_target_barrier_plan(&barrier);

        assert_eq!(plan.old_layout, "shader-read-only-optimal");
        assert_eq!(plan.new_layout, "color-attachment-optimal");
        assert_eq!(plan.src_stage, "fragment-shader");
        assert_eq!(plan.dst_stage, "color-attachment-output");
    }
}
