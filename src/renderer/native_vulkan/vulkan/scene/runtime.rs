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
    self, ExtDescriptorHeapExtensionDeviceCommands, HasBuilder,
    KhrSurfaceExtensionInstanceCommands, KhrSwapchainExtensionDeviceCommands,
};

use crate::engine::scene::{
    SceneObjectHandle, ScenePipelineBlend, SceneRenderingDeviceMeshDraw, SceneStorage,
};
use crate::renderer::native_vulkan::scene::native_vulkan_scene_shader_for_key;
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
    native_vulkan_vulkanalia_descriptor_heap_mixed_resource_bind_info_for_descriptor,
    native_vulkan_vulkanalia_descriptor_heap_mixed_sampler_bind_info_for_descriptor,
    native_vulkan_vulkanalia_descriptor_heap_resource_plan,
    native_vulkan_vulkanalia_descriptor_heap_resource_relative_combined_image_sampler_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_resource_relative_uniform_buffer_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_shader_binding_mapping_info,
    native_vulkan_vulkanalia_destroy_buffer,
    native_vulkan_vulkanalia_destroy_descriptor_heap_resource_resources,
    native_vulkan_vulkanalia_destroy_image,
    native_vulkan_vulkanalia_write_descriptor_heap_resource_image_sampler,
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

const SCENE_MESH_VERTEX_STRIDE_BYTES: u32 = 20;
const SCENE_DRAW_TRANSFORM_BYTES: u64 = 64;
const SCENE_MATERIAL_UNIFORM_BYTES: u64 = 48;
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
    pub descriptor_model: &'static str,
    pub descriptor_heap_resource_count: usize,
    pub descriptor_heap_sampler_count: usize,
    pub vertex_buffer_bytes: u64,
    pub index_buffer_bytes: u64,
    pub transform_uniform_bytes: u64,
    pub material_uniform_bytes: u64,
    pub sampled_fallback_texture_count: usize,
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
    white_upload: Option<NativeVulkanVulkanaliaRecordedImageUpload>,
    descriptor_heap: VulkanaliaDescriptorHeapResourceResources,
    descriptor_heap_plan: NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    pipeline: vk::Pipeline,
    draw_commands: Vec<SceneGpuDrawCommand>,
    sampled_slots: Vec<u32>,
    material_uniform_enabled: bool,
}

#[derive(Debug, Clone, Copy)]
struct SceneGpuDrawCommand {
    first_index: u32,
    index_count: u32,
    vertex_offset: i32,
    resource_descriptor_base: usize,
    sampler_descriptor_base: usize,
}

struct ScenePipelineResources {
    pipeline: vk::Pipeline,
}

