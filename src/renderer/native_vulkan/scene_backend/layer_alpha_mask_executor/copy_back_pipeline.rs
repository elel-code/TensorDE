//! Pipeline-key facts for WE alpha-mask flattexture copy-back draws.
//!
//! References:
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `artifacts/wallpaper-engine-workshop/steamcmd-root/assets/materials/util/flattexture.json`
//! - `artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/minimalalpha.frag`
//! - `artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/minimalalpha.vert`
//! - `references/godot/servers/rendering/renderer_rd/pipeline_hash_map_rd.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`

use serde::Serialize;
use vulkanalia::vk;

use crate::engine::scene_engine::{
    SceneBlendContract, SceneGraphPipelineClass, SceneGraphTarget, SceneLayerCompositorBlendKey,
    SceneMaterialRenderState, SceneObjectId,
};
use crate::renderer::native_vulkan::scene_backend::pipeline::{
    NativeVulkanScenePipelineCacheKey, NativeVulkanScenePipelineVertexLayout,
};
use crate::renderer::native_vulkan::scene_backend::texture_descriptors::{
    NativeVulkanSceneTextureDescriptorSource, NativeVulkanSceneTextureDescriptorVkFormat,
};

use super::copy_back::{
    NativeVulkanSceneLayerAlphaMaskCopyBackAlphaUniform,
    NativeVulkanSceneLayerAlphaMaskCopyBackDrawPlan,
};
use super::copy_back_geometry::FLATTEXTURE_COPY_BACK_RASTER_GEOMETRY;
use super::resource_binds::NativeVulkanSceneLayerAlphaMaskCopyBackDrawResourceBindPlan;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskCopyBackPipelinePlan {
    pub pipeline_count: usize,
    pub cache_key_count: usize,
    pub texture_slot_mask: u32,
    pub keys: Vec<NativeVulkanSceneLayerAlphaMaskCopyBackPipelineKeyPlan>,
    pub command_order: [&'static str; 5],
    #[serde(skip)]
    pub(in crate::renderer::native_vulkan) cache_keys: Vec<NativeVulkanScenePipelineCacheKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskCopyBackPipelineKeyPlan
{
    pub copy_back_draw_index: usize,
    pub command_index: usize,
    pub object: SceneObjectId,
    pub material: &'static str,
    pub shader: &'static str,
    pub source: SceneGraphTarget,
    pub target: SceneGraphTarget,
    pub target_format: NativeVulkanSceneTextureDescriptorVkFormat,
    pub texture_slot: u32,
    pub texture_slot_mask: u32,
    pub texture_source: NativeVulkanSceneTextureDescriptorSource,
    pub pipeline_class: SceneGraphPipelineClass,
    pub vertex_layout: NativeVulkanScenePipelineVertexLayout,
    pub blend_key: SceneLayerCompositorBlendKey,
    pub alpha_uniform: NativeVulkanSceneLayerAlphaMaskCopyBackAlphaUniform,
    pub heap_bind_index: usize,
    pub heap_slice_index: usize,
    pub base_resource_descriptor_index: usize,
    pub base_sampler_descriptor_index: usize,
    pub resource_descriptor_index: usize,
    pub sampler_descriptor_index: usize,
    pub shader_mapping: String,
    pub raster_geometry: &'static str,
    pub command_order: [&'static str; 6],
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_alpha_mask_copy_back_pipelines(
    copy_back_draws: &[NativeVulkanSceneLayerAlphaMaskCopyBackDrawPlan],
    copy_back_draw_binds: &[NativeVulkanSceneLayerAlphaMaskCopyBackDrawResourceBindPlan],
) -> Result<NativeVulkanSceneLayerAlphaMaskCopyBackPipelinePlan, String> {
    let mut keys = Vec::with_capacity(copy_back_draw_binds.len());
    let mut cache_keys = Vec::with_capacity(copy_back_draw_binds.len());
    let mut texture_slot_mask = 0u32;
    for draw_bind in copy_back_draw_binds {
        let draw = copy_back_draws
            .get(draw_bind.copy_back_draw_index)
            .ok_or_else(|| {
                format!(
                    "scene layer alpha-mask copy-back pipeline references missing draw index {}",
                    draw_bind.copy_back_draw_index
                )
            })?;
        validate_copy_back_draw_bind(draw, draw_bind)?;
        texture_slot_mask |= 1u32 << draw.texture_slot;
        let cache_key = copy_back_pipeline_cache_key(draw)?;
        keys.push(NativeVulkanSceneLayerAlphaMaskCopyBackPipelineKeyPlan {
            copy_back_draw_index: draw_bind.copy_back_draw_index,
            command_index: draw.command_index,
            object: draw.object,
            material: draw.material,
            shader: draw.shader,
            source: draw.source,
            target: draw.target,
            target_format: draw.target_format,
            texture_slot: draw.texture_slot,
            texture_slot_mask: 1u32 << draw.texture_slot,
            texture_source: draw.texture_source,
            pipeline_class: cache_key.pipeline_class,
            vertex_layout: cache_key.vertex_layout,
            blend_key: draw.blend_key,
            alpha_uniform: draw.alpha_uniform,
            heap_bind_index: draw_bind.heap_bind_index,
            heap_slice_index: draw_bind.heap_slice_index,
            base_resource_descriptor_index: draw_bind.base_resource_descriptor_index,
            base_sampler_descriptor_index: draw_bind.base_sampler_descriptor_index,
            resource_descriptor_index: draw_bind.base_resource_descriptor_index,
            sampler_descriptor_index: draw_bind.base_sampler_descriptor_index,
            shader_mapping: format!(
                "VK_EXT_descriptor_heap we.texture_slot{}.g_Texture{} -> alpha-mask-copy-back-heap-slice{}-resource{}-sampler{}",
                draw.texture_slot,
                draw.texture_slot,
                draw_bind.heap_slice_index,
                draw_bind.base_resource_descriptor_index,
                draw_bind.base_sampler_descriptor_index
            ),
            raster_geometry: FLATTEXTURE_COPY_BACK_RASTER_GEOMETRY,
            command_order: [
                "select_util_minimalalpha_shader",
                "select_render_state_flattexture_copy_back_geometry",
                "select_r8_unorm_full_alpha_mask_target",
                "select_dest_color_copy_back_blend_key_0x100",
                "map_g_Texture0_to_alpha_mask_copy_back_heap_slice",
                "preserve_g_Alpha_uniform_1",
            ],
        });
        if !cache_keys.iter().any(|existing| existing == &cache_key) {
            cache_keys.push(cache_key);
        }
    }

    Ok(NativeVulkanSceneLayerAlphaMaskCopyBackPipelinePlan {
        pipeline_count: keys.len(),
        cache_key_count: cache_keys.len(),
        texture_slot_mask,
        keys,
        command_order: [
            "read_copy_back_draw_resources",
            "read_copy_back_heap_bind_pairings",
            "derive_minimalalpha_copy_back_pipeline_keys",
            "map_copy_back_texture_slots_to_descriptor_heap_offsets",
            "preserve_render_state_flattexture_copy_back_draw_shape",
        ],
        cache_keys,
    })
}

impl NativeVulkanSceneLayerAlphaMaskCopyBackPipelinePlan {
    pub(in crate::renderer::native_vulkan) fn cache_keys(
        &self,
    ) -> &[NativeVulkanScenePipelineCacheKey] {
        &self.cache_keys
    }
}

impl NativeVulkanSceneLayerAlphaMaskCopyBackPipelineKeyPlan {
    pub(in crate::renderer::native_vulkan) fn cache_key(
        &self,
    ) -> NativeVulkanScenePipelineCacheKey {
        NativeVulkanScenePipelineCacheKey {
            shader: self.shader.to_owned(),
            shader_combo_values: Vec::new(),
            blend: SceneBlendContract::DestColorCopyBackBit0x100,
            render_state: SceneMaterialRenderState::translucent_2d(),
            pipeline_class: self.pipeline_class,
            vertex_layout: self.vertex_layout,
            target_format: vk::Format::R8_UNORM,
            texture_slot_mask: self.texture_slot_mask,
        }
    }
}

fn validate_copy_back_draw_bind(
    draw: &NativeVulkanSceneLayerAlphaMaskCopyBackDrawPlan,
    draw_bind: &NativeVulkanSceneLayerAlphaMaskCopyBackDrawResourceBindPlan,
) -> Result<(), String> {
    if draw.command_index != draw_bind.command_index {
        return Err(format!(
            "scene layer alpha-mask copy-back pipeline command mismatch: draw {}, bind {}",
            draw.command_index, draw_bind.command_index
        ));
    }
    if draw.object != draw_bind.object {
        return Err(format!(
            "scene layer alpha-mask copy-back pipeline object mismatch: draw {:?}, bind {:?}",
            draw.object, draw_bind.object
        ));
    }
    if draw.shader != draw_bind.shader {
        return Err(format!(
            "scene layer alpha-mask copy-back pipeline shader mismatch: draw {}, bind {}",
            draw.shader, draw_bind.shader
        ));
    }
    if draw.texture_slot != draw_bind.texture_slot {
        return Err(format!(
            "scene layer alpha-mask copy-back pipeline texture slot mismatch: draw {}, bind {}",
            draw.texture_slot, draw_bind.texture_slot
        ));
    }
    if draw.texture_source != draw_bind.texture_source {
        return Err(format!(
            "scene layer alpha-mask copy-back pipeline texture source mismatch: draw {:?}, bind {:?}",
            draw.texture_source, draw_bind.texture_source
        ));
    }
    Ok(())
}

fn copy_back_pipeline_cache_key(
    draw: &NativeVulkanSceneLayerAlphaMaskCopyBackDrawPlan,
) -> Result<NativeVulkanScenePipelineCacheKey, String> {
    if draw.target_format != NativeVulkanSceneTextureDescriptorVkFormat::R8Unorm {
        return Err(format!(
            "scene layer alpha-mask copy-back pipeline requires R8_UNORM target, got {:?}",
            draw.target_format
        ));
    }
    Ok(NativeVulkanScenePipelineCacheKey {
        shader: draw.shader.to_owned(),
        shader_combo_values: Vec::new(),
        blend: SceneBlendContract::DestColorCopyBackBit0x100,
        render_state: SceneMaterialRenderState::translucent_2d(),
        pipeline_class: SceneGraphPipelineClass::LayerUtilityIndexed,
        vertex_layout: NativeVulkanScenePipelineVertexLayout::FlatTexturePositionUv,
        target_format: vk::Format::R8_UNORM,
        texture_slot_mask: 1u32 << draw.texture_slot,
    })
}

#[cfg(test)]
mod tests {
    use super::super::copy_back::NativeVulkanSceneLayerAlphaMaskCopyBackAlphaUniform;
    use super::*;
    use crate::engine::scene_engine::{SceneGraphResourceRole, SceneLayerCompositorOperation};

    #[test]
    fn copy_back_pipeline_plan_tracks_heap_descriptor_offsets() {
        let draw = NativeVulkanSceneLayerAlphaMaskCopyBackDrawPlan {
            command_index: 3,
            object: SceneObjectId(77),
            operation: SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask,
            material: "materials/util/flattexture.json",
            shader: "util/minimalalpha",
            source: SceneGraphTarget::FullAlphaMaskIntermediate,
            target: SceneGraphTarget::FullAlphaMask,
            texture_slot: 0,
            texture_role: SceneGraphResourceRole::shader_texture(0),
            texture_source: NativeVulkanSceneTextureDescriptorSource::GraphTarget(
                SceneGraphTarget::FullAlphaMaskIntermediate,
            ),
            target_format: NativeVulkanSceneTextureDescriptorVkFormat::R8Unorm,
            alpha_uniform: NativeVulkanSceneLayerAlphaMaskCopyBackAlphaUniform {
                name: "g_Alpha",
                value_bits: 1.0f32.to_bits(),
            },
            blend_key: SceneLayerCompositorBlendKey::DestColorCopyBackBit0x100,
            command_order: [
                "load_materials_util_flattexture_json",
                "select_util_minimalalpha_shader",
                "bind_g_Texture0_to_full_alpha_mask_intermediate",
                "set_g_Alpha_to_1",
                "toggle_wrapper_blend_key_0x100",
                "draw_render_state_flattexture_copy_back",
            ],
        };
        let draw_bind = NativeVulkanSceneLayerAlphaMaskCopyBackDrawResourceBindPlan {
            copy_back_draw_index: 0,
            command_index: 3,
            object: SceneObjectId(77),
            shader: "util/minimalalpha",
            texture_slot: 0,
            texture_source: NativeVulkanSceneTextureDescriptorSource::GraphTarget(
                SceneGraphTarget::FullAlphaMaskIntermediate,
            ),
            bind_index: 2,
            heap_bind_index: 2,
            heap_slice_index: 2,
            base_resource_descriptor_index: 4,
            base_sampler_descriptor_index: 4,
            command_order: [
                "read_flattexture_copy_back_draw_resource",
                "select_flattexture_copy_back_heap_bind",
                "bind_flattexture_copy_back_resource_heap",
                "draw_minimalalpha_copy_back",
            ],
        };

        let plan =
            native_vulkan_plan_scene_layer_alpha_mask_copy_back_pipelines(&[draw], &[draw_bind])
                .expect("copy-back pipeline plan");

        assert_eq!(plan.pipeline_count, 1);
        assert_eq!(plan.cache_key_count, 1);
        assert_eq!(plan.texture_slot_mask, 1);
        let key = &plan.keys[0];
        assert_eq!(key.shader, "util/minimalalpha");
        assert_eq!(
            key.target_format,
            NativeVulkanSceneTextureDescriptorVkFormat::R8Unorm
        );
        assert_eq!(
            key.pipeline_class,
            SceneGraphPipelineClass::LayerUtilityIndexed
        );
        assert_eq!(
            key.vertex_layout,
            NativeVulkanScenePipelineVertexLayout::FlatTexturePositionUv
        );
        assert_eq!(key.raster_geometry, "render-state-flattexture-copy-back");
        assert_eq!(key.heap_bind_index, 2);
        assert_eq!(key.base_resource_descriptor_index, 4);
        assert_eq!(key.base_sampler_descriptor_index, 4);
        assert_eq!(key.resource_descriptor_index, 4);
        assert_eq!(key.sampler_descriptor_index, 4);
        assert_eq!(
            key.shader_mapping,
            "VK_EXT_descriptor_heap we.texture_slot0.g_Texture0 -> alpha-mask-copy-back-heap-slice2-resource4-sampler4"
        );
        assert_eq!(plan.cache_keys().len(), 1);
        assert_eq!(plan.cache_keys()[0].shader, "util/minimalalpha");
        assert_eq!(
            plan.cache_keys()[0].blend,
            SceneBlendContract::DestColorCopyBackBit0x100
        );
        assert_eq!(
            plan.cache_keys()[0].vertex_layout,
            NativeVulkanScenePipelineVertexLayout::FlatTexturePositionUv
        );
        assert_eq!(plan.cache_keys()[0].target_format, vk::Format::R8_UNORM);
    }
}
