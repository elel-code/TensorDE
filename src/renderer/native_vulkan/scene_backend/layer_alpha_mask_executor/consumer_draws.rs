//! Generated `CLIPPINGTARGET` consumer draw contracts for WE alpha-mask tokens.
//!
//! References:
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/renderer_rd/pipeline_hash_map_rd.h`

use serde::Serialize;

use crate::engine::scene_engine::{
    SceneGraphTarget, SceneLayerCompositorBlendKey, SceneLayerCompositorOperation,
    SceneLayerCompositorTarget, SceneObjectId,
};

use super::resource_binds::{
    NativeVulkanSceneLayerAlphaMaskResourceBindCommandPlan,
    NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan,
};
use super::rt_method8::{LAYER_490_RT_METHOD8_OFFSET, LAYER_490_RT_METHOD8_RECEIVER_LABEL};
use super::token_schedule::{
    NativeVulkanSceneLayerAlphaMaskTokenSchedulePlan,
    NativeVulkanSceneLayerAlphaMaskTokenScheduleStep,
    NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind,
};
use super::{
    CLIPPINGTARGET_TEXTURE_SLOT_MASK, NativeVulkanSceneLayerAlphaMaskDescriptorSource,
    NativeVulkanSceneLayerAlphaMaskRuntimePlan, NativeVulkanSceneLayerAlphaMaskTextureBindRole,
};

pub(in crate::renderer::native_vulkan) const GENERATED_CLIPPINGTARGET_SHADER: &str =
    "we/genericimage4";
const GENERATED_CLIPPINGTARGET_REQUIRED_TEXTURE_SLOTS: [u32; 2] = [0, 8];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawRuntimePlan
{
    pub command_count: usize,
    pub consumer_draw_count: usize,
    pub heap_binding_count: usize,
    pub texture_slot_mask: u32,
    pub bindings: Vec<NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawBindingPlan>,
    pub command_order: [&'static str; 6],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawBindingPlan
{
    pub consumer_draw_index: usize,
    pub command_index: usize,
    pub object: SceneObjectId,
    pub operation: SceneLayerCompositorOperation,
    pub source_mask: SceneGraphTarget,
    pub target: SceneLayerCompositorTarget,
    pub target_receiver: &'static str,
    pub draw_receiver_vtable_offset: &'static str,
    pub shader: &'static str,
    pub texture_slot_mask: u32,
    pub required_texture_slots: [u32; 2],
    pub heap_bind_index: usize,
    pub heap_slice_index: usize,
    pub base_resource_descriptor_index: usize,
    pub base_sampler_descriptor_index: usize,
    pub resource_descriptor_count: usize,
    pub texture_count: usize,
    pub material_uniform_buffer_handle: u64,
    pub material_uniform_device_address: u64,
    pub material_uniform_bytes: u64,
    pub material_uniform_payload_hash: u64,
    pub blend_byte_source: &'static str,
    pub generated_material_source: &'static str,
    pub shader_mappings: Vec<String>,
    pub command_order: [&'static str; 6],
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_draws(
    runtime: &NativeVulkanSceneLayerAlphaMaskRuntimePlan,
    resource_binds: &NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan,
    schedule: &NativeVulkanSceneLayerAlphaMaskTokenSchedulePlan,
) -> Result<NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawRuntimePlan, String> {
    if runtime.tokenized_layer_count == 0 {
        return Ok(NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawRuntimePlan::empty());
    }
    if runtime.commands.len() != schedule.command_count {
        return Err(format!(
            "scene layer alpha-mask generated consumer expected {} scheduled commands for {} runtime commands",
            schedule.command_count,
            runtime.commands.len()
        ));
    }

    let mut bindings = Vec::new();
    for step in schedule.steps.iter().filter(|step| {
        step.kind
            == NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::GeneratedClippingTargetConsumer
    }) {
        let command = runtime.commands.get(step.command_index).ok_or_else(|| {
            format!(
                "scene layer alpha-mask generated consumer references missing command {}",
                step.command_index
            )
        })?;
        validate_generated_consumer_step(step)?;
        validate_generated_consumer_command(step.command_index, command)?;
        let heap_bind_index = *step.matched_heap_bind_indices.first().ok_or_else(|| {
            format!(
                "scene layer alpha-mask generated consumer command {} has no matched heap bind",
                step.command_index
            )
        })?;
        let bind = resource_binds
            .binds
            .iter()
            .find(|bind| bind.heap_bind_index == heap_bind_index)
            .ok_or_else(|| {
                format!(
                    "scene layer alpha-mask generated consumer command {} references missing heap bind {}",
                    step.command_index, heap_bind_index
                )
            })?;
        bindings.push(generated_consumer_binding(bindings.len(), step, bind)?);
    }

    Ok(
        NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawRuntimePlan::from_bindings(
            runtime.commands.len(),
            bindings,
        ),
    )
}

impl NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawRuntimePlan {
    fn empty() -> Self {
        Self {
            command_count: 0,
            consumer_draw_count: 0,
            heap_binding_count: 0,
            texture_slot_mask: 0,
            bindings: Vec::new(),
            command_order: generated_consumer_command_order(),
        }
    }

    fn from_bindings(
        command_count: usize,
        bindings: Vec<NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawBindingPlan>,
    ) -> Self {
        Self {
            command_count,
            consumer_draw_count: bindings.len(),
            heap_binding_count: bindings.len(),
            texture_slot_mask: bindings
                .iter()
                .fold(0u32, |mask, binding| mask | binding.texture_slot_mask),
            bindings,
            command_order: generated_consumer_command_order(),
        }
    }
}

fn generated_consumer_binding(
    consumer_draw_index: usize,
    step: &NativeVulkanSceneLayerAlphaMaskTokenScheduleStep,
    bind: &NativeVulkanSceneLayerAlphaMaskResourceBindCommandPlan,
) -> Result<NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawBindingPlan, String> {
    validate_generated_consumer_heap_bind(step.command_index, step.object, bind)?;
    let material = bind.bind.material.as_ref().ok_or_else(|| {
        format!(
            "scene layer alpha-mask generated consumer command {} requires generated material uniform buffer",
            step.command_index
        )
    })?;
    Ok(
        NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawBindingPlan {
            consumer_draw_index,
            command_index: step.command_index,
            object: step.object,
            operation: SceneLayerCompositorOperation::DrawGeneratedClippingTarget,
            source_mask: SceneGraphTarget::FullAlphaMask,
            target: SceneLayerCompositorTarget::LayerTarget490,
            target_receiver: LAYER_490_RT_METHOD8_RECEIVER_LABEL,
            draw_receiver_vtable_offset: LAYER_490_RT_METHOD8_OFFSET,
            shader: GENERATED_CLIPPINGTARGET_SHADER,
            texture_slot_mask: CLIPPINGTARGET_TEXTURE_SLOT_MASK,
            required_texture_slots: GENERATED_CLIPPINGTARGET_REQUIRED_TEXTURE_SLOTS,
            heap_bind_index: bind.heap_bind_index,
            heap_slice_index: bind.bind.heap_slice_index,
            base_resource_descriptor_index: bind.bind.base_resource_descriptor_index,
            base_sampler_descriptor_index: bind.bind.base_sampler_descriptor_index,
            resource_descriptor_count: bind.bind.resource_descriptor_count,
            texture_count: bind.bind.texture_count,
            material_uniform_buffer_handle: material.buffer_handle,
            material_uniform_device_address: material.device_address,
            material_uniform_bytes: material.bytes,
            material_uniform_payload_hash: material.payload_hash,
            blend_byte_source: "subdraw+0x40 -> generated material +0x1f0",
            generated_material_source: "local generated material variant +0x428",
            shader_mappings: bind.bind.shader_mappings.clone(),
            command_order: [
                "read_generated_clippingtarget_token_step",
                "match_single_generated_clippingtarget_heap_bind",
                "validate_slot0_source_and_slot8_full_alpha_mask",
                "preserve_subdraw_blend_byte_to_generated_material_0x1f0",
                "preserve_layer_0x490_rt_method_8_draw_receiver",
                "lower_receiver_to_rt_method_8_bridge_plan",
            ],
        },
    )
}

fn validate_generated_consumer_step(
    step: &NativeVulkanSceneLayerAlphaMaskTokenScheduleStep,
) -> Result<(), String> {
    if step.matched_heap_bind_indices.len() != 1 {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {} requires exactly one heap bind, got {}",
            step.command_index,
            step.matched_heap_bind_indices.len()
        ));
    }
    Ok(())
}

fn validate_generated_consumer_command(
    command_index: usize,
    command: &super::NativeVulkanSceneLayerAlphaMaskCommandPlan,
) -> Result<(), String> {
    if command.operation != SceneLayerCompositorOperation::DrawGeneratedClippingTarget {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {command_index} expected DrawGeneratedClippingTarget, got {:?}",
            command.operation
        ));
    }
    if command.source_graph_target != Some(SceneGraphTarget::FullAlphaMask)
        || command.source != Some(SceneLayerCompositorTarget::FullAlphaMask)
    {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {command_index} must sample FullAlphaMask"
        ));
    }
    if command.target != SceneLayerCompositorTarget::LayerTarget490 {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {command_index} must draw through layer+0x490, got {:?}",
            command.target
        ));
    }
    if command.blend_key != SceneLayerCompositorBlendKey::SubdrawBlendByteToGeneratedMaterial1f0 {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {command_index} must lower subdraw +0x40 to generated material +0x1f0, got {:?}",
            command.blend_key
        ));
    }
    Ok(())
}

fn validate_generated_consumer_heap_bind(
    command_index: usize,
    object: SceneObjectId,
    bind: &NativeVulkanSceneLayerAlphaMaskResourceBindCommandPlan,
) -> Result<(), String> {
    if bind.object != object {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {command_index} object mismatch: command {:?}, heap {:?}",
            object, bind.object
        ));
    }
    if bind.operation != SceneLayerCompositorOperation::DrawGeneratedClippingTarget {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {command_index} requires generated target heap bind, got {:?}",
            bind.operation
        ));
    }
    if bind.shader != GENERATED_CLIPPINGTARGET_SHADER {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {command_index} shader mismatch: expected {}, heap {}",
            GENERATED_CLIPPINGTARGET_SHADER, bind.shader
        ));
    }
    if bind.role != NativeVulkanSceneLayerAlphaMaskTextureBindRole::GeneratedClippingTarget {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {command_index} requires GeneratedClippingTarget heap bind, got {:?}",
            bind.role
        ));
    }
    if bind.bind.texture_count != 2 || bind.bind.resource_descriptor_count < 3 {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {command_index} requires material uniform plus slot0/slot8 heap bind, got textures={} resources={}",
            bind.bind.texture_count, bind.bind.resource_descriptor_count
        ));
    }
    if bind.bind.material.is_none() {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {command_index} requires generated material uniform buffer"
        ));
    }
    let mut slots = bind
        .bind
        .heap_slice
        .bindings
        .iter()
        .map(|binding| binding.slot)
        .collect::<Vec<_>>();
    slots.sort_unstable();
    if slots != GENERATED_CLIPPINGTARGET_REQUIRED_TEXTURE_SLOTS {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {command_index} requires g_Texture0/g_Texture8 heap bind, got slots {:?}",
            slots
        ));
    }
    let has_full_mask = bind.bind.heap_slice.bindings.iter().any(|binding| {
        binding.slot == 8
            && binding.source
                == NativeVulkanSceneLayerAlphaMaskDescriptorSource::GraphTarget(
                    SceneGraphTarget::FullAlphaMask,
                )
    });
    if !has_full_mask {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {command_index} requires g_Texture8 to sample FullAlphaMask"
        ));
    }
    Ok(())
}

fn generated_consumer_command_order() -> [&'static str; 6] {
    [
        "read_generated_clippingtarget_schedule_steps",
        "resolve_single_generated_clippingtarget_heap_bind",
        "validate_genericimage4_clippingtarget_slots_0_8",
        "preserve_generated_material_0x428",
        "preserve_subdraw_blend_byte_to_material_0x1f0",
        "preserve_layer_0x490_generated_draw_receiver",
    ]
}

#[cfg(test)]
#[path = "consumer_draws_tests.rs"]
mod tests;
