//! Native Vulkan scene backend planning.
//!
//! References:
//! - `docs/gilder/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/gilder/docs/exe/blend-and-render.md`
//! - `reverse-engineered/gilder/docs/exe/global-uniforms.md`
//! - `references/gilder/godot/servers/rendering/renderer_scene_render.*`
//! - `references/gilder/godot/servers/rendering/rendering_device.*`
//! - `references/gilder/godot/drivers/vulkan/rendering_device_driver_vulkan.*`
//! - `src/renderer/native_vulkan/vulkan/core/descriptor_heap.rs`

use serde::Serialize;

use crate::engine::scene::{
    RendererSceneRenderPlan, RenderingServer, ResolvedSemanticFrame, SceneParticleSystemRecord,
    SceneRenderingDeviceGraphPlan, SceneStorage,
};
use crate::renderer::native_vulkan::present::render_item::NativeVulkanRenderItem;

use super::pipeline_cache::{
    NativeVulkanScenePipelineCachePlan, native_vulkan_scene_pipeline_cache_plan,
};
use super::render_graph_executor::{
    NativeVulkanSceneRenderGraphExecutorPlan, native_vulkan_scene_render_graph_executor_plan,
};
use super::resource_storage::{
    NativeVulkanSceneResourceStoragePlan, native_vulkan_scene_resource_storage_plan,
};

