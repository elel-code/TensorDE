//! RenderingServer boundary for the new scene engine.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/exe/scene-and-object.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_server_default.*`
//! - `references/godot/servers/rendering/renderer_scene_render.*`
//! - `references/godot/servers/rendering/rendering_device_graph.*`

use serde::{Deserialize, Serialize};

use super::abi::*;
use super::rendering_device_graph::SceneRenderingDeviceGraphPlan;
use super::semantic_world::{ResolvedSemanticFrame, SceneSemanticWorld, SceneSemanticWorldError};
use super::storage::SceneStorage;

#[derive(Debug, Clone, Copy)]
pub struct RenderingServer<'a> {
    storage: &'a SceneStorage,
}

impl<'a> RenderingServer<'a> {
    pub fn new(storage: &'a SceneStorage) -> Self {
        Self { storage }
    }

    pub fn storage(self) -> &'a SceneStorage {
        self.storage
    }

    pub fn renderer_scene_render_plan(self) -> RendererSceneRenderPlan {
        self.renderer_scene_render_plan_at(0.0)
    }

    pub fn try_renderer_scene_render_plan(
        self,
    ) -> Result<RendererSceneRenderPlan, SceneSemanticWorldError> {
        self.try_renderer_scene_render_plan_at(0.0)
    }

    pub fn renderer_scene_render_plan_at(self, scene_time_seconds: f32) -> RendererSceneRenderPlan {
        self.try_renderer_scene_render_plan_at(scene_time_seconds)
            .expect("scene semantic frame must resolve before render planning")
    }

    pub fn try_renderer_scene_render_plan_at(
        self,
        scene_time_seconds: f32,
    ) -> Result<RendererSceneRenderPlan, SceneSemanticWorldError> {
        let semantic_frame = self.resolved_semantic_frame_at(scene_time_seconds)?;
        Ok(self.renderer_scene_render_plan_from_semantic_frame(&semantic_frame))
    }

    fn renderer_scene_render_plan_from_semantic_frame(
        self,
        semantic_frame: &ResolvedSemanticFrame,
    ) -> RendererSceneRenderPlan {
        let render_pass_count = self
            .storage
            .render_graphs()
            .iter()
            .map(|graph| graph.pass_count as usize)
            .sum();
        let render_binding_count = self
            .storage
            .document()
            .render_passes
            .iter()
            .map(|pass| pass.binding_count as usize)
            .sum();
        let descriptor_heap_sampled_image_count: u32 = self
            .storage
            .shader_contracts()
            .iter()
            .map(|contract| contract.texture_slot_mask.count_ones())
            .sum();
        let descriptor_heap_sampler_count: u32 = self
            .storage
            .shader_contracts()
            .iter()
            .map(|contract| contract.sampler_heap_count)
            .sum();
        let contract_resource_descriptor_count: u32 = self
            .storage
            .shader_contracts()
            .iter()
            .map(|contract| contract.resource_heap_count)
            .sum();
        let descriptor_heap_storage_buffer_count =
            u32::from(!semantic_frame.puppet_bone_matrices.is_empty());
        let descriptor_heap_resource_count =
            contract_resource_descriptor_count.saturating_add(descriptor_heap_storage_buffer_count);
        RendererSceneRenderPlan {
            object_count: self.storage.objects().len(),
            visible_object_count: semantic_frame.visible_object_count,
            resource_count: self.storage.resources().len(),
            texture_count: self.storage.document().textures.len(),
            material_count: self.storage.materials().len(),
            mesh_count: self.storage.meshes().len(),
            visible_mesh_binding_count: semantic_frame.visible_mesh_binding_count,
            mesh_vertex_count: self.storage.document().mesh_vertices.len(),
            mesh_index_count: self.storage.document().mesh_indices.len(),
            puppet_binding_count: self.storage.puppets().len(),
            visible_puppet_binding_count: semantic_frame.visible_puppet_binding_count,
            puppet_bone_palette_count: semantic_frame.puppet_bone_palettes.len(),
            puppet_bone_matrix_count: semantic_frame.puppet_bone_matrices.len(),
            visible_puppet_bone_matrix_count: semantic_frame.visible_puppet_bone_matrix_count,
            attachment_link_count: semantic_frame.attachment_links.len(),
            effect_count: self.storage.effects().len(),
            visible_effect_instance_count: semantic_frame.visible_effect_instance_count,
            visible_effect_pass_count: semantic_frame.visible_effect_pass_count,
            visible_effect_fbo_count: semantic_frame.visible_effect_fbo_count,
            render_graph_count: self.storage.render_graphs().len(),
            render_pass_count,
            render_binding_count,
            image_target_count: self.storage.document().image_targets.len(),
            shader_contract_count: self.storage.shader_contracts().len(),
            resource_payload_bytes: self.storage.resource_payload_bytes(),
            descriptor_heap_required: true,
            descriptor_heap_resource_count,
            descriptor_heap_sampled_image_count,
            descriptor_heap_uniform_buffer_count: contract_resource_descriptor_count
                .saturating_sub(descriptor_heap_sampled_image_count),
            descriptor_heap_storage_buffer_count,
            descriptor_heap_sampler_count,
            fifo_latest_ready_present_required: true,
        }
    }

    pub fn rendering_device_graph_plan(self) -> SceneRenderingDeviceGraphPlan {
        self.rendering_device_graph_plan_at(0.0)
    }

    pub fn try_rendering_device_graph_plan(
        self,
    ) -> Result<SceneRenderingDeviceGraphPlan, SceneSemanticWorldError> {
        self.try_rendering_device_graph_plan_at(0.0)
    }

    pub fn rendering_device_graph_plan_at(
        self,
        scene_time_seconds: f32,
    ) -> SceneRenderingDeviceGraphPlan {
        self.try_rendering_device_graph_plan_at(scene_time_seconds)
            .expect("scene semantic frame must resolve before graph planning")
    }

    pub fn try_rendering_device_graph_plan_at(
        self,
        scene_time_seconds: f32,
    ) -> Result<SceneRenderingDeviceGraphPlan, SceneSemanticWorldError> {
        let semantic_frame = self.resolved_semantic_frame_at(scene_time_seconds)?;
        let render_plan = self.renderer_scene_render_plan_from_semantic_frame(&semantic_frame);
        Ok(
            SceneRenderingDeviceGraphPlan::from_storage_with_semantic_frame(
                self.storage,
                render_plan,
                &semantic_frame,
            ),
        )
    }

    pub fn scene_engine_render_plan(self) -> SceneEngineRenderPlan {
        self.scene_engine_render_plan_at(0.0)
    }

    pub fn try_scene_engine_render_plan(
        self,
    ) -> Result<SceneEngineRenderPlan, SceneSemanticWorldError> {
        self.try_scene_engine_render_plan_at(0.0)
    }

    pub fn scene_engine_render_plan_at(self, scene_time_seconds: f32) -> SceneEngineRenderPlan {
        self.try_scene_engine_render_plan_at(scene_time_seconds)
            .expect("scene semantic frame must resolve before render planning")
    }

    pub fn try_scene_engine_render_plan_at(
        self,
        scene_time_seconds: f32,
    ) -> Result<SceneEngineRenderPlan, SceneSemanticWorldError> {
        let semantic_frame = self.resolved_semantic_frame_at(scene_time_seconds)?;
        let renderer_scene_render =
            self.renderer_scene_render_plan_from_semantic_frame(&semantic_frame);
        let rendering_device_graph =
            SceneRenderingDeviceGraphPlan::from_storage_with_semantic_frame(
                self.storage,
                renderer_scene_render,
                &semantic_frame,
            );
        Ok(SceneEngineRenderPlan {
            renderer_scene_render,
            rendering_device_graph,
        })
    }

    pub fn semantic_world(self) -> Result<SceneSemanticWorld<'a>, SceneSemanticWorldError> {
        SceneSemanticWorld::from_storage(self.storage)
    }

    pub fn resolved_semantic_frame(self) -> Result<ResolvedSemanticFrame, SceneSemanticWorldError> {
        self.resolved_semantic_frame_at(0.0)
    }

    pub fn resolved_semantic_frame_at(
        self,
        scene_time_seconds: f32,
    ) -> Result<ResolvedSemanticFrame, SceneSemanticWorldError> {
        self.semantic_world()?.resolve_frame_at(scene_time_seconds)
    }

    pub fn object_render_graph(
        self,
        object: &SceneObjectRecord,
    ) -> Option<SceneObjectRenderGraph<'a>> {
        if object.render_graph == u32::MAX {
            return None;
        }
        let graph = self
            .storage
            .render_graphs()
            .get(object.render_graph as usize)?;
        Some(SceneObjectRenderGraph {
            graph,
            passes: self.storage.render_graph_passes(graph),
            storage: self.storage,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SceneObjectRenderGraph<'a> {
    pub graph: &'a SceneRenderGraphRecord,
    pub passes: &'a [SceneRenderPassRecord],
    storage: &'a SceneStorage,
}

impl<'a> SceneObjectRenderGraph<'a> {
    pub fn bindings_for(self, pass: &SceneRenderPassRecord) -> &'a [SceneRenderBindingRecord] {
        self.storage.render_pass_bindings(pass)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RendererSceneRenderPlan {
    pub object_count: usize,
    pub visible_object_count: usize,
    pub resource_count: usize,
    pub texture_count: usize,
    pub material_count: usize,
    pub mesh_count: usize,
    pub visible_mesh_binding_count: usize,
    pub mesh_vertex_count: usize,
    pub mesh_index_count: usize,
    pub puppet_binding_count: usize,
    pub visible_puppet_binding_count: usize,
    pub puppet_bone_palette_count: usize,
    pub puppet_bone_matrix_count: usize,
    pub visible_puppet_bone_matrix_count: usize,
    pub attachment_link_count: usize,
    pub effect_count: usize,
    pub visible_effect_instance_count: usize,
    pub visible_effect_pass_count: usize,
    pub visible_effect_fbo_count: usize,
    pub render_graph_count: usize,
    pub render_pass_count: usize,
    pub render_binding_count: usize,
    pub image_target_count: usize,
    pub shader_contract_count: usize,
    pub resource_payload_bytes: usize,
    pub descriptor_heap_required: bool,
    pub descriptor_heap_resource_count: u32,
    pub descriptor_heap_sampled_image_count: u32,
    pub descriptor_heap_uniform_buffer_count: u32,
    pub descriptor_heap_storage_buffer_count: u32,
    pub descriptor_heap_sampler_count: u32,
    pub fifo_latest_ready_present_required: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneEngineRenderPlan {
    pub renderer_scene_render: RendererSceneRenderPlan,
    pub rendering_device_graph: SceneRenderingDeviceGraphPlan,
}

impl RendererSceneRenderPlan {
    pub fn is_executable_boundary_ready(self) -> bool {
        self.descriptor_heap_required && self.fifo_latest_ready_present_required
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::binary::SceneBinaryDocument;
    use crate::engine::scene::storage::SceneStorage;

    #[test]
    fn rendering_server_counts_scene_storage_boundaries() {
        let storage = SceneStorage::from_document(SceneBinaryDocument::default()).expect("storage");
        let plan = RenderingServer::new(&storage).renderer_scene_render_plan();

        assert!(plan.is_executable_boundary_ready());
        assert_eq!(plan.object_count, 0);
        assert_eq!(plan.resource_payload_bytes, 0);

        let scene_engine = RenderingServer::new(&storage).scene_engine_render_plan();
        assert_eq!(scene_engine.renderer_scene_render.object_count, 0);
        assert!(scene_engine.rendering_device_graph.pass_nodes.is_empty());

        let timed_scene_engine = RenderingServer::new(&storage).scene_engine_render_plan_at(1.25);
        assert_eq!(timed_scene_engine.renderer_scene_render.object_count, 0);
        assert!(
            timed_scene_engine
                .rendering_device_graph
                .pass_nodes
                .is_empty()
        );

        let semantic_frame = RenderingServer::new(&storage)
            .resolved_semantic_frame()
            .expect("resolved semantic frame");
        assert!(semantic_frame.objects.is_empty());
    }
}
