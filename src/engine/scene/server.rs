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
        let descriptor_heap_resource_count: u32 = self
            .storage
            .shader_contracts()
            .iter()
            .map(|contract| contract.resource_heap_count)
            .sum();
        RendererSceneRenderPlan {
            object_count: self.storage.objects().len(),
            resource_count: self.storage.resources().len(),
            texture_count: self.storage.document().textures.len(),
            material_count: self.storage.materials().len(),
            mesh_count: self.storage.meshes().len(),
            mesh_vertex_count: self.storage.document().mesh_vertices.len(),
            mesh_index_count: self.storage.document().mesh_indices.len(),
            effect_count: self.storage.effects().len(),
            render_graph_count: self.storage.render_graphs().len(),
            render_pass_count,
            render_binding_count,
            image_target_count: self.storage.document().image_targets.len(),
            shader_contract_count: self.storage.shader_contracts().len(),
            resource_payload_bytes: self.storage.resource_payload_bytes(),
            descriptor_heap_required: true,
            descriptor_heap_resource_count,
            descriptor_heap_sampled_image_count,
            descriptor_heap_uniform_buffer_count: descriptor_heap_resource_count
                .saturating_sub(descriptor_heap_sampled_image_count),
            descriptor_heap_storage_buffer_count: 0,
            descriptor_heap_sampler_count,
            fifo_latest_ready_present_required: true,
        }
    }

    pub fn rendering_device_graph_plan(self) -> SceneRenderingDeviceGraphPlan {
        SceneRenderingDeviceGraphPlan::from_storage(self.storage, self.renderer_scene_render_plan())
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
    pub resource_count: usize,
    pub texture_count: usize,
    pub material_count: usize,
    pub mesh_count: usize,
    pub mesh_vertex_count: usize,
    pub mesh_index_count: usize,
    pub effect_count: usize,
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
    }
}