const SCENE_MESH_VERTEX_UPLOAD_STRIDE_BYTES: usize = 52;
const SCENE_MESH_INDEX_UPLOAD_STRIDE_BYTES: usize = 4;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneBackendPlan {
    pub renderer_scene_render: RendererSceneRenderPlan,
    pub rendering_device_graph: SceneRenderingDeviceGraphPlan,
    pub resource_storage: NativeVulkanSceneResourceStoragePlan,
    pub pipeline_cache: NativeVulkanScenePipelineCachePlan,
    pub render_graph_executor: NativeVulkanSceneRenderGraphExecutorPlan,
    pub descriptor_heap: NativeVulkanSceneDescriptorHeapPlan,
    pub mesh_upload: NativeVulkanSceneMeshUploadPlan,
    pub particle_systems: Vec<SceneParticleSystemRecord>,
    pub present_mode: &'static str,
    pub descriptor_heap_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneDescriptorHeapPlan {
    pub resource_descriptor_count: u32,
    pub sampled_image_descriptor_count: u32,
    pub input_attachment_descriptor_count: u32,
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
    native_vulkan_scene_backend_plan_at(storage, 0.0)
}

pub fn native_vulkan_scene_backend_plan_at(
    storage: &SceneStorage,
    scene_time_seconds: f32,
) -> NativeVulkanSceneBackendPlan {
    let rendering_server = RenderingServer::new(storage);
    let scene_engine = rendering_server.scene_engine_render_plan_at(scene_time_seconds);
    build_scene_backend_plan(
        storage,
        scene_engine.renderer_scene_render,
        scene_engine.rendering_device_graph,
    )
}

pub fn native_vulkan_scene_backend_plan_from_semantic_frame(
    storage: &SceneStorage,
    semantic_frame: &ResolvedSemanticFrame,
) -> NativeVulkanSceneBackendPlan {
    let rendering_server = RenderingServer::new(storage);
    let renderer_scene_render =
        rendering_server.renderer_scene_render_plan_from_semantic_frame(semantic_frame);
    let rendering_device_graph = SceneRenderingDeviceGraphPlan::from_storage_with_semantic_frame(
        storage,
        renderer_scene_render,
        semantic_frame,
    );
    build_scene_backend_plan(storage, renderer_scene_render, rendering_device_graph)
}

fn build_scene_backend_plan(
    storage: &SceneStorage,
    renderer_scene_render: RendererSceneRenderPlan,
    rendering_device_graph: SceneRenderingDeviceGraphPlan,
) -> NativeVulkanSceneBackendPlan {
    let utility_vertex_count =
        usize::from(rendering_device_graph.uses_fullscreen_utility_primitive()) * 3;
    let mesh_upload_vertex_count = renderer_scene_render
        .mesh_vertex_count
        .saturating_add(utility_vertex_count);
    let mesh_upload_index_count = renderer_scene_render
        .mesh_index_count
        .saturating_add(utility_vertex_count);
    let resource_storage = native_vulkan_scene_resource_storage_plan(
        storage,
        renderer_scene_render,
        &rendering_device_graph,
    );
    let pipeline_cache = native_vulkan_scene_pipeline_cache_plan(storage, &resource_storage);
    let render_graph_executor = native_vulkan_scene_render_graph_executor_plan(
        &rendering_device_graph,
        &resource_storage,
        &pipeline_cache,
    );

    NativeVulkanSceneBackendPlan {
        renderer_scene_render,
        rendering_device_graph,
        resource_storage,
        pipeline_cache,
        render_graph_executor,
        descriptor_heap: NativeVulkanSceneDescriptorHeapPlan {
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
        mesh_upload: NativeVulkanSceneMeshUploadPlan {
            mesh_count: renderer_scene_render.mesh_count,
            vertex_count: mesh_upload_vertex_count,
            index_count: mesh_upload_index_count,
            vertex_buffer_bytes: mesh_upload_vertex_count
                .saturating_mul(SCENE_MESH_VERTEX_UPLOAD_STRIDE_BYTES),
            index_buffer_bytes: mesh_upload_index_count
                .saturating_mul(SCENE_MESH_INDEX_UPLOAD_STRIDE_BYTES),
            device_address_required: renderer_scene_render.mesh_count > 0
                || utility_vertex_count != 0,
        },
        particle_systems: storage.particles().to_vec(),
        present_mode: "fifo-latest-ready",
        descriptor_heap_only: true,
    }
}

pub fn native_vulkan_scene_backend_plan_from_render_item(
    item: &NativeVulkanRenderItem,
) -> Result<Option<NativeVulkanSceneBackendPlan>, String> {
    let NativeVulkanRenderItem::Scene {
        scene_source: Some(scene_source),
        ..
    } = item
    else {
        return Ok(None);
    };
    if !scene_source
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gscene"))
    {
        return Ok(None);
    }

    let file = std::fs::File::open(scene_source)
        .map_err(|err| format!("open scene engine binary {}: {err}", scene_source.display()))?;
    let storage = SceneStorage::from_binary_reader(file).map_err(|err| {
        format!(
            "load scene engine binary {} into native Vulkan scene storage: {err}",
            scene_source.display()
        )
    })?;
    Ok(Some(native_vulkan_scene_backend_plan(&storage)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{FitMode, SceneSystems};
    use crate::engine::scene::{
        INVALID_MATERIAL_ID, INVALID_OBJECT_ID, SCENE_DEFAULT_FEATURE_FLAGS, SceneBinaryDocument,
        SceneCullMode, SceneDepthTest, SceneMaterialHandle, SceneMaterialRecord, SceneMeshRecord,
        SceneMeshVertexRecord, SceneObjectHandle, SceneObjectKind, SceneObjectRecord,
        ScenePipelineBlend, SceneProjectRecord, SceneRenderGraphActivationPolicy,
        SceneRenderGraphRecord, SceneRenderPassKind, SceneRenderPassRecord, SceneRenderTargetKind,
        SceneResourceId, SceneResourceKind, SceneResourceRecord, SceneShaderContractRecord,
        SceneStorage, SceneStringId, SceneVec3, write_scene_binary,
    };

    #[test]
    fn scene_backend_plan_requires_descriptor_heap_and_fifo_latest_ready() {
        let document = SceneBinaryDocument {
            strings: vec!["shader".to_owned(), "pipeline".to_owned()],
            shader_contracts: vec![SceneShaderContractRecord {
                shader_key: SceneStringId(0),
                pipeline_key: SceneStringId(1),
                texture_slot_mask: 0b101,
                input_attachment_slot_mask: 0b010,
                constant_start: 0,
                constant_count: 0,
                resource_heap_count: 4,
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
                color: SceneVec3 {
                    x: 1.0,
                    y: 1.0,
                    z: 1.0,
                },
                alpha: 1.0,
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
                    blend_indices: [0; 4],
                    blend_weights: [0.0; 4],
                },
                SceneMeshVertexRecord {
                    position: SceneVec3 {
                        x: 32.0,
                        y: -16.0,
                        z: 0.0,
                    },
                    uv: [1.0, 1.0],
                    blend_indices: [0; 4],
                    blend_weights: [0.0; 4],
                },
                SceneMeshVertexRecord {
                    position: SceneVec3 {
                        x: 32.0,
                        y: 16.0,
                        z: 0.0,
                    },
                    uv: [1.0, 0.0],
                    blend_indices: [0; 4],
                    blend_weights: [0.0; 4],
                },
                SceneMeshVertexRecord {
                    position: SceneVec3 {
                        x: -32.0,
                        y: 16.0,
                        z: 0.0,
                    },
                    uv: [0.0, 0.0],
                    blend_indices: [0; 4],
                    blend_weights: [0.0; 4],
                },
            ],
            mesh_indices: vec![0, 1, 2, 0, 2, 3],
            ..SceneBinaryDocument::default()
        };
        let storage = SceneStorage::from_document(document).expect("storage");
        let plan = native_vulkan_scene_backend_plan(&storage);

        assert!(plan.descriptor_heap_only);
        assert_eq!(plan.present_mode, "fifo-latest-ready");
        assert_eq!(plan.descriptor_heap.resource_descriptor_count, 4);
        assert_eq!(plan.descriptor_heap.sampled_image_descriptor_count, 2);
        assert_eq!(plan.descriptor_heap.input_attachment_descriptor_count, 1);
        assert_eq!(plan.descriptor_heap.uniform_buffer_descriptor_count, 1);
        assert_eq!(plan.descriptor_heap.storage_buffer_descriptor_count, 0);
        assert_eq!(plan.descriptor_heap.sampler_descriptor_count, 2);
        assert_eq!(plan.rendering_device_graph.pass_nodes.len(), 0);
        assert_eq!(plan.rendering_device_graph.mesh_draws.len(), 0);
        assert_eq!(
            plan.resource_storage.descriptor_heap.descriptor_model,
            "VK_EXT_descriptor_heap"
        );
        assert_eq!(
            plan.resource_storage
                .descriptor_heap
                .input_attachment_descriptor_count,
            1
        );
        assert_eq!(
            plan.resource_storage.shader_heap_slices[0]
                .input_attachment_descriptor_count,
            1
        );
        assert_eq!(plan.pipeline_cache.pipeline_count, 1);
        assert_eq!(
            plan.render_graph_executor.executor_status,
            "scene-render-graph-ready-without-mesh-draws"
        );
        assert_eq!(plan.mesh_upload.mesh_count, 1);
        assert_eq!(plan.mesh_upload.vertex_count, 4);
        assert_eq!(plan.mesh_upload.index_count, 6);
        assert_eq!(plan.mesh_upload.vertex_buffer_bytes, 208);
        assert_eq!(plan.mesh_upload.index_buffer_bytes, 24);
        assert!(plan.mesh_upload.device_address_required);
    }

    #[test]
    fn scene_backend_plan_loads_gscene_from_native_render_item() {
        let root = std::env::temp_dir().join(format!(
            "gilder-native-scene-item-plan-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("tmp");
        let scene_path = root.join("scene.gscene");
        let document = minimal_scene_backend_document();
        let mut bytes = Vec::new();
        write_scene_binary(&document, &mut bytes).expect("write scene binary");
        std::fs::write(&scene_path, bytes).expect("scene file");
        let item = NativeVulkanRenderItem::Scene {
            output_name: "HDMI-A-1".to_owned(),
            scene_source: Some(scene_path.clone()),
            display: None,
            display_image: None,
            display_color: None,
            manifest_max_fps: None,
            layer_count: 0,
            layers: Vec::new(),
            scene_systems: SceneSystems::default(),
            audio_cue_count: 0,
            bound_properties: Vec::new(),
            timeline_animation_count: 0,
            timeline_animated_layer_count: 0,
            puppet_animation_layer_count: 0,
            property_binding_count: 0,
            cursor_parallax_input_ready: false,
            dynamic_topology_required: true,
            scene_engine: None,
            scene_scenescript_binding_count: 0,
            scene_material_graph_count: 1,
            scene_material_graph_resource_count: 1,
            scene_effect_graph_count: 1,
            scene_mesh_count: 1,
            scene_mesh_vertex_count: 4,
            scene_mesh_index_count: 6,
            scene_audio_response_binding_count: 0,
            unsupported_scene_features: Vec::new(),
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            target_max_fps: None,
            renderer_status: "scene-engine-binary-ready-for-rendering-device-graph",
        };

        let plan = native_vulkan_scene_backend_plan_from_render_item(&item)
            .expect("backend plan")
            .expect("scene item plan");

        assert_eq!(plan.renderer_scene_render.mesh_count, 1);
        assert_eq!(plan.rendering_device_graph.mesh_draws.len(), 1);
        assert_eq!(plan.render_graph_executor.draw_count, 1);
        assert_eq!(plan.present_mode, "fifo-latest-ready");

        let _ = std::fs::remove_dir_all(root);
    }

    fn minimal_scene_backend_document() -> SceneBinaryDocument {
        SceneBinaryDocument {
            feature_flags: SCENE_DEFAULT_FEATURE_FLAGS,
            strings: vec![
                "Scene Demo".to_owned(),
                "scene".to_owned(),
                "scene.json".to_owned(),
                "materials/layer.json".to_owned(),
                "we/genericimage4".to_owned(),
                "we/genericimage4|blend=normal".to_owned(),
                "loose-file".to_owned(),
            ],
            project: SceneProjectRecord {
                title: SceneStringId(0),
                wallpaper_type: SceneStringId(1),
                scene_file: SceneStringId(2),
                preview: SceneStringId::NONE,
                properties_json: SceneStringId::NONE,
                logical_width: 1920,
                logical_height: 1080,
                clear_color: [0.0, 0.0, 0.0, 1.0],
                ambient_color: [0.3, 0.3, 0.3, 1.0],
                skylight_color: [0.3, 0.3, 0.3, 1.0],
                camera_eye: SceneVec3::default(),
                camera_center: SceneVec3 {
                    x: 0.0,
                    y: 0.0,
                    z: -1.0,
                },
                camera_up: SceneVec3 {
                    x: 0.0,
                    y: 1.0,
                    z: 0.0,
                },
            },
            resources: vec![SceneResourceRecord {
                id: SceneResourceId(0),
                kind: SceneResourceKind::MaterialJson,
                path: SceneStringId(3),
                source: SceneStringId(6),
                payload_offset: 0,
                payload_len: 2,
            }],
            resource_payload: b"{}".to_vec(),
            objects: vec![SceneObjectRecord {
                id: SceneObjectHandle(0),
                we_id: 7,
                name: SceneStringId::NONE,
                kind: SceneObjectKind::Image,
                resource: SceneResourceId::NONE,
                material: SceneMaterialHandle(0),
                parent_we_id: INVALID_OBJECT_ID,
                attachment: SceneStringId::NONE,
                origin: SceneVec3::default(),
                angles: SceneVec3::default(),
                scale: SceneVec3 {
                    x: 1.0,
                    y: 1.0,
                    z: 1.0,
                },
                color: SceneVec3 {
                    x: 1.0,
                    y: 1.0,
                    z: 1.0,
                },
                alpha: 1.0,
                visible: true,
                color_blend_mode: 0,
                sort_order: 0,
                effect_start: u32::MAX,
                effect_count: 0,
                render_graph: 0,
            }],
            materials: vec![SceneMaterialRecord {
                id: SceneMaterialHandle(0),
                resource: SceneResourceId(0),
                pass_start: 0,
                pass_count: 0,
            }],
            meshes: vec![SceneMeshRecord {
                object: SceneObjectHandle(0),
                material: SceneMaterialHandle(0),
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
                    blend_indices: [0; 4],
                    blend_weights: [0.0; 4],
                };
                4
            ],
            mesh_indices: vec![0, 1, 2, 0, 2, 3],
            render_graphs: vec![SceneRenderGraphRecord {
                object: SceneObjectHandle(0),
                activation_policy: SceneRenderGraphActivationPolicy::Always,
                pass_start: 0,
                pass_count: 1,
                unsupported_start: 0,
                unsupported_count: 0,
            }],
            render_passes: vec![SceneRenderPassRecord {
                id: 0,
                role: SceneRenderPassKind::BaseMaterial,
                draw_primitive: crate::engine::scene::SceneRenderPassDrawPrimitive::ObjectMesh,
                object: SceneObjectHandle(0),
                material: SceneMaterialHandle(INVALID_MATERIAL_ID),
                pass_index: 0,
                shader_key: SceneStringId(4),
                target: SceneRenderTargetKind::SceneColor,
                target_name: SceneStringId::NONE,
                binding_start: 0,
                binding_count: 0,
                effect_binding_start: u32::MAX,
                effect_binding_count: 0,
                effect_visibility_policy:
                    crate::engine::scene::SceneRenderEffectVisibilityPolicy::None,
                pipeline_blend: ScenePipelineBlend::Normal,
                scene_blend: crate::engine::scene::SceneCompositeBlend::Alpha,
                depth_test: SceneDepthTest::Disabled,
                depth_write: false,
                cull_mode: SceneCullMode::None,
                color_write_mask: crate::engine::scene::SceneColorWriteMask::Rgba,
                clear_target: false,
            }],
            shader_contracts: vec![SceneShaderContractRecord {
                shader_key: SceneStringId(4),
                pipeline_key: SceneStringId(5),
                texture_slot_mask: 0,
                input_attachment_slot_mask: 0,
                constant_start: 0,
                constant_count: 0,
                resource_heap_count: 1,
                sampler_heap_count: 0,
            }],
            ..SceneBinaryDocument::default()
        }
    }
}
