//! Vulkanalia scene mesh present runtime.
//!
//! References:
//! - `docs/gilder/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/gilder/docs/exe/blend-and-render.md`
//! - `reverse-engineered/gilder/docs/exe/global-uniforms.md`
//! - `reverse-engineered/gilder/docs/shader-conventions.md`
//! - `references/gilder/godot/servers/rendering/rendering_device_graph.*`

use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{Map, Value};
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{
    self, HasBuilder, KhrSurfaceExtensionInstanceCommands, KhrSwapchainExtensionDeviceCommands,
};

use crate::renderer::native_vulkan::vulkan::core::roadmap_2026::ROADMAP_2026_API_VERSION;

use crate::engine::scene::semantic_world::SemanticFrameResolver;
use crate::engine::scene::{RenderingServer, SceneScriptTarget, SceneStorage};
use crate::renderer::native_vulkan::{
    NativeVulkanClearColor, NativeVulkanVulkanaliaBuffer,
    NativeVulkanVulkanaliaBufferMemoryPreference,
    NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput,
    NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot, NativeVulkanVulkanaliaImage,
    NativeVulkanVulkanaliaPresentDeviceExtensionSnapshot,
    NativeVulkanVulkanaliaPresentQueueSnapshot, NativeVulkanVulkanaliaRecordedBufferUpload,
    NativeVulkanVulkanaliaRecordedImageUpload, NativeVulkanVulkanaliaSwapchainSnapshot,
    VulkanaliaDescriptorHeapResourceResources,
    native_vulkan_scene_backend_plan_from_semantic_frame, native_vulkan_vulkanalia_create_buffer,
    native_vulkan_vulkanalia_create_descriptor_heap_resource_resources,
    native_vulkan_vulkanalia_create_device_local_buffer_with_recorded_staging_upload,
    native_vulkan_vulkanalia_descriptor_heap_resource_plan,
    native_vulkan_vulkanalia_destroy_buffer,
    native_vulkan_vulkanalia_destroy_descriptor_heap_resource_resources,
    native_vulkan_vulkanalia_write_descriptor_heap_resource_image_sampler,
    native_vulkan_vulkanalia_write_descriptor_heap_resource_storage_buffer,
    native_vulkan_vulkanalia_write_descriptor_heap_resource_uniform_buffer,
};
use crate::renderer::native_wayland::{
    NativeWaylandHost, NativeWaylandHostOptions, NativeWaylandSurfaceHandles,
};

