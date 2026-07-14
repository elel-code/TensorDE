//! Vulkanalia scene mesh present runtime.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/global-uniforms.md`
//! - `reverse-engineered/docs/shader-conventions.md`
//! - `references/godot/servers/rendering/rendering_device_graph.*`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.*`

use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use vulkanalia::Version;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{
    self, HasBuilder, KhrSurfaceExtensionInstanceCommands, KhrSwapchainExtensionDeviceCommands,
};

use crate::engine::scene::{RenderingServer, SceneRenderingDeviceMeshDraw, SceneStorage};
use crate::renderer::native_vulkan::audio::system_monitor::NativeVulkanSystemAudioMonitor;
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

mod command_order;
mod alpha_coverage_scissor;
mod composite_scissor;
mod draw_recording;
mod draw_uniform;
mod effect_target;
mod flat_rounded_mask_coverage;
mod frame_capture;
mod frame_state;
mod fullscreen_primitive;
mod gpu_resource_lifecycle;
mod gpu_timing;
mod graph_execution;
mod material_uniform;
mod mesh_payload;
mod pipeline;
mod resource_residency;
mod sampled_binding;
mod scene_texture;
mod scene_viewport;

use command_order::scene_command_order;
use alpha_coverage_scissor::scene_alpha_coverage_scissors;
use draw_recording::{
    SceneGpuDrawCommand, SceneGpuGraphDrawRange, SceneGpuScissor, draw_range_count,
    scene_color_draw_ranges,
};
use draw_uniform::{SCENE_DRAW_UNIFORM_BYTES, pack_scene_draw_uniforms};
pub use frame_capture::NativeVulkanSceneFrameCaptureSnapshot;
use frame_capture::SceneFrameCapture;
use frame_state::{SceneFrameTopology, pack_scene_skinning_palette, write_scene_frame_buffers};
use fullscreen_primitive::{
    graph_uses_fullscreen_utility_primitive,
};
use gpu_resource_lifecycle::{
    color_subresource_range, create_white_texture_upload, destroy_recorded_image_upload,
    destroy_scene_gpu_resources, identity_component_mapping, release_scene_upload_staging,
    scene_color_image_view_info, scene_sampled_sampler_info, scene_white_image_view_info,
};
pub use gpu_timing::NativeVulkanSceneGpuTimingSnapshot;
use gpu_timing::SceneGpuTiming;
pub use resource_residency::NativeVulkanSceneResourceResidencySnapshot;
use material_uniform::{
    SCENE_MATERIAL_UNIFORM_BYTES, material_parameter_layout, pack_scene_material_uniforms,
    scene_audio_spectrum_status, scene_uses_audio_spectrum,
};
use mesh_payload::{pack_scene_indices, pack_scene_vertices};
use pipeline::{
    ScenePipelineResources, create_scene_pipelines,
    emit_scene_pipeline_diagnostics_if_requested, scene_pipeline_descriptor_layout,
    scene_pipeline_indices_for_draws,
};
use sampled_binding::{
    SceneSampledImageBindingPlan, SceneSampledImageSource, scene_sampled_image_binding_cycle,
};

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
    pub descriptor_model: &'static str,
    pub descriptor_heap_resource_count: usize,
    pub descriptor_heap_sampler_count: usize,
    pub vertex_buffer_bytes: u64,
    pub index_buffer_bytes: u64,
    pub transform_uniform_bytes: u64,
    pub material_uniform_bytes: u64,
    pub audio_spectrum_model: &'static str,
    pub audio_spectrum_ready: bool,
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
    transform_buffer: NativeVulkanVulkanaliaBuffer,
    material_buffer: Option<NativeVulkanVulkanaliaBuffer>,
    skinning_buffer: Option<NativeVulkanVulkanaliaBuffer>,
    white_upload: Option<NativeVulkanVulkanaliaRecordedImageUpload>,
    scene_textures: Vec<scene_texture::SceneTextureImageResource>,
    effect_targets: Vec<effect_target::SceneEffectTargetImageResource>,
    effect_target_command_plan: effect_target::SceneEffectTargetCommandPlan,
    effect_target_commands: Vec<effect_target::SceneEffectTargetCommand>,
    effect_target_allocations: Vec<crate::engine::scene::SceneRenderingDeviceTargetAllocation>,
    pass_nodes: Vec<crate::engine::scene::SceneRenderingDevicePassNode>,
    scene_color_draw_ranges: Vec<SceneGpuGraphDrawRange>,
    graph_execution_order: Vec<u32>,
    capture_scene_graph: Option<u32>,
    descriptor_heap: VulkanaliaDescriptorHeapResourceResources,
    descriptor_heap_plan: NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    pipelines: ScenePipelineResources,
    draw_commands: Vec<SceneGpuDrawCommand>,
    sampled_slots: Vec<u32>,
    sampled_binding_cycle: Vec<SceneSampledImageBindingPlan>,
    material_uniform_enabled: bool,
    frame_topology: SceneFrameTopology,
    dynamic_effect_uniforms: bool,
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
    let result = run_scene_present_inner(&vulkan, handles, options);
    native_vulkan_vulkanalia_destroy_instance(vulkan);
    result
}

fn run_scene_present_inner(
    vulkan: &NativeVulkanVulkanaliaInstance,
    handles: NativeWaylandSurfaceHandles,
    options: NativeVulkanVulkanaliaScenePresentOptions,
) -> Result<NativeVulkanVulkanaliaScenePresentSnapshot, String> {
    let instance = &vulkan.instance;
    let surface = create_vulkanalia_wayland_surface(instance, handles)?;
    let result = with_scene_present(instance, surface, handles, vulkan, options);
    unsafe {
        instance.destroy_surface_khr(surface, None);
    }
    result
}

