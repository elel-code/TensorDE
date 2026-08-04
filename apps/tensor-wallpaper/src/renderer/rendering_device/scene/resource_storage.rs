//! Vulkan scene resource storage planning.
//!
//! References:
//! - `docs/tensor-wallpaper/tensor-wallpaper-scene-engine-architecture.md`
//! - `reverse-engineered/tensor-wallpaper/docs/scene-format.md`
//! - `reverse-engineered/tensor-wallpaper/docs/material-format.md`
//! - `references/tensor-wallpaper/godot/servers/rendering/storage/*`
//! - `references/tensor-wallpaper/godot/servers/rendering/rendering_device.*`
//! - `crates/vulkan-renderer/src/descriptor_heap.rs`

use serde::Serialize;

use crate::engine::scene::{
    RendererSceneRenderPlan, SceneRenderingDeviceGraphPlan, SceneShaderContractRecord,
    SceneStorage, SceneStringId,
};

const SCENE_MESH_VERTEX_UPLOAD_STRIDE_BYTES: usize = 52;
const SCENE_MESH_INDEX_UPLOAD_STRIDE_BYTES: usize = 4;
pub const RENDERING_DEVICE_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderingDeviceSceneResourceStoragePlan {
    pub resource_record_count: usize,
    pub texture_record_count: usize,
    pub material_record_count: usize,
    pub effect_record_count: usize,
    pub resource_payload_bytes: usize,
    pub mesh_buffer: RenderingDeviceSceneMeshBufferPlan,
    pub skinning_buffer: RenderingDeviceSceneSkinningBufferPlan,
    pub effect_target_storage: RenderingDeviceSceneEffectTargetStoragePlan,
    pub descriptor_heap: RenderingDeviceSceneHeapStoragePlan,
    pub shader_heap_slices: Vec<RenderingDeviceSceneShaderHeapSlice>,
    pub payload_residency: &'static str,
    pub mesh_residency: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderingDeviceSceneSkinningBufferPlan {
    pub palette_count: usize,
    pub bone_matrix_count: usize,
    pub bone_matrix_buffer_bytes: usize,
    pub storage_buffer_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderingDeviceSceneEffectTargetStoragePlan {
    pub logical_target_count: usize,
    pub physical_target_count: usize,
    pub aliased_target_count: usize,
    pub named_fbo_count: usize,
    pub first_class_effect_target_count: usize,
    pub dynamic_rendering_image_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderingDeviceSceneMeshBufferPlan {
    pub mesh_count: usize,
    pub vertex_count: usize,
    pub index_count: usize,
    pub vertex_buffer_bytes: usize,
    pub index_buffer_bytes: usize,
    pub draw_count: usize,
    pub device_address_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderingDeviceSceneHeapStoragePlan {
    pub descriptor_model: &'static str,
    pub resource_descriptor_count: u32,
    pub sampled_image_descriptor_count: u32,
    pub input_attachment_descriptor_count: u32,
    pub uniform_buffer_descriptor_count: u32,
    pub storage_buffer_descriptor_count: u32,
    pub sampler_descriptor_count: u32,
    pub shader_contract_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RenderingDeviceSceneShaderHeapSlice {
    pub shader_key: SceneStringId,
    pub pipeline_key: SceneStringId,
    pub resource_descriptor_start: u32,
    pub resource_descriptor_count: u32,
    pub sampled_image_descriptor_count: u32,
    pub input_attachment_descriptor_count: u32,
    pub uniform_buffer_descriptor_count: u32,
    pub sampler_descriptor_start: u32,
    pub sampler_descriptor_count: u32,
}

pub fn rendering_device_scene_resource_storage_plan(
    storage: &SceneStorage,
    renderer_scene_render: RendererSceneRenderPlan,
    rendering_device_graph: &SceneRenderingDeviceGraphPlan,
) -> RenderingDeviceSceneResourceStoragePlan {
    let shader_heap_slices = shader_heap_slices(storage.document().shader_contracts.as_slice());
    let utility_vertex_count = rendering_device_graph.bound_utility_vertex_count(storage);
    let vertex_count = renderer_scene_render
        .mesh_vertex_count
        .saturating_add(utility_vertex_count);
    let index_count = renderer_scene_render
        .mesh_index_count
        .saturating_add(rendering_device_graph.fullscreen_utility_draw_count() * 3);
    RenderingDeviceSceneResourceStoragePlan {
        resource_record_count: renderer_scene_render.resource_count,
        texture_record_count: renderer_scene_render.texture_count,
        material_record_count: renderer_scene_render.material_count,
        effect_record_count: renderer_scene_render.effect_count,
        resource_payload_bytes: renderer_scene_render.resource_payload_bytes,
        mesh_buffer: RenderingDeviceSceneMeshBufferPlan {
            mesh_count: renderer_scene_render.mesh_count,
            vertex_count,
            index_count,
            vertex_buffer_bytes: vertex_count.saturating_mul(SCENE_MESH_VERTEX_UPLOAD_STRIDE_BYTES),
            index_buffer_bytes: index_count.saturating_mul(SCENE_MESH_INDEX_UPLOAD_STRIDE_BYTES),
            draw_count: rendering_device_graph.mesh_draws.len(),
            device_address_required: renderer_scene_render.mesh_count > 0
                || utility_vertex_count != 0,
        },
        skinning_buffer: RenderingDeviceSceneSkinningBufferPlan {
            palette_count: rendering_device_graph.puppet_bone_palettes.len(),
            bone_matrix_count: rendering_device_graph.puppet_bone_matrices.len(),
            bone_matrix_buffer_bytes: rendering_device_graph
                .puppet_bone_matrices
                .len()
                .saturating_add(usize::from(
                    !rendering_device_graph.puppet_bone_matrices.is_empty(),
                ))
                .saturating_mul(RENDERING_DEVICE_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES),
            storage_buffer_required: !rendering_device_graph.puppet_bone_matrices.is_empty(),
        },
        effect_target_storage: effect_target_storage_plan(rendering_device_graph),
        descriptor_heap: RenderingDeviceSceneHeapStoragePlan {
            descriptor_model: "VK_EXT_descriptor_heap",
            resource_descriptor_count: renderer_scene_render.descriptor_heap_resource_count,
            sampled_image_descriptor_count: renderer_scene_render
                .descriptor_heap_sampled_image_count,
            input_attachment_descriptor_count: renderer_scene_render
                .descriptor_heap_input_attachment_count,
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

fn effect_target_storage_plan(
    graph: &SceneRenderingDeviceGraphPlan,
) -> RenderingDeviceSceneEffectTargetStoragePlan {
    let named_fbo_count = graph
        .target_allocations
        .iter()
        .filter(|allocation| {
            allocation.target == crate::engine::scene::SceneRenderTargetKind::NamedFbo
        })
        .count();
    let first_class_effect_target_count = graph
        .target_allocations
        .iter()
        .filter(|allocation| {
            allocation.target == crate::engine::scene::SceneRenderTargetKind::FirstClassEffectTarget
        })
        .count();
    RenderingDeviceSceneEffectTargetStoragePlan {
        logical_target_count: graph.target_allocations.len(),
        physical_target_count: graph.graph_physical_target_count as usize,
        aliased_target_count: graph.graph_aliased_target_count as usize,
        named_fbo_count,
        first_class_effect_target_count,
        dynamic_rendering_image_required: !graph.target_allocations.is_empty(),
    }
}

fn shader_heap_slices(
    contracts: &[SceneShaderContractRecord],
) -> Vec<RenderingDeviceSceneShaderHeapSlice> {
    let mut resource_descriptor_start = 0;
    let mut sampler_descriptor_start = 0;
    let mut slices = Vec::with_capacity(contracts.len());
    for contract in contracts {
        let sampled_image_descriptor_count = contract.texture_slot_mask.count_ones();
        let input_attachment_descriptor_count =
            contract.input_attachment_slot_mask.count_ones();
        let uniform_buffer_descriptor_count = contract
            .resource_heap_count
            .saturating_sub(sampled_image_descriptor_count)
            .saturating_sub(input_attachment_descriptor_count);
        slices.push(RenderingDeviceSceneShaderHeapSlice {
            shader_key: contract.shader_key,
            pipeline_key: contract.pipeline_key,
            resource_descriptor_start,
            resource_descriptor_count: contract.resource_heap_count,
            sampled_image_descriptor_count,
            input_attachment_descriptor_count,
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
        RenderingServer, SceneBinaryDocument, SceneRenderTargetKind, SceneRenderingDeviceGraphPlan,
        SceneRenderingDevicePuppetBoneMatrix, SceneRenderingDeviceTargetAllocation,
        SceneShaderContractRecord, SceneStorage, SceneStringId, SceneTargetExtentDomain,
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
                    input_attachment_slot_mask: 0,
                    constant_start: 0,
                    constant_count: 0,
                    resource_heap_count: 4,
                    sampler_heap_count: 2,
                },
                SceneShaderContractRecord {
                    shader_key: SceneStringId(2),
                    pipeline_key: SceneStringId(3),
                    texture_slot_mask: 0b1,
                    input_attachment_slot_mask: 0b10,
                    constant_start: 0,
                    constant_count: 0,
                    resource_heap_count: 3,
                    sampler_heap_count: 1,
                },
            ],
            ..SceneBinaryDocument::default()
        };
        let storage = SceneStorage::from_document(document).expect("storage");
        let server = RenderingServer::new(&storage);
        let render_plan = server.renderer_scene_render_plan();
        let graph = server.rendering_device_graph_plan();

        let plan = rendering_device_scene_resource_storage_plan(&storage, render_plan, &graph);

        assert_eq!(plan.resource_payload_bytes, 4);
        assert_eq!(plan.descriptor_heap.resource_descriptor_count, 7);
        assert_eq!(plan.descriptor_heap.sampled_image_descriptor_count, 3);
        assert_eq!(plan.descriptor_heap.input_attachment_descriptor_count, 1);
        assert_eq!(plan.descriptor_heap.uniform_buffer_descriptor_count, 3);
        assert_eq!(plan.descriptor_heap.sampler_descriptor_count, 3);
        assert_eq!(plan.shader_heap_slices.len(), 2);
        assert_eq!(plan.shader_heap_slices[0].resource_descriptor_start, 0);
        assert_eq!(plan.shader_heap_slices[0].sampler_descriptor_start, 0);
        assert_eq!(plan.shader_heap_slices[1].resource_descriptor_start, 4);
        assert_eq!(plan.shader_heap_slices[1].sampler_descriptor_start, 2);
        assert_eq!(
            plan.shader_heap_slices[1].input_attachment_descriptor_count,
            1
        );
        assert_eq!(plan.shader_heap_slices[1].uniform_buffer_descriptor_count, 1);
        assert_eq!(
            plan.payload_residency,
            "scene-resource-payload-offset-slices"
        );
    }

    #[test]
    fn resource_storage_tracks_effect_target_image_allocation_pressure() {
        let storage = SceneStorage::from_document(SceneBinaryDocument::default()).expect("storage");
        let render_plan = RenderingServer::new(&storage).renderer_scene_render_plan();
        let graph = SceneRenderingDeviceGraphPlan {
            target_allocations: vec![
                SceneRenderingDeviceTargetAllocation {
                    graph_index: 0,
                    target: SceneRenderTargetKind::NamedFbo,
                    target_name: SceneStringId(0),
                    first_write_pass_id: 1,
                    last_use_pass_id: 2,
                    physical_slot: 0,
                    width: 0,
                    height: 0,
                    extent_domain: SceneTargetExtentDomain::PhysicalSurface,
                },
                SceneRenderingDeviceTargetAllocation {
                    graph_index: 0,
                    target: SceneRenderTargetKind::FirstClassEffectTarget,
                    target_name: SceneStringId(1),
                    first_write_pass_id: 3,
                    last_use_pass_id: 4,
                    physical_slot: 0,
                    width: 0,
                    height: 0,
                    extent_domain: SceneTargetExtentDomain::PhysicalSurface,
                },
            ],
            graph_physical_target_count: 1,
            graph_aliased_target_count: 1,
            ..empty_graph_plan()
        };

        let plan = rendering_device_scene_resource_storage_plan(&storage, render_plan, &graph);

        assert_eq!(plan.effect_target_storage.logical_target_count, 2);
        assert_eq!(plan.effect_target_storage.physical_target_count, 1);
        assert_eq!(plan.effect_target_storage.aliased_target_count, 1);
        assert_eq!(plan.effect_target_storage.named_fbo_count, 1);
        assert_eq!(
            plan.effect_target_storage.first_class_effect_target_count,
            1
        );
        assert!(plan.effect_target_storage.dynamic_rendering_image_required);
    }

    #[test]
    fn skinning_storage_accounts_for_alpha_and_identity_fallback_entry() {
        let storage = SceneStorage::from_document(SceneBinaryDocument::default()).expect("storage");
        let render_plan = RenderingServer::new(&storage).renderer_scene_render_plan();
        let mut graph = empty_graph_plan();
        graph.puppet_bone_matrices = vec![SceneRenderingDevicePuppetBoneMatrix {
            puppet_index: 0,
            bone_index: 0,
            parent_index: -1,
            matrix: [[0.0; 4]; 4],
            alpha: 0.5,
        }];

        let plan = rendering_device_scene_resource_storage_plan(&storage, render_plan, &graph);

        assert_eq!(plan.skinning_buffer.bone_matrix_count, 1);
        assert_eq!(
            plan.skinning_buffer.bone_matrix_buffer_bytes,
            2 * RENDERING_DEVICE_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES
        );
        assert!(plan.skinning_buffer.storage_buffer_required);
    }

    fn empty_graph_plan() -> SceneRenderingDeviceGraphPlan {
        SceneRenderingDeviceGraphPlan {
            pass_nodes: Vec::new(),
            target_allocations: Vec::new(),
            effect_batches: Vec::new(),
            effect_batch_instances: Vec::new(),
            sampled_bindings: Vec::new(),
            material_sampled_bindings: Vec::new(),
            mesh_draws: Vec::new(),
            puppet_bone_palettes: Vec::new(),
            puppet_bone_matrices: Vec::new(),
            particle_gpu_emitters: Vec::new(),
            resolved_object_count: 0,
            resolved_visible_object_count: 0,
            resolved_attachment_link_count: 0,
            resolved_visible_effect_instance_count: 0,
            resolved_visible_effect_pass_count: 0,
            resolved_visible_effect_fbo_count: 0,
            descriptor_heap_required: true,
            descriptor_heap_resource_count: 0,
            descriptor_heap_sampled_image_count: 0,
            descriptor_heap_uniform_buffer_count: 0,
            descriptor_heap_storage_buffer_count: 0,
            descriptor_heap_sampler_count: 0,
            graph_physical_target_count: 0,
            graph_aliased_target_count: 0,
            fifo_latest_ready_present_required: true,
        }
    }
}
