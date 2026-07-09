
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

struct NativeVulkanVulkanaliaDecodedImagePresentSequenceBuilder {
    requested_present_frame_count: u32,
    started_at: Instant,
    first_presented_at: Option<Instant>,
    last_presented_at: Option<Instant>,
    present_delta_min_micros: Option<u64>,
    present_delta_max_micros: Option<u64>,
    present_delta_over_6250us_count: u32,
    present_delta_over_8334us_count: u32,
    slow_frames: Vec<NativeVulkanVulkanaliaDecodedImagePresentSlowFrameSnapshot>,
    submitted_present_frame_count: u32,
    presented_frame_count: u32,
    frame_sleep_count: u32,
    missed_frame_pacing_count: u32,
    total_pacing_sleep_micros: u64,
    total_present_call_micros: u64,
    max_present_call_micros: u64,
    total_present_wait_frame_slot_micros: u64,
    max_present_wait_frame_slot_micros: u64,
    total_present_acquire_next_image_micros: u64,
    max_present_acquire_next_image_micros: u64,
    total_present_record_command_buffer_micros: u64,
    max_present_record_command_buffer_micros: u64,
    total_present_submit_command_buffer_micros: u64,
    max_present_submit_command_buffer_micros: u64,
    total_present_queue_present_micros: u64,
    max_present_queue_present_micros: u64,
    total_present_wait_after_queue_present_micros: u64,
    max_present_wait_after_queue_present_micros: u64,
    pts_monotonic: bool,
    last_pts_ns: Option<u64>,
    source_frame_pts_delta_min_ns: Option<u64>,
    source_frame_pts_delta_max_ns: Option<u64>,
    last_pts_ms: Option<u64>,
    source_frame_pts_delta_min_ms: Option<u64>,
    source_frame_pts_delta_max_ms: Option<u64>,
    display_order_monotonic: bool,
    last_display_order_key: Option<i64>,
    uses_present_id2: bool,
    present_wait2_available: bool,
    present_wait_after_present: bool,
    all_zero_copy_presented: bool,
    sampled_array_layer_mask: u128,
    latest_draw: Option<NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot>,
    draws_head: Vec<NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot>,
    draws_tail: Vec<NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot>,
}

#[derive(Debug, Clone, Copy)]
struct NativeVulkanVulkanaliaDecodedImagePresentExecutionEvidence {
    ffmpeg_read_thread_active: bool,
    video_decode_worker_active: bool,
    present_worker_active: bool,
    source_count: u32,
    decode_thread_count: u32,
    decode_async_exec_depth: u32,
    ffmpeg_retained_avframe_count: u32,
    ffmpeg_retained_avframe_peak_count: u32,
    descriptor_sampler_cache_entry_count: u32,
    descriptor_sampler_cache_peak_entry_count: u32,
    descriptor_sampler_cache_rewrite_count: u32,
    descriptor_sampler_cache_recreate_count: u32,
    descriptor_sampler_cache_resource_heap_bytes: u64,
    descriptor_sampler_cache_sampler_heap_bytes: u64,
}

impl NativeVulkanVulkanaliaDecodedImagePresentExecutionEvidence {
    fn execution_model(self) -> &'static str {
        if self.decode_async_exec_depth == 0 && self.decode_thread_count == 1 {
            return "FFmpeg avcodec Vulkan hwdecode send/receive -> AVVkFrame descriptor-source handoff -> dynamic-rendering present worker";
        }
        if self.source_count > 1 || self.decode_thread_count > 1 {
            "FFmpeg-style N-source read threads -> per-source bounded packet queues -> per-source Vulkan Video decode workers -> per-source decoded-frame handoffs -> one dynamic-rendering present worker"
        } else {
            "FFmpeg-style read thread -> bounded packet queue -> single Vulkan Video decode worker -> bounded decoded-frame handoff -> present worker"
        }
    }

    fn ffmpeg_thread_model(self) -> &'static str {
        if self.decode_async_exec_depth == 0 && self.decode_thread_count == 1 {
            return "one FFmpeg avcodec Vulkan hwdecode owner on the Vulkanalia-provided device; Gilder presents retained AVVkFrame refs after waiting their timeline semaphores";
        }
        if self.source_count > 1 || self.decode_thread_count > 1 {
            "one FFmpeg packet read thread and one native Vulkan Video decode worker per streaming source; one shared native present worker composites decoded GPU images without CPU pixel copies"
        } else {
            "one FFmpeg packet read thread, one native Vulkan Video decode worker, one native present worker; Vulkan async-depth follows FFmpeg Vulkan decode formula"
        }
    }
}

