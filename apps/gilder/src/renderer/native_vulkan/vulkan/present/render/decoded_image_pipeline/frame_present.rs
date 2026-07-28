use super::*;

#[allow(clippy::too_many_arguments)]
pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_present_decoded_image_once(
    device: &Device,
    queue: vk::Queue,
    queue_family_index: u32,
    swapchain: vk::SwapchainKHR,
    swapchain_images: &[vk::Image],
    swapchain_format: vk::Format,
    swapchain_extent: vk::Extent2D,
    resource_image: &VulkanaliaVideoSessionResourceImage,
    sampler: &VulkanaliaDecodedImagePresentSamplerResources,
    pipeline: &VulkanaliaDecodedImagePresentPipelineResources,
    present_timing: VulkanaliaDecodedImagePresentTimingConfig,
    clear_color: NativeVulkanClearColor,
) -> Result<NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot, String> {
    let frame_resources = native_vulkan_vulkanalia_create_decoded_image_present_frame_resources(
        device,
        swapchain_images,
        swapchain_format,
        queue_family_index,
    )?;
    let result = native_vulkan_vulkanalia_present_decoded_image_frame(
        device,
        queue,
        swapchain,
        swapchain_images,
        swapchain_format,
        swapchain_extent,
        resource_image,
        sampler,
        pipeline,
        &frame_resources,
        sampler.snapshot.sampled_array_layer,
        0,
        false,
        None,
        None,
        None,
        None,
        0,
        "single-frame-present",
        0,
        "unpaced-single-frame-smoke",
        present_timing,
        vk::Semaphore::null(),
        0,
        None,
        None,
        clear_color,
        None,
    );
    native_vulkan_vulkanalia_destroy_decoded_image_present_frame_resources(device, frame_resources);
    result
}

#[allow(clippy::too_many_arguments)]
pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_present_decoded_image_frame(
    device: &Device,
    queue: vk::Queue,
    swapchain: vk::SwapchainKHR,
    swapchain_images: &[vk::Image],
    swapchain_format: vk::Format,
    swapchain_extent: vk::Extent2D,
    resource_image: &VulkanaliaVideoSessionResourceImage,
    sampler: &VulkanaliaDecodedImagePresentSamplerResources,
    pipeline: &VulkanaliaDecodedImagePresentPipelineResources,
    frame_resources: &VulkanaliaDecodedImagePresentFrameResources,
    sampled_array_layer: u32,
    present_frame_index: u32,
    present_frame_slot_prepared: bool,
    source_frame_pts_ns: Option<u64>,
    source_frame_duration_ns: Option<u64>,
    source_frame_pts_ms: Option<u64>,
    source_frame_duration_ms: Option<u64>,
    display_order_key: i64,
    display_order_key_source: &'static str,
    pacing_sleep_micros: u64,
    pacing_clock_model: &'static str,
    present_timing: VulkanaliaDecodedImagePresentTimingConfig,
    decode_complete_semaphore: vk::Semaphore,
    decode_complete_value: u64,
    queue_host_access_lock: Option<&Mutex<()>>,
    after_render_submit_before_present: Option<&mut dyn FnMut(u32) -> Result<(), String>>,
    clear_color: NativeVulkanClearColor,
    scene_overlay_draw: Option<VulkanaliaSceneVideoOverlayFrameDraw<'_>>,
) -> Result<NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot, String> {
    let decoded_sources = [VulkanaliaDecodedImagePresentSource {
        image: VulkanaliaDecodedImagePresentImageSource::from_resource_image(resource_image),
        sampler,
        sampled_array_layer,
    }];
    let decode_waits = [VulkanaliaDecodedImagePresentDecodeWait {
        semaphore: decode_complete_semaphore,
        value: decode_complete_value,
    }];
    native_vulkan_vulkanalia_present_decoded_image_frame_with_sources(
        device,
        queue,
        swapchain,
        swapchain_images,
        swapchain_format,
        swapchain_extent,
        pipeline,
        frame_resources,
        &decoded_sources,
        sampled_array_layer,
        present_frame_index,
        present_frame_slot_prepared,
        source_frame_pts_ns,
        source_frame_duration_ns,
        source_frame_pts_ms,
        source_frame_duration_ms,
        display_order_key,
        display_order_key_source,
        pacing_sleep_micros,
        pacing_clock_model,
        present_timing,
        &decode_waits,
        queue_host_access_lock,
        after_render_submit_before_present,
        clear_color,
        scene_overlay_draw,
    )
}