fn with_scene_present(
    instance: &Instance,
    surface: vk::SurfaceKHR,
    handles: NativeWaylandSurfaceHandles,
    vulkan: &NativeVulkanVulkanaliaInstance,
    mut options: NativeVulkanVulkanaliaScenePresentOptions,
) -> Result<NativeVulkanVulkanaliaScenePresentSnapshot, String> {
    let mut system_audio_monitor = NativeVulkanSystemAudioMonitor::start_if_needed(
        scene_uses_audio_spectrum(&options.storage),
    );
    let physical_devices = unsafe { instance.enumerate_physical_devices() }
        .map_err(|err| format!("vkEnumeratePhysicalDevices(vulkanalia scene present): {err:?}"))?;
    let mut present_queue_family_count = 0usize;
    let selection = select_vulkanalia_present_queue(
        instance,
        surface,
        handles,
        &physical_devices,
        &mut present_queue_family_count,
    )?;
    let present_device = create_vulkanalia_present_device(
        instance,
        &selection,
        vulkanalia_surface_maintenance1_enabled(vulkan),
    )?;
    if !present_device.feature_selection.synchronization2_enabled {
        unsafe {
            present_device.device.destroy_device(None);
        }
        return Err(format!(
            "selected Vulkan device {:?} is missing synchronization2 required by scene QueueSubmit2",
            selection.physical_device_name
        ));
    }
    if !present_device.feature_selection.dynamic_rendering_enabled {
        unsafe {
            present_device.device.destroy_device(None);
        }
        return Err(format!(
            "selected Vulkan device {:?} is missing dynamic rendering required by scene present",
            selection.physical_device_name
        ));
    }
    if !present_device
        .feature_selection
        .core_features
        .descriptor_heap
    {
        unsafe {
            present_device.device.destroy_device(None);
        }
        return Err(format!(
            "selected Vulkan device {:?} is missing VK_EXT_descriptor_heap required by scene present",
            selection.physical_device_name
        ));
    }

    let project = options.storage.project();
    let automatic_surface_extent = scene_viewport::automatic_scene_surface_extent(
        (project.logical_width, project.logical_height),
        handles.buffer_size,
    );
    let swapchain_plan = match create_vulkanalia_swapchain_plan(
        instance,
        selection.physical_device,
        surface,
        options
            .surface_extent
            .unwrap_or(automatic_surface_extent),
        vulkanalia_surface_capabilities2_enabled(vulkan),
        &present_device.feature_selection,
    ) {
        Ok(plan) => plan,
        Err(err) => {
            unsafe {
                present_device.device.destroy_device(None);
            }
            return Err(err);
        }
    };
    let device = &present_device.device;
    let swapchain = match unsafe { device.create_swapchain_khr(&swapchain_plan.create_info, None) }
    {
        Ok(swapchain) => swapchain,
        Err(err) => {
            unsafe {
                present_device.device.destroy_device(None);
            }
            return Err(format!(
                "vkCreateSwapchainKHR(vulkanalia scene present): {err:?}"
            ));
        }
    };
    let swapchain_images = match unsafe { device.get_swapchain_images_khr(swapchain) } {
        Ok(images) => images,
        Err(err) => {
            unsafe {
                device.destroy_swapchain_khr(swapchain, None);
                present_device.device.destroy_device(None);
            }
            return Err(format!(
                "vkGetSwapchainImagesKHR(vulkanalia scene present): {err:?}"
            ));
        }
    };

    let command_pool_info = vk::CommandPoolCreateInfo::builder()
        .flags(
            vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER
                | vk::CommandPoolCreateFlags::TRANSIENT,
        )
        .queue_family_index(selection.queue_family_index);
    let command_pool = match unsafe { device.create_command_pool(&command_pool_info, None) } {
        Ok(command_pool) => command_pool,
        Err(err) => {
            unsafe {
                device.destroy_swapchain_khr(swapchain, None);
                present_device.device.destroy_device(None);
            }
            return Err(format!(
                "vkCreateCommandPool(vulkanalia scene present): {err:?}"
            ));
        }
    };
    let command_buffer_count = swapchain_images.len().saturating_add(1) as u32;
    let command_buffer_info = vk::CommandBufferAllocateInfo::builder()
        .command_pool(command_pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(command_buffer_count);
    let command_buffers = match unsafe { device.allocate_command_buffers(&command_buffer_info) } {
        Ok(command_buffers) => command_buffers,
        Err(err) => {
            unsafe {
                device.destroy_command_pool(command_pool, None);
                device.destroy_swapchain_khr(swapchain, None);
                present_device.device.destroy_device(None);
            }
            return Err(format!(
                "vkAllocateCommandBuffers(vulkanalia scene present): {err:?}"
            ));
        }
    };
    let setup_command_buffer = *command_buffers
        .last()
        .ok_or_else(|| "scene present did not allocate setup command buffer".to_owned())?;
    let present_command_buffers = &command_buffers[..swapchain_images.len()];

    let mut swapchain_views = Vec::with_capacity(swapchain_images.len());
    for image in &swapchain_images {
        let view_info = vk::ImageViewCreateInfo::builder()
            .image(*image)
            .view_type(vk::ImageViewType::_2D)
            .format(swapchain_plan.format.format)
            .components(identity_component_mapping())
            .subresource_range(color_subresource_range())
            .build();
        match unsafe { device.create_image_view(&view_info, None) } {
            Ok(view) => swapchain_views.push(view),
            Err(err) => {
                unsafe {
                    for view in swapchain_views {
                        device.destroy_image_view(view, None);
                    }
                    device.destroy_command_pool(command_pool, None);
                    device.destroy_swapchain_khr(swapchain, None);
                    present_device.device.destroy_device(None);
                }
                return Err(format!(
                    "vkCreateImageView(vulkanalia scene swapchain): {err:?}"
                ));
            }
        }
    }

    let memory_properties =
        unsafe { instance.get_physical_device_memory_properties(selection.physical_device) };
    begin_one_time_commands(device, setup_command_buffer, "scene setup")?;
    let mut scene_resources = match create_scene_gpu_resources(
        device,
        &memory_properties,
        setup_command_buffer,
        &options.storage,
        swapchain_plan.format.format,
        *swapchain_images
            .first()
            .ok_or_else(|| "scene swapchain has no images".to_owned())?,
        swapchain_plan.extent,
        &present_device.feature_selection.descriptor_heap_properties,
        present_device
            .feature_selection
            .blend_operation_advanced_enabled,
        present_device
            .feature_selection
            .blend_operation_advanced_coherent_operations,
        options.capture_scene_graph,
    ) {
        Ok(resources) => resources,
        Err(err) => {
            unsafe {
                for view in swapchain_views {
                    device.destroy_image_view(view, None);
                }
                device.destroy_command_pool(command_pool, None);
                device.destroy_swapchain_khr(swapchain, None);
                present_device.device.destroy_device(None);
            }
            return Err(err);
        }
    };
    end_one_time_commands(device, setup_command_buffer, "scene setup")?;
    if let Err(err) = submit_and_wait_setup_commands(
        device,
        present_device.queue,
        setup_command_buffer,
        "scene setup",
    ) {
        destroy_scene_gpu_resources(device, scene_resources);
        unsafe {
            for view in swapchain_views {
                device.destroy_image_view(view, None);
            }
            device.destroy_command_pool(command_pool, None);
            device.destroy_swapchain_khr(swapchain, None);
            present_device.device.destroy_device(None);
        }
        return Err(err);
    }
    release_scene_upload_staging(device, &mut scene_resources);
    let released_resource_payload_bytes = options.storage.release_parsed_resource_payload();
    let released_texture_payload_bytes = options.storage.release_uploaded_texture_payload();
    let semantic_world = RenderingServer::new(&options.storage)
        .semantic_world()
        .expect("scene semantic world was validated during Vulkan GPU setup");
    let semaphore_info = vk::SemaphoreCreateInfo::builder();
    let image_available = match unsafe { device.create_semaphore(&semaphore_info, None) } {
        Ok(semaphore) => semaphore,
        Err(err) => {
            destroy_scene_gpu_resources(device, scene_resources);
            unsafe {
                for view in swapchain_views {
                    device.destroy_image_view(view, None);
                }
                device.destroy_command_pool(command_pool, None);
                device.destroy_swapchain_khr(swapchain, None);
                present_device.device.destroy_device(None);
            }
            return Err(format!(
                "vkCreateSemaphore(image_available vulkanalia scene present): {err:?}"
            ));
        }
    };
    let mut render_finished = Vec::with_capacity(swapchain_images.len());
    for image_index in 0..swapchain_images.len() {
        match unsafe { device.create_semaphore(&semaphore_info, None) } {
            Ok(semaphore) => render_finished.push(semaphore),
            Err(err) => {
                destroy_scene_gpu_resources(device, scene_resources);
                unsafe {
                    for semaphore in render_finished {
                        device.destroy_semaphore(semaphore, None);
                    }
                    device.destroy_semaphore(image_available, None);
                    for view in swapchain_views {
                        device.destroy_image_view(view, None);
                    }
                    device.destroy_command_pool(command_pool, None);
                    device.destroy_swapchain_khr(swapchain, None);
                    present_device.device.destroy_device(None);
                }
                return Err(format!(
                    "vkCreateSemaphore(render_finished image {image_index} vulkanalia scene present): {err:?}"
                ));
            }
        }
    }
    let fence_info = vk::FenceCreateInfo::builder().flags(vk::FenceCreateFlags::SIGNALED);
    let in_flight = match unsafe { device.create_fence(&fence_info, None) } {
        Ok(fence) => fence,
        Err(err) => {
            destroy_scene_gpu_resources(device, scene_resources);
            unsafe {
                for semaphore in render_finished {
                    device.destroy_semaphore(semaphore, None);
                }
                device.destroy_semaphore(image_available, None);
                for view in swapchain_views {
                    device.destroy_image_view(view, None);
                }
                device.destroy_command_pool(command_pool, None);
                device.destroy_swapchain_khr(swapchain, None);
                present_device.device.destroy_device(None);
            }
            return Err(format!("vkCreateFence(vulkanalia scene present): {err:?}"));
        }
    };
    let mut frame_capture = if let Some(path) = options.capture_frame.clone() {
        match SceneFrameCapture::create(
            device,
            &memory_properties,
            path,
            swapchain_plan.extent,
            swapchain_plan.format.format,
            options.capture_frame_number,
            options.capture_frame_count,
            options.capture_frame_step,
            options.capture_frame_downscale,
        ) {
            Ok(capture) => Some(capture),
            Err(err) => {
                unsafe {
                    device.destroy_fence(in_flight, None);
                    for semaphore in render_finished {
                        device.destroy_semaphore(semaphore, None);
                    }
                    device.destroy_semaphore(image_available, None);
                    for view in swapchain_views {
                        device.destroy_image_view(view, None);
                    }
                }
                destroy_scene_gpu_resources(device, scene_resources);
                unsafe {
                    device.destroy_command_pool(command_pool, None);
                    device.destroy_swapchain_khr(swapchain, None);
                    present_device.device.destroy_device(None);
                }
                return Err(err);
            }
        }
    } else {
        None
    };
    let mut gpu_timing = SceneGpuTiming::create(
        device,
        instance,
        selection.physical_device,
        selection.queue_family_index,
        options.gpu_timing,
        &scene_resources.graph_execution_order,
    )?;
    let started_at = Instant::now();
    let deadline = started_at + options.duration;
    let frame_interval = options
        .target_max_fps
        .filter(|fps| *fps > 0)
        .map(|fps| Duration::from_secs_f64(1.0 / fps as f64));
    let mut next_frame = Instant::now();
    let mut frames_presented = 0u64;
    let mut last_present_completed_at = None::<Instant>;
    let mut present_delta_min_micros = None::<u64>;
    let mut present_delta_max_micros = None::<u64>;
    let mut present_delta_over_6250us_count = 0u64;
    let mut present_delta_over_8334us_count = 0u64;
    let mut transform_uniform_update_count = 0u64;
    let mut effect_uniform_update_count = 0u64;
    let mut skinning_storage_update_count = 0u64;
    let mut frame_state_update_total_micros = 0u64;
    let mut sampled_descriptor_update_total_micros = 0u64;
    let mut command_recording_total_micros = 0u64;
    let mut fence_wait_total_micros = 0u64;
    let mut acquire_wait_total_micros = 0u64;
    let mut queue_present_total_micros = 0u64;
    let mut composite_scissor_draw_count = 0usize;
    let mut composite_scissor_covered_pixels = 0u64;
    let mut composite_scissor_avoided_pixels = 0u64;
    let mut image_layouts = vec![vk::ImageLayout::UNDEFINED; swapchain_images.len()];
    let fixed_scene_time_seconds = std::env::var("GILDER_NATIVE_VULKAN_SCENE_FIXED_TIME")
        .ok()
        .and_then(|value| value.parse::<f32>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0);

    while Instant::now() < deadline {
        system_audio_monitor.publish_latest();
        let fence_wait_started = Instant::now();
        unsafe {
            device
                .wait_for_fences(&[in_flight], true, u64::MAX)
                .map_err(|err| format!("vkWaitForFences(vulkanalia scene present): {err:?}"))?;
            device
                .reset_fences(&[in_flight])
                .map_err(|err| format!("vkResetFences(vulkanalia scene present): {err:?}"))?;
        }
        fence_wait_total_micros =
            fence_wait_total_micros.saturating_add(elapsed_micros_u64(fence_wait_started));
        if let Some(timing) = gpu_timing.as_mut() {
            timing.collect_completed(device)?;
        }
        let scene_time_seconds =
            fixed_scene_time_seconds.unwrap_or_else(|| started_at.elapsed().as_secs_f32());
        let frame_state_update_started = Instant::now();
        let frame_update = write_scene_frame_buffers(
            device,
            &options.storage,
            &semantic_world,
            &mut scene_resources.frame_topology,
            &mut scene_resources.draw_commands,
            &scene_resources.transform_buffer,
            scene_resources.material_buffer.as_ref(),
            scene_resources.skinning_buffer.as_ref(),
            scene_resources.dynamic_effect_uniforms,
            scene_time_seconds,
            [swapchain_plan.extent.width, swapchain_plan.extent.height],
        )?;
        frame_state_update_total_micros = frame_state_update_total_micros
            .saturating_add(elapsed_micros_u64(frame_state_update_started));
        composite_scissor_draw_count = scene_resources
            .draw_commands
            .iter()
            .filter(|draw| draw.scissor.is_some())
            .count();
        composite_scissor_covered_pixels = scene_resources
            .draw_commands
            .iter()
            .filter_map(|draw| draw.scissor)
            .map(|scissor| u64::from(scissor.extent[0]) * u64::from(scissor.extent[1]))
            .sum();
        let full_target_pixels = u64::from(swapchain_plan.extent.width)
            * u64::from(swapchain_plan.extent.height)
            * composite_scissor_draw_count as u64;
        composite_scissor_avoided_pixels =
            full_target_pixels.saturating_sub(composite_scissor_covered_pixels);
        transform_uniform_update_count = transform_uniform_update_count
            .saturating_add(u64::from(frame_update.transform_uniform_updated));
        effect_uniform_update_count = effect_uniform_update_count
            .saturating_add(u64::from(frame_update.material_uniform_updated));
        skinning_storage_update_count = skinning_storage_update_count
            .saturating_add(u64::from(frame_update.skinning_storage_updated));
        let reference_phase =
            frames_presented as usize % scene_resources.sampled_binding_cycle.len();
        let acquire_wait_started = Instant::now();
        let (image_index, _) = unsafe {
            device.acquire_next_image_khr(swapchain, u64::MAX, image_available, vk::Fence::null())
        }
        .map_err(|err| format!("vkAcquireNextImageKHR(vulkanalia scene present): {err:?}"))?;
        acquire_wait_total_micros =
            acquire_wait_total_micros.saturating_add(elapsed_micros_u64(acquire_wait_started));
        let image_index = image_index as usize;
        let sampled_descriptor_update_started = Instant::now();
        write_scene_frame_sampled_descriptors(
            device,
            &mut scene_resources,
            reference_phase,
            swapchain_images[image_index],
            swapchain_plan.format.format,
        )?;
        sampled_descriptor_update_total_micros = sampled_descriptor_update_total_micros
            .saturating_add(elapsed_micros_u64(sampled_descriptor_update_started));
        let render_finished = *render_finished.get(image_index).ok_or_else(|| {
            format!("swapchain image index {image_index} has no present semaphore")
        })?;
        let command_buffer = present_command_buffers
            .get(image_index)
            .copied()
            .ok_or_else(|| format!("swapchain image index {image_index} has no command buffer"))?;
        let frame_number = frames_presented.saturating_add(1);
        let capture_this_frame = frame_capture
            .as_ref()
            .is_some_and(|capture| capture.should_capture(frame_number));
        let pending_frame_capture = capture_this_frame.then(|| frame_capture.as_ref()).flatten();

        let command_recording_started = Instant::now();
        record_scene_present_command_buffer(
            device,
            command_buffer,
            swapchain_images[image_index],
            swapchain_views[image_index],
            image_layouts[image_index],
            swapchain_plan.extent,
            options.clear_color,
            &scene_resources,
            reference_phase,
            pending_frame_capture,
            gpu_timing.as_ref(),
        )?;
        command_recording_total_micros = command_recording_total_micros
            .saturating_add(elapsed_micros_u64(command_recording_started));
        image_layouts[image_index] = vk::ImageLayout::PRESENT_SRC_KHR;
        submit_scene_present_command_buffer2(
            device,
            present_device.queue,
            command_buffer,
            image_available,
            render_finished,
            in_flight,
        )?;
        if let Some(timing) = gpu_timing.as_mut() {
            timing.mark_submitted();
        }
        let swapchains = [swapchain];
        let image_indices = [image_index as u32];
        let wait_semaphores = [render_finished];
        let present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(&wait_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);
        let queue_present_started = Instant::now();
        unsafe {
            device
                .queue_present_khr(present_device.queue, &present_info)
                .map_err(|err| format!("vkQueuePresentKHR(vulkanalia scene present): {err:?}"))?;
        }
        queue_present_total_micros = queue_present_total_micros
            .saturating_add(elapsed_micros_u64(queue_present_started));
        if let Some(capture) = capture_this_frame.then(|| frame_capture.as_mut()).flatten() {
            unsafe {
                device
                    .wait_for_fences(&[in_flight], true, u64::MAX)
                    .map_err(|err| {
                        format!("vkWaitForFences(vulkanalia scene frame capture): {err:?}")
                    })?;
            }
            capture.read_completed_frame(device, frame_number)?;
        }
        let present_completed_at = Instant::now();
        if let Some(last_present_completed_at) = last_present_completed_at {
            let delta_micros = present_completed_at
                .duration_since(last_present_completed_at)
                .as_micros()
                .min(u64::MAX as u128) as u64;
            present_delta_min_micros = Some(
                present_delta_min_micros.map_or(delta_micros, |value| value.min(delta_micros)),
            );
            present_delta_max_micros = Some(
                present_delta_max_micros.map_or(delta_micros, |value| value.max(delta_micros)),
            );
            if delta_micros > 6_250 {
                present_delta_over_6250us_count = present_delta_over_6250us_count.saturating_add(1);
            }
            if delta_micros > 8_334 {
                present_delta_over_8334us_count = present_delta_over_8334us_count.saturating_add(1);
            }
        }
        last_present_completed_at = Some(present_completed_at);
        frames_presented += 1;

        if let Some(interval) = frame_interval {
            next_frame += interval;
            let now = Instant::now();
            if next_frame > now {
                thread::sleep(next_frame - now);
            } else {
                next_frame = now;
            }
        }
    }
    let _ = unsafe { device.device_wait_idle() };
    if let Some(timing) = gpu_timing.as_mut() {
        timing.collect_completed(device)?;
    }
    let elapsed = started_at.elapsed();
    let gpu_timing_snapshot = gpu_timing.as_ref().map(SceneGpuTiming::snapshot);
    let frame_capture_write_error = frame_capture
        .as_mut()
        .and_then(|capture| capture.write_png().err());
    let vertex_buffer_bytes = scene_resources.vertex_buffer.snapshot.requested_bytes;
    let index_buffer_bytes = scene_resources.index_buffer.snapshot.requested_bytes;
    let transform_uniform_bytes = scene_resources.transform_buffer.snapshot.requested_bytes;
    let material_uniform_bytes = scene_resources
        .material_buffer
        .as_ref()
        .map_or(0, |buffer| buffer.snapshot.requested_bytes);
    let skinning_storage_bytes = scene_resources
        .skinning_buffer
        .as_ref()
        .map_or(0, |buffer| buffer.snapshot.requested_bytes);
    let resource_residency = resource_residency::scene_resource_residency_snapshot(&scene_resources);
    let sampled_fallback_texture_count = usize::from(scene_resources.white_upload.is_some());
    let sampled_fallback_descriptor_count = scene_resources
        .sampled_binding_cycle
        .first()
        .map_or(0, |plan| plan.fallback_descriptor_count);
    let sampled_scene_texture_descriptor_count = scene_resources
        .sampled_binding_cycle
        .first()
        .map_or(0, |plan| plan.scene_texture_descriptor_count);
    let sampled_scene_color_snapshot_descriptor_count = scene_resources
        .sampled_binding_cycle
        .first()
        .map_or(0, |plan| plan.scene_color_snapshot_descriptor_count);
    let sampled_effect_target_descriptor_count = scene_resources
        .sampled_binding_cycle
        .first()
        .map_or(0, |plan| plan.effect_target_descriptor_count);
    let effect_target_reference_cycle_length = scene_resources.sampled_binding_cycle.len();
    let descriptor_heap_resource_count = scene_resources
        .descriptor_heap_plan
        .resource_descriptor_count;
    let descriptor_heap_sampler_count = scene_resources.descriptor_heap_plan.sampler_count;
    let scene_texture_image_count = scene_resources.scene_textures.len();
    let scene_texture_memory_bytes =
        scene_texture::scene_texture_memory_bytes(&scene_resources.scene_textures);
    let effect_target_physical_image_count = scene_resources.effect_targets.len();
    let effect_target_memory_bytes =
        effect_target::effect_target_memory_bytes(&scene_resources.effect_targets);
    let effect_target_dynamic_rendering_recorded = effect_target_physical_image_count > 0;
    let effect_target_dynamic_rendering_pass_count = scene_resources
        .effect_target_command_plan
        .dynamic_rendering_pass_count;
    let effect_batch_count = scene_resources
        .effect_targets
        .iter()
        .filter(|target| target.plan.batch_field_count > 1)
        .count();
    let effect_batch_instance_count = effect_target::effect_batch_instance_count(
        &scene_resources.effect_target_commands,
    );
    let effect_batch_field_count = scene_resources
        .effect_targets
        .iter()
        .filter(|target| target.plan.batch_field_count > 1)
        .map(|target| target.plan.batch_field_count as usize)
        .sum();
    let effect_target_copy_command_count = scene_resources
        .effect_target_command_plan
        .copy_command_count;
    let effect_target_swap_reference_count = scene_resources
        .effect_target_command_plan
        .swap_reference_command_count;
    let effect_target_mesh_draw_count = scene_resources.effect_target_command_plan.mesh_draw_count;
    let effect_target_discard_load_count = scene_resources
        .effect_target_command_plan
        .discard_load_count;
    let effect_target_fullscreen_draw_count = scene_resources
        .effect_target_command_plan
        .fullscreen_triangle_draw_count;
    let scene_color_mesh_draw_count = draw_range_count(&scene_resources.scene_color_draw_ranges);
    let scene_pipeline_count = scene_resources.pipelines.entries.len();
    let mesh_draw_count = scene_resources.draw_commands.len();
    let alpha_coverage_scissor_draw_count = scene_resources
        .draw_commands
        .iter()
        .filter(|draw| !draw.alpha_coverage_scissors.is_empty())
        .count();
    let alpha_coverage_scissor_segment_count = scene_resources
        .draw_commands
        .iter()
        .map(|draw| draw.alpha_coverage_scissors.len())
        .sum();
    let alpha_coverage_scissor_pixels = scene_resources
        .draw_commands
        .iter()
        .flat_map(|draw| &draw.alpha_coverage_scissors)
        .map(|scissor| u64::from(scissor.extent[0]) * u64::from(scissor.extent[1]))
        .sum();
    let mesh_draw_recorded = mesh_draw_count > 0;
    let capture_scene_graph = scene_resources.capture_scene_graph;
    let frame_capture_requested = frame_capture.is_some();
    let frame_capture_snapshot = frame_capture
        .as_ref()
        .and_then(SceneFrameCapture::snapshot)
        .cloned();
    let command_order = scene_command_order(
        scene_resources.sampled_slots.is_empty(),
        sampled_fallback_texture_count != 0,
        scene_texture_image_count != 0,
        scene_resources.skinning_buffer.is_some(),
        scene_pipeline_count > 1,
        scene_resources.dynamic_effect_uniforms,
        effect_target_dynamic_rendering_recorded,
        effect_target_copy_command_count > 0,
        effect_target_swap_reference_count > 0,
        effect_target_mesh_draw_count > 0,
        effect_target_fullscreen_draw_count > 0,
    );

    unsafe {
        device.destroy_fence(in_flight, None);
        for semaphore in render_finished {
            device.destroy_semaphore(semaphore, None);
        }
        device.destroy_semaphore(image_available, None);
        for view in swapchain_views {
            device.destroy_image_view(view, None);
        }
    }
    if let Some(capture) = frame_capture.take() {
        capture.destroy(device);
    }
    if let Some(timing) = gpu_timing.take() {
        timing.destroy(device);
    }
    destroy_scene_gpu_resources(device, scene_resources);
    unsafe {
        device.destroy_command_pool(command_pool, None);
        device.destroy_swapchain_khr(swapchain, None);
        present_device.device.destroy_device(None);
    }
    if let Some(err) = frame_capture_write_error {
        return Err(err);
    }
    if frame_capture_requested && frame_capture_snapshot.is_none() {
        return Err(
            "scene frame capture requested, but the runtime ended before presenting a frame"
                .to_owned(),
        );
    }

    system_audio_monitor.publish_latest();
    let (audio_spectrum_model, audio_spectrum_ready) = scene_audio_spectrum_status();
    Ok(NativeVulkanVulkanaliaScenePresentSnapshot {
        binding: "vulkanalia",
        route: "scene-mesh-dynamic-rendering-present",
        loader: vulkan.loader_name.to_owned(),
        requested_api_version: Version::V1_4_0.to_string(),
        runtime_elapsed_ms: elapsed.as_millis().min(u64::MAX as u128) as u64,
        frames_presented,
        average_present_fps: if elapsed.is_zero() {
            0.0
        } else {
            frames_presented as f64 / elapsed.as_secs_f64()
        },
        present_delta_min_micros,
        present_delta_max_micros,
        present_delta_over_6250us_count,
        present_delta_over_8334us_count,
        clear_color: options.clear_color,
        capture_scene_graph,
        selected_queue: NativeVulkanVulkanaliaPresentQueueSnapshot {
            physical_device_index: selection.physical_device_index,
            physical_device_name: selection.physical_device_name,
            physical_device_type: selection.physical_device_type,
            queue_family_index: selection.queue_family_index,
            queue_count: selection.queue_count,
            queue_flags: queue_flag_labels(selection.queue_flags),
            supports_graphics: selection.queue_flags.contains(vk::QueueFlags::GRAPHICS),
            supports_present: true,
            supports_wayland_presentation: selection.supports_wayland_presentation,
        },
        device_extensions: present_device.extension_snapshot,
        swapchain: NativeVulkanVulkanaliaSwapchainSnapshot {
            created: true,
            format: format!("{:?}", swapchain_plan.format.format),
            color_space: format!("{:?}", swapchain_plan.format.color_space),
            present_mode: present_mode_label(swapchain_plan.present_mode),
            extent: (swapchain_plan.extent.width, swapchain_plan.extent.height),
            extent_selection: swapchain_plan.extent_selection,
            image_count: swapchain_images.len(),
            min_image_count: swapchain_plan.image_count,
            composite_alpha: composite_alpha_label(swapchain_plan.composite_alpha),
            image_usage: vec![
                "transfer-src",
                "transfer-dst",
                "color-attachment",
                "sampled",
            ],
            create_flags: swapchain_create_flag_labels(swapchain_plan.create_flags),
            present_id2_enabled: swapchain_plan.present_id2_enabled,
            present_wait2_enabled: swapchain_plan.present_wait2_enabled,
        },
        command_submit_model: "acquire_next_image_khr -> cmd_begin_rendering -> scene mesh draw -> queue_submit2 -> queue_present_khr",
        uses_synchronization2: true,
        uses_submit2: true,
        uses_dynamic_rendering: true,
        effect_target_physical_image_count,
        effect_target_memory_bytes,
        effect_target_dynamic_rendering_recorded,
        effect_target_dynamic_rendering_pass_count,
        effect_batch_count,
        effect_batch_instance_count,
        effect_batch_field_count,
        effect_target_copy_command_count,
        effect_target_swap_reference_count,
        effect_target_mesh_draw_count,
        effect_target_discard_load_count,
        scene_color_mesh_draw_count,
        descriptor_model: "VK_EXT_descriptor_heap",
        descriptor_heap_resource_count,
        descriptor_heap_sampler_count,
        vertex_buffer_bytes,
        index_buffer_bytes,
        transform_uniform_bytes,
        material_uniform_bytes,
        audio_spectrum_model,
        audio_spectrum_ready,
        skinning_storage_bytes,
        resource_residency,
        scene_texture_image_count,
        scene_texture_memory_bytes,
        released_resource_payload_bytes,
        released_texture_payload_bytes,
        sampled_fallback_texture_count,
        sampled_fallback_descriptor_count,
        sampled_scene_texture_descriptor_count,
        sampled_scene_color_snapshot_descriptor_count,
        sampled_effect_target_descriptor_count,
        effect_target_reference_cycle_length,
        transform_uniform_update_count,
        effect_uniform_update_count,
        skinning_storage_update_count,
        frame_state_update_total_micros,
        sampled_descriptor_update_total_micros,
        command_recording_total_micros,
        fence_wait_total_micros,
        acquire_wait_total_micros,
        queue_present_total_micros,
        gpu_timing: gpu_timing_snapshot,
        composite_scissor_draw_count,
        composite_scissor_covered_pixels,
        composite_scissor_avoided_pixels,
        alpha_coverage_scissor_draw_count,
        alpha_coverage_scissor_segment_count,
        alpha_coverage_scissor_pixels,
        scene_pipeline_count,
        mesh_draw_count,
        mesh_draw_recorded,
        command_order,
        present_backend: "vulkanalia-scene-present-runtime",
        frame_capture: frame_capture_snapshot,
    })
}

fn create_scene_gpu_resources(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    setup_command_buffer: vk::CommandBuffer,
    storage: &SceneStorage,
    target_format: vk::Format,
    initial_scene_color_image: vk::Image,
    extent: vk::Extent2D,
    descriptor_heap_properties: &crate::renderer::native_vulkan::NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    advanced_blend_enabled: bool,
    advanced_blend_coherent: bool,
    capture_scene_graph: Option<u32>,
) -> Result<SceneGpuResources, String> {
    let backend_plan = native_vulkan_scene_backend_plan(storage);
    if backend_plan.rendering_device_graph.mesh_draws.is_empty() {
        return Err("scene present requires at least one render graph mesh draw".to_owned());
    }
    let descriptor_layout =
        scene_pipeline_descriptor_layout(storage, &backend_plan.rendering_device_graph)?;
    let sampled_binding_cycle = scene_sampled_image_binding_cycle(
        &backend_plan.rendering_device_graph,
        &descriptor_layout.sampled_slots,
    )?;
    let sampled_binding_plan = sampled_binding_cycle
        .first()
        .ok_or_else(|| "scene sampled binding cycle is empty".to_owned())?;
    let effect_target_plans = effect_target::scene_effect_target_image_plan(
        storage,
        &backend_plan.rendering_device_graph,
        target_format,
        extent,
    )?;
    let effect_target_commands =
        effect_target::scene_effect_target_commands(storage, &backend_plan.rendering_device_graph);
    let effect_target_command_plan = effect_target::scene_effect_target_command_plan(
        &effect_target_commands,
        &backend_plan.rendering_device_graph,
    );
    let effect_target_allocations = backend_plan
        .rendering_device_graph
        .target_allocations
        .clone();
    let scene_color_ranges = scene_color_draw_ranges(&backend_plan.rendering_device_graph);
    let graph_execution_order = graph_execution::scene_graph_execution_order(
        &backend_plan.rendering_device_graph,
        capture_scene_graph,
    )?;
    let pipeline_indices = scene_pipeline_indices_for_draws(
        storage,
        &backend_plan.rendering_device_graph,
        target_format,
        &effect_target_plans,
    )?;
    emit_scene_pipeline_diagnostics_if_requested(
        storage,
        &backend_plan.rendering_device_graph,
        target_format,
        &effect_target_plans,
        &pipeline_indices,
    )?;
    let draw_count = backend_plan.rendering_device_graph.mesh_draws.len();
    let include_fullscreen_utility =
        graph_uses_fullscreen_utility_primitive(&backend_plan.rendering_device_graph);
    let alpha_coverage_scissors = if std::env::var_os(
        "GILDER_NATIVE_VULKAN_SCENE_FULL_ALPHA_COVERAGE_TARGET",
    )
    .is_some()
    {
        vec![Vec::new(); draw_count]
    } else {
        scene_alpha_coverage_scissors(
            storage,
            &backend_plan.rendering_device_graph,
            [extent.width, extent.height],
        )
    };
    let vertex_payload = pack_scene_vertices(storage, include_fullscreen_utility);
    let index_payload = pack_scene_indices(storage, include_fullscreen_utility);
    let transform_payload = pack_scene_draw_uniforms(
        storage,
        &backend_plan.rendering_device_graph.mesh_draws,
        0.0,
        [extent.width, extent.height],
    );
    let material_payload = descriptor_layout.material_uniform_enabled.then(|| {
        pack_scene_material_uniforms(
            storage,
            &backend_plan.rendering_device_graph.mesh_draws,
            0.0,
        )
    });
    let dynamic_effect_uniforms =
        backend_plan
            .rendering_device_graph
            .mesh_draws
            .iter()
            .any(|draw| {
                material_parameter_layout(storage, draw.material).uses_dynamic_material_input()
            });
    let skinning_payload = descriptor_layout
        .skinning_storage_enabled
        .then(|| pack_scene_skinning_palette(&backend_plan.rendering_device_graph));
    let frame_topology = SceneFrameTopology::from_graph(&backend_plan.rendering_device_graph);

    let vertex_buffer = native_vulkan_vulkanalia_create_buffer(
        device,
        memory_properties,
        "scene-mesh-vertex-buffer",
        vertex_payload.len() as u64,
        vk::BufferUsageFlags::VERTEX_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
        NativeVulkanVulkanaliaBufferMemoryPreference::HostUpload,
        Some(&vertex_payload),
    )?;
    let index_buffer = match native_vulkan_vulkanalia_create_buffer(
        device,
        memory_properties,
        "scene-mesh-index-buffer",
        index_payload.len() as u64,
        vk::BufferUsageFlags::INDEX_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
        NativeVulkanVulkanaliaBufferMemoryPreference::HostUpload,
        Some(&index_payload),
    ) {
        Ok(buffer) => buffer,
        Err(err) => {
            native_vulkan_vulkanalia_destroy_buffer(device, vertex_buffer);
            return Err(err);
        }
    };
    let transform_buffer = match native_vulkan_vulkanalia_create_buffer(
        device,
        memory_properties,
        "scene-draw-transform-uniform-buffer",
        transform_payload.len() as u64,
        vk::BufferUsageFlags::UNIFORM_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
        NativeVulkanVulkanaliaBufferMemoryPreference::HostUpload,
        Some(&transform_payload),
    ) {
        Ok(buffer) => buffer,
        Err(err) => {
            native_vulkan_vulkanalia_destroy_buffer(device, index_buffer);
            native_vulkan_vulkanalia_destroy_buffer(device, vertex_buffer);
            return Err(err);
        }
    };
    let material_buffer = match material_payload.as_ref() {
        Some(payload) => match native_vulkan_vulkanalia_create_buffer(
            device,
            memory_properties,
            "scene-material-uniform-buffer",
            payload.len() as u64,
            vk::BufferUsageFlags::UNIFORM_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            NativeVulkanVulkanaliaBufferMemoryPreference::HostUpload,
            Some(payload),
        ) {
            Ok(buffer) => Some(buffer),
            Err(err) => {
                native_vulkan_vulkanalia_destroy_buffer(device, transform_buffer);
                native_vulkan_vulkanalia_destroy_buffer(device, index_buffer);
                native_vulkan_vulkanalia_destroy_buffer(device, vertex_buffer);
                return Err(err);
            }
        },
        None => None,
    };
    let skinning_buffer = match skinning_payload.as_ref() {
        Some(payload) => match native_vulkan_vulkanalia_create_buffer(
            device,
            memory_properties,
            "scene-puppet-bone-storage-buffer",
            payload.len() as u64,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            NativeVulkanVulkanaliaBufferMemoryPreference::HostUpload,
            Some(payload),
        ) {
            Ok(buffer) => Some(buffer),
            Err(err) => {
                if let Some(buffer) = material_buffer {
                    native_vulkan_vulkanalia_destroy_buffer(device, buffer);
                }
                native_vulkan_vulkanalia_destroy_buffer(device, transform_buffer);
                native_vulkan_vulkanalia_destroy_buffer(device, index_buffer);
                native_vulkan_vulkanalia_destroy_buffer(device, vertex_buffer);
                return Err(err);
            }
        },
        None => None,
    };

    let white_upload = if sampled_binding_plan.fallback_descriptor_count == 0 {
        None
    } else {
        match create_white_texture_upload(device, memory_properties, setup_command_buffer) {
            Ok(upload) => Some(upload),
            Err(err) => {
                if let Some(buffer) = skinning_buffer {
                    native_vulkan_vulkanalia_destroy_buffer(device, buffer);
                }
                if let Some(buffer) = material_buffer {
                    native_vulkan_vulkanalia_destroy_buffer(device, buffer);
                }
                native_vulkan_vulkanalia_destroy_buffer(device, transform_buffer);
                native_vulkan_vulkanalia_destroy_buffer(device, index_buffer);
                native_vulkan_vulkanalia_destroy_buffer(device, vertex_buffer);
                return Err(err);
            }
        }
    };

    let (resource_descriptors, draw_commands) = scene_descriptor_plan_inputs(
        &backend_plan.rendering_device_graph.mesh_draws,
        &descriptor_layout,
        &pipeline_indices,
        &alpha_coverage_scissors,
    );
    let descriptor_heap_plan = native_vulkan_vulkanalia_descriptor_heap_resource_plan(
        NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput {
            resource_descriptors,
            sampler_count: descriptor_layout
                .sampled_slots
                .len()
                .saturating_mul(draw_count),
            properties: *descriptor_heap_properties,
        },
    );
    if !descriptor_heap_plan.backend_ready {
        let err = format!(
            "scene descriptor heap plan is not ready: {:?}",
            descriptor_heap_plan.blocking_reason
        );
        if let Some(upload) = white_upload {
            destroy_recorded_image_upload(device, upload);
        }
        if let Some(buffer) = material_buffer {
            native_vulkan_vulkanalia_destroy_buffer(device, buffer);
        }
        if let Some(buffer) = skinning_buffer {
            native_vulkan_vulkanalia_destroy_buffer(device, buffer);
        }
        native_vulkan_vulkanalia_destroy_buffer(device, transform_buffer);
        native_vulkan_vulkanalia_destroy_buffer(device, index_buffer);
        native_vulkan_vulkanalia_destroy_buffer(device, vertex_buffer);
        return Err(err);
    }
    let mut descriptor_heap =
        match native_vulkan_vulkanalia_create_descriptor_heap_resource_resources(
            device,
            memory_properties,
            &descriptor_heap_plan,
        ) {
            Ok(resources) => resources,
            Err(err) => {
                if let Some(upload) = white_upload {
                    destroy_recorded_image_upload(device, upload);
                }
                if let Some(buffer) = material_buffer {
                    native_vulkan_vulkanalia_destroy_buffer(device, buffer);
                }
                if let Some(buffer) = skinning_buffer {
                    native_vulkan_vulkanalia_destroy_buffer(device, buffer);
                }
                native_vulkan_vulkanalia_destroy_buffer(device, transform_buffer);
                native_vulkan_vulkanalia_destroy_buffer(device, index_buffer);
                native_vulkan_vulkanalia_destroy_buffer(device, vertex_buffer);
                return Err(err);
            }
        };

    let effect_targets = match effect_target::create_scene_effect_target_images(
        device,
        memory_properties,
        &effect_target_plans,
    ) {
        Ok(targets) => targets,
        Err(err) => {
            native_vulkan_vulkanalia_destroy_descriptor_heap_resource_resources(
                device,
                descriptor_heap,
            );
            if let Some(upload) = white_upload {
                destroy_recorded_image_upload(device, upload);
            }
            if let Some(buffer) = material_buffer {
                native_vulkan_vulkanalia_destroy_buffer(device, buffer);
            }
            if let Some(buffer) = skinning_buffer {
                native_vulkan_vulkanalia_destroy_buffer(device, buffer);
            }
            native_vulkan_vulkanalia_destroy_buffer(device, transform_buffer);
            native_vulkan_vulkanalia_destroy_buffer(device, index_buffer);
            native_vulkan_vulkanalia_destroy_buffer(device, vertex_buffer);
            return Err(err);
        }
    };
    effect_target::record_scene_effect_target_initial_layouts(
        device,
        setup_command_buffer,
        &effect_targets,
    );
    let scene_textures = match scene_texture::create_scene_texture_images(
        device,
        memory_properties,
        setup_command_buffer,
        storage,
        &sampled_binding_cycle,
    ) {
        Ok(textures) => textures,
        Err(err) => {
            effect_target::destroy_scene_effect_target_images(device, effect_targets);
            native_vulkan_vulkanalia_destroy_descriptor_heap_resource_resources(
                device,
                descriptor_heap,
            );
            if let Some(upload) = white_upload {
                destroy_recorded_image_upload(device, upload);
            }
            if let Some(buffer) = material_buffer {
                native_vulkan_vulkanalia_destroy_buffer(device, buffer);
            }
            if let Some(buffer) = skinning_buffer {
                native_vulkan_vulkanalia_destroy_buffer(device, buffer);
            }
            native_vulkan_vulkanalia_destroy_buffer(device, transform_buffer);
            native_vulkan_vulkanalia_destroy_buffer(device, index_buffer);
            native_vulkan_vulkanalia_destroy_buffer(device, vertex_buffer);
            return Err(err);
        }
    };

    if let Err(err) = write_scene_descriptors(
        device,
        &mut descriptor_heap,
        &draw_commands,
        &transform_buffer,
        material_buffer.as_ref(),
        skinning_buffer.as_ref(),
        white_upload.as_ref().map(|upload| &upload.image),
        &scene_textures,
        &effect_targets,
        sampled_binding_plan,
        Some((initial_scene_color_image, target_format)),
    ) {
        scene_texture::destroy_scene_texture_images(device, scene_textures);
        effect_target::destroy_scene_effect_target_images(device, effect_targets);
        native_vulkan_vulkanalia_destroy_descriptor_heap_resource_resources(
            device,
            descriptor_heap,
        );
        if let Some(upload) = white_upload {
            destroy_recorded_image_upload(device, upload);
        }
        if let Some(buffer) = material_buffer {
            native_vulkan_vulkanalia_destroy_buffer(device, buffer);
        }
        if let Some(buffer) = skinning_buffer {
            native_vulkan_vulkanalia_destroy_buffer(device, buffer);
        }
        native_vulkan_vulkanalia_destroy_buffer(device, transform_buffer);
        native_vulkan_vulkanalia_destroy_buffer(device, index_buffer);
        native_vulkan_vulkanalia_destroy_buffer(device, vertex_buffer);
        return Err(err);
    }
    let pipeline_resources = match create_scene_pipelines(
        device,
        target_format,
        extent,
        storage,
        &backend_plan.rendering_device_graph,
        &descriptor_heap_plan,
        &descriptor_layout,
        &effect_target_plans,
        advanced_blend_enabled,
        advanced_blend_coherent,
    ) {
        Ok(resources) => resources,
        Err(err) => {
            scene_texture::destroy_scene_texture_images(device, scene_textures);
            effect_target::destroy_scene_effect_target_images(device, effect_targets);
            native_vulkan_vulkanalia_destroy_descriptor_heap_resource_resources(
                device,
                descriptor_heap,
            );
            if let Some(upload) = white_upload {
                destroy_recorded_image_upload(device, upload);
            }
            if let Some(buffer) = material_buffer {
                native_vulkan_vulkanalia_destroy_buffer(device, buffer);
            }
            if let Some(buffer) = skinning_buffer {
                native_vulkan_vulkanalia_destroy_buffer(device, buffer);
            }
            native_vulkan_vulkanalia_destroy_buffer(device, transform_buffer);
            native_vulkan_vulkanalia_destroy_buffer(device, index_buffer);
            native_vulkan_vulkanalia_destroy_buffer(device, vertex_buffer);
            return Err(err);
        }
    };

    Ok(SceneGpuResources {
        vertex_buffer,
        index_buffer,
        transform_buffer,
        material_buffer,
        skinning_buffer,
        white_upload,
        scene_textures,
        effect_targets,
        effect_target_command_plan,
        effect_target_commands,
        effect_target_allocations,
        pass_nodes: backend_plan.rendering_device_graph.pass_nodes.clone(),
        scene_color_draw_ranges: scene_color_ranges,
        graph_execution_order,
        capture_scene_graph,
        descriptor_heap,
        descriptor_heap_plan,
        pipelines: pipeline_resources,
        draw_commands,
        sampled_slots: descriptor_layout.sampled_slots,
        sampled_binding_cycle,
        material_uniform_enabled: descriptor_layout.material_uniform_enabled,
        frame_topology,
        dynamic_effect_uniforms,
    })
}

fn write_scene_descriptors(
    device: &Device,
    descriptor_heap: &mut VulkanaliaDescriptorHeapResourceResources,
    draw_commands: &[SceneGpuDrawCommand],
    transform_buffer: &NativeVulkanVulkanaliaBuffer,
    material_buffer: Option<&NativeVulkanVulkanaliaBuffer>,
    skinning_buffer: Option<&NativeVulkanVulkanaliaBuffer>,
    white_image: Option<&NativeVulkanVulkanaliaImage>,
    scene_textures: &[scene_texture::SceneTextureImageResource],
    effect_targets: &[effect_target::SceneEffectTargetImageResource],
    sampled_binding_plan: &SceneSampledImageBindingPlan,
    scene_color: Option<(vk::Image, vk::Format)>,
) -> Result<(), String> {
    for (draw_index, draw) in draw_commands.iter().enumerate() {
        native_vulkan_vulkanalia_write_descriptor_heap_resource_uniform_buffer(
            device,
            descriptor_heap,
            draw.resource_descriptor_base,
            transform_buffer
                .device_address
                .saturating_add(draw_index as u64 * SCENE_DRAW_UNIFORM_BYTES),
            SCENE_DRAW_UNIFORM_BYTES,
        )?;
        let mut resource_descriptor_index = draw.resource_descriptor_base + 1;
        if let Some(material_buffer) = material_buffer {
            native_vulkan_vulkanalia_write_descriptor_heap_resource_uniform_buffer(
                device,
                descriptor_heap,
                resource_descriptor_index,
                material_buffer
                    .device_address
                    .saturating_add(draw_index as u64 * SCENE_MATERIAL_UNIFORM_BYTES),
                SCENE_MATERIAL_UNIFORM_BYTES,
            )?;
            resource_descriptor_index += 1;
        }
        if let Some(skinning_buffer) = skinning_buffer {
            native_vulkan_vulkanalia_write_descriptor_heap_resource_storage_buffer(
                device,
                descriptor_heap,
                resource_descriptor_index,
                skinning_buffer
                    .device_address
                    .saturating_add(draw.skinning_byte_offset),
                draw.skinning_byte_count,
            )?;
        }
    }
    write_scene_sampled_descriptors(
        device,
        descriptor_heap,
        draw_commands,
        white_image,
        scene_textures,
        effect_targets,
        sampled_binding_plan,
        scene_color,
        material_buffer.is_some(),
        skinning_buffer.is_some(),
    )
}

fn write_scene_frame_sampled_descriptors(
    device: &Device,
    scene: &mut SceneGpuResources,
    reference_phase: usize,
    scene_color_image: vk::Image,
    scene_color_format: vk::Format,
) -> Result<(), String> {
    let sampled_binding_plan = scene
        .sampled_binding_cycle
        .get(reference_phase)
        .ok_or_else(|| format!("scene sampled binding phase {reference_phase} is missing"))?;
    write_scene_sampled_descriptors(
        device,
        &mut scene.descriptor_heap,
        &scene.draw_commands,
        scene.white_upload.as_ref().map(|upload| &upload.image),
        &scene.scene_textures,
        &scene.effect_targets,
        sampled_binding_plan,
        Some((scene_color_image, scene_color_format)),
        scene.material_uniform_enabled,
        scene.skinning_buffer.is_some(),
    )
}

fn write_scene_sampled_descriptors(
    device: &Device,
    descriptor_heap: &mut VulkanaliaDescriptorHeapResourceResources,
    draw_commands: &[SceneGpuDrawCommand],
    white_image: Option<&NativeVulkanVulkanaliaImage>,
    scene_textures: &[scene_texture::SceneTextureImageResource],
    effect_targets: &[effect_target::SceneEffectTargetImageResource],
    sampled_binding_plan: &SceneSampledImageBindingPlan,
    scene_color: Option<(vk::Image, vk::Format)>,
    material_uniform_enabled: bool,
    skinning_storage_enabled: bool,
) -> Result<(), String> {
    let fallback_image_view_info = white_image.map(scene_white_image_view_info);
    let fallback_sampler_info = scene_sampled_sampler_info();
    for (draw_index, draw) in draw_commands.iter().enumerate() {
        let resource_descriptor_index = draw.resource_descriptor_base
            + 1
            + usize::from(material_uniform_enabled)
            + usize::from(skinning_storage_enabled);
        for sampled_index in 0..sampled_binding_plan.sampled_slot_count {
            let source = sampled_binding_plan
                .source(draw_index, sampled_index)
                .ok_or_else(|| {
                    format!(
                        "scene draw {draw_index} sampled descriptor {sampled_index} has no binding plan"
                    )
                })?;
            let (image_view_info, sampler_info) = match source {
                SceneSampledImageSource::FallbackWhite => (
                    fallback_image_view_info.ok_or_else(|| {
                        "scene fallback sampled binding has no fallback texture".to_owned()
                    })?,
                    fallback_sampler_info,
                ),
                SceneSampledImageSource::SceneTexture { resource } => {
                    let texture = scene_texture::scene_texture_image(scene_textures, resource)
                        .ok_or_else(|| {
                            format!(
                                "scene sampled texture resource {} has no GPU image",
                                resource.0
                            )
                        })?;
                    (
                        scene_texture::scene_texture_image_view_info(texture),
                        scene_texture::scene_texture_sampler_info(texture),
                    )
                }
                SceneSampledImageSource::SceneColorSnapshot => {
                    let (image, format) = scene_color.ok_or_else(|| {
                        "scene color snapshot descriptor is unavailable before image acquire"
                            .to_owned()
                    })?;
                    (scene_color_image_view_info(image, format), fallback_sampler_info)
                }
                SceneSampledImageSource::EffectTarget {
                    physical_slot,
                    batch_atlas_tile,
                } => {
                    let resource = effect_targets
                        .iter()
                        .find(|resource| resource.plan.physical_slot == physical_slot)
                        .ok_or_else(|| {
                            format!(
                                "scene sampled effect target physical slot {physical_slot} has no image"
                            )
                        })?;
                    (
                        effect_target::effect_target_sampled_image_view_info(
                            resource,
                            batch_atlas_tile,
                        ),
                        fallback_sampler_info,
                    )
                }
            };
            native_vulkan_vulkanalia_write_descriptor_heap_resource_image_sampler(
                device,
                descriptor_heap,
                resource_descriptor_index + sampled_index,
                draw.sampler_descriptor_base + sampled_index,
                &image_view_info,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                &sampler_info,
            )?;
        }
    }
    Ok(())
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
mod tests {
    use super::*;
    use crate::engine::scene::{
        INVALID_MATERIAL_ID, SceneMaterialHandle, SceneObjectHandle,
        SceneRenderingDeviceDrawPrimitive,
    };

    #[test]
    fn automatic_surface_extent_prefers_authored_scene_pixels() {
        assert_eq!(
            scene_viewport::automatic_scene_surface_extent((3840, 2160), (2561, 1440)),
            (3840, 2160)
        );
        assert_eq!(
            scene_viewport::automatic_scene_surface_extent((0, 0), (2561, 1440)),
            (2561, 1440)
        );
    }

    #[test]
    fn descriptor_plan_adds_skinning_storage_buffer_after_uniforms() {
        let draw = SceneRenderingDeviceMeshDraw {
            primitive: SceneRenderingDeviceDrawPrimitive::ObjectMesh,
            mesh_index: 0,
            resolved_object_index: 0,
            clip_transform: [[0.0; 4]; 4],
            authored_source_extent: [0.0; 2],
            skinning_palette_start: 2,
            skinning_palette_count: 3,
            resolved_color: crate::engine::scene::SceneVec3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            resolved_alpha: 1.0,
            apply_resolved_visual: true,
            effect_batch_atlas_tile: u32::MAX,
            effect_batch_atlas_grid: [0; 2],
            object: SceneObjectHandle(0),
            material: SceneMaterialHandle(INVALID_MATERIAL_ID),
            vertex_start: 0,
            vertex_count: 4,
            index_start: 0,
            index_count: 6,
        };
        let layout = pipeline::ScenePipelineDescriptorLayout {
            sampled_slots: Vec::new(),
            material_uniform_enabled: true,
            skinning_storage_enabled: true,
        };

        let (descriptors, commands) = scene_descriptor_plan_inputs(
            &[draw],
            &layout,
            &[2],
            &[Vec::new()],
        );

        assert_eq!(
            descriptors,
            vec![
                NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::StorageBuffer,
            ]
        );
        assert_eq!(
            commands[0].skinning_byte_offset,
            3 * NATIVE_VULKAN_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES as u64
        );
        assert_eq!(
            commands[0].skinning_byte_count,
            3 * NATIVE_VULKAN_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES as u64
        );
        assert_eq!(commands[0].pipeline_index, 2);
    }
}
