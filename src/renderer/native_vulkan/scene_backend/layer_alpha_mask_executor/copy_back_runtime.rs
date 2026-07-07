//! Runtime command-list assembly for WE alpha-mask flattexture copy-back draws.
//!
//! References:
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;

use super::copy_back_command::NativeVulkanSceneLayerAlphaMaskCopyBackCommandPlan;
use super::copy_back_geometry::{
    FLATTEXTURE_COPY_BACK_VERTEX_STRIDE_BYTES,
    NativeVulkanSceneLayerAlphaMaskCopyBackRenderStateGeometryBuffers,
};
use super::copy_back_pipeline::NativeVulkanSceneLayerAlphaMaskCopyBackPipelinePlan;
use super::resource_binds::NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan;
use crate::renderer::native_vulkan::scene_backend::frame_resources::NativeVulkanSceneFrameResources;
use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_resource_heap::NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo;
use crate::renderer::native_vulkan::scene_backend::resource_storage::NativeVulkanSceneRenderStateUtilityGeometry;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskCopyBackRuntimeCommandPlan
{
    pub command_count: usize,
    pub warmed_pipeline_count: usize,
    pub descriptor_heap_bind_count: usize,
    pub render_state_geometry_bind_count: usize,
    pub pipeline_bind_count: usize,
    pub resource_heap_bind_count: usize,
    pub direct_draw_count: usize,
    pub commands: Vec<NativeVulkanSceneLayerAlphaMaskCopyBackCommandPlan>,
    pub command_order: [&'static str; 6],
}

impl NativeVulkanSceneLayerAlphaMaskCopyBackRuntimeCommandPlan {
    fn from_pipeline_bind_infos_and_geometry(
        pipelines: &NativeVulkanSceneLayerAlphaMaskCopyBackPipelinePlan,
        mut bind_info_for_heap_bind: impl FnMut(
            usize,
        ) -> Result<
            NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo,
            String,
        >,
        geometry: NativeVulkanSceneLayerAlphaMaskCopyBackRenderStateGeometryBuffers,
        warmed_pipeline_count: usize,
    ) -> Result<Self, String> {
        if pipelines.keys.is_empty() {
            return Ok(Self::empty());
        }

        let mut commands = Vec::with_capacity(pipelines.keys.len());
        for pipeline in &pipelines.keys {
            let bind_info = bind_info_for_heap_bind(pipeline.heap_bind_index).map_err(
                |err| {
                    format!(
                        "{err}; scene layer alpha-mask copy-back command {} requires heap-bind {} bind info",
                        pipeline.command_index, pipeline.heap_bind_index
                    )
                },
            )?;
            commands.push(
                NativeVulkanSceneLayerAlphaMaskCopyBackCommandPlan::from_pipeline_heap_and_geometry(
                    pipeline, &bind_info, geometry,
                )?,
            );
        }

        let pipeline_bind_count = commands
            .iter()
            .map(|command| command.pipeline_bind_count)
            .sum();
        let resource_heap_bind_count = commands
            .iter()
            .map(|command| command.resource_heap_bind_count)
            .sum();
        let direct_draw_count = commands
            .iter()
            .map(|command| command.direct_draw_count)
            .sum();

        Ok(Self {
            command_count: commands.len(),
            warmed_pipeline_count,
            descriptor_heap_bind_count: commands.len(),
            render_state_geometry_bind_count: commands.len(),
            pipeline_bind_count,
            resource_heap_bind_count,
            direct_draw_count,
            commands,
            command_order: [
                "require_warmed_util_minimalalpha_copy_back_pipelines",
                "load_render_state_flattexture_copy_back_utility_geometry",
                "resolve_flattexture_copy_back_descriptor_heap_bind",
                "build_copy_back_command_plan",
                "preserve_draw_style_copy_back_no_transfer_copy",
                "defer_recording_to_alpha_mask_token_scheduler",
            ],
        })
    }

    fn empty() -> Self {
        Self {
            command_count: 0,
            warmed_pipeline_count: 0,
            descriptor_heap_bind_count: 0,
            render_state_geometry_bind_count: 0,
            pipeline_bind_count: 0,
            resource_heap_bind_count: 0,
            direct_draw_count: 0,
            commands: Vec::new(),
            command_order: [
                "require_warmed_util_minimalalpha_copy_back_pipelines",
                "load_render_state_flattexture_copy_back_utility_geometry",
                "resolve_flattexture_copy_back_descriptor_heap_bind",
                "build_copy_back_command_plan",
                "preserve_draw_style_copy_back_no_transfer_copy",
                "defer_recording_to_alpha_mask_token_scheduler",
            ],
        }
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_alpha_mask_copy_back_runtime_commands(
    frame_resources: &NativeVulkanSceneFrameResources,
    resource_binds: &NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan,
) -> Result<NativeVulkanSceneLayerAlphaMaskCopyBackRuntimeCommandPlan, String> {
    let pipelines = &resource_binds.copy_back_pipelines;
    if pipelines.keys.is_empty() {
        return Ok(NativeVulkanSceneLayerAlphaMaskCopyBackRuntimeCommandPlan::empty());
    }

    for cache_key in pipelines.cache_keys() {
        frame_resources.cached_mesh_pipeline(cache_key).map_err(|err| {
            format!(
                "{err}; scene layer alpha-mask runtime requires util/minimalalpha copy-back pipeline warmup before command-list assembly"
            )
        })?;
    }

    let geometry = render_state_copy_back_geometry_buffers(frame_resources)?;
    NativeVulkanSceneLayerAlphaMaskCopyBackRuntimeCommandPlan::from_pipeline_bind_infos_and_geometry(
        pipelines,
        |heap_bind_index| frame_resources.layer_alpha_mask_resource_heap_bind_info(heap_bind_index),
        geometry,
        pipelines.cache_keys().len(),
    )
}

pub(super) fn render_state_copy_back_geometry_buffers(
    frame_resources: &NativeVulkanSceneFrameResources,
) -> Result<NativeVulkanSceneLayerAlphaMaskCopyBackRenderStateGeometryBuffers, String> {
    let buffers = frame_resources
        .render_state_utility_geometry_buffers(
            NativeVulkanSceneRenderStateUtilityGeometry::LayerAlphaMaskCopyBackState48,
        )
        .map_err(|err| {
            format!(
                "{err}; scene layer alpha-mask copy-back requires retained state_body+0x48 utility geometry"
            )
        })?;
    if buffers.vertex.bytes % u64::from(FLATTEXTURE_COPY_BACK_VERTEX_STRIDE_BYTES) != 0 {
        return Err(format!(
            "scene layer alpha-mask copy-back state_body+0x48 vertex buffer size {} is not aligned to flattexture stride {}",
            buffers.vertex.bytes, FLATTEXTURE_COPY_BACK_VERTEX_STRIDE_BYTES
        ));
    }
    let vertex_count = buffers.vertex.bytes / u64::from(FLATTEXTURE_COPY_BACK_VERTEX_STRIDE_BYTES);
    let vertex_count = u32::try_from(vertex_count).map_err(|_| {
        format!(
            "scene layer alpha-mask copy-back state_body+0x48 vertex count {vertex_count} exceeds u32"
        )
    })?;
    NativeVulkanSceneLayerAlphaMaskCopyBackRenderStateGeometryBuffers::from_render_state_utility_geometry_buffers(
        buffers,
        vertex_count,
    )
}

#[cfg(test)]
mod tests {
    use super::super::copy_back::NativeVulkanSceneLayerAlphaMaskCopyBackAlphaUniform;
    use super::super::copy_back::NativeVulkanSceneLayerAlphaMaskCopyBackDrawPlan;
    use super::super::copy_back_geometry::{
        FLATTEXTURE_COPY_BACK_VERTEX_BYTES, FLATTEXTURE_COPY_BACK_VERTEX_COUNT,
        native_vulkan_scene_layer_alpha_mask_copy_back_fullscreen_triangle_payload,
    };
    use super::super::copy_back_pipeline::native_vulkan_plan_scene_layer_alpha_mask_copy_back_pipelines;
    use super::super::resource_binds::NativeVulkanSceneLayerAlphaMaskCopyBackDrawResourceBindPlan;
    use super::*;
    use crate::engine::scene_engine::{
        SceneGraphResourceRole, SceneGraphTarget, SceneLayerCompositorBlendKey,
        SceneLayerCompositorOperation, SceneObjectId, ScenePuppetId,
    };
    use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_resource_heap::{
        NativeVulkanSceneLayerAlphaMaskHeapSliceBinding,
        NativeVulkanSceneLayerAlphaMaskHeapSliceKey,
    };
    use crate::renderer::native_vulkan::scene_backend::texture_descriptors::{
        NativeVulkanSceneTextureDescriptorSource, NativeVulkanSceneTextureDescriptorVkFormat,
    };
    use vulkanalia::vk;
    use vulkanalia::vk::Handle;
    use vulkanalia::vk::HasBuilder;

    #[test]
    fn copy_back_runtime_commands_join_pipeline_heap_and_render_state_geometry() {
        let pipelines = copy_back_pipelines();
        let plan =
            NativeVulkanSceneLayerAlphaMaskCopyBackRuntimeCommandPlan::from_pipeline_bind_infos_and_geometry(
                &pipelines,
                |heap_bind_index| {
                    assert_eq!(heap_bind_index, 2);
                    Ok(bind_info(2, 4, 4))
                },
                geometry(),
                1,
            )
            .expect("copy-back runtime command plan");

        assert_eq!(plan.command_count, 1);
        assert_eq!(plan.warmed_pipeline_count, 1);
        assert_eq!(plan.descriptor_heap_bind_count, 1);
        assert_eq!(plan.render_state_geometry_bind_count, 1);
        assert_eq!(plan.pipeline_bind_count, 1);
        assert_eq!(plan.resource_heap_bind_count, 1);
        assert_eq!(plan.direct_draw_count, 1);
        assert_eq!(plan.commands[0].command_index, 3);
        assert_eq!(plan.commands[0].geometry.source_field, "state_body+0x48");
        assert_eq!(
            plan.command_order,
            [
                "require_warmed_util_minimalalpha_copy_back_pipelines",
                "load_render_state_flattexture_copy_back_utility_geometry",
                "resolve_flattexture_copy_back_descriptor_heap_bind",
                "build_copy_back_command_plan",
                "preserve_draw_style_copy_back_no_transfer_copy",
                "defer_recording_to_alpha_mask_token_scheduler"
            ]
        );
    }

    #[test]
    fn copy_back_runtime_commands_use_heap_bind_index_not_heap_slice_guess() {
        let mut draw_bind = copy_back_draw_bind();
        draw_bind.heap_bind_index = 9;
        draw_bind.heap_slice_index = 2;
        let pipelines = native_vulkan_plan_scene_layer_alpha_mask_copy_back_pipelines(
            &[copy_back_draw()],
            &[draw_bind],
        )
        .expect("copy-back pipelines");

        let plan =
            NativeVulkanSceneLayerAlphaMaskCopyBackRuntimeCommandPlan::from_pipeline_bind_infos_and_geometry(
                &pipelines,
                |heap_bind_index| {
                    assert_eq!(heap_bind_index, 9);
                    Ok(bind_info(9, 4, 4))
                },
                geometry(),
                1,
            )
            .expect("copy-back runtime command plan");

        assert_eq!(plan.commands[0].heap_slice_index, 2);
    }

    #[test]
    fn copy_back_runtime_commands_reject_missing_heap_bind() {
        let pipelines = copy_back_pipelines();
        let err =
            NativeVulkanSceneLayerAlphaMaskCopyBackRuntimeCommandPlan::from_pipeline_bind_infos_and_geometry(
                &pipelines,
                |heap_bind_index| Err(format!("missing heap-bind {heap_bind_index}")),
                geometry(),
                1,
            )
            .expect_err("missing heap bind must fail");

        assert!(err.contains("requires heap-bind 2 bind info"));
    }

    fn copy_back_pipelines() -> NativeVulkanSceneLayerAlphaMaskCopyBackPipelinePlan {
        native_vulkan_plan_scene_layer_alpha_mask_copy_back_pipelines(
            &[copy_back_draw()],
            &[copy_back_draw_bind()],
        )
        .expect("copy-back pipelines")
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
            heap_slice_index: 2,
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
        heap_bind_index: usize,
        base_resource_descriptor_index: usize,
        base_sampler_descriptor_index: usize,
    ) -> NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo {
        NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo {
            heap_bind_index,
            object: SceneObjectId(77),
            puppet: ScenePuppetId(5),
            shader: "util/minimalalpha".to_owned(),
            role: super::super::NativeVulkanSceneLayerAlphaMaskTextureBindRole::FlatTextureCopyBack,
            heap_slice_index: 2,
            heap_slice: NativeVulkanSceneLayerAlphaMaskHeapSliceKey {
                shader: "util/minimalalpha".to_owned(),
                bindings: vec![NativeVulkanSceneLayerAlphaMaskHeapSliceBinding {
                    slot: 0,
                    source:
                        super::super::NativeVulkanSceneLayerAlphaMaskDescriptorSource::GraphTarget(
                            SceneGraphTarget::FullAlphaMaskIntermediate,
                        ),
                }],
            },
            material: None,
            base_resource_descriptor_index,
            base_sampler_descriptor_index,
            resource_descriptor_count: 1,
            texture_count: 1,
            shader_mappings: vec![
                "we.texture_slot0.g_Texture0 -> alpha-mask-heap-slice-offset0".to_owned(),
            ],
            resource_bind: vk::BindHeapInfoEXT::builder().build(),
            sampler_bind: vk::BindHeapInfoEXT::builder().build(),
        }
    }

    fn geometry() -> NativeVulkanSceneLayerAlphaMaskCopyBackRenderStateGeometryBuffers {
        NativeVulkanSceneLayerAlphaMaskCopyBackRenderStateGeometryBuffers {
            vertex: vk::Buffer::from_raw(11),
            vertex_bytes: FLATTEXTURE_COPY_BACK_VERTEX_BYTES,
            vertex_count: FLATTEXTURE_COPY_BACK_VERTEX_COUNT,
            vertex_stride_bytes: FLATTEXTURE_COPY_BACK_VERTEX_STRIDE_BYTES,
            vertex_payload_hash:
                native_vulkan_scene_layer_alpha_mask_copy_back_fullscreen_triangle_payload(false)
                    .payload_hash,
        }
    }
}
