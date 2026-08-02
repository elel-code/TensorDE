//! Gilder scene runtime over the shared `vulkan-renderer` ownership root.

use std::time::Duration;

use serde::Serialize;
use serde_json::{Map, Value};

use crate::engine::scene::semantic_world::SemanticFrameResolver;
use crate::engine::scene::{SceneScriptTarget, SceneStorage};
use crate::renderer::native_vulkan::NativeVulkanClearColor;
use crate::renderer::native_wayland::{
    NativeWaylandHostOptions, NativeWaylandSurfaceSnapshot,
};

mod command_order;
mod composite_scissor;
mod descriptor_layout;
mod descriptor_plan;
mod draw_recording;
mod draw_uniform;
mod dynamic_text;
mod effect_target;
mod flat_rounded_mask_coverage;
mod frame_events;
mod frame_state;
mod fullscreen_primitive;
mod graph_execution;
mod input_attachment_binding;
mod local_read;
mod material_uniform;
mod mesh_payload;
mod native_descriptor_push;
mod pipeline;
mod sampled_binding;
mod scene_color_clear;
mod scene_owned_uniform;
mod scene_viewport;
mod semantic_diagnostics;
mod shader_descriptor_push;
mod shader_program;
mod shader_uniform;
mod shared_present_loop;
mod shared_resources;
mod shared_scene;
mod scene_terminal_program;

pub use scene_owned_uniform::{
    NativeVulkanSceneOwnedUniformArenaPlanSnapshot, NativeVulkanSceneOwnedUniformSliceSnapshot,
    native_vulkan_scene_owned_uniform_arena_plan,
};
pub use semantic_diagnostics::NativeVulkanSceneSemanticDiagnosticsSnapshot;
use semantic_diagnostics::scene_semantic_diagnostics_snapshot;

const SCENE_MESH_VERTEX_STRIDE_BYTES: u32 = 52;
const SCENE_VIDEO_VERTEX_STRIDE_BYTES: u32 = 20;
const SCENE_WHITE_TEXTURE_BYTES: &[u8] = &[255, 255, 255, 255];

use descriptor_layout::{ScenePipelineDescriptorLayout, scene_pipeline_descriptor_layout};
use draw_recording::SceneGpuDrawCommand;
use draw_uniform::SCENE_DRAW_UNIFORM_BYTES;
use material_uniform::{SCENE_MATERIAL_UNIFORM_BYTES, draw_parameter_layout};
use sampled_binding::scene_sampled_image_binding_cycle;

const NATIVE_SLANG_CONSTANT_BUFFER_DESCRIPTOR_KIND: vulkan_renderer::DescriptorSlotKind =
    vulkan_renderer::DescriptorSlotKind::UniformBuffer;

