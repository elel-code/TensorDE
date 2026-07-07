//! Uniform and projected-screen-UV contract for generated `CLIPPINGTARGET` draws.
//!
//! References:
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/renderer_rd/shader_rd.cpp`
//! - `references/godot/servers/rendering/rendering_device_graph.h`

use serde::Serialize;

use crate::engine::scene_engine::{SceneGraphTarget, SceneObjectId};

use super::consumer_command::{
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerCommandPlan,
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan,
};
use super::consumer_draws::GENERATED_CLIPPINGTARGET_SHADER;

const ACTIVE_CLIPPING_MAX_COUNT: u32 = 0x0b;
const ACTIVE_CLIPPING_COUNT_STATE_OFFSET: u32 = 0x12ea;
const ACTIVE_CLIPPING_RAW_DWORD_STATE_OFFSET: u32 = 0x1330;
const ACTIVE_CLIPPING_INDEX_STATE_OFFSET: u32 = 0x1334;
const ACTIVE_CLIPPING_WEIGHT_STATE_OFFSET: u32 = 0x1360;
const ACTIVE_CLIPPING_TRANSFORM_STATE_OFFSET: u32 = 0x0cb0;
const ACTIVE_CLIPPING_OPTIONAL_FLAG_STATE_OFFSET: u32 = 0x138c;
const ACTIVE_CLIPPING_OPTIONAL_FLOAT_STATE_OFFSET: u32 = 0x1390;
const ACTIVE_CLIPPING_BITSET_LAYER_AUX_OFFSET: u32 = 0x0398;
const ACTIVE_CLIPPING_WEIGHT_LAYER_AUX_OFFSET: u32 = 0x03a0;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskGeneratedConsumerUniformPlan
{
    pub consumer_draw_count: usize,
    pub uniform_binding_count: usize,
    pub screen_uv_contract_count: usize,
    pub active_clipping_upload_contract_count: usize,
    pub slot8_alpha_sample_count: usize,
    pub bindings: Vec<NativeVulkanSceneLayerAlphaMaskGeneratedConsumerUniformBindingPlan>,
    pub command_order: [&'static str; 6],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskGeneratedConsumerUniformBindingPlan
{
    pub uniform_binding_index: usize,
    pub consumer_draw_index: usize,
    pub command_index: usize,
    pub object: SceneObjectId,
    pub shader: &'static str,
    pub shader_combo_values: Vec<String>,
    pub source_mask: SceneGraphTarget,
    pub color_target: SceneGraphTarget,
    pub material_source: &'static str,
    pub projected_position_source: &'static str,
    pub projected_y_transform: &'static str,
    pub screen_uv_formula: &'static str,
    pub alpha_mask_sample: &'static str,
    pub alpha_apply_formula: &'static str,
    pub effective_alpha_formula: &'static str,
    pub material_uniform_buffer_handle: u64,
    pub material_uniform_device_address: u64,
    pub material_uniform_bytes: u64,
    pub material_uniform_payload_hash: u64,
    pub active_clipping_max_count: u32,
    pub active_clipping_count_state_offset: u32,
    pub active_clipping_raw_dword_state_offset: u32,
    pub active_clipping_index_state_offset: u32,
    pub active_clipping_weight_state_offset: u32,
    pub active_clipping_transform_state_offset: u32,
    pub active_clipping_optional_flag_state_offset: u32,
    pub active_clipping_optional_float_state_offset: u32,
    pub active_clipping_bitset_layer_aux_offset: u32,
    pub active_clipping_weight_layer_aux_offset: u32,
    pub gpu_uniform_upload_status: &'static str,
    pub reference_points: [&'static str; 3],
    pub command_order: [&'static str; 8],
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_uniforms(
    commands: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan,
) -> Result<NativeVulkanSceneLayerAlphaMaskGeneratedConsumerUniformPlan, String> {
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerUniformPlan::from_commands(commands)
}

impl NativeVulkanSceneLayerAlphaMaskGeneratedConsumerUniformPlan {
    fn from_commands(
        commands: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan,
    ) -> Result<Self, String> {
        if commands.commands.is_empty() {
            return Ok(Self::empty());
        }

        let mut bindings = Vec::with_capacity(commands.commands.len());
        for command in &commands.commands {
            bindings.push(
                NativeVulkanSceneLayerAlphaMaskGeneratedConsumerUniformBindingPlan::from_command(
                    bindings.len(),
                    command,
                )?,
            );
        }
        Ok(Self {
            consumer_draw_count: commands.command_count,
            uniform_binding_count: bindings.len(),
            screen_uv_contract_count: bindings.len(),
            active_clipping_upload_contract_count: bindings.len(),
            slot8_alpha_sample_count: bindings.len(),
            bindings,
            command_order: generated_consumer_uniform_command_order(),
        })
    }

    fn empty() -> Self {
        Self {
            consumer_draw_count: 0,
            uniform_binding_count: 0,
            screen_uv_contract_count: 0,
            active_clipping_upload_contract_count: 0,
            slot8_alpha_sample_count: 0,
            bindings: Vec::new(),
            command_order: generated_consumer_uniform_command_order(),
        }
    }

    pub(in crate::renderer::native_vulkan) fn uniform_for_consumer_draw(
        &self,
        consumer_draw_index: usize,
    ) -> Option<&NativeVulkanSceneLayerAlphaMaskGeneratedConsumerUniformBindingPlan> {
        self.bindings
            .iter()
            .find(|binding| binding.consumer_draw_index == consumer_draw_index)
    }
}

impl NativeVulkanSceneLayerAlphaMaskGeneratedConsumerUniformBindingPlan {
    fn from_command(
        uniform_binding_index: usize,
        command: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerCommandPlan,
    ) -> Result<Self, String> {
        validate_generated_consumer_command_for_uniforms(command)?;
        Ok(Self {
            uniform_binding_index,
            consumer_draw_index: command.consumer_draw_index,
            command_index: command.command_index,
            object: command.object,
            shader: GENERATED_CLIPPINGTARGET_SHADER,
            shader_combo_values: command.shader_combo_values.clone(),
            source_mask: command.source_mask,
            color_target: command.color_target,
            material_source: command.material_source,
            projected_position_source: "genericimage4.vert CLIPPINGUVS emits gl_Position.xyw",
            projected_y_transform: "genericimage4.vert flips v_ScreenPos.y",
            screen_uv_formula: "(v_ScreenPos.xy / v_ScreenPos.z) * 0.5 + 0.5",
            alpha_mask_sample: "texSample2D(g_Texture8, screenUV).r",
            alpha_apply_formula: "gl_FragColor.a *= texSample2D(g_Texture8, screenUV).r",
            effective_alpha_formula: command.effective_alpha_formula,
            material_uniform_buffer_handle: command.material_uniform_buffer_handle,
            material_uniform_device_address: command.material_uniform_device_address,
            material_uniform_bytes: command.material_uniform_bytes,
            material_uniform_payload_hash: command.material_uniform_payload_hash,
            active_clipping_max_count: ACTIVE_CLIPPING_MAX_COUNT,
            active_clipping_count_state_offset: ACTIVE_CLIPPING_COUNT_STATE_OFFSET,
            active_clipping_raw_dword_state_offset: ACTIVE_CLIPPING_RAW_DWORD_STATE_OFFSET,
            active_clipping_index_state_offset: ACTIVE_CLIPPING_INDEX_STATE_OFFSET,
            active_clipping_weight_state_offset: ACTIVE_CLIPPING_WEIGHT_STATE_OFFSET,
            active_clipping_transform_state_offset: ACTIVE_CLIPPING_TRANSFORM_STATE_OFFSET,
            active_clipping_optional_flag_state_offset: ACTIVE_CLIPPING_OPTIONAL_FLAG_STATE_OFFSET,
            active_clipping_optional_float_state_offset:
                ACTIVE_CLIPPING_OPTIONAL_FLOAT_STATE_OFFSET,
            active_clipping_bitset_layer_aux_offset: ACTIVE_CLIPPING_BITSET_LAYER_AUX_OFFSET,
            active_clipping_weight_layer_aux_offset: ACTIVE_CLIPPING_WEIGHT_LAYER_AUX_OFFSET,
            gpu_uniform_upload_status: "retained generated-material uniform buffer resolved from +0x428 state",
            reference_points: [
                "reverse-engineered/docs/exe/clipping-pipeline.md: genericimage4 CLIPPINGUVS projected screen UV formula",
                "reverse-engineered/docs/exe/clipping-pipeline.md: CLIPPINGTARGET consumes g_Texture8 red channel",
                "reverse-engineered/docs/exe/clipping-pipeline.md: active clipping uniform upload at 0x14020cff0",
            ],
            command_order: [
                "validate_genericimage4_clippingtarget_command",
                "pin_clippinguvs_projected_position_formula",
                "pin_clippingtarget_slot8_alpha_sample_formula",
                "pin_active_clipping_state_offsets",
                "preserve_generated_material_0x428_uniform_source",
                "preserve_token1_effective_alpha_formula",
                "bind_retained_generated_material_uniform_buffer",
                "expose_uniform_contract_to_recorder_requirements",
            ],
        })
    }
}

fn validate_generated_consumer_command_for_uniforms(
    command: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerCommandPlan,
) -> Result<(), String> {
    if command.shader != GENERATED_CLIPPINGTARGET_SHADER {
        return Err(format!(
            "scene layer alpha-mask generated consumer uniform contract requires {}, got {}",
            GENERATED_CLIPPINGTARGET_SHADER, command.shader
        ));
    }
    if !command
        .shader_combo_values
        .iter()
        .any(|combo| combo == "CLIPPINGTARGET=1")
        || !command
            .shader_combo_values
            .iter()
            .any(|combo| combo == "CLIPPINGUVS=1")
    {
        return Err(format!(
            "scene layer alpha-mask generated consumer uniform contract requires CLIPPINGTARGET=1 and CLIPPINGUVS=1, got {:?}",
            command.shader_combo_values
        ));
    }
    if command.source_mask != SceneGraphTarget::FullAlphaMask {
        return Err(format!(
            "scene layer alpha-mask generated consumer uniform contract command {} must sample FullAlphaMask",
            command.command_index
        ));
    }
    if command.texture_count != 2 || command.resource_descriptor_count < 3 {
        return Err(format!(
            "scene layer alpha-mask generated consumer uniform contract command {} requires material uniform plus slot0 source and slot8 FullAlphaMask sampled images",
            command.command_index
        ));
    }
    if command.material_uniform_buffer_handle == 0
        || command.material_uniform_device_address == 0
        || command.material_uniform_bytes == 0
    {
        return Err(format!(
            "scene layer alpha-mask generated consumer uniform contract command {} requires retained generated material uniform buffer",
            command.command_index
        ));
    }
    Ok(())
}

fn generated_consumer_uniform_command_order() -> [&'static str; 6] {
    [
        "read_generated_clippingtarget_command_plan",
        "validate_clippingtarget_and_clippinguvs_shader_combos",
        "pin_projected_screen_uv_shader_formula",
        "pin_active_clipping_uniform_state_offsets",
        "preserve_generated_material_0x428_uniform_source",
        "bind_retained_generated_material_uniform_buffer",
    ]
}

#[cfg(test)]
#[path = "consumer_uniform_tests.rs"]
mod tests;