impl NativeVulkanVulkanaliaDecodedImagePresentSequenceBuilder {
    fn new(requested_present_frame_count: u32, started_at: Instant) -> Self {
        Self {
            requested_present_frame_count,
            started_at,
            first_presented_at: None,
            last_presented_at: None,
            present_delta_min_micros: None,
            present_delta_max_micros: None,
            present_delta_over_6250us_count: 0,
            present_delta_over_8334us_count: 0,
            slow_frames: Vec::new(),
            submitted_present_frame_count: 0,
            presented_frame_count: 0,
            frame_sleep_count: 0,
            missed_frame_pacing_count: 0,
            total_pacing_sleep_micros: 0,
            total_present_call_micros: 0,
            max_present_call_micros: 0,
            total_present_wait_frame_slot_micros: 0,
            max_present_wait_frame_slot_micros: 0,
            total_present_acquire_next_image_micros: 0,
            max_present_acquire_next_image_micros: 0,
            total_present_record_command_buffer_micros: 0,
            max_present_record_command_buffer_micros: 0,
            total_present_submit_command_buffer_micros: 0,
            max_present_submit_command_buffer_micros: 0,
            total_present_queue_present_micros: 0,
            max_present_queue_present_micros: 0,
            total_present_wait_after_queue_present_micros: 0,
            max_present_wait_after_queue_present_micros: 0,
            pts_monotonic: true,
            last_pts_ns: None,
            source_frame_pts_delta_min_ns: None,
            source_frame_pts_delta_max_ns: None,
            last_pts_ms: None,
            source_frame_pts_delta_min_ms: None,
            source_frame_pts_delta_max_ms: None,
            display_order_monotonic: true,
            last_display_order_key: None,
            uses_present_id2: false,
            present_wait2_available: false,
            present_wait_after_present: false,
            all_zero_copy_presented: true,
            sampled_array_layer_mask: 0,
            latest_draw: None,
            draws_head: Vec::with_capacity(DECODED_IMAGE_PRESENT_TELEMETRY_RETAINED_FRAMES),
            draws_tail: Vec::with_capacity(DECODED_IMAGE_PRESENT_TELEMETRY_RETAINED_FRAMES),
        }
    }