struct ScenePipelineDescriptorLayout {
    sampled_slots: Vec<u32>,
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
    let scene_resources = match create_scene_gpu_resources(
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
    let sampled_fallback_texture_count = usize::from(!scene_resources.sampled_slots.is_empty());
    let descriptor_heap_resource_count = scene_resources
        .descriptor_heap_plan
        .resource_descriptor_count;
    let descriptor_heap_sampler_count = scene_resources.descriptor_heap_plan.sampler_count;
    let mesh_draw_count = scene_resources.draw_commands.len();
    let mesh_draw_recorded = mesh_draw_count > 0;
    let command_order = scene_command_order(scene_resources.sampled_slots.is_empty());

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
        descriptor_model: "VK_EXT_descriptor_heap",
        descriptor_heap_resource_count,
        descriptor_heap_sampler_count,
        vertex_buffer_bytes,
        index_buffer_bytes,
        transform_uniform_bytes,
        material_uniform_bytes,
        sampled_fallback_texture_count,
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
    let shader_key = first_draw_shader_key(storage)?;
    let shader = native_vulkan_scene_shader_for_key(shader_key)
        .ok_or_else(|| format!("scene shader {shader_key:?} is not in the built-in catalog"))?;
    let contract = storage
        .shader_contracts()
        .iter()
        .find(|contract| storage.string(contract.shader_key) == Some(shader_key))
        .ok_or_else(|| format!("scene shader {shader_key:?} has no shader contract"))?;
    let descriptor_layout = ScenePipelineDescriptorLayout {
        sampled_slots: sampled_slots(contract.texture_slot_mask),
        material_uniform_enabled: shader_uses_material_uniform(shader_key),
    };
    let draw_count = backend_plan.rendering_device_graph.mesh_draws.len();
    let vertex_payload = pack_scene_vertices(storage);
    let index_payload = pack_scene_indices(storage);
    let transform_payload =
        pack_scene_transforms(storage, &backend_plan.rendering_device_graph.mesh_draws);
    let material_payload = descriptor_layout
        .material_uniform_enabled
        .then(|| pack_scene_material_uniforms(draw_count));

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

    let white_upload = if descriptor_layout.sampled_slots.is_empty() {
        None
    } else {
        match create_white_texture_upload(device, memory_properties, setup_command_buffer) {
            Ok(upload) => Some(upload),
            Err(err) => {
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
                native_vulkan_vulkanalia_destroy_buffer(device, transform_buffer);
                native_vulkan_vulkanalia_destroy_buffer(device, index_buffer);
                native_vulkan_vulkanalia_destroy_buffer(device, vertex_buffer);
                return Err(err);
            }
        };

    write_scene_descriptors(
        device,
        &mut descriptor_heap,
        &draw_commands,
        &transform_buffer,
        material_buffer.as_ref(),
        white_upload.as_ref().map(|upload| &upload.image),
        descriptor_layout.sampled_slots.len(),
    )?;
    let pipeline_resources = match create_scene_pipeline(
        device,
        target_format,
        extent,
        shader.vertex_spirv,
        shader.fragment_spirv,
        &descriptor_heap_plan,
        &descriptor_layout,
        primary_scene_blend(storage),
    ) {
        Ok(resources) => resources,
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
        white_upload,
        descriptor_heap,
        descriptor_heap_plan,
        pipeline: pipeline_resources.pipeline,
        draw_commands,
        sampled_slots: descriptor_layout.sampled_slots,
        material_uniform_enabled: descriptor_layout.material_uniform_enabled,
    })
}

fn write_scene_descriptors(
    device: &Device,
    descriptor_heap: &mut VulkanaliaDescriptorHeapResourceResources,
    draw_commands: &[SceneGpuDrawCommand],
    transform_buffer: &NativeVulkanVulkanaliaBuffer,
    material_buffer: Option<&NativeVulkanVulkanaliaBuffer>,
    white_image: Option<&NativeVulkanVulkanaliaImage>,
    sampled_slot_count: usize,
) -> Result<(), String> {
    let image_view_info = white_image.map(scene_white_image_view_info);
    let sampler_info = white_image.map(|_| scene_white_sampler_info());
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
        if sampled_slot_count > 0 {
            let image_view_info = image_view_info
                .as_ref()
                .ok_or_else(|| "scene sampled slots require a fallback texture".to_owned())?;
            let sampler_info = sampler_info
                .as_ref()
                .ok_or_else(|| "scene sampled slots require a fallback sampler".to_owned())?;
            for sampled_index in 0..sampled_slot_count {
                native_vulkan_vulkanalia_write_descriptor_heap_resource_image_sampler(
                    device,
                    descriptor_heap,
                    resource_descriptor_index + sampled_index,
                    draw.sampler_descriptor_base + sampled_index,
                    image_view_info,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    sampler_info,
                )?;
            }
        }
    }
    Ok(())
}