use super::super::core::instance::{
    NativeVulkanVulkanaliaInstance,
    native_vulkan_vulkanalia_create_instance_with_required_extensions,
    native_vulkan_vulkanalia_destroy_instance,
};
use super::super::present::swapchain::{
    REQUIRED_INSTANCE_EXTENSIONS, composite_alpha_label, create_vulkanalia_present_device,
    create_vulkanalia_swapchain_plan, create_vulkanalia_wayland_surface, present_mode_label,
    queue_flag_labels, select_vulkanalia_present_queue, swapchain_create_flag_labels,
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
mod frame_context;
mod frame_events;
mod frame_state;
mod fullscreen_primitive;
mod gpu_resource_lifecycle;
mod gpu_timing;
mod graph_execution;
mod input_attachment_binding;
mod local_read;
mod material_uniform;
mod mesh_payload;
mod native_descriptor_push;
mod particle_compute_dispatch;
mod particle_resources;
mod pipeline;
mod present_loop;
mod resource_cleanup;
mod resource_residency;
mod resource_setup;
mod sampled_binding;
mod scene_color_clear;
mod scene_color_msaa;
mod scene_owned_uniform;
mod scene_texture;
mod scene_viewport;
mod shader_descriptor_push;
mod shader_program;
mod shader_uniform;

use present_loop::with_scene_present;
use resource_setup::*;

use command_order::scene_command_order;
use descriptor_layout::{ScenePipelineDescriptorLayout, scene_pipeline_descriptor_layout};
use descriptor_plan::scene_descriptor_plan_inputs;
use draw_recording::{
    SceneGpuDrawCommand, SceneGpuGraphDrawRange, draw_range_count, scene_color_draw_ranges,
};
use draw_uniform::{SCENE_DRAW_UNIFORM_BYTES, pack_scene_draw_uniforms};
use frame_context::{
    ScenePresentFrameContext, create_scene_present_frame_contexts,
    destroy_scene_present_frame_contexts, scene_frame_slot_count,
};
use frame_events::SceneRuntimeEventSources;
use frame_state::{SceneFrameTopology, pack_scene_skinning_palette, write_scene_frame_buffers};
use fullscreen_primitive::graph_uses_fullscreen_utility_primitive;
use gpu_resource_lifecycle::{
    color_subresource_range, create_white_texture_upload, destroy_recorded_image_upload,
    destroy_scene_gpu_frame_resources, destroy_scene_gpu_resources, identity_component_mapping,
    release_scene_upload_staging, scene_color_image_view_info, scene_sampled_sampler_info,
    scene_white_image_view_info,
};
pub use gpu_timing::NativeVulkanSceneGpuTimingSnapshot;
use gpu_timing::SceneGpuTiming;
use input_attachment_binding::{
    SceneInputAttachmentBindingPlan, scene_input_attachment_binding_cycle,
};
use local_read::{
    SceneLocalReadDeviceLimits, SceneLocalReadScopePlan, scene_local_read_scope_plans,
};
use material_uniform::{
    SCENE_MATERIAL_UNIFORM_BYTES, draw_parameter_layout, pack_scene_material_uniforms,
};
use mesh_payload::{pack_scene_indices, pack_scene_vertices};
use native_descriptor_push::resolve_scene_native_descriptor_pushes;
use pipeline::{
    ScenePipelineResources, create_scene_pipelines, emit_scene_pipeline_diagnostics_if_requested,
    scene_disabled_pipeline_indices_for_draws_with_local_read,
    scene_pipeline_indices_for_draws_with_local_read,
};
use resource_cleanup::destroy_scene_present_runtime_resources;
pub use resource_residency::NativeVulkanSceneResourceResidencySnapshot;
use sampled_binding::{
    SceneSampledImageBindingPlan, SceneSampledImageSource, scene_sampled_image_binding_cycle,
};
use scene_color_clear::SceneGpuSceneColorClear;
use scene_owned_uniform::SceneOwnedUniformArenaPlan;
pub use scene_owned_uniform::{
    NativeVulkanSceneOwnedUniformArenaPlanSnapshot, NativeVulkanSceneOwnedUniformSliceSnapshot,
    native_vulkan_scene_owned_uniform_arena_plan,
};

const SCENE_MESH_VERTEX_STRIDE_BYTES: u32 = 52;
const SCENE_WHITE_TEXTURE_BYTES: &[u8] = &[255, 255, 255, 255];

#[derive(Debug, Clone, PartialEq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanVulkanaliaScenePresentOptions {
    pub host: NativeWaylandHostOptions,
    pub wait_configure_roundtrips: usize,
    pub duration: Option<Duration>,
    pub target_max_fps: Option<u32>,
    pub clear_color: NativeVulkanClearColor,
    pub storage: SceneStorage,
    pub user_property_overrides: Map<String, Value>,
    pub surface_extent: Option<(u32, u32)>,
    pub gpu_timing: bool,
    pub pointer_replay_normalized: Option<[f64; 2]>,
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
pub struct NativeVulkanVulkanaliaScenePresentSnapshot {
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
    pub selected_queue: NativeVulkanVulkanaliaPresentQueueSnapshot,
    pub device_extensions: NativeVulkanVulkanaliaPresentDeviceExtensionSnapshot,
    pub swapchain: NativeVulkanVulkanaliaSwapchainSnapshot,
    pub command_submit_model: &'static str,
    pub uses_synchronization2: bool,
    pub uses_submit2: bool,
    pub uses_dynamic_rendering: bool,
    pub scene_color_rasterization_samples: &'static str,
    pub uses_multisampled_render_to_single_sampled: bool,
    pub uses_explicit_scene_color_msaa_resolve: bool,
    pub scene_color_msaa_memory_bytes: u64,
    pub frame_slot_count: usize,
    pub effect_target_physical_image_count: usize,
    pub effect_target_memory_bytes: u64,
    pub effect_target_dynamic_rendering_recorded: bool,
    pub effect_target_dynamic_rendering_pass_count: usize,
    pub effect_batch_count: usize,
    pub effect_batch_instance_count: usize,
    pub effect_batch_field_count: usize,
    pub effect_target_copy_command_count: usize,
    pub effect_target_swap_reference_count: usize,
    pub effect_target_mesh_draw_count: usize,
    pub effect_target_discard_load_count: usize,
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
    pub audio_spectrum_model: &'static str,
    pub audio_spectrum_ready: bool,
    pub audio_spectrum_peak: f32,
    pub audio_spectrum_active_band_count: u32,
    pub skinning_storage_bytes: u64,
    pub resource_residency: NativeVulkanSceneResourceResidencySnapshot,
    pub scene_texture_image_count: usize,
    pub scene_texture_memory_bytes: u64,
    pub released_resource_payload_bytes: usize,
    pub released_texture_payload_bytes: usize,
    pub released_mesh_vertex_payload_bytes: usize,
    pub released_mesh_index_payload_bytes: usize,
    pub sampled_fallback_texture_count: usize,
    pub sampled_fallback_descriptor_count: usize,
    pub sampled_scene_texture_descriptor_count: usize,
    pub sampled_scene_color_snapshot_descriptor_count: usize,
    pub sampled_effect_target_descriptor_count: usize,
    pub effect_target_reference_cycle_length: usize,
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
    pub gpu_timing: Option<NativeVulkanSceneGpuTimingSnapshot>,
    pub composite_scissor_draw_count: usize,
    pub composite_scissor_covered_pixels: u64,
    pub composite_scissor_avoided_pixels: u64,
    pub scene_pipeline_count: usize,
    pub mesh_draw_count: usize,
    pub particle_instance_capacity: u64,
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
            .ok_or_else(|| {
                format!(
                    "script effect visibility binding {} is outside scene storage",
                    delta.selector
                )
            })?;
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
            .ok_or_else(|| {
                format!(
                    "script effect visibility references missing object {}",
                    delta.object.0
                )
            })?;
        let effect_index = delta
            .selector
            .checked_sub(object.effect_start)
            .filter(|index| *index < object.effect_count)
            .ok_or_else(|| {
                format!(
                    "script effect visibility binding {} is outside object {} effect range",
                    delta.selector, delta.object.0
                )
            })?;
        let resolved = resolver
            .resolved_frame()
            .object_effect(delta.selector)
            .ok_or_else(|| {
                format!(
                    "resolved semantic frame has no effect binding {}",
                    delta.selector
                )
            })?;
        snapshots.push(NativeVulkanSceneScriptEffectVisibilitySnapshot {
            object_handle: object.id.0,
            object_id: object.we_id,
            object_name: if object.name.is_some() {
                storage
                    .string(object.name)
                    .expect("scene storage validates object name strings")
            } else {
                ""
            }
            .to_owned(),
            binding_index: delta.selector,
            effect_index,
            effect_handle: binding.effect.0,
            effect_name: if binding.name.is_some() {
                storage
                    .string(binding.name)
                    .expect("scene storage validates effect name strings")
            } else {
                ""
            }
            .to_owned(),
            authored_visible: binding.visible,
            final_self_visible: resolved.self_visible,
            final_resolved_visible: resolved.resolved_visible,
        });
    }
    snapshots.sort_unstable_by_key(|snapshot| snapshot.binding_index);
    Ok(snapshots)
}

