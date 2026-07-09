//! Native Vulkan scene resource storage planning.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `references/godot/servers/rendering/storage/*`
//! - `references/godot/servers/rendering/rendering_device.*`
//! - `src/renderer/native_vulkan/vulkan/core/descriptor_heap.rs`

use serde::Serialize;

use crate::engine::scene::{
    RendererSceneRenderPlan, SceneRenderingDeviceGraphPlan, SceneShaderContractRecord,
    SceneStorage, SceneStringId,
};

const SCENE_MESH_VERTEX_UPLOAD_STRIDE_BYTES: usize = 20;
const SCENE_MESH_INDEX_UPLOAD_STRIDE_BYTES: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneResourceStoragePlan {
    pub resource_record_count: usize,
    pub texture_record_count: usize,
    pub material_record_count: usize,
    pub effect_record_count: usize,
    pub resource_payload_bytes: usize,
    pub mesh_buffer: NativeVulkanSceneMeshBufferPlan,
    pub descriptor_heap: NativeVulkanSceneHeapStoragePlan,
    pub shader_heap_slices: Vec<NativeVulkanSceneShaderHeapSlice>,
    pub payload_residency: &'static str,
    pub mesh_residency: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneMeshBufferPlan {
    pub mesh_count: usize,
    pub vertex_count: usize,
    pub index_count: usize,
    pub vertex_buffer_bytes: usize,
    pub index_buffer_bytes: usize,
    pub draw_count: usize,
    pub device_address_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneHeapStoragePlan {
    pub descriptor_model: &'static str,
    pub resource_descriptor_count: u32,
    pub sampled_image_descriptor_count: u32,
    pub uniform_buffer_descriptor_count: u32,
    pub storage_buffer_descriptor_count: u32,
    pub sampler_descriptor_count: u32,
    pub shader_contract_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneShaderHeapSlice {
    pub shader_key: SceneStringId,
    pub pipeline_key: SceneStringId,
    pub resource_descriptor_start: u32,
    pub resource_descriptor_count: u32,
    pub sampled_image_descriptor_count: u32,
    pub uniform_buffer_descriptor_count: u32,
    pub sampler_descriptor_start: u32,
    pub sampler_descriptor_count: u32,
}

pub fn native_vulkan_scene_resource_storage_plan(
    storage: &SceneStorage,
    renderer_scene_render: RendererSceneRenderPlan,
    rendering_device_graph: &SceneRenderingDeviceGraphPlan,
) -> NativeVulkanSceneResourceStoragePlan {
    let shader_heap_slices = shader_heap_slices(storage.document().shader_contracts.as_slice());
    NativeVulkanSceneResourceStoragePlan {
        resource_record_count: renderer_scene_render.resource_count,
        texture_record_count: renderer_scene_render.texture_count,
        material_record_count: renderer_scene_render.material_count,
        effect_record_count: renderer_scene_render.effect_count,
        resource_payload_bytes: renderer_scene_render.resource_payload_bytes,
        mesh_buffer: NativeVulkanSceneMeshBufferPlan {
            mesh_count: renderer_scene_render.mesh_count,
            vertex_count: renderer_scene_render.mesh_vertex_count,
            index_count: renderer_scene_render.mesh_index_count,
            vertex_buffer_bytes: renderer_scene_render
                .mesh_vertex_count
                .saturating_mul(SCENE_MESH_VERTEX_UPLOAD_STRIDE_BYTES),
            index_buffer_bytes: renderer_scene_render
                .mesh_index_count
                .saturating_mul(SCENE_MESH_INDEX_UPLOAD_STRIDE_BYTES),
            draw_count: rendering_device_graph.mesh_draws.len(),
            device_address_required: renderer_scene_render.mesh_count > 0,
        },
        descriptor_heap: NativeVulkanSceneHeapStoragePlan {
            descriptor_model: "VK_EXT_descriptor_heap",
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
        shader_heap_slices,
        payload_residency: "scene-resource-payload-offset-slices",
        mesh_residency: "device-addressable-scene-mesh-buffers",
    }
}

fn shader_heap_slices(
    contracts: &[SceneShaderContractRecord],
) -> Vec<NativeVulkanSceneShaderHeapSlice> {
    let mut resource_descriptor_start = 0;
    let mut sampler_descriptor_start = 0;
    let mut slices = Vec::with_capacity(contracts.len());
    for contract in contracts {
        let sampled_image_descriptor_count = contract.texture_slot_mask.count_ones();
        let uniform_buffer_descriptor_count = contract
            .resource_heap_count
            .saturating_sub(sampled_image_descriptor_count);
        slices.push(NativeVulkanSceneShaderHeapSlice {
            shader_key: contract.shader_key,
            pipeline_key: contract.pipeline_key,
            resource_descriptor_start,
            resource_descriptor_count: contract.resource_heap_count,
            sampled_image_descriptor_count,
            uniform_buffer_descriptor_count,
            sampler_descriptor_start,
            sampler_descriptor_count: contract.sampler_heap_count,
        });
        resource_descriptor_start =
            resource_descriptor_start.saturating_add(contract.resource_heap_count);
        sampler_descriptor_start =
            sampler_descriptor_start.saturating_add(contract.sampler_heap_count);
    }
    slices
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::{
        RenderingServer, SceneBinaryDocument, SceneShaderContractRecord, SceneStorage,
        SceneStringId,
    };

    #[test]
    fn resource_storage_assigns_shader_heap_slices_without_payload_copy() {
        let document = SceneBinaryDocument {
            strings: vec![
                "shader-a".to_owned(),
                "pipeline-a".to_owned(),
                "shader-b".to_owned(),
                "pipeline-b".to_owned(),
            ],
            resource_payload: vec![1, 2, 3, 4],
            shader_contracts: vec![
                SceneShaderContractRecord {
                    shader_key: SceneStringId(0),
                    pipeline_key: SceneStringId(1),
                    texture_slot_mask: 0b101,
                    constant_start: 0,
                    constant_count: 0,
                    resource_heap_count: 4,
                    sampler_heap_count: 2,
                },
                SceneShaderContractRecord {
                    shader_key: SceneStringId(2),
                    pipeline_key: SceneStringId(3),
                    texture_slot_mask: 0b1,
                    constant_start: 0,
                    constant_count: 0,
                    resource_heap_count: 2,
                    sampler_heap_count: 1,
                },
            ],
            ..SceneBinaryDocument::default()
        };
        let storage = SceneStorage::from_document(document).expect("storage");
        let server = RenderingServer::new(&storage);
        let render_plan = server.renderer_scene_render_plan();
        let graph = server.rendering_device_graph_plan();

        let plan = native_vulkan_scene_resource_storage_plan(&storage, render_plan, &graph);

        assert_eq!(plan.resource_payload_bytes, 4);
        assert_eq!(plan.descriptor_heap.resource_descriptor_count, 6);
        assert_eq!(plan.descriptor_heap.sampled_image_descriptor_count, 3);
        assert_eq!(plan.descriptor_heap.uniform_buffer_descriptor_count, 3);
        assert_eq!(plan.descriptor_heap.sampler_descriptor_count, 3);
        assert_eq!(plan.shader_heap_slices.len(), 2);
        assert_eq!(plan.shader_heap_slices[0].resource_descriptor_start, 0);
        assert_eq!(plan.shader_heap_slices[0].sampler_descriptor_start, 0);
        assert_eq!(plan.shader_heap_slices[1].resource_descriptor_start, 4);
        assert_eq!(plan.shader_heap_slices[1].sampler_descriptor_start, 2);
        assert_eq!(
            plan.payload_residency,
            "scene-resource-payload-offset-slices"
        );
    }
}
