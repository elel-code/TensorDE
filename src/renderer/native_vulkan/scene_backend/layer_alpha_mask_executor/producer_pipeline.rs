//! Pipeline and heap-bind contracts for WE `clippingmaskimage4` producer draws.
//!
//! References:
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/renderer_rd/pipeline_hash_map_rd.h`

use serde::Serialize;
use vulkanalia::vk;

use crate::engine::scene_engine::{
    SceneBlendContract, SceneGraphPipelineClass, SceneGraphTarget, SceneLayerCompositorOperation,
    SceneMaterialRenderState, SceneObjectId,
};
use crate::renderer::native_vulkan::scene_backend::pipeline::{
    NativeVulkanScenePipelineCacheKey, NativeVulkanScenePipelineVertexLayout,
};
use crate::renderer::native_vulkan::scene_backend::texture_descriptors::NativeVulkanSceneTextureDescriptorVkFormat;

use super::producer_draws::{
    CLIPPINGMASKIMAGE4_MATERIAL, CLIPPINGMASKIMAGE4_SHADER,
    NativeVulkanSceneLayerAlphaMaskProducerDrawPlan,
    NativeVulkanSceneLayerAlphaMaskProducerDrawRuntimePlan,
};
use super::resource_binds::{
    NativeVulkanSceneLayerAlphaMaskResourceBindCommandPlan,
    NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan,
};
use super::{
    CLIPPINGMASKIMAGE4_MORPH_TEXTURE_SLOT, CLIPPINGMASKIMAGE4_REQUIRED_TEXTURE_SLOT_MASK,
    NativeVulkanSceneLayerAlphaMaskTextureBindRole,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskProducerPipelinePlan {
    pub producer_draw_count: usize,
    pub pipeline_binding_count: usize,
    pub cache_key_count: usize,
    pub texture_slot_mask: u32,
    pub draw_requires_subdraw_selection_count: usize,
    pub bindings: Vec<NativeVulkanSceneLayerAlphaMaskProducerPipelineBindingPlan>,
    pub command_order: [&'static str; 6],
    #[serde(skip)]
    cache_keys: Vec<NativeVulkanScenePipelineCacheKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskProducerPipelineBindingPlan
{
    pub producer_draw_index: usize,
    pub command_index: usize,
    pub object: SceneObjectId,
    pub material: &'static str,
    pub shader: &'static str,
    pub target: SceneGraphTarget,
    pub target_format: NativeVulkanSceneTextureDescriptorVkFormat,
    pub pipeline_class: SceneGraphPipelineClass,
    pub vertex_layout: NativeVulkanScenePipelineVertexLayout,
    pub texture_slot_mask: u32,
    pub optional_morph_texture_slot: u32,
    pub heap_bind_index: usize,
    pub heap_slice_index: usize,
    pub base_resource_descriptor_index: usize,
    pub base_sampler_descriptor_index: usize,
    pub resource_descriptor_count: usize,
    pub texture_count: usize,
    pub clipping_record_index: u32,
    pub requires_subdraw_index_selection: bool,
    pub shader_mappings: Vec<String>,
    pub command_order: [&'static str; 6],
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_alpha_mask_producer_pipelines(
    producer_draws: &NativeVulkanSceneLayerAlphaMaskProducerDrawRuntimePlan,
    resource_binds: &NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan,
) -> Result<NativeVulkanSceneLayerAlphaMaskProducerPipelinePlan, String> {
    if producer_draws.draws.is_empty() {
        return Ok(NativeVulkanSceneLayerAlphaMaskProducerPipelinePlan::empty());
    }

    let mut bindings = Vec::new();
    let mut cache_keys = Vec::new();
    for draw in &producer_draws.draws {
        let cache_key = producer_pipeline_cache_key(draw)?;
        if !cache_keys.iter().any(|existing| existing == &cache_key) {
            cache_keys.push(cache_key);
        }
        let requires_subdraw_index_selection = draw.heap_bind_indices.len() > 1;
        for heap_bind_index in &draw.heap_bind_indices {
            let bind = resource_binds
                .binds
                .iter()
                .find(|bind| bind.heap_bind_index == *heap_bind_index)
                .ok_or_else(|| {
                    format!(
                        "scene layer alpha-mask producer command {} references missing heap bind {}",
                        draw.command_index, heap_bind_index
                    )
                })?;
            bindings.push(producer_pipeline_binding(
                draw,
                bind,
                requires_subdraw_index_selection,
            )?);
        }
    }

    Ok(
        NativeVulkanSceneLayerAlphaMaskProducerPipelinePlan::from_bindings(
            producer_draws.producer_draw_count,
            bindings,
            cache_keys,
        ),
    )
}

impl NativeVulkanSceneLayerAlphaMaskProducerPipelinePlan {
    pub(in crate::renderer::native_vulkan) fn cache_keys(
        &self,
    ) -> &[NativeVulkanScenePipelineCacheKey] {
        &self.cache_keys
    }

    fn empty() -> Self {
        Self {
            producer_draw_count: 0,
            pipeline_binding_count: 0,
            cache_key_count: 0,
            texture_slot_mask: 0,
            draw_requires_subdraw_selection_count: 0,
            bindings: Vec::new(),
            cache_keys: Vec::new(),
            command_order: producer_pipeline_command_order(),
        }
    }

    fn from_bindings(
        producer_draw_count: usize,
        bindings: Vec<NativeVulkanSceneLayerAlphaMaskProducerPipelineBindingPlan>,
        cache_keys: Vec<NativeVulkanScenePipelineCacheKey>,
    ) -> Self {
        let draw_requires_subdraw_selection_count = (0..producer_draw_count)
            .filter(|draw_index| {
                bindings.iter().any(|binding| {
                    binding.producer_draw_index == *draw_index
                        && binding.requires_subdraw_index_selection
                })
            })
            .count();
        Self {
            producer_draw_count,
            pipeline_binding_count: bindings.len(),
            cache_key_count: cache_keys.len(),
            texture_slot_mask: bindings
                .iter()
                .fold(0u32, |mask, binding| mask | binding.texture_slot_mask),
            draw_requires_subdraw_selection_count,
            bindings,
            cache_keys,
            command_order: producer_pipeline_command_order(),
        }
    }
}

impl NativeVulkanSceneLayerAlphaMaskProducerPipelineBindingPlan {
    pub(in crate::renderer::native_vulkan) fn cache_key(
        &self,
    ) -> NativeVulkanScenePipelineCacheKey {
        NativeVulkanScenePipelineCacheKey {
            shader: self.shader.to_owned(),
            blend: SceneBlendContract::TranslucentAlpha,
            render_state: SceneMaterialRenderState::translucent_2d(),
            pipeline_class: self.pipeline_class,
            vertex_layout: self.vertex_layout,
            target_format: vk::Format::R8_UNORM,
            texture_slot_mask: self.texture_slot_mask,
        }
    }
}

fn producer_pipeline_binding(
    draw: &NativeVulkanSceneLayerAlphaMaskProducerDrawPlan,
    bind: &NativeVulkanSceneLayerAlphaMaskResourceBindCommandPlan,
    requires_subdraw_index_selection: bool,
) -> Result<NativeVulkanSceneLayerAlphaMaskProducerPipelineBindingPlan, String> {
    validate_producer_heap_bind(draw, bind)?;
    let NativeVulkanSceneLayerAlphaMaskTextureBindRole::ClippingMaskImage4 {
        clipping_record_index,
    } = bind.role
    else {
        unreachable!("validate_producer_heap_bind requires clippingmaskimage4 role");
    };
    Ok(NativeVulkanSceneLayerAlphaMaskProducerPipelineBindingPlan {
        producer_draw_index: draw.producer_draw_index,
        command_index: draw.command_index,
        object: draw.object,
        material: draw.material,
        shader: draw.shader,
        target: draw.target,
        target_format: NativeVulkanSceneTextureDescriptorVkFormat::R8Unorm,
        pipeline_class: SceneGraphPipelineClass::PuppetSkinning,
        vertex_layout: NativeVulkanScenePipelineVertexLayout::SceneMeshV0,
        texture_slot_mask: CLIPPINGMASKIMAGE4_REQUIRED_TEXTURE_SLOT_MASK,
        optional_morph_texture_slot: CLIPPINGMASKIMAGE4_MORPH_TEXTURE_SLOT,
        heap_bind_index: bind.heap_bind_index,
        heap_slice_index: bind.bind.heap_slice_index,
        base_resource_descriptor_index: bind.bind.base_resource_descriptor_index,
        base_sampler_descriptor_index: bind.bind.base_sampler_descriptor_index,
        resource_descriptor_count: bind.bind.resource_descriptor_count,
        texture_count: bind.bind.texture_count,
        clipping_record_index,
        requires_subdraw_index_selection,
        shader_mappings: bind.bind.shader_mappings.clone(),
        command_order: [
            "read_clippingmaskimage4_producer_draw",
            "match_heap_bind_by_command_object_and_role",
            "validate_slot0_slot1_alpha_mask_texture_only_heap",
            "derive_clippingmaskimage4_puppet_pipeline_key",
            "preserve_clipping_record_index_for_token_subdraw_selection",
            "defer_exact_bind_choice_to_token_subdraw_index",
        ],
    })
}

fn validate_producer_heap_bind(
    draw: &NativeVulkanSceneLayerAlphaMaskProducerDrawPlan,
    bind: &NativeVulkanSceneLayerAlphaMaskResourceBindCommandPlan,
) -> Result<(), String> {
    if bind.object != draw.object {
        return Err(format!(
            "scene layer alpha-mask producer command {} object mismatch: draw {:?}, heap {:?}",
            draw.command_index, draw.object, bind.object
        ));
    }
    if bind.operation != SceneLayerCompositorOperation::DrawClippingMask {
        return Err(format!(
            "scene layer alpha-mask producer command {} requires DrawClippingMask heap bind, got {:?}",
            draw.command_index, bind.operation
        ));
    }
    if bind.shader != draw.shader {
        return Err(format!(
            "scene layer alpha-mask producer command {} shader mismatch: draw {}, heap {}",
            draw.command_index, draw.shader, bind.shader
        ));
    }
    if !matches!(
        bind.role,
        NativeVulkanSceneLayerAlphaMaskTextureBindRole::ClippingMaskImage4 { .. }
    ) {
        return Err(format!(
            "scene layer alpha-mask producer command {} requires ClippingMaskImage4 heap bind, got {:?}",
            draw.command_index, bind.role
        ));
    }
    if bind.bind.texture_count != 2 || bind.bind.resource_descriptor_count != 2 {
        return Err(format!(
            "scene layer alpha-mask producer command {} requires slot0/slot1 texture-only heap bind, got textures={} resources={}",
            draw.command_index, bind.bind.texture_count, bind.bind.resource_descriptor_count
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
    if slots != [0, 1] {
        return Err(format!(
            "scene layer alpha-mask producer command {} requires g_Texture0/g_Texture1 heap bind, got slots {:?}",
            draw.command_index, slots
        ));
    }
    Ok(())
}

fn producer_pipeline_cache_key(
    draw: &NativeVulkanSceneLayerAlphaMaskProducerDrawPlan,
) -> Result<NativeVulkanScenePipelineCacheKey, String> {
    if draw.material != CLIPPINGMASKIMAGE4_MATERIAL || draw.shader != CLIPPINGMASKIMAGE4_SHADER {
        return Err(format!(
            "scene layer alpha-mask producer command {} requires clippingmaskimage4 material/shader, got {} / {}",
            draw.command_index, draw.material, draw.shader
        ));
    }
    if draw.pipeline_class != SceneGraphPipelineClass::PuppetSkinning {
        return Err(format!(
            "scene layer alpha-mask producer command {} requires PuppetSkinning pipeline class, got {:?}",
            draw.command_index, draw.pipeline_class
        ));
    }
    if draw.target_format != "R8_UNORM"
        || draw.texture_slot_mask != CLIPPINGMASKIMAGE4_REQUIRED_TEXTURE_SLOT_MASK
    {
        return Err(format!(
            "scene layer alpha-mask producer command {} requires R8_UNORM slot0/slot1 pipeline key, got format {} mask {:#x}",
            draw.command_index, draw.target_format, draw.texture_slot_mask
        ));
    }
    Ok(NativeVulkanScenePipelineCacheKey {
        shader: draw.shader.to_owned(),
        blend: SceneBlendContract::TranslucentAlpha,
        render_state: SceneMaterialRenderState::translucent_2d(),
        pipeline_class: SceneGraphPipelineClass::PuppetSkinning,
        vertex_layout: NativeVulkanScenePipelineVertexLayout::SceneMeshV0,
        target_format: vk::Format::R8_UNORM,
        texture_slot_mask: CLIPPINGMASKIMAGE4_REQUIRED_TEXTURE_SLOT_MASK,
    })
}

fn producer_pipeline_command_order() -> [&'static str; 6] {
    [
        "read_clippingmaskimage4_producer_draws",
        "resolve_candidate_clippingmaskimage4_heap_binds",
        "validate_alpha_mask_texture_only_heap_shape",
        "derive_unique_clippingmaskimage4_pipeline_cache_keys",
        "preserve_clipping_record_indices",
        "mark_multi_record_draws_waiting_for_token_subdraw_index",
    ]
}

#[cfg(test)]
#[path = "producer_pipeline_tests.rs"]
mod tests;
