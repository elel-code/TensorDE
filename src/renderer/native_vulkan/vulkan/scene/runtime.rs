//! Vulkanalia scene mesh present runtime.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/global-uniforms.md`
//! - `reverse-engineered/docs/shader-conventions.md`
//! - `references/godot/servers/rendering/rendering_device_graph.*`

use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use vulkanalia::Version;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{
    self, HasBuilder, KhrSurfaceExtensionInstanceCommands, KhrSwapchainExtensionDeviceCommands,
};

use crate::engine::scene::{
    RenderingServer, SceneParticleGpuEmitterPlan, SceneRenderingDeviceDrawPrimitive,
    SceneRenderingDeviceMeshDraw, SceneStorage,
};
use crate::renderer::native_vulkan::{
    NATIVE_VULKAN_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES, NativeVulkanClearColor,
    NativeVulkanVulkanaliaBuffer, NativeVulkanVulkanaliaBufferMemoryPreference,
    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind,
    NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput,
    NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot, NativeVulkanVulkanaliaImage,
    NativeVulkanVulkanaliaPresentDeviceExtensionSnapshot,
    NativeVulkanVulkanaliaPresentQueueSnapshot, NativeVulkanVulkanaliaRecordedImageUpload,
    NativeVulkanVulkanaliaSwapchainSnapshot, VulkanaliaDescriptorHeapResourceResources,
    native_vulkan_scene_backend_plan, native_vulkan_vulkanalia_create_buffer,
    native_vulkan_vulkanalia_create_descriptor_heap_resource_resources,
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
    OPTIONAL_INSTANCE_EXTENSIONS, REQUIRED_INSTANCE_EXTENSIONS, composite_alpha_label,
    create_vulkanalia_present_device, create_vulkanalia_swapchain_plan,
    create_vulkanalia_wayland_surface, present_mode_label, queue_flag_labels,
    select_vulkanalia_present_queue, swapchain_create_flag_labels,
    vulkanalia_surface_capabilities2_enabled, vulkanalia_surface_maintenance1_enabled,
};

mod alpha_coverage_scissor;
mod command_order;
mod composite_scissor;
mod draw_recording;
mod draw_uniform;
mod effect_target;
mod flat_rounded_mask_coverage;
mod frame_capture;
mod frame_context;
mod frame_events;
mod frame_state;
mod fullscreen_primitive;
mod gpu_resource_lifecycle;
mod gpu_timing;
mod graph_execution;
mod material_uniform;
mod mesh_payload;
mod particle_compute_dispatch;
mod particle_resources;
mod pipeline;
mod present_loop;
mod resource_residency;
mod resource_setup;
mod sampled_binding;
mod scene_color_clear;
mod scene_color_msaa;
mod scene_texture;
mod scene_viewport;

use present_loop::with_scene_present;
use resource_setup::*;

