//! Pipeline and heap-bind contracts for generated `CLIPPINGTARGET` consumer draws.
//!
//! References:
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/renderer_rd/shader_rd.cpp`
//! - `references/godot/servers/rendering/renderer_rd/pipeline_hash_map_rd.h`

use serde::Serialize;

use crate::engine::scene_engine::{
    SceneAlphaWriteMode, SceneBlendContract, SceneCullMode, SceneGraphPipelineClass,
    SceneGraphTarget, SceneLayerCompositorTarget, SceneMaterialRenderState, SceneObjectId,
};
use crate::renderer::native_vulkan::scene_backend::pipeline::{
    NativeVulkanScenePipelineCacheKey, NativeVulkanScenePipelineShaderComboValue,
    NativeVulkanScenePipelineVertexLayout, native_vulkan_scene_pipeline_shader_combo_values,
};
use crate::renderer::native_vulkan::scene_backend::texture_descriptors::NativeVulkanSceneTextureDescriptorVkFormat;

use super::CLIPPINGTARGET_TEXTURE_SLOT_MASK;
use super::consumer_draws::{
    GENERATED_CLIPPINGTARGET_SHADER,
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawBindingPlan,
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawRuntimePlan,
};
use super::consumer_target::{
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetBindingPlan,
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetPlan,
};
use super::rt_method8::LAYER_490_RT_METHOD8_GEOMETRY_SOURCE;

const GENERATED_CLIPPINGTARGET_SHADER_COMBOS: [(&str, u32); 2] =
    [("CLIPPINGTARGET", 1), ("CLIPPINGUVS", 1)];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelinePlan
{
    pub consumer_draw_count: usize,
    pub pipeline_binding_count: usize,
    pub cache_key_count: usize,
    pub texture_slot_mask: u32,
    pub shader_combo_override_count: usize,
    pub bindings: Vec<NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelineBindingPlan>,
    pub command_order: [&'static str; 6],
    #[serde(skip)]
    cache_keys: Vec<NativeVulkanScenePipelineCacheKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelineBindingPlan
{
    pub consumer_draw_index: usize,
    pub command_index: usize,
    pub object: SceneObjectId,
    pub shader: &'static str,
    pub shader_combo_values: Vec<NativeVulkanScenePipelineShaderComboValue>,
    pub source_mask: SceneGraphTarget,
    pub target: SceneLayerCompositorTarget,
    pub target_format: NativeVulkanSceneTextureDescriptorVkFormat,
    pub pipeline_class: SceneGraphPipelineClass,
    pub vertex_layout: NativeVulkanScenePipelineVertexLayout,
    pub blend: SceneBlendContract,
    pub cull_mode: SceneCullMode,
    pub alpha_write: SceneAlphaWriteMode,
    pub color_write_mask: &'static str,
    pub texture_slot_mask: u32,
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
    pub shader_mappings: Vec<String>,
    pub material_source: &'static str,
    pub blend_byte_source: &'static str,
    pub geometry_source: &'static str,
    pub command_order: [&'static str; 6],
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_pipelines(
    consumer_draws: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawRuntimePlan,
    target_format: NativeVulkanSceneTextureDescriptorVkFormat,
    pipeline_class: SceneGraphPipelineClass,
) -> Result<NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelinePlan, String> {
    let targets = NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetPlan {
        consumer_draw_count: consumer_draws.consumer_draw_count,
        target_binding_count: consumer_draws.bindings.len(),
        color_target_count: usize::from(!consumer_draws.bindings.is_empty()),
        bindings: consumer_draws
            .bindings
            .iter()
            .map(|consumer| {
                Ok(
                    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetBindingPlan {
                        consumer_draw_index: consumer.consumer_draw_index,
                        command_index: consumer.command_index,
                        object: consumer.object,
                        draw_receiver: consumer.target,
                        color_target: SceneGraphTarget::Swapchain,
                        target_format,
                        target_format_label: target_format_label(target_format),
                        width: 1,
                        height: 1,
                        pipeline_class,
                        vertex_layout: generated_consumer_vertex_layout(pipeline_class)?,
                        geometry_source: LAYER_490_RT_METHOD8_GEOMETRY_SOURCE,
                        command_order: [
                            "resolve_layer_0x490_rt_draw_receiver",
                            "keep_color_target_separate_from_layer_receiver",
                            "read_current_layer_color_target_format",
                            "validate_generated_consumer_color_attachment_format",
                            "select_scene_mesh_vertex_layout_for_layer_0x490",
                            "preserve_0x14020b15e_geometry_creation_site",
                        ],
                    },
                )
            })
            .collect::<Result<Vec<_>, String>>()?,
        command_order: [
            "read_generated_clippingtarget_draw_contracts",
            "resolve_layer_0x490_color_target",
            "validate_color_target_format_and_extent",
            "preserve_layer_0x490_rt_method_8_receiver",
            "select_mesh_or_puppet_pipeline_class",
            "produce_generated_consumer_target_plan",
        ],
    };
    native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_pipelines_from_targets(
        consumer_draws,
        &targets,
    )
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_pipelines_from_targets(
    consumer_draws: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawRuntimePlan,
    targets: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetPlan,
) -> Result<NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelinePlan, String> {
    if consumer_draws.bindings.is_empty() {
        return Ok(NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelinePlan::empty());
    }
    if consumer_draws.consumer_draw_count != targets.consumer_draw_count
        || consumer_draws.bindings.len() != targets.target_binding_count
    {
        return Err(format!(
            "scene layer alpha-mask generated consumer pipeline expected {} target bindings for {} draws, got {} / {}",
            consumer_draws.bindings.len(),
            consumer_draws.consumer_draw_count,
            targets.target_binding_count,
            targets.consumer_draw_count
        ));
    }

    let mut bindings = Vec::with_capacity(consumer_draws.bindings.len());
    let mut cache_keys = Vec::new();
    for consumer in &consumer_draws.bindings {
        let target = targets
            .target_for_consumer_draw(consumer.consumer_draw_index)
            .ok_or_else(|| {
                format!(
                    "scene layer alpha-mask generated consumer command {} has no LayerTarget490 target binding",
                    consumer.command_index
                )
            })?;
        let binding = generated_consumer_pipeline_binding(consumer, target)?;
        let cache_key = binding.cache_key();
        if !cache_keys.iter().any(|existing| existing == &cache_key) {
            cache_keys.push(cache_key);
        }
        bindings.push(binding);
    }

    Ok(
        NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelinePlan::from_bindings(
            consumer_draws.consumer_draw_count,
            bindings,
            cache_keys,
        ),
    )
}

impl NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelinePlan {
    pub(in crate::renderer::native_vulkan) fn cache_keys(
        &self,
    ) -> &[NativeVulkanScenePipelineCacheKey] {
        &self.cache_keys
    }

    fn empty() -> Self {
        Self {
            consumer_draw_count: 0,
            pipeline_binding_count: 0,
            cache_key_count: 0,
            texture_slot_mask: 0,
            shader_combo_override_count: 0,
            bindings: Vec::new(),
            cache_keys: Vec::new(),
            command_order: generated_consumer_pipeline_command_order(),
        }
    }

    fn from_bindings(
        consumer_draw_count: usize,
        bindings: Vec<NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelineBindingPlan>,
        cache_keys: Vec<NativeVulkanScenePipelineCacheKey>,
    ) -> Self {
        let shader_combo_override_count = bindings
            .iter()
            .map(|binding| binding.shader_combo_values.len())
            .sum();
        Self {
            consumer_draw_count,
            pipeline_binding_count: bindings.len(),
            cache_key_count: cache_keys.len(),
            texture_slot_mask: bindings
                .iter()
                .fold(0u32, |mask, binding| mask | binding.texture_slot_mask),
            shader_combo_override_count,
            bindings,
            cache_keys,
            command_order: generated_consumer_pipeline_command_order(),
        }
    }
}

impl NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelineBindingPlan {
    pub(in crate::renderer::native_vulkan) fn cache_key(
        &self,
    ) -> NativeVulkanScenePipelineCacheKey {
        NativeVulkanScenePipelineCacheKey {
            shader: self.shader.to_owned(),
            shader_combo_values: self.shader_combo_values.clone(),
            blend: self.blend,
            render_state: SceneMaterialRenderState {
                cull_mode: self.cull_mode,
                alpha_write: self.alpha_write,
                ..SceneMaterialRenderState::translucent_2d()
            },
            pipeline_class: self.pipeline_class,
            vertex_layout: self.vertex_layout,
            target_format: self.target_format.to_vk_format(),
            texture_slot_mask: self.texture_slot_mask,
        }
    }
}

fn generated_consumer_pipeline_binding(
    consumer: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawBindingPlan,
    target: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetBindingPlan,
) -> Result<NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelineBindingPlan, String> {
    validate_generated_consumer_draw_for_pipeline(consumer)?;
    validate_generated_consumer_target_for_pipeline(consumer, target)?;
    Ok(
        NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelineBindingPlan {
            consumer_draw_index: consumer.consumer_draw_index,
            command_index: consumer.command_index,
            object: consumer.object,
            shader: GENERATED_CLIPPINGTARGET_SHADER,
            shader_combo_values: native_vulkan_scene_pipeline_shader_combo_values(
                &GENERATED_CLIPPINGTARGET_SHADER_COMBOS,
            ),
            source_mask: consumer.source_mask,
            target: consumer.target,
            target_format: target.target_format,
            pipeline_class: target.pipeline_class,
            vertex_layout: target.vertex_layout,
            blend: SceneBlendContract::TranslucentAlpha,
            cull_mode: SceneCullMode::Back,
            alpha_write: SceneAlphaWriteMode::Disabled,
            color_write_mask: "RGB",
            texture_slot_mask: CLIPPINGTARGET_TEXTURE_SLOT_MASK,
            heap_bind_index: consumer.heap_bind_index,
            heap_slice_index: consumer.heap_slice_index,
            base_resource_descriptor_index: consumer.base_resource_descriptor_index,
            base_sampler_descriptor_index: consumer.base_sampler_descriptor_index,
            resource_descriptor_count: consumer.resource_descriptor_count,
            texture_count: consumer.texture_count,
            material_uniform_buffer_handle: consumer.material_uniform_buffer_handle,
            material_uniform_device_address: consumer.material_uniform_device_address,
            material_uniform_bytes: consumer.material_uniform_bytes,
            material_uniform_payload_hash: consumer.material_uniform_payload_hash,
            shader_mappings: consumer.shader_mappings.clone(),
            material_source: consumer.generated_material_source,
            blend_byte_source: consumer.blend_byte_source,
            geometry_source: LAYER_490_RT_METHOD8_GEOMETRY_SOURCE,
            command_order: [
                "read_generated_clippingtarget_draw_contract",
                "require_clippinguvs_and_clippingtarget_shader_combos",
                "validate_slot0_source_and_slot8_full_alpha_mask",
                "select_resolved_layer_0x490_color_target_format",
                "preserve_layer_0x490_rt_method_8_geometry_source",
                "derive_rgb_only_alpha_over_back_cull_generated_clippingtarget_pipeline_key",
            ],
        },
    )
}

fn validate_generated_consumer_draw_for_pipeline(
    consumer: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawBindingPlan,
) -> Result<(), String> {
    if consumer.shader != GENERATED_CLIPPINGTARGET_SHADER {
        return Err(format!(
            "scene layer alpha-mask generated consumer pipeline requires {}, got {}",
            GENERATED_CLIPPINGTARGET_SHADER, consumer.shader
        ));
    }
    if consumer.source_mask != SceneGraphTarget::FullAlphaMask
        || consumer.target != SceneLayerCompositorTarget::LayerTarget490
    {
        return Err(format!(
            "scene layer alpha-mask generated consumer pipeline requires FullAlphaMask -> LayerTarget490, got {:?} -> {:?}",
            consumer.source_mask, consumer.target
        ));
    }
    if consumer.texture_slot_mask != CLIPPINGTARGET_TEXTURE_SLOT_MASK
        || consumer.required_texture_slots != [0, 8]
    {
        return Err(format!(
            "scene layer alpha-mask generated consumer pipeline requires g_Texture0/g_Texture8, got mask {:#x} slots {:?}",
            consumer.texture_slot_mask, consumer.required_texture_slots
        ));
    }
    if consumer.texture_count != 2 || consumer.resource_descriptor_count < 3 {
        return Err(format!(
            "scene layer alpha-mask generated consumer pipeline requires generated material uniform plus two sampled images, got textures={} resources={}",
            consumer.texture_count, consumer.resource_descriptor_count
        ));
    }
    if consumer.material_uniform_buffer_handle == 0
        || consumer.material_uniform_device_address == 0
        || consumer.material_uniform_bytes == 0
    {
        return Err(format!(
            "scene layer alpha-mask generated consumer pipeline requires retained generated material uniform buffer for command {}",
            consumer.command_index
        ));
    }
    Ok(())
}

fn generated_consumer_vertex_layout(
    pipeline_class: SceneGraphPipelineClass,
) -> Result<NativeVulkanScenePipelineVertexLayout, String> {
    match pipeline_class {
        SceneGraphPipelineClass::Mesh | SceneGraphPipelineClass::PuppetSkinning => {
            Ok(NativeVulkanScenePipelineVertexLayout::SceneMeshV0)
        }
        pipeline_class => Err(format!(
            "scene layer alpha-mask generated consumer pipeline requires mesh/subdraw geometry, got {:?}",
            pipeline_class
        )),
    }
}

fn validate_generated_consumer_target_for_pipeline(
    consumer: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawBindingPlan,
    target: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetBindingPlan,
) -> Result<(), String> {
    if consumer.consumer_draw_index != target.consumer_draw_index
        || consumer.command_index != target.command_index
        || consumer.object != target.object
        || consumer.target != target.draw_receiver
    {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {} target binding does not match draw contract",
            consumer.command_index
        ));
    }
    validate_generated_consumer_target_format(target.target_format)?;
    generated_consumer_vertex_layout(target.pipeline_class)?;
    Ok(())
}

fn target_format_label(target_format: NativeVulkanSceneTextureDescriptorVkFormat) -> &'static str {
    match target_format {
        NativeVulkanSceneTextureDescriptorVkFormat::R16G16B16A16Sfloat => "R16G16B16A16_SFLOAT",
        NativeVulkanSceneTextureDescriptorVkFormat::R16G16Sfloat => "R16G16_SFLOAT",
        NativeVulkanSceneTextureDescriptorVkFormat::R8G8B8A8Unorm => "R8G8B8A8_UNORM",
        NativeVulkanSceneTextureDescriptorVkFormat::B8G8R8A8Unorm => "B8G8R8A8_UNORM",
        NativeVulkanSceneTextureDescriptorVkFormat::R16Sfloat => "R16_SFLOAT",
        NativeVulkanSceneTextureDescriptorVkFormat::R8Unorm => "R8_UNORM",
    }
}

fn validate_generated_consumer_target_format(
    target_format: NativeVulkanSceneTextureDescriptorVkFormat,
) -> Result<(), String> {
    match target_format {
        NativeVulkanSceneTextureDescriptorVkFormat::R8G8B8A8Unorm
        | NativeVulkanSceneTextureDescriptorVkFormat::B8G8R8A8Unorm
        | NativeVulkanSceneTextureDescriptorVkFormat::R16G16B16A16Sfloat => Ok(()),
        target_format => Err(format!(
            "scene layer alpha-mask generated consumer pipeline requires color layer target format, got {:?}",
            target_format
        )),
    }
}

fn generated_consumer_pipeline_command_order() -> [&'static str; 6] {
    [
        "read_generated_clippingtarget_draws",
        "derive_we_shader_combo_variant",
        "require_color_layer_target_format",
        "preserve_generated_material_0x428",
        "preserve_subdraw_blend_byte_to_material_0x1f0",
        "derive_unique_generated_clippingtarget_pipeline_cache_keys",
    ]
}

#[cfg(test)]
#[path = "consumer_pipeline_tests.rs"]
mod tests;