struct SceneGpuResources {
    mesh_uploads: SceneMeshGpuUploads,
    mesh_coverage: composite_scissor::SceneMeshCoveragePlans,
    frame_resources: Vec<SceneGpuFrameResources>,
    active_frame_slot: usize,
    white_upload: Option<NativeVulkanVulkanaliaRecordedImageUpload>,
    scene_textures: Vec<scene_texture::SceneTextureImageResource>,
    effect_targets: Vec<effect_target::SceneEffectTargetImageResource>,
    effect_target_command_plan: effect_target::SceneEffectTargetCommandPlan,
    effect_target_commands: Vec<effect_target::SceneEffectTargetCommand>,
    effect_target_allocations: Vec<crate::engine::scene::SceneRenderingDeviceTargetAllocation>,
    pass_nodes: Vec<crate::engine::scene::SceneRenderingDevicePassNode>,
    scene_color_draw_ranges: Vec<SceneGpuGraphDrawRange>,
    scene_color_attachment_clear: Option<SceneGpuSceneColorClear>,
    scene_color_attachment_clear_enabled: bool,
    graph_execution_order: Vec<u32>,
    descriptor_heap_plan: NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    particle_global_descriptor_base: Option<usize>,
    pipelines: ScenePipelineResources,
    draw_commands: Vec<SceneGpuDrawCommand>,
    descriptor_layout: ScenePipelineDescriptorLayout,
    sampled_binding_cycle: Vec<SceneSampledImageBindingPlan>,
    input_attachment_binding_cycle: Vec<SceneInputAttachmentBindingPlan>,
    local_read_scopes: Vec<SceneLocalReadScopePlan>,
    local_read_limits: SceneLocalReadDeviceLimits,
    sampled_descriptor_dirty_update_enabled: bool,
    frame_topology: SceneFrameTopology,
    transform_scratch: Vec<u8>,
    scene_owned_uniform_plan: SceneOwnedUniformArenaPlan,
    scene_owned_uniform_scratch: Vec<u8>,
    dynamic_text: dynamic_text::SceneDynamicTextRuntime,
    dynamic_effect_uniforms: bool,
    scene_color_msaa_enabled: bool,
    multisampled_render_to_single_sampled_enabled: bool,
    scene_color_msaa_targets: Vec<NativeVulkanVulkanaliaImage>,
    particle_resources: Option<particle_resources::SceneParticleGpuResources>,
    particle_scene_time_seconds: f32,
}

