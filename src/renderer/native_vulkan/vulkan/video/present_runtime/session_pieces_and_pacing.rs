#[allow(clippy::too_many_arguments)]
fn create_video_present_session_pieces(
    instance: &Instance,
    vulkan: &NativeVulkanVulkanaliaInstance,
    context: &NativeVulkanVulkanaliaVideoPresentDeviceContext,
    selection: &super::video_present_device::NativeVulkanVulkanaliaVideoPresentPhysicalDeviceSelection,
    codec: NativeVulkanVideoSessionCodec,
    width: u32,
    height: u32,
    swapchain_handle: vk::SwapchainKHR,
    swapchain_images: &[vk::Image],
    swapchain_extent: vk::Extent2D,
    swapchain_format: vk::Format,
    target_max_fps: Option<u32>,
    audio_master_clock: NativeVulkanVulkanaliaVideoPresentAudioMasterClock,
    decoded_image_present_timing: VulkanaliaDecodedImagePresentTimingConfig,
    clear_color: NativeVulkanClearColor,
    swapchain: super::swapchain::NativeVulkanVulkanaliaSwapchainSnapshot,
    streaming_decode: NativeVulkanVulkanaliaStreamingDecodeRequests,
    requested_present_frame_count: u32,
    scene_video_overlay_input: Option<NativeVulkanVulkanaliaSceneVideoOverlayInput>,
) -> Result<NativeVulkanVulkanaliaVideoPresentSessionPieces, String> {
    if scene_video_overlay_input.is_some() {
        return Err(native_vulkan_scene_video_overlay_removed_error());
    }
    with_native_vulkan_vulkanalia_video_session_capabilities(
        instance,
        selection.physical_device,
        codec,
        None,
        None,
        |profile_info, queried| {
            let driver_session_max_dpb_slots = native_vulkan_vulkanalia_video_session_max_dpb_slots(
                queried.capabilities.max_dpb_slots,
            );
            let driver_session_max_active_reference_pictures =
                native_vulkan_vulkanalia_video_session_max_active_reference_pictures(
                    queried.capabilities.max_active_reference_pictures,
                    driver_session_max_dpb_slots,
                );
            #[cfg(feature = "native-vulkan-video")]
            let mut prepared_streaming_decode =
                native_vulkan_vulkanalia_prepare_streaming_decode_requests(
                    streaming_decode,
                    codec,
                    driver_session_max_dpb_slots,
                )?;
            #[cfg(not(feature = "native-vulkan-video"))]
            let prepared_streaming_decode =
                native_vulkan_vulkanalia_prepare_streaming_decode_requests(
                    streaming_decode,
                    codec,
                    driver_session_max_dpb_slots,
                )?;
            let requested_extent = prepared_streaming_decode
                .coded_extent()
                .unwrap_or(vk::Extent2D { width, height });
            let av1_sequence_header = prepared_streaming_decode.av1_sequence_header();
            if !native_vulkan_vulkanalia_video_session_extent_supported(
                requested_extent,
                queried.capabilities,
            ) {
                return Err(format!(
                    "requested Vulkanalia video present session extent {}x{} is outside driver capabilities",
                    requested_extent.width, requested_extent.height
                ));
            }
            let required_dpb_slots =
                prepared_streaming_decode.required_resource_image_array_layers();
            let session_max_dpb_slots = native_vulkan_vulkanalia_select_stream_session_dpb_slots(
                required_dpb_slots,
                driver_session_max_dpb_slots,
            )?;
            let required_active_reference_pictures =
                prepared_streaming_decode.required_max_active_reference_pictures();
            let session_max_active_reference_pictures =
                native_vulkan_vulkanalia_select_stream_session_active_reference_pictures(
                    required_active_reference_pictures,
                    driver_session_max_active_reference_pictures,
                    session_max_dpb_slots,
                )?;
            let resource_image_array_layers =
                native_vulkan_vulkanalia_select_stream_resource_image_array_layers(
                    required_dpb_slots,
                    session_max_dpb_slots,
                )?;
            let picture_format = native_vulkan_vulkanalia_video_session_effective_picture_format(
                codec,
                av1_sequence_header,
            );
            let video_session_create_flags = native_vulkan_vulkanalia_video_session_create_flags(
                context
                    .video_feature_selection
                    .inline_session_parameters_enabled,
            );
            let create_info = vk::VideoSessionCreateInfoKHR::builder()
                .flags(video_session_create_flags)
                .queue_family_index(selection.video_queue_family_index)
                .video_profile(profile_info)
                .picture_format(picture_format)
                .reference_picture_format(picture_format)
                .max_coded_extent(requested_extent)
                .max_dpb_slots(session_max_dpb_slots)
                .max_active_reference_pictures(session_max_active_reference_pictures)
                .std_header_version(&queried.capabilities.std_header_version)
                .build();
            let session =
                native_vulkan_vulkanalia_create_video_session(&context.device, &create_info)?;
            let mut session = Some(session);
            let mut memory_resources = None;
            let mut resource_image = None;
            let mut decoded_image_present_pipeline = None;
            let mut decoded_image_present_sampler = None;
            let mut decoded_image_present_frame_resources = None;
            let mut scene_video_overlay: Option<VulkanaliaSceneVideoOverlayResources> = None;
            let result = (|| -> Result<NativeVulkanVulkanaliaVideoPresentSessionPieces, String> {
                let memory_properties = unsafe {
                    instance.get_physical_device_memory_properties(selection.physical_device)
                };
                let resources = native_vulkan_vulkanalia_bind_video_session_memory_resources(
                    &context.device,
                    &memory_properties,
                    session
                        .as_ref()
                        .copied()
                        .expect("Vulkanalia video session is live"),
                )?;
                memory_resources = Some(resources);
                let resource_queue_family_indices = video_present_queue_family_indices(
                    selection.video_queue_family_index,
                    selection.present_queue_family_index,
                );
                let image = native_vulkan_vulkanalia_create_video_session_resource_image(
                    instance,
                    &context.device,
                    &memory_properties,
                    selection.physical_device,
                    profile_info,
                    requested_extent,
                    resource_image_array_layers,
                    picture_format,
                    queried.decode_capability_flags,
                    &resource_queue_family_indices,
                )?;
                resource_image = Some(image);
                let resource_image_ref = resource_image
                    .as_ref()
                    .expect("Vulkanalia resource image has just been created");
                let resource_image_snapshot =
                    NativeVulkanVulkanaliaVideoSessionResourceImageSmokeSnapshot {
                        image_created: true,
                        memory_bound: true,
                        image_view_created: resource_image_ref.view != vk::ImageView::default(),
                        layer_view_count: resource_image_ref.layer_views.len(),
                        resource_image: resource_image_ref.snapshot.clone(),
                    };
                let same_queue_family =
                    selection.video_queue_family_index == selection.present_queue_family_index;
                let same_queue_handle =
                    same_queue_family && context.video_queue_index == context.present_queue_index;
                let (decoded_image_present_sampler_snapshot, decoded_image_present_sampler_error) =
                    match native_vulkan_vulkanalia_create_decoded_image_present_sampler_resources(
                        &context.device,
                        &memory_properties,
                        resource_image_ref,
                        picture_format,
                        0,
                        selection.video_queue_family_index,
                        selection.present_queue_family_index,
                        context
                            .video_feature_selection
                            .core_features
                            .descriptor_heap,
                        context.video_feature_selection.descriptor_heap_properties,
                    ) {
                        Ok(resources) => {
                            let snapshot = resources.snapshot.clone();
                            decoded_image_present_sampler = Some(resources);
                            (Some(snapshot), None)
                        }
                        Err(err) => (None, Some(err)),
                    };
                let (decoded_image_present_pipeline_snapshot, decoded_image_present_pipeline_error) =
                    if !context.video_feature_selection.dynamic_rendering_enabled {
                        (
                            None,
                            Some(
                                "dynamicRendering feature is unavailable on selected Vulkanalia video+present device"
                                    .to_owned(),
                            ),
                        )
                    } else if let Some(sampler) = decoded_image_present_sampler.as_ref() {
                        let target_extent = vk::Extent2D {
                            width: swapchain.extent.0,
                            height: swapchain.extent.1,
                        };
                        match native_vulkan_vulkanalia_create_decoded_image_present_pipeline_resources(
                            &context.device,
                            swapchain_format,
                            target_extent,
                            &sampler.snapshot.descriptor_heap_plan,
                        ) {
                            Ok(resources) => {
                                let snapshot = resources.snapshot.clone();
                                decoded_image_present_pipeline = Some(resources);
                                (Some(snapshot), None)
                            }
                            Err(err) => (None, Some(err)),
                        }
                    } else {
                        (
                            None,
                            Some(
                                "decoded image present pipeline requires a live plane descriptor-heap sampler resource"
                                    .to_owned(),
                            ),
                        )
                    };
                let decoded_image_present_sequence_requested =
                    native_vulkan_vulkanalia_streaming_decode_requested(&prepared_streaming_decode);
                let mut decoded_image_present_sequence_error = None;
                let mut decoded_image_present_sequence = None;
                if decoded_image_present_sequence_requested {
                    if decoded_image_present_sampler.is_none() {
                        decoded_image_present_sequence_error = Some(
                            "decoded image present sequence requires a live plane descriptor-heap sampler resource"
                                .to_owned(),
                        );
                    } else if decoded_image_present_pipeline.is_none() {
                        decoded_image_present_sequence_error = Some(
                            "decoded image present sequence requires a live dynamic-rendering pipeline"
                                .to_owned(),
                        );
                    } else {
                        match native_vulkan_vulkanalia_create_decoded_image_present_frame_resources(
                            &context.device,
                            swapchain_images,
                            swapchain_format,
                            selection.present_queue_family_index,
                        ) {
                            Ok(resources) => {
                                decoded_image_present_frame_resources = Some(resources);
                            }
                            Err(err) => {
                                decoded_image_present_sequence_error = Some(err);
                            }
                        }
                    }
                }
                let memory_binding = memory_resources
                    .as_ref()
                    .expect("Vulkanalia session memory resources are live")
                    .snapshot
                    .clone();
                let (h264_ready_prefix_decode, h265_ready_prefix_decode, av1_ready_prefix_decode) = {
                    let requested_present_frame_count_for_sequence =
                        requested_present_frame_count.max(1);
                    let sequence_started_at = Instant::now();
                    let mut sequence_builder = if decoded_image_present_sequence_error.is_none()
                        && decoded_image_present_sequence_requested
                    {
                        Some(
                            NativeVulkanVulkanaliaDecodedImagePresentSequenceBuilder::new(
                                requested_present_frame_count_for_sequence,
                                sequence_started_at,
                            ),
                        )
                    } else {
                        None
                    };
                    let present_handoff = NativeVulkanVulkanaliaDecodedPresentHandoff::new(
                        FFMPEG_VIDEO_PICTURE_QUEUE_SIZE,
                        resource_image_array_layers as usize,
                    );
                    #[cfg(feature = "native-vulkan-video")]
                    let ffmpeg_decode_async_exec_depth =
                        native_vulkan_vulkanalia_ffmpeg_decode_async_exec_depth(
                            selection.video_queue_count,
                        );
                    let queue_host_access_mutex = Mutex::new(());
                    let queue_host_access_lock =
                        same_queue_handle.then_some(&queue_host_access_mutex);
                    #[cfg(feature = "native-vulkan-video")]
                    let decode_async_exec_depth_for_sequence = ffmpeg_decode_async_exec_depth;
                    #[cfg(not(feature = "native-vulkan-video"))]
                    let decode_async_exec_depth_for_sequence = 0;
                    let sequence_execution_evidence =
                        NativeVulkanVulkanaliaDecodedImagePresentExecutionEvidence {
                            ffmpeg_read_thread_active: decoded_image_present_sequence_requested,
                            video_decode_worker_active: decoded_image_present_sequence_requested,
                            present_worker_active: sequence_builder.is_some(),
                            source_count: 1,
                            decode_thread_count: FFMPEG_SINGLE_DECODE_THREAD_COUNT,
                            decode_async_exec_depth: decode_async_exec_depth_for_sequence,
                            ffmpeg_retained_avframe_count: 0,
                            ffmpeg_retained_avframe_peak_count: 0,
                            descriptor_sampler_cache_entry_count: 0,
                            descriptor_sampler_cache_peak_entry_count: 0,
                            descriptor_sampler_cache_rewrite_count: 0,
                            descriptor_sampler_cache_recreate_count: 0,
                            descriptor_sampler_cache_resource_heap_bytes: 0,
                            descriptor_sampler_cache_sampler_heap_bytes: 0,
                        };
                    // Persistent timeline semaphore shared by the decode submits and the
                    // present submits. Seed the per-frame counter from its current value so
                    // signalled values stay strictly increasing across present sequences.
                    let decode_complete_semaphore = decoded_image_present_frame_resources
                        .as_ref()
                        .map(|frame_resources| frame_resources.decode_complete_semaphore())
                        .unwrap_or_else(vk::Semaphore::null);
                    #[cfg(feature = "native-vulkan-video")]
                    let decode_complete_value = std::cell::Cell::new(
                        if decode_complete_semaphore != vk::Semaphore::null() {
                            unsafe {
                                context
                                    .device
                                    .get_semaphore_counter_value(decode_complete_semaphore)
                            }
                            .map_err(|err| {
                                format!("vkGetSemaphoreCounterValue(decode_complete): {err:?}")
                            })?
                        } else {
                            0
                        },
                    );
                    let mut completed_sequence_builder = None;
                    let (
                        h264_ready_prefix_decode,
                        h265_ready_prefix_decode,
                        av1_ready_prefix_decode,
                    ) = thread::scope(|scope| -> Result<_, String> {
                        let present_worker = if let Some(mut worker_sequence_builder) =
                            sequence_builder.take()
                        {
                            let worker_handoff = present_handoff.clone();
                            let resource_image_ref = resource_image
                                .as_ref()
                                .expect("Vulkanalia resource image is live");
                            let sampler =
                                decoded_image_present_sampler.as_mut().ok_or_else(|| {
                                    "Vulkanalia decoded image present sampler is unavailable"
                                        .to_owned()
                                })?;
                            let pipeline =
                                decoded_image_present_pipeline.as_ref().ok_or_else(|| {
                                    "Vulkanalia decoded image present pipeline is unavailable"
                                        .to_owned()
                                })?;
                            let frame_resources = decoded_image_present_frame_resources
                                .as_ref()
                                .ok_or_else(|| {
                                    "decoded image present sequence has no reusable frame resources"
                                        .to_owned()
                                })?;
                            let mut scene_video_overlay = scene_video_overlay.as_mut();
                            let device = &context.device;
                            let present_queue = context.present_queue;
                            Some(
                                thread::Builder::new()
                                    .name("gilder-ffmpeg-video-present-worker".to_owned())
                                    .stack_size(256 * 1024)
                                    .spawn_scoped(scope, move || {
                                let worker_result = (|| -> Result<_, String> {
                                    let mut present_frame_index = 0u32;
                                    let mut present_frame_timer =
                                        NativeVulkanVulkanaliaPresentFrameTimer::new(
                                            target_max_fps,
                                            audio_master_clock,
                                        );
                                    let mut pending_present_frame_slots = VecDeque::<u32>::new();
                                    let mut first_frame_preroll_pending = true;
                                    loop {
                                        let frame = if first_frame_preroll_pending {
                                            first_frame_preroll_pending = false;
                                            worker_handoff.recv_after_preroll(
                                                DECODED_IMAGE_PRESENT_STARTUP_PREROLL_FRAMES,
                                            )?
                                        } else {
                                            match worker_handoff.try_recv()? {
                                                Some(frame) => Some(frame),
                                                None => {
                                                    if pending_present_frame_slots.is_empty() {
                                                        worker_handoff.recv()?
                                                    } else {
                                                        match worker_handoff
                                                            .recv_or_release_waiter()?
                                                        {
                                                            NativeVulkanVulkanaliaDecodedPresentHandoffRecv::Frame(frame) => {
                                                                Some(frame)
                                                            }
                                                            NativeVulkanVulkanaliaDecodedPresentHandoffRecv::ReleaseWaiter => {
                                                                let Some(present_frame_slot) =
                                                                    pending_present_frame_slots.pop_front()
                                                                else {
                                                                    continue;
                                                                };
                                                                native_vulkan_vulkanalia_wait_decoded_image_present_frame_slot(
                                                                    device,
                                                                    frame_resources,
                                                                    present_frame_slot,
                                                                )?;
                                                                worker_handoff.complete_present_frame_slot_releases(
                                                                    present_frame_slot,
                                                                )?;
                                                                continue;
                                                            }
                                                            NativeVulkanVulkanaliaDecodedPresentHandoffRecv::Closed => None,
                                                        }
                                                    }
                                                }
                                            }
                                        };
                                        let Some(frame) = frame else {
                                            break;
                                        };
                                        if present_frame_index
                                            >= requested_present_frame_count_for_sequence
                                        {
                                            worker_handoff
                                                .mark_frame_released(frame.sampled_array_layer)?;
                                            continue;
                                        }
                                        if present_frame_index == 0 {
                                            let worker_sequence_started_at = Instant::now();
                                            worker_sequence_builder.started_at =
                                                worker_sequence_started_at;
                                            present_frame_timer.reset(worker_sequence_started_at);
                                        }
                                        let present_frame_slot_count =
                                            native_vulkan_vulkanalia_decoded_image_present_frame_slot_count(
                                                frame_resources,
                                            )
                                            .max(1);
                                        let present_frame_slot =
                                            present_frame_index as usize % present_frame_slot_count;
                                        native_vulkan_vulkanalia_prepare_decoded_image_present_frame_slot(
                                            device,
                                            frame_resources,
                                            present_frame_slot as u32,
                                        )?;
                                        worker_handoff.complete_present_frame_slot_releases(
                                            present_frame_slot as u32,
                                        )?;
                                        if let Some(position) = pending_present_frame_slots
                                            .iter()
                                            .position(|slot| *slot == present_frame_slot as u32)
                                        {
                                            pending_present_frame_slots.remove(position);
                                        }
                                        if frame.sampled_array_layer
                                            >= resource_image_ref.snapshot.array_layers
                                        {
                                            return Err(format!(
                                                "decoded image present sampled layer {} exceeds {} image layers",
                                                frame.sampled_array_layer,
                                                resource_image_ref.snapshot.array_layers
                                            ));
                                        }
                                        let (pacing_sleep_micros, pacing_clock_model) =
                                            present_frame_timer.pace_frame(
                                                present_frame_index,
                                                frame.source_frame_pts_ns,
                                                frame.source_frame_duration_ns,
                                                frame.source_frame_pts_ms,
                                                frame.source_frame_duration_ms,
                                            );
                                        let mut record_layer_present_release =
                                            |present_frame_slot: u32| {
                                                worker_handoff.record_layer_present_release(
                                                    frame.sampled_array_layer,
                                                    present_frame_slot,
                                                )
                                            };
                                        let overlay_elapsed_ms = worker_sequence_builder
                                            .started_at
                                            .elapsed()
                                            .as_millis()
                                            .min(u128::from(u64::MAX))
                                            as u64;
                                        let scene_overlay_draw =
                                            if let Some(scene_video_overlay) =
                                                scene_video_overlay.as_deref_mut()
                                            {
                                                scene_video_overlay.frame_draw(
                                                    device,
                                                    present_frame_slot as u32,
                                                    overlay_elapsed_ms,
                                                    swapchain_extent,
                                                )?
                                            } else {
                                                None
                                            };
                                        let draw =
                                            native_vulkan_vulkanalia_present_decoded_image_frame(
                                                device,
                                                present_queue,
                                                swapchain_handle,
                                                swapchain_images,
                                                swapchain_format,
                                                swapchain_extent,
                                                resource_image_ref,
                                                sampler,
                                                pipeline,
                                                frame_resources,
                                                frame.sampled_array_layer,
                                                present_frame_index,
                                                true,
                                                frame.source_frame_pts_ns,
                                                frame.source_frame_duration_ns,
                                                frame.source_frame_pts_ms,
                                                frame.source_frame_duration_ms,
                                                frame.display_order_key,
                                                frame.display_order_key_source,
                                                pacing_sleep_micros,
                                                pacing_clock_model,
                                                decoded_image_present_timing,
                                                decode_complete_semaphore,
                                                frame.decode_complete_value,
                                                queue_host_access_lock,
                                                Some(&mut record_layer_present_release),
                                                clear_color,
                                                scene_overlay_draw,
                                            )?;
                                        pending_present_frame_slots.push_back(draw.present_frame_slot);
                                        // FFmpeg FrameQueue releases the displayed AVFrame as
                                        // soon as display handoff has advanced; for Vulkan, keep
                                        // the decoded layer only until the render fence signals,
                                        // not until the same WSI frame slot is reused
                                        // (references/ffmpeg/fftools/ffplay.c:788-800,
                                        // references/ffmpeg/fftools/ffplay_renderer.c:780-786).
                                        let mut pending_slot_index = 0usize;
                                        while pending_slot_index < pending_present_frame_slots.len()
                                        {
                                            let present_frame_slot =
                                                pending_present_frame_slots[pending_slot_index];
                                            if native_vulkan_vulkanalia_try_complete_decoded_image_present_frame_slot(
                                                device,
                                                frame_resources,
                                                present_frame_slot,
                                            )? {
                                                pending_present_frame_slots
                                                    .remove(pending_slot_index);
                                                worker_handoff
                                                    .complete_present_frame_slot_releases(
                                                        present_frame_slot,
                                                    )?;
                                            } else {
                                                pending_slot_index += 1;
                                            }
                                        }
                                        worker_sequence_builder.push(draw);
                                        present_frame_index =
                                            present_frame_index.saturating_add(1);
                                    }
                                    while let Some(present_frame_slot) =
                                        pending_present_frame_slots.pop_front()
                                    {
                                        native_vulkan_vulkanalia_wait_decoded_image_present_frame_slot(
                                            device,
                                            frame_resources,
                                            present_frame_slot,
                                        )?;
                                        worker_handoff.complete_present_frame_slot_releases(
                                            present_frame_slot,
                                        )?;
                                    }
                                    let present_frame_slot_count =
                                        native_vulkan_vulkanalia_decoded_image_present_frame_slot_count(
                                            frame_resources,
                                        );
                                    for present_frame_slot in 0..present_frame_slot_count {
                                        native_vulkan_vulkanalia_wait_decoded_image_present_frame_slot(
                                            device,
                                            frame_resources,
                                            present_frame_slot as u32,
                                        )?;
                                        worker_handoff.complete_present_frame_slot_releases(
                                            present_frame_slot as u32,
                                        )?;
                                    }
                                    Ok(worker_sequence_builder)
                                })();
                                if let Err(err) = &worker_result {
                                    worker_handoff.fail(err.clone());
                                }
                                worker_result
                                    })
                                    .map_err(|err| {
                                        format!(
                                            "spawn FFmpeg-style video present worker: {err}"
                                        )
                                    })?,
                            )
                        } else {
                            None
                        };

                        #[cfg(feature = "native-vulkan-video")]
                        let decode_handoff = present_handoff.clone();
                        #[cfg(feature = "native-vulkan-video")]
                        let decoded_image_present_sequence_failed =
                            decoded_image_present_sequence_error.is_some();
                        #[cfg(feature = "native-vulkan-video")]
                        let decode_device = &context.device;
                        #[cfg(feature = "native-vulkan-video")]
                        let decode_video_queue = context.video_queue;
                        #[cfg(feature = "native-vulkan-video")]
                        let decode_video_queue_family_index = selection.video_queue_family_index;
                        #[cfg(feature = "native-vulkan-video")]
                        let decode_capabilities = queried.capabilities;
                        #[cfg(feature = "native-vulkan-video")]
                        let decode_memory_properties = &memory_properties;
                        #[cfg(feature = "native-vulkan-video")]
                        let decode_video_session = session
                            .as_ref()
                            .copied()
                            .expect("Vulkanalia video session is live");
                        #[cfg(feature = "native-vulkan-video")]
                        let decode_resource_image = resource_image
                            .as_ref()
                            .expect("Vulkanalia resource image is live");
                        #[cfg(feature = "native-vulkan-video")]
                        let decode_non_coherent_atom_size =
                            selection.properties.limits.non_coherent_atom_size;
                        #[cfg(feature = "native-vulkan-video")]
                        let decode_codec = codec;
                        let decode_worker = thread::Builder::new()
                            .name("gilder-ffmpeg-video-decode-worker".to_owned())
                            .stack_size(256 * 1024)
                            .spawn_scoped(scope, move || {
                            #[cfg(feature = "native-vulkan-video")]
                            let mut wait_for_output_slot_present_release =
                                |sampled_array_layer: u32| -> Result<(), String> {
                                    decode_handoff
                                        .wait_layer_present_release_completed(sampled_array_layer)
                                };
                            #[cfg(feature = "native-vulkan-video")]
                            let mut enqueue_decoded_frame =
                                |decode_frame_index: u32,
                                 sampled_array_layer: u32,
                                 source_frame_pts_ns: Option<u64>,
                                 source_frame_duration_ns: Option<u64>,
                                 source_frame_pts_ms: Option<u64>,
                                 source_frame_duration_ms: Option<u64>,
                                 display_order_key: i64,
                                 display_order_key_source: &'static str,
                                 decode_complete_value: u64|
                                 -> Result<(), String> {
                                    if decoded_image_present_sequence_failed {
                                        return Ok(());
                                    }
                                    if decode_frame_index
                                        >= requested_present_frame_count_for_sequence
                                    {
                                        return Ok(());
                                    }
                                    decode_handoff.enqueue(
                                        NativeVulkanVulkanaliaDecodedPresentHandoffFrame {
                                            decode_frame_index,
                                            sampled_array_layer,
                                            source_frame_pts_ns,
                                            source_frame_duration_ns,
                                            source_frame_pts_ms,
                                            source_frame_duration_ms,
                                            display_order_key,
                                            display_order_key_source,
                                            decode_complete_value,
                                        },
                                    )
                                };
                            (|| -> Result<_, String> {
                            #[cfg(feature = "native-vulkan-video")]
                            let h264_ready_prefix_decode = if let Some(prepared) =
                                prepared_streaming_decode.h264.take()
                            {
                                let NativeVulkanVulkanaliaPreparedH264StreamingDecode {
                                    request,
                                    mut queue,
                                    parameter_sets,
                                    bootstrap,
                                } = prepared;
                                let mut planner = NativeVulkanH264DecodeReferencePlanner::new(
                                    resource_image_array_layers,
                                    bootstrap.stream_max_active_reference_pictures,
                                    bootstrap.max_frame_num,
                                    parameter_sets.sps.gaps_in_frame_num_value_allowed_flag,
                                );
                                let mut pts_state =
                                    NativeVulkanVulkanaliaStreamingPtsState::new(queue.loop_count);
                                let mut next_frame = || {
                                    native_vulkan_vulkanalia_next_h264_streaming_frame(
                                        &mut queue,
                                        &mut planner,
                                        &mut pts_state,
                                    )
                                };
                                Some(
                                            native_vulkan_vulkanalia_record_h264_streaming_decode_into_image(
                                                decode_device,
                                                decode_video_queue,
                                                queue_host_access_lock,
                                                decode_memory_properties,
                                                decode_video_queue_family_index,
                                                profile_info,
                                                requested_extent,
                                                decode_capabilities,
                                                decode_video_session,
                                                decode_codec,
                                                resource_image_array_layers,
                                                ffmpeg_decode_async_exec_depth,
                                                decode_non_coherent_atom_size,
                                                NativeVulkanVulkanaliaH264StreamingDecodeInput {
                                                    parameter_sets,
                                                    requested_frame_count: request.playback_frame_count,
                                                    next_frame: &mut next_frame,
                                                },
                                                decode_resource_image,
                                                Some(&mut wait_for_output_slot_present_release),
                                                Some(&mut enqueue_decoded_frame),
                                                decode_complete_semaphore,
                                                &decode_complete_value,
                                            )?,
                                        )
                            } else {
                                None
                            };
                            #[cfg(not(feature = "native-vulkan-video"))]
                            let h264_ready_prefix_decode = None;
                            #[cfg(feature = "native-vulkan-video")]
                            let h265_ready_prefix_decode = if let Some(prepared) =
                                prepared_streaming_decode.h265.take()
                            {
                                let NativeVulkanVulkanaliaPreparedH265StreamingDecode {
                                    request,
                                    mut queue,
                                    parameter_sets,
                                    bootstrap,
                                } = prepared;
                                let mut planner = NativeVulkanH265DecodeReferencePlanner::new(
                                    resource_image_array_layers,
                                    bootstrap.stream_max_pic_order_cnt_lsb,
                                );
                                let mut pts_state =
                                    NativeVulkanVulkanaliaStreamingPtsState::new(queue.loop_count);
                                let mut next_frame = || {
                                    native_vulkan_vulkanalia_next_h265_streaming_frame(
                                        &mut queue,
                                        &mut planner,
                                        &mut pts_state,
                                    )
                                };
                                Some(
                                            native_vulkan_vulkanalia_record_h265_streaming_decode_into_image(
                                                decode_device,
                                                decode_video_queue,
                                                queue_host_access_lock,
                                                decode_memory_properties,
                                                decode_video_queue_family_index,
                                                profile_info,
                                                requested_extent,
                                                decode_capabilities,
                                                decode_video_session,
                                                decode_codec,
                                                resource_image_array_layers,
                                                ffmpeg_decode_async_exec_depth,
                                                decode_non_coherent_atom_size,
                                                NativeVulkanVulkanaliaH265StreamingDecodeInput {
                                                    parameter_sets,
                                                    requested_frame_count: request.playback_frame_count,
                                                    next_frame: &mut next_frame,
                                                },
                                                decode_resource_image,
                                                Some(&mut wait_for_output_slot_present_release),
                                                Some(&mut enqueue_decoded_frame),
                                                decode_complete_semaphore,
                                                &decode_complete_value,
                                            )?,
                                        )
                            } else {
                                None
                            };
                            #[cfg(not(feature = "native-vulkan-video"))]
                            let h265_ready_prefix_decode = None;
                            #[cfg(feature = "native-vulkan-video")]
                            let av1_ready_prefix_decode = if let Some(prepared) =
                                prepared_streaming_decode.av1.take()
                            {
                                let NativeVulkanVulkanaliaPreparedAv1StreamingDecode {
                                    request,
                                    mut queue,
                                    sequence_header,
                                    bootstrap: _,
                                } = prepared;
                                let av1_planner_dpb_slots = resource_image_array_layers.max(1);
                                let mut planner = NativeVulkanAv1DecodeReferencePlanner::new(
                                    av1_planner_dpb_slots,
                                );
                                let mut active_dpb_refs =
                                    vec![
                                        None::<NativeVulkanAv1ActiveDpbReference>;
                                        av1_planner_dpb_slots as usize
                                    ];
                                let mut pts_state =
                                    NativeVulkanVulkanaliaStreamingPtsState::new(queue.loop_count);
                                let mut next_frame = || {
                                    native_vulkan_vulkanalia_next_av1_streaming_frame(
                                        &mut queue,
                                        &mut planner,
                                        &mut active_dpb_refs,
                                        &sequence_header,
                                        &mut pts_state,
                                    )
                                };
                                Some(
                                        native_vulkan_vulkanalia_record_av1_streaming_decode_into_image(
                                            decode_device,
                                            decode_video_queue,
                                            queue_host_access_lock,
                                            decode_memory_properties,
                                            decode_video_queue_family_index,
                                            profile_info,
                                            requested_extent,
                                            decode_capabilities,
                                            decode_video_session,
                                            decode_codec,
                                            resource_image_array_layers,
                                            ffmpeg_decode_async_exec_depth,
                                            decode_non_coherent_atom_size,
                                            NativeVulkanVulkanaliaAv1StreamingDecodeInput {
                                                sequence_header: sequence_header.clone(),
                                                requested_frame_count: request.playback_frame_count,
                                                next_frame: &mut next_frame,
                                            },
                                            decode_resource_image,
                                            Some(&mut wait_for_output_slot_present_release),
                                            Some(&mut enqueue_decoded_frame),
                                            decode_complete_semaphore,
                                            &decode_complete_value,
                                        )?,
                                    )
                            } else {
                                None
                            };
                            #[cfg(not(feature = "native-vulkan-video"))]
                            let av1_ready_prefix_decode = None;
                            Ok((
                                h264_ready_prefix_decode,
                                h265_ready_prefix_decode,
                                av1_ready_prefix_decode,
                            ))
                            })()
                            })
                            .map_err(|err| {
                                format!("spawn FFmpeg-style video decode worker: {err}")
                            })?;
                        let decode_result = match decode_worker.join() {
                            Ok(result) => result,
                            Err(_) => Err("video decode worker panicked".to_owned()),
                        };
                        let close_result = present_handoff.close();
                        let present_result = if let Some(present_worker) = present_worker {
                            match present_worker.join() {
                                Ok(result) => result.map(Some),
                                Err(_) => Err("decoded image present worker panicked".to_owned()),
                            }
                        } else {
                            Ok(None)
                        };
                        close_result?;
                        let decode_result = decode_result?;
                        if let Some(builder) = present_result? {
                            completed_sequence_builder = Some(builder);
                        }
                        Ok(decode_result)
                    })?;
                    sequence_builder = completed_sequence_builder;
                    if let Some(sequence_builder) = sequence_builder.take() {
                        let handoff_snapshot = present_handoff.snapshot(
                            "decoded-image-present-worker-layer-ring",
                            "FFmpeg FrameQueue-style decoded-frame handoff: decode enqueues FIFO metadata into a fixed 3-frame ring and present starts as soon as the first display frame is available",
                            "no frame drop in ready-prefix evidence; decoded layer reuse waits on render-fence/final-drain completion instead of retaining stale copied frames",
                            "present worker drains FIFO metadata carrying FFmpeg-style PTS/POC/order-hint keys without a startup preroll gate; decoded layer release is fence driven",
                            "frame pixels are sampled from the Vulkan decode image through VK_EXT_descriptor_heap, then the swapchain image owns the displayed result",
                            FFMPEG_FFPLAY_FRAME_QUEUE_REFERENCE,
                        )?;
                        decoded_image_present_sequence =
                            sequence_builder.finish(handoff_snapshot, sequence_execution_evidence);
                    }
                    (
                        h264_ready_prefix_decode,
                        h265_ready_prefix_decode,
                        av1_ready_prefix_decode,
                    )
                };
                Ok(NativeVulkanVulkanaliaVideoPresentSessionPieces {
                    session: session.take().expect("Vulkanalia video session is live"),
                    memory_resources: memory_resources
                        .take()
                        .expect("Vulkanalia session memory resources are live"),
                    resource_image: resource_image
                        .take()
                        .expect("Vulkanalia resource image is live"),
                    decoded_image_present_pipeline: decoded_image_present_pipeline.take(),
                    decoded_image_present_sampler: decoded_image_present_sampler.take(),
                    scene_video_overlay: scene_video_overlay.take(),
                    decoded_image_present_sequence,
                    decoded_image_present_sequence_error,
                    h264_ready_prefix_decode,
                    snapshot: NativeVulkanVulkanaliaVideoPresentSessionProbeSnapshot {
                        binding: "vulkanalia",
                        route: VIDEO_PRESENT_SESSION_RETAINED_RESOURCE_ROUTE,
                        codec,
                        requested_extent: (requested_extent.width, requested_extent.height),
                        surface_host: None,
                        device: device_snapshot_from_selection(
                            vulkan, selection, context, codec, swapchain,
                        ),
                        video_session_created: true,
                        video_session_create_inline_session_parameters: context
                            .video_feature_selection
                            .inline_session_parameters_enabled,
                        video_session_create_flags_bits: video_session_create_flags.bits(),
                        memory_binding,
                        resource_image: resource_image_snapshot,
                        picture_format: format!("{picture_format:?}"),
                        decode_capability_flags: video_decode_capability_flag_labels(
                            queried.decode_capability_flags,
                        ),
                        session_max_dpb_slots,
                        session_max_active_reference_pictures,
                        resource_queue_family_indices,
                        resource_queue_sharing_model: decoded_image_resource_sharing_model(
                            same_queue_family,
                        ),
                        decoded_image_zero_copy_presentable_candidate: true,
                        decoded_image_present_sampler: decoded_image_present_sampler_snapshot,
                        decoded_image_present_sampler_error,
                        decoded_image_present_pipeline: decoded_image_present_pipeline_snapshot,
                        decoded_image_present_pipeline_error,
                        decoded_image_present_boundary: "retained Vulkanalia runtime owns video session memory, coincident sampled DPB/output image, descriptor-heap Y/UV plane sampler resources, and Wayland swapchain until the caller drops the runtime; next step records the dynamic-rendering fullscreen draw into the graphics present pass",
                        ffmpeg_reference: FFMPEG_VULKAN_DECODE_REFERENCE,
                    },
                    h265_ready_prefix_decode,
                    av1_ready_prefix_decode,
                })
            })();

            if let Some(scene_video_overlay) = scene_video_overlay.take() {
                native_vulkan_vulkanalia_destroy_scene_video_overlay_resources(
                    &context.device,
                    scene_video_overlay,
                );
            }
            if let Some(frame_resources) = decoded_image_present_frame_resources.take() {
                native_vulkan_vulkanalia_destroy_decoded_image_present_frame_resources(
                    &context.device,
                    frame_resources,
                );
            }
            if let Some(pipeline) = decoded_image_present_pipeline.take() {
                native_vulkan_vulkanalia_destroy_decoded_image_present_pipeline_resources(
                    &context.device,
                    pipeline,
                );
            }
            if let Some(sampler) = decoded_image_present_sampler.take() {
                native_vulkan_vulkanalia_destroy_decoded_image_present_sampler_resources(
                    &context.device,
                    sampler,
                );
            }
            if let Some(image) = resource_image.take() {
                native_vulkan_vulkanalia_destroy_video_session_resource_image(
                    &context.device,
                    image,
                );
            }
            if let Some(resources) = memory_resources.take() {
                native_vulkan_vulkanalia_destroy_video_session_memory_binding_resources(
                    &context.device,
                    resources,
                );
            }
            if let Some(session) = session.take() {
                native_vulkan_vulkanalia_destroy_video_session(&context.device, session);
            }

            result
        },
    )
}

include!("session_pieces_and_pacing/sequence_evidence.rs");
