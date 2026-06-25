#![allow(dead_code)]

use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use vulkanalia::Version;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{
    self, HasBuilder, KhrSurfaceExtensionInstanceCommands, KhrSwapchainExtensionDeviceCommands,
};

use crate::renderer::native_vulkan::NativeVulkanClearColor;
use crate::renderer::native_wayland::{
    NativeWaylandHost, NativeWaylandHostOptions, NativeWaylandSurfaceHandles,
};

use super::instance::{
    NativeVulkanVulkanaliaInstance,
    native_vulkan_vulkanalia_create_instance_with_required_extensions,
    native_vulkan_vulkanalia_destroy_instance,
};
use super::swapchain::{
    NativeVulkanVulkanaliaPresentDeviceExtensionSnapshot,
    NativeVulkanVulkanaliaPresentQueueSnapshot, NativeVulkanVulkanaliaSwapchainSnapshot,
    OPTIONAL_INSTANCE_EXTENSIONS, REQUIRED_INSTANCE_EXTENSIONS, composite_alpha_label,
    create_vulkanalia_present_device, create_vulkanalia_swapchain_plan,
    create_vulkanalia_wayland_surface, present_mode_label, queue_flag_labels,
    select_vulkanalia_present_queue, swapchain_create_flag_labels,
    vulkanalia_surface_capabilities2_enabled, vulkanalia_surface_maintenance1_enabled,
};

#[derive(Debug, Clone, PartialEq)]
pub struct NativeVulkanVulkanaliaClearPresentOptions {
    pub host: NativeWaylandHostOptions,
    pub wait_configure_roundtrips: usize,
    pub duration: Duration,
    pub target_max_fps: Option<u32>,
    pub clear_color: NativeVulkanClearColor,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanVulkanaliaClearPresentSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub loader: String,
    pub requested_api_version: String,
    pub runtime_elapsed_ms: u64,
    pub frames_presented: u64,
    pub average_present_fps: f64,
    pub clear_color: NativeVulkanClearColor,
    pub selected_queue: NativeVulkanVulkanaliaPresentQueueSnapshot,
    pub device_extensions: NativeVulkanVulkanaliaPresentDeviceExtensionSnapshot,
    pub swapchain: NativeVulkanVulkanaliaSwapchainSnapshot,
    pub command_submit_model: &'static str,
    pub uses_synchronization2: bool,
    pub uses_submit2: bool,
    pub present_backend: &'static str,
    pub ffmpeg_reference: &'static str,
}

pub fn run_native_vulkan_vulkanalia_clear_present(
    options: NativeVulkanVulkanaliaClearPresentOptions,
) -> Result<NativeVulkanVulkanaliaClearPresentSnapshot, String> {
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
    let result = run_vulkanalia_clear_present_inner(&vulkan, handles, options);
    native_vulkan_vulkanalia_destroy_instance(vulkan);
    result
}

fn run_vulkanalia_clear_present_inner(
    vulkan: &NativeVulkanVulkanaliaInstance,
    handles: NativeWaylandSurfaceHandles,
    options: NativeVulkanVulkanaliaClearPresentOptions,
) -> Result<NativeVulkanVulkanaliaClearPresentSnapshot, String> {
    let instance = &vulkan.instance;
    let surface = create_vulkanalia_wayland_surface(instance, handles)?;
    let result = with_vulkanalia_clear_present(instance, surface, handles, vulkan, options);
    unsafe {
        instance.destroy_surface_khr(surface, None);
    }
    result
}

