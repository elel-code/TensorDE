//! Vulkanalia scene mesh present runtime.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/global-uniforms.md`
//! - `reverse-engineered/docs/shader-conventions.md`
//! - `references/godot/servers/rendering/rendering_device_graph.*`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.*`

use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use vulkanalia::Version;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{
    self, HasBuilder, KhrSurfaceExtensionInstanceCommands, KhrSwapchainExtensionDeviceCommands,
};

use crate::engine::scene::{SceneRenderingDeviceMeshDraw, SceneStorage};
use crate::renderer::native_vulkan::{
    NativeVulkanClearColor, NativeVulkanVulkanaliaBuffer,
    NativeVulkanVulkanaliaBufferMemoryPreference,
    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind,
    NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput,
    NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot, NativeVulkanVulkanaliaImage,
    NativeVulkanVulkanaliaImageMipUpload, NativeVulkanVulkanaliaPresentDeviceExtensionSnapshot,
    NativeVulkanVulkanaliaPresentQueueSnapshot, NativeVulkanVulkanaliaRecordedImageUpload,
    NativeVulkanVulkanaliaSwapchainSnapshot, VulkanaliaDescriptorHeapResourceResources,
    native_vulkan_scene_backend_plan, native_vulkan_vulkanalia_create_buffer,
    native_vulkan_vulkanalia_create_descriptor_heap_resource_resources,
    native_vulkan_vulkanalia_create_sampled_image_with_recorded_staging_upload,
    native_vulkan_vulkanalia_descriptor_heap_resource_plan,
    native_vulkan_vulkanalia_destroy_buffer,
    native_vulkan_vulkanalia_destroy_descriptor_heap_resource_resources,
    native_vulkan_vulkanalia_destroy_image,
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
mod draw_recording;
mod effect_target;
mod fullscreen_primitive;
mod material_uniform;
mod pipeline;
mod sampled_binding;

use command_order::scene_command_order;
use draw_recording::{
    SceneGpuDrawCommand, SceneGpuDrawRange, draw_range_count, record_scene_draw_extent,
    record_scene_mesh_draw_ranges, scene_color_draw_ranges,
};
use fullscreen_primitive::{
    append_fullscreen_triangle_indices, append_fullscreen_triangle_vertices,
    graph_uses_fullscreen_utility_primitive,
};
use material_uniform::pack_scene_material_uniforms;
use pipeline::{
    ScenePipelineResources, create_scene_pipelines, destroy_scene_pipelines,
    scene_pipeline_descriptor_layout, scene_pipeline_indices_for_draws,
};
use sampled_binding::{
    SceneSampledImageBindingPlan, SceneSampledImageSource, scene_sampled_image_binding_cycle,
};

const SCENE_MESH_VERTEX_STRIDE_BYTES: u32 = 20;
const SCENE_DRAW_TRANSFORM_BYTES: u64 = 64;
const SCENE_MATERIAL_UNIFORM_BYTES: u64 = 48;
const SCENE_PUPPET_BONE_MATRIX_BYTES: u64 = 64;
const SCENE_WHITE_TEXTURE_BYTES: &[u8] = &[255, 255, 255, 255];

#[derive(Debug, Clone, PartialEq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanVulkanaliaScenePresentOptions {
    pub host: NativeWaylandHostOptions,
    pub wait_configure_roundtrips: usize,
    pub duration: Duration,
    pub target_max_fps: Option<u32>,
    pub clear_color: NativeVulkanClearColor,
    pub storage: SceneStorage,
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
    pub effect_target_physical_image_count: usize,
    pub effect_target_memory_bytes: u64,
    pub effect_target_dynamic_rendering_recorded: bool,
    pub effect_target_copy_command_count: usize,
    pub effect_target_swap_reference_count: usize,
    pub effect_target_mesh_draw_count: usize,
    pub scene_color_mesh_draw_count: usize,
    pub descriptor_model: &'static str,
    pub descriptor_heap_resource_count: usize,
    pub descriptor_heap_sampler_count: usize,
    pub vertex_buffer_bytes: u64,
    pub index_buffer_bytes: u64,
    pub transform_uniform_bytes: u64,
    pub material_uniform_bytes: u64,
    pub skinning_storage_bytes: u64,
    pub sampled_fallback_texture_count: usize,
    pub sampled_fallback_descriptor_count: usize,
    pub sampled_effect_target_descriptor_count: usize,
    pub effect_target_reference_cycle_length: usize,
    pub scene_pipeline_count: usize,
    pub mesh_draw_count: usize,
    pub mesh_draw_recorded: bool,
    pub command_order: Vec<&'static str>,
    pub present_backend: &'static str,
}

