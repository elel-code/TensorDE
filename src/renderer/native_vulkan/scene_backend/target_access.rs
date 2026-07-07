//! Shared scene target layout access helpers for graph-recorded Vulkan work.
//!
//! References:
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;
use vulkanalia::vk::Handle;

use crate::engine::scene_engine::SceneGraphTarget;

use super::frame_resources::NativeVulkanSceneFrameResources;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneTargetTransitionPlan {
    pub target: SceneGraphTarget,
    pub old_layout: &'static str,
    pub new_layout: &'static str,
    pub src_stage: &'static str,
    pub dst_stage: &'static str,
    pub src_access: &'static str,
    pub dst_access: &'static str,
    pub reason: &'static str,
    pub command_order: [&'static str; 2],
}

#[derive(Debug, Clone, Copy)]
struct NativeVulkanSceneTargetLayoutAccess {
    stage: vk::PipelineStageFlags2,
    access: vk::AccessFlags2,
    stage_label: &'static str,
    access_label: &'static str,
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_target_transition(
    frame_resources: &mut NativeVulkanSceneFrameResources,
    device: &Device,
    command_buffer: vk::CommandBuffer,
    target: SceneGraphTarget,
    new_layout: vk::ImageLayout,
    reason: &'static str,
) -> Result<Option<NativeVulkanSceneTargetTransitionPlan>, String> {
    if target == SceneGraphTarget::Swapchain {
        return Err(
            "scene target transition cannot directly mutate swapchain from offscreen graph helper"
                .to_owned(),
        );
    }
    if command_buffer == vk::CommandBuffer::null() {
        return Err("scene target transition requires a valid command buffer".to_owned());
    }
    let binding = frame_resources.offscreen_target_binding(target)?;
    if binding.image == vk::Image::null() {
        return Err(format!(
            "scene target transition for {target:?} requires a valid image"
        ));
    }
    if binding.current_layout == new_layout {
        return Ok(None);
    }

    let previous = scene_target_layout_access(binding.current_layout, true)?;
    let next = scene_target_layout_access(new_layout, false)?;
    let image_barrier = vk::ImageMemoryBarrier2::builder()
        .src_stage_mask(previous.stage)
        .src_access_mask(previous.access)
        .dst_stage_mask(next.stage)
        .dst_access_mask(next.access)
        .old_layout(binding.current_layout)
        .new_layout(new_layout)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(binding.image)
        .subresource_range(scene_target_color_subresource_range())
        .build();
    let image_barriers = [image_barrier];
    let dependency = vk::DependencyInfo::builder()
        .image_memory_barriers(&image_barriers)
        .build();
    unsafe {
        device.cmd_pipeline_barrier2(command_buffer, &dependency);
    }
    frame_resources.mark_offscreen_target_layout(target, new_layout)?;
    Ok(Some(NativeVulkanSceneTargetTransitionPlan {
        target,
        old_layout: scene_target_layout_label(binding.current_layout)?,
        new_layout: scene_target_layout_label(new_layout)?,
        src_stage: previous.stage_label,
        dst_stage: next.stage_label,
        src_access: previous.access_label,
        dst_access: next.access_label,
        reason,
        command_order: [
            "map_scene_target_layout_to_vk_sync2",
            "cmd_pipeline_barrier2_scene_target",
        ],
    }))
}

fn scene_target_layout_access(
    layout: vk::ImageLayout,
    source: bool,
) -> Result<NativeVulkanSceneTargetLayoutAccess, String> {
    match layout {
        vk::ImageLayout::UNDEFINED if source => Ok(NativeVulkanSceneTargetLayoutAccess {
            stage: vk::PipelineStageFlags2::TOP_OF_PIPE,
            access: vk::AccessFlags2::empty(),
            stage_label: "top-of-pipe",
            access_label: "none",
        }),
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL => Ok(NativeVulkanSceneTargetLayoutAccess {
            stage: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            access: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            stage_label: "color-attachment-output",
            access_label: "color-attachment-write",
        }),
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL => Ok(NativeVulkanSceneTargetLayoutAccess {
            stage: vk::PipelineStageFlags2::FRAGMENT_SHADER,
            access: vk::AccessFlags2::SHADER_SAMPLED_READ,
            stage_label: "fragment-shader",
            access_label: "shader-sampled-read",
        }),
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL => Ok(NativeVulkanSceneTargetLayoutAccess {
            stage: vk::PipelineStageFlags2::ALL_TRANSFER,
            access: vk::AccessFlags2::TRANSFER_READ,
            stage_label: "transfer",
            access_label: "transfer-read",
        }),
        vk::ImageLayout::TRANSFER_DST_OPTIMAL => Ok(NativeVulkanSceneTargetLayoutAccess {
            stage: vk::PipelineStageFlags2::ALL_TRANSFER,
            access: vk::AccessFlags2::TRANSFER_WRITE,
            stage_label: "transfer",
            access_label: "transfer-write",
        }),
        _ => Err(format!(
            "scene target layout {layout:?} has no runtime transition mapping"
        )),
    }
}

fn scene_target_layout_label(layout: vk::ImageLayout) -> Result<&'static str, String> {
    match layout {
        vk::ImageLayout::UNDEFINED => Ok("undefined"),
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL => Ok("color-attachment-optimal"),
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL => Ok("shader-read-only-optimal"),
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL => Ok("transfer-src-optimal"),
        vk::ImageLayout::TRANSFER_DST_OPTIMAL => Ok("transfer-dst-optimal"),
        _ => Err(format!(
            "scene target layout {layout:?} has no telemetry label"
        )),
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
