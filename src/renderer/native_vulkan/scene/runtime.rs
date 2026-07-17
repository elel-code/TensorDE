//! Native Vulkan scene present runtime boundary.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/global-uniforms.md`
//! - `references/godot/servers/rendering/renderer_scene_render.*`
//! - `references/godot/servers/rendering/rendering_device_graph.*`
//! - `src/renderer/native_vulkan/vulkan/core/descriptor_heap.rs`

use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;

use crate::engine::scene::SceneStorage;
use crate::renderer::native_vulkan::{
    NativeVulkanClearColor, NativeVulkanError, NativeVulkanOptions,
    NativeVulkanSceneFrameCaptureSnapshot,
    NativeVulkanVulkanaliaScenePresentOptions, NativeVulkanVulkanaliaScenePresentSnapshot,
    run_native_vulkan_vulkanalia_scene_present,
};

use super::{NativeVulkanSceneBackendPlan, native_vulkan_scene_backend_plan};

#[derive(Debug, Clone, PartialEq)]
pub struct NativeVulkanSceneRunOptions {
    pub capture_frame: Option<PathBuf>,
    pub capture_frame_number: u64,
    pub capture_frame_count: u64,
    pub capture_frame_step: u64,
    pub capture_frame_downscale: u32,
    pub capture_frame_region: Option<(u32, u32, u32, u32)>,
    pub capture_frame_reference: Option<PathBuf>,
    pub capture_frame_time_step_seconds: Option<f32>,
    pub capture_scene_graph: Option<u32>,
    pub clear_color_override: Option<NativeVulkanClearColor>,
    pub surface_extent: Option<(u32, u32)>,
    pub gpu_timing: bool,
}