struct SceneMeshGpuUploads {
    vertex: NativeVulkanVulkanaliaRecordedBufferUpload,
    index: NativeVulkanVulkanaliaRecordedBufferUpload,
}

impl SceneMeshGpuUploads {
    fn create(
        device: &Device,
        memory_properties: &vk::PhysicalDeviceMemoryProperties,
        command_buffer: vk::CommandBuffer,
        vertex_payload: &[u8],
        index_payload: &[u8],
    ) -> Result<Self, String> {
        let vertex =
            native_vulkan_vulkanalia_create_device_local_buffer_with_recorded_staging_upload(
                device,
                memory_properties,
                command_buffer,
                "scene-mesh-vertex-buffer",
                vertex_payload.len() as u64,
                vk::BufferUsageFlags::VERTEX_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
                vertex_payload,
            )?;
        let index =
            match native_vulkan_vulkanalia_create_device_local_buffer_with_recorded_staging_upload(
                device,
                memory_properties,
                command_buffer,
                "scene-mesh-index-buffer",
                index_payload.len() as u64,
                vk::BufferUsageFlags::INDEX_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
                index_payload,
            ) {
                Ok(upload) => upload,
                Err(error) => {
                    destroy_recorded_buffer_upload(device, vertex);
                    return Err(error);
                }
            };
        Ok(Self { vertex, index })
    }