    fn push(&mut self, draw: NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot) {
        if draw.submitted {
            self.submitted_present_frame_count =
                self.submitted_present_frame_count.saturating_add(1);
        }
        if draw.presented {
            let presented_at = Instant::now();
            self.first_presented_at.get_or_insert(presented_at);
            if let Some(last_presented_at) = self.last_presented_at {
                let delta_micros =
                    duration_micros_u64(presented_at.saturating_duration_since(last_presented_at));
                self.present_delta_min_micros = Some(
                    self.present_delta_min_micros
                        .map(|current| current.min(delta_micros))
                        .unwrap_or(delta_micros),
                );
                self.present_delta_max_micros = Some(
                    self.present_delta_max_micros
                        .map(|current| current.max(delta_micros))
                        .unwrap_or(delta_micros),
                );
                if delta_micros > 6_250 {
                    self.present_delta_over_6250us_count =
                        self.present_delta_over_6250us_count.saturating_add(1);
                }
                if delta_micros > 8_334 {
                    self.present_delta_over_8334us_count =
                        self.present_delta_over_8334us_count.saturating_add(1);
                }
                if delta_micros > DECODED_IMAGE_PRESENT_SLOW_FRAME_THRESHOLD_MICROS
                    && self.slow_frames.len() < DECODED_IMAGE_PRESENT_SLOW_FRAME_TELEMETRY_LIMIT
                {
                    self.slow_frames.push(
                        NativeVulkanVulkanaliaDecodedImagePresentSlowFrameSnapshot {
                            present_frame_index: draw.present_frame_index,
                            present_frame_slot: draw.present_frame_slot,
                            sampled_array_layer: draw.sampled_array_layer,
                            delta_micros,
                            present_call_total_micros: draw.present_call_total_micros,
                            present_record_command_buffer_micros: draw
                                .present_record_command_buffer_micros,
                            present_submit_command_buffer_micros: draw
                                .present_submit_command_buffer_micros,
                            present_queue_present_micros: draw.present_queue_present_micros,
                            present_wait_frame_slot_micros: draw.present_wait_frame_slot_micros,
                            source_frame_pts_ns: draw.source_frame_pts_ns,
                            display_order_key: draw.display_order_key,
                        },
                    );
                }
            }
            self.last_presented_at = Some(presented_at);
            self.presented_frame_count = self.presented_frame_count.saturating_add(1);
        }
        self.total_pacing_sleep_micros = self
            .total_pacing_sleep_micros
            .saturating_add(draw.pacing_sleep_micros);
        if draw.pacing_sleep_micros > 0 {
            self.frame_sleep_count = self.frame_sleep_count.saturating_add(1);
        }
        if draw.pacing_clock_model == "audio-clock-master-video-late-no-sleep" {
            self.missed_frame_pacing_count = self.missed_frame_pacing_count.saturating_add(1);
        }
        self.total_present_call_micros = self
            .total_present_call_micros
            .saturating_add(draw.present_call_total_micros);
        self.max_present_call_micros = self
            .max_present_call_micros
            .max(draw.present_call_total_micros);
        self.total_present_wait_frame_slot_micros = self
            .total_present_wait_frame_slot_micros
            .saturating_add(draw.present_wait_frame_slot_micros);
        self.max_present_wait_frame_slot_micros = self
            .max_present_wait_frame_slot_micros
            .max(draw.present_wait_frame_slot_micros);
        self.total_present_acquire_next_image_micros = self
            .total_present_acquire_next_image_micros
            .saturating_add(draw.present_acquire_next_image_micros);
        self.max_present_acquire_next_image_micros = self
            .max_present_acquire_next_image_micros
            .max(draw.present_acquire_next_image_micros);
        self.total_present_record_command_buffer_micros = self
            .total_present_record_command_buffer_micros
            .saturating_add(draw.present_record_command_buffer_micros);
        self.max_present_record_command_buffer_micros = self
            .max_present_record_command_buffer_micros
            .max(draw.present_record_command_buffer_micros);
        self.total_present_submit_command_buffer_micros = self
            .total_present_submit_command_buffer_micros
            .saturating_add(draw.present_submit_command_buffer_micros);
        self.max_present_submit_command_buffer_micros = self
            .max_present_submit_command_buffer_micros
            .max(draw.present_submit_command_buffer_micros);
        self.total_present_queue_present_micros = self
            .total_present_queue_present_micros
            .saturating_add(draw.present_queue_present_micros);
        self.max_present_queue_present_micros = self
            .max_present_queue_present_micros
            .max(draw.present_queue_present_micros);
        self.total_present_wait_after_queue_present_micros = self
            .total_present_wait_after_queue_present_micros
            .saturating_add(draw.present_wait_after_queue_present_micros);
        self.max_present_wait_after_queue_present_micros = self
            .max_present_wait_after_queue_present_micros
            .max(draw.present_wait_after_queue_present_micros);
        if let Some(pts_ms) = draw.source_frame_pts_ms {
            if let Some(last) = self.last_pts_ms {
                if last > pts_ms {
                    self.pts_monotonic = false;
                } else {
                    let delta = pts_ms.saturating_sub(last);
                    if delta > 0 {
                        self.source_frame_pts_delta_min_ms = Some(
                            self.source_frame_pts_delta_min_ms
                                .map(|current| current.min(delta))
                                .unwrap_or(delta),
                        );
                        self.source_frame_pts_delta_max_ms = Some(
                            self.source_frame_pts_delta_max_ms
                                .map(|current| current.max(delta))
                                .unwrap_or(delta),
                        );
                    }
                }
            }
            self.last_pts_ms = Some(pts_ms);
        }
        if self
            .last_display_order_key
            .is_some_and(|last| last > draw.display_order_key)
        {
            self.display_order_monotonic = false;
        }
        self.last_display_order_key = Some(draw.display_order_key);
        self.uses_present_id2 |= draw.uses_present_id2;
        self.present_wait2_available |= draw.present_wait2_available;
        self.present_wait_after_present |= draw.present_wait_after_present;
        self.all_zero_copy_presented &= draw.zero_copy_presented;
        if draw.sampled_array_layer < 128 {
            self.sampled_array_layer_mask |= 1u128 << draw.sampled_array_layer;
        }
        if let Some(pts_ns) = draw.source_frame_pts_ns {
            if let Some(last) = self.last_pts_ns {
                if last > pts_ns {
                    self.pts_monotonic = false;
                } else {
                    let delta = pts_ns.saturating_sub(last);
                    if delta > 0 {
                        self.source_frame_pts_delta_min_ns = Some(
                            self.source_frame_pts_delta_min_ns
                                .map(|current| current.min(delta))
                                .unwrap_or(delta),
                        );
                        self.source_frame_pts_delta_max_ns = Some(
                            self.source_frame_pts_delta_max_ns
                                .map(|current| current.max(delta))
                                .unwrap_or(delta),
                        );
                    }
                }
            }
            self.last_pts_ns = Some(pts_ns);
        }

        if DECODED_IMAGE_PRESENT_TELEMETRY_RETAINED_FRAMES == 0 {
            self.latest_draw = Some(draw);
            return;
        }
        self.latest_draw = Some(draw.clone());
        if self.draws_head.len() < DECODED_IMAGE_PRESENT_TELEMETRY_RETAINED_FRAMES {
            self.draws_head.push(draw.clone());
        }
        if self.draws_tail.len() == DECODED_IMAGE_PRESENT_TELEMETRY_RETAINED_FRAMES {
            self.draws_tail.remove(0);
        }
        self.draws_tail.push(draw);
    }

