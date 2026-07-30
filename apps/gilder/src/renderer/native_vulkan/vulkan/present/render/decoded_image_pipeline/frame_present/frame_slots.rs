//! Timeline-safe decoded-present frame-slot lifecycle.

use super::*;

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_wait_decoded_image_present_frame_slot(
    device: &Device,
    resources: &VulkanaliaDecodedImagePresentFrameResources,
    present_frame_slot: u32,
) -> Result<u64, String> {
    let slot = present_frame_slot as usize;
    let fence = resources.in_flight.get(slot).copied().ok_or_else(|| {
        format!(
            "decoded image present frame slot {slot} exceeds {} in-flight fence(s)",
            resources.in_flight.len()
        )
    })?;
    let started_at = Instant::now();
    unsafe {
        device
            .wait_for_fences(&[fence], true, u64::MAX)
            .map_err(|err| {
                format!("vkWaitForFences(vulkanalia decoded image present layer release): {err:?}")
            })?;
    }
    Ok(native_vulkan_vulkanalia_elapsed_micros(started_at))
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_try_complete_decoded_image_present_frame_slot(
    device: &Device,
    resources: &VulkanaliaDecodedImagePresentFrameResources,
    present_frame_slot: u32,
) -> Result<bool, String> {
    let slot = present_frame_slot as usize;
    let fence = resources.in_flight.get(slot).copied().ok_or_else(|| {
        format!(
            "decoded image present frame slot {slot} exceeds {} in-flight fence(s)",
            resources.in_flight.len()
        )
    })?;
    let status = unsafe { device.get_fence_status(fence) }.map_err(|err| {
        format!("vkGetFenceStatus(vulkanalia decoded image present layer release): {err:?}")
    })?;
    if status == vk::SuccessCode::SUCCESS {
        Ok(true)
    } else if status == vk::SuccessCode::NOT_READY {
        Ok(false)
    } else {
        Err(format!(
            "vkGetFenceStatus(vulkanalia decoded image present layer release) returned unexpected status {status:?}"
        ))
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_decoded_image_present_frame_slot_count(
    resources: &VulkanaliaDecodedImagePresentFrameResources,
) -> usize {
    resources.in_flight.len()
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_decoded_image_present_command_pool(
    resources: &VulkanaliaDecodedImagePresentFrameResources,
) -> vk::CommandPool {
    resources.command_pool
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_prepare_decoded_image_present_frame_slot(
    device: &Device,
    resources: &VulkanaliaDecodedImagePresentFrameResources,
    present_frame_slot: u32,
) -> Result<u64, String> {
    let slot = present_frame_slot as usize;
    let fence = resources.in_flight.get(slot).copied().ok_or_else(|| {
        format!(
            "decoded image present frame slot {slot} exceeds {} in-flight fence(s)",
            resources.in_flight.len()
        )
    })?;
    let started_at = Instant::now();
    unsafe {
        device
            .wait_for_fences(&[fence], true, u64::MAX)
            .map_err(|err| format!("vkWaitForFences(vulkanalia decoded image present): {err:?}"))?;
        device
            .reset_fences(&[fence])
            .map_err(|err| format!("vkResetFences(vulkanalia decoded image present): {err:?}"))?;
    }
    Ok(native_vulkan_vulkanalia_elapsed_micros(started_at))
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_destroy_decoded_image_present_frame_resources(
    device: &Device,
    resources: VulkanaliaDecodedImagePresentFrameResources,
) {
    let _ = unsafe { device.device_wait_idle() };
    native_vulkan_vulkanalia_destroy_partial_decoded_image_present_frame_resources(
        device,
        resources.swapchain_image_views,
        resources.command_pool,
        resources.image_available,
        resources.render_finished,
        resources.in_flight,
        resources.decode_complete,
    );
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_destroy_partial_decoded_image_present_frame_resources(
    device: &Device,
    swapchain_image_views: Vec<vk::ImageView>,
    command_pool: vk::CommandPool,
    image_available: Vec<vk::Semaphore>,
    render_finished: Vec<vk::Semaphore>,
    in_flight: Vec<vk::Fence>,
    decode_complete: vk::Semaphore,
) {
    unsafe {
        if decode_complete != vk::Semaphore::null() {
            device.destroy_semaphore(decode_complete, None);
        }
        for fence in in_flight {
            if fence != vk::Fence::null() {
                device.destroy_fence(fence, None);
            }
        }
        for semaphore in render_finished {
            if semaphore != vk::Semaphore::null() {
                device.destroy_semaphore(semaphore, None);
            }
        }
        for semaphore in image_available {
            if semaphore != vk::Semaphore::null() {
                device.destroy_semaphore(semaphore, None);
            }
        }
        if command_pool != vk::CommandPool::null() {
            device.destroy_command_pool(command_pool, None);
        }
        for view in swapchain_image_views {
            device.destroy_image_view(view, None);
        }
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_create_present_swapchain_image_views(
    device: &Device,
    images: &[vk::Image],
    format: vk::Format,
) -> Result<Vec<vk::ImageView>, String> {
    let mut views = Vec::with_capacity(images.len());
    for image in images {
        let create_info = vk::ImageViewCreateInfo::builder()
            .image(*image)
            .view_type(vk::ImageViewType::_2D)
            .format(format)
            .subresource_range(native_vulkan_vulkanalia_color_subresource_range());
        match unsafe { device.create_image_view(&create_info, None) } {
            Ok(view) => views.push(view),
            Err(err) => {
                for view in views {
                    unsafe {
                        device.destroy_image_view(view, None);
                    }
                }
                return Err(format!(
                    "vkCreateImageView(vulkanalia decoded image present swapchain): {err:?}"
                ));
            }
        }
    }
    Ok(views)
}