fn create_scene_pipeline(
    device: &Device,
    target_format: vk::Format,
    extent: vk::Extent2D,
    vertex_spirv: &[u32],
    fragment_spirv: &[u32],
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    descriptor_layout: &ScenePipelineDescriptorLayout,
    blend: ScenePipelineBlend,
) -> Result<ScenePipelineResources, String> {
    if extent.width == 0 || extent.height == 0 {
        return Err("scene pipeline requires non-zero extent".to_owned());
    }
    let vertex_module = create_shader_module(device, vertex_spirv, "scene vertex")?;
    let result = (|| -> Result<ScenePipelineResources, String> {
        let fragment_module = create_shader_module(device, fragment_spirv, "scene fragment")?;
        let result = (|| -> Result<ScenePipelineResources, String> {
            let shader_entry = b"main\0";
            let vertex_mapping =
                native_vulkan_vulkanalia_descriptor_heap_resource_relative_uniform_buffer_binding_mapping(
                    descriptor_heap_plan,
                    2,
                    0,
                    0,
                )?;
            let vertex_mappings = [vertex_mapping];
            let mut vertex_mapping_info =
                native_vulkan_vulkanalia_descriptor_heap_shader_binding_mapping_info(
                    &vertex_mappings,
                )?;
            let mut vertex_stage = vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(vertex_module)
                .name(shader_entry)
                .build();
            vertex_stage.next = &mut vertex_mapping_info as *mut _ as *const std::ffi::c_void;

            let mut fragment_mappings = Vec::new();
            if descriptor_layout.material_uniform_enabled {
                fragment_mappings.push(
                    native_vulkan_vulkanalia_descriptor_heap_resource_relative_uniform_buffer_binding_mapping(
                        descriptor_heap_plan,
                        3,
                        0,
                        1,
                    )?,
                );
            }
            let sampled_base = 1 + usize::from(descriptor_layout.material_uniform_enabled);
            for (sampled_index, slot) in descriptor_layout.sampled_slots.iter().enumerate() {
                fragment_mappings.push(
                    native_vulkan_vulkanalia_descriptor_heap_resource_relative_combined_image_sampler_binding_mapping(
                        descriptor_heap_plan,
                        *slot,
                        0,
                        sampled_base + sampled_index,
                        0,
                        sampled_index,
                    )?,
                );
            }
            let mut fragment_mapping_info =
                native_vulkan_vulkanalia_descriptor_heap_shader_binding_mapping_info(
                    &fragment_mappings,
                )?;
            let mut fragment_stage = vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(fragment_module)
                .name(shader_entry)
                .build();
            if !fragment_mappings.is_empty() {
                fragment_stage.next =
                    &mut fragment_mapping_info as *mut _ as *const std::ffi::c_void;
            }
            let stages = [vertex_stage, fragment_stage];
            let binding = vk::VertexInputBindingDescription::builder()
                .binding(0)
                .stride(SCENE_MESH_VERTEX_STRIDE_BYTES)
                .input_rate(vk::VertexInputRate::VERTEX)
                .build();
            let attributes = [
                vk::VertexInputAttributeDescription::builder()
                    .location(0)
                    .binding(0)
                    .format(vk::Format::R32G32_SFLOAT)
                    .offset(0)
                    .build(),
                vk::VertexInputAttributeDescription::builder()
                    .location(1)
                    .binding(0)
                    .format(vk::Format::R32G32_SFLOAT)
                    .offset(8)
                    .build(),
                vk::VertexInputAttributeDescription::builder()
                    .location(2)
                    .binding(0)
                    .format(vk::Format::R32_SFLOAT)
                    .offset(16)
                    .build(),
            ];
            let bindings = [binding];
            let vertex_input = vk::PipelineVertexInputStateCreateInfo::builder()
                .vertex_binding_descriptions(&bindings)
                .vertex_attribute_descriptions(&attributes)
                .build();
            let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::builder()
                .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
                .build();
            let viewport = vk::Viewport::builder()
                .x(0.0)
                .y(0.0)
                .width(extent.width as f32)
                .height(extent.height as f32)
                .min_depth(0.0)
                .max_depth(1.0)
                .build();
            let scissor = vk::Rect2D::builder()
                .offset(vk::Offset2D { x: 0, y: 0 })
                .extent(extent)
                .build();
            let viewports = [viewport];
            let scissors = [scissor];
            let viewport_state = vk::PipelineViewportStateCreateInfo::builder()
                .viewports(&viewports)
                .scissors(&scissors)
                .build();
            let rasterization = vk::PipelineRasterizationStateCreateInfo::builder()
                .polygon_mode(vk::PolygonMode::FILL)
                .cull_mode(vk::CullModeFlags::NONE)
                .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
                .line_width(1.0)
                .build();
            let multisample = vk::PipelineMultisampleStateCreateInfo::builder()
                .rasterization_samples(vk::SampleCountFlags::_1)
                .build();
            let color_attachment = scene_color_blend_attachment(blend);
            let color_attachments = [color_attachment];
            let color_blend = vk::PipelineColorBlendStateCreateInfo::builder()
                .attachments(&color_attachments)
                .build();
            let color_attachment_formats = [target_format];
            let mut rendering_info = vk::PipelineRenderingCreateInfo::builder()
                .color_attachment_formats(&color_attachment_formats)
                .build();
            let mut pipeline_flags2 = vk::PipelineCreateFlags2CreateInfo::builder()
                .flags(vk::PipelineCreateFlags2::DESCRIPTOR_HEAP_EXT)
                .build();
            let mut pipeline_info = vk::GraphicsPipelineCreateInfo::builder()
                .stages(&stages)
                .vertex_input_state(&vertex_input)
                .input_assembly_state(&input_assembly)
                .viewport_state(&viewport_state)
                .rasterization_state(&rasterization)
                .multisample_state(&multisample)
                .color_blend_state(&color_blend)
                .layout(vk::PipelineLayout::null())
                .render_pass(vk::RenderPass::null())
                .subpass(0)
                .push_next(&mut rendering_info);
            pipeline_info = pipeline_info.push_next(&mut pipeline_flags2);
            let pipeline_info = pipeline_info.build();
            let (pipelines, _success_code) = unsafe {
                device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
            }
            .map_err(|err| format!("vkCreateGraphicsPipelines(vulkanalia scene): {err:?}"))?;
            Ok(ScenePipelineResources {
                pipeline: pipelines[0],
            })
        })();
        unsafe {
            device.destroy_shader_module(fragment_module, None);
        }
        result
    })();
    unsafe {
        device.destroy_shader_module(vertex_module, None);
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
        device.cmd_bind_pipeline(
            command_buffer,
            vk::PipelineBindPoint::GRAPHICS,
            scene.pipeline,
        );
        let vertex_buffers = [scene.vertex_buffer.buffer];
        let vertex_offsets = [0u64];
        device.cmd_bind_vertex_buffers(command_buffer, 0, &vertex_buffers, &vertex_offsets);
        device.cmd_bind_index_buffer(
            command_buffer,
            scene.index_buffer.buffer,
            0,
            vk::IndexType::UINT32,
        );
        for draw in &scene.draw_commands {
            let resource_bind =
                native_vulkan_vulkanalia_descriptor_heap_mixed_resource_bind_info_for_descriptor(
                    &scene.descriptor_heap,
                    draw.resource_descriptor_base,
                )?;
            device.cmd_bind_resource_heap_ext(command_buffer, &resource_bind);
            if !scene.sampled_slots.is_empty() {
                let sampler_bind = native_vulkan_vulkanalia_descriptor_heap_mixed_sampler_bind_info_for_descriptor(
                    &scene.descriptor_heap,
                    draw.sampler_descriptor_base,
                )?;
                device.cmd_bind_sampler_heap_ext(command_buffer, &sampler_bind);
            }
            device.cmd_draw_indexed(
                command_buffer,
                draw.index_count,
                1,
                draw.first_index,
                draw.vertex_offset,
                0,
            );
        }
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

fn first_draw_shader_key(storage: &SceneStorage) -> Result<&str, String> {
    let pass = storage
        .document()
        .render_passes
        .iter()
        .find(|pass| pass.object.0 != crate::engine::scene::INVALID_OBJECT_ID)
        .ok_or_else(|| "scene render graph has no mesh pass".to_owned())?;
    storage
        .string(pass.shader_key)
        .ok_or_else(|| "scene mesh pass has no shader key".to_owned())
}

fn sampled_slots(mask: u32) -> Vec<u32> {
    (0..32).filter(|slot| mask & (1u32 << slot) != 0).collect()
}

fn scene_descriptor_plan_inputs(
    draws: &[SceneRenderingDeviceMeshDraw],
    layout: &ScenePipelineDescriptorLayout,
) -> (
    Vec<NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind>,
    Vec<SceneGpuDrawCommand>,
) {
    let per_draw_resource_count =
        1 + usize::from(layout.material_uniform_enabled) + layout.sampled_slots.len();
    let mut resources = Vec::with_capacity(draws.len().saturating_mul(per_draw_resource_count));
    let mut commands = Vec::with_capacity(draws.len());
    for (index, draw) in draws.iter().enumerate() {
        let base = index * per_draw_resource_count;
        resources.push(NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer);
        if layout.material_uniform_enabled {
            resources
                .push(NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer);
        }
        resources.extend(
            layout
                .sampled_slots
                .iter()
                .map(|_| NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage),
        );
        commands.push(SceneGpuDrawCommand {
            first_index: draw.index_start,
            index_count: draw.index_count,
            vertex_offset: draw.vertex_start as i32,
            resource_descriptor_base: base,
            sampler_descriptor_base: index * layout.sampled_slots.len(),
        });
    }
    (resources, commands)
}

fn pack_scene_vertices(storage: &SceneStorage) -> Vec<u8> {
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
    payload
}

fn pack_scene_indices(storage: &SceneStorage) -> Vec<u8> {
    let mut payload = Vec::with_capacity(storage.document().mesh_indices.len() * 4);
    for index in &storage.document().mesh_indices {
        payload.extend_from_slice(&index.to_le_bytes());
    }
    payload
}

fn pack_scene_transforms(
    storage: &SceneStorage,
    draws: &[SceneRenderingDeviceMeshDraw],
) -> Vec<u8> {
    let mut payload = Vec::with_capacity(draws.len() * SCENE_DRAW_TRANSFORM_BYTES as usize);
    for draw in draws {
        let object = storage
            .objects()
            .iter()
            .find(|object| object.id == draw.object)
            .copied();
        let matrix = scene_draw_matrix(storage, object.map(|object| object.id));
        for row in matrix {
            for value in row {
                payload.extend_from_slice(&value.to_le_bytes());
            }
        }
    }
    payload
}

fn scene_draw_matrix(storage: &SceneStorage, object: Option<SceneObjectHandle>) -> [[f32; 4]; 4] {
    let object =
        object.and_then(|handle| storage.objects().iter().find(|object| object.id == handle));
    let project = storage.project();
    let width = project.logical_width.max(1) as f32;
    let height = project.logical_height.max(1) as f32;
    let origin = object.map(|object| object.origin).unwrap_or_default();
    let scale = object
        .map(|object| object.scale)
        .unwrap_or(crate::engine::scene::SceneVec3 {
            x: 1.0,
            y: 1.0,
            z: 1.0,
        });
    [
        [2.0 * scale.x / width, 0.0, 0.0, 2.0 * origin.x / width],
        [0.0, -2.0 * scale.y / height, 0.0, -2.0 * origin.y / height],
        [0.0, 0.0, scale.z, origin.z],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn pack_scene_material_uniforms(draw_count: usize) -> Vec<u8> {
    let mut payload = Vec::with_capacity(draw_count * SCENE_MATERIAL_UNIFORM_BYTES as usize);
    for _ in 0..draw_count {
        for value in [
            1.0_f32, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        ] {
            payload.extend_from_slice(&value.to_le_bytes());
        }
    }
    payload
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

fn scene_white_sampler_info() -> vk::SamplerCreateInfo {
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

fn create_shader_module(
    device: &Device,
    code: &[u32],
    label: &'static str,
) -> Result<vk::ShaderModule, String> {
    if code.first().copied() != Some(0x0723_0203) {
        return Err(format!("scene {label} shader is not valid SPIR-V bytecode"));
    }
    let create_info = vk::ShaderModuleCreateInfo::builder()
        .code(code)
        .code_size(std::mem::size_of_val(code));
    unsafe { device.create_shader_module(&create_info, None) }
        .map_err(|err| format!("vkCreateShaderModule(vulkanalia {label}): {err:?}"))
}

fn primary_scene_blend(storage: &SceneStorage) -> ScenePipelineBlend {
    storage
        .document()
        .render_passes
        .iter()
        .find(|pass| pass.object.0 != crate::engine::scene::INVALID_OBJECT_ID)
        .map(|pass| pass.pipeline_blend)
        .unwrap_or(ScenePipelineBlend::Normal)
}

fn scene_color_blend_attachment(
    blend: ScenePipelineBlend,
) -> vk::PipelineColorBlendAttachmentState {
    let builder = vk::PipelineColorBlendAttachmentState::builder().color_write_mask(
        vk::ColorComponentFlags::R
            | vk::ColorComponentFlags::G
            | vk::ColorComponentFlags::B
            | vk::ColorComponentFlags::A,
    );
    match blend {
        ScenePipelineBlend::Disabled => builder.blend_enable(false).build(),
        ScenePipelineBlend::Additive => builder
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE)
            .alpha_blend_op(vk::BlendOp::ADD)
            .build(),
        _ => builder
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .alpha_blend_op(vk::BlendOp::ADD)
            .build(),
    }
}

fn shader_uses_material_uniform(shader_key: &str) -> bool {
    let key = shader_key.to_ascii_lowercase();
    key.contains("genericimage")
        || key == "color"
        || key.starts_with("color__")
        || key == "we/color"
        || key.starts_with("we/color__")
        || key == "text"
        || key.starts_with("text__")
        || key == "we/text"
        || key.starts_with("we/text__")
        || key.contains("genericparticle")
}

fn destroy_scene_gpu_resources(device: &Device, resources: SceneGpuResources) {
    unsafe {
        device.destroy_pipeline(resources.pipeline, None);
    }
    native_vulkan_vulkanalia_destroy_descriptor_heap_resource_resources(
        device,
        resources.descriptor_heap,
    );
    if let Some(upload) = resources.white_upload {
        destroy_recorded_image_upload(device, upload);
    }
    if let Some(buffer) = resources.material_buffer {
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

fn scene_command_order(no_sampled_slots: bool) -> Vec<&'static str> {
    let mut order = vec![
        "create_scene_vertex_buffer",
        "create_scene_index_buffer",
        "create_scene_uniform_buffers",
        "create_descriptor_heap_resource_buffer",
    ];
    if !no_sampled_slots {
        order.extend([
            "create_descriptor_heap_sampler_buffer",
            "upload_scene_fallback_texture",
        ]);
    }
    order.extend([
        "write_descriptor_heap_uniform_buffer_descriptors",
        "write_descriptor_heap_sampled_image_descriptors",
        "cmd_begin_rendering",
        "cmd_bind_resource_heap_ext",
        "cmd_bind_sampler_heap_ext_when_sampled_slots_exist",
        "cmd_bind_scene_mesh_pipeline",
        "cmd_bind_scene_mesh_vertex_index_buffers",
        "cmd_draw_indexed_scene_meshes",
        "cmd_end_rendering",
        "queue_submit2",
        "queue_present_khr",
    ]);
    order
}