impl Default for NativeVulkanSceneRunOptions {
    fn default() -> Self {
        Self {
            capture_frame: None,
            capture_frame_number: 1,
            capture_frame_count: 1,
            capture_frame_step: 1,
            capture_frame_downscale: 1,
            capture_frame_region: None,
            capture_frame_reference: None,
            capture_frame_time_step_seconds: None,
            capture_scene_graph: None,
            clear_color_override: None,
            surface_extent: None,
            gpu_timing: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneRuntimeSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub source: PathBuf,
    pub frame_capture: Option<NativeVulkanSceneFrameCaptureSnapshot>,
    pub capture_scene_graph: Option<u32>,
    pub backend_plan: NativeVulkanSceneBackendPlan,
    pub present: NativeVulkanVulkanaliaScenePresentSnapshot,
    pub frames_presented: u64,
    pub average_present_fps: f64,
    pub present_delta_min_micros: Option<u64>,
    pub present_delta_max_micros: Option<u64>,
    pub present_delta_over_6250us_count: u64,
    pub present_delta_over_8334us_count: u64,
    pub descriptor_model: &'static str,
    pub present_mode: &'static str,
    pub render_graph_draw_count: usize,
    pub mesh_draw_count: usize,
    pub mesh_draw_recording_ready: bool,
    pub mesh_draw_recorded_this_run: bool,
    pub runtime_status: &'static str,
}

pub fn run_scene(
    options: NativeVulkanOptions,
    duration: Duration,
    source: PathBuf,
) -> Result<NativeVulkanSceneRuntimeSnapshot, NativeVulkanError> {
    run_scene_with_options(
        options,
        duration,
        source,
        NativeVulkanSceneRunOptions::default(),
    )
}

pub fn run_scene_with_options(
    options: NativeVulkanOptions,
    duration: Duration,
    source: PathBuf,
    scene_options: NativeVulkanSceneRunOptions,
) -> Result<NativeVulkanSceneRuntimeSnapshot, NativeVulkanError> {
    if scene_options.capture_frame.is_some() && duration.is_zero() {
        return Err(NativeVulkanError::Scene(
            "scene frame capture requires a non-zero runtime duration".to_owned(),
        ));
    }
    if scene_options.capture_frame.is_some() && scene_options.capture_frame_number == 0 {
        return Err(NativeVulkanError::Scene(
            "scene frame capture number must be at least 1".to_owned(),
        ));
    }
    if scene_options.capture_frame.is_some() && scene_options.capture_frame_count == 0 {
        return Err(NativeVulkanError::Scene(
            "scene frame capture count must be at least 1".to_owned(),
        ));
    }
    if scene_options.capture_frame.is_some() && scene_options.capture_frame_step == 0 {
        return Err(NativeVulkanError::Scene(
            "scene frame capture step must be at least 1".to_owned(),
        ));
    }
    if scene_options.capture_frame.is_some() && scene_options.capture_frame_downscale == 0 {
        return Err(NativeVulkanError::Scene(
            "scene frame capture downscale must be at least 1".to_owned(),
        ));
    }
    if scene_options.capture_frame.is_none() && scene_options.capture_frame_region.is_some() {
        return Err(NativeVulkanError::Scene(
            "scene frame capture region requires frame capture".to_owned(),
        ));
    }
    if scene_options.capture_frame.is_none() && scene_options.capture_frame_reference.is_some() {
        return Err(NativeVulkanError::Scene(
            "scene temporal reference requires frame capture".to_owned(),
        ));
    }
    if scene_options.capture_frame_reference.is_some() && scene_options.capture_frame_count < 3 {
        return Err(NativeVulkanError::Scene(
            "scene temporal reference requires at least three captured frames".to_owned(),
        ));
    }
    if scene_options.capture_frame_reference.is_some()
        && scene_options.capture_frame_time_step_seconds.is_none()
    {
        return Err(NativeVulkanError::Scene(
            "scene temporal reference requires a deterministic capture time step".to_owned(),
        ));
    }
    if scene_options.capture_frame.is_none()
        && scene_options.capture_frame_time_step_seconds.is_some()
    {
        return Err(NativeVulkanError::Scene(
            "scene deterministic capture time step requires frame capture".to_owned(),
        ));
    }
    if scene_options
        .capture_frame_time_step_seconds
        .is_some_and(|step| !step.is_finite() || step <= 0.0)
    {
        return Err(NativeVulkanError::Scene(
            "scene deterministic capture time step must be finite and positive".to_owned(),
        ));
    }
    if scene_options.capture_frame.is_none() && scene_options.capture_scene_graph.is_some() {
        return Err(NativeVulkanError::Scene(
            "scene graph isolation requires frame capture".to_owned(),
        ));
    }
    let file = std::fs::File::open(&source).map_err(|err| {
        NativeVulkanError::Scene(format!(
            "open scene engine binary {}: {err}",
            source.display()
        ))
    })?;
    let storage = SceneStorage::from_binary_reader(file).map_err(|err| {
        NativeVulkanError::Scene(format!(
            "load scene engine binary {} into scene runtime storage: {err}",
            source.display()
        ))
    })?;
    let backend_plan = native_vulkan_scene_backend_plan(&storage);
    validate_scene_runtime_plan(&backend_plan)?;
    let clear_color = scene_options
        .clear_color_override
        .unwrap_or_else(|| scene_clear_color(&storage));
    let capture_scene_graph = scene_options.capture_scene_graph;

    let present =
        run_native_vulkan_vulkanalia_scene_present(NativeVulkanVulkanaliaScenePresentOptions {
            host: options.host,
            wait_configure_roundtrips: options.wait_configure_roundtrips,
            duration,
            target_max_fps: options.target_max_fps,
            clear_color,
            storage,
            capture_frame: scene_options.capture_frame,
            capture_frame_number: scene_options.capture_frame_number,
            capture_frame_count: scene_options.capture_frame_count,
            capture_frame_step: scene_options.capture_frame_step,
            capture_frame_downscale: scene_options.capture_frame_downscale,
            capture_frame_region: scene_options.capture_frame_region,
            capture_frame_reference: scene_options.capture_frame_reference,
            capture_frame_time_step_seconds: scene_options.capture_frame_time_step_seconds,
            capture_scene_graph,
            surface_extent: scene_options.surface_extent,
            gpu_timing: scene_options.gpu_timing,
        })
        .map_err(NativeVulkanError::Scene)?;
    let frame_capture = present.frame_capture.clone();
    let mesh_draw_recording_ready = backend_plan.render_graph_executor.draw_count > 0
        && backend_plan.pipeline_cache.shader_catalog_hit_count
            == backend_plan.pipeline_cache.pipeline_count
        && backend_plan
            .resource_storage
            .mesh_buffer
            .device_address_required;

    Ok(NativeVulkanSceneRuntimeSnapshot {
        binding: "vulkanalia",
        route: "scene-engine-vulkan-present-runtime",
        source,
        frame_capture,
        capture_scene_graph,
        frames_presented: present.frames_presented,
        average_present_fps: present.average_present_fps,
        present_delta_min_micros: present.present_delta_min_micros,
        present_delta_max_micros: present.present_delta_max_micros,
        present_delta_over_6250us_count: present.present_delta_over_6250us_count,
        present_delta_over_8334us_count: present.present_delta_over_8334us_count,
        descriptor_model: "VK_EXT_descriptor_heap",
        present_mode: present.swapchain.present_mode,
        render_graph_draw_count: backend_plan.render_graph_executor.draw_count,
        mesh_draw_count: backend_plan.rendering_device_graph.mesh_draws.len(),
        mesh_draw_recording_ready,
        mesh_draw_recorded_this_run: present.mesh_draw_recorded,
        runtime_status: if mesh_draw_recording_ready {
            "scene-engine-vulkan-mesh-draw-recorded"
        } else {
            "scene-engine-present-loop-ready-without-mesh-draws"
        },
        backend_plan,
        present,
    })
}

fn validate_scene_runtime_plan(
    backend_plan: &NativeVulkanSceneBackendPlan,
) -> Result<(), NativeVulkanError> {
    if backend_plan.present_mode != "fifo-latest-ready" {
        return Err(NativeVulkanError::Scene(
            "scene runtime requires FIFO latest ready present".to_owned(),
        ));
    }
    if backend_plan.pipeline_cache.shader_catalog_hit_count
        != backend_plan.pipeline_cache.pipeline_count
    {
        return Err(NativeVulkanError::Scene(format!(
            "scene runtime missing built-in shader catalog entries: {}",
            backend_plan.pipeline_cache.missing_shader_keys.join(", ")
        )));
    }
    if backend_plan.rendering_device_graph.descriptor_heap_required
        && backend_plan
            .resource_storage
            .descriptor_heap
            .descriptor_model
            != "VK_EXT_descriptor_heap"
    {
        return Err(NativeVulkanError::Scene(
            "scene runtime requires VK_EXT_descriptor_heap resource binding".to_owned(),
        ));
    }
    Ok(())
}

fn scene_clear_color(storage: &SceneStorage) -> NativeVulkanClearColor {
    let [r, g, b, a] = storage.project().clear_color;
    NativeVulkanClearColor { r, g, b, a }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::{RendererSceneRenderPlan, SceneRenderingDeviceGraphPlan};
    use crate::renderer::native_vulkan::scene::{
        NativeVulkanSceneDescriptorHeapPlan, NativeVulkanSceneMeshUploadPlan,
    };
    use super::super::resource_storage::{
        NativeVulkanSceneEffectTargetStoragePlan, NativeVulkanSceneSkinningBufferPlan,
    };
    use crate::renderer::native_vulkan::scene::{
        NativeVulkanSceneHeapStoragePlan, NativeVulkanSceneMeshBufferPlan,
        NativeVulkanScenePipelineCachePlan, NativeVulkanSceneRenderGraphExecutorPlan,
        NativeVulkanSceneResourceStoragePlan,
    };

    #[test]
    fn scene_runtime_plan_rejects_missing_builtin_shader_catalog_entries() {
        let mut plan = empty_backend_plan();
        plan.pipeline_cache.pipeline_count = 1;
        plan.pipeline_cache.missing_shader_keys = vec!["not-built-in".to_owned()];

        let err = validate_scene_runtime_plan(&plan).unwrap_err();

        assert!(err.to_string().contains("missing built-in shader catalog"));
    }

    fn empty_backend_plan() -> NativeVulkanSceneBackendPlan {
        NativeVulkanSceneBackendPlan {
            renderer_scene_render: RendererSceneRenderPlan {
                object_count: 0,
                visible_object_count: 0,
                resource_count: 0,
                texture_count: 0,
                material_count: 0,
                mesh_count: 0,
                visible_mesh_binding_count: 0,
                mesh_vertex_count: 0,
                mesh_index_count: 0,
                puppet_binding_count: 0,
                visible_puppet_binding_count: 0,
                puppet_bone_palette_count: 0,
                puppet_bone_matrix_count: 0,
                visible_puppet_bone_matrix_count: 0,
                attachment_link_count: 0,
                effect_count: 0,
                visible_effect_instance_count: 0,
                visible_effect_pass_count: 0,
                visible_effect_fbo_count: 0,
                render_graph_count: 0,
                render_pass_count: 0,
                render_binding_count: 0,
                image_target_count: 0,
                shader_contract_count: 0,
                resource_payload_bytes: 0,
                descriptor_heap_required: true,
                descriptor_heap_resource_count: 0,
                descriptor_heap_sampled_image_count: 0,
                descriptor_heap_uniform_buffer_count: 0,
                descriptor_heap_storage_buffer_count: 0,
                descriptor_heap_sampler_count: 0,
                fifo_latest_ready_present_required: true,
            },
            rendering_device_graph: SceneRenderingDeviceGraphPlan {
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
            },
            resource_storage: NativeVulkanSceneResourceStoragePlan {
                resource_record_count: 0,
                texture_record_count: 0,
                material_record_count: 0,
                effect_record_count: 0,
                resource_payload_bytes: 0,
                mesh_buffer: NativeVulkanSceneMeshBufferPlan {
                    mesh_count: 0,
                    vertex_count: 0,
                    index_count: 0,
                    vertex_buffer_bytes: 0,
                    index_buffer_bytes: 0,
                    draw_count: 0,
                    device_address_required: false,
                },
                skinning_buffer: NativeVulkanSceneSkinningBufferPlan {
                    palette_count: 0,
                    bone_matrix_count: 0,
                    bone_matrix_buffer_bytes: 0,
                    storage_buffer_required: false,
                },
                effect_target_storage: NativeVulkanSceneEffectTargetStoragePlan {
                    logical_target_count: 0,
                    physical_target_count: 0,
                    aliased_target_count: 0,
                    named_fbo_count: 0,
                    first_class_effect_target_count: 0,
                    dynamic_rendering_image_required: false,
                },
                descriptor_heap: NativeVulkanSceneHeapStoragePlan {
                    descriptor_model: "VK_EXT_descriptor_heap",
                    resource_descriptor_count: 0,
                    sampled_image_descriptor_count: 0,
                    uniform_buffer_descriptor_count: 0,
                    storage_buffer_descriptor_count: 0,
                    sampler_descriptor_count: 0,
                    shader_contract_count: 0,
                },
                shader_heap_slices: Vec::new(),
                payload_residency: "scene-resource-payload-offset-slices",
                mesh_residency: "device-addressable-scene-mesh-buffers",
            },
            pipeline_cache: NativeVulkanScenePipelineCachePlan {
                pipeline_count: 0,
                entries: Vec::new(),
                shader_catalog_entry_count: 0,
                shader_catalog_hit_count: 0,
                missing_shader_keys: Vec::new(),
                cache_model: "pipeline-key-hash-cache",
                shader_catalog_source: "built-in-scene-shader-catalog",
            },
            render_graph_executor: NativeVulkanSceneRenderGraphExecutorPlan {
                command_count: 0,
                draw_count: 0,
                commands: Vec::new(),
                heap_binding_model: "VK_EXT_descriptor_heap",
                present_mode: "fifo-latest-ready",
                executor_status: "scene-render-graph-empty",
            },
            descriptor_heap: NativeVulkanSceneDescriptorHeapPlan {
                resource_descriptor_count: 0,
                sampled_image_descriptor_count: 0,
                uniform_buffer_descriptor_count: 0,
                storage_buffer_descriptor_count: 0,
                sampler_descriptor_count: 0,
                shader_contract_count: 0,
            },
            mesh_upload: NativeVulkanSceneMeshUploadPlan {
                mesh_count: 0,
                vertex_count: 0,
                index_count: 0,
                vertex_buffer_bytes: 0,
                index_buffer_bytes: 0,
                device_address_required: false,
            },
            particle_systems: Vec::new(),
            present_mode: "fifo-latest-ready",
            legacy_binding_forbidden: true,
        }
    }
}