use alpha_coverage_scissor::scene_alpha_coverage_scissors;
use command_order::scene_command_order;
use draw_recording::{
    SceneGpuDrawCommand, SceneGpuGraphDrawRange, SceneGpuScissor, draw_range_count,
    scene_color_draw_ranges,
};
use draw_uniform::{SCENE_DRAW_UNIFORM_BYTES, pack_scene_draw_uniforms};
use frame_capture::SceneFrameCapture;
pub use frame_capture::{
    NativeVulkanSceneFrameCaptureSnapshot, NativeVulkanSceneFrameTemporalAnalysisSnapshot,
};
use frame_context::{
    create_scene_present_frame_contexts, destroy_scene_present_frame_contexts,
    scene_frame_slot_count,
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
use material_uniform::{
    SCENE_MATERIAL_UNIFORM_BYTES, draw_parameter_layout, pack_scene_material_uniforms,
};
use mesh_payload::{pack_scene_indices, pack_scene_vertices};
use pipeline::{
    ScenePipelineResources, create_scene_pipelines, emit_scene_pipeline_diagnostics_if_requested,
    scene_pipeline_descriptor_layout, scene_pipeline_indices_for_draws,
};
pub use resource_residency::NativeVulkanSceneResourceResidencySnapshot;
use sampled_binding::{
    SceneSampledImageBindingPlan, SceneSampledImageSource, scene_sampled_image_binding_cycle,
};
use scene_color_clear::SceneGpuSceneColorClear;

const SCENE_MESH_VERTEX_STRIDE_BYTES: u32 = 52;
const SCENE_WHITE_TEXTURE_BYTES: &[u8] = &[255, 255, 255, 255];

#[derive(Debug, Clone, PartialEq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanVulkanaliaScenePresentOptions {
    pub host: NativeWaylandHostOptions,
    pub wait_configure_roundtrips: usize,
    pub duration: Duration,
    pub target_max_fps: Option<u32>,
    pub clear_color: NativeVulkanClearColor,
    pub storage: SceneStorage,
    pub capture_frame: Option<PathBuf>,
    pub capture_frame_number: u64,
    pub capture_frame_count: u64,
    pub capture_frame_step: u64,
    pub capture_frame_downscale: u32,
    pub capture_frame_region: Option<(u32, u32, u32, u32)>,
    pub capture_frame_reference: Option<PathBuf>,
    pub capture_frame_time_step_seconds: Option<f32>,
    pub capture_scene_graph: Option<u32>,
    pub surface_extent: Option<(u32, u32)>,
    pub gpu_timing: bool,
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
    pub capture_scene_graph: Option<u32>,
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
    pub sampled_fallback_texture_count: usize,
    pub sampled_fallback_descriptor_count: usize,
    pub sampled_scene_texture_descriptor_count: usize,
    pub sampled_scene_color_snapshot_descriptor_count: usize,
    pub sampled_effect_target_descriptor_count: usize,
    pub effect_target_reference_cycle_length: usize,
    pub transform_uniform_update_count: u64,
    pub effect_uniform_update_count: u64,
    pub skinning_storage_update_count: u64,
    pub frame_state_update_total_micros: u64,
    pub semantic_incremental_resolve_enabled: bool,
    pub semantic_retained_puppet_resolve_enabled: bool,
    pub semantic_dynamic_entity_count: usize,
    pub semantic_resolve_total_micros: u64,
    pub graph_update_total_micros: u64,
    pub transform_update_total_micros: u64,
    pub material_update_total_micros: u64,
    pub skinning_update_total_micros: u64,
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
    pub alpha_coverage_scissor_draw_count: usize,
    pub alpha_coverage_scissor_segment_count: usize,
    pub alpha_coverage_scissor_pixels: u64,
    pub scene_pipeline_count: usize,
    pub mesh_draw_count: usize,
    pub particle_instance_capacity: u64,
    pub particle_instance_submitted: u64,
    pub particle_gpu_emitter_count: u32,
    pub particle_gpu_total_capacity: u64,
    pub particle_gpu_state_bytes: u64,
    pub particle_gpu_indirect_bytes: u64,
    pub particle_gpu_device_local: bool,
    pub particle_compute_pipeline_created: bool,
    pub particle_compute_dispatch_enabled: bool,
    pub particle_indirect_readback_valid: bool,
    pub particle_indirect_readback_instance_total: u64,
    pub mesh_draw_recorded: bool,
    pub command_order: Vec<&'static str>,
    pub present_backend: &'static str,
    #[serde(skip)]
    pub(in crate::renderer::native_vulkan) frame_capture:
        Option<NativeVulkanSceneFrameCaptureSnapshot>,
}

struct SceneGpuResources {
    vertex_buffer: NativeVulkanVulkanaliaBuffer,
    index_buffer: NativeVulkanVulkanaliaBuffer,
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
    capture_scene_graph: Option<u32>,
    descriptor_heap_plan: NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    particle_global_descriptor_base: Option<usize>,
    pipelines: ScenePipelineResources,
    draw_commands: Vec<SceneGpuDrawCommand>,
    sampled_slots: Vec<u32>,
    sampled_binding_cycle: Vec<SceneSampledImageBindingPlan>,
    sampled_descriptor_dirty_update_enabled: bool,
    material_uniform_enabled: bool,
    frame_topology: SceneFrameTopology,
    dynamic_effect_uniforms: bool,
    scene_color_msaa_enabled: bool,
    multisampled_render_to_single_sampled_enabled: bool,
    scene_color_msaa_targets: Vec<NativeVulkanVulkanaliaImage>,
    particle_resources: Option<particle_resources::SceneParticleGpuResources>,
    particle_scene_time_seconds: f32,
}

