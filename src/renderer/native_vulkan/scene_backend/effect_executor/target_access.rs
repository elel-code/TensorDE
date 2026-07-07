//! Effect target layout access and synchronization helpers.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/effects/effect-semantics.md`
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene_engine::{
    SceneEffectPassGraphMaterialPass, SceneEffectPassGraphOutput, SceneGraphTarget,
};
use crate::renderer::native_vulkan::NativeVulkanClearColor;

use super::super::frame_resources::NativeVulkanSceneFrameResources;
use super::super::render_target::{
    NativeVulkanSceneOffscreenRenderTarget, NativeVulkanSceneRenderTarget,
};
use super::super::target_formats::NativeVulkanSceneGraphTargetFormatPlan;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneEffectTargetAccessPlan {
    Transition(NativeVulkanSceneEffectTargetTransitionPlan),
    InitialClear(NativeVulkanSceneEffectTargetInitialClearPlan),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectTargetInitialClearPlan {
    pub target: SceneGraphTarget,
    pub clear_layout: &'static str,
    pub final_layout: &'static str,
    pub transitions: Vec<NativeVulkanSceneEffectTargetTransitionPlan>,
    pub command_order: [&'static str; 3],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectTargetTransitionPlan {
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
struct NativeVulkanSceneEffectLayoutAccess {
    stage: vk::PipelineStageFlags2,
    access: vk::AccessFlags2,
    stage_label: &'static str,
    access_label: &'static str,
}

pub(super) fn record_effect_target_shader_read_access(
    frame_resources: &mut NativeVulkanSceneFrameResources,
    device: &Device,
    command_buffer: vk::CommandBuffer,
    target: SceneGraphTarget,
) -> Result<Option<NativeVulkanSceneEffectTargetAccessPlan>, String> {
    let binding = frame_resources.offscreen_target_binding(target)?;
    if binding.current_layout == vk::ImageLayout::UNDEFINED {
        let to_clear = record_effect_target_transition(
            frame_resources,
            device,
            command_buffer,
            target,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            "initialize-effect-target-before-first-sampled-read",
        )?
        .ok_or_else(|| {
            format!("scene effect target {target:?} initialization transition was not recorded")
        })?;
        unsafe {
            device.cmd_clear_color_image(
                command_buffer,
                binding.image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 0.0],
                },
                &[effect_color_subresource_range()],
            );
        }
        let to_read = record_effect_target_transition(
            frame_resources,
            device,
            command_buffer,
            target,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            "effect-target-initial-clear-to-sampled-read",
        )?
        .ok_or_else(|| {
            format!("scene effect target {target:?} sampled-read transition was not recorded")
        })?;
        return Ok(Some(NativeVulkanSceneEffectTargetAccessPlan::InitialClear(
            NativeVulkanSceneEffectTargetInitialClearPlan {
                target,
                clear_layout: "transfer-dst-optimal",
                final_layout: "shader-read-only-optimal",
                transitions: vec![to_clear, to_read],
                command_order: [
                    "transition_undefined_effect_target_to_transfer_dst",
                    "cmd_clear_color_image_transparent",
                    "transition_effect_target_to_shader_read",
                ],
            },
        )));
    }

    Ok(record_effect_target_transition(
        frame_resources,
        device,
        command_buffer,
        target,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        "effect-material-input-sampled-read",
    )?
    .map(NativeVulkanSceneEffectTargetAccessPlan::Transition))
}

pub(super) fn record_effect_target_transition(
    frame_resources: &mut NativeVulkanSceneFrameResources,
    device: &Device,
    command_buffer: vk::CommandBuffer,
    target: SceneGraphTarget,
    new_layout: vk::ImageLayout,
    reason: &'static str,
) -> Result<Option<NativeVulkanSceneEffectTargetTransitionPlan>, String> {
    if target == SceneGraphTarget::Swapchain {
        return Err("scene effect runtime does not write direct swapchain targets before the object-final compositor exists".to_owned());
    }
    if command_buffer == vk::CommandBuffer::null() {
        return Err("scene effect target transition requires a valid command buffer".to_owned());
    }
    let binding = frame_resources.offscreen_target_binding(target)?;
    if binding.current_layout == new_layout {
        return Ok(None);
    }
    let previous = effect_layout_access(binding.current_layout, true)?;
    let next = effect_layout_access(new_layout, false)?;
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
        .subresource_range(effect_color_subresource_range())
        .build();
    let image_barriers = [image_barrier];
    let dependency = vk::DependencyInfo::builder()
        .image_memory_barriers(&image_barriers)
        .build();
    unsafe {
        device.cmd_pipeline_barrier2(command_buffer, &dependency);
    }
    frame_resources.mark_offscreen_target_layout(target, new_layout)?;
    Ok(Some(NativeVulkanSceneEffectTargetTransitionPlan {
        target,
        old_layout: effect_layout_label(binding.current_layout)?,
        new_layout: effect_layout_label(new_layout)?,
        src_stage: previous.stage_label,
        dst_stage: next.stage_label,
        src_access: previous.access_label,
        dst_access: next.access_label,
        reason,
        command_order: [
            "map_effect_target_layout_to_vk_sync2",
            "cmd_pipeline_barrier2_effect_target",
        ],
    }))
}

pub(super) fn effect_offscreen_render_target(
    frame_resources: &NativeVulkanSceneFrameResources,
    target: SceneGraphTarget,
) -> Result<NativeVulkanSceneRenderTarget, String> {
    if target == SceneGraphTarget::Swapchain {
        return Err(
            "scene effect runtime direct swapchain output requires the object-final compositor"
                .to_owned(),
        );
    }
    let binding = frame_resources.offscreen_target_binding(target)?;
    Ok(NativeVulkanSceneRenderTarget::Offscreen(
        NativeVulkanSceneOffscreenRenderTarget {
            target,
            image: binding.image,
            image_view: binding.view,
            extent: vk::Extent2D {
                width: binding.width,
                height: binding.height,
            },
            initial_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            final_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        },
    ))
}

pub(super) fn effect_target_format(
    frame_resources: &NativeVulkanSceneFrameResources,
    target_formats: &NativeVulkanSceneGraphTargetFormatPlan,
    target: SceneGraphTarget,
) -> Result<vk::Format, String> {
    if target == SceneGraphTarget::Swapchain {
        return target_formats.format(target);
    }
    frame_resources
        .offscreen_target_binding(target)
        .map(|binding| binding.format)
        .or_else(|effect_err| {
            target_formats.format(target).map_err(|graph_err| {
                format!("{effect_err}; scene graph target format fallback also failed: {graph_err}")
            })
        })
}

pub(super) fn effect_pass_render_target(
    pass: &SceneEffectPassGraphMaterialPass,
) -> Result<SceneGraphTarget, String> {
    match pass.output {
        SceneEffectPassGraphOutput::GraphTarget(target) => Ok(target),
        SceneEffectPassGraphOutput::ObjectFinal(object) => Err(format!(
            "scene effect runtime pass {} for object {:?} outputs ObjectFinal({object:?}) and requires the object-final compositor target resolver",
            pass.pass_index, pass.object
        )),
    }
}

pub(super) fn effect_access_transition_count(
    accesses: &[NativeVulkanSceneEffectTargetAccessPlan],
) -> usize {
    accesses
        .iter()
        .map(|access| match access {
            NativeVulkanSceneEffectTargetAccessPlan::Transition(_) => 1,
            NativeVulkanSceneEffectTargetAccessPlan::InitialClear(clear) => clear.transitions.len(),
        })
        .sum()
}

pub(super) fn effect_access_initial_clear_count(
    accesses: &[NativeVulkanSceneEffectTargetAccessPlan],
) -> usize {
    accesses
        .iter()
        .filter(|access| {
            matches!(
                access,
                NativeVulkanSceneEffectTargetAccessPlan::InitialClear(_)
            )
        })
        .count()
}

pub(super) fn effect_copy_subresource_layers() -> vk::ImageSubresourceLayers {
    vk::ImageSubresourceLayers::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .mip_level(0)
        .base_array_layer(0)
        .layer_count(1)
        .build()
}

fn effect_color_subresource_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1)
        .build()
}