    fn finish(
        self,
        present_handoff: NativeVulkanVulkanaliaDecodedPresentHandoffSnapshot,
        execution: NativeVulkanVulkanaliaDecodedImagePresentExecutionEvidence,
    ) -> Option<NativeVulkanVulkanaliaDecodedImagePresentSequenceSnapshot> {
        let latest_draw = self.latest_draw;
        if latest_draw.is_none() {
            return None;
        }
        let teardown_inclusive_elapsed = self.started_at.elapsed();
        let average_present_teardown_inclusive_fps =
            if self.presented_frame_count == 0 || teardown_inclusive_elapsed.is_zero() {
                0.0
            } else {
                f64::from(self.presented_frame_count) / teardown_inclusive_elapsed.as_secs_f64()
            };
        let present_interval_elapsed = match (
            self.first_presented_at,
            self.last_presented_at,
            self.presented_frame_count,
        ) {
            (Some(first), Some(last), presented_frame_count) if presented_frame_count > 1 => {
                last.saturating_duration_since(first)
            }
            _ => Duration::ZERO,
        };
        let average_present_fps =
            if self.presented_frame_count > 1 && !present_interval_elapsed.is_zero() {
                f64::from(self.presented_frame_count.saturating_sub(1))
                    / present_interval_elapsed.as_secs_f64()
            } else {
                average_present_teardown_inclusive_fps
            };
        let wait_tuning = native_vulkan_vulkanalia_present_wait_tuning();
        Some(NativeVulkanVulkanaliaDecodedImagePresentSequenceSnapshot {
            binding: "vulkanalia",
            route: "decoded-image-dynamic-rendering-present-sequence",
            execution_model: execution.execution_model(),
            ffmpeg_thread_model: execution.ffmpeg_thread_model(),
            ffmpeg_read_thread_active: execution.ffmpeg_read_thread_active,
            video_decode_worker_active: execution.video_decode_worker_active,
            present_worker_active: execution.present_worker_active,
            decode_thread_count: execution.decode_thread_count,
            decode_async_exec_depth: execution.decode_async_exec_depth,
            requested_present_frame_count: self.requested_present_frame_count,
            submitted_present_frame_count: self.submitted_present_frame_count,
            presented_frame_count: self.presented_frame_count,
            average_present_fps,
            average_present_teardown_inclusive_fps,
            present_interval_elapsed_micros: duration_micros_u64(present_interval_elapsed),
            present_teardown_inclusive_elapsed_micros: duration_micros_u64(
                teardown_inclusive_elapsed,
            ),
            present_delta_min_micros: self.present_delta_min_micros,
            present_delta_max_micros: self.present_delta_max_micros,
            present_delta_over_6250us_count: self.present_delta_over_6250us_count,
            present_delta_over_8334us_count: self.present_delta_over_8334us_count,
            slow_frame_telemetry_limit: DECODED_IMAGE_PRESENT_SLOW_FRAME_TELEMETRY_LIMIT,
            slow_frames: self.slow_frames,
            retained_frame_telemetry_limit: DECODED_IMAGE_PRESENT_TELEMETRY_RETAINED_FRAMES,
            distinct_sampled_array_layer_count: self.sampled_array_layer_mask.count_ones(),
            sampled_array_layers_head: self
                .draws_head
                .iter()
                .map(|draw| draw.sampled_array_layer)
                .collect(),
            sampled_array_layers_tail: self
                .draws_tail
                .iter()
                .map(|draw| draw.sampled_array_layer)
                .collect(),
            source_frame_pts_ns_head: self
                .draws_head
                .iter()
                .map(|draw| draw.source_frame_pts_ns)
                .collect(),
            source_frame_pts_ns_tail: self
                .draws_tail
                .iter()
                .map(|draw| draw.source_frame_pts_ns)
                .collect(),
            source_frame_pts_delta_min_ns: self.source_frame_pts_delta_min_ns,
            source_frame_pts_delta_max_ns: self.source_frame_pts_delta_max_ns,
            source_frame_duration_ns_head: self
                .draws_head
                .iter()
                .map(|draw| draw.source_frame_duration_ns)
                .collect(),
            source_frame_duration_ns_tail: self
                .draws_tail
                .iter()
                .map(|draw| draw.source_frame_duration_ns)
                .collect(),
            source_frame_pts_ms_head: self
                .draws_head
                .iter()
                .map(|draw| draw.source_frame_pts_ms)
                .collect(),
            source_frame_pts_ms_tail: self
                .draws_tail
                .iter()
                .map(|draw| draw.source_frame_pts_ms)
                .collect(),
            source_frame_pts_delta_min_ms: self.source_frame_pts_delta_min_ms,
            source_frame_pts_delta_max_ms: self.source_frame_pts_delta_max_ms,
            source_frame_duration_ms_head: self
                .draws_head
                .iter()
                .map(|draw| draw.source_frame_duration_ms)
                .collect(),
            source_frame_duration_ms_tail: self
                .draws_tail
                .iter()
                .map(|draw| draw.source_frame_duration_ms)
                .collect(),
            display_order_keys_head: self
                .draws_head
                .iter()
                .map(|draw| draw.display_order_key)
                .collect(),
            display_order_keys_tail: self
                .draws_tail
                .iter()
                .map(|draw| draw.display_order_key)
                .collect(),
            display_order_key_sources_head: self
                .draws_head
                .iter()
                .map(|draw| draw.display_order_key_source)
                .collect(),
            display_order_key_sources_tail: self
                .draws_tail
                .iter()
                .map(|draw| draw.display_order_key_source)
                .collect(),
            present_ids_head: self.draws_head.iter().map(|draw| draw.present_id).collect(),
            present_ids_tail: self.draws_tail.iter().map(|draw| draw.present_id).collect(),
            frame_sleep_count: self.frame_sleep_count,
            missed_frame_pacing_count: self.missed_frame_pacing_count,
            total_pacing_sleep_micros: self.total_pacing_sleep_micros,
            present_sleep_guard_micros: duration_micros_u64(wait_tuning.sleep_guard),
            present_spin_guard_micros: duration_micros_u64(wait_tuning.spin_guard),
            total_present_call_micros: self.total_present_call_micros,
            max_present_call_micros: self.max_present_call_micros,
            total_present_wait_frame_slot_micros: self.total_present_wait_frame_slot_micros,
            max_present_wait_frame_slot_micros: self.max_present_wait_frame_slot_micros,
            total_present_acquire_next_image_micros: self.total_present_acquire_next_image_micros,
            max_present_acquire_next_image_micros: self.max_present_acquire_next_image_micros,
            total_present_record_command_buffer_micros: self
                .total_present_record_command_buffer_micros,
            max_present_record_command_buffer_micros: self.max_present_record_command_buffer_micros,
            total_present_submit_command_buffer_micros: self
                .total_present_submit_command_buffer_micros,
            max_present_submit_command_buffer_micros: self.max_present_submit_command_buffer_micros,
            total_present_queue_present_micros: self.total_present_queue_present_micros,
            max_present_queue_present_micros: self.max_present_queue_present_micros,
            total_present_wait_after_queue_present_micros: self
                .total_present_wait_after_queue_present_micros,
            max_present_wait_after_queue_present_micros: self
                .max_present_wait_after_queue_present_micros,
            pts_monotonic: self.pts_monotonic,
            display_order_monotonic: self.display_order_monotonic,
            uses_present_id2: self.uses_present_id2,
            present_wait2_available: self.present_wait2_available,
            present_wait_after_present: self.present_wait_after_present,
            present_handoff,
            latest_draw,
            draws_head: self.draws_head,
            draws_tail: self.draws_tail,
            frame_order_model: "FFmpeg-style display queue: decode submissions enqueue FIFO metadata carrying PTS/POC/order-hint keys with decode-index fallback; ready-prefix windows may be looped as metadata-only sampled-layer references before Vulkanalia dynamic rendering",
            present_resource_reuse_model: "one swapchain image-view set, one command pool, one semaphore pair, one fence set and one bounded decoded-frame handoff reused across decoded-image present frames",
            ffmpeg_retained_avframe_count: execution.ffmpeg_retained_avframe_count,
            ffmpeg_retained_avframe_peak_count: execution.ffmpeg_retained_avframe_peak_count,
            descriptor_sampler_cache_entry_count: execution.descriptor_sampler_cache_entry_count,
            descriptor_sampler_cache_peak_entry_count: execution
                .descriptor_sampler_cache_peak_entry_count,
            descriptor_sampler_cache_rewrite_count: execution
                .descriptor_sampler_cache_rewrite_count,
            descriptor_sampler_cache_recreate_count: execution
                .descriptor_sampler_cache_recreate_count,
            descriptor_sampler_cache_resource_heap_bytes: execution
                .descriptor_sampler_cache_resource_heap_bytes,
            descriptor_sampler_cache_sampler_heap_bytes: execution
                .descriptor_sampler_cache_sampler_heap_bytes,
            descriptor_sampler_cache_total_heap_bytes: execution
                .descriptor_sampler_cache_resource_heap_bytes
                .saturating_add(execution.descriptor_sampler_cache_sampler_heap_bytes),
            telemetry_retention_model: "compact head/tail/latest frame telemetry only; hot video runtime does not retain every draw snapshot",
            all_zero_copy_presented: self.all_zero_copy_presented,
            uses_dynamic_rendering: true,
            uses_synchronization2: true,
            uses_submit2: true,
            ffmpeg_reference: FFMPEG_VULKAN_DECODE_REFERENCE,
        })
    }
}

