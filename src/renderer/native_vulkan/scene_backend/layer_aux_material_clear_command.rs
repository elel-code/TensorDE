//! Command emission contract for WE auxiliary fullscreenlayer clear draws.
//!
//! References:
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `artifacts/wallpaper-engine-workshop/steamcmd-root/assets/materials/util/fullscreenlayer.json`
//! - `artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/passthrough.vert`
//! - `artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/passthrough.frag`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, ExtDescriptorHeapExtensionDeviceCommands, Handle};

use crate::engine::scene_engine::{SceneGraphPipelineClass, SceneGraphTarget, SceneObjectId};

use super::frame_resources::NativeVulkanSceneFrameResources;
use super::layer_aux_material_draws::{
    NativeVulkanSceneLayerAuxMaterialDrawCommandPlan,
    NativeVulkanSceneLayerAuxMaterialDrawFramePlan,
    NativeVulkanSceneLayerAuxMaterialDrawReceiverKind, WE_AUX_MATERIAL_CLEAR_VERTEX_BYTES,
    WE_AUX_MATERIAL_CLEAR_VERTEX_COUNT, WE_RT_TARGET_POSITION_UV_LAYOUT_BITMASK,
    WE_RT_TARGET_POSITION_UV_STRIDE_BYTES, native_vulkan_scene_layer_aux_clear_triangle_payload,
};
use super::layer_aux_material_pipeline::{
    NativeVulkanSceneLayerAuxMaterialPipelineFramePlan,
    NativeVulkanSceneLayerAuxMaterialPipelineKeyPlan, WE_AUX_FULLSCREEN_LAYER_MATERIAL,
    WE_AUX_FULLSCREEN_LAYER_SHADER, WE_AUX_FULLSCREEN_LAYER_TEXTURE_SLOT,
    WE_AUX_FULLSCREEN_LAYER_TEXTURE_SOURCE,
};
use super::layer_aux_material_resource_heap::NativeVulkanSceneLayerAuxMaterialResourceHeapBindInfo;
use super::pipeline::{
    NativeVulkanScenePipelineResourceHeapClass, NativeVulkanScenePipelineVertexLayout,
};
use super::resource_buffers::{
    NativeVulkanSceneLayerAuxMaterialClearGeometryBuffers, scene_stable_byte_hash,
};
use super::resource_storage::{
    NativeVulkanSceneGpuBufferOwner, NativeVulkanSceneGpuBufferRole,
    NativeVulkanSceneLayerAuxMaterialClearGeometry,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAuxMaterialClearRuntimeCommandPlan
{
    pub command_count: usize,
    pub warmed_pipeline_count: usize,
    pub heap_bind_count: usize,
    pub geometry_bind_count: usize,
    pub pipeline_bind_count: usize,
    pub resource_heap_bind_count: usize,
    pub direct_draw_count: usize,
    pub commands: Vec<NativeVulkanSceneLayerAuxMaterialClearCommandPlan>,
    pub command_order: [&'static str; 6],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAuxMaterialClearCommandPlan {
    pub command_index: usize,
    pub block_index: usize,
    pub object: SceneObjectId,
    pub material: &'static str,
    pub shader: &'static str,
    pub source: &'static str,
    pub source_target: SceneGraphTarget,
    pub target: SceneGraphTarget,
    pub target_format: &'static str,
    pub texture_slot: u32,
    pub heap_slice_index: usize,
    pub base_resource_descriptor_index: usize,
    pub base_sampler_descriptor_index: usize,
    pub geometry: NativeVulkanSceneLayerAuxMaterialClearGeometryPlan,
    pub pipeline_bind_count: usize,
    pub resource_heap_bind_count: usize,
    pub direct_draw_count: usize,
    pub draw_call: &'static str,
    pub command_order: [&'static str; 6],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAuxMaterialClearGeometryPlan {
    pub geometry: NativeVulkanSceneLayerAuxMaterialClearGeometry,
    pub vertex_buffer_handle: u64,
    pub vertex_bytes: u64,
    pub vertex_stride_bytes: u32,
    pub vertex_count: u32,
    pub vertex_payload_hash: u64,
    pub expected_vertex_payload_hash: u64,
    pub layout_bitmask: u32,
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_aux_material_clear_runtime_commands(
    frame_resources: &NativeVulkanSceneFrameResources,
    material_draws: &NativeVulkanSceneLayerAuxMaterialDrawFramePlan,
    pipelines: &NativeVulkanSceneLayerAuxMaterialPipelineFramePlan,
) -> Result<NativeVulkanSceneLayerAuxMaterialClearRuntimeCommandPlan, String> {
    NativeVulkanSceneLayerAuxMaterialClearRuntimeCommandPlan::from_frame_resources_and_plans(
        frame_resources,
        material_draws,
        pipelines,
    )
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_layer_aux_material_clear_command(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    pipeline: &NativeVulkanSceneLayerAuxMaterialPipelineKeyPlan,
    vk_pipeline: vk::Pipeline,
    bind_info: &NativeVulkanSceneLayerAuxMaterialResourceHeapBindInfo,
    draw: &NativeVulkanSceneLayerAuxMaterialDrawCommandPlan,
    geometry: NativeVulkanSceneLayerAuxMaterialClearGeometryBuffers,
) -> Result<NativeVulkanSceneLayerAuxMaterialClearCommandPlan, String> {
    if command_buffer == vk::CommandBuffer::null() {
        return Err(format!(
            "scene aux clear material command {} requires a valid command buffer",
            pipeline.command_index
        ));
    }
    if vk_pipeline == vk::Pipeline::null() {
        return Err(format!(
            "scene aux clear material command {} requires a warmed util/passthrough vk::Pipeline",
            pipeline.command_index
        ));
    }
    let plan =
        NativeVulkanSceneLayerAuxMaterialClearCommandPlan::from_pipeline_heap_draw_and_geometry(
            pipeline, bind_info, draw, geometry,
        )?;
    unsafe {
        device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::GRAPHICS, vk_pipeline);
        device.cmd_bind_resource_heap_ext(command_buffer, &bind_info.resource_bind);
        device.cmd_bind_sampler_heap_ext(command_buffer, &bind_info.sampler_bind);
        let vertex_buffers = [geometry.vertex.buffer];
        let vertex_offsets = [0u64];
        device.cmd_bind_vertex_buffers(command_buffer, 0, &vertex_buffers, &vertex_offsets);
        device.cmd_draw(command_buffer, WE_AUX_MATERIAL_CLEAR_VERTEX_COUNT, 1, 0, 0);
    }
    Ok(plan)
}

impl NativeVulkanSceneLayerAuxMaterialClearRuntimeCommandPlan {
    fn from_frame_resources_and_plans(
        frame_resources: &NativeVulkanSceneFrameResources,
        material_draws: &NativeVulkanSceneLayerAuxMaterialDrawFramePlan,
        pipelines: &NativeVulkanSceneLayerAuxMaterialPipelineFramePlan,
    ) -> Result<Self, String> {
        if material_draws.command_count == 0 && pipelines.clear_pipeline_count == 0 {
            return Ok(Self::empty());
        }
        if pipelines.clear_pipeline_count != material_draws.command_count {
            return Err(format!(
                "scene aux clear material runtime needs one util/passthrough pipeline per material draw command, got pipelines={} draws={}",
                pipelines.clear_pipeline_count, material_draws.command_count
            ));
        }

        for cache_key in pipelines.cache_keys() {
            frame_resources.cached_mesh_pipeline(cache_key).map_err(|err| {
                format!(
                    "{err}; scene aux clear material runtime requires util/passthrough fullscreenlayer pipeline warmup before command-list assembly"
                )
            })?;
        }

        let mut commands = Vec::with_capacity(pipelines.clear_keys.len());
        for pipeline in &pipelines.clear_keys {
            let draw = material_draws
                .commands
                .iter()
                .find(|draw| {
                    draw.command_index == pipeline.command_index
                        && draw.block_index == pipeline.block_index
                        && draw.object == pipeline.object
                })
                .ok_or_else(|| {
                    format!(
                        "scene aux clear material command {} has no matching draw receiver plan",
                        pipeline.command_index
                    )
                })?;
            let bind_info = frame_resources
                .layer_aux_material_resource_heap_bind_info_for_command(pipeline.command_index)?;
            let geometry = frame_resources.layer_aux_material_clear_geometry_buffers(
                NativeVulkanSceneLayerAuxMaterialClearGeometry {
                    object: pipeline.object,
                },
            )?;
            commands.push(
                NativeVulkanSceneLayerAuxMaterialClearCommandPlan::from_pipeline_heap_draw_and_geometry(
                    pipeline, &bind_info, draw, geometry,
                )?,
            );
        }

        Ok(Self::from_commands(commands, pipelines.cache_keys().len()))
    }

    pub(in crate::renderer::native_vulkan) fn covers_pipelines_and_draws(
        &self,
        material_draws: &NativeVulkanSceneLayerAuxMaterialDrawFramePlan,
        pipelines: &NativeVulkanSceneLayerAuxMaterialPipelineFramePlan,
    ) -> bool {
        self.command_count == material_draws.command_count
            && self.command_count == pipelines.clear_pipeline_count
            && self.heap_bind_count == self.command_count
            && self.geometry_bind_count == self.command_count
            && pipelines.clear_keys.iter().all(|pipeline| {
                self.commands.iter().any(|command| {
                    command.command_index == pipeline.command_index
                        && command.block_index == pipeline.block_index
                        && command.object == pipeline.object
                })
            })
    }

    fn from_commands(
        commands: Vec<NativeVulkanSceneLayerAuxMaterialClearCommandPlan>,
        warmed_pipeline_count: usize,
    ) -> Self {
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
        Self {
            command_count: commands.len(),
            warmed_pipeline_count,
            heap_bind_count: commands.len(),
            geometry_bind_count: commands.len(),
            pipeline_bind_count,
            resource_heap_bind_count,
            direct_draw_count,
            commands,
            command_order: [
                "require_warmed_aux_fullscreenlayer_pipeline",
                "resolve_aux_material_heap_slice",
                "load_aux_0x3f0_position_uv_geometry",
                "validate_we_stack_triangle_payload_hash",
                "build_aux_clear_material_command_plan",
                "emit_aux_0x410_to_aux_0x3f0_draw",
            ],
        }
    }

    fn empty() -> Self {
        Self {
            command_count: 0,
            warmed_pipeline_count: 0,
            heap_bind_count: 0,
            geometry_bind_count: 0,
            pipeline_bind_count: 0,
            resource_heap_bind_count: 0,
            direct_draw_count: 0,
            commands: Vec::new(),
            command_order: [
                "require_warmed_aux_fullscreenlayer_pipeline",
                "resolve_aux_material_heap_slice",
                "load_aux_0x3f0_position_uv_geometry",
                "validate_we_stack_triangle_payload_hash",
                "build_aux_clear_material_command_plan",
                "emit_aux_0x410_to_aux_0x3f0_draw",
            ],
        }
    }
}

impl NativeVulkanSceneLayerAuxMaterialClearCommandPlan {
    pub(in crate::renderer::native_vulkan) fn from_pipeline_heap_draw_and_geometry(
        pipeline: &NativeVulkanSceneLayerAuxMaterialPipelineKeyPlan,
        bind_info: &NativeVulkanSceneLayerAuxMaterialResourceHeapBindInfo,
        draw: &NativeVulkanSceneLayerAuxMaterialDrawCommandPlan,
        geometry: NativeVulkanSceneLayerAuxMaterialClearGeometryBuffers,
    ) -> Result<Self, String> {
        validate_aux_clear_pipeline_for_command(pipeline)?;
        validate_aux_clear_heap_bind_for_command(pipeline, bind_info)?;
        validate_aux_clear_draw_for_command(pipeline, draw)?;
        let geometry = NativeVulkanSceneLayerAuxMaterialClearGeometryPlan::from_draw_and_buffers(
            draw, geometry,
        )?;
        Ok(Self {
            command_index: pipeline.command_index,
            block_index: pipeline.block_index,
            object: pipeline.object,
            material: pipeline.material,
            shader: pipeline.shader,
            source: pipeline.source,
            source_target: bind_info.source_target,
            target: pipeline.target,
            target_format: pipeline.target_format,
            texture_slot: pipeline.texture_slot,
            heap_slice_index: bind_info.heap_slice_index,
            base_resource_descriptor_index: bind_info.base_resource_descriptor_index,
            base_sampler_descriptor_index: bind_info.base_sampler_descriptor_index,
            geometry,
            pipeline_bind_count: 1,
            resource_heap_bind_count: 1,
            direct_draw_count: 1,
            draw_call: "vkCmdDraw",
            command_order: [
                "cmd_bind_util_passthrough_aux_pipeline",
                "cmd_bind_aux_material_resource_heap_ext",
                "cmd_bind_aux_material_sampler_heap_ext",
                "cmd_bind_aux_0x3f0_position_uv_vertex_buffer",
                "cmd_draw_aux_0x3f0_fullscreen_triangle",
                "retain_aux_0x410_scope_release_order",
            ],
        })
    }
}

impl NativeVulkanSceneLayerAuxMaterialClearGeometryPlan {
    fn from_draw_and_buffers(
        draw: &NativeVulkanSceneLayerAuxMaterialDrawCommandPlan,
        geometry: NativeVulkanSceneLayerAuxMaterialClearGeometryBuffers,
    ) -> Result<Self, String> {
        if geometry.geometry.object != draw.object {
            return Err(format!(
                "scene aux clear material geometry object mismatch: draw {:?}, buffer {:?}",
                draw.object, geometry.geometry.object
            ));
        }
        let expected_owner =
            NativeVulkanSceneGpuBufferOwner::LayerAuxMaterialClearGeometry(geometry.geometry);
        if geometry.vertex.key.owner != expected_owner
            || geometry.vertex.key.role
                != NativeVulkanSceneGpuBufferRole::LayerAuxMaterialClearVertex
        {
            return Err(format!(
                "scene aux clear material command {:?} requires LayerAuxMaterialClearVertex for {:?}, got {:?}",
                draw.object, geometry.geometry, geometry.vertex.key
            ));
        }
        if geometry.vertex.buffer == vk::Buffer::null() {
            return Err(format!(
                "scene aux clear material command {:?} has null retained vertex buffer",
                draw.object
            ));
        }
        if geometry.vertex.bytes != WE_AUX_MATERIAL_CLEAR_VERTEX_BYTES {
            return Err(format!(
                "scene aux clear material command {:?} needs {} vertex bytes, got {}",
                draw.object, WE_AUX_MATERIAL_CLEAR_VERTEX_BYTES, geometry.vertex.bytes
            ));
        }

        let clear_payload = draw
            .clear_material
            .clear_triangle_payload
            .as_ref()
            .ok_or_else(|| {
                format!(
                    "scene aux clear material command {:?} has no 0x14020a2d2 stack triangle payload facts",
                    draw.object
                )
            })?;
        let expected_payload = native_vulkan_scene_layer_aux_clear_triangle_payload(
            clear_payload.source_width,
            clear_payload.source_height,
            clear_payload.target_width,
            clear_payload.target_height,
            clear_payload.uv_y_flipped,
        )?;
        let expected_vertex_payload_hash = scene_stable_byte_hash(&expected_payload.bytes);
        if geometry.vertex.payload_hash != expected_vertex_payload_hash {
            return Err(format!(
                "scene aux clear material command {:?} vertex payload hash mismatch: expected {:#x}, got {:#x}",
                draw.object, expected_vertex_payload_hash, geometry.vertex.payload_hash
            ));
        }

        Ok(Self {
            geometry: geometry.geometry,
            vertex_buffer_handle: geometry.vertex.buffer.as_raw(),
            vertex_bytes: geometry.vertex.bytes,
            vertex_stride_bytes: WE_RT_TARGET_POSITION_UV_STRIDE_BYTES,
            vertex_count: WE_AUX_MATERIAL_CLEAR_VERTEX_COUNT,
            vertex_payload_hash: geometry.vertex.payload_hash,
            expected_vertex_payload_hash,
            layout_bitmask: WE_RT_TARGET_POSITION_UV_LAYOUT_BITMASK,
        })
    }
}

fn validate_aux_clear_pipeline_for_command(
    pipeline: &NativeVulkanSceneLayerAuxMaterialPipelineKeyPlan,
) -> Result<(), String> {
    if pipeline.material != WE_AUX_FULLSCREEN_LAYER_MATERIAL
        || pipeline.shader != WE_AUX_FULLSCREEN_LAYER_SHADER
        || pipeline.source != WE_AUX_FULLSCREEN_LAYER_TEXTURE_SOURCE
        || pipeline.texture_slot != WE_AUX_FULLSCREEN_LAYER_TEXTURE_SLOT
    {
        return Err(format!(
            "scene aux clear material command requires fullscreenlayer/_rt_FullFrameBuffer/g_Texture0, got material={} shader={} source={} slot={}",
            pipeline.material, pipeline.shader, pipeline.source, pipeline.texture_slot
        ));
    }
    if pipeline.target != SceneGraphTarget::LayerAuxClear(pipeline.object) {
        return Err(format!(
            "scene aux clear material command object {:?} must target LayerAuxClear, got {:?}",
            pipeline.object, pipeline.target
        ));
    }
    if pipeline.pipeline_class != SceneGraphPipelineClass::LayerUtilityIndexed
        || pipeline.vertex_layout != NativeVulkanScenePipelineVertexLayout::FlatTexturePositionUv
        || pipeline.resource_heap != NativeVulkanScenePipelineResourceHeapClass::LayerAuxMaterial
        || pipeline.draw_receiver
            != NativeVulkanSceneLayerAuxMaterialDrawReceiverKind::Aux3f0ClearMaterialNonIndexed
    {
        return Err(format!(
            "scene aux clear material command {} has incompatible pipeline class/layout/heap/receiver",
            pipeline.command_index
        ));
    }
    Ok(())
}

fn validate_aux_clear_heap_bind_for_command(
    pipeline: &NativeVulkanSceneLayerAuxMaterialPipelineKeyPlan,
    bind_info: &NativeVulkanSceneLayerAuxMaterialResourceHeapBindInfo,
) -> Result<(), String> {
    if bind_info.command_index != pipeline.command_index
        || bind_info.block_index != pipeline.block_index
        || bind_info.object != pipeline.object
    {
        return Err(format!(
            "scene aux clear material heap bind mismatch: pipeline command {} block {} object {:?}, heap command {} block {} object {:?}",
            pipeline.command_index,
            pipeline.block_index,
            pipeline.object,
            bind_info.command_index,
            bind_info.block_index,
            bind_info.object
        ));
    }
    if bind_info.material != pipeline.material
        || bind_info.shader != pipeline.shader
        || bind_info.source != pipeline.source
        || bind_info.target != pipeline.target
        || bind_info.texture_slot != pipeline.texture_slot
    {
        return Err(format!(
            "scene aux clear material heap bind command {} does not match fullscreenlayer pipeline key",
            pipeline.command_index
        ));
    }
    if bind_info.texture_count != 1 || bind_info.resource_descriptor_count < 1 {
        return Err(format!(
            "scene aux clear material command {} requires one sampled texture in aux heap, got textures={} resources={}",
            pipeline.command_index, bind_info.texture_count, bind_info.resource_descriptor_count
        ));
    }
    Ok(())
}

fn validate_aux_clear_draw_for_command(
    pipeline: &NativeVulkanSceneLayerAuxMaterialPipelineKeyPlan,
    draw: &NativeVulkanSceneLayerAuxMaterialDrawCommandPlan,
) -> Result<(), String> {
    if draw.command_index != pipeline.command_index
        || draw.block_index != pipeline.block_index
        || draw.object != pipeline.object
    {
        return Err(format!(
            "scene aux clear material draw mismatch: pipeline command {} block {} object {:?}, draw command {} block {} object {:?}",
            pipeline.command_index,
            pipeline.block_index,
            pipeline.object,
            draw.command_index,
            draw.block_index,
            draw.object
        ));
    }
    if draw.clear_material.receiver_kind
        != NativeVulkanSceneLayerAuxMaterialDrawReceiverKind::Aux3f0ClearMaterialNonIndexed
        || draw.clear_material.layout_bitmask != WE_RT_TARGET_POSITION_UV_LAYOUT_BITMASK
        || draw.clear_material.vertex_stride_bytes != WE_RT_TARGET_POSITION_UV_STRIDE_BYTES
        || draw.clear_material.vertex_count != WE_AUX_MATERIAL_CLEAR_VERTEX_COUNT
        || draw.clear_material.vertex_bytes != WE_AUX_MATERIAL_CLEAR_VERTEX_BYTES
    {
        return Err(format!(
            "scene aux clear material command {} requires 0x14020a3ea aux+0x3f0 non-indexed position/uv receiver",
            pipeline.command_index
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::native_vulkan::scene_backend::layer_aux_material_draws::{
        NativeVulkanSceneLayerAuxClearTrianglePayloadPlan,
        NativeVulkanSceneLayerAuxMaterialDrawReceiverPlan, WE_RT_TARGET_POSITION_UV_ATTR_IDS,
        WE_RT_TARGET_VPTR, WE_RT_TARGET_WRAPPER_CREATE_NON_INDEXED_VMA,
    };
    use crate::renderer::native_vulkan::scene_backend::pipeline::NativeVulkanScenePipelineResourceHeapClass;
    use crate::renderer::native_vulkan::scene_backend::resource_buffers::{
        NativeVulkanSceneGpuBufferBinding, NativeVulkanSceneGpuBufferKey,
    };
    use vulkanalia::vk::HasBuilder;

    #[test]
    fn aux_clear_material_command_binds_pipeline_heap_geometry_then_draws() {
        let object = SceneObjectId(1530);
        let payload =
            native_vulkan_scene_layer_aux_clear_triangle_payload(3840, 2160, 1920, 1080, false)
                .expect("payload");
        let hash = scene_stable_byte_hash(&payload.bytes);

        let plan =
            NativeVulkanSceneLayerAuxMaterialClearCommandPlan::from_pipeline_heap_draw_and_geometry(
                &pipeline_key(object),
                &bind_info(object),
                &draw_command(object, hash),
                geometry(object, hash),
            )
            .expect("aux clear material command");

        assert_eq!(plan.command_index, 7);
        assert_eq!(plan.material, WE_AUX_FULLSCREEN_LAYER_MATERIAL);
        assert_eq!(plan.shader, WE_AUX_FULLSCREEN_LAYER_SHADER);
        assert_eq!(plan.target, SceneGraphTarget::LayerAuxClear(object));
        assert_eq!(
            plan.geometry.vertex_bytes,
            WE_AUX_MATERIAL_CLEAR_VERTEX_BYTES
        );
        assert_eq!(
            plan.geometry.vertex_stride_bytes,
            WE_RT_TARGET_POSITION_UV_STRIDE_BYTES
        );
        assert_eq!(plan.geometry.vertex_payload_hash, hash);
        assert_eq!(plan.pipeline_bind_count, 1);
        assert_eq!(plan.resource_heap_bind_count, 1);
        assert_eq!(plan.direct_draw_count, 1);
    }

    #[test]
    fn aux_clear_material_command_rejects_payload_hash_drift() {
        let object = SceneObjectId(1530);
        let payload =
            native_vulkan_scene_layer_aux_clear_triangle_payload(3840, 2160, 1920, 1080, false)
                .expect("payload");
        let hash = scene_stable_byte_hash(&payload.bytes);

        let err =
            NativeVulkanSceneLayerAuxMaterialClearCommandPlan::from_pipeline_heap_draw_and_geometry(
                &pipeline_key(object),
                &bind_info(object),
                &draw_command(object, hash),
                geometry(object, hash ^ 1),
            )
            .expect_err("hash drift must fail");

        assert!(err.contains("vertex payload hash mismatch"));
    }

    #[test]
    fn aux_clear_material_runtime_plan_requires_matching_pipeline_count() {
        let err =
            NativeVulkanSceneLayerAuxMaterialClearRuntimeCommandPlan::from_frame_resources_and_plans(
                &NativeVulkanSceneFrameResources::new(),
                &NativeVulkanSceneLayerAuxMaterialDrawFramePlan {
                    active_block_count: 1,
                    command_count: 1,
                    draw_receiver_count: 2,
                    non_indexed_draw_receiver_count: 1,
                    indexed_draw_receiver_count: 1,
                    retained_active_geometry_count: 1,
                    commands: Vec::new(),
                    command_order: [
                        "load_aux_clear_prep_commands",
                        "resolve_aux_material_targets",
                        "emit_aux_clear_material_target_receiver",
                        "emit_aux_generated_material_target_receiver",
                        "validate_active_mdlv_geometry_residency",
                        "preserve_wrapper_arguments",
                        "feed_aux_clear_prep_recorder_without_mesh_owner",
                        "keep_resource_heap_binding_model",
                    ],
                },
                &NativeVulkanSceneLayerAuxMaterialPipelineFramePlan::empty(),
            )
            .expect_err("missing pipeline must fail");

        assert!(err.contains("one util/passthrough pipeline"));
    }

    fn pipeline_key(object: SceneObjectId) -> NativeVulkanSceneLayerAuxMaterialPipelineKeyPlan {
        NativeVulkanSceneLayerAuxMaterialPipelineKeyPlan {
            command_index: 7,
            block_index: 11,
            object,
            material: WE_AUX_FULLSCREEN_LAYER_MATERIAL,
            shader: WE_AUX_FULLSCREEN_LAYER_SHADER,
            source: WE_AUX_FULLSCREEN_LAYER_TEXTURE_SOURCE,
            target: SceneGraphTarget::LayerAuxClear(object),
            target_format: "R8G8B8A8_UNORM",
            texture_slot: WE_AUX_FULLSCREEN_LAYER_TEXTURE_SLOT,
            texture_slot_mask: 1,
            pipeline_class: SceneGraphPipelineClass::LayerUtilityIndexed,
            vertex_layout: NativeVulkanScenePipelineVertexLayout::FlatTexturePositionUv,
            resource_heap: NativeVulkanScenePipelineResourceHeapClass::LayerAuxMaterial,
            draw_receiver:
                NativeVulkanSceneLayerAuxMaterialDrawReceiverKind::Aux3f0ClearMaterialNonIndexed,
            command_order: [
                "read_materials_util_fullscreenlayer_json",
                "select_util_passthrough_shader",
                "bind_rt_full_frame_buffer_as_g_texture0",
                "select_aux_0x3e8_color_target_format",
                "select_position_uv_triangle_receiver_aux_0x3f0",
                "derive_resource_heap_scoped_pipeline_key",
            ],
        }
    }

    fn bind_info(object: SceneObjectId) -> NativeVulkanSceneLayerAuxMaterialResourceHeapBindInfo {
        NativeVulkanSceneLayerAuxMaterialResourceHeapBindInfo {
            clear_bind_index: 3,
            command_index: 7,
            block_index: 11,
            object,
            material: WE_AUX_FULLSCREEN_LAYER_MATERIAL,
            shader: WE_AUX_FULLSCREEN_LAYER_SHADER,
            source: WE_AUX_FULLSCREEN_LAYER_TEXTURE_SOURCE,
            source_target: SceneGraphTarget::ObjectFinal(object),
            target: SceneGraphTarget::LayerAuxClear(object),
            texture_slot: WE_AUX_FULLSCREEN_LAYER_TEXTURE_SLOT,
            heap_slice_index: 5,
            base_resource_descriptor_index: 13,
            base_sampler_descriptor_index: 17,
            resource_descriptor_count: 1,
            texture_count: 1,
            shader_mappings: vec!["we.texture_slot0.g_Texture0 -> aux-material".to_owned()],
            resource_bind: vk::BindHeapInfoEXT::builder().build(),
            sampler_bind: vk::BindHeapInfoEXT::builder().build(),
        }
    }

    fn draw_command(
        object: SceneObjectId,
        payload_hash: u64,
    ) -> NativeVulkanSceneLayerAuxMaterialDrawCommandPlan {
        NativeVulkanSceneLayerAuxMaterialDrawCommandPlan {
            command_index: 7,
            block_index: 11,
            object,
            clear_material: clear_receiver(payload_hash),
            generated_material: generated_receiver(object),
            reference_points: [
                "reverse-engineered/docs/exe/blend-and-render.md: 0x140207740 draws [aux+0x410]->[aux+0x3f0] and [aux+0x408]->[aux+0x3f8]",
                "reverse-engineered/docs/exe/d3d11-context-calls.md: wrapper [9]/+0x48 and wrapper [8]/+0x40 create target-like draw receivers",
                "reverse-engineered/tools/audit_opacity_final_alpha_path.py: 0x14020a3ea stores aux+0x3f0",
                "reverse-engineered/docs/exe/clipping-pipeline.md: 0x14020b1e8 stores aux+0x3f8 from active material entry",
                "0x14020a379..0x14020a390 releases the previous aux+0x3f0 before replacement",
                "references/godot/servers/rendering/rendering_device_graph.cpp: draw resources are explicit graph inputs before recording",
            ],
            command_order: [
                "load_aux_clear_prep_command",
                "materialize_aux_0x3f0_non_indexed_receiver_contract",
                "materialize_aux_0x3f8_indexed_receiver_contract",
                "require_retained_active_mdlv_geometry_for_aux_0x3f8",
                "preserve_wrapper_create_arguments",
                "feed_aux_clear_prep_recorder_without_mesh_owner",
                "keep_resource_heap_binding_model",
            ],
        }
    }

    fn clear_receiver(payload_hash: u64) -> NativeVulkanSceneLayerAuxMaterialDrawReceiverPlan {
        NativeVulkanSceneLayerAuxMaterialDrawReceiverPlan {
            receiver_kind:
                NativeVulkanSceneLayerAuxMaterialDrawReceiverKind::Aux3f0ClearMaterialNonIndexed,
            material_offset: 0x410,
            target_offset: 0x3f0,
            create_call_vma: 0x14020a3ea,
            store_vma: 0x14020a3f0,
            wrapper_create_vma: WE_RT_TARGET_WRAPPER_CREATE_NON_INDEXED_VMA,
            target_vptr: WE_RT_TARGET_VPTR,
            draw_method_vma: 0x1400ea780,
            layout_key_source: "0x140098c30([0,7]) -> 0x9",
            vertex_payload_source: "stack triangle at 0x14020a2d2..0x14020a379",
            index_payload_source: None,
            layout_key_helper_vma: 0x140098c30,
            attribute_ids: WE_RT_TARGET_POSITION_UV_ATTR_IDS,
            layout_bitmask: WE_RT_TARGET_POSITION_UV_LAYOUT_BITMASK,
            vertex_stride_bytes: WE_RT_TARGET_POSITION_UV_STRIDE_BYTES,
            vertex_count: WE_AUX_MATERIAL_CLEAR_VERTEX_COUNT,
            vertex_bytes: WE_AUX_MATERIAL_CLEAR_VERTEX_BYTES,
            index_count: 0,
            index_width_selector: None,
            topology_selector: 0,
            stack_usage_byte: Some(0),
            active_entry_owner_index: None,
            retained_vertex_bytes: Some(WE_AUX_MATERIAL_CLEAR_VERTEX_BYTES),
            retained_index_bytes: None,
            clear_triangle_payload: Some(clear_payload_plan(payload_hash)),
            reference_points: [
                "0x14020a2d2..0x14020a379 fills the 3*20-byte stack vertex payload",
                "0x14020a3ba..0x14020a3d2 computes layout key from attr ids [0,7]",
                "0x14020a3d4..0x14020a3ea calls wrapper +0x48 with r9d=3 and topology selector 0",
                "0x14020a3f0 stores the created receiver at [aux+0x3f0]",
            ],
            command_order: [
                "release_previous_aux_0x3f0_receiver",
                "build_position_uv_layout_key",
                "emit_three_vertex_position_uv_triangle",
                "create_non_indexed_target_like_receiver",
                "store_aux_0x3f0_receiver",
                "draw_under_aux_0x410_material_scope",
            ],
        }
    }

    fn generated_receiver(
        object: SceneObjectId,
    ) -> NativeVulkanSceneLayerAuxMaterialDrawReceiverPlan {
        let _ = object;
        NativeVulkanSceneLayerAuxMaterialDrawReceiverPlan {
            receiver_kind:
                NativeVulkanSceneLayerAuxMaterialDrawReceiverKind::Aux3f8GeneratedMaterialIndexed,
            material_offset: 0x408,
            target_offset: 0x3f8,
            create_call_vma: 0x14020b1e8,
            store_vma: 0x14020b1f3,
            wrapper_create_vma: 0x14009a880,
            target_vptr: WE_RT_TARGET_VPTR,
            draw_method_vma: 0x1400eacd0,
            layout_key_source: "active material entry +0x38",
            vertex_payload_source: "active material entry +0x48",
            index_payload_source: Some("active material entry +0x58"),
            layout_key_helper_vma: 0,
            attribute_ids: [0, 0],
            layout_bitmask: 0x99,
            vertex_stride_bytes: 20,
            vertex_count: 3,
            vertex_bytes: 60,
            index_count: 3,
            index_width_selector: Some(0),
            topology_selector: 0,
            stack_usage_byte: Some(0),
            active_entry_owner_index: Some(0),
            retained_vertex_bytes: Some(60),
            retained_index_bytes: Some(6),
            clear_triangle_payload: None,
            reference_points: [
                "0x14020b171 gates the active material upload from [aux+0x390]",
                "0x14020b17b..0x14020b1e3 passes active entry layout/vertex/index payload through wrapper [8]",
                "0x14020b182 forces stack usage byte 0; index/topology selectors are 0",
                "0x14020b1f3 stores the created receiver at [aux+0x3f8]",
            ],
            command_order: [
                "resolve_active_material_entry",
                "validate_retained_mdlv_vertex_index_payload",
                "create_indexed_target_like_receiver",
                "store_aux_0x3f8_receiver",
                "draw_under_aux_0x408_material_scope",
                "preserve_static_r16_triangle_list",
            ],
        }
    }

    fn clear_payload_plan(_payload_hash: u64) -> NativeVulkanSceneLayerAuxClearTrianglePayloadPlan {
        NativeVulkanSceneLayerAuxClearTrianglePayloadPlan {
            create_region: "0x14020a2d2..0x14020a379",
            position_constants_vma: [0x140492704, 0x140492ff0],
            uv_formula_region: "0x14020a2f1..0x14020a33c",
            flip_flag_source: "[[layer+0xc8]+0x118] bit0",
            source_width: 3840,
            source_height: 2160,
            target_width: 1920,
            target_height: 1080,
            uv_y_flipped: false,
            uv_x_scale_bits: (2.0f32 * 1920.0 / 3840.0).to_bits(),
            uv_y_scale_bits: (1080.0f32 / 2160.0).to_bits(),
            clip_positions_bits: [
                [(-1.0f32).to_bits(), 1.0f32.to_bits(), 0.0f32.to_bits()],
                [(-1.0f32).to_bits(), (-3.0f32).to_bits(), 0.0f32.to_bits()],
                [3.0f32.to_bits(), 1.0f32.to_bits(), 0.0f32.to_bits()],
            ],
            uv_x_formula: ["0", "0", "2*target_width/source_width"],
            uv_y_normal_formula: [
                "target_height/source_height",
                "-target_height/source_height",
                "target_height/source_height",
            ],
            uv_y_flipped_formula: ["0", "2*target_width/source_width", "0"],
        }
    }

    fn geometry(
        object: SceneObjectId,
        payload_hash: u64,
    ) -> NativeVulkanSceneLayerAuxMaterialClearGeometryBuffers {
        let geometry = NativeVulkanSceneLayerAuxMaterialClearGeometry { object };
        NativeVulkanSceneLayerAuxMaterialClearGeometryBuffers {
            geometry,
            vertex: NativeVulkanSceneGpuBufferBinding {
                key: NativeVulkanSceneGpuBufferKey {
                    owner: NativeVulkanSceneGpuBufferOwner::LayerAuxMaterialClearGeometry(geometry),
                    role: NativeVulkanSceneGpuBufferRole::LayerAuxMaterialClearVertex,
                },
                buffer: vk::Buffer::from_raw(0x1234),
                bytes: WE_AUX_MATERIAL_CLEAR_VERTEX_BYTES,
                payload_hash,
            },
        }
    }
}