struct SceneGpuResources {
    vertex_buffer: NativeVulkanVulkanaliaBuffer,
    index_buffer: NativeVulkanVulkanaliaBuffer,
    transform_buffer: NativeVulkanVulkanaliaBuffer,
    material_buffer: Option<NativeVulkanVulkanaliaBuffer>,
    skinning_buffer: Option<NativeVulkanVulkanaliaBuffer>,
    white_upload: Option<NativeVulkanVulkanaliaRecordedImageUpload>,
    effect_targets: Vec<effect_target::SceneEffectTargetImageResource>,
    effect_target_command_plan: effect_target::SceneEffectTargetCommandPlan,
    effect_target_commands: Vec<effect_target::SceneEffectTargetCommand>,
    effect_target_allocations: Vec<crate::engine::scene::SceneRenderingDeviceTargetAllocation>,
    scene_color_draw_ranges: Vec<SceneGpuDrawRange>,
    descriptor_heap: VulkanaliaDescriptorHeapResourceResources,
    descriptor_heap_plan: NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    pipelines: ScenePipelineResources,
    draw_commands: Vec<SceneGpuDrawCommand>,
    sampled_slots: Vec<u32>,
    sampled_binding_cycle: Vec<SceneSampledImageBindingPlan>,
    material_uniform_enabled: bool,
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
    options: NativeVulkanVulkanaliaScenePresentOptions,
) -> Result<NativeVulkanVulkanaliaScenePresentSnapshot, String> {
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
        return Err(
            "Vulkanalia scene present requires synchronization2 for QueueSubmit2".to_owned(),
        );
    }
    if !present_device.feature_selection.dynamic_rendering_enabled {
        unsafe {
            present_device.device.destroy_device(None);
        }
        return Err("Vulkanalia scene present requires dynamic rendering".to_owned());
    }
    if !present_device
        .feature_selection
        .core_features
        .descriptor_heap
    {
        unsafe {
            present_device.device.destroy_device(None);
        }
        return Err("Vulkanalia scene present requires VK_EXT_descriptor_heap".to_owned());
    }

    let swapchain_plan = match create_vulkanalia_swapchain_plan(
        instance,
        selection.physical_device,
        surface,
        handles.buffer_size,
        vulkanalia_surface_capabilities2_enabled(vulkan),
        &present_device.feature_selection,
        options.target_max_fps.is_none(),
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
        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
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
        swapchain_plan.extent,
        &present_device.feature_selection.descriptor_heap_properties,
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
    let mut image_layouts = vec![vk::ImageLayout::UNDEFINED; swapchain_images.len()];

    while Instant::now() < deadline {
        unsafe {
            device
                .wait_for_fences(&[in_flight], true, u64::MAX)
                .map_err(|err| format!("vkWaitForFences(vulkanalia scene present): {err:?}"))?;
            device
                .reset_fences(&[in_flight])
                .map_err(|err| format!("vkResetFences(vulkanalia scene present): {err:?}"))?;
        }
        let reference_phase = frames_presented as usize % scene_resources.sampled_binding_cycle.len();
        write_scene_frame_sampled_descriptors(device, &mut scene_resources, reference_phase)?;
        let (image_index, _) = unsafe {
            device.acquire_next_image_khr(swapchain, u64::MAX, image_available, vk::Fence::null())
        }
        .map_err(|err| format!("vkAcquireNextImageKHR(vulkanalia scene present): {err:?}"))?;
        let image_index = image_index as usize;
        let render_finished = *render_finished.get(image_index).ok_or_else(|| {
            format!("swapchain image index {image_index} has no present semaphore")
        })?;
        let command_buffer = present_command_buffers
            .get(image_index)
            .copied()
            .ok_or_else(|| format!("swapchain image index {image_index} has no command buffer"))?;

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
        )?;
        image_layouts[image_index] = vk::ImageLayout::PRESENT_SRC_KHR;
        submit_scene_present_command_buffer2(
            device,
            present_device.queue,
            command_buffer,
            image_available,
            render_finished,
            in_flight,
        )?;
        let swapchains = [swapchain];
        let image_indices = [image_index as u32];
        let wait_semaphores = [render_finished];
        let present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(&wait_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);
        unsafe {
            device
                .queue_present_khr(present_device.queue, &present_info)
                .map_err(|err| format!("vkQueuePresentKHR(vulkanalia scene present): {err:?}"))?;
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
    let elapsed = started_at.elapsed();
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
    let sampled_fallback_texture_count = usize::from(scene_resources.white_upload.is_some());
    let sampled_fallback_descriptor_count = scene_resources
        .sampled_binding_cycle
        .first()
        .map_or(0, |plan| plan.fallback_descriptor_count);
    let sampled_effect_target_descriptor_count = scene_resources
        .sampled_binding_cycle
        .first()
        .map_or(0, |plan| plan.effect_target_descriptor_count);
    let effect_target_reference_cycle_length = scene_resources.sampled_binding_cycle.len();
    let descriptor_heap_resource_count = scene_resources
        .descriptor_heap_plan
        .resource_descriptor_count;
    let descriptor_heap_sampler_count = scene_resources.descriptor_heap_plan.sampler_count;
    let effect_target_physical_image_count = scene_resources.effect_targets.len();
    let effect_target_memory_bytes =
        effect_target::effect_target_memory_bytes(&scene_resources.effect_targets);
    let effect_target_dynamic_rendering_recorded = effect_target_physical_image_count > 0;
    let effect_target_copy_command_count =
        scene_resources.effect_target_command_plan.copy_command_count;
    let effect_target_swap_reference_count = scene_resources
        .effect_target_command_plan
        .swap_reference_command_count;
    let effect_target_mesh_draw_count = scene_resources
        .effect_target_command_plan
        .mesh_draw_count;
    let effect_target_fullscreen_draw_count = scene_resources
        .effect_target_command_plan
        .fullscreen_triangle_draw_count;
    let scene_color_mesh_draw_count =
        draw_range_count(&scene_resources.scene_color_draw_ranges);
    let scene_pipeline_count = scene_resources.pipelines.entries.len();
    let mesh_draw_count = scene_resources.draw_commands.len();
    let mesh_draw_recorded = mesh_draw_count > 0;
    let command_order = scene_command_order(
        scene_resources.sampled_slots.is_empty(),
        scene_resources.skinning_buffer.is_some(),
        scene_pipeline_count > 1,
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
    destroy_scene_gpu_resources(device, scene_resources);
    unsafe {
        device.destroy_command_pool(command_pool, None);
        device.destroy_swapchain_khr(swapchain, None);
        present_device.device.destroy_device(None);
    }

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
            image_usage: vec!["transfer-src", "transfer-dst", "color-attachment"],
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
        effect_target_copy_command_count,
        effect_target_swap_reference_count,
        effect_target_mesh_draw_count,
        scene_color_mesh_draw_count,
        descriptor_model: "VK_EXT_descriptor_heap",
        descriptor_heap_resource_count,
        descriptor_heap_sampler_count,
        vertex_buffer_bytes,
        index_buffer_bytes,
        transform_uniform_bytes,
        material_uniform_bytes,
        skinning_storage_bytes,
        sampled_fallback_texture_count,
        sampled_fallback_descriptor_count,
        sampled_effect_target_descriptor_count,
        effect_target_reference_cycle_length,
        scene_pipeline_count,
        mesh_draw_count,
        mesh_draw_recorded,
        command_order,
        present_backend: "vulkanalia-scene-present-runtime",
    })
}

