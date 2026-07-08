//! Runtime command contract for WE auxiliary generated active-entry draws.
//!
//! References:
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::vk::{self, Handle};

use crate::engine::scene_engine::SceneObjectId;

use super::frame_resources::NativeVulkanSceneFrameResources;
use super::layer_aux_material_commands::{
    NativeVulkanSceneLayerAuxMaterialCommandFramePlan,
    NativeVulkanSceneLayerAuxMaterialCommandPlan, NativeVulkanSceneLayerAuxScopedMaterialDrawKind,
};
use super::layer_aux_material_draws::{
    NativeVulkanSceneLayerAuxMaterialDrawCommandPlan,
    NativeVulkanSceneLayerAuxMaterialDrawFramePlan,
    NativeVulkanSceneLayerAuxMaterialDrawReceiverKind,
};
use super::layer_aux_material_pipeline::{
    NativeVulkanSceneLayerAuxGeneratedMaterialPipelineRequirement,
    NativeVulkanSceneLayerAuxMaterialPipelineFramePlan,
};
use super::resource_buffers::{
    NativeVulkanSceneGpuBufferBinding, NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvGeometryBuffers,
};
use super::resource_storage::{
    NativeVulkanSceneGpuBufferOwner, NativeVulkanSceneGpuBufferRole,
    NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAuxMaterialGeneratedRuntimeCommandPlan
{
    pub command_count: usize,
    pub pipeline_requirement_count: usize,
    pub resource_heap_requirement_count: usize,
    pub uniform_requirement_count: usize,
    pub geometry_bind_count: usize,
    pub indexed_draw_count: usize,
    pub r16_index_draw_count: usize,
    pub commands: Vec<NativeVulkanSceneLayerAuxMaterialGeneratedIndexedDrawCommandPlan>,
    pub command_order: [&'static str; 7],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAuxMaterialGeneratedIndexedDrawCommandPlan
{
    pub command_index: usize,
    pub block_index: usize,
    pub object: SceneObjectId,
    pub entry_owner_index: u32,
    pub material_offset: u32,
    pub target_offset: u32,
    pub layout_bitmask: u32,
    pub vertex_stride_bytes: u32,
    pub vertex_count: u32,
    pub index_count: u32,
    pub vertex: NativeVulkanSceneLayerAuxMaterialGeneratedBufferBindingPlan,
    pub index: NativeVulkanSceneLayerAuxMaterialGeneratedBufferBindingPlan,
    pub target_draw_method_vma: u64,
    pub state_prep_region: &'static str,
    pub state_cleanup_region: &'static str,
    pub generated_active_entry_blend_byte_source: &'static str,
    pub generated_vec4_source: &'static str,
    pub pipeline_requirement: &'static str,
    pub resource_heap_requirement: &'static str,
    pub uniform_requirement: &'static str,
    pub index_type: &'static str,
    pub draw_call: &'static str,
    pub command_order: [&'static str; 8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAuxMaterialGeneratedBufferBindingPlan
{
    pub owner: NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry,
    pub role: NativeVulkanSceneGpuBufferRole,
    pub buffer_handle: u64,
    pub bytes: u64,
    pub payload_hash: u64,
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_aux_material_generated_runtime_commands(
    frame_resources: &NativeVulkanSceneFrameResources,
    material_draws: &NativeVulkanSceneLayerAuxMaterialDrawFramePlan,
    material_commands: &NativeVulkanSceneLayerAuxMaterialCommandFramePlan,
    pipelines: &NativeVulkanSceneLayerAuxMaterialPipelineFramePlan,
) -> Result<NativeVulkanSceneLayerAuxMaterialGeneratedRuntimeCommandPlan, String> {
    NativeVulkanSceneLayerAuxMaterialGeneratedRuntimeCommandPlan::from_frame_resources_and_plans(
        frame_resources,
        material_draws,
        material_commands,
        pipelines,
    )
}

impl NativeVulkanSceneLayerAuxMaterialGeneratedRuntimeCommandPlan {
    fn from_frame_resources_and_plans(
        frame_resources: &NativeVulkanSceneFrameResources,
        material_draws: &NativeVulkanSceneLayerAuxMaterialDrawFramePlan,
        material_commands: &NativeVulkanSceneLayerAuxMaterialCommandFramePlan,
        pipelines: &NativeVulkanSceneLayerAuxMaterialPipelineFramePlan,
    ) -> Result<Self, String> {
        if pipelines.generated_requirements.is_empty() && material_commands.command_count == 0 {
            return Ok(Self::empty());
        }
        if pipelines.generated_requirements.len() != material_commands.command_count
            || material_commands.command_count != material_draws.command_count
        {
            return Err(format!(
                "scene aux generated material runtime needs one active-entry requirement per material command, got requirements={} commands={} draws={}",
                pipelines.generated_requirements.len(),
                material_commands.command_count,
                material_draws.command_count
            ));
        }

        let mut commands = Vec::with_capacity(pipelines.generated_requirements.len());
        for requirement in &pipelines.generated_requirements {
            let material_command = material_commands
                .commands
                .iter()
                .find(|command| {
                    command.command_index == requirement.command_index
                        && command.block_index == requirement.block_index
                        && command.object == requirement.object
                })
                .ok_or_else(|| {
                    format!(
                        "scene aux generated material command {} has no scoped material command",
                        requirement.command_index
                    )
                })?;
            let material_draw = material_draws
                .commands
                .iter()
                .find(|draw| {
                    draw.command_index == requirement.command_index
                        && draw.block_index == requirement.block_index
                        && draw.object == requirement.object
                })
                .ok_or_else(|| {
                    format!(
                        "scene aux generated material command {} has no active-entry draw receiver",
                        requirement.command_index
                    )
                })?;
            let entry_owner_index = material_draw
                .generated_material
                .active_entry_owner_index
                .ok_or_else(|| {
                    format!(
                        "scene aux generated material command {} has no active entry owner index",
                        requirement.command_index
                    )
                })?;
            let geometry_key = NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry {
                object: requirement.object,
                entry_owner_index,
            };
            let geometry = frame_resources
                .layer_alpha_mask_rt_method8_mdlv_geometry_buffers(geometry_key)
                .map_err(|err| {
                    format!(
                        "{err}; scene aux generated material command {} requires retained active-entry MDLV geometry",
                        requirement.command_index
                    )
                })?;
            commands.push(
                NativeVulkanSceneLayerAuxMaterialGeneratedIndexedDrawCommandPlan::from_requirement_material_draw_and_geometry(
                    requirement,
                    material_command,
                    material_draw,
                    geometry,
                )?,
            );
        }

        Ok(Self::from_commands(commands))
    }

    pub(in crate::renderer::native_vulkan) fn covers_material_commands_and_requirements(
        &self,
        material_commands: &NativeVulkanSceneLayerAuxMaterialCommandFramePlan,
        pipelines: &NativeVulkanSceneLayerAuxMaterialPipelineFramePlan,
    ) -> bool {
        self.command_count == material_commands.command_count
            && self.command_count == pipelines.generated_requirements.len()
            && pipelines.generated_requirements.iter().all(|requirement| {
                self.commands.iter().any(|command| {
                    command.command_index == requirement.command_index
                        && command.block_index == requirement.block_index
                        && command.object == requirement.object
                })
            })
    }

    fn from_commands(
        commands: Vec<NativeVulkanSceneLayerAuxMaterialGeneratedIndexedDrawCommandPlan>,
    ) -> Self {
        Self {
            command_count: commands.len(),
            pipeline_requirement_count: commands.len(),
            resource_heap_requirement_count: commands.len(),
            uniform_requirement_count: commands.len(),
            geometry_bind_count: commands.len(),
            indexed_draw_count: commands.len(),
            r16_index_draw_count: commands.len(),
            commands,
            command_order: [
                "read_aux_generated_active_entry_requirements",
                "match_aux_0x408_scope_to_active_entry_geometry",
                "resolve_retained_mdlv_vertex_index_buffers",
                "validate_active_entry_layout_stride_counts",
                "preserve_generated_state_sources",
                "require_generated_material_pipeline_heap_uniforms",
                "build_aux_0x3f8_indexed_draw_contract",
            ],
        }
    }

    fn empty() -> Self {
        Self {
            command_count: 0,
            pipeline_requirement_count: 0,
            resource_heap_requirement_count: 0,
            uniform_requirement_count: 0,
            geometry_bind_count: 0,
            indexed_draw_count: 0,
            r16_index_draw_count: 0,
            commands: Vec::new(),
            command_order: [
                "read_aux_generated_active_entry_requirements",
                "match_aux_0x408_scope_to_active_entry_geometry",
                "resolve_retained_mdlv_vertex_index_buffers",
                "validate_active_entry_layout_stride_counts",
                "preserve_generated_state_sources",
                "require_generated_material_pipeline_heap_uniforms",
                "build_aux_0x3f8_indexed_draw_contract",
            ],
        }
    }
}

impl NativeVulkanSceneLayerAuxMaterialGeneratedIndexedDrawCommandPlan {
    fn from_requirement_material_draw_and_geometry(
        requirement: &NativeVulkanSceneLayerAuxGeneratedMaterialPipelineRequirement,
        material_command: &NativeVulkanSceneLayerAuxMaterialCommandPlan,
        material_draw: &NativeVulkanSceneLayerAuxMaterialDrawCommandPlan,
        geometry: NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvGeometryBuffers,
    ) -> Result<Self, String> {
        validate_generated_requirement(requirement, material_command, material_draw)?;
        let entry_owner_index = material_draw
            .generated_material
            .active_entry_owner_index
            .ok_or_else(|| {
                format!(
                    "scene aux generated material command {} has no active entry owner index",
                    requirement.command_index
                )
            })?;
        let expected_geometry = NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry {
            object: requirement.object,
            entry_owner_index,
        };
        if geometry.geometry != expected_geometry {
            return Err(format!(
                "scene aux generated material command {} geometry mismatch: expected {:?}, got {:?}",
                requirement.command_index, expected_geometry, geometry.geometry
            ));
        }
        let expected_vertex_bytes = material_draw
            .generated_material
            .retained_vertex_bytes
            .ok_or_else(|| {
                format!(
                    "scene aux generated material command {} missing retained vertex byte fact",
                    requirement.command_index
                )
            })?;
        let expected_index_bytes = material_draw
            .generated_material
            .retained_index_bytes
            .ok_or_else(|| {
                format!(
                    "scene aux generated material command {} missing retained index byte fact",
                    requirement.command_index
                )
            })?;
        let vertex = validate_generated_buffer(
            requirement.command_index,
            geometry.geometry,
            geometry.vertex,
            NativeVulkanSceneGpuBufferRole::LayerAlphaMaskRtMethod8MdlvVertex,
            expected_vertex_bytes,
        )?;
        let index = validate_generated_buffer(
            requirement.command_index,
            geometry.geometry,
            geometry.index,
            NativeVulkanSceneGpuBufferRole::LayerAlphaMaskRtMethod8MdlvIndex,
            expected_index_bytes,
        )?;
        let generated_draw = &material_command.generated_material_draw;
        Ok(Self {
            command_index: requirement.command_index,
            block_index: requirement.block_index,
            object: requirement.object,
            entry_owner_index,
            material_offset: requirement.material_offset,
            target_offset: requirement.target_offset,
            layout_bitmask: requirement.layout_bitmask,
            vertex_stride_bytes: requirement.vertex_stride_bytes,
            vertex_count: requirement.vertex_count,
            index_count: requirement.index_count,
            vertex,
            index,
            target_draw_method_vma: generated_draw.target_draw_method_vma,
            state_prep_region: generated_draw.state_prep_region.ok_or_else(|| {
                format!(
                    "scene aux generated material command {} missing generated state prep region",
                    requirement.command_index
                )
            })?,
            state_cleanup_region: generated_draw.state_cleanup_region.ok_or_else(|| {
                format!(
                    "scene aux generated material command {} missing generated state cleanup region",
                    requirement.command_index
                )
            })?,
            generated_active_entry_blend_byte_source: generated_draw
                .generated_active_entry_blend_byte_source
                .ok_or_else(|| {
                    format!(
                        "scene aux generated material command {} missing active-entry blend byte source",
                        requirement.command_index
                    )
                })?,
            generated_vec4_source: generated_draw.generated_vec4_source.ok_or_else(|| {
                format!(
                    "scene aux generated material command {} missing generated vec4 source",
                    requirement.command_index
                )
            })?,
            pipeline_requirement: requirement.shader_source_required,
            resource_heap_requirement: generated_draw.resource_heap_status,
            uniform_requirement: "state+0x12e9/state+0x12ec generated material constants must be resident before draw",
            index_type: "VK_INDEX_TYPE_UINT16",
            draw_call: "vkCmdDrawIndexed",
            command_order: [
                "prepare_generated_material_state_0x14020785e",
                "bind_aux_0x408_material_scope",
                "bind_generated_material_pipeline_heap_uniforms",
                "bind_aux_0x3f8_active_entry_vertex_buffer",
                "bind_aux_0x3f8_active_entry_r16_index_buffer",
                "cmd_draw_indexed_aux_0x3f8",
                "release_aux_0x408_material_scope",
                "cleanup_generated_material_state_0x140207ac7",
            ],
        })
    }
}

fn validate_generated_requirement(
    requirement: &NativeVulkanSceneLayerAuxGeneratedMaterialPipelineRequirement,
    material_command: &NativeVulkanSceneLayerAuxMaterialCommandPlan,
    material_draw: &NativeVulkanSceneLayerAuxMaterialDrawCommandPlan,
) -> Result<(), String> {
    if material_command.command_index != requirement.command_index
        || material_command.block_index != requirement.block_index
        || material_command.object != requirement.object
        || material_draw.command_index != requirement.command_index
        || material_draw.block_index != requirement.block_index
        || material_draw.object != requirement.object
    {
        return Err(format!(
            "scene aux generated material command {} has mismatched command/material/draw identity",
            requirement.command_index
        ));
    }
    let command_draw = &material_command.generated_material_draw;
    let receiver = &material_draw.generated_material;
    if command_draw.draw_kind
        != NativeVulkanSceneLayerAuxScopedMaterialDrawKind::GeneratedMaterialAux408ToAux3f8
        || command_draw.receiver_kind
            != NativeVulkanSceneLayerAuxMaterialDrawReceiverKind::Aux3f8GeneratedMaterialIndexed
        || receiver.receiver_kind
            != NativeVulkanSceneLayerAuxMaterialDrawReceiverKind::Aux3f8GeneratedMaterialIndexed
        || requirement.draw_receiver
            != NativeVulkanSceneLayerAuxMaterialDrawReceiverKind::Aux3f8GeneratedMaterialIndexed
    {
        return Err(format!(
            "scene aux generated material command {} requires aux+0x408 -> aux+0x3f8 indexed receiver",
            requirement.command_index
        ));
    }
    if requirement.layout_bitmask != receiver.layout_bitmask
        || requirement.vertex_stride_bytes != receiver.vertex_stride_bytes
        || requirement.vertex_count != receiver.vertex_count
        || requirement.index_count != receiver.index_count
        || command_draw.layout_bitmask != receiver.layout_bitmask
        || command_draw.vertex_stride_bytes != receiver.vertex_stride_bytes
        || command_draw.vertex_count != receiver.vertex_count
        || command_draw.index_count != receiver.index_count
    {
        return Err(format!(
            "scene aux generated material command {} active-entry geometry facts drift",
            requirement.command_index
        ));
    }
    Ok(())
}

fn validate_generated_buffer(
    command_index: usize,
    geometry: NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry,
    binding: NativeVulkanSceneGpuBufferBinding,
    expected_role: NativeVulkanSceneGpuBufferRole,
    expected_bytes: u64,
) -> Result<NativeVulkanSceneLayerAuxMaterialGeneratedBufferBindingPlan, String> {
    let expected_owner =
        NativeVulkanSceneGpuBufferOwner::LayerAlphaMaskRtMethod8MdlvEntry(geometry);
    if binding.key.owner != expected_owner || binding.key.role != expected_role {
        return Err(format!(
            "scene aux generated material command {command_index} requires {:?}/{:?}, got {:?}",
            expected_owner, expected_role, binding.key
        ));
    }
    if binding.buffer == vk::Buffer::null() {
        return Err(format!(
            "scene aux generated material command {command_index} has null retained {:?} buffer",
            expected_role
        ));
    }
    if binding.bytes != expected_bytes {
        return Err(format!(
            "scene aux generated material command {command_index} {:?} byte mismatch: expected {}, got {}",
            expected_role, expected_bytes, binding.bytes
        ));
    }
    Ok(
        NativeVulkanSceneLayerAuxMaterialGeneratedBufferBindingPlan {
            owner: geometry,
            role: expected_role,
            buffer_handle: binding.buffer.as_raw(),
            bytes: binding.bytes,
            payload_hash: binding.payload_hash,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::WE_LAYER_AUX_GENERATED_MATERIAL_OFFSET;
    use crate::renderer::native_vulkan::scene_backend::layer_aux_material_commands::{
        NativeVulkanSceneLayerAuxScopedMaterialDrawPlan,
        WE_AUX_GENERATED_ACTIVE_ENTRY_BLEND_BYTE_SOURCE, WE_AUX_GENERATED_MATERIAL_BIND_CALL_VMA,
        WE_AUX_GENERATED_MATERIAL_RELEASE_CALL_VMA, WE_AUX_GENERATED_MATERIAL_TARGET_DRAW_CALL_VMA,
        WE_AUX_GENERATED_STATE_CLEANUP_REGION, WE_AUX_GENERATED_STATE_PREP_REGION,
        WE_AUX_GENERATED_VEC4_SOURCE, WE_AUX_MATERIAL_BIND_HELPER_VMA,
        WE_AUX_MATERIAL_RELEASE_HELPER_VMA,
    };
    use crate::renderer::native_vulkan::scene_backend::layer_aux_material_draws::{
        NativeVulkanSceneLayerAuxMaterialDrawReceiverPlan, WE_RT_TARGET_INDEXED_DRAW_VMA,
        WE_RT_TARGET_VPTR, WE_RT_TARGET_WRAPPER_CREATE_INDEXED_VMA,
    };
    use crate::renderer::native_vulkan::scene_backend::resource_buffers::{
        NativeVulkanSceneGpuBufferKey, NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvGeometryBuffers,
    };

    #[test]
    fn aux_generated_command_uses_retained_active_entry_geometry() {
        let object = SceneObjectId(1530);
        let command = NativeVulkanSceneLayerAuxMaterialGeneratedIndexedDrawCommandPlan::from_requirement_material_draw_and_geometry(
            &requirement(object),
            &material_command(object),
            &material_draw(object),
            geometry_buffers(object, 0x1111, 0x2222),
        )
        .expect("generated command");

        assert_eq!(command.command_index, 9);
        assert_eq!(command.object, object);
        assert_eq!(command.entry_owner_index, 4);
        assert_eq!(command.layout_bitmask, 0x180000f);
        assert_eq!(command.vertex_stride_bytes, 80);
        assert_eq!(command.vertex_count, 4106);
        assert_eq!(command.index_count, 23_988);
        assert_eq!(command.vertex.bytes, 328_480);
        assert_eq!(command.index.bytes, 47_976);
        assert_eq!(command.index_type, "VK_INDEX_TYPE_UINT16");
        assert_eq!(command.draw_call, "vkCmdDrawIndexed");
    }

    #[test]
    fn aux_generated_command_rejects_wrong_index_owner() {
        let object = SceneObjectId(1530);
        let mut geometry = geometry_buffers(object, 0x1111, 0x2222);
        geometry.index.key.owner =
            NativeVulkanSceneGpuBufferOwner::LayerAlphaMaskRtMethod8MdlvEntry(
                NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry {
                    object,
                    entry_owner_index: 5,
                },
            );

        let err = NativeVulkanSceneLayerAuxMaterialGeneratedIndexedDrawCommandPlan::from_requirement_material_draw_and_geometry(
            &requirement(object),
            &material_command(object),
            &material_draw(object),
            geometry,
        )
        .expect_err("wrong owner must fail");

        assert!(err.contains("requires"));
    }

    fn requirement(
        object: SceneObjectId,
    ) -> NativeVulkanSceneLayerAuxGeneratedMaterialPipelineRequirement {
        NativeVulkanSceneLayerAuxGeneratedMaterialPipelineRequirement {
            command_index: 9,
            block_index: 2,
            object,
            draw_receiver:
                NativeVulkanSceneLayerAuxMaterialDrawReceiverKind::Aux3f8GeneratedMaterialIndexed,
            material_offset: WE_LAYER_AUX_GENERATED_MATERIAL_OFFSET,
            target_offset: 0x3f8,
            layout_bitmask: 0x180000f,
            vertex_stride_bytes: 80,
            vertex_count: 4106,
            index_count: 23_988,
            material_entry_source: "active material entry [aux+0x18] + [aux+0x390] * 0xc8",
            shader_source_required: "generated aux+0x408 material shader and resource heap slice must come from retained material entry, not mesh fallback",
            command_order: [
                "preserve_generated_material_state_prep",
                "read_active_material_entry_shader_contract",
                "derive_generated_material_pipeline_key_from_entry",
                "bind_generated_material_resource_heap_slice",
                "record_indexed_aux_0x3f8_receiver_draw",
            ],
        }
    }

    fn material_command(object: SceneObjectId) -> NativeVulkanSceneLayerAuxMaterialCommandPlan {
        NativeVulkanSceneLayerAuxMaterialCommandPlan {
            command_index: 9,
            block_index: 2,
            object,
            clear_scope_command_index: 9,
            clear_material_draw: generated_scoped_draw(object),
            generated_material_draw: generated_scoped_draw(object),
            target_restore_region: "0x140207b02..0x140207b39",
            reference_points: ["test", "test", "test", "test", "test"],
            command_order: [
                "enter_aux_clear_scope",
                "bind_aux_0x410_material",
                "draw_aux_0x3f0_target_receiver",
                "release_aux_0x410_material",
                "prepare_aux_generated_material_state",
                "bind_aux_0x408_material_and_draw_aux_0x3f8",
                "cleanup_aux_generated_material_state",
                "restore_parent_target_scope",
            ],
        }
    }

    fn material_draw(object: SceneObjectId) -> NativeVulkanSceneLayerAuxMaterialDrawCommandPlan {
        NativeVulkanSceneLayerAuxMaterialDrawCommandPlan {
            command_index: 9,
            block_index: 2,
            object,
            clear_material: generated_receiver(object),
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

    fn generated_scoped_draw(
        _object: SceneObjectId,
    ) -> NativeVulkanSceneLayerAuxScopedMaterialDrawPlan {
        NativeVulkanSceneLayerAuxScopedMaterialDrawPlan {
            draw_kind:
                NativeVulkanSceneLayerAuxScopedMaterialDrawKind::GeneratedMaterialAux408ToAux3f8,
            material_offset: WE_LAYER_AUX_GENERATED_MATERIAL_OFFSET,
            target_offset: 0x3f8,
            receiver_kind:
                NativeVulkanSceneLayerAuxMaterialDrawReceiverKind::Aux3f8GeneratedMaterialIndexed,
            bind_helper_vma: WE_AUX_MATERIAL_BIND_HELPER_VMA,
            bind_call_vma: WE_AUX_GENERATED_MATERIAL_BIND_CALL_VMA,
            target_draw_call_vma: WE_AUX_GENERATED_MATERIAL_TARGET_DRAW_CALL_VMA,
            release_helper_vma: WE_AUX_MATERIAL_RELEASE_HELPER_VMA,
            release_call_vma: WE_AUX_GENERATED_MATERIAL_RELEASE_CALL_VMA,
            target_draw_method_vma: WE_RT_TARGET_INDEXED_DRAW_VMA,
            layout_bitmask: 0x180000f,
            vertex_stride_bytes: 80,
            vertex_count: 4106,
            index_count: 23_988,
            state_prep_region: Some(WE_AUX_GENERATED_STATE_PREP_REGION),
            state_cleanup_region: Some(WE_AUX_GENERATED_STATE_CLEANUP_REGION),
            generated_active_entry_blend_byte_source: Some(
                WE_AUX_GENERATED_ACTIVE_ENTRY_BLEND_BYTE_SOURCE,
            ),
            generated_vec4_source: Some(WE_AUX_GENERATED_VEC4_SOURCE),
            matrix_stack_operation: Some("test"),
            color_factor_operation: Some("test"),
            pipeline_status: "requires aux generated material pipeline binding before vkCmdDrawIndexed",
            resource_heap_status: "requires aux generated material resource heap slice and generated uniform state before draw",
            command_order: [
                "prepare_generated_material_state",
                "bind_material_scope_0x140155fc0",
                "commit_material_state",
                "bind_aux_0x3f8_target_indexed_stream",
                "record_target_vtable_1_draw",
                "release_material_scope_0x140157430",
                "cleanup_generated_material_state",
            ],
        }
    }

    fn generated_receiver(
        _object: SceneObjectId,
    ) -> NativeVulkanSceneLayerAuxMaterialDrawReceiverPlan {
        NativeVulkanSceneLayerAuxMaterialDrawReceiverPlan {
            receiver_kind:
                NativeVulkanSceneLayerAuxMaterialDrawReceiverKind::Aux3f8GeneratedMaterialIndexed,
            material_offset: WE_LAYER_AUX_GENERATED_MATERIAL_OFFSET,
            target_offset: 0x3f8,
            create_call_vma: 0x14020b1e8,
            store_vma: 0x14020b1f3,
            wrapper_create_vma: WE_RT_TARGET_WRAPPER_CREATE_INDEXED_VMA,
            target_vptr: WE_RT_TARGET_VPTR,
            draw_method_vma: WE_RT_TARGET_INDEXED_DRAW_VMA,
            layout_key_source: "[aux+0x18] + [aux+0x390] * 0xc8 + 0x38",
            vertex_payload_source: "active material entry +0x48, count +0x40/+0x3c",
            index_payload_source: Some("active material entry +0x58, count +0x50/2"),
            layout_key_helper_vma: 0,
            attribute_ids: [0, 0],
            layout_bitmask: 0x180000f,
            vertex_stride_bytes: 80,
            vertex_count: 4106,
            vertex_bytes: 328_480,
            index_count: 23_988,
            index_width_selector: Some(0),
            topology_selector: 0,
            stack_usage_byte: Some(0),
            active_entry_owner_index: Some(4),
            retained_vertex_bytes: Some(328_480),
            retained_index_bytes: Some(47_976),
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

    fn geometry_buffers(
        object: SceneObjectId,
        vertex_hash: u64,
        index_hash: u64,
    ) -> NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvGeometryBuffers {
        let geometry = NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry {
            object,
            entry_owner_index: 4,
        };
        NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvGeometryBuffers {
            geometry,
            vertex: buffer(
                geometry,
                NativeVulkanSceneGpuBufferRole::LayerAlphaMaskRtMethod8MdlvVertex,
                vk::Buffer::from_raw(0x1000),
                328_480,
                vertex_hash,
            ),
            index: buffer(
                geometry,
                NativeVulkanSceneGpuBufferRole::LayerAlphaMaskRtMethod8MdlvIndex,
                vk::Buffer::from_raw(0x2000),
                47_976,
                index_hash,
            ),
        }
    }

    fn buffer(
        geometry: NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry,
        role: NativeVulkanSceneGpuBufferRole,
        buffer: vk::Buffer,
        bytes: u64,
        payload_hash: u64,
    ) -> NativeVulkanSceneGpuBufferBinding {
        NativeVulkanSceneGpuBufferBinding {
            key: NativeVulkanSceneGpuBufferKey {
                owner: NativeVulkanSceneGpuBufferOwner::LayerAlphaMaskRtMethod8MdlvEntry(geometry),
                role,
            },
            buffer,
            bytes,
            payload_hash,
        }
    }
}
