//! Uniform contract for WE `clippingmaskimage4` producer draws.
//!
//! References:
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/clippingmaskimage4.frag`
//! - `references/godot/servers/rendering/rendering_device_graph.h`

use serde::Serialize;

use crate::engine::scene_engine::{SceneGraphTarget, SceneObjectId};
use crate::renderer::native_vulkan::scene_backend::render_target::NativeVulkanSceneRenderTargetLoadOp;

use super::producer_draws::{
    CLIPPINGMASKIMAGE4_SHADER, NativeVulkanSceneLayerAlphaMaskProducerDrawPlan,
    NativeVulkanSceneLayerAlphaMaskProducerDrawRuntimePlan,
};
use super::producer_pipeline::{
    NativeVulkanSceneLayerAlphaMaskProducerPipelineBindingPlan,
    NativeVulkanSceneLayerAlphaMaskProducerPipelinePlan,
};
use super::{CLIPPINGMASKIMAGE4_MORPH_TEXTURE_SLOT, CLIPPINGMASKIMAGE4_REQUIRED_TEXTURE_SLOT_MASK};

const RENDER_VAR0_INVERT_FLAG_MASK: u32 = 0x2;
const CLEAR_SETTER_VTABLE_OFFSET: &'static str = "0x118";
const CLEAR_EMIT_VTABLE_OFFSET: &'static str = "0x120";
const STATE_RENDER_VAR0_MIRROR_OFFSET: &'static str = "0xa8";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskProducerUniformPlan {
    pub producer_draw_count: usize,
    pub uniform_binding_count: usize,
    pub render_var0_contract_count: usize,
    pub clear_scalar_contract_count: usize,
    pub morph_texture_contract_count: usize,
    pub slot0_slot1_sample_contract_count: usize,
    pub bindings: Vec<NativeVulkanSceneLayerAlphaMaskProducerUniformBindingPlan>,
    pub command_order: [&'static str; 6],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskProducerUniformBindingPlan
{
    pub uniform_binding_index: usize,
    pub producer_draw_index: usize,
    pub command_index: usize,
    pub object: SceneObjectId,
    pub shader: &'static str,
    pub target: SceneGraphTarget,
    pub target_byte: u8,
    pub clear_first: bool,
    pub target_scope_load_op: NativeVulkanSceneRenderTargetLoadOp,
    pub heap_bind_indices: Vec<usize>,
    pub pipeline_binding_count: usize,
    pub texture_slot_mask: u32,
    pub optional_morph_texture_slot: u32,
    pub render_var0_uniform: &'static str,
    pub render_var0_component: &'static str,
    pub render_var0_value_source: &'static str,
    pub render_var0_invert_flag_mask: u32,
    pub render_var0_formula: &'static str,
    pub state_render_var0_mirror_offset: &'static str,
    pub clear_scalar_source: &'static str,
    pub clear_setter_vtable_offset: &'static str,
    pub clear_emit_vtable_offset: &'static str,
    pub slot0_sample_source: &'static str,
    pub slot1_sample_source: &'static str,
    pub slot5_morph_texture_source: &'static str,
    pub slot5_morph_enable_condition: &'static str,
    pub alpha_formula: &'static str,
    pub red_formula: &'static str,
    pub reference_points: [&'static str; 4],
    pub command_order: [&'static str; 8],
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_alpha_mask_producer_uniforms(
    producer_draws: &NativeVulkanSceneLayerAlphaMaskProducerDrawRuntimePlan,
    producer_pipelines: &NativeVulkanSceneLayerAlphaMaskProducerPipelinePlan,
) -> Result<NativeVulkanSceneLayerAlphaMaskProducerUniformPlan, String> {
    NativeVulkanSceneLayerAlphaMaskProducerUniformPlan::from_draws_and_pipelines(
        producer_draws,
        producer_pipelines,
    )
}

impl NativeVulkanSceneLayerAlphaMaskProducerUniformPlan {
    fn from_draws_and_pipelines(
        producer_draws: &NativeVulkanSceneLayerAlphaMaskProducerDrawRuntimePlan,
        producer_pipelines: &NativeVulkanSceneLayerAlphaMaskProducerPipelinePlan,
    ) -> Result<Self, String> {
        if producer_draws.draws.is_empty() {
            return Ok(Self::empty());
        }

        let mut bindings = Vec::with_capacity(producer_draws.draws.len());
        for draw in &producer_draws.draws {
            let pipeline_bindings = producer_pipelines
                .bindings
                .iter()
                .filter(|binding| binding.producer_draw_index == draw.producer_draw_index)
                .collect::<Vec<_>>();
            if pipeline_bindings.is_empty() {
                return Err(format!(
                    "scene layer alpha-mask producer command {} has no clippingmaskimage4 pipeline binding for uniform contract",
                    draw.command_index
                ));
            }
            bindings.push(
                NativeVulkanSceneLayerAlphaMaskProducerUniformBindingPlan::from_draw_and_pipeline_bindings(
                    bindings.len(),
                    draw,
                    &pipeline_bindings,
                )?,
            );
        }

        Ok(Self {
            producer_draw_count: producer_draws.producer_draw_count,
            uniform_binding_count: bindings.len(),
            render_var0_contract_count: bindings.len(),
            clear_scalar_contract_count: bindings.len(),
            morph_texture_contract_count: bindings.len(),
            slot0_slot1_sample_contract_count: bindings.len(),
            bindings,
            command_order: producer_uniform_command_order(),
        })
    }

    fn empty() -> Self {
        Self {
            producer_draw_count: 0,
            uniform_binding_count: 0,
            render_var0_contract_count: 0,
            clear_scalar_contract_count: 0,
            morph_texture_contract_count: 0,
            slot0_slot1_sample_contract_count: 0,
            bindings: Vec::new(),
            command_order: producer_uniform_command_order(),
        }
    }

    pub(in crate::renderer::native_vulkan) fn uniform_for_producer_draw(
        &self,
        producer_draw_index: usize,
    ) -> Option<&NativeVulkanSceneLayerAlphaMaskProducerUniformBindingPlan> {
        self.bindings
            .iter()
            .find(|binding| binding.producer_draw_index == producer_draw_index)
    }
}

impl NativeVulkanSceneLayerAlphaMaskProducerUniformBindingPlan {
    fn from_draw_and_pipeline_bindings(
        uniform_binding_index: usize,
        draw: &NativeVulkanSceneLayerAlphaMaskProducerDrawPlan,
        pipeline_bindings: &[&NativeVulkanSceneLayerAlphaMaskProducerPipelineBindingPlan],
    ) -> Result<Self, String> {
        validate_producer_uniform_draw(draw, pipeline_bindings)?;
        let heap_bind_indices = pipeline_bindings
            .iter()
            .map(|binding| binding.heap_bind_index)
            .collect::<Vec<_>>();
        Ok(Self {
            uniform_binding_index,
            producer_draw_index: draw.producer_draw_index,
            command_index: draw.command_index,
            object: draw.object,
            shader: CLIPPINGMASKIMAGE4_SHADER,
            target: draw.target,
            target_byte: draw.target_byte,
            clear_first: draw.clear_first,
            target_scope_load_op: draw.target_scope_load_op,
            heap_bind_indices,
            pipeline_binding_count: pipeline_bindings.len(),
            texture_slot_mask: CLIPPINGMASKIMAGE4_REQUIRED_TEXTURE_SLOT_MASK,
            optional_morph_texture_slot: CLIPPINGMASKIMAGE4_MORPH_TEXTURE_SLOT,
            render_var0_uniform: "g_RenderVar0",
            render_var0_component: "x",
            render_var0_value_source: "subdraw+0x44 bit 0x2 -> 0.0/1.0",
            render_var0_invert_flag_mask: RENDER_VAR0_INVERT_FLAG_MASK,
            render_var0_formula: "r = mix(r, 1 - r, g_RenderVar0.x)",
            state_render_var0_mirror_offset: STATE_RENDER_VAR0_MIRROR_OFFSET,
            clear_scalar_source: "state+0x1518.vtable+0x118 stores wrapper clear floats; +0x120 emits clear when requested",
            clear_setter_vtable_offset: CLEAR_SETTER_VTABLE_OFFSET,
            clear_emit_vtable_offset: CLEAR_EMIT_VTABLE_OFFSET,
            slot0_sample_source: "wrapper+0xd0 from caller material source texture -> g_Texture0",
            slot1_sample_source: "wrapper+0xd8 from subdraw+0x38 resolved mask texture -> g_Texture1",
            slot5_morph_texture_source: "wrapper+0xf8 from [layer+0x4b8]+0x418 -> g_Texture5",
            slot5_morph_enable_condition: "MORPHING combo == 1",
            alpha_formula: "mix(pow(texture0.a, 4), texture0.a, texture1.r)",
            red_formula: "texture1.r * alpha, then inverted by g_RenderVar0.x when subdraw+0x44 bit 0x2 is set",
            reference_points: [
                "reverse-engineered/docs/exe/clipping-pipeline.md: clippingmaskimage4 formula and g_RenderVar0.x",
                "reverse-engineered/docs/exe/clipping-pipeline.md: 0x14020d7b8..0x14020d811 slot0/slot1/slot5 writes",
                "reverse-engineered/docs/exe/composelayer-and-effecttarget.md: 0x14020d731..0x14020d798 clear scalar",
                "reverse-engineered/docs/exe/d3d11-context-calls.md: 0x14020d6a0 target byte and clear-first behavior",
            ],
            command_order: [
                "validate_clippingmaskimage4_producer_pipeline_bindings",
                "pin_slot0_source_and_slot1_mask_texture_sources",
                "pin_render_var0_x_from_subdraw_flag_0x44_bit_0x2",
                "pin_clear_scalar_wrapper_0x118_0x120",
                "preserve_morph_slot5_morphing_condition",
                "preserve_mask_alpha_and_red_formulas",
                "expose_producer_uniform_contract_to_requirements",
                "defer_actual_uniform_buffer_write_to_rt_method_8_recorder",
            ],
        })
    }
}

fn validate_producer_uniform_draw(
    draw: &NativeVulkanSceneLayerAlphaMaskProducerDrawPlan,
    pipeline_bindings: &[&NativeVulkanSceneLayerAlphaMaskProducerPipelineBindingPlan],
) -> Result<(), String> {
    if draw.shader != CLIPPINGMASKIMAGE4_SHADER
        || draw.texture_slot_mask != CLIPPINGMASKIMAGE4_REQUIRED_TEXTURE_SLOT_MASK
        || draw.optional_morph_texture_slot != CLIPPINGMASKIMAGE4_MORPH_TEXTURE_SLOT
    {
        return Err(format!(
            "scene layer alpha-mask producer command {} uniform contract requires clippingmaskimage4 slot0/slot1 and optional slot5, got shader {} mask {:#x} morph slot {}",
            draw.command_index,
            draw.shader,
            draw.texture_slot_mask,
            draw.optional_morph_texture_slot
        ));
    }
    let heap_bind_indices = pipeline_bindings
        .iter()
        .map(|binding| binding.heap_bind_index)
        .collect::<Vec<_>>();
    if heap_bind_indices != draw.heap_bind_indices {
        return Err(format!(
            "scene layer alpha-mask producer command {} uniform contract heap-bind list drifted: draw {:?}, pipeline {:?}",
            draw.command_index, draw.heap_bind_indices, heap_bind_indices
        ));
    }
    for binding in pipeline_bindings {
        if binding.command_index != draw.command_index
            || binding.object != draw.object
            || binding.shader != CLIPPINGMASKIMAGE4_SHADER
            || binding.target != draw.target
            || binding.texture_slot_mask != CLIPPINGMASKIMAGE4_REQUIRED_TEXTURE_SLOT_MASK
            || binding.optional_morph_texture_slot != CLIPPINGMASKIMAGE4_MORPH_TEXTURE_SLOT
        {
            return Err(format!(
                "scene layer alpha-mask producer command {} uniform contract drifted from pipeline binding {}",
                draw.command_index, binding.heap_bind_index
            ));
        }
    }
    Ok(())
}

fn producer_uniform_command_order() -> [&'static str; 6] {
    [
        "read_clippingmaskimage4_producer_draws",
        "read_clippingmaskimage4_pipeline_heap_bindings",
        "pin_render_var0_x_invert_scalar_contract",
        "pin_clear_scalar_wrapper_contract",
        "pin_slot0_slot1_slot5_texture_contract",
        "defer_uniform_write_to_rt_method_8_recorder",
    ]
}

#[cfg(test)]
#[path = "producer_uniform_tests.rs"]
mod tests;
