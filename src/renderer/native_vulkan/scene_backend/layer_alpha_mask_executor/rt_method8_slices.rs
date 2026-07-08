//! Retained index-slice buffer requirements for WE `[layer+0x490]` RT method [8] draws.
//!
//! References:
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`

use serde::Serialize;

use crate::engine::scene_engine::SceneObjectId;
use crate::renderer::native_vulkan::scene_backend::resource_buffers::{
    NativeVulkanSceneGpuBufferRecordBinding,
    NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceBufferRecords,
};
use crate::renderer::native_vulkan::scene_backend::resource_storage::{
    NativeVulkanSceneGpuBufferOwner, NativeVulkanSceneGpuBufferRole,
    NativeVulkanSceneGpuBufferUsage, NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry,
    NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSlice,
};

use super::rt_method8_buffers::NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvGeometryBufferRequirementPlan;
use super::rt_method8_payload::{
    LAYER_490_RT_METHOD8_SLICE_HELPER_APPEND_TOKEN0_VMA,
    LAYER_490_RT_METHOD8_SLICE_HELPER_NO_TOKEN_VMA,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceRequirementPlan
{
    pub geometry_count: usize,
    pub slice_requirement_count: usize,
    pub requirements: Vec<NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceRequirement>,
    pub command_order: [&'static str; 5],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceRequirement
{
    pub requirement_index: usize,
    pub object: SceneObjectId,
    pub entry_owner_index: u32,
    pub slice: NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSlice,
    pub owner: NativeVulkanSceneGpuBufferOwner,
    pub index_role: NativeVulkanSceneGpuBufferRole,
    pub index_usage: NativeVulkanSceneGpuBufferUsage,
    pub index: NativeVulkanSceneGpuBufferRecordBinding,
    pub helper_vma: &'static str,
    pub reference_points: [&'static str; 4],
    pub command_order: [&'static str; 5],
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_alpha_mask_rt_method8_mdlv_index_slices<
    ResolveSlices,
>(
    geometry_buffers: &NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvGeometryBufferRequirementPlan,
    mut resolve_slices: ResolveSlices,
) -> Result<NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceRequirementPlan, String>
where
    ResolveSlices:
        FnMut(
            NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry,
        ) -> Vec<NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceBufferRecords>,
{
    let mut requirements = Vec::new();
    for geometry_requirement in &geometry_buffers.requirements {
        let records = resolve_slices(geometry_requirement.geometry);
        if records.is_empty() {
            return Err(format!(
                "scene layer alpha-mask object {:?} requires retained RT method [8] indexed-slice buffers for [layer+0x490]",
                geometry_requirement.object
            ));
        }
        for record in records {
            requirements.push(
                NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceRequirement::from_record(
                    requirements.len(),
                    record,
                )?,
            );
        }
    }

    Ok(
        NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceRequirementPlan {
            geometry_count: geometry_buffers.geometry_count,
            slice_requirement_count: requirements.len(),
            requirements,
            command_order: [
                "read_rt_method8_geometry_buffer_requirements",
                "resolve_retained_mdlv_index_slice_buffers_from_gpu_store",
                "validate_slice_owner_role_and_index_usage",
                "carry_slice_records_to_alpha_mask_recorder_requirements",
                "forbid_recorder_side_cpu_slice_materialization",
            ],
        },
    )
}

impl NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceRequirementPlan {
    pub(in crate::renderer::native_vulkan) fn requirements_for_object(
        &self,
        object: SceneObjectId,
    ) -> Vec<NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceRequirement> {
        self.requirements
            .iter()
            .filter(|requirement| requirement.object == object)
            .cloned()
            .collect()
    }
}

impl NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceRequirement {
    fn from_record(
        requirement_index: usize,
        record: NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceBufferRecords,
    ) -> Result<Self, String> {
        let owner =
            NativeVulkanSceneGpuBufferOwner::LayerAlphaMaskRtMethod8MdlvIndexSlice(record.slice);
        let index_role = NativeVulkanSceneGpuBufferRole::LayerAlphaMaskRtMethod8MdlvSliceIndex;
        if record.index.key.owner != owner
            || record.index.key.role != index_role
            || index_role.usage() != NativeVulkanSceneGpuBufferUsage::Index
            || record.index.bytes == 0
        {
            return Err(format!(
                "scene layer alpha-mask RT method [8] slice record {requirement_index} has invalid owner/role/usage"
            ));
        }
        Ok(Self {
            requirement_index,
            object: record.slice.object,
            entry_owner_index: record.slice.entry_owner_index,
            slice: record.slice,
            owner,
            index_role,
            index_usage: index_role.usage(),
            index: record.index,
            helper_vma: match record.slice.kind {
                crate::renderer::native_vulkan::scene_backend::resource_storage::NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceKind::FirstListAppendToken0 => {
                    LAYER_490_RT_METHOD8_SLICE_HELPER_APPEND_TOKEN0_VMA
                }
                crate::renderer::native_vulkan::scene_backend::resource_storage::NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceKind::SecondListNoToken => {
                    LAYER_490_RT_METHOD8_SLICE_HELPER_NO_TOKEN_VMA
                }
            },
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
        })
    }
}
