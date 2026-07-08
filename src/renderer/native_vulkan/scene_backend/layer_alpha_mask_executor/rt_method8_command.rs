//! Runtime command-list contract for WE `[layer+0x490]` RT method [8] indexed draws.
//!
//! References:
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;

use crate::engine::scene_engine::{SceneGraphPipelineClass, SceneObjectId};
use crate::renderer::native_vulkan::scene_backend::resource_buffers::{
    NativeVulkanSceneGpuBufferRecordBinding,
    NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvGeometryBufferRecords,
};
use crate::renderer::native_vulkan::scene_backend::resource_storage::{
    NativeVulkanSceneGpuBufferOwner, NativeVulkanSceneGpuBufferRole,
    NativeVulkanSceneGpuBufferUsage, NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry,
    NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSlice,
};

use super::recorder_requirements::{
    NativeVulkanSceneLayerAlphaMaskRecorderRequirement,
    NativeVulkanSceneLayerAlphaMaskRecorderRequirementKind,
    NativeVulkanSceneLayerAlphaMaskRecorderRequirementPlan,
    NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvRecorderGeometryRequirement,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommandPlan
{
    pub requirement_count: usize,
    pub command_count: usize,
    pub producer_command_count: usize,
    pub generated_consumer_command_count: usize,
    pub geometry_bind_count: usize,
    pub slice_bind_count: usize,
    pub indexed_draw_count: usize,
    pub r16_index_draw_count: usize,
    pub commands: Vec<NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommand>,
    pub command_order: [&'static str; 7],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommand
{
    pub command_index: usize,
    pub object: SceneObjectId,
    pub kind: NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawKind,
    pub shader: &'static str,
    pub pipeline_class: SceneGraphPipelineClass,
    pub rt_method8_call_site: &'static str,
    pub rt_method8_method_vma: &'static str,
    pub geometry: NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry,
    pub vertex: NativeVulkanSceneGpuBufferRecordBinding,
    pub geometry_index: NativeVulkanSceneGpuBufferRecordBinding,
    pub slices: Vec<NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawSliceCommand>,
    pub geometry_bind_count: usize,
    pub slice_bind_count: usize,
    pub indexed_draw_count: usize,
    pub index_type: &'static str,
    pub draw_call: &'static str,
    pub receiver: &'static str,
    pub reference_points: [&'static str; 5],
    pub command_order: [&'static str; 8],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawSliceCommand
{
    pub requirement_index: usize,
    pub slice: NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSlice,
    pub helper_vma: &'static str,
    pub index: NativeVulkanSceneGpuBufferRecordBinding,
    pub index_count: u32,
    pub index_type: &'static str,
    pub draw_call: &'static str,
    pub command_order: [&'static str; 5],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawKind
{
    ClippingMaskImage4Producer,
    GeneratedClippingTargetConsumer,
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_alpha_mask_rt_method8_indexed_draw_commands<
    ResolveGeometry,
>(
    requirements: &NativeVulkanSceneLayerAlphaMaskRecorderRequirementPlan,
    mut resolve_geometry: ResolveGeometry,
) -> Result<NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommandPlan, String>
where
    ResolveGeometry:
        FnMut(
            NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry,
        )
            -> Result<NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvGeometryBufferRecords, String>,
{
    let mut commands = Vec::new();
    for requirement in &requirements.requirements {
        let Some(kind) = rt_method8_indexed_draw_kind(requirement.kind) else {
            continue;
        };
        let geometry_requirement = requirement.rt_method8_mdlv_geometry.ok_or_else(|| {
            format!(
                "scene layer alpha-mask RT method [8] command {} has no retained MDLV geometry requirement",
                requirement.command_index
            )
        })?;
        let geometry = NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry {
            object: geometry_requirement.object,
            entry_owner_index: geometry_requirement.entry_owner_index,
        };
        let geometry_records = resolve_geometry(geometry).map_err(|err| {
            format!(
                "{err}; scene layer alpha-mask RT method [8] command {} requires retained [layer+0x490] geometry buffers",
                requirement.command_index
            )
        })?;
        commands.push(
            NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommand::from_requirement_and_geometry(
                kind,
                requirement,
                geometry_requirement,
                geometry_records,
            )?,
        );
    }
    Ok(
        NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommandPlan::from_commands(
            requirements.requirement_count,
            commands,
        ),
    )
}

impl NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommandPlan {
    fn from_commands(
        requirement_count: usize,
        commands: Vec<NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommand>,
    ) -> Self {
        Self {
            requirement_count,
            command_count: commands.len(),
            producer_command_count: commands
                .iter()
                .filter(|command| {
                    command.kind
                        == NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawKind::ClippingMaskImage4Producer
                })
                .count(),
            generated_consumer_command_count: commands
                .iter()
                .filter(|command| {
                    command.kind
                        == NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawKind::GeneratedClippingTargetConsumer
                })
                .count(),
            geometry_bind_count: commands
                .iter()
                .map(|command| command.geometry_bind_count)
                .sum(),
            slice_bind_count: commands
                .iter()
                .map(|command| command.slice_bind_count)
                .sum(),
            indexed_draw_count: commands
                .iter()
                .map(|command| command.indexed_draw_count)
                .sum(),
            r16_index_draw_count: commands
                .iter()
                .map(|command| command.indexed_draw_count)
                .sum(),
            commands,
            command_order: [
                "read_alpha_mask_recorder_requirements",
                "filter_rt_method8_producer_and_generated_consumer_requirements",
                "resolve_retained_mdlv_vertex_and_index_buffer_records",
                "validate_layer_0x490_geometry_owner_role_usage",
                "validate_retained_r16_index_slice_records",
                "build_draw_indexed_command_plan",
                "defer_vulkan_recording_to_token_scheduler",
            ],
        }
    }
}

impl NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommand {
    fn from_requirement_and_geometry(
        kind: NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawKind,
        requirement: &NativeVulkanSceneLayerAlphaMaskRecorderRequirement,
        geometry_requirement: NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvRecorderGeometryRequirement,
        geometry_records: NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvGeometryBufferRecords,
    ) -> Result<Self, String> {
        validate_geometry_records(
            requirement.command_index,
            geometry_requirement,
            &geometry_records,
        )?;
        if requirement.rt_method8_mdlv_index_slices.is_empty() {
            return Err(format!(
                "scene layer alpha-mask RT method [8] command {} has no retained MDLV index slices",
                requirement.command_index
            ));
        }
        let shader = requirement.shader.ok_or_else(|| {
            format!(
                "scene layer alpha-mask RT method [8] command {} has no shader contract",
                requirement.command_index
            )
        })?;
        let pipeline_class = requirement.pipeline_class.ok_or_else(|| {
            format!(
                "scene layer alpha-mask RT method [8] command {} has no pipeline class contract",
                requirement.command_index
            )
        })?;
        let call_site = requirement.rt_method8_call_site.ok_or_else(|| {
            format!(
                "scene layer alpha-mask RT method [8] command {} has no call-site contract",
                requirement.command_index
            )
        })?;
        let method_vma = requirement.rt_method8_method_vma.ok_or_else(|| {
            format!(
                "scene layer alpha-mask RT method [8] command {} has no method VMA contract",
                requirement.command_index
            )
        })?;

        let mut slices = Vec::with_capacity(requirement.rt_method8_mdlv_index_slices.len());
        for slice in &requirement.rt_method8_mdlv_index_slices {
            slices.push(
                NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawSliceCommand::from_requirement(
                    requirement.command_index,
                    geometry_records.geometry,
                    slice,
                )?,
            );
        }
        Ok(Self {
            command_index: requirement.command_index,
            object: requirement.object,
            kind,
            shader,
            pipeline_class,
            rt_method8_call_site: call_site,
            rt_method8_method_vma: method_vma,
            geometry: geometry_records.geometry,
            vertex: geometry_records.vertex,
            geometry_index: geometry_records.index,
            geometry_bind_count: 1,
            slice_bind_count: slices.len(),
            indexed_draw_count: slices.len(),
            slices,
            index_type: "VK_INDEX_TYPE_UINT16",
            draw_call: "vkCmdDrawIndexed",
            receiver: "[layer+0x490].vtable+0x40",
            reference_points: [
                "reverse-engineered/docs/exe/blend-and-render.md: [layer+0x490] vtable+0x40 consumes indexed draw buffers",
                "reverse-engineered/docs/exe/clipping-pipeline.md: 0x14020d6a0 producer draw uses [layer+0x490].vtable+0x40",
                "reverse-engineered/docs/exe/clipping-pipeline.md: 0x140208bbb/0x14020908c generated draws route through [layer+0x490]",
                "reverse-engineered/docs/exe/d3d11-context-calls.md: wrapper [8] carries indexed draw buffer contract",
                "references/godot/servers/rendering/rendering_device_graph.cpp: draw-list records vertex/index resources before draw_indexed",
            ],
            command_order: [
                "bind_pipeline_for_rt_method8_requirement",
                "bind_resource_heap_for_we_texture_slots",
                "bind_layer_0x490_mdlv_vertex_buffer",
                "bind_layer_0x490_mdlv_geometry_index_buffer",
                "iterate_retained_subdraw_index_slices",
                "bind_r16_slice_index_buffer",
                "cmd_draw_indexed_for_slice",
                "preserve_token_scheduler_order",
            ],
        })
    }
}

impl NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawSliceCommand {
    fn from_requirement(
        command_index: usize,
        geometry: NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry,
        requirement: &super::rt_method8_slices::NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceRequirement,
    ) -> Result<Self, String> {
        if requirement.object != geometry.object
            || requirement.entry_owner_index != geometry.entry_owner_index
            || requirement.slice.object != geometry.object
            || requirement.slice.entry_owner_index != geometry.entry_owner_index
        {
            return Err(format!(
                "scene layer alpha-mask RT method [8] command {command_index} slice {} does not match retained geometry owner",
                requirement.requirement_index
            ));
        }
        if requirement.index_usage != NativeVulkanSceneGpuBufferUsage::Index {
            return Err(format!(
                "scene layer alpha-mask RT method [8] command {command_index} slice {} is not an index-buffer requirement",
                requirement.requirement_index
            ));
        }
        let expected_owner = NativeVulkanSceneGpuBufferOwner::LayerAlphaMaskRtMethod8MdlvIndexSlice(
            requirement.slice,
        );
        let expected_role = NativeVulkanSceneGpuBufferRole::LayerAlphaMaskRtMethod8MdlvSliceIndex;
        if requirement.owner != expected_owner
            || requirement.index_role != expected_role
            || requirement.index.key.owner != expected_owner
            || requirement.index.key.role != expected_role
        {
            return Err(format!(
                "scene layer alpha-mask RT method [8] command {command_index} slice {} has invalid retained index owner/role",
                requirement.requirement_index
            ));
        }
        if requirement.index.bytes == 0 || requirement.index.bytes % 2 != 0 {
            return Err(format!(
                "scene layer alpha-mask RT method [8] command {command_index} slice {} index bytes {} are not valid R16 indices",
                requirement.requirement_index, requirement.index.bytes
            ));
        }
        let index_count = u32::try_from(requirement.index.bytes / 2).map_err(|_| {
            format!(
                "scene layer alpha-mask RT method [8] command {command_index} slice {} R16 index count exceeds u32",
                requirement.requirement_index
            )
        })?;
        Ok(Self {
            requirement_index: requirement.requirement_index,
            slice: requirement.slice,
            helper_vma: requirement.helper_vma,
            index: requirement.index,
            index_count,
            index_type: "VK_INDEX_TYPE_UINT16",
            draw_call: "vkCmdDrawIndexed",
            command_order: [
                "use_retained_slice_index_buffer",
                "bind_slice_as_uint16_index_buffer",
                "derive_index_count_from_r16_bytes",
                "cmd_draw_indexed",
                "advance_to_next_slice_without_cpu_payload_rebuild",
            ],
        })
    }
}

fn rt_method8_indexed_draw_kind(
    kind: NativeVulkanSceneLayerAlphaMaskRecorderRequirementKind,
) -> Option<NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawKind> {
    match kind {
        NativeVulkanSceneLayerAlphaMaskRecorderRequirementKind::ClippingMaskImage4Producer => {
            Some(
                NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawKind::ClippingMaskImage4Producer,
            )
        }
        NativeVulkanSceneLayerAlphaMaskRecorderRequirementKind::GeneratedClippingTargetConsumer => {
            Some(
                NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawKind::GeneratedClippingTargetConsumer,
            )
        }
        NativeVulkanSceneLayerAlphaMaskRecorderRequirementKind::TokenProgramDispatch
        | NativeVulkanSceneLayerAlphaMaskRecorderRequirementKind::FlatTextureCopyBackGraphNode => {
            None
        }
    }
}

fn validate_geometry_records(
    command_index: usize,
    requirement: NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvRecorderGeometryRequirement,
    records: &NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvGeometryBufferRecords,
) -> Result<(), String> {
    if records.geometry.object != requirement.object
        || records.geometry.entry_owner_index != requirement.entry_owner_index
    {
        return Err(format!(
            "scene layer alpha-mask RT method [8] command {command_index} geometry records do not match recorder requirement"
        ));
    }
    if requirement.vertex_usage != NativeVulkanSceneGpuBufferUsage::Vertex
        || requirement.index_usage != NativeVulkanSceneGpuBufferUsage::Index
    {
        return Err(format!(
            "scene layer alpha-mask RT method [8] command {command_index} geometry requirement has invalid vertex/index usage"
        ));
    }
    if records.vertex.key.owner != requirement.owner
        || records.vertex.key.role != requirement.vertex_role
        || records.index.key.owner != requirement.owner
        || records.index.key.role != requirement.index_role
        || records.vertex.bytes == 0
        || records.index.bytes == 0
    {
        return Err(format!(
            "scene layer alpha-mask RT method [8] command {command_index} geometry buffers have invalid owner/role/bytes"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::recorder_requirements::NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvRecorderGeometryRequirement;
    use super::super::rt_method8_slices::{
        NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceRequirement,
        NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceRequirementPlan,
    };
    use super::super::token_schedule::NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus;
    use super::*;
    use crate::engine::scene_engine::{
        SceneGraphTarget, SceneLayerCompositorCondition, SceneLayerCompositorEntry,
        SceneLayerCompositorOperation, SceneLayerCompositorTarget,
    };
    use crate::renderer::native_vulkan::scene_backend::resource_buffers::NativeVulkanSceneGpuBufferKey;
    use crate::renderer::native_vulkan::scene_backend::resource_storage::NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceKind;

    #[test]
    fn indexed_draw_commands_bind_geometry_and_r16_slices() {
        let plan = native_vulkan_plan_scene_layer_alpha_mask_rt_method8_indexed_draw_commands(
            &requirements(vec![producer_requirement(1)]),
            |geometry| Ok(geometry_records(geometry, 80, 12)),
        )
        .expect("indexed draw commands");

        assert_eq!(plan.requirement_count, 1);
        assert_eq!(plan.command_count, 1);
        assert_eq!(plan.producer_command_count, 1);
        assert_eq!(plan.generated_consumer_command_count, 0);
        assert_eq!(plan.geometry_bind_count, 1);
        assert_eq!(plan.slice_bind_count, 2);
        assert_eq!(plan.indexed_draw_count, 2);
        assert_eq!(plan.r16_index_draw_count, 2);
        assert_eq!(
            plan.commands[0].kind,
            NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawKind::ClippingMaskImage4Producer
        );
        assert_eq!(plan.commands[0].draw_call, "vkCmdDrawIndexed");
        assert_eq!(plan.commands[0].index_type, "VK_INDEX_TYPE_UINT16");
        assert_eq!(plan.commands[0].slices[0].index_count, 3);
        assert_eq!(plan.commands[0].slices[1].helper_vma, "0x14020c710");
    }

    #[test]
    fn indexed_draw_commands_include_generated_consumer_kind() {
        let plan = native_vulkan_plan_scene_layer_alpha_mask_rt_method8_indexed_draw_commands(
            &requirements(vec![generated_requirement(4)]),
            |geometry| Ok(geometry_records(geometry, 80, 12)),
        )
        .expect("indexed draw commands");

        assert_eq!(plan.command_count, 1);
        assert_eq!(plan.producer_command_count, 0);
        assert_eq!(plan.generated_consumer_command_count, 1);
        assert_eq!(
            plan.commands[0].kind,
            NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawKind::GeneratedClippingTargetConsumer
        );
        assert_eq!(plan.commands[0].shader, "we/genericimage4");
    }

    #[test]
    fn indexed_draw_commands_reject_unaligned_slice_bytes() {
        let mut requirement = producer_requirement(1);
        requirement.rt_method8_mdlv_index_slices[0].index.bytes = 5;
        let err = native_vulkan_plan_scene_layer_alpha_mask_rt_method8_indexed_draw_commands(
            &requirements(vec![requirement]),
            |geometry| Ok(geometry_records(geometry, 80, 12)),
        )
        .expect_err("unaligned R16 slice must fail");

        assert!(err.contains("not valid R16 indices"));
    }

    #[test]
    fn indexed_draw_commands_reject_geometry_owner_drift() {
        let err = native_vulkan_plan_scene_layer_alpha_mask_rt_method8_indexed_draw_commands(
            &requirements(vec![producer_requirement(1)]),
            |geometry| {
                let mut records = geometry_records(geometry, 80, 12);
                records.vertex.key.role =
                    NativeVulkanSceneGpuBufferRole::LayerAlphaMaskRtMethod8MdlvIndex;
                Ok(records)
            },
        )
        .expect_err("geometry role drift must fail");

        assert!(err.contains("geometry buffers have invalid owner/role/bytes"));
    }

    fn requirements(
        requirements: Vec<NativeVulkanSceneLayerAlphaMaskRecorderRequirement>,
    ) -> NativeVulkanSceneLayerAlphaMaskRecorderRequirementPlan {
        NativeVulkanSceneLayerAlphaMaskRecorderRequirementPlan {
            step_count: requirements.len(),
            requirement_count: requirements.len(),
            token_program_requirement_count: 0,
            clippingmaskimage4_producer_requirement_count: requirements
                .iter()
                .filter(|requirement| {
                    requirement.kind
                        == NativeVulkanSceneLayerAlphaMaskRecorderRequirementKind::ClippingMaskImage4Producer
                })
                .count(),
            flattexture_copy_back_ready_requirement_count: 0,
            generated_clippingtarget_consumer_requirement_count: requirements
                .iter()
                .filter(|requirement| {
                    requirement.kind
                        == NativeVulkanSceneLayerAlphaMaskRecorderRequirementKind::GeneratedClippingTargetConsumer
                })
                .count(),
            pending_recorder_requirement_count: requirements.len(),
            ready_graph_node_requirement_count: 0,
            no_draw_requirement_count: 0,
            missing_we_fact_count: 0,
            requirements,
            command_order: [
                "read_token_schedule",
                "join_token_heap_binds",
                "join_draw_pipelines_targets_uniforms",
                "join_rt_method8_bridge_and_retained_geometry",
                "emit_alpha_mask_recorder_requirements",
                "leave_incomplete_recorders_pending",
            ],
        }
    }

    fn producer_requirement(
        command_index: usize,
    ) -> NativeVulkanSceneLayerAlphaMaskRecorderRequirement {
        draw_requirement(
            command_index,
            NativeVulkanSceneLayerAlphaMaskRecorderRequirementKind::ClippingMaskImage4Producer,
            "we/clippingmaskimage4",
            SceneGraphTarget::FullAlphaMask,
            "0x14020d83e",
        )
    }

    fn generated_requirement(
        command_index: usize,
    ) -> NativeVulkanSceneLayerAlphaMaskRecorderRequirement {
        draw_requirement(
            command_index,
            NativeVulkanSceneLayerAlphaMaskRecorderRequirementKind::GeneratedClippingTargetConsumer,
            "we/genericimage4",
            SceneGraphTarget::ObjectFinal(SceneObjectId(7)),
            "0x14020908c",
        )
    }

    fn draw_requirement(
        command_index: usize,
        kind: NativeVulkanSceneLayerAlphaMaskRecorderRequirementKind,
        shader: &'static str,
        target_mask: SceneGraphTarget,
        call_site: &'static str,
    ) -> NativeVulkanSceneLayerAlphaMaskRecorderRequirement {
        let object = SceneObjectId(7);
        NativeVulkanSceneLayerAlphaMaskRecorderRequirement {
            command_index,
            object,
            entry: match kind {
                NativeVulkanSceneLayerAlphaMaskRecorderRequirementKind::GeneratedClippingTargetConsumer => {
                    SceneLayerCompositorEntry::TokenizedCompositeWithMaterialEntry53
                }
                _ => SceneLayerCompositorEntry::AlphaMaskHelper20d6a0,
            },
            operation: match kind {
                NativeVulkanSceneLayerAlphaMaskRecorderRequirementKind::GeneratedClippingTargetConsumer => {
                    SceneLayerCompositorOperation::DrawGeneratedClippingTarget
                }
                _ => SceneLayerCompositorOperation::DrawClippingMask,
            },
            condition: match kind {
                NativeVulkanSceneLayerAlphaMaskRecorderRequirementKind::GeneratedClippingTargetConsumer => {
                    SceneLayerCompositorCondition::TokenizedGeneratedMaterial
                }
                _ => SceneLayerCompositorCondition::Token1OrToken2FirstPair,
            },
            source: None,
            target: SceneLayerCompositorTarget::LayerTarget490,
            source_graph_target: None,
            target_graph_target: Some(target_mask),
            kind,
            recording_status: match kind {
                NativeVulkanSceneLayerAlphaMaskRecorderRequirementKind::GeneratedClippingTargetConsumer => {
                    NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus::PendingGeneratedClippingTargetRecorder
                }
                _ => NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus::PendingClippingMaskImage4ProducerRecorder,
            },
            shader: Some(shader),
            pipeline_class: Some(SceneGraphPipelineClass::PuppetSkinning),
            target_format: Some("R8_UNORM"),
            texture_slot_mask: 0x3,
            heap_bind_count: 1,
            heap_bind_indices: vec![0],
            producer_draw_index: (kind
                == NativeVulkanSceneLayerAlphaMaskRecorderRequirementKind::ClippingMaskImage4Producer)
                .then_some(0),
            producer_target_scope_index: None,
            producer_uniform_index: None,
            generated_consumer_draw_index: (kind
                == NativeVulkanSceneLayerAlphaMaskRecorderRequirementKind::GeneratedClippingTargetConsumer)
                .then_some(0),
            generated_consumer_uniform_index: None,
            rt_method8_bridge_index: Some(0),
            rt_method8_call_site: Some(call_site),
            rt_method8_method_vma: Some("0x1400eacd0"),
            rt_method8_mdlv_geometry: Some(geometry_requirement(object)),
            rt_method8_mdlv_index_slices: slice_plan(object).requirements,
            target_scope_load_op: None,
            requires_initialized_initial_layout: None,
            source_mask: None,
            target_mask: Some(target_mask),
            missing_we_facts: Vec::new(),
            reference_points: Vec::new(),
            command_order: Vec::new(),
        }
    }

    fn geometry_requirement(
        object: SceneObjectId,
    ) -> NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvRecorderGeometryRequirement {
        let geometry = NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry {
            object,
            entry_owner_index: 0,
        };
        NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvRecorderGeometryRequirement {
            buffer_requirement_index: 0,
            object,
            entry_owner_index: 0,
            owner: NativeVulkanSceneGpuBufferOwner::LayerAlphaMaskRtMethod8MdlvEntry(geometry),
            vertex_role: NativeVulkanSceneGpuBufferRole::LayerAlphaMaskRtMethod8MdlvVertex,
            index_role: NativeVulkanSceneGpuBufferRole::LayerAlphaMaskRtMethod8MdlvIndex,
            vertex_usage: NativeVulkanSceneGpuBufferUsage::Vertex,
            index_usage: NativeVulkanSceneGpuBufferUsage::Index,
            geometry_source: "0x14020b15e",
            payload_rebuild_vma: "0x14020ae00",
            aux_payload_region: "aux+0x298",
        }
    }

    fn geometry_records(
        geometry: NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry,
        vertex_bytes: u64,
        index_bytes: u64,
    ) -> NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvGeometryBufferRecords {
        let owner = NativeVulkanSceneGpuBufferOwner::LayerAlphaMaskRtMethod8MdlvEntry(geometry);
        NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvGeometryBufferRecords {
            geometry,
            vertex: NativeVulkanSceneGpuBufferRecordBinding {
                key: NativeVulkanSceneGpuBufferKey {
                    owner,
                    role: NativeVulkanSceneGpuBufferRole::LayerAlphaMaskRtMethod8MdlvVertex,
                },
                bytes: vertex_bytes,
                payload_hash: 0x1111,
            },
            index: NativeVulkanSceneGpuBufferRecordBinding {
                key: NativeVulkanSceneGpuBufferKey {
                    owner,
                    role: NativeVulkanSceneGpuBufferRole::LayerAlphaMaskRtMethod8MdlvIndex,
                },
                bytes: index_bytes,
                payload_hash: 0x2222,
            },
        }
    }

    fn slice_plan(
        object: SceneObjectId,
    ) -> NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceRequirementPlan {
        let first_slice = NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSlice {
            object,
            entry_owner_index: 0,
            subdraw_index: 0,
            kind: NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceKind::FirstListAppendToken0,
        };
        let second_slice = NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSlice {
            object,
            entry_owner_index: 0,
            subdraw_index: 1,
            kind: NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceKind::SecondListNoToken,
        };
        NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceRequirementPlan {
            geometry_count: 1,
            slice_requirement_count: 2,
            requirements: vec![
                slice_requirement(0, first_slice, 6, "0x14020c850"),
                slice_requirement(1, second_slice, 4, "0x14020c710"),
            ],
            command_order: [
                "read_rt_method8_geometry_buffer_requirements",
                "resolve_retained_mdlv_index_slice_buffers_from_gpu_store",
                "validate_slice_owner_role_and_index_usage",
                "carry_slice_records_to_alpha_mask_recorder_requirements",
                "forbid_recorder_side_cpu_slice_materialization",
            ],
        }
    }

    fn slice_requirement(
        requirement_index: usize,
        slice: NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSlice,
        bytes: u64,
        helper_vma: &'static str,
    ) -> NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceRequirement {
        let owner = NativeVulkanSceneGpuBufferOwner::LayerAlphaMaskRtMethod8MdlvIndexSlice(slice);
        let role = NativeVulkanSceneGpuBufferRole::LayerAlphaMaskRtMethod8MdlvSliceIndex;
        NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceRequirement {
            requirement_index,
            object: slice.object,
            entry_owner_index: slice.entry_owner_index,
            slice,
            owner,
            index_role: role,
            index_usage: NativeVulkanSceneGpuBufferUsage::Index,
            index: NativeVulkanSceneGpuBufferRecordBinding {
                key: NativeVulkanSceneGpuBufferKey { owner, role },
                bytes,
                payload_hash: 0x3333 + requirement_index as u64,
            },
            helper_vma,
            reference_points: [
                "reverse-engineered/docs/exe/clipping-pipeline.md: 0x14020c710 materializes no-token R16 index slices",
                "reverse-engineered/docs/exe/clipping-pipeline.md: 0x14020c850 materializes R16 index slices and appends token 0",
                "reverse-engineered/docs/exe/blend-and-render.md: [layer+0x490] vtable+0x40 consumes indexed draw buffers",
                "references/godot/servers/rendering/rendering_device_graph.cpp: draw-list indexed resources are recorded before draw execution",
            ],
            command_order: [
                "select_mdlv_subdraw_slice_owner",
                "validate_retained_slice_index_buffer_record",
                "map_slice_kind_to_recovered_helper_vma",
                "attach_slice_to_rt_method8_recorder_requirement",
                "record_indexed_draw_without_cpu_materializing_slice_payload",
            ],
        }
    }
}