fn create_scene_gpu_resources(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    setup_command_buffer: vk::CommandBuffer,
    storage: &SceneStorage,
    target_format: vk::Format,
    extent: vk::Extent2D,
    descriptor_heap_properties: &crate::renderer::native_vulkan::NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
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
    let effect_target_commands = effect_target::scene_effect_target_commands(
        storage,
        &backend_plan.rendering_device_graph,
    );
    let effect_target_command_plan = effect_target::scene_effect_target_command_plan(
        &effect_target_commands,
        &backend_plan.rendering_device_graph,
    );
    let effect_target_allocations = backend_plan.rendering_device_graph.target_allocations.clone();
    let scene_color_ranges = scene_color_draw_ranges(&backend_plan.rendering_device_graph);
    let pipeline_indices = scene_pipeline_indices_for_draws(
        storage,
        &backend_plan.rendering_device_graph,
        target_format,
        &effect_target_plans,
    )?;
    let draw_count = backend_plan.rendering_device_graph.mesh_draws.len();
    let include_fullscreen_utility =
        graph_uses_fullscreen_utility_primitive(&backend_plan.rendering_device_graph);
    let vertex_payload = pack_scene_vertices(storage, include_fullscreen_utility);
    let index_payload = pack_scene_indices(storage, include_fullscreen_utility);
    let transform_payload = pack_scene_transforms(&backend_plan.rendering_device_graph.mesh_draws);
    let material_payload = descriptor_layout
        .material_uniform_enabled
        .then(|| {
            pack_scene_material_uniforms(storage, &backend_plan.rendering_device_graph.mesh_draws)
        });
    let skinning_payload = descriptor_layout
        .skinning_storage_enabled
        .then(|| pack_scene_skinning_matrices(&backend_plan.rendering_device_graph));

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

    if let Err(err) = write_scene_descriptors(
        device,
        &mut descriptor_heap,
        &draw_commands,
        &transform_buffer,
        material_buffer.as_ref(),
        skinning_buffer.as_ref(),
        white_upload.as_ref().map(|upload| &upload.image),
        &effect_targets,
        sampled_binding_plan,
    ) {
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
    ) {
        Ok(resources) => resources,
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

    Ok(SceneGpuResources {
        vertex_buffer,
        index_buffer,
        transform_buffer,
        material_buffer,
        skinning_buffer,
        white_upload,
        effect_targets,
        effect_target_command_plan,
        effect_target_commands,
        effect_target_allocations,
        scene_color_draw_ranges: scene_color_ranges,
        descriptor_heap,
        descriptor_heap_plan,
        pipelines: pipeline_resources,
        draw_commands,
        sampled_slots: descriptor_layout.sampled_slots,
        sampled_binding_cycle,
        material_uniform_enabled: descriptor_layout.material_uniform_enabled,
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
    effect_targets: &[effect_target::SceneEffectTargetImageResource],
    sampled_binding_plan: &SceneSampledImageBindingPlan,
) -> Result<(), String> {
    for (draw_index, draw) in draw_commands.iter().enumerate() {
        native_vulkan_vulkanalia_write_descriptor_heap_resource_uniform_buffer(
            device,
            descriptor_heap,
            draw.resource_descriptor_base,
            transform_buffer
                .device_address
                .saturating_add(draw_index as u64 * SCENE_DRAW_TRANSFORM_BYTES),
            SCENE_DRAW_TRANSFORM_BYTES,
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
        effect_targets,
        sampled_binding_plan,
        material_buffer.is_some(),
        skinning_buffer.is_some(),
    )
}

fn write_scene_frame_sampled_descriptors(
    device: &Device,
    scene: &mut SceneGpuResources,
    reference_phase: usize,
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
        &scene.effect_targets,
        sampled_binding_plan,
        scene.material_uniform_enabled,
        scene.skinning_buffer.is_some(),
    )
}

fn write_scene_sampled_descriptors(
    device: &Device,
    descriptor_heap: &mut VulkanaliaDescriptorHeapResourceResources,
    draw_commands: &[SceneGpuDrawCommand],
    white_image: Option<&NativeVulkanVulkanaliaImage>,
    effect_targets: &[effect_target::SceneEffectTargetImageResource],
    sampled_binding_plan: &SceneSampledImageBindingPlan,
    material_uniform_enabled: bool,
    skinning_storage_enabled: bool,
) -> Result<(), String> {
    let fallback_image_view_info = white_image.map(scene_white_image_view_info);
    let sampler_info = scene_sampled_sampler_info();
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
            let image_view_info = match source {
                SceneSampledImageSource::FallbackWhite => fallback_image_view_info.ok_or_else(|| {
                    "scene fallback sampled binding has no fallback texture".to_owned()
                })?,
                SceneSampledImageSource::EffectTarget { physical_slot } => {
                    let resource = effect_targets
                        .iter()
                        .find(|resource| resource.plan.physical_slot == physical_slot)
                        .ok_or_else(|| {
                            format!(
                                "scene sampled effect target physical slot {physical_slot} has no image"
                            )
                        })?;
                    effect_target::effect_target_sampled_image_view_info(resource)
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

        let mut record_effect_draws = |draw_start, draw_count| {
            record_scene_mesh_draw_ranges(
                device,
                command_buffer,
                scene,
                &[SceneGpuDrawRange {
                    start: draw_start,
                    count: draw_count,
                }],
            )
        };
        effect_target::record_scene_effect_target_passes(
            device,
            command_buffer,
            &scene.effect_target_commands,
            &scene.effect_target_allocations,
            &scene
                .sampled_binding_cycle
                .get(reference_phase)
                .ok_or_else(|| {
                    format!("scene sampled binding phase {reference_phase} is missing")
                })?
                .initial_reference_physical_slots,
            &scene.effect_targets,
            &mut record_effect_draws,
        )?;

        let to_attachment = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(match old_layout {
                vk::ImageLayout::UNDEFINED => vk::PipelineStageFlags2::TOP_OF_PIPE,
                _ => vk::PipelineStageFlags2::ALL_COMMANDS,
            })
            .src_access_mask(vk::AccessFlags2::empty())
            .dst_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .old_layout(old_layout)
            .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(swapchain_image)
            .subresource_range(color_subresource_range())
            .build();
        let to_attachment_barriers = [to_attachment];
        let to_attachment_dependency = vk::DependencyInfo::builder()
            .image_memory_barriers(&to_attachment_barriers)
            .build();
        device.cmd_pipeline_barrier2(command_buffer, &to_attachment_dependency);

        let clear_value = vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [clear_color.r, clear_color.g, clear_color.b, clear_color.a],
            },
        };
        let color_attachment = vk::RenderingAttachmentInfo::builder()
            .image_view(swapchain_view)
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(clear_value)
            .build();
        let color_attachments = [color_attachment];
        let render_area = vk::Rect2D::builder()
            .offset(vk::Offset2D { x: 0, y: 0 })
            .extent(extent)
            .build();
        let rendering_info = vk::RenderingInfo::builder()
            .render_area(render_area)
            .layer_count(1)
            .color_attachments(&color_attachments)
            .build();
        device.cmd_begin_rendering(command_buffer, &rendering_info);
        record_scene_draw_extent(device, command_buffer, extent);
        record_scene_mesh_draw_ranges(device, command_buffer, scene, &scene.scene_color_draw_ranges)?;
        device.cmd_end_rendering(command_buffer);

        let to_present = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::BOTTOM_OF_PIPE)
            .dst_access_mask(vk::AccessFlags2::empty())
            .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(swapchain_image)
            .subresource_range(color_subresource_range())
            .build();
        let to_present_barriers = [to_present];
        let to_present_dependency = vk::DependencyInfo::builder()
            .image_memory_barriers(&to_present_barriers)
            .build();
        device.cmd_pipeline_barrier2(command_buffer, &to_present_dependency);
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
            resources.push(
                NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::StorageBuffer,
            );
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
            resource_descriptor_base: base,
            sampler_descriptor_base: index * layout.sampled_slots.len(),
            skinning_byte_offset,
            skinning_byte_count,
        });
    }
    (resources, commands)
}

