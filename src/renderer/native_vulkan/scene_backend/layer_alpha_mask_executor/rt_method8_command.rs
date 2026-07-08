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
use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_resource_heap::NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo;
use crate::renderer::native_vulkan::scene_backend::resource_buffers::{
    NativeVulkanSceneGpuBufferBinding, NativeVulkanSceneGpuBufferRecordBinding,
    NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvGeometryBufferRecords,
    NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvGeometryBuffers,
    NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceBuffers,
};
use crate::renderer::native_vulkan::scene_backend::resource_storage::{
    NativeVulkanSceneGpuBufferOwner, NativeVulkanSceneGpuBufferRole,
    NativeVulkanSceneGpuBufferUsage, NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry,
    NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSlice,
};
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, ExtDescriptorHeapExtensionDeviceCommands};

use super::NativeVulkanSceneLayerAlphaMaskTextureBindRole;
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
    pub heap_bind_index: usize,
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
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskRtMethod8RecordedDrawCommandPlan
{
    pub command_index: usize,
    pub object: SceneObjectId,
    pub kind: NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawKind,
    pub heap_bind_index: usize,
    pub heap_slice_index: usize,
    pub pipeline_bind_count: usize,
    pub resource_heap_bind_count: usize,
    pub vertex_buffer_bind_count: usize,
    pub slice_index_buffer_bind_count: usize,
    pub indexed_draw_count: usize,
    pub r16_index_count: u32,
    pub draw_call: &'static str,
    pub command_order: [&'static str; 6],
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
        let heap_bind_index = sole_heap_bind_index(requirement)?;

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
            heap_bind_index,
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

impl NativeVulkanSceneLayerAlphaMaskRtMethod8RecordedDrawCommandPlan {
    fn from_command_and_buffers(
        command: &NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommand,
        bind_info: &NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo,
        geometry: NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvGeometryBuffers,
        slices: &[NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceBuffers],
    ) -> Result<Self, String> {
        validate_heap_bind_for_recording(command, bind_info)?;
        validate_geometry_buffers_for_recording(command, geometry)?;
        if slices.len() != command.slices.len() {
            return Err(format!(
                "scene layer alpha-mask RT method [8] command {} expected {} retained slice buffers, got {}",
                command.command_index,
                command.slices.len(),
                slices.len()
            ));
        }
        let mut r16_index_count = 0u32;
        for (slice_command, slice_buffer) in command.slices.iter().zip(slices.iter().copied()) {
            validate_slice_buffer_for_recording(
                command.command_index,
                slice_command,
                slice_buffer,
            )?;
            r16_index_count = r16_index_count
                .checked_add(slice_command.index_count)
                .ok_or_else(|| {
                    format!(
                        "scene layer alpha-mask RT method [8] command {} total R16 index count overflowed",
                        command.command_index
                    )
                })?;
        }
        Ok(Self {
            command_index: command.command_index,
            object: command.object,
            kind: command.kind,
            heap_bind_index: bind_info.heap_bind_index,
            heap_slice_index: bind_info.heap_slice_index,
            pipeline_bind_count: 1,
            resource_heap_bind_count: 1,
            vertex_buffer_bind_count: 1,
            slice_index_buffer_bind_count: slices.len(),
            indexed_draw_count: slices.len(),
            r16_index_count,
            draw_call: "vkCmdDrawIndexed",
            command_order: [
                "cmd_bind_rt_method8_pipeline",
                "cmd_bind_alpha_mask_resource_heap_ext",
                "cmd_bind_alpha_mask_sampler_heap_ext",
                "cmd_bind_layer_0x490_vertex_buffer",
                "cmd_bind_each_r16_slice_index_buffer",
                "cmd_draw_indexed_each_slice",
            ],
        })
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_layer_alpha_mask_rt_method8_indexed_draw_command(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    command: &NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommand,
    vk_pipeline: vk::Pipeline,
    bind_info: &NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo,
    geometry: NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvGeometryBuffers,
    slices: &[NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceBuffers],
) -> Result<NativeVulkanSceneLayerAlphaMaskRtMethod8RecordedDrawCommandPlan, String> {
    if command_buffer == vk::CommandBuffer::null() {
        return Err(format!(
            "scene layer alpha-mask RT method [8] command {} requires a valid command buffer",
            command.command_index
        ));
    }
    if vk_pipeline == vk::Pipeline::null() {
        return Err(format!(
            "scene layer alpha-mask RT method [8] command {} requires a warmed vk::Pipeline",
            command.command_index
        ));
    }
    let plan =
        NativeVulkanSceneLayerAlphaMaskRtMethod8RecordedDrawCommandPlan::from_command_and_buffers(
            command, bind_info, geometry, slices,
        )?;
    unsafe {
        device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::GRAPHICS, vk_pipeline);
        device.cmd_bind_resource_heap_ext(command_buffer, &bind_info.resource_bind);
        device.cmd_bind_sampler_heap_ext(command_buffer, &bind_info.sampler_bind);
        let vertex_buffers = [geometry.vertex.buffer];
        let vertex_offsets = [0u64];
        device.cmd_bind_vertex_buffers(command_buffer, 0, &vertex_buffers, &vertex_offsets);
        for (slice_command, slice_buffer) in command.slices.iter().zip(slices.iter().copied()) {
            device.cmd_bind_index_buffer(
                command_buffer,
                slice_buffer.index.buffer,
                0,
                vk::IndexType::UINT16,
            );
            device.cmd_draw_indexed(command_buffer, slice_command.index_count, 1, 0, 0, 0);
        }
    }
    Ok(plan)
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

fn sole_heap_bind_index(
    requirement: &NativeVulkanSceneLayerAlphaMaskRecorderRequirement,
) -> Result<usize, String> {
    if requirement.heap_bind_count != 1 || requirement.heap_bind_indices.len() != 1 {
        return Err(format!(
            "scene layer alpha-mask RT method [8] command {} requires exactly one heap bind, got count={} indices={:?}",
            requirement.command_index, requirement.heap_bind_count, requirement.heap_bind_indices
        ));
    }
    Ok(requirement.heap_bind_indices[0])
}

fn validate_heap_bind_for_recording(
    command: &NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommand,
    bind_info: &NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo,
) -> Result<(), String> {
    if bind_info.heap_bind_index != command.heap_bind_index {
        return Err(format!(
            "scene layer alpha-mask RT method [8] command {} heap-bind mismatch: command {}, heap {}",
            command.command_index, command.heap_bind_index, bind_info.heap_bind_index
        ));
    }
    if bind_info.object != command.object {
        return Err(format!(
            "scene layer alpha-mask RT method [8] command {} object mismatch: command {:?}, heap {:?}",
            command.command_index, command.object, bind_info.object
        ));
    }
    if bind_info.shader != command.shader {
        return Err(format!(
            "scene layer alpha-mask RT method [8] command {} shader mismatch: command {}, heap {}",
            command.command_index, command.shader, bind_info.shader
        ));
    }
    let valid_role = match command.kind {
        NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawKind::ClippingMaskImage4Producer => {
            matches!(
                bind_info.role,
                NativeVulkanSceneLayerAlphaMaskTextureBindRole::ClippingMaskImage4 { .. }
            )
        }
        NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawKind::GeneratedClippingTargetConsumer => {
            bind_info.role == NativeVulkanSceneLayerAlphaMaskTextureBindRole::GeneratedClippingTarget
        }
    };
    if !valid_role {
        return Err(format!(
            "scene layer alpha-mask RT method [8] command {} heap role {:?} does not match {:?}",
            command.command_index, bind_info.role, command.kind
        ));
    }
    Ok(())
}

fn validate_geometry_buffers_for_recording(
    command: &NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommand,
    geometry: NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvGeometryBuffers,
) -> Result<(), String> {
    if geometry.geometry != command.geometry {
        return Err(format!(
            "scene layer alpha-mask RT method [8] command {} geometry buffer owner mismatch",
            command.command_index
        ));
    }
    validate_actual_buffer(
        command.command_index,
        "vertex",
        geometry.vertex,
        command.vertex,
    )?;
    validate_actual_buffer(
        command.command_index,
        "geometry index",
        geometry.index,
        command.geometry_index,
    )?;
    Ok(())
}

fn validate_slice_buffer_for_recording(
    command_index: usize,
    slice_command: &NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawSliceCommand,
    slice_buffer: NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceBuffers,
) -> Result<(), String> {
    if slice_buffer.slice != slice_command.slice {
        return Err(format!(
            "scene layer alpha-mask RT method [8] command {command_index} slice buffer owner mismatch"
        ));
    }
    validate_actual_buffer(
        command_index,
        "slice index",
        slice_buffer.index,
        slice_command.index,
    )
}

fn validate_actual_buffer(
    command_index: usize,
    label: &'static str,
    actual: NativeVulkanSceneGpuBufferBinding,
    expected: NativeVulkanSceneGpuBufferRecordBinding,
) -> Result<(), String> {
    if actual.buffer == vk::Buffer::null()
        || actual.key != expected.key
        || actual.bytes != expected.bytes
        || actual.payload_hash != expected.payload_hash
    {
        return Err(format!(
            "scene layer alpha-mask RT method [8] command {command_index} {label} buffer does not match retained record"
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
        SceneLayerCompositorOperation, SceneLayerCompositorTarget, ScenePuppetId, SceneResourceId,
    };
    use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_resource_heap::{
        NativeVulkanSceneLayerAlphaMaskHeapSliceBinding,
        NativeVulkanSceneLayerAlphaMaskHeapSliceKey,
    };
    use crate::renderer::native_vulkan::scene_backend::resource_buffers::NativeVulkanSceneGpuBufferKey;
    use crate::renderer::native_vulkan::scene_backend::resource_storage::NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceKind;
    use vulkanalia::vk::Handle;

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

    #[test]
    fn recorded_draw_command_plan_binds_heap_geometry_and_r16_slices() {
        let command = indexed_draw_command(producer_requirement(1));
        let geometry = geometry_buffers(&command);
        let slices = slice_buffers(&command);
        let bind_info = heap_bind_info(
            &command,
            NativeVulkanSceneLayerAlphaMaskTextureBindRole::ClippingMaskImage4 {
                clipping_record_index: 0,
            },
        );

        let plan =
            NativeVulkanSceneLayerAlphaMaskRtMethod8RecordedDrawCommandPlan::from_command_and_buffers(
                &command,
                &bind_info,
                geometry,
                &slices,
            )
            .expect("recorded draw command plan");

        assert_eq!(plan.command_index, 1);
        assert_eq!(plan.heap_bind_index, command.heap_bind_index);
        assert_eq!(plan.heap_slice_index, 8);
        assert_eq!(plan.pipeline_bind_count, 1);
        assert_eq!(plan.resource_heap_bind_count, 1);
        assert_eq!(plan.vertex_buffer_bind_count, 1);
        assert_eq!(plan.slice_index_buffer_bind_count, 2);
        assert_eq!(plan.indexed_draw_count, 2);
        assert_eq!(plan.r16_index_count, 5);
        assert_eq!(plan.draw_call, "vkCmdDrawIndexed");
    }

    #[test]
    fn recorded_draw_command_plan_rejects_wrong_heap_role() {
        let command = indexed_draw_command(producer_requirement(1));
        let geometry = geometry_buffers(&command);
        let slices = slice_buffers(&command);
        let bind_info = heap_bind_info(
            &command,
            NativeVulkanSceneLayerAlphaMaskTextureBindRole::GeneratedClippingTarget,
        );

        let err =
            NativeVulkanSceneLayerAlphaMaskRtMethod8RecordedDrawCommandPlan::from_command_and_buffers(
                &command,
                &bind_info,
                geometry,
                &slices,
            )
            .expect_err("wrong heap role must fail");

        assert!(err.contains("heap role"));
    }

    #[test]
    fn recorded_draw_command_plan_rejects_slice_buffer_record_drift() {
        let command = indexed_draw_command(producer_requirement(1));
        let geometry = geometry_buffers(&command);
        let mut slices = slice_buffers(&command);
        slices[0].index.payload_hash ^= 1;
        let bind_info = heap_bind_info(
            &command,
            NativeVulkanSceneLayerAlphaMaskTextureBindRole::ClippingMaskImage4 {
                clipping_record_index: 0,
            },
        );

        let err =
            NativeVulkanSceneLayerAlphaMaskRtMethod8RecordedDrawCommandPlan::from_command_and_buffers(
                &command,
                &bind_info,
                geometry,
                &slices,
            )
            .expect_err("slice binding drift must fail");

        assert!(err.contains("slice index buffer does not match"));
    }

    fn indexed_draw_command(
        requirement: NativeVulkanSceneLayerAlphaMaskRecorderRequirement,
    ) -> NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommand {
        let plan = native_vulkan_plan_scene_layer_alpha_mask_rt_method8_indexed_draw_commands(
            &requirements(vec![requirement]),
            |geometry| Ok(geometry_records(geometry, 80, 12)),
        )
        .expect("indexed draw command plan");
        plan.commands[0].clone()
    }

    fn geometry_buffers(
        command: &NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommand,
    ) -> NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvGeometryBuffers {
        NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvGeometryBuffers {
            geometry: command.geometry,
            vertex: actual_buffer(command.vertex, 0x1100),
            index: actual_buffer(command.geometry_index, 0x1200),
        }
    }

    fn slice_buffers(
        command: &NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommand,
    ) -> Vec<NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceBuffers> {
        command
            .slices
            .iter()
            .enumerate()
            .map(
                |(index, slice)| NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceBuffers {
                    slice: slice.slice,
                    index: actual_buffer(slice.index, 0x2100 + index as u64),
                },
            )
            .collect()
    }

    fn actual_buffer(
        record: NativeVulkanSceneGpuBufferRecordBinding,
        raw: u64,
    ) -> NativeVulkanSceneGpuBufferBinding {
        NativeVulkanSceneGpuBufferBinding {
            key: record.key,
            buffer: vk::Buffer::from_raw(raw),
            bytes: record.bytes,
            payload_hash: record.payload_hash,
        }
    }

    fn heap_bind_info(
        command: &NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommand,
        role: NativeVulkanSceneLayerAlphaMaskTextureBindRole,
    ) -> NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo {
        NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo {
            heap_bind_index: command.heap_bind_index,
            object: command.object,
            puppet: ScenePuppetId(5),
            shader: command.shader.to_owned(),
            role,
            heap_slice_index: 8,
            heap_slice: NativeVulkanSceneLayerAlphaMaskHeapSliceKey {
                shader: command.shader.to_owned(),
                bindings: vec![
                    NativeVulkanSceneLayerAlphaMaskHeapSliceBinding {
                        slot: 0,
                        source: super::super::NativeVulkanSceneLayerAlphaMaskDescriptorSource::ResidentTexture(
                            SceneResourceId(9),
                        ),
                    },
                    NativeVulkanSceneLayerAlphaMaskHeapSliceBinding {
                        slot: 1,
                        source: super::super::NativeVulkanSceneLayerAlphaMaskDescriptorSource::GraphTarget(
                            SceneGraphTarget::FullAlphaMask,
                        ),
                    },
                ],
            },
            material: None,
            base_resource_descriptor_index: 16,
            base_sampler_descriptor_index: 32,
            resource_descriptor_count: 2,
            texture_count: 2,
            shader_mappings: vec![
                "we.texture_slot0.g_Texture0 -> alpha-mask-heap-slice-offset0".to_owned(),
                "we.texture_slot1.g_Texture1 -> alpha-mask-heap-slice-offset1".to_owned(),
            ],
            resource_bind: vk::BindHeapInfoEXT::builder().build(),
            sampler_bind: vk::BindHeapInfoEXT::builder().build(),
        }
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