#[derive(Debug, Clone)]
struct NativeVulkanVulkanaliaPresentFrameTimer {
    frame_timer: Option<Instant>,
    target_max_fps: Option<u32>,
    audio_master_clock: NativeVulkanVulkanaliaVideoPresentAudioMasterClock,
    audio_master_started_at: Option<Instant>,
    last_pts_ns: Option<u64>,
    last_duration_ns: Option<u64>,
}

impl NativeVulkanVulkanaliaPresentFrameTimer {
    fn new(
        target_max_fps: Option<u32>,
        audio_master_clock: NativeVulkanVulkanaliaVideoPresentAudioMasterClock,
    ) -> Self {
        Self {
            frame_timer: None,
            target_max_fps: target_max_fps.filter(|fps| *fps > 0),
            audio_master_clock,
            audio_master_started_at: None,
            last_pts_ns: None,
            last_duration_ns: None,
        }
    }

    fn reset(&mut self, now: Instant) {
        self.frame_timer = Some(now);
        self.audio_master_started_at = self.audio_master_clock.enabled.then_some(now);
        self.last_pts_ns = None;
        self.last_duration_ns = None;
    }

    fn pace_frame(
        &mut self,
        present_frame_index: u32,
        source_frame_pts_ns: Option<u64>,
        source_frame_duration_ns: Option<u64>,
        source_frame_pts_ms: Option<u64>,
        source_frame_duration_ms: Option<u64>,
    ) -> (u64, &'static str) {
        let pts_ns = source_frame_pts_ns
            .or_else(|| source_frame_pts_ms.map(|pts| pts.saturating_mul(1_000_000)));
        let duration_ns = source_frame_duration_ns.or_else(|| {
            source_frame_duration_ms.map(|duration| duration.saturating_mul(1_000_000))
        });
        let now = Instant::now();
        if self.frame_timer.is_none() || present_frame_index == 0 {
            self.frame_timer = Some(now);
            self.audio_master_started_at = self.audio_master_clock.enabled.then_some(now);
            self.last_pts_ns = pts_ns;
            self.last_duration_ns = duration_ns;
            return (
                0,
                if self.audio_master_clock.enabled {
                    "audio-clock-master-first-frame"
                } else {
                    "ffmpeg-frame-timer-first-frame"
                },
            );
        }

        if let Some((delay, clock_model)) =
            self.audio_master_delay_for_frame(now, present_frame_index, pts_ns, duration_ns)
        {
            if delay.is_zero() {
                self.last_pts_ns = pts_ns;
                self.last_duration_ns = duration_ns;
                return (0, clock_model);
            }
            let deadline = now + delay;
            let slept = native_vulkan_vulkanalia_wait_until_video_present_deadline(deadline);
            self.frame_timer = Some(deadline);
            self.last_pts_ns = pts_ns;
            self.last_duration_ns = duration_ns;
            return (
                u64::try_from(slept.as_micros()).unwrap_or(u64::MAX),
                clock_model,
            );
        }

        let (delay, clock_model) = self.next_delay(pts_ns, duration_ns);
        if delay.is_zero() {
            self.last_pts_ns = pts_ns;
            self.last_duration_ns = duration_ns;
            return (0, clock_model);
        }
        let frame_timer = self.frame_timer.unwrap_or(now);
        let deadline = frame_timer + delay;
        let wait_started_at = Instant::now();
        let slept = if deadline > wait_started_at {
            native_vulkan_vulkanalia_wait_until_video_present_deadline(deadline)
        } else {
            Duration::ZERO
        };
        self.frame_timer = Some(deadline);
        let after_wait = Instant::now();
        if after_wait > deadline
            && after_wait.duration_since(deadline) > FFMPEG_AV_SYNC_THRESHOLD_MAX
        {
            // FFmpeg's video_refresh() advances frame_timer by the nominal
            // delay, then only resynchronizes on large lateness
            // (references/ffmpeg/fftools/ffplay.c:1665-1683).
            self.frame_timer = Some(after_wait);
        }
        self.last_pts_ns = pts_ns;
        self.last_duration_ns = duration_ns;
        (
            u64::try_from(slept.as_micros()).unwrap_or(u64::MAX),
            clock_model,
        )
    }