fn scene_draw_skinning_range(draw: &SceneRenderingDeviceMeshDraw) -> (u64, u64) {
    if draw.skinning_palette_count == 0 {
        return (0, SCENE_PUPPET_BONE_MATRIX_BYTES);
    }
    (
        draw.skinning_palette_start.saturating_add(1) as u64 * SCENE_PUPPET_BONE_MATRIX_BYTES,
        draw.skinning_palette_count as u64 * SCENE_PUPPET_BONE_MATRIX_BYTES,
    )
}

fn pack_scene_vertices(storage: &SceneStorage, include_fullscreen_utility: bool) -> Vec<u8> {
    let mut payload = Vec::with_capacity(
        storage.document().mesh_vertices.len() * SCENE_MESH_VERTEX_STRIDE_BYTES as usize,
    );
    for vertex in &storage.document().mesh_vertices {
        for value in [
            vertex.position.x,
            vertex.position.y,
            vertex.uv[0],
            vertex.uv[1],
            1.0,
        ] {
            payload.extend_from_slice(&value.to_le_bytes());
        }
    }
    if include_fullscreen_utility {
        append_fullscreen_triangle_vertices(&mut payload);
    }
    payload
}

fn pack_scene_indices(storage: &SceneStorage, include_fullscreen_utility: bool) -> Vec<u8> {
    let mut payload = Vec::with_capacity(storage.document().mesh_indices.len() * 4);
    for index in &storage.document().mesh_indices {
        payload.extend_from_slice(&index.to_le_bytes());
    }
    if include_fullscreen_utility {
        append_fullscreen_triangle_indices(&mut payload);
    }
    payload
}