#[allow(clippy::too_many_arguments)]
pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_present_decoded_image_frame_with_sources(
    device: &Device,
    queue: vk::Queue,
    swapchain: vk::SwapchainKHR,
    swapchain_images: &[vk::Image],
    swapchain_format: vk::Format,
    swapchain_extent: vk::Extent2D,
    pipeline: &VulkanaliaDecodedImagePresentPipelineResources,
    frame_resources: &VulkanaliaDecodedImagePresentFrameResources,
    decoded_sources: &[VulkanaliaDecodedImagePresentSource<'_>],
    sampled_array_layer: u32,
    present_frame_index: u32,
    present_frame_slot_prepared: bool,
    source_frame_pts_ns: Option<u64>,
    source_frame_duration_ns: Option<u64>,
    source_frame_pts_ms: Option<u64>,
    source_frame_duration_ms: Option<u64>,
    display_order_key: i64,
    display_order_key_source: &'static str,
    pacing_sleep_micros: u64,
    pacing_clock_model: &'static str,
    present_timing: VulkanaliaDecodedImagePresentTimingConfig,
    decode_waits: &[VulkanaliaDecodedImagePresentDecodeWait],
    queue_host_access_lock: Option<&Mutex<()>>,
    mut after_render_submit_before_present: Option<&mut dyn FnMut(u32) -> Result<(), String>>,
    clear_color: NativeVulkanClearColor,
    scene_overlay_draw: Option<VulkanaliaSceneVideoOverlayFrameDraw<'_>>,
) -> Result<NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot, String> {
    if swapchain_images.is_empty() {
        return Err("decoded image present requires at least one swapchain image".to_owned());
    }
    if swapchain_extent.width == 0 || swapchain_extent.height == 0 {
        return Err("decoded image present requires non-zero swapchain extent".to_owned());
    }
    if decoded_sources.is_empty() {
        return Err("decoded image present requires at least one decoded source".to_owned());
    }
    for (source_index, source) in decoded_sources.iter().enumerate() {
        if source.sampled_array_layer >= source.image.array_layers {
            return Err(format!(
                "decoded image present source {source_index} sampled layer {} exceeds {} image layers",
                source.sampled_array_layer, source.image.array_layers
            ));
        }
    }
    if frame_resources.swapchain_image_views.len() != swapchain_images.len() {
        return Err(format!(
            "decoded image present frame resource image-view count {} does not match swapchain image count {}",
            frame_resources.swapchain_image_views.len(),
            swapchain_images.len()
        ));
    }
    let frame_slot_count = frame_resources.in_flight.len();
    if frame_slot_count == 0
        || frame_resources.image_available.len() != frame_slot_count
        || frame_resources.render_finished.len() != frame_slot_count
    {
        return Err(format!(
            "decoded image present frame slots are inconsistent: image_available={}, render_finished={}, in_flight={}",
            frame_resources.image_available.len(),
            frame_resources.render_finished.len(),
            frame_resources.in_flight.len()
        ));
    }
    let present_frame_slot = present_frame_index as usize % frame_slot_count;
    let image_available = frame_resources.image_available[present_frame_slot];
    let in_flight = frame_resources.in_flight[present_frame_slot];
    let present_call_started_at = Instant::now();

    let mut present_wait_frame_slot_micros = if present_frame_slot_prepared {
        0
    } else {
        native_vulkan_vulkanalia_prepare_decoded_image_present_frame_slot(
            device,
            frame_resources,
            present_frame_slot as u32,
        )?
    };
    {
        let mut swapchain_image_in_flight = frame_resources
            .swapchain_image_in_flight
            .lock()
            .map_err(|_| {
                "decoded image present swapchain-image fence cache is poisoned".to_owned()
            })?;
        for cached_fence in swapchain_image_in_flight.iter_mut() {
            if *cached_fence == in_flight {
                *cached_fence = vk::Fence::null();
            }
        }
    }
    let stage_started_at = Instant::now();
    let (image_index, _) = unsafe {
        device.acquire_next_image_khr(swapchain, u64::MAX, image_available, vk::Fence::null())
    }
    .map_err(|err| format!("vkAcquireNextImageKHR(vulkanalia decoded image present): {err:?}"))?;
    let present_acquire_next_image_micros =
        native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
    let image_index_usize = image_index as usize;
    let render_finished = frame_resources
        .render_finished
        .get(image_index_usize)
        .copied()
        .ok_or_else(|| {
            format!("swapchain image index {image_index_usize} has no present semaphore")
        })?;
    let previous_swapchain_image_fence = {
        let swapchain_image_in_flight =
            frame_resources
                .swapchain_image_in_flight
                .lock()
                .map_err(|_| {
                    "decoded image present swapchain-image fence cache is poisoned".to_owned()
                })?;
        swapchain_image_in_flight
            .get(image_index_usize)
            .copied()
            .ok_or_else(|| {
                format!("swapchain image index {image_index_usize} has no tracked fence")
            })?
    };
    if previous_swapchain_image_fence != vk::Fence::null()
        && previous_swapchain_image_fence != in_flight
    {
        let stage_started_at = Instant::now();
        unsafe {
            device
                .wait_for_fences(&[previous_swapchain_image_fence], true, u64::MAX)
                .map_err(|err| {
                    format!(
                        "vkWaitForFences(vulkanalia decoded image present swapchain image reuse): {err:?}"
                    )
                })?;
        }
        present_wait_frame_slot_micros = present_wait_frame_slot_micros
            .saturating_add(native_vulkan_vulkanalia_elapsed_micros(stage_started_at));
    }
    let command_buffer = frame_resources
        .command_buffers
        .get(image_index_usize)
        .copied()
        .ok_or_else(|| {
            format!("swapchain image index {image_index_usize} has no command buffer")
        })?;
    let swapchain_image = *swapchain_images
        .get(image_index_usize)
        .ok_or_else(|| format!("swapchain image index {image_index_usize} is unavailable"))?;
    let swapchain_view = *frame_resources
        .swapchain_image_views
        .get(image_index_usize)
        .ok_or_else(|| format!("swapchain view index {image_index_usize} is unavailable"))?;

    let stage_started_at = Instant::now();
    native_vulkan_vulkanalia_record_decoded_image_present_command_buffer(
        device,
        command_buffer,
        swapchain_image,
        swapchain_view,
        swapchain_extent,
        decoded_sources,
        frame_resources.present_queue_family_index,
        pipeline,
        clear_color,
        scene_overlay_draw,
    )?;
    let present_record_command_buffer_micros =
        native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
    let stage_started_at = Instant::now();
    let queue_host_access_guard =
        if let Some(lock) = queue_host_access_lock {
            Some(lock.lock().map_err(|_| {
                "decoded image present queue host-access lock is poisoned".to_owned()
            })?)
        } else {
            None
        };
    native_vulkan_vulkanalia_submit_decoded_image_present_command_buffer2(
        device,
        queue,
        command_buffer,
        image_available,
        render_finished,
        in_flight,
        decode_waits,
    )?;
    {
        let mut swapchain_image_in_flight = frame_resources
            .swapchain_image_in_flight
            .lock()
            .map_err(|_| {
                "decoded image present swapchain-image fence cache is poisoned".to_owned()
            })?;
        let slot = swapchain_image_in_flight
            .get_mut(image_index_usize)
            .ok_or_else(|| {
                format!("swapchain image index {image_index_usize} has no tracked fence")
            })?;
        *slot = in_flight;
    }
    let present_submit_command_buffer_micros =
        native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
    // FFmpeg/libplacebo unmaps the AVFrame immediately after the rendered
    // frame is submitted/swapped, not after the next FIFO pacing wait
    // (references/gilder/ffmpeg/fftools/ffplay_renderer.c:780-786).
    let after_render_submit_before_present_result =
        if let Some(after_render_submit_before_present) =
            after_render_submit_before_present.as_deref_mut()
        {
            after_render_submit_before_present(present_frame_slot as u32)
        } else {
            Ok(())
        };

    let swapchains = [swapchain];
    let image_indices = [image_index];
    let wait_semaphores = [render_finished];
    let present_id = present_timing.present_id(present_frame_index);
    let present_ids = [present_id.unwrap_or(0)];
    let mut present_id2_info = present_id.map(|_| {
        vk::PresentId2KHR::builder()
            .present_ids(&present_ids)
            .build()
    });
    let mut present_info = vk::PresentInfoKHR::builder()
        .wait_semaphores(&wait_semaphores)
        .swapchains(&swapchains)
        .image_indices(&image_indices);
    if present_timing.present_id2_enabled {
        if let Some(present_id2_info) = present_id2_info.as_mut() {
            present_info = present_info.push_next(present_id2_info);
        }
    }
    let stage_started_at = Instant::now();
    unsafe {
        device
            .queue_present_khr(queue, &present_info)
            .map_err(|err| {
                format!("vkQueuePresentKHR(vulkanalia decoded image present): {err:?}")
            })?;
    }
    drop(queue_host_access_guard);
    let present_queue_present_micros = native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
    after_render_submit_before_present_result?;
    let stage_started_at = Instant::now();
    let present_wait_after_present = present_timing.wait_after_queue_present(
        device,
        swapchain,
        present_id,
        "decoded image present",
    )?;
    let present_wait_after_queue_present_micros =
        native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
    let present_call_total_micros =
        native_vulkan_vulkanalia_elapsed_micros(present_call_started_at);
    let scene_video_layer_draw_enabled =
        scene_overlay_draw.is_some_and(|draw| draw.video_layer.is_some());
    let scene_overlay_blend_draw_enabled = false;

    Ok(NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot {
        binding: "vulkanalia",
        route: "decoded-image-dynamic-rendering-present-draw",
        present_frame_index,
        sampled_array_layer,
        sampled_array_layer_source: "submitted-dst-base-array-layer-via-draw-first-instance",
        source_frame_pts_ns,
        source_frame_duration_ns,
        source_frame_pts_ms,
        source_frame_duration_ms,
        display_order_key,
        display_order_key_source,
        pacing_sleep_micros,
        pacing_clock_model,
        present_call_total_micros,
        present_wait_frame_slot_micros,
        present_acquire_next_image_micros,
        present_record_command_buffer_micros,
        present_submit_command_buffer_micros,
        present_queue_present_micros,
        present_wait_after_queue_present_micros,
        present_frame_slot: present_frame_slot as u32,
        present_sync_model: "frame-slot semaphore/fence reuse; no per-present queue_wait_idle",
        wait_idle_after_present: false,
        present_id,
        present_id_mode: present_timing.present_id_mode(),
        uses_present_id2: present_timing.present_id2_enabled,
        present_wait2_available: present_timing.present_wait2_enabled,
        present_wait_after_present,
        swapchain_image_index: image_index,
        swapchain_image_view_count: frame_resources.swapchain_image_views.len(),
        target_format: format!("{swapchain_format:?}"),
        extent: (swapchain_extent.width, swapchain_extent.height),
        clear_color: [clear_color.r, clear_color.g, clear_color.b, clear_color.a],
        command_buffer_recorded: true,
        submitted: true,
        presented: true,
        decoded_image_layout_transition: "video-decode-dpb -> shader-read-only-optimal -> video-decode-dpb",
        swapchain_layout_transition: "undefined -> color-attachment-optimal -> present-src-khr",
        render_model: if scene_video_layer_draw_enabled {
            "VK_EXT_descriptor_heap retained Y/UV plane-array sampler mapping plus native scene video layer indexed quads -> Vulkan 1.4 dynamic rendering pass -> Wayland swapchain"
        } else if scene_overlay_blend_draw_enabled {
            "VK_EXT_descriptor_heap retained Y/UV plane-array sampler mapping plus native scene overlay draw -> Vulkan 1.4 dynamic rendering pass -> Wayland swapchain"
        } else {
            "VK_EXT_descriptor_heap retained Y/UV plane-array sampler mapping -> Vulkan 1.4 dynamic rendering fullscreen triangle -> Wayland swapchain"
        },
        command_order: native_vulkan_vulkanalia_decoded_image_present_command_order(
            true,
            present_timing.present_id_mode(),
            present_timing.present_wait_mode(),
            scene_video_layer_draw_enabled,
            scene_overlay_blend_draw_enabled,
        ),
        uses_pipeline_rendering_create_info: true,
        uses_dynamic_rendering: true,
        uses_synchronization2: true,
        uses_submit2: true,
        zero_copy_presented: true,
        descriptor_model: "VK_EXT_descriptor_heap",
        ffmpeg_reference: FFMPEG_VULKAN_DECODE_REFERENCE,
    })
}

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

pub(super) fn native_vulkan_vulkanalia_destroy_partial_decoded_image_present_frame_resources(
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

pub(super) fn native_vulkan_vulkanalia_create_present_swapchain_image_views(
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

#[allow(clippy::too_many_arguments)]
pub(super) fn native_vulkan_vulkanalia_record_decoded_image_present_command_buffer(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    swapchain_image: vk::Image,
    swapchain_view: vk::ImageView,
    extent: vk::Extent2D,
    decoded_sources: &[VulkanaliaDecodedImagePresentSource<'_>],
    present_queue_family_index: u32,
    pipeline: &VulkanaliaDecodedImagePresentPipelineResources,
    clear_color: NativeVulkanClearColor,
    scene_overlay_draw: Option<VulkanaliaSceneVideoOverlayFrameDraw<'_>>,
) -> Result<(), String> {
    if decoded_sources.is_empty() {
        return Err("decoded image present command buffer requires decoded sources".to_owned());
    }
    unsafe {
        device
            .reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty())
            .map_err(|err| {
                format!("vkResetCommandBuffer(vulkanalia decoded image present): {err:?}")
            })?;
        let begin_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
            .build();
        device
            .begin_command_buffer(command_buffer, &begin_info)
            .map_err(|err| {
                format!("vkBeginCommandBuffer(vulkanalia decoded image present): {err:?}")
            })?;

        let mut image_barriers = Vec::with_capacity(decoded_sources.len().saturating_add(1));
        for source in decoded_sources {
            let (src_queue_family_index, dst_queue_family_index) =
                native_vulkan_vulkanalia_decoded_image_present_queue_family_barrier_indices(
                    source.image,
                    present_queue_family_index,
                )?;
            image_barriers.push(
                vk::ImageMemoryBarrier2::builder()
                    .src_stage_mask(vk::PipelineStageFlags2::NONE)
                    .src_access_mask(vk::AccessFlags2::NONE)
                    .dst_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
                    .dst_access_mask(vk::AccessFlags2::SHADER_SAMPLED_READ)
                    .old_layout(source.image.current_layout)
                    .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .src_queue_family_index(src_queue_family_index)
                    .dst_queue_family_index(dst_queue_family_index)
                    .image(source.image.image)
                    .subresource_range(
                        native_vulkan_vulkanalia_decoded_image_layer_subresource_range(
                            source.sampled_array_layer,
                        ),
                    )
                    .build(),
            );
        }
        let swapchain_to_attachment = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(vk::PipelineStageFlags2::TOP_OF_PIPE)
            .src_access_mask(vk::AccessFlags2::empty())
            .dst_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(swapchain_image)
            .subresource_range(native_vulkan_vulkanalia_color_subresource_range())
            .build();
        image_barriers.push(swapchain_to_attachment);
        let dependency = vk::DependencyInfo::builder()
            .image_memory_barriers(&image_barriers)
            .build();
        device.cmd_pipeline_barrier2(command_buffer, &dependency);

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
        let scene_video_layer_draw = scene_overlay_draw.and_then(|draw| draw.video_layer);
        if let Some(scene_video_layer_draw) = scene_video_layer_draw {
            native_vulkan_vulkanalia_record_decoded_image_scene_video_layer_draws_inside_rendering(
                device,
                command_buffer,
                extent,
                &pipeline.scene_video_layer,
                scene_video_layer_draw,
                decoded_sources,
            )?;
        } else {
            let fullscreen_source = match decoded_sources {
                [source] => source,
                _ => {
                    return Err(
                        "fullscreen decoded-image present requires exactly one decoded source; multi-source scene present must provide video-layer draw commands"
                            .to_owned(),
                    );
                }
            };
            let resource_bind = native_vulkan_vulkanalia_descriptor_heap_resource_bind_info(
                &fullscreen_source.sampler.descriptor_heap,
            );
            let sampler_bind = native_vulkan_vulkanalia_descriptor_heap_sampler_bind_info(
                &fullscreen_source.sampler.descriptor_heap,
            );
            device.cmd_bind_resource_heap_ext(command_buffer, &resource_bind);
            device.cmd_bind_sampler_heap_ext(command_buffer, &sampler_bind);
            device.cmd_bind_pipeline(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline.pipeline,
            );
            device.cmd_draw(
                command_buffer,
                3,
                1,
                0,
                fullscreen_source.sampled_array_layer,
            );
        }
        if let Some(scene_overlay_draw) = scene_overlay_draw {
            if let Some(video_layer) = scene_overlay_draw.video_layer {
                native_vulkan_vulkanalia_record_decoded_image_scene_video_layer_draws_inside_rendering(
                    device,
                    command_buffer,
                    extent,
                    &pipeline.scene_video_layer,
                    video_layer,
                    decoded_sources,
                )?;
            }
        }
        device.cmd_end_rendering(command_buffer);

        let mut present_barriers = Vec::with_capacity(decoded_sources.len().saturating_add(1));
        for source in decoded_sources {
            let (src_queue_family_index, dst_queue_family_index) =
                native_vulkan_vulkanalia_decoded_image_present_queue_family_barrier_indices(
                    source.image,
                    present_queue_family_index,
                )?;
            present_barriers.push(
                vk::ImageMemoryBarrier2::builder()
                    .src_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
                    .src_access_mask(vk::AccessFlags2::SHADER_SAMPLED_READ)
                    .dst_stage_mask(vk::PipelineStageFlags2::NONE)
                    .dst_access_mask(vk::AccessFlags2::NONE)
                    .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .new_layout(source.image.restore_layout)
                    .src_queue_family_index(dst_queue_family_index)
                    .dst_queue_family_index(src_queue_family_index)
                    .image(source.image.image)
                    .subresource_range(
                        native_vulkan_vulkanalia_decoded_image_layer_subresource_range(
                            source.sampled_array_layer,
                        ),
                    )
                    .build(),
            );
        }
        let swapchain_to_present = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::BOTTOM_OF_PIPE)
            .dst_access_mask(vk::AccessFlags2::empty())
            .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(swapchain_image)
            .subresource_range(native_vulkan_vulkanalia_color_subresource_range())
            .build();
        present_barriers.push(swapchain_to_present);
        let present_dependency = vk::DependencyInfo::builder()
            .image_memory_barriers(&present_barriers)
            .build();
        device.cmd_pipeline_barrier2(command_buffer, &present_dependency);

        device.end_command_buffer(command_buffer).map_err(|err| {
            format!("vkEndCommandBuffer(vulkanalia decoded image present): {err:?}")
        })?;
    }

    Ok(())
}

pub(super) fn native_vulkan_vulkanalia_record_decoded_image_scene_video_layer_draws_inside_rendering(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    extent: vk::Extent2D,
    pipeline: &VulkanaliaDecodedImageSceneVideoLayerPipelineResources,
    draw: VulkanaliaSceneVideoLayerFrameDraw<'_>,
    decoded_sources: &[VulkanaliaDecodedImagePresentSource<'_>],
) -> Result<u32, String> {
    if extent.width == 0 || extent.height == 0 {
        return Err("decoded scene video layer draw requires non-zero extent".to_owned());
    }
    if decoded_sources.is_empty() {
        return Err("decoded scene video layer draw requires decoded sources".to_owned());
    }
    if draw.draw_commands.is_empty() {
        return Err("decoded scene video layer draw requires at least one draw".to_owned());
    }
    for (draw_index, draw_command) in draw.draw_commands.iter().enumerate() {
        if draw_command.index_count == 0 {
            return Err("decoded scene video layer draw requires non-empty indices".to_owned());
        }
        if draw_command.resource_index as usize >= decoded_sources.len() {
            return Err(format!(
                "decoded scene video layer draw {draw_index} resource index {} exceeds decoded source count {}",
                draw_command.resource_index,
                decoded_sources.len()
            ));
        }
    }

    unsafe {
        device.cmd_bind_pipeline(
            command_buffer,
            vk::PipelineBindPoint::GRAPHICS,
            pipeline.pipeline,
        );
        let vertex_buffers = [draw.vertex_buffer];
        let vertex_offsets = [0u64];
        device.cmd_bind_vertex_buffers(command_buffer, 0, &vertex_buffers, &vertex_offsets);
        device.cmd_bind_index_buffer(command_buffer, draw.index_buffer, 0, vk::IndexType::UINT32);
        let push_constants = [extent.width as f32, extent.height as f32];
        let push_constant_bytes = std::slice::from_raw_parts(
            push_constants.as_ptr().cast::<u8>(),
            DECODED_IMAGE_SCENE_VIDEO_LAYER_PUSH_CONSTANT_BYTES as usize,
        );
        device.cmd_push_constants(
            command_buffer,
            pipeline.pipeline_layout,
            vk::ShaderStageFlags::VERTEX,
            0,
            push_constant_bytes,
        );
        let mut bound_resource_index = None;
        for draw_command in draw.draw_commands {
            if bound_resource_index != Some(draw_command.resource_index) {
                let source = &decoded_sources[draw_command.resource_index as usize];
                let resource_bind = native_vulkan_vulkanalia_descriptor_heap_resource_bind_info(
                    &source.sampler.descriptor_heap,
                );
                let sampler_bind = native_vulkan_vulkanalia_descriptor_heap_sampler_bind_info(
                    &source.sampler.descriptor_heap,
                );
                device.cmd_bind_resource_heap_ext(command_buffer, &resource_bind);
                device.cmd_bind_sampler_heap_ext(command_buffer, &sampler_bind);
                bound_resource_index = Some(draw_command.resource_index);
            }
            let sampled_array_layer =
                decoded_sources[draw_command.resource_index as usize].sampled_array_layer;
            device.cmd_draw_indexed(
                command_buffer,
                draw_command.index_count,
                1,
                draw_command.first_index,
                0,
                sampled_array_layer,
            );
        }
    }

    Ok(draw
        .draw_commands
        .iter()
        .fold(0u32, |sum, draw| sum.saturating_add(draw.index_count)))
}