    fn release_staging(&mut self, device: &Device) {
        for upload in [&mut self.vertex, &mut self.index] {
            if let Some(staging) = upload.staging.take() {
                native_vulkan_vulkanalia_destroy_buffer(device, staging);
            }
        }
    }

    fn destroy(self, device: &Device) {
        destroy_recorded_buffer_upload(device, self.index);
        destroy_recorded_buffer_upload(device, self.vertex);
    }
}

fn destroy_recorded_buffer_upload(
    device: &Device,
    upload: NativeVulkanVulkanaliaRecordedBufferUpload,
) {
    if let Some(staging) = upload.staging {
        native_vulkan_vulkanalia_destroy_buffer(device, staging);
    }
    native_vulkan_vulkanalia_destroy_buffer(device, upload.target);
}

struct SceneGpuFrameResources {
    transform_buffer: NativeVulkanVulkanaliaBuffer,
    material_buffer: Option<NativeVulkanVulkanaliaBuffer>,
    skinning_buffer: Option<NativeVulkanVulkanaliaBuffer>,
    scene_owned_uniform_buffer: Option<NativeVulkanVulkanaliaBuffer>,
    descriptor_heap: VulkanaliaDescriptorHeapResourceResources,
    image_binding_phase: usize,
}

impl SceneGpuResources {
    fn active_frame(&self) -> &SceneGpuFrameResources {
        &self.frame_resources[self.active_frame_slot]
    }
}

pub(in crate::renderer::native_vulkan) fn run_native_vulkan_vulkanalia_scene_present(
    options: NativeVulkanVulkanaliaScenePresentOptions,
) -> Result<NativeVulkanVulkanaliaScenePresentSnapshot, String> {
    let mut host =
        NativeWaylandHost::connect(options.host.clone()).map_err(|err| err.to_string())?;
    host.wait_until_configured(options.wait_configure_roundtrips)
        .map_err(|err| err.to_string())?;
    let handles = host.surface_handles().map_err(|err| err.to_string())?;

    let vulkan = native_vulkan_vulkanalia_create_instance_with_required_extensions(
        REQUIRED_INSTANCE_EXTENSIONS,
    )?;
    let result = run_scene_present_inner(&vulkan, handles, &mut host, options);
    native_vulkan_vulkanalia_destroy_instance(vulkan);
    result
}

fn run_scene_present_inner(
    vulkan: &NativeVulkanVulkanaliaInstance,
    handles: NativeWaylandSurfaceHandles,
    host: &mut NativeWaylandHost,
    options: NativeVulkanVulkanaliaScenePresentOptions,
) -> Result<NativeVulkanVulkanaliaScenePresentSnapshot, String> {
    let instance = &vulkan.instance;
    let surface = create_vulkanalia_wayland_surface(instance, handles)?;
    let result = with_scene_present(instance, surface, handles, host, vulkan, options);
    unsafe {
        instance.destroy_surface_khr(surface, None);
    }
    result
}

fn record_scene_present_command_buffer(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    swapchain_image: vk::Image,
    swapchain_view: vk::ImageView,
    old_layout: vk::ImageLayout,
    extent: vk::Extent2D,
    clear_color: NativeVulkanClearColor,
    scene: &SceneGpuResources,
    reference_phase: usize,
    gpu_timing: Option<&SceneGpuTiming>,
) -> Result<(), String> {
    unsafe {
        device
            .reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty())
            .map_err(|err| format!("vkResetCommandBuffer(vulkanalia scene present): {err:?}"))?;
        let begin_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
            .build();
        device
            .begin_command_buffer(command_buffer, &begin_info)
            .map_err(|err| format!("vkBeginCommandBuffer(vulkanalia scene present): {err:?}"))?;
    }
    if let Some(timing) = gpu_timing {
        timing.record_start(device, command_buffer);
    }
    graph_execution::record_scene_graphs_to_swapchain(
        device,
        command_buffer,
        swapchain_image,
        swapchain_view,
        old_layout,
        extent,
        clear_color,
        scene,
        reference_phase,
        gpu_timing,
    )?;
    graph_execution::transition_swapchain_to_present(device, command_buffer, swapchain_image);
    if let Some(timing) = gpu_timing {
        timing.record_finish(device, command_buffer);
    }
    unsafe {
        device
            .end_command_buffer(command_buffer)
            .map_err(|err| format!("vkEndCommandBuffer(vulkanalia scene present): {err:?}"))?;
    }
    Ok(())
}