struct SceneGpuFrameResources {
    transform_buffer: NativeVulkanVulkanaliaBuffer,
    material_buffer: Option<NativeVulkanVulkanaliaBuffer>,
    skinning_buffer: Option<NativeVulkanVulkanaliaBuffer>,
    descriptor_heap: VulkanaliaDescriptorHeapResourceResources,
    sampled_binding_phase: usize,
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

    let mut requested_instance_extensions = REQUIRED_INSTANCE_EXTENSIONS.to_vec();
    requested_instance_extensions.extend_from_slice(OPTIONAL_INSTANCE_EXTENSIONS);
    let vulkan = native_vulkan_vulkanalia_create_instance_with_required_extensions(
        &requested_instance_extensions,
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
    frame_capture: Option<&SceneFrameCapture>,
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
    if let Some(capture) = frame_capture {
        capture.record_swapchain_copy(device, command_buffer, swapchain_image);
    } else {
        graph_execution::transition_swapchain_to_present(device, command_buffer, swapchain_image);
    }
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

fn scene_descriptor_plan_inputs(
    draws: &[SceneRenderingDeviceMeshDraw],
    particle_emitters: &[SceneParticleGpuEmitterPlan],
    layout: &pipeline::ScenePipelineDescriptorLayout,
    pipeline_indices: &[u32],
    alpha_coverage_scissors: &[Vec<SceneGpuScissor>],
) -> (
    Vec<NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind>,
    Vec<SceneGpuDrawCommand>,
) {
    let per_draw_resource_count = 1
        + usize::from(layout.material_uniform_enabled)
        + usize::from(layout.skinning_storage_enabled)
        + layout.sampled_slots.len();
    let mut resources = Vec::with_capacity(draws.len().saturating_mul(per_draw_resource_count));
    let mut commands = Vec::with_capacity(draws.len());
    for (index, draw) in draws.iter().enumerate() {
        let base = index * per_draw_resource_count;
        resources.push(NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer);
        if layout.material_uniform_enabled {
            resources
                .push(NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer);
        }
        let (skinning_byte_offset, skinning_byte_count) = if layout.skinning_storage_enabled {
            resources
                .push(NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::StorageBuffer);
            scene_draw_skinning_range(draw)
        } else {
            (0, 0)
        };
        resources.extend(
            layout
                .sampled_slots
                .iter()
                .map(|_| NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage),
        );
        commands.push(SceneGpuDrawCommand {
            primitive: draw.primitive,
            pipeline_index: pipeline_indices.get(index).copied().unwrap_or(0),
            first_index: draw.index_start,
            index_count: draw.index_count,
            vertex_offset: draw.vertex_start as i32,
            vertex_count: draw.vertex_count,
            instance_count: draw.instance_count,
            instance_capacity: draw.instance_count,
            particle_indirect_index: particle_emitters
                .iter()
                .find(|emitter| {
                    draw.primitive == SceneRenderingDeviceDrawPrimitive::ParticleBillboard
                        && emitter.object == draw.object
                })
                .map(|emitter| emitter.indirect_draw_index),
            resource_descriptor_base: base,
            sampler_descriptor_base: index * layout.sampled_slots.len(),
            skinning_byte_offset,
            skinning_byte_count,
            scissor: None,
            alpha_coverage_scissors: alpha_coverage_scissors
                .get(index)
                .cloned()
                .unwrap_or_default(),
        });
    }
    (resources, commands)
}

fn scene_draw_skinning_range(draw: &SceneRenderingDeviceMeshDraw) -> (u64, u64) {
    if draw.skinning_palette_count == 0 {
        return (
            0,
            NATIVE_VULKAN_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES as u64,
        );
    }
    (
        draw.skinning_palette_start.saturating_add(1) as u64
            * NATIVE_VULKAN_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES as u64,
        draw.skinning_palette_count as u64
            * NATIVE_VULKAN_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES as u64,
    )
}

#[cfg(test)]
mod tests;
