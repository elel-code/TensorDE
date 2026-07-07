//! Effect copy/swap runtime command handling.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/effects/effect-semantics.md`
//! - `reverse-engineered/effects/fluidsimulation.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use std::collections::BTreeSet;

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene_engine::{
    SceneEffectPassGraphCopy, SceneEffectPassGraphSwap, SceneGraphTarget, SceneObjectId,
};

use super::super::frame_resources::NativeVulkanSceneFrameResources;
use super::target_access::{
    NativeVulkanSceneEffectTargetTransitionPlan, effect_copy_subresource_layers,
    record_effect_target_transition,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectCopyCommandPlan {
    pub graph_command_index: usize,
    pub object: SceneObjectId,
    pub pass_index: usize,
    pub source: SceneGraphTarget,
    pub target: SceneGraphTarget,
    pub width: u32,
    pub height: u32,
    pub source_access: Option<NativeVulkanSceneEffectTargetTransitionPlan>,
    pub target_access: Option<NativeVulkanSceneEffectTargetTransitionPlan>,
    pub copy_image_count: usize,
    pub command_order: [&'static str; 4],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectSwapCommandPlan {
    pub graph_command_index: usize,
    pub object: SceneObjectId,
    pub pass_index: usize,
    pub a: SceneGraphTarget,
    pub b: SceneGraphTarget,
    pub alias_applied_in_lowering: bool,
    pub gpu_command_count: usize,
    pub command_order: [&'static str; 2],
}

impl NativeVulkanSceneEffectSwapCommandPlan {
    pub(super) fn from_graph_swap(swap: &SceneEffectPassGraphSwap) -> Self {
        Self {
            graph_command_index: swap.graph_command_index,
            object: swap.object,
            pass_index: swap.pass_index,
            a: swap.a,
            b: swap.b,
            alias_applied_in_lowering: true,
            gpu_command_count: 0,
            command_order: [
                "apply_effect_fbo_alias_during_graph_lowering",
                "emit_no_gpu_command_for_effect_swap",
            ],
        }
    }
}

pub(super) fn record_effect_copy_command(
    frame_resources: &mut NativeVulkanSceneFrameResources,
    device: &Device,
    command_buffer: vk::CommandBuffer,
    copy: &SceneEffectPassGraphCopy,
    written_targets: &mut BTreeSet<SceneGraphTarget>,
) -> Result<NativeVulkanSceneEffectCopyCommandPlan, String> {
    if copy.source == copy.target {
        return Err(format!(
            "scene effect copy command {} for object {:?} uses the same source and target {:?}",
            copy.pass_index, copy.object, copy.source
        ));
    }
    let source_access = record_effect_target_transition(
        frame_resources,
        device,
        command_buffer,
        copy.source,
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        "effect-copy-source-transfer-read",
    )?;
    let target_access = record_effect_target_transition(
        frame_resources,
        device,
        command_buffer,
        copy.target,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        "effect-copy-target-transfer-write",
    )?;
    let source = frame_resources.offscreen_target_binding(copy.source)?;
    let target = frame_resources.offscreen_target_binding(copy.target)?;
    validate_effect_copy_images(
        copy,
        source.format,
        target.format,
        source.width,
        source.height,
        target.width,
        target.height,
    )?;

    let region = vk::ImageCopy::builder()
        .src_subresource(effect_copy_subresource_layers())
        .src_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
        .dst_subresource(effect_copy_subresource_layers())
        .dst_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
        .extent(vk::Extent3D {
            width: source.width,
            height: source.height,
            depth: 1,
        })
        .build();
    unsafe {
        device.cmd_copy_image(
            command_buffer,
            source.image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            target.image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[region],
        );
    }
    written_targets.insert(copy.target);

    Ok(NativeVulkanSceneEffectCopyCommandPlan {
        graph_command_index: copy.graph_command_index,
        object: copy.object,
        pass_index: copy.pass_index,
        source: copy.source,
        target: copy.target,
        width: source.width,
        height: source.height,
        source_access,
        target_access,
        copy_image_count: 1,
        command_order: [
            "transition_effect_copy_source_to_transfer_src",
            "transition_effect_copy_target_to_transfer_dst",
            "cmd_copy_image_effect_target",
            "retain_effect_copy_target_as_transfer_dst",
        ],
    })
}

fn validate_effect_copy_images(
    copy: &SceneEffectPassGraphCopy,
    source_format: vk::Format,
    target_format: vk::Format,
    source_width: u32,
    source_height: u32,
    target_width: u32,
    target_height: u32,
) -> Result<(), String> {
    if source_format != target_format {
        return Err(format!(
            "scene effect copy command {} for object {:?} requires matching formats: source {:?}, target {:?}",
            copy.pass_index, copy.object, source_format, target_format
        ));
    }
    if source_width != target_width || source_height != target_height {
        return Err(format!(
            "scene effect copy command {} for object {:?} requires matching extents: source {}x{}, target {}x{}",
            copy.pass_index, copy.object, source_width, source_height, target_width, target_height
        ));
    }
    Ok(())
}