fn with_vulkanalia_clear_present(
    instance: &Instance,
    surface: vk::SurfaceKHR,
    handles: NativeWaylandSurfaceHandles,
    vulkan: &NativeVulkanVulkanaliaInstance,
    options: NativeVulkanVulkanaliaClearPresentOptions,
) -> Result<NativeVulkanVulkanaliaClearPresentSnapshot, String> {
    let physical_devices = unsafe { instance.enumerate_physical_devices() }
        .map_err(|err| format!("vkEnumeratePhysicalDevices(vulkanalia clear present): {err:?}"))?;
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
            "Vulkanalia clear present requires synchronization2 for QueueSubmit2".to_owned(),
        );
    }

    let swapchain_plan = match create_vulkanalia_swapchain_plan(
        instance,
        selection.physical_device,
        surface,
        handles.buffer_size,
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
                "vkCreateSwapchainKHR(vulkanalia clear present): {err:?}"
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
                "vkGetSwapchainImagesKHR(vulkanalia clear present): {err:?}"
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
                "vkCreateCommandPool(vulkanalia clear present): {err:?}"
            ));
        }
    };
    let command_buffer_info = vk::CommandBufferAllocateInfo::builder()
        .command_pool(command_pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(swapchain_images.len() as u32);
    let command_buffers = match unsafe { device.allocate_command_buffers(&command_buffer_info) } {
        Ok(command_buffers) => command_buffers,
        Err(err) => {
            unsafe {
                device.destroy_command_pool(command_pool, None);
                device.destroy_swapchain_khr(swapchain, None);
                present_device.device.destroy_device(None);
            }
            return Err(format!(
                "vkAllocateCommandBuffers(vulkanalia clear present): {err:?}"
            ));
        }
    };
    let semaphore_info = vk::SemaphoreCreateInfo::builder();
    let image_available = match unsafe { device.create_semaphore(&semaphore_info, None) } {
        Ok(semaphore) => semaphore,
        Err(err) => {
            unsafe {
                device.destroy_command_pool(command_pool, None);
                device.destroy_swapchain_khr(swapchain, None);
                present_device.device.destroy_device(None);
            }
            return Err(format!(
                "vkCreateSemaphore(image_available vulkanalia clear present): {err:?}"
            ));
        }
    };
    let render_finished = match unsafe { device.create_semaphore(&semaphore_info, None) } {
        Ok(semaphore) => semaphore,
        Err(err) => {
            unsafe {
                device.destroy_semaphore(image_available, None);
                device.destroy_command_pool(command_pool, None);
                device.destroy_swapchain_khr(swapchain, None);
                present_device.device.destroy_device(None);
            }
            return Err(format!(
                "vkCreateSemaphore(render_finished vulkanalia clear present): {err:?}"
            ));
        }
    };
    let fence_info = vk::FenceCreateInfo::builder().flags(vk::FenceCreateFlags::SIGNALED);
    let in_flight = match unsafe { device.create_fence(&fence_info, None) } {
        Ok(fence) => fence,
        Err(err) => {
            unsafe {
                device.destroy_semaphore(render_finished, None);
                device.destroy_semaphore(image_available, None);
                device.destroy_command_pool(command_pool, None);
                device.destroy_swapchain_khr(swapchain, None);
                present_device.device.destroy_device(None);
            }
            return Err(format!("vkCreateFence(vulkanalia clear present): {err:?}"));
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
    let mut image_layouts = vec![vk::ImageLayout::UNDEFINED; swapchain_images.len()];
    while Instant::now() < deadline {
        unsafe {
            device
                .wait_for_fences(&[in_flight], true, u64::MAX)
                .map_err(|err| format!("vkWaitForFences(vulkanalia clear present): {err:?}"))?;
            device
                .reset_fences(&[in_flight])
                .map_err(|err| format!("vkResetFences(vulkanalia clear present): {err:?}"))?;
        }
        let (image_index, _) = unsafe {
            device.acquire_next_image_khr(swapchain, u64::MAX, image_available, vk::Fence::null())
        }
        .map_err(|err| format!("vkAcquireNextImageKHR(vulkanalia clear present): {err:?}"))?;
        let image_index = image_index as usize;
        let command_buffer = command_buffers
            .get(image_index)
            .copied()
            .ok_or_else(|| format!("swapchain image index {image_index} has no command buffer"))?;
        record_vulkanalia_clear_present_command_buffer(
            device,
            command_buffer,
            swapchain_images[image_index],
            image_layouts[image_index],
            options.clear_color,
        )?;
        image_layouts[image_index] = vk::ImageLayout::PRESENT_SRC_KHR;
        submit_vulkanalia_clear_present_command_buffer2(
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
                .map_err(|err| format!("vkQueuePresentKHR(vulkanalia clear present): {err:?}"))?;
        }
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
    unsafe {
        device.destroy_fence(in_flight, None);
        device.destroy_semaphore(render_finished, None);
        device.destroy_semaphore(image_available, None);
        device.destroy_command_pool(command_pool, None);
        device.destroy_swapchain_khr(swapchain, None);
        present_device.device.destroy_device(None);
    }

    Ok(NativeVulkanVulkanaliaClearPresentSnapshot {
        binding: "vulkanalia",
        route: "clear-present",
        loader: vulkan.loader_name.to_owned(),
        requested_api_version: Version::V1_4_0.to_string(),
        runtime_elapsed_ms: elapsed.as_millis().min(u64::MAX as u128) as u64,
        frames_presented,
        average_present_fps: if elapsed.is_zero() {
            0.0
        } else {
            frames_presented as f64 / elapsed.as_secs_f64()
        },
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
            image_count: swapchain_images.len(),
            min_image_count: swapchain_plan.image_count,
            composite_alpha: composite_alpha_label(swapchain_plan.composite_alpha),
            image_usage: vec!["transfer-dst", "color-attachment"],
            create_flags: swapchain_create_flag_labels(swapchain_plan.create_flags),
            present_id2_enabled: swapchain_plan.present_id2_enabled,
            present_wait2_enabled: swapchain_plan.present_wait2_enabled,
        },
        command_submit_model: "acquire_next_image_khr -> cmd_pipeline_barrier2 -> cmd_clear_color_image -> queue_submit2 -> queue_present_khr",
        uses_synchronization2: true,
        uses_submit2: true,
        present_backend: "vulkanalia-clear-present-runtime",
        ffmpeg_reference: "references/ffmpeg/libavutil/vulkan.c",
    })
}

fn record_vulkanalia_clear_present_command_buffer(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    image: vk::Image,
    old_layout: vk::ImageLayout,
    clear_color: NativeVulkanClearColor,
) -> Result<(), String> {
    unsafe {
        device
            .reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty())
            .map_err(|err| format!("vkResetCommandBuffer(vulkanalia clear present): {err:?}"))?;

        let begin_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
            .build();
        device
            .begin_command_buffer(command_buffer, &begin_info)
            .map_err(|err| format!("vkBeginCommandBuffer(vulkanalia clear present): {err:?}"))?;

        let range = vulkanalia_color_subresource_range();
        let to_transfer = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(match old_layout {
                vk::ImageLayout::UNDEFINED => vk::PipelineStageFlags2::TOP_OF_PIPE,
                _ => vk::PipelineStageFlags2::ALL_COMMANDS,
            })
            .src_access_mask(vk::AccessFlags2::empty())
            .dst_stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
            .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .old_layout(old_layout)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(range)
            .build();
        let to_transfer_barriers = [to_transfer];
        let to_transfer_dependency = vk::DependencyInfo::builder()
            .image_memory_barriers(&to_transfer_barriers)
            .build();
        device.cmd_pipeline_barrier2(command_buffer, &to_transfer_dependency);

        let color = vk::ClearColorValue {
            float32: [clear_color.r, clear_color.g, clear_color.b, clear_color.a],
        };
        device.cmd_clear_color_image(
            command_buffer,
            image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &color,
            &[range],
        );

        let to_present = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
            .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::BOTTOM_OF_PIPE)
            .dst_access_mask(vk::AccessFlags2::empty())
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(range)
            .build();
        let to_present_barriers = [to_present];
        let to_present_dependency = vk::DependencyInfo::builder()
            .image_memory_barriers(&to_present_barriers)
            .build();
        device.cmd_pipeline_barrier2(command_buffer, &to_present_dependency);

        device
            .end_command_buffer(command_buffer)
            .map_err(|err| format!("vkEndCommandBuffer(vulkanalia clear present): {err:?}"))?;
    }

    Ok(())
}

fn submit_vulkanalia_clear_present_command_buffer2(
    device: &Device,
    queue: vk::Queue,
    command_buffer: vk::CommandBuffer,
    image_available: vk::Semaphore,
    render_finished: vk::Semaphore,
    fence: vk::Fence,
) -> Result<(), String> {
    let wait = vk::SemaphoreSubmitInfo::builder()
        .semaphore(image_available)
        .stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
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
            .map_err(|err| format!("vkQueueSubmit2(vulkanalia clear present): {err:?}"))?;
    }

    Ok(())
}

fn vulkanalia_color_subresource_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1)
        .build()
}