    fn next_delay(
        &self,
        pts_ns: Option<u64>,
        duration_ns: Option<u64>,
    ) -> (Duration, &'static str) {
        if let (Some(last_pts_ns), Some(pts_ns)) = (self.last_pts_ns, pts_ns) {
            if pts_ns > last_pts_ns {
                return (
                    Duration::from_nanos(pts_ns - last_pts_ns),
                    "ffmpeg-frame-timer-pts-delta-sleep",
                );
            }
        }
        if let Some(last_duration_ns) = self.last_duration_ns.filter(|duration| *duration > 0) {
            return (
                Duration::from_nanos(last_duration_ns),
                "ffmpeg-frame-timer-last-duration-sleep",
            );
        }
        if let Some(duration_ns) = duration_ns.filter(|duration| *duration > 0) {
            return (
                Duration::from_nanos(duration_ns),
                "ffmpeg-frame-timer-duration-sleep",
            );
        }
        if let Some(target_max_fps) = self.target_max_fps {
            return (
                native_vulkan_vulkanalia_frame_count_duration(1, target_max_fps),
                "ffmpeg-frame-timer-target-fps-sleep",
            );
        }
        (Duration::ZERO, "unpaced-no-video-clock")
    }

    fn audio_master_delay_for_frame(
        &self,
        now: Instant,
        present_frame_index: u32,
        pts_ns: Option<u64>,
        duration_ns: Option<u64>,
    ) -> Option<(Duration, &'static str)> {
        if !self.audio_master_clock.enabled || present_frame_index == 0 {
            return None;
        }
        let master_clock_ns = self.audio_master_clock_ns(now)?;
        let video_clock_ns =
            self.current_video_clock_ns(present_frame_index, pts_ns, duration_ns)?;
        if video_clock_ns <= master_clock_ns {
            return Some((Duration::ZERO, "audio-clock-master-video-late-no-sleep"));
        }
        let delay_ns = video_clock_ns.saturating_sub(master_clock_ns);
        Some((
            Duration::from_nanos(delay_ns),
            "audio-clock-master-pts-sync-sleep",
        ))
    }

