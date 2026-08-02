//! Native Vulkan scene pipeline cache planning.
//!
//! References:
//! - `docs/gilder/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/gilder/docs/material-format.md`
//! - `reverse-engineered/gilder/docs/effect-format.md`
//! - `references/gilder/godot/servers/rendering/renderer_rd/pipeline_hash_map_rd.h`
//! - `references/gilder/godot/servers/rendering/rendering_device_graph.*`

use serde::Serialize;

use crate::engine::scene::{
    ScenePipelineBlend, SceneRenderingDeviceDrawPrimitive, SceneShaderStage, SceneStorage,
    SceneStringId,
};

use super::resource_storage::{
    NativeVulkanSceneResourceStoragePlan, NativeVulkanSceneShaderHeapSlice,
};
use super::shader_catalog::{
    BuiltinSceneShader, native_vulkan_scene_shader_for_key,
    native_vulkan_scene_vertex_spirv_for_primitive,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanScenePipelineCachePlan {
    pub pipeline_count: usize,
    pub entries: Vec<NativeVulkanScenePipelineCacheEntry>,
    pub shader_program_count: usize,
    pub shader_programs: Vec<NativeVulkanSceneShaderProgramSet>,
    pub shader_catalog_entry_count: usize,
    pub shader_catalog_hit_count: usize,
    pub scene_owned_shader_hit_count: usize,
    pub missing_shader_keys: Vec<String>,
    pub cache_model: &'static str,
    pub shader_catalog_source: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanScenePipelineCacheEntry {
    pub shader_key: SceneStringId,
    pub pipeline_key: SceneStringId,
    pub resource_descriptor_start: u32,
    pub resource_descriptor_count: u32,
    pub sampler_descriptor_start: u32,
    pub sampler_descriptor_count: u32,
    pub material_ids: Vec<u32>,
    pub material_pass_count: u32,
    pub render_pass_count: u32,
    pub primary_blend: ScenePipelineBlend,
    pub shader_catalog_available: bool,
    pub scene_owned_program_available: bool,
    pub shader_catalog_key: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneShaderProgramSet {
    pub shader_key: SceneStringId,
    pub shader_catalog_key: &'static str,
    pub vertex_programs: Vec<NativeVulkanSceneVertexProgram>,
    pub base_fragment_program: NativeVulkanSceneSpirvProgram,
    pub local_read_fragment_program: Option<NativeVulkanSceneSpirvProgram>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneVertexProgram {
    pub primitive: SceneRenderingDeviceDrawPrimitive,
    #[serde(flatten)]
    pub program: NativeVulkanSceneSpirvProgram,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneSpirvProgram {
    pub spirv_bytes: usize,
    pub spirv_words: &'static [u32],
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
        .filter(|entry| {
            entry.shader_catalog_available && !entry.scene_owned_program_available
        })
        .count();
    let scene_owned_shader_hit_count = entries
        .iter()
        .filter(|entry| entry.scene_owned_program_available)
        .count();
    let missing_shader_keys = unresolved_draw_shader_keys(storage, &entries);
    let shader_programs = shader_programs_for_entries(storage, &entries);

    NativeVulkanScenePipelineCachePlan {
        pipeline_count: entries.len(),
        entries,
        shader_program_count: shader_programs.len(),
        shader_programs,
        shader_catalog_entry_count: super::shader_catalog::native_vulkan_scene_shader_catalog()
            .len(),
        shader_catalog_hit_count,
        scene_owned_shader_hit_count,
        missing_shader_keys,
        cache_model: "pipeline-key-hash-cache",
        shader_catalog_source: "built-in-scene-shader-catalog",
    }
}

fn unresolved_draw_shader_keys(
    storage: &SceneStorage,
    entries: &[NativeVulkanScenePipelineCacheEntry],
) -> Vec<String> {
    entries
        .iter()
        .filter(|entry| entry.render_pass_count != 0)
        .filter(|entry| {
            !entry.shader_catalog_available && !entry.scene_owned_program_available
        })
        .filter_map(|entry| storage.string(entry.shader_key))
        .map(str::to_owned)
        .collect()
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
    let scene_owned_program_available = storage
        .shader_program(slice.shader_key, SceneShaderStage::Vertex)
        .is_some()
        && storage
            .shader_program(slice.shader_key, SceneShaderStage::Fragment)
            .is_some();

    NativeVulkanScenePipelineCacheEntry {
        shader_key: slice.shader_key,
        pipeline_key: slice.pipeline_key,
        resource_descriptor_start: slice.resource_descriptor_start,
        resource_descriptor_count: slice.resource_descriptor_count,
        sampler_descriptor_start: slice.sampler_descriptor_start,
        sampler_descriptor_count: slice.sampler_descriptor_count,
        material_ids: material_passes.iter().map(|pass| pass.material.0).collect(),
        material_pass_count: material_passes.len() as u32,
        render_pass_count,
        primary_blend,
        shader_catalog_available: shader.is_some(),
        scene_owned_program_available,
        shader_catalog_key: shader.map(|shader| shader.key),
    }
}

fn shader_programs_for_entries(
    storage: &SceneStorage,
    entries: &[NativeVulkanScenePipelineCacheEntry],
) -> Vec<NativeVulkanSceneShaderProgramSet> {
    let mut programs = Vec::new();
    for entry in entries {
        if entry.scene_owned_program_available {
            continue;
        }
        if programs.iter().any(
            |program: &NativeVulkanSceneShaderProgramSet| {
                program.shader_key == entry.shader_key
            },
        ) {
            continue;
        }
        let Some(shader) = storage
            .string(entry.shader_key)
            .and_then(native_vulkan_scene_shader_for_key)
        else {
            continue;
        };
        programs.push(shader_program_set(entry.shader_key, shader));
    }
    programs
}

fn shader_program_set(
    shader_key: SceneStringId,
    shader: &'static BuiltinSceneShader,
) -> NativeVulkanSceneShaderProgramSet {
    const PRIMITIVES: [SceneRenderingDeviceDrawPrimitive; 4] = [
        SceneRenderingDeviceDrawPrimitive::ObjectMesh,
        SceneRenderingDeviceDrawPrimitive::FullscreenTriangle,
        SceneRenderingDeviceDrawPrimitive::ObjectUvSupportQuad,
        SceneRenderingDeviceDrawPrimitive::ParticleBillboard,
    ];
    let vertex_programs = PRIMITIVES
        .into_iter()
        .filter_map(|primitive| {
            native_vulkan_scene_vertex_spirv_for_primitive(shader, primitive).map(|spirv| {
                NativeVulkanSceneVertexProgram {
                    primitive,
                    program: spirv_program(spirv),
                }
            })
        })
        .collect();
    NativeVulkanSceneShaderProgramSet {
        shader_key,
        shader_catalog_key: shader.key,
        vertex_programs,
        base_fragment_program: spirv_program(shader.fragment_spirv),
        local_read_fragment_program: shader
            .local_read_shader
            .map(|variant| spirv_program(variant.fragment_spirv)),
    }
}

fn spirv_program(spirv_words: &'static [u32]) -> NativeVulkanSceneSpirvProgram {
    NativeVulkanSceneSpirvProgram {
        spirv_bytes: spirv_words
            .len()
            .checked_mul(std::mem::size_of::<u32>())
            .expect("built-in SPIR-V byte count overflow"),
        spirv_words,
    }
}

#[cfg(test)]
mod tests {
    use super::super::resource_storage::native_vulkan_scene_resource_storage_plan;
    use super::*;
    use crate::engine::scene::{
        RenderingServer, SceneBinaryDocument, SceneMaterialPassRecord, SceneShaderContractRecord,
        SceneShaderProgramRecord, SceneStorage, SceneStringId,
    };

    #[test]
    fn pipeline_cache_keys_builtin_shader_contracts_and_pass_state() {
        let document = SceneBinaryDocument {
            strings: vec!["we/genericimage4".to_owned(), "pipeline".to_owned()],
            shader_contracts: vec![SceneShaderContractRecord {
                shader_key: SceneStringId(0),
                pipeline_key: SceneStringId(1),
                texture_slot_mask: 0,
                input_attachment_slot_mask: 0,
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
        assert_eq!(cache.shader_program_count, 1);
        assert_eq!(cache.shader_programs[0].vertex_programs.len(), 1);
        assert_eq!(
            cache.shader_programs[0].vertex_programs[0].primitive,
            SceneRenderingDeviceDrawPrimitive::ObjectMesh
        );
        assert!(cache.shader_programs[0].vertex_programs[0].program.spirv_bytes > 0);
        assert_eq!(
            cache.shader_programs[0].vertex_programs[0]
                .program
                .spirv_words[0],
            0x0723_0203
        );
        assert!(
            cache.shader_programs[0]
                .base_fragment_program
                .spirv_bytes
                > 0
        );
        assert!(
            cache.shader_programs[0]
                .local_read_fragment_program
                .is_none()
        );
        assert_eq!(cache.shader_catalog_source, "built-in-scene-shader-catalog");
    }

    #[test]
    fn shader_program_inventory_exposes_primitive_and_local_read_variants() {
        let shimmer = native_vulkan_scene_shader_for_key("effects/shimmer__SLOTS_9")
            .expect("shimmer shader");
        let shimmer_programs = shader_program_set(SceneStringId(4), shimmer);
        assert_eq!(shimmer_programs.vertex_programs.len(), 2);
        assert!(shimmer_programs.vertex_programs.iter().any(|program| {
            program.primitive == SceneRenderingDeviceDrawPrimitive::FullscreenTriangle
                && program.program.spirv_words == shimmer.vertex.spirv
        }));
        assert!(shimmer_programs.vertex_programs.iter().any(|program| {
            program.primitive == SceneRenderingDeviceDrawPrimitive::ObjectMesh
                && Some(program.program.spirv_words)
                    == shimmer.object_mesh_vertex.map(|vertex| vertex.spirv)
        }));

        let passthrough = native_vulkan_scene_shader_for_key("we/passthrough")
            .expect("passthrough shader");
        let passthrough_programs = shader_program_set(SceneStringId(9), passthrough);
        assert_eq!(
            passthrough_programs
                .local_read_fragment_program
                .expect("local-read fragment program")
                .spirv_words,
            passthrough
                .local_read_shader
                .expect("local-read shader")
                .fragment_spirv
        );
    }

    #[test]
    fn complete_scene_owned_program_is_available_without_a_catalog_hit() {
        let storage = scene_owned_pipeline_storage(
            "workshop/example/effects/custom",
            true,
        );
        let server = RenderingServer::new(&storage);
        let render_plan = server.renderer_scene_render_plan();
        let graph = server.rendering_device_graph_plan();
        let resources = native_vulkan_scene_resource_storage_plan(&storage, render_plan, &graph);

        let cache = native_vulkan_scene_pipeline_cache_plan(&storage, &resources);

        assert_eq!(cache.pipeline_count, 1);
        assert_eq!(cache.shader_catalog_hit_count, 0);
        assert_eq!(cache.scene_owned_shader_hit_count, 1);
        assert!(cache.missing_shader_keys.is_empty());
        assert!(cache.entries[0].scene_owned_program_available);
        assert!(!cache.entries[0].shader_catalog_available);
        assert_eq!(cache.entries[0].shader_catalog_key, None);
    }

    #[test]
    fn only_draw_reachable_incomplete_scene_owned_program_is_unresolved() {
        let storage = scene_owned_pipeline_storage(
            "workshop/example/effects/custom",
            false,
        );
        let server = RenderingServer::new(&storage);
        let render_plan = server.renderer_scene_render_plan();
        let graph = server.rendering_device_graph_plan();
        let resources = native_vulkan_scene_resource_storage_plan(&storage, render_plan, &graph);

        let cache = native_vulkan_scene_pipeline_cache_plan(&storage, &resources);

        assert_eq!(cache.shader_catalog_hit_count, 0);
        assert_eq!(cache.scene_owned_shader_hit_count, 0);
        assert!(cache.missing_shader_keys.is_empty());
        assert!(!cache.entries[0].scene_owned_program_available);

        let mut reachable_entries = cache.entries.clone();
        reachable_entries[0].render_pass_count = 1;
        assert_eq!(
            unresolved_draw_shader_keys(&storage, &reachable_entries),
            vec!["workshop/example/effects/custom".to_owned()]
        );
    }

    #[test]
    fn scene_owned_program_wins_without_counting_same_key_catalog_availability_as_a_hit() {
        let storage = scene_owned_pipeline_storage("we/genericimage4", true);
        let server = RenderingServer::new(&storage);
        let render_plan = server.renderer_scene_render_plan();
        let graph = server.rendering_device_graph_plan();
        let resources = native_vulkan_scene_resource_storage_plan(&storage, render_plan, &graph);

        let cache = native_vulkan_scene_pipeline_cache_plan(&storage, &resources);

        assert!(cache.entries[0].shader_catalog_available);
        assert!(cache.entries[0].scene_owned_program_available);
        assert_eq!(cache.shader_catalog_hit_count, 0);
        assert_eq!(cache.scene_owned_shader_hit_count, 1);
        assert_eq!(cache.shader_program_count, 0);
        assert!(cache.shader_programs.is_empty());
        assert!(cache.missing_shader_keys.is_empty());
    }

    fn scene_owned_pipeline_storage(key: &str, complete: bool) -> SceneStorage {
        let spirv = vec![0x0723_0203, 0x0001_0600, 0, 2, 0];
        let mut shader_programs = vec![scene_owned_program(
            SceneShaderStage::Vertex,
            spirv.len(),
        )];
        if complete {
            shader_programs.push(scene_owned_program(
                SceneShaderStage::Fragment,
                spirv.len(),
            ));
        }
        SceneStorage::from_document(SceneBinaryDocument {
            strings: vec![
                key.to_owned(),
                "pipeline".to_owned(),
                "main".to_owned(),
            ],
            shader_contracts: vec![SceneShaderContractRecord {
                shader_key: SceneStringId(0),
                pipeline_key: SceneStringId(1),
                texture_slot_mask: 0,
                input_attachment_slot_mask: 0,
                constant_start: 0,
                constant_count: 0,
                resource_heap_count: 0,
                sampler_heap_count: 0,
            }],
            shader_programs,
            shader_spirv: spirv,
            ..SceneBinaryDocument::default()
        })
        .expect("scene-owned pipeline storage")
    }

    fn scene_owned_program(
        stage: SceneShaderStage,
        spirv_count: usize,
    ) -> SceneShaderProgramRecord {
        SceneShaderProgramRecord {
            program_key: SceneStringId(0),
            stage,
            entry_point: SceneStringId(2),
            spirv_start: 0,
            spirv_count: spirv_count as u32,
            binding_start: 0,
            binding_count: 0,
            stage_io_start: 0,
            stage_io_count: 0,
            uniform_buffer_start: 0,
            uniform_buffer_count: 0,
            push_constant_bytes: 0,
        }
    }
}
