use super::*;

pub(super) fn with_scene_present(
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
    let frame_slot_count = scene_frame_slot_count(
        options.capture_frame.is_some(),
        options.gpu_timing,
        std::env::var_os("GILDER_NATIVE_VULKAN_SCENE_ALLOW_MULTISLOT_CAPTURE").is_some(),
    );

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
    let command_buffer_count = frame_slot_count.saturating_add(1) as u32;
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
    let present_command_buffers = &command_buffers[..frame_slot_count];

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
        present_device
            .feature_selection
            .vulkan_1_4_properties
            .max_sampler_anisotropy_x1,
        &present_device.feature_selection.descriptor_heap_properties,
        present_device
            .feature_selection
            .blend_operation_advanced_enabled,
        present_device
            .feature_selection
            .blend_operation_advanced_coherent_operations,
        options.capture_scene_graph,
        frame_slot_count,
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
    let mut semantic_resolver =
        crate::engine::scene::semantic_world::SemanticFrameResolver::from_world(&semantic_world)
            .expect("scene semantic frame was validated during Vulkan GPU setup");
    let frame_contexts = match create_scene_present_frame_contexts(device, present_command_buffers)
    {
        Ok(contexts) => contexts,
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
            return Err(err);
        }
    };
    let semaphore_info = vk::SemaphoreCreateInfo::builder();
    let mut render_finished = Vec::with_capacity(swapchain_images.len());
    for image_index in 0..swapchain_images.len() {
        match unsafe { device.create_semaphore(&semaphore_info, None) } {
            Ok(semaphore) => render_finished.push(semaphore),
            Err(err) => {
                destroy_scene_gpu_resources(device, scene_resources);
                destroy_scene_present_frame_contexts(device, frame_contexts);
                unsafe {
                    for semaphore in render_finished {
                        device.destroy_semaphore(semaphore, None);
                    }
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
                destroy_scene_present_frame_contexts(device, frame_contexts);
                unsafe {
                    for semaphore in render_finished {
                        device.destroy_semaphore(semaphore, None);
                    }
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
    let effect_timing_commands = options
        .gpu_timing
        .then(|| {
            effect_target::scene_effect_target_timing_commands(
                &scene_resources.effect_target_commands,
                &scene_resources.graph_execution_order,
            )
        })
        .unwrap_or_default();
    let mut gpu_timing = SceneGpuTiming::create(
        device,
        instance,
        selection.physical_device,
        selection.queue_family_index,
        options.gpu_timing,
        &scene_resources.graph_execution_order,
        &effect_timing_commands,
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
    let mut semantic_resolve_total_micros = 0u64;
    let mut graph_update_total_micros = 0u64;
    let mut transform_update_total_micros = 0u64;
    let mut material_update_total_micros = 0u64;
    let mut skinning_update_total_micros = 0u64;
    let mut draw_policy_update_total_micros = 0u64;
    let mut sampled_descriptor_update_count = 0u64;
    let mut sampled_descriptor_update_total_micros = 0u64;
    let mut command_recording_total_micros = 0u64;
    let mut fence_wait_total_micros = 0u64;
    let mut acquire_wait_total_micros = 0u64;
    let mut queue_present_total_micros = 0u64;
    let mut composite_scissor_draw_count = 0usize;
    let mut composite_scissor_covered_pixels = 0u64;
    let mut composite_scissor_avoided_pixels = 0u64;
    let mut scene_color_attachment_clear_frame_count = 0u64;
    let mut image_layouts = vec![vk::ImageLayout::UNDEFINED; swapchain_images.len()];
    let fixed_scene_time_seconds = std::env::var("GILDER_NATIVE_VULKAN_SCENE_FIXED_TIME")
        .ok()
        .and_then(|value| value.parse::<f32>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0);

    while Instant::now() < deadline {
        system_audio_monitor.publish_latest();
        let frame_slot = frames_presented as usize % frame_contexts.len();
        let frame_context = frame_contexts[frame_slot];
        scene_resources.active_frame_slot = frame_slot;
        let fence_wait_started = Instant::now();
        unsafe {
            device
                .wait_for_fences(&[frame_context.fence], true, u64::MAX)
                .map_err(|err| format!("vkWaitForFences(vulkanalia scene present): {err:?}"))?;
            device
                .reset_fences(&[frame_context.fence])
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
        let frame_resources = &scene_resources.frame_resources[frame_slot];
        let frame_update = write_scene_frame_buffers(
            device,
            &options.storage,
            &semantic_world,
            &mut semantic_resolver,
            &mut scene_resources.frame_topology,
            &mut scene_resources.draw_commands,
            &frame_resources.transform_buffer,
            frame_resources.material_buffer.as_ref(),
            frame_resources.skinning_buffer.as_ref(),
            scene_resources.dynamic_effect_uniforms,
            options.gpu_timing,
            &scene_resources.graph_execution_order,
            scene_resources.scene_color_attachment_clear_enabled,
            scene_time_seconds,
            [swapchain_plan.extent.width, swapchain_plan.extent.height],
        )?;
        scene_resources.scene_color_attachment_clear = frame_update.scene_color_attachment_clear;
        scene_color_attachment_clear_frame_count = scene_color_attachment_clear_frame_count
            .saturating_add(u64::from(
                frame_update.scene_color_attachment_clear.is_some(),
            ));
        frame_state_update_total_micros = frame_state_update_total_micros
            .saturating_add(elapsed_micros_u64(frame_state_update_started));
        semantic_resolve_total_micros = semantic_resolve_total_micros
            .saturating_add(frame_update.cpu_timing.semantic_resolve_micros);
        graph_update_total_micros = graph_update_total_micros
            .saturating_add(frame_update.cpu_timing.graph_update_micros);
        transform_update_total_micros = transform_update_total_micros
            .saturating_add(frame_update.cpu_timing.transform_update_micros);
        material_update_total_micros = material_update_total_micros
            .saturating_add(frame_update.cpu_timing.material_update_micros);
        skinning_update_total_micros = skinning_update_total_micros
            .saturating_add(frame_update.cpu_timing.skinning_update_micros);
        draw_policy_update_total_micros = draw_policy_update_total_micros
            .saturating_add(frame_update.cpu_timing.draw_policy_update_micros);
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
            device.acquire_next_image_khr(
                swapchain,
                u64::MAX,
                frame_context.image_available,
                vk::Fence::null(),
            )
        }
        .map_err(|err| format!("vkAcquireNextImageKHR(vulkanalia scene present): {err:?}"))?;
        acquire_wait_total_micros =
            acquire_wait_total_micros.saturating_add(elapsed_micros_u64(acquire_wait_started));
        let image_index = image_index as usize;
        let sampled_descriptor_update_started = Instant::now();
        let sampled_descriptor_updates = write_scene_frame_sampled_descriptors(
            device,
            &mut scene_resources,
            frame_slot,
            reference_phase,
            swapchain_images[image_index],
            swapchain_plan.format.format,
        )?;
        sampled_descriptor_update_count = sampled_descriptor_update_count
            .saturating_add(sampled_descriptor_updates as u64);
        sampled_descriptor_update_total_micros = sampled_descriptor_update_total_micros
            .saturating_add(elapsed_micros_u64(sampled_descriptor_update_started));
        let render_finished = *render_finished.get(image_index).ok_or_else(|| {
            format!("swapchain image index {image_index} has no present semaphore")
        })?;
        let command_buffer = frame_context.command_buffer;
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
            frame_context.image_available,
            render_finished,
            frame_context.fence,
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
                    .wait_for_fences(&[frame_context.fence], true, u64::MAX)
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
    let transform_uniform_bytes = scene_resources
        .frame_resources
        .iter()
        .map(|frame| frame.transform_buffer.snapshot.requested_bytes)
        .sum();
    let material_uniform_bytes = scene_resources
        .frame_resources
        .iter()
        .filter_map(|frame| frame.material_buffer.as_ref())
        .map(|buffer| buffer.snapshot.requested_bytes)
        .sum();
    let skinning_storage_bytes = scene_resources
        .frame_resources
        .iter()
        .filter_map(|frame| frame.skinning_buffer.as_ref())
        .map(|buffer| buffer.snapshot.requested_bytes)
        .sum();
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
    let scene_color_attachment_clear_draw_count = usize::from(
        scene_resources.scene_color_attachment_clear.is_some(),
    );
    let scene_color_recorded_mesh_draw_count =
        scene_color_mesh_draw_count.saturating_sub(scene_color_attachment_clear_draw_count);
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
        scene_resources
            .frame_resources
            .first()
            .is_some_and(|frame| frame.skinning_buffer.is_some()),
        scene_pipeline_count > 1,
        scene_resources.dynamic_effect_uniforms,
        effect_target_dynamic_rendering_recorded,
        effect_target_copy_command_count > 0,
        effect_target_swap_reference_count > 0,
        effect_target_mesh_draw_count > 0,
        effect_target_fullscreen_draw_count > 0,
    );

    destroy_scene_present_frame_contexts(device, frame_contexts);
    unsafe {
        for semaphore in render_finished {
            device.destroy_semaphore(semaphore, None);
        }
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
        frame_slot_count,
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
        scene_color_recorded_mesh_draw_count,
        scene_color_attachment_clear_draw_count,
        scene_color_attachment_clear_frame_count,
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
        semantic_incremental_resolve_enabled: semantic_resolver.incremental_enabled(),
        semantic_retained_puppet_resolve_enabled: semantic_resolver.retained_puppet_enabled(),
        semantic_dynamic_entity_count: semantic_resolver.dynamic_entity_count(),
        semantic_resolve_total_micros,
        graph_update_total_micros,
        transform_update_total_micros,
        material_update_total_micros,
        skinning_update_total_micros,
        draw_policy_update_total_micros,
        sampled_descriptor_update_count,
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
