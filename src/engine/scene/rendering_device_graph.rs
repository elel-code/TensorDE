//! RenderingDevice graph plan for scene storage.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_device_graph.*`
//! - `references/godot/servers/rendering/renderer_scene_render.*`

use serde::{Deserialize, Serialize};

use super::abi::*;
use super::semantic_world::ResolvedSemanticFrame;
use super::server::RendererSceneRenderPlan;
use super::storage::SceneStorage;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneRenderingDeviceGraphPlan {
    pub pass_nodes: Vec<SceneRenderingDevicePassNode>,
    pub mesh_draws: Vec<SceneRenderingDeviceMeshDraw>,
    pub resolved_object_count: usize,
    pub resolved_visible_object_count: usize,
    pub resolved_attachment_link_count: usize,
    pub descriptor_heap_required: bool,
    pub descriptor_heap_resource_count: u32,
    pub descriptor_heap_sampled_image_count: u32,
    pub descriptor_heap_uniform_buffer_count: u32,
    pub descriptor_heap_storage_buffer_count: u32,
    pub descriptor_heap_sampler_count: u32,
    pub fifo_latest_ready_present_required: bool,
}

impl SceneRenderingDeviceGraphPlan {
    pub(crate) fn from_storage(
        storage: &SceneStorage,
        render_plan: RendererSceneRenderPlan,
    ) -> Self {
        let semantic_frame = super::semantic_world::SceneSemanticWorld::from_storage(storage)
            .expect("scene semantic world must build for graph planning")
            .resolve_frame()
            .expect("scene semantic frame must resolve for graph planning");
        Self::from_storage_with_semantic_frame(storage, render_plan, &semantic_frame)
    }

