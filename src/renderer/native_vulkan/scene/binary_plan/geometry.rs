use crate::core::scene::binary::{
    SCENE_BINARY_NONE_ID, SceneBinaryError, SceneBinaryGeometryRecord, SceneBinaryLayoutPlan,
};

use super::material::NativeVulkanSceneBinaryRecordRange;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneBinaryGeometryRecord {
    pub(in crate::renderer::native_vulkan::scene) owner_name: u32,
    pub(in crate::renderer::native_vulkan::scene) kind: u16,
    pub(in crate::renderer::native_vulkan::scene) flags: u16,
    pub(in crate::renderer::native_vulkan::scene) width: f32,
    pub(in crate::renderer::native_vulkan::scene) height: f32,
    pub(in crate::renderer::native_vulkan::scene) vertices: NativeVulkanSceneBinaryRecordRange,
    pub(in crate::renderer::native_vulkan::scene) indices: NativeVulkanSceneBinaryRecordRange,
    pub(in crate::renderer::native_vulkan::scene) material_uv_count: u32,
    pub(in crate::renderer::native_vulkan::scene) primitive_kind: u16,
    pub(in crate::renderer::native_vulkan::scene) vertex_layout: u16,
    pub(in crate::renderer::native_vulkan::scene) bounds_min_x: f32,
    pub(in crate::renderer::native_vulkan::scene) bounds_min_y: f32,
    pub(in crate::renderer::native_vulkan::scene) bounds_max_x: f32,
    pub(in crate::renderer::native_vulkan::scene) bounds_max_y: f32,
    pub(in crate::renderer::native_vulkan::scene) uv_min_u: f32,
    pub(in crate::renderer::native_vulkan::scene) uv_min_v: f32,
    pub(in crate::renderer::native_vulkan::scene) uv_max_u: f32,
    pub(in crate::renderer::native_vulkan::scene) uv_max_v: f32,
}

pub(super) struct NativeVulkanSceneBinaryGeometryRecords {
    pub(super) records: Vec<NativeVulkanSceneBinaryGeometryRecord>,
    pub(super) generated_vertex_count: u32,
    pub(super) generated_index_count: u32,
    pub(super) mesh_vertex_count: u32,
    pub(super) mesh_index_count: u32,
}

pub(super) fn native_vulkan_scene_binary_geometry_records(
    container: &[u8],
    layout: &SceneBinaryLayoutPlan,
) -> Result<NativeVulkanSceneBinaryGeometryRecords, SceneBinaryError> {
    let geometry_records = layout.geometry_records(container)?;
    let mut records = Vec::with_capacity(geometry_records.len());
    let mut generated_vertex_count = 0u32;
    let mut generated_index_count = 0u32;
    let mut mesh_vertex_count = 0u32;
    let mut mesh_index_count = 0u32;

    for geometry in geometry_records {
        let geometry = geometry?;
        let vertex_count = if geometry.first_vertex == SCENE_BINARY_NONE_ID {
            generated_vertex_count = generated_vertex_count.saturating_add(geometry.vertex_count);
            geometry.vertex_count
        } else {
            let stream_count = layout
                .geometry_vertex_record_range(container, geometry)?
                .len()
                .min(u32::MAX as usize) as u32;
            mesh_vertex_count = mesh_vertex_count.saturating_add(stream_count);
            stream_count
        };
        let index_count = if geometry.first_index == SCENE_BINARY_NONE_ID {
            generated_index_count = generated_index_count.saturating_add(geometry.index_count);
            geometry.index_count
        } else {
            let stream_count = layout
                .geometry_index_record_range(container, geometry)?
                .len()
                .min(u32::MAX as usize) as u32;
            mesh_index_count = mesh_index_count.saturating_add(stream_count);
            stream_count
        };
        records.push(native_vulkan_scene_binary_geometry_record(
            geometry,
            vertex_count,
            index_count,
        ));
    }

    Ok(NativeVulkanSceneBinaryGeometryRecords {
        records,
        generated_vertex_count,
        generated_index_count,
        mesh_vertex_count,
        mesh_index_count,
    })
}

fn native_vulkan_scene_binary_geometry_record(
    geometry: SceneBinaryGeometryRecord,
    vertex_count: u32,
    index_count: u32,
) -> NativeVulkanSceneBinaryGeometryRecord {
    NativeVulkanSceneBinaryGeometryRecord {
        owner_name: geometry.owner_name,
        kind: geometry.kind,
        flags: geometry.flags,
        width: geometry.width,
        height: geometry.height,
        vertices: NativeVulkanSceneBinaryRecordRange {
            first_record: geometry.first_vertex,
            record_count: vertex_count,
        },
        indices: NativeVulkanSceneBinaryRecordRange {
            first_record: geometry.first_index,
            record_count: index_count,
        },
        material_uv_count: geometry.material_uv_count,
        primitive_kind: geometry.primitive_kind,
        vertex_layout: geometry.vertex_layout,
        bounds_min_x: geometry.bounds_min_x,
        bounds_min_y: geometry.bounds_min_y,
        bounds_max_x: geometry.bounds_max_x,
        bounds_max_y: geometry.bounds_max_y,
        uv_min_u: geometry.uv_min_u,
        uv_min_v: geometry.uv_min_v,
        uv_max_u: geometry.uv_max_u,
        uv_max_v: geometry.uv_max_v,
    }
}
