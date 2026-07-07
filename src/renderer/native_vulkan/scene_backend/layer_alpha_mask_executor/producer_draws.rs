//! Producer draw contracts for WE `clippingmaskimage4` alpha-mask steps.
//!
//! References:
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`

use serde::Serialize;

use crate::engine::scene_engine::{
    SceneGraphPipelineClass, SceneGraphTarget, SceneLayerCompositorBlendKey,
    SceneLayerCompositorCondition, SceneLayerCompositorEntry, SceneLayerCompositorOperation,
    SceneObjectId,
};
use crate::renderer::native_vulkan::scene_backend::render_target::NativeVulkanSceneRenderTargetLoadOp;

use super::resource_binds::{
    NativeVulkanSceneLayerAlphaMaskBindRequirement,
    NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan,
};
use super::token_schedule::{
    NativeVulkanSceneLayerAlphaMaskTokenSchedulePlan,
    NativeVulkanSceneLayerAlphaMaskTokenScheduleStep,
    NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind,
};
use super::{
    CLIPPINGMASKIMAGE4_MORPH_TEXTURE_SLOT, CLIPPINGMASKIMAGE4_REQUIRED_TEXTURE_SLOT_MASK,
    NativeVulkanSceneLayerAlphaMaskRuntimePlan,
};

pub(in crate::renderer::native_vulkan) const CLIPPINGMASKIMAGE4_MATERIAL: &str =
    "materials/util/clippingmaskimage4.json";
pub(in crate::renderer::native_vulkan) const CLIPPINGMASKIMAGE4_SHADER: &str =
    "we/clippingmaskimage4";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskProducerDrawRuntimePlan
{
    pub command_count: usize,
    pub producer_draw_count: usize,
    pub full_mask_producer_count: usize,
    pub intermediate_mask_producer_count: usize,
    pub clear_target_scope_count: usize,
    pub load_target_scope_count: usize,
    pub texture_slot_mask: u32,
    pub draws: Vec<NativeVulkanSceneLayerAlphaMaskProducerDrawPlan>,
    pub command_order: [&'static str; 6],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskProducerDrawPlan {
    pub producer_draw_index: usize,
    pub command_index: usize,
    pub object: SceneObjectId,
    pub condition: SceneLayerCompositorCondition,
    pub target: SceneGraphTarget,
    pub target_byte: u8,
    pub clear_first: bool,
    pub target_scope_load_op: NativeVulkanSceneRenderTargetLoadOp,
    pub material: &'static str,
    pub shader: &'static str,
    pub pipeline_class: SceneGraphPipelineClass,
    pub target_format: &'static str,
    pub texture_slot_mask: u32,
    pub optional_morph_texture_slot: u32,
    pub heap_bind_count: usize,
    pub heap_bind_indices: Vec<usize>,
    pub subdraw_mask_texture_field_offset: &'static str,
    pub subdraw_invert_flag: &'static str,
    pub draw_receiver: &'static str,
    pub draw_receiver_vtable_offset: &'static str,
    pub reference_points: [&'static str; 4],
    pub command_order: [&'static str; 6],
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_alpha_mask_producer_draws(
    runtime: &NativeVulkanSceneLayerAlphaMaskRuntimePlan,
    resource_binds: &NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan,
    schedule: &NativeVulkanSceneLayerAlphaMaskTokenSchedulePlan,
) -> Result<NativeVulkanSceneLayerAlphaMaskProducerDrawRuntimePlan, String> {
    if runtime.tokenized_layer_count == 0 {
        return Ok(NativeVulkanSceneLayerAlphaMaskProducerDrawRuntimePlan::empty());
    }

    let mut draws = Vec::new();
    for step in &schedule.steps {
        if !matches!(
            step.kind,
            NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::FullMaskProducer
                | NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::IntermediateMaskProducer
        ) {
            continue;
        }
        let command = runtime.commands.get(step.command_index).ok_or_else(|| {
            format!(
                "scene layer alpha-mask producer draw references missing command {}",
                step.command_index
            )
        })?;
        let bind = resource_binds
            .token_commands
            .iter()
            .find(|bind| bind.command_index == step.command_index)
            .ok_or_else(|| {
                format!(
                    "scene layer alpha-mask producer command {} has no token heap-bind fact",
                    step.command_index
                )
            })?;
        validate_producer_step(step, command, bind.requirement)?;
        let target = command.target_graph_target.ok_or_else(|| {
            format!(
                "scene layer alpha-mask producer command {} has no graph target",
                step.command_index
            )
        })?;
        let (target_byte, clear_first, target_scope_load_op) =
            producer_target_byte_and_scope(step.kind, command.condition, target)?;
        draws.push(NativeVulkanSceneLayerAlphaMaskProducerDrawPlan {
            producer_draw_index: draws.len(),
            command_index: step.command_index,
            object: command.object,
            condition: command.condition,
            target,
            target_byte,
            clear_first,
            target_scope_load_op,
            material: CLIPPINGMASKIMAGE4_MATERIAL,
            shader: CLIPPINGMASKIMAGE4_SHADER,
            pipeline_class: SceneGraphPipelineClass::PuppetSkinning,
            target_format: "R8_UNORM",
            texture_slot_mask: CLIPPINGMASKIMAGE4_REQUIRED_TEXTURE_SLOT_MASK,
            optional_morph_texture_slot: CLIPPINGMASKIMAGE4_MORPH_TEXTURE_SLOT,
            heap_bind_count: step.matched_heap_bind_count,
            heap_bind_indices: step.matched_heap_bind_indices.clone(),
            subdraw_mask_texture_field_offset: "0x38",
            subdraw_invert_flag: "0x44 bit 0x2",
            draw_receiver: "[layer+0x490]",
            draw_receiver_vtable_offset: "0x40",
            reference_points: [
                "reverse-engineered/docs/exe/clipping-pipeline.md: 0x14020d6bc target byte",
                "reverse-engineered/docs/exe/clipping-pipeline.md: token 1/token 2 clear_first behavior",
                "reverse-engineered/docs/exe/composelayer-and-effecttarget.md: 0x14009b140/0x14009b160 clear pair",
                "reverse-engineered/docs/exe/blend-and-render.md: [layer+0x490].vtable+0x40 RT method [8]",
            ],
            command_order: [
                "read_scheduled_clippingmaskimage4_producer",
                "map_token_condition_to_target_byte",
                "map_clear_first_to_target_scope_load_op",
                "bind_clippingmaskimage4_resource_heap",
                "record_layer_0x490_rt_method_8_draw",
                "retain_alpha_mask_target_layout",
            ],
        });
    }

    Ok(NativeVulkanSceneLayerAlphaMaskProducerDrawRuntimePlan::from_draws(draws))
}

impl NativeVulkanSceneLayerAlphaMaskProducerDrawRuntimePlan {
    fn empty() -> Self {
        Self {
            command_count: 0,
            producer_draw_count: 0,
            full_mask_producer_count: 0,
            intermediate_mask_producer_count: 0,
            clear_target_scope_count: 0,
            load_target_scope_count: 0,
            texture_slot_mask: 0,
            draws: Vec::new(),
            command_order: producer_draw_command_order(),
        }
    }

    fn from_draws(draws: Vec<NativeVulkanSceneLayerAlphaMaskProducerDrawPlan>) -> Self {
        Self {
            command_count: draws.len(),
            producer_draw_count: draws.len(),
            full_mask_producer_count: draws
                .iter()
                .filter(|draw| draw.target == SceneGraphTarget::FullAlphaMask)
                .count(),
            intermediate_mask_producer_count: draws
                .iter()
                .filter(|draw| draw.target == SceneGraphTarget::FullAlphaMaskIntermediate)
                .count(),
            clear_target_scope_count: draws
                .iter()
                .filter(|draw| {
                    draw.target_scope_load_op == NativeVulkanSceneRenderTargetLoadOp::Clear
                })
                .count(),
            load_target_scope_count: draws
                .iter()
                .filter(|draw| {
                    draw.target_scope_load_op == NativeVulkanSceneRenderTargetLoadOp::Load
                })
                .count(),
            texture_slot_mask: draws
                .iter()
                .fold(0u32, |mask, draw| mask | draw.texture_slot_mask),
            draws,
            command_order: producer_draw_command_order(),
        }
    }
}

fn validate_producer_step(
    step: &NativeVulkanSceneLayerAlphaMaskTokenScheduleStep,
    command: &super::NativeVulkanSceneLayerAlphaMaskCommandPlan,
    requirement: NativeVulkanSceneLayerAlphaMaskBindRequirement,
) -> Result<(), String> {
    if command.entry != SceneLayerCompositorEntry::AlphaMaskHelper20d6a0
        || command.operation != SceneLayerCompositorOperation::DrawClippingMask
        || command.source.is_some()
        || command.blend_key != SceneLayerCompositorBlendKey::Inherit
    {
        return Err(format!(
            "scene layer alpha-mask producer command {} must be 0x14020d6a0 DrawClippingMask with inherited blend",
            step.command_index
        ));
    }
    if requirement != NativeVulkanSceneLayerAlphaMaskBindRequirement::ClippingMaskImage4 {
        return Err(format!(
            "scene layer alpha-mask producer command {} expected clippingmaskimage4 heap-bind fact, got {:?}",
            step.command_index, requirement
        ));
    }
    if step.matched_heap_bind_count == 0 || step.matched_heap_bind_indices.is_empty() {
        return Err(format!(
            "scene layer alpha-mask producer command {} requires at least one clippingmaskimage4 heap bind",
            step.command_index
        ));
    }
    Ok(())
}

fn producer_target_byte_and_scope(
    kind: NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind,
    condition: SceneLayerCompositorCondition,
    target: SceneGraphTarget,
) -> Result<(u8, bool, NativeVulkanSceneRenderTargetLoadOp), String> {
    match (kind, condition, target) {
        (
            NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::FullMaskProducer,
            SceneLayerCompositorCondition::Token1OrToken2FirstPair,
            SceneGraphTarget::FullAlphaMask,
        ) => Ok((0, true, NativeVulkanSceneRenderTargetLoadOp::Clear)),
        (
            NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::IntermediateMaskProducer,
            SceneLayerCompositorCondition::Token2IntermediatePairOrFinalMask,
            SceneGraphTarget::FullAlphaMaskIntermediate,
        ) => Ok((1, false, NativeVulkanSceneRenderTargetLoadOp::Load)),
        _ => Err(format!(
            "scene layer alpha-mask producer cannot map {kind:?}/{condition:?}/{target:?} to WE target byte"
        )),
    }
}

fn producer_draw_command_order() -> [&'static str; 6] {
    [
        "read_alpha_mask_token_schedule",
        "select_clippingmaskimage4_producer_steps",
        "validate_0x14020d6a0_command_shape",
        "map_target_byte_0_full_1_intermediate",
        "map_clear_first_to_clear_or_load_scope",
        "preserve_layer_0x490_rt_method_8_draw_receiver",
    ]
}

#[cfg(test)]
#[path = "producer_draws_tests.rs"]
mod tests;
