//! Native Vulkan scene backend planning.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/global-uniforms.md`
//! - `references/godot/servers/rendering/renderer_scene_render.*`
//! - `references/godot/servers/rendering/rendering_device.*`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.*`
//! - `src/renderer/native_vulkan/vulkan/core/descriptor_heap.rs`

use serde::Serialize;

use crate::engine::scene::{
    RendererSceneRenderPlan, RenderingServer, SceneRenderingDeviceGraphPlan, SceneStorage,
};

const SCENE_MESH_VERTEX_UPLOAD_STRIDE_BYTES: usize = 20;
const SCENE_MESH_INDEX_UPLOAD_STRIDE_BYTES: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneBackendPlan {
    pub renderer_scene_render: RendererSceneRenderPlan,
    pub rendering_device_graph: SceneRenderingDeviceGraphPlan,
    pub descriptor_heap: NativeVulkanSceneDescriptorHeapPlan,
    pub mesh_upload: NativeVulkanSceneMeshUploadPlan,
    pub render_graph_executor: &'static str,
    pub present_mode: &'static str,
    pub legacy_descriptor_sets_forbidden: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneDescriptorHeapPlan {
    pub resource_descriptor_count: u32,
    pub sampled_image_descriptor_count: u32,
    pub uniform_buffer_descriptor_count: u32,
    pub storage_buffer_descriptor_count: u32,
    pub sampler_descriptor_count: u32,
    pub shader_contract_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneMeshUploadPlan {
    pub mesh_count: usize,
    pub vertex_count: usize,
    pub index_count: usize,
    pub vertex_buffer_bytes: usize,
    pub index_buffer_bytes: usize,
    pub device_address_required: bool,
}

pub fn native_vulkan_scene_backend_plan(storage: &SceneStorage) -> NativeVulkanSceneBackendPlan {
    let rendering_server = RenderingServer::new(storage);
    let renderer_scene_render = rendering_server.renderer_scene_render_plan();
    let rendering_device_graph = rendering_server.rendering_device_graph_plan();

    NativeVulkanSceneBackendPlan {
        renderer_scene_render,
        rendering_device_graph,
        descriptor_heap: NativeVulkanSceneDescriptorHeapPlan {
            resource_descriptor_count: renderer_scene_render.descriptor_heap_resource_count,
            sampled_image_descriptor_count: renderer_scene_render
                .descriptor_heap_sampled_image_count,
            uniform_buffer_descriptor_count: renderer_scene_render
                .descriptor_heap_uniform_buffer_count,
            storage_buffer_descriptor_count: renderer_scene_render
                .descriptor_heap_storage_buffer_count,
            sampler_descriptor_count: renderer_scene_render.descriptor_heap_sampler_count,
            shader_contract_count: renderer_scene_render.shader_contract_count,
        },
        mesh_upload: NativeVulkanSceneMeshUploadPlan {
            mesh_count: renderer_scene_render.mesh_count,
            vertex_count: renderer_scene_render.mesh_vertex_count,
            index_count: renderer_scene_render.mesh_index_count,
            vertex_buffer_bytes: renderer_scene_render
                .mesh_vertex_count
                .saturating_mul(SCENE_MESH_VERTEX_UPLOAD_STRIDE_BYTES),
            index_buffer_bytes: renderer_scene_render
                .mesh_index_count
                .saturating_mul(SCENE_MESH_INDEX_UPLOAD_STRIDE_BYTES),
            device_address_required: renderer_scene_render.mesh_count > 0,
        },
        render_graph_executor: "renderer/native_vulkan/scene/render_graph_executor",
        present_mode: "fifo-latest-ready",
        legacy_descriptor_sets_forbidden: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::{
        INVALID_MATERIAL_ID, INVALID_OBJECT_ID, SceneBinaryDocument, SceneMaterialHandle,
        SceneMeshRecord, SceneMeshVertexRecord, SceneObjectHandle, SceneObjectKind,
        SceneObjectRecord, SceneResourceId, SceneShaderContractRecord, SceneStorage, SceneStringId,
        SceneVec3,
    };

    #[test]
    fn scene_backend_plan_requires_descriptor_heap_and_fifo_latest_ready() {
        let document = SceneBinaryDocument {
            strings: vec!["shader".to_owned(), "pipeline".to_owned()],
            shader_contracts: vec![SceneShaderContractRecord {
                shader_key: SceneStringId(0),
                pipeline_key: SceneStringId(1),
                texture_slot_mask: 0b101,
                constant_start: 0,
                constant_count: 0,
                resource_heap_count: 3,
                sampler_heap_count: 2,
            }],
            objects: vec![SceneObjectRecord {
                id: SceneObjectHandle(0),
                we_id: 1,
                name: SceneStringId::NONE,
                kind: SceneObjectKind::Image,
                resource: SceneResourceId::NONE,
                material: SceneMaterialHandle(INVALID_MATERIAL_ID),
                parent_we_id: INVALID_OBJECT_ID,
                attachment: SceneStringId::NONE,
                origin: SceneVec3::default(),
                angles: SceneVec3::default(),
                scale: SceneVec3 {
                    x: 1.0,
                    y: 1.0,
                    z: 1.0,
                },
                visible: true,
                color_blend_mode: 0,
                sort_order: 0,
                effect_start: u32::MAX,
                effect_count: 0,
                render_graph: u32::MAX,
            }],
            meshes: vec![SceneMeshRecord {
                object: SceneObjectHandle(0),
                material: SceneMaterialHandle(INVALID_MATERIAL_ID),
                vertex_start: 0,
                vertex_count: 4,
                index_start: 0,
                index_count: 6,
                width: 64.0,
                height: 32.0,
                bounds_min: SceneVec3 {
                    x: -32.0,
                    y: -16.0,
                    z: 0.0,
                },
                bounds_max: SceneVec3 {
                    x: 32.0,
                    y: 16.0,
                    z: 0.0,
                },
            }],
            mesh_vertices: vec![
                SceneMeshVertexRecord {
                    position: SceneVec3 {
                        x: -32.0,
                        y: -16.0,
                        z: 0.0,
                    },
                    uv: [0.0, 1.0],
                },
                SceneMeshVertexRecord {
                    position: SceneVec3 {
                        x: 32.0,
                        y: -16.0,
                        z: 0.0,
                    },
                    uv: [1.0, 1.0],
                },
                SceneMeshVertexRecord {
                    position: SceneVec3 {
                        x: 32.0,
                        y: 16.0,
                        z: 0.0,
                    },
                    uv: [1.0, 0.0],
                },
                SceneMeshVertexRecord {
                    position: SceneVec3 {
                        x: -32.0,
                        y: 16.0,
                        z: 0.0,
                    },
                    uv: [0.0, 0.0],
                },
            ],
            mesh_indices: vec![0, 1, 2, 0, 2, 3],
            ..SceneBinaryDocument::default()
        };
        let storage = SceneStorage::from_document(document).expect("storage");
        let plan = native_vulkan_scene_backend_plan(&storage);

        assert!(plan.legacy_descriptor_sets_forbidden);
        assert_eq!(plan.present_mode, "fifo-latest-ready");
        assert_eq!(plan.descriptor_heap.resource_descriptor_count, 3);
        assert_eq!(plan.descriptor_heap.sampled_image_descriptor_count, 2);
        assert_eq!(plan.descriptor_heap.uniform_buffer_descriptor_count, 1);
        assert_eq!(plan.descriptor_heap.storage_buffer_descriptor_count, 0);
        assert_eq!(plan.descriptor_heap.sampler_descriptor_count, 2);
        assert_eq!(plan.rendering_device_graph.pass_nodes.len(), 0);
        assert_eq!(plan.rendering_device_graph.mesh_draws.len(), 0);
        assert_eq!(plan.mesh_upload.mesh_count, 1);
        assert_eq!(plan.mesh_upload.vertex_count, 4);
        assert_eq!(plan.mesh_upload.index_count, 6);
        assert_eq!(plan.mesh_upload.vertex_buffer_bytes, 80);
        assert_eq!(plan.mesh_upload.index_buffer_bytes, 24);
        assert!(plan.mesh_upload.device_address_required);
    }
}