fn submit_scene_present_command_buffer2(
    device: &Device,
    queue: vk::Queue,
    command_buffer: vk::CommandBuffer,
    image_available: vk::Semaphore,
    render_finished: vk::Semaphore,
    fence: vk::Fence,
) -> Result<(), String> {
    let wait = vk::SemaphoreSubmitInfo::builder()
        .semaphore(image_available)
        .stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
        .build();
    let waits = [wait];
    let command_buffer_info = vk::CommandBufferSubmitInfo::builder()
        .command_buffer(command_buffer)
        .build();
    let command_buffer_infos = [command_buffer_info];
    let signal = vk::SemaphoreSubmitInfo::builder()
        .semaphore(render_finished)
        .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .build();
    let signals = [signal];
    let submit_info = vk::SubmitInfo2::builder()
        .wait_semaphore_infos(&waits)
        .command_buffer_infos(&command_buffer_infos)
        .signal_semaphore_infos(&signals)
        .build();
    unsafe {
        device
            .queue_submit2(queue, &[submit_info], fence)
            .map_err(|err| format!("vkQueueSubmit2(vulkanalia scene present): {err:?}"))?;
    }
    Ok(())
}

fn submit_and_wait_setup_commands(
    device: &Device,
    queue: vk::Queue,
    command_buffer: vk::CommandBuffer,
    label: &'static str,
) -> Result<(), String> {
    let fence_info = vk::FenceCreateInfo::builder();
    let fence = unsafe { device.create_fence(&fence_info, None) }
        .map_err(|err| format!("vkCreateFence(vulkanalia {label}): {err:?}"))?;
    let command_buffer_info = vk::CommandBufferSubmitInfo::builder()
        .command_buffer(command_buffer)
        .build();
    let command_buffer_infos = [command_buffer_info];
    let submit_info = vk::SubmitInfo2::builder()
        .command_buffer_infos(&command_buffer_infos)
        .build();
    let result = unsafe {
        device
            .queue_submit2(queue, &[submit_info], fence)
            .map_err(|err| format!("vkQueueSubmit2(vulkanalia {label}): {err:?}"))
            .and_then(|()| {
                device
                    .wait_for_fences(&[fence], true, u64::MAX)
                    .map(|_| ())
                    .map_err(|err| format!("vkWaitForFences(vulkanalia {label}): {err:?}"))
            })
    };
    unsafe {
        device.destroy_fence(fence, None);
    }
    result
}

fn begin_one_time_commands(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    label: &'static str,
) -> Result<(), String> {
    unsafe {
        device
            .reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty())
            .map_err(|err| format!("vkResetCommandBuffer(vulkanalia {label}): {err:?}"))?;
        let begin_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
            .build();
        device
            .begin_command_buffer(command_buffer, &begin_info)
            .map_err(|err| format!("vkBeginCommandBuffer(vulkanalia {label}): {err:?}"))?;
    }
    Ok(())
}

fn elapsed_micros_u64(started: Instant) -> u64 {
    started.elapsed().as_micros().min(u64::MAX as u128) as u64
}

fn end_one_time_commands(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    label: &'static str,
) -> Result<(), String> {
    unsafe {
        device
            .end_command_buffer(command_buffer)
            .map_err(|err| format!("vkEndCommandBuffer(vulkanalia {label}): {err:?}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
