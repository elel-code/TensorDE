//! Scene mesh draw command planning and Vulkan command recording.
//!
//! References:
//! - `reverse-engineered/docs/mdl-format.md`
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/renderer_scene_render.h`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene_engine::{
    SCENE_GPU_MESH_INDEX_BYTES, SCENE_GPU_MESH_VERTEX_BYTES, SceneGeometryId, SceneGraphDraw,
    SceneGraphPipelineClass,
};

use super::resource_buffers::{
    NativeVulkanSceneGpuBufferBinding, NativeVulkanSceneGpuBufferKey,
    NativeVulkanSceneMeshDrawBufferRecords, NativeVulkanSceneMeshDrawBuffers,
};
use super::resource_storage::{NativeVulkanSceneGpuBufferOwner, NativeVulkanSceneGpuBufferRole};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneMeshDrawCommandPlan {
    pub geometry: SceneGeometryId,
    pub index_count: u32,
    pub vertex_bytes: u64,
    pub index_bytes: u64,
    pub vertex_payload_hash: u64,
    pub index_payload_hash: u64,
    pub command_order: [&'static str; 3],
}

impl NativeVulkanSceneMeshDrawCommandPlan {
    pub fn from_record_bindings(
        draw: &SceneGraphDraw,
        buffers: &NativeVulkanSceneMeshDrawBufferRecords,
    ) -> Result<Self, String> {
        validate_mesh_draw_bindings(
            draw,
            buffers.geometry,
            buffers.vertex.key,
            buffers.vertex.bytes,
            buffers.index.key,
            buffers.index.bytes,
        )?;
        Ok(Self {
            geometry: buffers.geometry,
            index_count: draw.index_count,
            vertex_bytes: buffers.vertex.bytes,
            index_bytes: buffers.index.bytes,
            vertex_payload_hash: buffers.vertex.payload_hash,
            index_payload_hash: buffers.index.payload_hash,
            command_order: scene_mesh_draw_command_order(),
        })
    }

    pub fn from_buffer_bindings(
        draw: &SceneGraphDraw,
        buffers: NativeVulkanSceneMeshDrawBuffers,
    ) -> Result<Self, String> {
        validate_mesh_draw_bindings(
            draw,
            buffers.geometry,
            buffers.vertex.key,
            buffers.vertex.bytes,
            buffers.index.key,
            buffers.index.bytes,
        )?;
        Ok(Self {
            geometry: buffers.geometry,
            index_count: draw.index_count,
            vertex_bytes: buffers.vertex.bytes,
            index_bytes: buffers.index.bytes,
            vertex_payload_hash: buffers.vertex.payload_hash,
            index_payload_hash: buffers.index.payload_hash,
            command_order: scene_mesh_draw_command_order(),
        })
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_mesh_draw_command(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    draw: &SceneGraphDraw,
    buffers: NativeVulkanSceneMeshDrawBuffers,
) -> Result<NativeVulkanSceneMeshDrawCommandPlan, String> {
    let plan = NativeVulkanSceneMeshDrawCommandPlan::from_buffer_bindings(draw, buffers)?;
    record_mesh_draw_buffers(
        device,
        command_buffer,
        buffers.vertex,
        buffers.index,
        plan.index_count,
    );
    Ok(plan)
}

fn record_mesh_draw_buffers(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    vertex: NativeVulkanSceneGpuBufferBinding,
    index: NativeVulkanSceneGpuBufferBinding,
    index_count: u32,
) {
    unsafe {
        let vertex_buffers = [vertex.buffer];
        let vertex_offsets = [0u64];
        device.cmd_bind_vertex_buffers(command_buffer, 0, &vertex_buffers, &vertex_offsets);
        device.cmd_bind_index_buffer(command_buffer, index.buffer, 0, vk::IndexType::UINT32);
        device.cmd_draw_indexed(command_buffer, index_count, 1, 0, 0, 0);
    }
}

fn validate_mesh_draw_bindings(
    draw: &SceneGraphDraw,
    geometry: SceneGeometryId,
    vertex_key: NativeVulkanSceneGpuBufferKey,
    vertex_bytes: u64,
    index_key: NativeVulkanSceneGpuBufferKey,
    index_bytes: u64,
) -> Result<(), String> {
    if draw.pipeline != SceneGraphPipelineClass::Mesh {
        return Err(format!(
            "scene mesh draw command requires Mesh pipeline, got {:?}",
            draw.pipeline
        ));
    }
    if draw.index_count == 0 {
        return Err("scene mesh draw command requires non-zero index count".to_owned());
    }
    if draw.geometry != Some(geometry) {
        return Err(format!(
            "scene mesh draw command geometry mismatch: draw {:?}, buffers {:?}",
            draw.geometry, geometry
        ));
    }
    validate_mesh_buffer_key(
        geometry,
        vertex_key,
        NativeVulkanSceneGpuBufferRole::MeshVertex,
    )?;
    validate_mesh_buffer_key(
        geometry,
        index_key,
        NativeVulkanSceneGpuBufferRole::MeshIndex,
    )?;
    if vertex_bytes < SCENE_GPU_MESH_VERTEX_BYTES {
        return Err(format!(
            "scene mesh draw command vertex buffer too small: {vertex_bytes} bytes"
        ));
    }
    let required_index_bytes = u64::from(draw.index_count) * SCENE_GPU_MESH_INDEX_BYTES;
    if index_bytes < required_index_bytes {
        return Err(format!(
            "scene mesh draw command index buffer too small: {index_bytes} bytes for {} indices",
            draw.index_count
        ));
    }
    Ok(())
}

fn validate_mesh_buffer_key(
    geometry: SceneGeometryId,
    key: NativeVulkanSceneGpuBufferKey,
    role: NativeVulkanSceneGpuBufferRole,
) -> Result<(), String> {
    let expected = NativeVulkanSceneGpuBufferKey {
        owner: NativeVulkanSceneGpuBufferOwner::MeshGeometry(geometry),
        role,
    };
    if key != expected {
        return Err(format!(
            "scene mesh draw command expected buffer key {expected:?}, got {key:?}"
        ));
    }
    Ok(())
}

fn scene_mesh_draw_command_order() -> [&'static str; 3] {
    [
        "cmd_bind_vertex_buffers",
        "cmd_bind_index_buffer",
        "cmd_draw_indexed",
    ]
}

#[cfg(test)]
mod tests {
    use super::super::resource_buffers::NativeVulkanSceneGpuBufferRecordBinding;
    use super::*;
    use crate::engine::scene_engine::{
        SceneBlendContract, SceneGraphResourceBinding, SceneMaterialKey, SceneObjectId,
    };

    #[test]
    fn mesh_draw_plan_binds_vertex_index_then_draws_indexed() {
        let draw = mesh_draw(SceneGeometryId(4), 6);
        let records = mesh_records(SceneGeometryId(4), 120, 24);

        let plan =
            NativeVulkanSceneMeshDrawCommandPlan::from_record_bindings(&draw, &records).unwrap();

        assert_eq!(plan.geometry, SceneGeometryId(4));
        assert_eq!(plan.index_count, 6);
        assert_eq!(
            plan.command_order,
            [
                "cmd_bind_vertex_buffers",
                "cmd_bind_index_buffer",
                "cmd_draw_indexed"
            ]
        );
    }

    #[test]
    fn mesh_draw_plan_rejects_geometry_mismatch() {
        let draw = mesh_draw(SceneGeometryId(4), 6);
        let records = mesh_records(SceneGeometryId(5), 120, 24);

        let err = NativeVulkanSceneMeshDrawCommandPlan::from_record_bindings(&draw, &records)
            .expect_err("geometry mismatch must fail");

        assert!(err.contains("geometry mismatch"));
    }

    #[test]
    fn mesh_draw_plan_rejects_incomplete_index_buffer() {
        let draw = mesh_draw(SceneGeometryId(4), 6);
        let records = mesh_records(SceneGeometryId(4), 120, 20);

        let err = NativeVulkanSceneMeshDrawCommandPlan::from_record_bindings(&draw, &records)
            .expect_err("short index buffer must fail");

        assert!(err.contains("index buffer too small"));
    }

    #[test]
    fn mesh_draw_plan_rejects_non_mesh_pipeline() {
        let mut draw = mesh_draw(SceneGeometryId(4), 6);
        draw.pipeline = SceneGraphPipelineClass::Quad;
        let records = mesh_records(SceneGeometryId(4), 120, 24);

        let err = NativeVulkanSceneMeshDrawCommandPlan::from_record_bindings(&draw, &records)
            .expect_err("non-mesh pipeline must fail");

        assert!(err.contains("requires Mesh pipeline"));
    }

    fn mesh_draw(geometry: SceneGeometryId, index_count: u32) -> SceneGraphDraw {
        SceneGraphDraw {
            object: SceneObjectId(2),
            pipeline: SceneGraphPipelineClass::Mesh,
            material: SceneMaterialKey {
                shader: "we/genericimage4".to_owned(),
                blend: SceneBlendContract::TranslucentAlpha,
                render_state: crate::engine::scene_engine::SceneMaterialRenderState::translucent_2d(
                ),
            },
            geometry: Some(geometry),
            puppet: None,
            resources: Vec::<SceneGraphResourceBinding>::new(),
            index_count,
        }
    }

    fn mesh_records(
        geometry: SceneGeometryId,
        vertex_bytes: u64,
        index_bytes: u64,
    ) -> NativeVulkanSceneMeshDrawBufferRecords {
        NativeVulkanSceneMeshDrawBufferRecords {
            geometry,
            vertex: NativeVulkanSceneGpuBufferRecordBinding {
                key: NativeVulkanSceneGpuBufferKey {
                    owner: NativeVulkanSceneGpuBufferOwner::MeshGeometry(geometry),
                    role: NativeVulkanSceneGpuBufferRole::MeshVertex,
                },
                bytes: vertex_bytes,
                payload_hash: 11,
            },
            index: NativeVulkanSceneGpuBufferRecordBinding {
                key: NativeVulkanSceneGpuBufferKey {
                    owner: NativeVulkanSceneGpuBufferOwner::MeshGeometry(geometry),
                    role: NativeVulkanSceneGpuBufferRole::MeshIndex,
                },
                bytes: index_bytes,
                payload_hash: 12,
            },
        }
    }
}