fn pack_scene_transforms(draws: &[SceneRenderingDeviceMeshDraw]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(draws.len() * SCENE_DRAW_TRANSFORM_BYTES as usize);
    for draw in draws {
        for row in draw.clip_transform {
            for value in row {
                payload.extend_from_slice(&value.to_le_bytes());
            }
        }
    }
    payload
}

fn pack_scene_skinning_matrices(
    graph: &crate::engine::scene::SceneRenderingDeviceGraphPlan,
) -> Vec<u8> {
    let mut payload = Vec::with_capacity(
        graph
            .puppet_bone_matrices
            .len()
            .saturating_add(1)
            * SCENE_PUPPET_BONE_MATRIX_BYTES as usize,
    );
    push_scene_skinning_matrix(
        &mut payload,
        [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    );
    for matrix in &graph.puppet_bone_matrices {
        push_scene_skinning_matrix(&mut payload, matrix.matrix);
    }
    payload
}

fn push_scene_skinning_matrix(payload: &mut Vec<u8>, matrix: [[f32; 4]; 4]) {
    for row in matrix {
        for value in row {
            payload.extend_from_slice(&value.to_le_bytes());
        }
    }
}

fn create_white_texture_upload(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    command_buffer: vk::CommandBuffer,
) -> Result<NativeVulkanVulkanaliaRecordedImageUpload, String> {
    let mip = NativeVulkanVulkanaliaImageMipUpload {
        buffer_offset: 0,
        byte_count: SCENE_WHITE_TEXTURE_BYTES.len() as u64,
        width: 1,
        height: 1,
    };
    native_vulkan_vulkanalia_create_sampled_image_with_recorded_staging_upload(
        device,
        memory_properties,
        command_buffer,
        "scene-white-fallback-texture",
        vk::Format::R8G8B8A8_UNORM,
        1,
        1,
        1,
        SCENE_WHITE_TEXTURE_BYTES,
        &[mip],
    )
}

fn scene_white_image_view_info(image: &NativeVulkanVulkanaliaImage) -> vk::ImageViewCreateInfo {
    vk::ImageViewCreateInfo::builder()
        .image(image.image)
        .view_type(vk::ImageViewType::_2D)
        .format(vk::Format::R8G8B8A8_UNORM)
        .components(identity_component_mapping())
        .subresource_range(color_subresource_range())
        .build()
}

fn scene_sampled_sampler_info() -> vk::SamplerCreateInfo {
    vk::SamplerCreateInfo::builder()
        .mag_filter(vk::Filter::LINEAR)
        .min_filter(vk::Filter::LINEAR)
        .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .max_lod(0.0)
        .build()
}

fn destroy_scene_gpu_resources(device: &Device, resources: SceneGpuResources) {
    destroy_scene_pipelines(device, resources.pipelines);
    native_vulkan_vulkanalia_destroy_descriptor_heap_resource_resources(
        device,
        resources.descriptor_heap,
    );
    effect_target::destroy_scene_effect_target_images(device, resources.effect_targets);
    if let Some(upload) = resources.white_upload {
        destroy_recorded_image_upload(device, upload);
    }
    if let Some(buffer) = resources.material_buffer {
        native_vulkan_vulkanalia_destroy_buffer(device, buffer);
    }
    if let Some(buffer) = resources.skinning_buffer {
        native_vulkan_vulkanalia_destroy_buffer(device, buffer);
    }
    native_vulkan_vulkanalia_destroy_buffer(device, resources.transform_buffer);
    native_vulkan_vulkanalia_destroy_buffer(device, resources.index_buffer);
    native_vulkan_vulkanalia_destroy_buffer(device, resources.vertex_buffer);
}

fn destroy_recorded_image_upload(
    device: &Device,
    upload: NativeVulkanVulkanaliaRecordedImageUpload,
) {
    native_vulkan_vulkanalia_destroy_buffer(device, upload.staging);
    native_vulkan_vulkanalia_destroy_image(device, upload.image);
}

fn color_subresource_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1)
        .build()
}

