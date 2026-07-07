//! Command recording contract for WE alpha-mask flattexture copy-back draws.
//!
//! References:
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/minimalalpha.vert`
//! - `artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/minimalalpha.frag`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, ExtDescriptorHeapExtensionDeviceCommands};

use crate::engine::scene_engine::{SceneGraphTarget, SceneLayerCompositorBlendKey, SceneObjectId};
use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_resource_heap::NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo;
use crate::renderer::native_vulkan::scene_backend::texture_descriptors::NativeVulkanSceneTextureDescriptorVkFormat;

use super::NativeVulkanSceneLayerAlphaMaskTextureBindRole;
use super::copy_back_geometry::{
    NativeVulkanSceneLayerAlphaMaskCopyBackRenderStateGeometryBuffers,
    NativeVulkanSceneLayerAlphaMaskCopyBackRenderStateGeometryPlan,
};
use super::copy_back_pipeline::NativeVulkanSceneLayerAlphaMaskCopyBackPipelineKeyPlan;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskCopyBackCommandPlan {
    pub command_index: usize,
    pub object: SceneObjectId,
    pub shader: &'static str,
    pub source: SceneGraphTarget,
    pub target: SceneGraphTarget,
    pub target_format: NativeVulkanSceneTextureDescriptorVkFormat,
    pub blend_key: SceneLayerCompositorBlendKey,
    pub heap_bind_index: usize,
    pub resource_set_index: usize,
    pub base_resource_descriptor_index: usize,
    pub base_sampler_descriptor_index: usize,
    pub geometry: NativeVulkanSceneLayerAlphaMaskCopyBackRenderStateGeometryPlan,
    pub pipeline_bind_count: usize,
    pub resource_heap_bind_count: usize,
    pub direct_draw_count: usize,
    pub draw_call: &'static str,
    pub command_order: [&'static str; 5],
}

impl NativeVulkanSceneLayerAlphaMaskCopyBackCommandPlan {
    pub(in crate::renderer::native_vulkan) fn from_pipeline_heap_and_geometry(
        pipeline: &NativeVulkanSceneLayerAlphaMaskCopyBackPipelineKeyPlan,
        bind_info: &NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo,
        geometry: NativeVulkanSceneLayerAlphaMaskCopyBackRenderStateGeometryBuffers,
    ) -> Result<Self, String> {
        validate_copy_back_pipeline_for_command(pipeline)?;
        validate_copy_back_heap_bind_for_command(pipeline, bind_info)?;
        let geometry =
            NativeVulkanSceneLayerAlphaMaskCopyBackRenderStateGeometryPlan::from_raster_geometry_and_buffers(
                pipeline.raster_geometry,
                geometry,
            )?;
        Ok(Self {
            command_index: pipeline.command_index,
            object: pipeline.object,
            shader: pipeline.shader,
            source: pipeline.source,
            target: pipeline.target,
            target_format: pipeline.target_format,
            blend_key: pipeline.blend_key,
            heap_bind_index: bind_info.heap_bind_index,
            resource_set_index: pipeline.resource_set_index,
            base_resource_descriptor_index: pipeline.base_resource_descriptor_index,
            base_sampler_descriptor_index: pipeline.base_sampler_descriptor_index,
            geometry,
            pipeline_bind_count: 1,
            resource_heap_bind_count: 1,
            direct_draw_count: 1,
            draw_call: "vkCmdDraw",
            command_order: [
                "cmd_bind_util_minimalalpha_pipeline",
                "cmd_bind_alpha_mask_copy_back_resource_heap_ext",
                "cmd_bind_alpha_mask_copy_back_sampler_heap_ext",
                "cmd_bind_render_state_flattexture_copy_back_geometry",
                "cmd_draw_render_state_flattexture_copy_back",
            ],
        })
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_layer_alpha_mask_copy_back_command(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    pipeline: &NativeVulkanSceneLayerAlphaMaskCopyBackPipelineKeyPlan,
    vk_pipeline: vk::Pipeline,
    bind_info: &NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo,
    geometry: NativeVulkanSceneLayerAlphaMaskCopyBackRenderStateGeometryBuffers,
) -> Result<NativeVulkanSceneLayerAlphaMaskCopyBackCommandPlan, String> {
    if command_buffer == vk::CommandBuffer::null() {
        return Err(format!(
            "scene layer alpha-mask copy-back command {} requires a valid command buffer",
            pipeline.command_index
        ));
    }
    if vk_pipeline == vk::Pipeline::null() {
        return Err(format!(
            "scene layer alpha-mask copy-back command {} requires a warmed util/minimalalpha vk::Pipeline",
            pipeline.command_index
        ));
    }
    let plan = NativeVulkanSceneLayerAlphaMaskCopyBackCommandPlan::from_pipeline_heap_and_geometry(
        pipeline, bind_info, geometry,
    )?;
    unsafe {
        device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::GRAPHICS, vk_pipeline);
        device.cmd_bind_resource_heap_ext(command_buffer, &bind_info.resource_bind);
        device.cmd_bind_sampler_heap_ext(command_buffer, &bind_info.sampler_bind);
        let vertex_buffers = [geometry.vertex];
        let vertex_offsets = [0u64];
        device.cmd_bind_vertex_buffers(command_buffer, 0, &vertex_buffers, &vertex_offsets);
        device.cmd_draw(command_buffer, geometry.vertex_count, 1, 0, 0);
    }
    Ok(plan)
}

fn validate_copy_back_pipeline_for_command(
    pipeline: &NativeVulkanSceneLayerAlphaMaskCopyBackPipelineKeyPlan,
) -> Result<(), String> {
    if pipeline.shader != "util/minimalalpha" {
        return Err(format!(
            "scene layer alpha-mask copy-back command requires util/minimalalpha shader, got {}",
            pipeline.shader
        ));
    }
    if pipeline.material != "materials/util/flattexture.json" {
        return Err(format!(
            "scene layer alpha-mask copy-back command requires materials/util/flattexture.json, got {}",
            pipeline.material
        ));
    }
    if pipeline.source != SceneGraphTarget::FullAlphaMaskIntermediate
        || pipeline.target != SceneGraphTarget::FullAlphaMask
    {
        return Err(format!(
            "scene layer alpha-mask copy-back command must draw FullAlphaMaskIntermediate -> FullAlphaMask, got {:?} -> {:?}",
            pipeline.source, pipeline.target
        ));
    }
    if pipeline.target_format != NativeVulkanSceneTextureDescriptorVkFormat::R8Unorm {
        return Err(format!(
            "scene layer alpha-mask copy-back command requires R8_UNORM target, got {:?}",
            pipeline.target_format
        ));
    }
    if pipeline.texture_slot != 0 || pipeline.texture_slot_mask != 1 {
        return Err(format!(
            "scene layer alpha-mask copy-back command requires only g_Texture0, got slot {} mask {:#x}",
            pipeline.texture_slot, pipeline.texture_slot_mask
        ));
    }
    if pipeline.blend_key != SceneLayerCompositorBlendKey::DestColorCopyBackBit0x100 {
        return Err(format!(
            "scene layer alpha-mask copy-back command requires blend-key bit 0x100, got {:?}",
            pipeline.blend_key
        ));
    }
    Ok(())
}

fn validate_copy_back_heap_bind_for_command(
    pipeline: &NativeVulkanSceneLayerAlphaMaskCopyBackPipelineKeyPlan,
    bind_info: &NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo,
) -> Result<(), String> {
    if bind_info.role != NativeVulkanSceneLayerAlphaMaskTextureBindRole::FlatTextureCopyBack {
        return Err(format!(
            "scene layer alpha-mask copy-back command requires FlatTextureCopyBack heap bind, got {:?}",
            bind_info.role
        ));
    }
    if bind_info.object != pipeline.object {
        return Err(format!(
            "scene layer alpha-mask copy-back command object mismatch: pipeline {:?}, heap {:?}",
            pipeline.object, bind_info.object
        ));
    }
    if bind_info.shader != pipeline.shader {
        return Err(format!(
            "scene layer alpha-mask copy-back command shader mismatch: pipeline {}, heap {}",
            pipeline.shader, bind_info.shader
        ));
    }
    if bind_info.heap_bind_index != pipeline.heap_bind_index {
        return Err(format!(
            "scene layer alpha-mask copy-back command heap-bind mismatch: pipeline {}, heap {}",
            pipeline.heap_bind_index, bind_info.heap_bind_index
        ));
    }
    if bind_info.resource_set_index != pipeline.resource_set_index {
        return Err(format!(
            "scene layer alpha-mask copy-back command resource-set mismatch: pipeline {}, heap {}",
            pipeline.resource_set_index, bind_info.resource_set_index
        ));
    }
    if bind_info.base_resource_descriptor_index != pipeline.base_resource_descriptor_index {
        return Err(format!(
            "scene layer alpha-mask copy-back command resource descriptor base mismatch: pipeline {}, heap {}",
            pipeline.base_resource_descriptor_index, bind_info.base_resource_descriptor_index
        ));
    }
    if bind_info.base_sampler_descriptor_index != pipeline.base_sampler_descriptor_index {
        return Err(format!(
            "scene layer alpha-mask copy-back command sampler descriptor base mismatch: pipeline {}, heap {}",
            pipeline.base_sampler_descriptor_index, bind_info.base_sampler_descriptor_index
        ));
    }
    if bind_info.texture_count != 1 || bind_info.resource_descriptor_count < 1 {
        return Err(format!(
            "scene layer alpha-mask copy-back command requires one sampled image descriptor, got textures={} resources={}",
            bind_info.texture_count, bind_info.resource_descriptor_count
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::copy_back::NativeVulkanSceneLayerAlphaMaskCopyBackAlphaUniform;
    use super::*;
    use crate::engine::scene_engine::{
        SceneGraphResourceRole, SceneLayerCompositorOperation, ScenePuppetId,
    };
    use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_executor::copy_back::NativeVulkanSceneLayerAlphaMaskCopyBackDrawPlan;
    use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_executor::copy_back_geometry::{
        FLATTEXTURE_COPY_BACK_VERTEX_STRIDE_BYTES,
        TARGET_LIKE_INDEXED_QUAD_HELPER_RASTER_GEOMETRY,
    };
    use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_executor::resource_binds::NativeVulkanSceneLayerAlphaMaskCopyBackDrawResourceBindPlan;
    use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_resource_heap::{
        NativeVulkanSceneLayerAlphaMaskResourceSetBinding,
        NativeVulkanSceneLayerAlphaMaskResourceSetKey,
    };
    use crate::renderer::native_vulkan::scene_backend::texture_descriptors::NativeVulkanSceneTextureDescriptorSource;
    use vulkanalia::vk::Handle;

    #[test]
    fn copy_back_command_plan_binds_pipeline_heap_geometry_then_draws() {
        let key = copy_back_pipeline_key();
        let plan =
            NativeVulkanSceneLayerAlphaMaskCopyBackCommandPlan::from_pipeline_heap_and_geometry(
                &key,
                &bind_info(4, 4),
                geometry(),
            )
            .expect("copy-back command plan");

        assert_eq!(plan.command_index, 3);
        assert_eq!(plan.shader, "util/minimalalpha");
        assert_eq!(plan.source, SceneGraphTarget::FullAlphaMaskIntermediate);
        assert_eq!(plan.target, SceneGraphTarget::FullAlphaMask);
        assert_eq!(plan.heap_bind_index, 2);
        assert_eq!(plan.resource_set_index, 2);
        assert_eq!(plan.base_resource_descriptor_index, 4);
        assert_eq!(plan.base_sampler_descriptor_index, 4);
        assert_eq!(plan.pipeline_bind_count, 1);
        assert_eq!(plan.resource_heap_bind_count, 1);
        assert_eq!(plan.direct_draw_count, 1);
        assert_eq!(plan.draw_call, "vkCmdDraw");
        assert_eq!(plan.geometry.source_field, "render_state+0x48");
        assert_eq!(plan.geometry.draw_load_vma, 0x14020da78);
        assert_eq!(plan.geometry.draw_call_vma, 0x14020da7f);
        assert_eq!(plan.geometry.blend_toggle_vma, 0x14020da40);
        assert_eq!(plan.geometry.vertex_layout, "a_Position.xyz+a_TexCoord.xy");
        assert_eq!(
            plan.command_order,
            [
                "cmd_bind_util_minimalalpha_pipeline",
                "cmd_bind_alpha_mask_copy_back_resource_heap_ext",
                "cmd_bind_alpha_mask_copy_back_sampler_heap_ext",
                "cmd_bind_render_state_flattexture_copy_back_geometry",
                "cmd_draw_render_state_flattexture_copy_back"
            ]
        );
    }

    #[test]
    fn copy_back_command_rejects_wrong_heap_slice() {
        let key = copy_back_pipeline_key();
        let err =
            NativeVulkanSceneLayerAlphaMaskCopyBackCommandPlan::from_pipeline_heap_and_geometry(
                &key,
                &bind_info(5, 4),
                geometry(),
            )
            .expect_err("wrong resource base must fail");

        assert!(err.contains("resource descriptor base mismatch"));
    }

    #[test]
    fn copy_back_command_rejects_missing_render_state_geometry() {
        let key = copy_back_pipeline_key();
        let mut missing = geometry();
        missing.vertex = vk::Buffer::null();

        let err =
            NativeVulkanSceneLayerAlphaMaskCopyBackCommandPlan::from_pipeline_heap_and_geometry(
                &key,
                &bind_info(4, 4),
                missing,
            )
            .expect_err("missing geometry must fail");

        assert!(err.contains("render_state+0x48"));
    }

    #[test]
    fn copy_back_command_rejects_target_like_indexed_helper_geometry() {
        let mut key = copy_back_pipeline_key();
        key.raster_geometry = TARGET_LIKE_INDEXED_QUAD_HELPER_RASTER_GEOMETRY;

        let err =
            NativeVulkanSceneLayerAlphaMaskCopyBackCommandPlan::from_pipeline_heap_and_geometry(
                &key,
                &bind_info(4, 4),
                geometry(),
            )
            .expect_err("indexed helper geometry must not be accepted");

        assert!(err.contains("requires render-state-flattexture-copy-back geometry"));
    }

    fn copy_back_pipeline_key() -> NativeVulkanSceneLayerAlphaMaskCopyBackPipelineKeyPlan {
        super::super::copy_back_pipeline::native_vulkan_plan_scene_layer_alpha_mask_copy_back_pipelines(
            &[copy_back_draw()],
            &[copy_back_draw_bind()],
        )
        .expect("copy-back pipeline plan")
        .keys
        .remove(0)
    }

    fn copy_back_draw() -> NativeVulkanSceneLayerAlphaMaskCopyBackDrawPlan {
        NativeVulkanSceneLayerAlphaMaskCopyBackDrawPlan {
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
        }
    }

    fn copy_back_draw_bind() -> NativeVulkanSceneLayerAlphaMaskCopyBackDrawResourceBindPlan {
        NativeVulkanSceneLayerAlphaMaskCopyBackDrawResourceBindPlan {
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
            resource_set_index: 2,
            base_resource_descriptor_index: 4,
            base_sampler_descriptor_index: 4,
            command_order: [
                "read_flattexture_copy_back_draw_resource",
                "select_flattexture_copy_back_heap_bind",
                "bind_flattexture_copy_back_resource_heap",
                "draw_minimalalpha_copy_back",
            ],
        }
    }

    fn bind_info(
        base_resource_descriptor_index: usize,
        base_sampler_descriptor_index: usize,
    ) -> NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo {
        NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo {
            heap_bind_index: 2,
            object: SceneObjectId(77),
            puppet: ScenePuppetId(5),
            shader: "util/minimalalpha".to_owned(),
            role: NativeVulkanSceneLayerAlphaMaskTextureBindRole::FlatTextureCopyBack,
            resource_set_index: 2,
            resource_set: NativeVulkanSceneLayerAlphaMaskResourceSetKey {
                shader: "util/minimalalpha".to_owned(),
                bindings: vec![NativeVulkanSceneLayerAlphaMaskResourceSetBinding {
                    slot: 0,
                    source:
                        super::super::NativeVulkanSceneLayerAlphaMaskDescriptorSource::GraphTarget(
                            SceneGraphTarget::FullAlphaMaskIntermediate,
                        ),
                }],
            },
            base_resource_descriptor_index,
            base_sampler_descriptor_index,
            resource_descriptor_count: 1,
            texture_count: 1,
            shader_mappings: vec![
                "set0.binding0.g_Texture0 -> alpha-mask-resource-set-offset0".to_owned(),
            ],
            resource_bind: vk::BindHeapInfoEXT::builder().build(),
            sampler_bind: vk::BindHeapInfoEXT::builder().build(),
        }
    }

    fn geometry() -> NativeVulkanSceneLayerAlphaMaskCopyBackRenderStateGeometryBuffers {
        NativeVulkanSceneLayerAlphaMaskCopyBackRenderStateGeometryBuffers {
            vertex: vk::Buffer::from_raw(11),
            vertex_bytes: 80,
            vertex_count: 4,
            vertex_stride_bytes: FLATTEXTURE_COPY_BACK_VERTEX_STRIDE_BYTES,
            vertex_payload_hash: 100,
        }
    }
}
