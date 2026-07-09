//! Native Vulkan scene pipeline cache planning.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/effect-format.md`
//! - `references/godot/servers/rendering/renderer_rd/pipeline_hash_map_rd.h`
//! - `references/godot/servers/rendering/rendering_device_graph.*`

use serde::Serialize;

use crate::engine::scene::{ScenePipelineBlend, SceneStorage, SceneStringId};

use super::resource_storage::{
    NativeVulkanSceneResourceStoragePlan, NativeVulkanSceneShaderHeapSlice,
};
use super::shader_catalog::native_vulkan_scene_shader_for_key;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanScenePipelineCachePlan {
    pub pipeline_count: usize,
    pub entries: Vec<NativeVulkanScenePipelineCacheEntry>,
    pub shader_catalog_entry_count: usize,
    pub shader_catalog_hit_count: usize,
    pub missing_shader_keys: Vec<String>,
    pub cache_model: &'static str,
    pub shader_catalog_source: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanScenePipelineCacheEntry {
    pub shader_key: SceneStringId,
    pub pipeline_key: SceneStringId,
    pub resource_descriptor_start: u32,
    pub resource_descriptor_count: u32,
    pub sampler_descriptor_start: u32,
    pub sampler_descriptor_count: u32,
    pub material_pass_count: u32,
    pub render_pass_count: u32,
    pub primary_blend: ScenePipelineBlend,
    pub shader_catalog_available: bool,
    pub shader_catalog_key: Option<&'static str>,
    pub vertex_spirv_bytes: usize,
    pub fragment_spirv_bytes: usize,
}

pub fn native_vulkan_scene_pipeline_cache_plan(
    storage: &SceneStorage,
    resource_storage: &NativeVulkanSceneResourceStoragePlan,
) -> NativeVulkanScenePipelineCachePlan {
    let entries = resource_storage
        .shader_heap_slices
        .iter()
        .map(|slice| pipeline_entry_for_slice(storage, *slice))
        .collect::<Vec<_>>();
    let shader_catalog_hit_count = entries
        .iter()
        .filter(|entry| entry.shader_catalog_available)
        .count();
    let missing_shader_keys = entries
        .iter()
        .filter(|entry| !entry.shader_catalog_available)
        .filter_map(|entry| storage.string(entry.shader_key))
        .map(str::to_owned)
        .collect::<Vec<_>>();

    NativeVulkanScenePipelineCachePlan {
        pipeline_count: entries.len(),
        entries,
        shader_catalog_entry_count: super::shader_catalog::native_vulkan_scene_shader_catalog()
            .len(),
        shader_catalog_hit_count,
        missing_shader_keys,
        cache_model: "pipeline-key-hash-cache",
        shader_catalog_source: "built-in-scene-shader-catalog",
    }
}

fn pipeline_entry_for_slice(
    storage: &SceneStorage,
    slice: NativeVulkanSceneShaderHeapSlice,
) -> NativeVulkanScenePipelineCacheEntry {
    let material_passes = storage
        .document()
        .material_passes
        .iter()
        .filter(|pass| pass.shader_key == slice.shader_key)
        .collect::<Vec<_>>();
    let render_pass_count = storage
        .document()
        .render_passes
        .iter()
        .filter(|pass| pass.shader_key == slice.shader_key)
        .count() as u32;
    let primary_blend = material_passes
        .first()
        .map(|pass| pass.pipeline_blend)
        .unwrap_or(ScenePipelineBlend::Normal);
    let shader = storage
        .string(slice.shader_key)
        .and_then(native_vulkan_scene_shader_for_key);

    NativeVulkanScenePipelineCacheEntry {
        shader_key: slice.shader_key,
        pipeline_key: slice.pipeline_key,
        resource_descriptor_start: slice.resource_descriptor_start,
        resource_descriptor_count: slice.resource_descriptor_count,
        sampler_descriptor_start: slice.sampler_descriptor_start,
        sampler_descriptor_count: slice.sampler_descriptor_count,
        material_pass_count: material_passes.len() as u32,
        render_pass_count,
        primary_blend,
        shader_catalog_available: shader.is_some(),
        shader_catalog_key: shader.map(|shader| shader.key),
        vertex_spirv_bytes: shader.map_or(0, |shader| shader.vertex_spirv.len() * 4),
        fragment_spirv_bytes: shader.map_or(0, |shader| shader.fragment_spirv.len() * 4),
    }
}

#[cfg(test)]
mod tests {
    use super::super::resource_storage::native_vulkan_scene_resource_storage_plan;
    use super::*;
    use crate::engine::scene::{
        RenderingServer, SceneBinaryDocument, SceneMaterialPassRecord, SceneShaderContractRecord,
        SceneStorage, SceneStringId,
    };

    #[test]
    fn pipeline_cache_keys_builtin_shader_contracts_and_pass_state() {
        let document = SceneBinaryDocument {
            strings: vec!["genericimage4".to_owned(), "pipeline".to_owned()],
            shader_contracts: vec![SceneShaderContractRecord {
                shader_key: SceneStringId(0),
                pipeline_key: SceneStringId(1),
                texture_slot_mask: 0,
                constant_start: 0,
                constant_count: 0,
                resource_heap_count: 1,
                sampler_heap_count: 0,
            }],
            material_passes: vec![SceneMaterialPassRecord {
                material: crate::engine::scene::SceneMaterialHandle(0),
                shader_key: SceneStringId(0),
                target: SceneStringId::NONE,
                texture_start: 0,
                texture_count: 0,
                constant_start: 0,
                constant_count: 0,
                pipeline_blend: ScenePipelineBlend::Additive,
                depth_test: crate::engine::scene::SceneDepthTest::Disabled,
                depth_write: false,
                cull_mode: crate::engine::scene::SceneCullMode::None,
                alpha_writing: SceneStringId::NONE,
                clear_target: false,
            }],
            ..SceneBinaryDocument::default()
        };
        let storage = SceneStorage::from_document(document).expect("storage");
        let server = RenderingServer::new(&storage);
        let render_plan = server.renderer_scene_render_plan();
        let graph = server.rendering_device_graph_plan();
        let resource_storage =
            native_vulkan_scene_resource_storage_plan(&storage, render_plan, &graph);

        let cache = native_vulkan_scene_pipeline_cache_plan(&storage, &resource_storage);

        assert_eq!(cache.pipeline_count, 1);
        assert_eq!(cache.entries[0].pipeline_key, SceneStringId(1));
        assert_eq!(cache.entries[0].material_pass_count, 1);
        assert_eq!(cache.entries[0].primary_blend, ScenePipelineBlend::Additive);
        assert_eq!(cache.shader_catalog_hit_count, 1);
        assert_eq!(cache.missing_shader_keys, Vec::<String>::new());
        assert_eq!(
            cache.entries[0].shader_catalog_key,
            Some("we/genericimage4")
        );
        assert!(cache.entries[0].vertex_spirv_bytes > 0);
        assert!(cache.entries[0].fragment_spirv_bytes > 0);
        assert_eq!(cache.shader_catalog_source, "built-in-scene-shader-catalog");
    }
}