fn identity_component_mapping() -> vk::ComponentMapping {
    vk::ComponentMapping {
        r: vk::ComponentSwizzle::IDENTITY,
        g: vk::ComponentSwizzle::IDENTITY,
        b: vk::ComponentSwizzle::IDENTITY,
        a: vk::ComponentSwizzle::IDENTITY,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::{
        INVALID_MATERIAL_ID, SceneMaterialHandle, SceneObjectHandle, SceneRenderingDeviceGraphPlan,
        SceneRenderingDeviceDrawPrimitive, SceneRenderingDevicePuppetBoneMatrix,
    };

    #[test]
    fn transform_upload_uses_graph_clip_transform() {
        let draw = SceneRenderingDeviceMeshDraw {
            primitive: SceneRenderingDeviceDrawPrimitive::ObjectMesh,
            mesh_index: 0,
            resolved_object_index: 7,
            clip_transform: [
                [1.0, 2.0, 3.0, 4.0],
                [5.0, 6.0, 7.0, 8.0],
                [9.0, 10.0, 11.0, 12.0],
                [13.0, 14.0, 15.0, 16.0],
            ],
            skinning_palette_start: crate::engine::scene::INVALID_OBJECT_ID,
            skinning_palette_count: 0,
            object: SceneObjectHandle(0),
            material: SceneMaterialHandle(INVALID_MATERIAL_ID),
            vertex_start: 0,
            vertex_count: 4,
            index_start: 0,
            index_count: 6,
        };

        let payload = pack_scene_transforms(&[draw]);

        assert_eq!(payload.len(), SCENE_DRAW_TRANSFORM_BYTES as usize);
        assert_eq!(f32::from_le_bytes(payload[0..4].try_into().unwrap()), 1.0);
        assert_eq!(f32::from_le_bytes(payload[12..16].try_into().unwrap()), 4.0);
        assert_eq!(f32::from_le_bytes(payload[60..64].try_into().unwrap()), 16.0);
    }

    #[test]
    fn descriptor_plan_adds_skinning_storage_buffer_after_uniforms() {
        let draw = SceneRenderingDeviceMeshDraw {
            primitive: SceneRenderingDeviceDrawPrimitive::ObjectMesh,
            mesh_index: 0,
            resolved_object_index: 0,
            clip_transform: [[0.0; 4]; 4],
            skinning_palette_start: 2,
            skinning_palette_count: 3,
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

        let (descriptors, commands) = scene_descriptor_plan_inputs(&[draw], &layout, &[2]);

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
            3 * SCENE_PUPPET_BONE_MATRIX_BYTES
        );
        assert_eq!(
            commands[0].skinning_byte_count,
            3 * SCENE_PUPPET_BONE_MATRIX_BYTES
        );
        assert_eq!(commands[0].pipeline_index, 2);
    }

    #[test]
    fn skinning_payload_prefixes_identity_fallback_matrix() {
        let graph = SceneRenderingDeviceGraphPlan {
            pass_nodes: Vec::new(),
            target_allocations: Vec::new(),
            sampled_bindings: Vec::new(),
            mesh_draws: Vec::new(),
            puppet_bone_palettes: Vec::new(),
            puppet_bone_matrices: vec![SceneRenderingDevicePuppetBoneMatrix {
                puppet_index: 0,
                bone_index: 41,
                parent_index: -1,
                matrix: [
                    [1.0, 2.0, 3.0, 4.0],
                    [5.0, 6.0, 7.0, 8.0],
                    [9.0, 10.0, 11.0, 12.0],
                    [13.0, 14.0, 15.0, 16.0],
                ],
            }],
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
            descriptor_heap_storage_buffer_count: 1,
            descriptor_heap_sampler_count: 0,
            graph_physical_target_count: 0,
            graph_aliased_target_count: 0,
            fifo_latest_ready_present_required: true,
        };

        let payload = pack_scene_skinning_matrices(&graph);

        assert_eq!(
            payload.len(),
            2 * SCENE_PUPPET_BONE_MATRIX_BYTES as usize
        );
        assert_eq!(f32_from_payload(&payload, 0), 1.0);
        assert_eq!(f32_from_payload(&payload, 20), 1.0);
        assert_eq!(f32_from_payload(&payload, 40), 1.0);
        assert_eq!(f32_from_payload(&payload, 60), 1.0);
        assert_eq!(
            f32_from_payload(&payload, SCENE_PUPPET_BONE_MATRIX_BYTES as usize),
            1.0
        );
        assert_eq!(
            f32_from_payload(
                &payload,
                SCENE_PUPPET_BONE_MATRIX_BYTES as usize + 60
            ),
            16.0
        );
    }

    fn f32_from_payload(payload: &[u8], offset: usize) -> f32 {
        f32::from_le_bytes(payload[offset..offset + 4].try_into().unwrap())
    }
}