pub(super) fn transparent_clear_color() -> NativeVulkanClearColor {
    NativeVulkanClearColor {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    }
}

fn effect_layout_access(
    layout: vk::ImageLayout,
    source: bool,
) -> Result<NativeVulkanSceneEffectLayoutAccess, String> {
    match layout {
        vk::ImageLayout::UNDEFINED if source => Ok(NativeVulkanSceneEffectLayoutAccess {
            stage: vk::PipelineStageFlags2::TOP_OF_PIPE,
            access: vk::AccessFlags2::empty(),
            stage_label: "top-of-pipe",
            access_label: "none",
        }),
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL => Ok(NativeVulkanSceneEffectLayoutAccess {
            stage: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            access: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            stage_label: "color-attachment-output",
            access_label: "color-attachment-write",
        }),
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL => Ok(NativeVulkanSceneEffectLayoutAccess {
            stage: vk::PipelineStageFlags2::FRAGMENT_SHADER,
            access: vk::AccessFlags2::SHADER_SAMPLED_READ,
            stage_label: "fragment-shader",
            access_label: "shader-sampled-read",
        }),
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL => Ok(NativeVulkanSceneEffectLayoutAccess {
            stage: vk::PipelineStageFlags2::ALL_TRANSFER,
            access: vk::AccessFlags2::TRANSFER_READ,
            stage_label: "transfer",
            access_label: "transfer-read",
        }),
        vk::ImageLayout::TRANSFER_DST_OPTIMAL => Ok(NativeVulkanSceneEffectLayoutAccess {
            stage: vk::PipelineStageFlags2::ALL_TRANSFER,
            access: vk::AccessFlags2::TRANSFER_WRITE,
            stage_label: "transfer",
            access_label: "transfer-write",
        }),
        _ => Err(format!(
            "scene effect target layout {layout:?} has no runtime transition mapping"
        )),
    }
}

fn effect_layout_label(layout: vk::ImageLayout) -> Result<&'static str, String> {
    match layout {
        vk::ImageLayout::UNDEFINED => Ok("undefined"),
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL => Ok("color-attachment-optimal"),
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL => Ok("shader-read-only-optimal"),
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL => Ok("transfer-src-optimal"),
        vk::ImageLayout::TRANSFER_DST_OPTIMAL => Ok("transfer-dst-optimal"),
        _ => Err(format!(
            "scene effect target layout {layout:?} has no telemetry label"
        )),
    }
}
