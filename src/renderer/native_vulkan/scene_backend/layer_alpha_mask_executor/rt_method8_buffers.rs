//! Retained buffer requirements for WE `[layer+0x490]` RT method [8] geometry.
//!
//! References:
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`

use std::collections::BTreeSet;

use serde::Serialize;

use crate::engine::scene_engine::SceneObjectId;
use crate::renderer::native_vulkan::scene_backend::resource_storage::{
    NativeVulkanSceneGpuBufferOwner, NativeVulkanSceneGpuBufferRole,
    NativeVulkanSceneGpuBufferUsage, NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry,
};

use super::rt_method8::{
    LAYER_490_RT_METHOD8_GEOMETRY_SOURCE, NativeVulkanSceneLayerAlphaMaskRtMethod8BridgePlan,
};
use super::rt_method8_payload::{
    LAYER_490_RT_METHOD8_AUX_PAYLOAD_REGION, LAYER_490_RT_METHOD8_PAYLOAD_REBUILD_VMA,
};

pub(in crate::renderer::native_vulkan) const LAYER_490_RT_METHOD8_DEFAULT_ENTRY_OWNER_INDEX: u32 =
    0;
pub(in crate::renderer::native_vulkan) const LAYER_490_RT_METHOD8_ENTRY_OWNER_SOURCE: &str =
    "[[layer+0x4b8]+0x18] first/current 0xc8 MDLV entry-owner";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvGeometryBufferRequirementPlan
{
    pub command_count: usize,
    pub bridge_count: usize,
    pub geometry_count: usize,
    pub vertex_requirement_count: usize,
    pub index_requirement_count: usize,
    pub entry_owner_source: &'static str,
    pub requirements: Vec<NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvGeometryBufferRequirement>,
    pub command_order: [&'static str; 7],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvGeometryBufferRequirement
{
    pub requirement_index: usize,
    pub object: SceneObjectId,
    pub entry_owner_index: u32,
    pub geometry: NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry,
    pub owner: NativeVulkanSceneGpuBufferOwner,
    pub vertex_role: NativeVulkanSceneGpuBufferRole,
    pub index_role: NativeVulkanSceneGpuBufferRole,
    pub vertex_usage: NativeVulkanSceneGpuBufferUsage,
    pub index_usage: NativeVulkanSceneGpuBufferUsage,
    pub geometry_source: &'static str,
    pub payload_rebuild_vma: &'static str,
    pub aux_payload_region: &'static str,
    pub reference_points: [&'static str; 4],
    pub command_order: [&'static str; 6],
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_alpha_mask_rt_method8_mdlv_geometry_buffers(
    bridges: &NativeVulkanSceneLayerAlphaMaskRtMethod8BridgePlan,
) -> Result<NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvGeometryBufferRequirementPlan, String> {
    let mut geometries = BTreeSet::new();
    for bridge in &bridges.bridges {
        if bridge.is_raw_shader_resource_bind {
            return Err(format!(
                "scene layer alpha-mask RT method [8] bridge {} tried to lower [layer+0x490] as a shader resource bind",
                bridge.bridge_index
            ));
        }
        if !bridge.is_indexed_vector_draw {
            return Err(format!(
                "scene layer alpha-mask RT method [8] bridge {} requires retained indexed MDLV geometry",
                bridge.bridge_index
            ));
        }
        geometries.insert(NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry {
            object: bridge.object,
            entry_owner_index: LAYER_490_RT_METHOD8_DEFAULT_ENTRY_OWNER_INDEX,
        });
    }

    let requirements = geometries
        .into_iter()
        .enumerate()
        .map(|(requirement_index, geometry)| {
            NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvGeometryBufferRequirement::from_geometry(
                requirement_index,
                geometry,
            )
        })
        .collect::<Vec<_>>();
    Ok(
        NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvGeometryBufferRequirementPlan {
            command_count: bridges.command_count,
            bridge_count: bridges.bridge_count,
            geometry_count: requirements.len(),
            vertex_requirement_count: requirements.len(),
            index_requirement_count: requirements.len(),
            entry_owner_source: LAYER_490_RT_METHOD8_ENTRY_OWNER_SOURCE,
            requirements,
            command_order: rt_method8_mdlv_geometry_buffer_command_order(),
        },
    )
}

impl NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvGeometryBufferRequirementPlan {
    pub(in crate::renderer::native_vulkan) fn requirement_for_object(
        &self,
        object: SceneObjectId,
    ) -> Option<(
        usize,
        &NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvGeometryBufferRequirement,
    )> {
        self.requirements
            .iter()
            .enumerate()
            .find(|(_, requirement)| requirement.object == object)
    }
}

impl NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvGeometryBufferRequirement {
    fn from_geometry(
        requirement_index: usize,
        geometry: NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry,
    ) -> Self {
        let vertex_role = NativeVulkanSceneGpuBufferRole::LayerAlphaMaskRtMethod8MdlvVertex;
        let index_role = NativeVulkanSceneGpuBufferRole::LayerAlphaMaskRtMethod8MdlvIndex;
        Self {
            requirement_index,
            object: geometry.object,
            entry_owner_index: geometry.entry_owner_index,
            geometry,
            owner: NativeVulkanSceneGpuBufferOwner::LayerAlphaMaskRtMethod8MdlvEntry(geometry),
            vertex_role,
            index_role,
            vertex_usage: vertex_role.usage(),
            index_usage: index_role.usage(),
            geometry_source: LAYER_490_RT_METHOD8_GEOMETRY_SOURCE,
            payload_rebuild_vma: LAYER_490_RT_METHOD8_PAYLOAD_REBUILD_VMA,
            aux_payload_region: LAYER_490_RT_METHOD8_AUX_PAYLOAD_REGION,
            reference_points: [
                "reverse-engineered/docs/exe/blend-and-render.md: 0x14020b15e uploads first/current MDLV entry-owner geometry to [layer+0x490]",
                "reverse-engineered/docs/exe/clipping-pipeline.md: aux+0x298 is later materialized into indexed slices on [layer+0x490]",
                "reverse-engineered/docs/exe/d3d11-context-calls.md: wrapper [8] creates indexed RT/draw-target vertex and index buffers",
                "references/godot/servers/rendering/rendering_device_graph.cpp: draw-list vertex/index buffers are explicit recorded resources before draw_indexed",
            ],
            command_order: [
                "read_closed_rt_method8_bridge",
                "select_first_current_mdlv_entry_owner",
                "dedupe_retained_geometry_by_scene_object",
                "assign_layer_alpha_mask_rt_method8_mdlv_owner",
                "require_retained_vertex_and_index_buffer_roles",
                "feed_recorder_without_mesh_geometry_owner",
            ],
        }
    }
}

fn rt_method8_mdlv_geometry_buffer_command_order() -> [&'static str; 7] {
    [
        "read_rt_method8_bridge_plan",
        "validate_indexed_draw_bridge_not_shader_resource_bind",
        "map_layer_0x4b8_plus_0x18_to_entry_owner_index_zero",
        "dedupe_mdlv_entry_geometry_requirements",
        "emit_layer_alpha_mask_rt_method8_mdlv_buffer_owner",
        "emit_vertex_and_index_buffer_roles",
        "feed_recorder_requirements",
    ]
}

#[cfg(test)]
#[path = "rt_method8_buffers_tests.rs"]
mod tests;