#[derive(Debug, Clone, PartialEq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanScenePresentOptions {
    pub host: NativeWaylandHostOptions,
    pub wait_configure_roundtrips: usize,
    pub duration: Option<Duration>,
    pub target_max_fps: Option<u32>,
    pub clear_color: NativeVulkanClearColor,
    pub storage: SceneStorage,
    pub user_property_overrides: Map<String, Value>,
    pub surface_extent: Option<(u32, u32)>,
    pub gpu_timing: bool,
    pub semantic_diagnostics: bool,
    pub pointer_replay_normalized: Option<[f64; 2]>,
    pub video_sources:
        Vec<crate::renderer::native_vulkan::scene::NativeVulkanSceneVideoSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneQueueSnapshot {
    pub physical_device_index: usize,
    pub physical_device_name: String,
    pub physical_device_type: String,
    pub queue_family_index: u32,
    pub queue_count: u32,
    pub supports_graphics: bool,
    pub supports_present: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneDeviceSnapshot {
    pub api_version: String,
    pub roadmap_2026_ready: bool,
    pub available_device_extensions: Vec<String>,
    pub synchronization2_enabled: bool,
    pub dynamic_rendering_enabled: bool,
    pub descriptor_heap_enabled: bool,
    pub pipeline_binaries_enabled: bool,
    pub fifo_latest_ready_enabled: bool,
    pub advanced_blend_enabled: bool,
    pub advanced_blend_coherent: bool,
    pub multisampled_render_to_single_sampled_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneSwapchainSnapshot {
    pub created: bool,
    pub format: String,
    pub color_space: String,
    pub present_mode: &'static str,
    pub extent: (u32, u32),
    pub image_count: usize,
    pub min_image_count: u32,
    pub composite_alpha: &'static str,
    pub image_usage: Vec<&'static str>,
    pub present_id2_enabled: bool,
    pub present_wait2_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneScriptEffectVisibilitySnapshot {
    pub object_handle: u32,
    pub object_id: u32,
    pub object_name: String,
    pub binding_index: u32,
    pub effect_index: u32,
    pub effect_handle: u32,
    pub effect_name: String,
    pub authored_visible: bool,
    pub final_self_visible: bool,
    pub final_resolved_visible: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanScenePresentSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub loader: String,
    pub requested_api_version: String,
    pub runtime_elapsed_ms: u64,
    pub frames_presented: u64,
    pub average_present_fps: f64,
    pub present_delta_min_micros: Option<u64>,
    pub present_delta_max_micros: Option<u64>,
    pub present_delta_over_6250us_count: u64,
    pub present_delta_over_8334us_count: u64,
    pub clear_color: NativeVulkanClearColor,
    /// Final Wayland configure/fractional-scale facts for this presentation
    /// run. `buffer_size` is the host-side physical pixel request that must
    /// agree with `swapchain.extent` on Gilder's exact-extent path.
    pub wayland_surface: NativeWaylandSurfaceSnapshot,
    pub selected_queue: NativeVulkanSceneQueueSnapshot,
    pub device: NativeVulkanSceneDeviceSnapshot,
    pub swapchain: NativeVulkanSceneSwapchainSnapshot,
    pub command_submit_model: &'static str,
    pub uses_synchronization2: bool,
    pub uses_submit2: bool,
    pub uses_dynamic_rendering: bool,
    pub scene_color_rasterization_samples: &'static str,
    pub uses_multisampled_render_to_single_sampled: bool,
    pub uses_explicit_scene_color_msaa_resolve: bool,
    pub scene_color_msaa_memory_bytes: u64,
    pub scene_color_target_model: &'static str,
    pub scene_color_offscreen_image_count: usize,
    pub scene_color_offscreen_memory_bytes: u64,
    pub scene_color_offscreen_extent: (u32, u32),
    pub scene_color_distinct_from_swapchain: bool,
    pub terminal_present_model: &'static str,
    pub frame_slot_count: usize,
    pub effect_target_physical_image_count: usize,
    pub effect_target_memory_bytes: u64,
    pub scene_color_mesh_draw_count: usize,
    pub scene_color_recorded_mesh_draw_count: usize,
    pub scene_color_attachment_clear_draw_count: usize,
    pub scene_color_attachment_clear_frame_count: u64,
    pub descriptor_model: &'static str,
    pub descriptor_heap_resource_count: usize,
    pub descriptor_heap_sampler_count: usize,
    pub vertex_buffer_bytes: u64,
    pub index_buffer_bytes: u64,
    pub transform_uniform_bytes: u64,
    pub material_uniform_bytes: u64,
    pub scene_owned_uniform_bytes: u64,
    pub skinning_storage_bytes: u64,
    pub scene_texture_image_count: usize,
    pub scene_texture_memory_bytes: u64,
    pub released_resource_payload_bytes: usize,
    pub released_texture_payload_bytes: usize,
    pub released_mesh_vertex_payload_bytes: usize,
    pub released_mesh_index_payload_bytes: usize,
    pub transform_uniform_update_count: u64,
    pub effect_uniform_update_count: u64,
    pub skinning_storage_update_count: u64,
    pub scene_owned_uniform_update_count: u64,
    pub frame_state_update_total_micros: u64,
    pub semantic_incremental_resolve_enabled: bool,
    pub semantic_retained_puppet_resolve_enabled: bool,
    pub semantic_dynamic_entity_count: usize,
    pub scene_script_memory: Option<crate::engine::scene::SceneScriptMemorySnapshot>,
    pub script_effect_visibility: Vec<NativeVulkanSceneScriptEffectVisibilitySnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_diagnostics: Option<NativeVulkanSceneSemanticDiagnosticsSnapshot>,
    pub semantic_resolve_total_micros: u64,
    pub graph_update_total_micros: u64,
    pub transform_update_total_micros: u64,
    pub material_update_total_micros: u64,
    pub skinning_update_total_micros: u64,
    pub scene_owned_uniform_update_total_micros: u64,
    pub draw_policy_update_total_micros: u64,
    pub sampled_descriptor_update_count: u64,
    pub sampled_descriptor_update_total_micros: u64,
    pub command_recording_total_micros: u64,
    pub fence_wait_total_micros: u64,
    pub acquire_wait_total_micros: u64,
    pub queue_present_total_micros: u64,
    pub gpu_timing: Option<serde_json::Value>,
    pub composite_scissor_draw_count: usize,
    pub composite_scissor_covered_pixels: u64,
    pub composite_scissor_avoided_pixels: u64,
    pub scene_pipeline_count: usize,
    pub scene_pipeline_machine_code_binary_count: usize,
    pub scene_pipeline_machine_code_bytes: usize,
    pub scene_pipeline_machine_code_cache_hits: usize,
    pub scene_pipeline_machine_code_cache_misses: usize,
    pub mesh_draw_count: usize,
    pub particle_gpu_emitter_count: u32,
    pub particle_gpu_total_capacity: u64,
    pub particle_gpu_state_bytes: u64,
    pub particle_gpu_indirect_bytes: u64,
    pub particle_gpu_frame_time_bytes: u64,
    pub particle_gpu_device_local: bool,
    pub particle_compute_pipeline_created: bool,
    pub particle_compute_dispatch_enabled: bool,
    pub mesh_draw_recorded: bool,
    pub command_order: Vec<&'static str>,
    pub present_backend: &'static str,
}

pub(in crate::renderer::native_vulkan) fn run_native_vulkan_scene_present(
    options: NativeVulkanScenePresentOptions,
) -> Result<NativeVulkanScenePresentSnapshot, String> {
    shared_present_loop::run(options)
}

fn scene_script_effect_visibility_snapshot(
    storage: &SceneStorage,
    resolver: &SemanticFrameResolver,
) -> Result<Vec<NativeVulkanSceneScriptEffectVisibilitySnapshot>, String> {
    let mut snapshots = Vec::new();
    for delta in resolver
        .retained_script_deltas()
        .iter()
        .filter(|delta| delta.target == SceneScriptTarget::EffectVisible)
    {
        let binding = storage
            .object_effects()
            .get(delta.selector as usize)
            .ok_or_else(|| format!("script effect visibility binding {} is missing", delta.selector))?;
        if binding.object != delta.object {
            return Err(format!(
                "script effect visibility binding {} belongs to object {}, not {}",
                delta.selector, binding.object.0, delta.object.0
            ));
        }
        let object = storage
            .objects()
            .get(delta.object.0 as usize)
            .filter(|object| object.id == delta.object)
            .ok_or_else(|| format!("script effect visibility object {} is missing", delta.object.0))?;
        let effect_index = delta
            .selector
            .checked_sub(object.effect_start)
            .filter(|index| *index < object.effect_count)
            .ok_or_else(|| format!("effect binding {} is outside object {}", delta.selector, object.id.0))?;
        let resolved = resolver
            .resolved_frame()
            .object_effect(delta.selector)
            .ok_or_else(|| format!("resolved effect binding {} is missing", delta.selector))?;
        snapshots.push(NativeVulkanSceneScriptEffectVisibilitySnapshot {
            object_handle: object.id.0,
            object_id: object.we_id,
            object_name: if object.name.is_some() {
                storage
                    .string(object.name)
                    .expect("validated object name")
                    .to_owned()
            } else {
                String::new()
            },
            binding_index: delta.selector,
            effect_index,
            effect_handle: binding.effect.0,
            effect_name: if binding.name.is_some() {
                storage
                    .string(binding.name)
                    .expect("validated effect name")
                    .to_owned()
            } else {
                String::new()
            },
            authored_visible: binding.visible,
            final_self_visible: resolved.self_visible,
            final_resolved_visible: resolved.resolved_visible,
        });
    }
    snapshots.sort_unstable_by_key(|snapshot| snapshot.binding_index);
    Ok(snapshots)
}

fn elapsed_micros_u64(started: std::time::Instant) -> u64 {
    started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests;