    pub(crate) fn from_storage_with_semantic_frame(
        storage: &SceneStorage,
        render_plan: RendererSceneRenderPlan,
        semantic_frame: &ResolvedSemanticFrame,
    ) -> Self {
        let mut pass_nodes = Vec::new();
        let mut mesh_draws = Vec::new();

        for (graph_index, graph) in storage.render_graphs().iter().enumerate() {
            for (local_pass_index, pass) in storage.render_graph_passes(graph).iter().enumerate() {
                let mesh_draw_start = mesh_draws.len() as u32;
                if pass_draws_object_mesh(pass) && pass_object_is_visible(semantic_frame, pass) {
                    for (mesh_index, mesh) in storage.meshes().iter().enumerate() {
                        if mesh.object == pass.object {
                            mesh_draws.push(SceneRenderingDeviceMeshDraw {
                                mesh_index: mesh_index as u32,
                                object: mesh.object,
                                material: mesh.material,
                                vertex_start: mesh.vertex_start,
                                vertex_count: mesh.vertex_count,
                                index_start: mesh.index_start,
                                index_count: mesh.index_count,
                            });
                        }
                    }
                }
                pass_nodes.push(SceneRenderingDevicePassNode {
                    graph_index: graph_index as u32,
                    pass_record_index: graph.pass_start + local_pass_index as u32,
                    pass_id: pass.id,
                    role: pass.role,
                    target: pass.target,
                    binding_start: pass.binding_start,
                    binding_count: pass.binding_count,
                    mesh_draw_start,
                    mesh_draw_count: mesh_draws.len() as u32 - mesh_draw_start,
                });
            }
        }

        Self {
            pass_nodes,
            mesh_draws,
            resolved_object_count: semantic_frame.objects.len(),
            resolved_visible_object_count: semantic_frame.visible_object_count,
            resolved_attachment_link_count: semantic_frame.attachment_links.len(),
            descriptor_heap_required: render_plan.descriptor_heap_required,
            descriptor_heap_resource_count: render_plan.descriptor_heap_resource_count,
            descriptor_heap_sampled_image_count: render_plan.descriptor_heap_sampled_image_count,
            descriptor_heap_uniform_buffer_count: render_plan.descriptor_heap_uniform_buffer_count,
            descriptor_heap_storage_buffer_count: render_plan.descriptor_heap_storage_buffer_count,
            descriptor_heap_sampler_count: render_plan.descriptor_heap_sampler_count,
            fifo_latest_ready_present_required: render_plan.fifo_latest_ready_present_required,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneRenderingDevicePassNode {
    pub graph_index: u32,
    pub pass_record_index: u32,
    pub pass_id: u32,
    pub role: SceneRenderPassKind,
    pub target: SceneRenderTargetKind,
    pub binding_start: u32,
    pub binding_count: u32,
    pub mesh_draw_start: u32,
    pub mesh_draw_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneRenderingDeviceMeshDraw {
    pub mesh_index: u32,
    pub object: SceneObjectHandle,
    pub material: SceneMaterialHandle,
    pub vertex_start: u32,
    pub vertex_count: u32,
    pub index_start: u32,
    pub index_count: u32,
}

fn pass_draws_object_mesh(pass: &SceneRenderPassRecord) -> bool {
    pass.object.0 != INVALID_OBJECT_ID
        && matches!(
            pass.role,
            SceneRenderPassKind::BaseMaterial
                | SceneRenderPassKind::EffectMaterial
                | SceneRenderPassKind::ColorBlendPassthrough
        )
}

fn pass_object_is_visible(
    semantic_frame: &ResolvedSemanticFrame,
    pass: &SceneRenderPassRecord,
) -> bool {
    semantic_frame
        .object(pass.object)
        .is_some_and(|object| object.resolved_visible)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::{RenderingServer, SceneStorage};
    use crate::engine::scene::{
        SceneBinaryDocument, SceneMaterialHandle, SceneMeshRecord, SceneMeshVertexRecord,
        SceneObjectHandle, SceneObjectKind, SceneObjectRecord, SceneRenderGraphRecord,
        SceneRenderPassRecord, SceneResourceId, SceneShaderContractRecord, SceneStringId,
        SceneVec3,
    };

    #[test]
    fn rendering_device_graph_plans_mesh_draws_and_heap_counts() {
        let document = SceneBinaryDocument {
            strings: vec!["shader".to_owned(), "pipeline".to_owned()],
            objects: vec![SceneObjectRecord {
                id: SceneObjectHandle(0),
                we_id: 7,
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
                render_graph: 0,
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
                };
                4
            ],
            mesh_indices: vec![0, 1, 2, 0, 2, 3],
            render_graphs: vec![SceneRenderGraphRecord {
                object: SceneObjectHandle(0),
                pass_start: 0,
                pass_count: 1,
                unsupported_start: 0,
                unsupported_count: 0,
            }],
            render_passes: vec![SceneRenderPassRecord {
                id: 9,
                role: SceneRenderPassKind::BaseMaterial,
                object: SceneObjectHandle(0),
                pass_index: 0,
                shader_key: SceneStringId(0),
                target: SceneRenderTargetKind::SceneColor,
                target_name: SceneStringId::NONE,
                binding_start: 0,
                binding_count: 0,
                pipeline_blend: ScenePipelineBlend::Normal,
                depth_test: SceneDepthTest::Disabled,
                depth_write: false,
                cull_mode: SceneCullMode::None,
            }],
            shader_contracts: vec![SceneShaderContractRecord {
                shader_key: SceneStringId(0),
                pipeline_key: SceneStringId(1),
                texture_slot_mask: 0b1,
                constant_start: 0,
                constant_count: 0,
                resource_heap_count: 2,
                sampler_heap_count: 1,
            }],
            ..SceneBinaryDocument::default()
        };
        let storage = SceneStorage::from_document(document).expect("storage");
        let graph = RenderingServer::new(&storage).rendering_device_graph_plan();

        assert_eq!(graph.pass_nodes.len(), 1);
        assert_eq!(graph.pass_nodes[0].pass_id, 9);
        assert_eq!(graph.pass_nodes[0].mesh_draw_count, 1);
        assert_eq!(graph.mesh_draws[0].vertex_count, 4);
        assert_eq!(graph.mesh_draws[0].index_count, 6);
        assert_eq!(graph.descriptor_heap_resource_count, 2);
        assert_eq!(graph.descriptor_heap_sampled_image_count, 1);
        assert_eq!(graph.descriptor_heap_uniform_buffer_count, 1);
        assert!(graph.fifo_latest_ready_present_required);
    }
}