    fn audio_master_clock_ns(&self, now: Instant) -> Option<u64> {
        let started_at = self.audio_master_started_at?;
        Some(
            self.audio_master_clock
                .start_clock_ns
                .unwrap_or(0)
                .saturating_add(
                    u64::try_from(now.duration_since(started_at).as_nanos()).unwrap_or(u64::MAX),
                ),
        )
    }

    fn current_video_clock_ns(
        &self,
        present_frame_index: u32,
        pts_ns: Option<u64>,
        duration_ns: Option<u64>,
    ) -> Option<u64> {
        if let Some(pts_ns) = pts_ns {
            return Some(pts_ns);
        }
        if let (Some(last_pts_ns), Some(duration_ns)) =
            (self.last_pts_ns, self.last_duration_ns.or(duration_ns))
        {
            if duration_ns > 0 {
                return Some(last_pts_ns.saturating_add(duration_ns));
            }
        }
        self.target_max_fps.filter(|fps| *fps > 0).map(|fps| {
            let clock_ns = (u128::from(present_frame_index) * 1_000_000_000u128) / u128::from(fps);
            u64::try_from(clock_ns).unwrap_or(u64::MAX)
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct NativeVulkanVulkanaliaPresentWaitTuning {
    sleep_guard: Duration,
    spin_guard: Duration,
}

fn native_vulkan_vulkanalia_present_wait_tuning() -> NativeVulkanVulkanaliaPresentWaitTuning {
    static TUNING: OnceLock<NativeVulkanVulkanaliaPresentWaitTuning> = OnceLock::new();
    *TUNING.get_or_init(|| {
        let sleep_guard = native_vulkan_duration_from_env_micros(
            VIDEO_PRESENT_SLEEP_GUARD_ENV,
            VIDEO_PRESENT_SLEEP_GUARD_DEFAULT_MICROS,
        );
        let spin_guard = native_vulkan_duration_from_env_micros(
            VIDEO_PRESENT_SPIN_GUARD_ENV,
            VIDEO_PRESENT_SPIN_GUARD_DEFAULT_MICROS,
        )
        .min(sleep_guard);
        NativeVulkanVulkanaliaPresentWaitTuning {
            sleep_guard,
            spin_guard,
        }
    })
}

fn native_vulkan_duration_from_env_micros(name: &str, default_micros: u64) -> Duration {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_micros)
        .unwrap_or_else(|| Duration::from_micros(default_micros))
}

fn native_vulkan_vulkanalia_wait_until_video_present_deadline(deadline: Instant) -> Duration {
    let started_at = Instant::now();
    let wait_tuning = native_vulkan_vulkanalia_present_wait_tuning();
    loop {
        let now = Instant::now();
        if now >= deadline {
            return now.saturating_duration_since(started_at);
        }
        let remaining = deadline.duration_since(now);
        if remaining > wait_tuning.sleep_guard {
            thread::sleep(remaining - wait_tuning.sleep_guard);
        } else if remaining > wait_tuning.spin_guard {
            thread::yield_now();
        } else {
            std::hint::spin_loop();
        }
    }
}

fn native_vulkan_vulkanalia_frame_count_duration(
    frame_count: u32,
    target_max_fps: u32,
) -> Duration {
    let fps = u128::from(target_max_fps.max(1));
    let nanos = u128::from(frame_count).saturating_mul(1_000_000_000u128) / fps;
    Duration::from_nanos(nanos.min(u128::from(u64::MAX)) as u64)
}

fn duration_micros_u64(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

fn native_vulkan_vulkanalia_ffmpeg_decode_async_exec_depth(video_queue_count: u32) -> u32 {
    let queue_context_count = video_queue_count.max(1);
    let thread_count = FFMPEG_SINGLE_DECODE_THREAD_COUNT.max(1);
    // Exact FFmpeg Vulkan decode async-depth formula for this runtime's single
    // decode worker thread (references/ffmpeg/libavcodec/vulkan_decode.c:1368-1378).
    queue_context_count
        .saturating_mul(2)
        .min(thread_count.saturating_mul(2))
        .max(thread_count)
        .max(1)
}
