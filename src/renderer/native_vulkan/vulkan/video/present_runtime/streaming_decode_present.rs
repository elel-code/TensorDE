
#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn run_native_vulkan_vulkanalia_h265_streaming_video_present_decode_with_scene_video_overlay(
    options: NativeVulkanVulkanaliaH265StreamingVideoPresentDecodeOptions,
    scene_video_overlay: Option<NativeVulkanVulkanaliaSceneVideoOverlayInput>,
) -> Result<NativeVulkanVulkanaliaH265RetainedVideoPresentDecodeSnapshot, String> {
    if !matches!(
        options.session.codec,
        NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::H265Main10
    ) {
        return Err(
            "Vulkanalia streaming video-present decode currently supports H.265 only".to_owned(),
        );
    }
    let playback_frame_count = options.playback_frame_count;
    let session_options = options.session.clone();
    let mut runtime =
        create_native_vulkan_vulkanalia_video_present_session_runtime_with_ready_prefix_decode(
            session_options,
            NativeVulkanVulkanaliaStreamingDecodeRequests {
                h264: None,
                h265: Some(options),
                av1: None,
            },
            playback_frame_count,
            scene_video_overlay,
        )?;
    let decode = runtime
        .resources
        .as_ref()
        .and_then(|resources| resources.h265_ready_prefix_decode.clone())
        .ok_or_else(|| {
            "Vulkanalia streaming H.265 video-present decode produced no decode snapshot".to_owned()
        })?;
    let present = runtime
        .resources
        .as_mut()
        .ok_or_else(|| "Vulkanalia retained runtime resources are unavailable".to_owned())?
        .decoded_image_present_result(decode.dst_base_array_layer);
    Ok(
        NativeVulkanVulkanaliaH265RetainedVideoPresentDecodeSnapshot {
            session: runtime.snapshot().clone(),
            decode,
            decoded_into_retained_resource_image: true,
            decoded_image_present_sequence_requested: true,
            decoded_image_present_sequence: present.sequence,
            decoded_image_present_sequence_error: present.sequence_error,
            decoded_image_present_draw_requested: true,
            decoded_image_present_draw: present.draw,
            decoded_image_present_draw_error: present.draw_error,
            decoded_image_zero_copy_presented: present.zero_copy_presented,
        },
    )
}

#[cfg(feature = "native-vulkan-video")]
pub fn run_native_vulkan_vulkanalia_av1_streaming_video_present_decode(
    options: NativeVulkanVulkanaliaAv1StreamingVideoPresentDecodeOptions,
) -> Result<NativeVulkanVulkanaliaAv1RetainedVideoPresentDecodeSnapshot, String> {
    run_native_vulkan_vulkanalia_av1_streaming_video_present_decode_with_scene_video_overlay(
        options, None,
    )
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn run_native_vulkan_vulkanalia_av1_streaming_video_present_decode_with_scene_video_overlay(
    options: NativeVulkanVulkanaliaAv1StreamingVideoPresentDecodeOptions,
    scene_video_overlay: Option<NativeVulkanVulkanaliaSceneVideoOverlayInput>,
) -> Result<NativeVulkanVulkanaliaAv1RetainedVideoPresentDecodeSnapshot, String> {
    if !matches!(
        options.session.codec,
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10
    ) {
        return Err(
            "Vulkanalia streaming video-present decode currently supports AV1 only".to_owned(),
        );
    }
    let playback_frame_count = options.playback_frame_count;
    let session_options = options.session.clone();
    let mut runtime =
        create_native_vulkan_vulkanalia_video_present_session_runtime_with_ready_prefix_decode(
            session_options,
            NativeVulkanVulkanaliaStreamingDecodeRequests {
                h264: None,
                h265: None,
                av1: Some(options),
            },
            playback_frame_count,
            scene_video_overlay,
        )?;
    let decode = runtime
        .resources
        .as_ref()
        .and_then(|resources| resources.av1_ready_prefix_decode.clone())
        .ok_or_else(|| {
            "Vulkanalia streaming AV1 video-present decode produced no decode snapshot".to_owned()
        })?;
    let present = runtime
        .resources
        .as_mut()
        .ok_or_else(|| "Vulkanalia retained runtime resources are unavailable".to_owned())?
        .decoded_image_present_result(decode.dst_base_array_layer);
    Ok(
        NativeVulkanVulkanaliaAv1RetainedVideoPresentDecodeSnapshot {
            session: runtime.snapshot().clone(),
            decode,
            decoded_into_retained_resource_image: true,
            decoded_image_present_sequence_requested: true,
            decoded_image_present_sequence: present.sequence,
            decoded_image_present_sequence_error: present.sequence_error,
            decoded_image_present_draw_requested: true,
            decoded_image_present_draw: present.draw,
            decoded_image_present_draw_error: present.draw_error,
            decoded_image_zero_copy_presented: present.zero_copy_presented,
        },
    )
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn run_native_vulkan_vulkanalia_multi_streaming_video_present_decode_with_scene_video_overlay(
    options: NativeVulkanVulkanaliaMultiStreamingVideoPresentDecodeOptions,
    scene_video_overlay: Option<NativeVulkanVulkanaliaSceneVideoOverlayInput>,
) -> Result<NativeVulkanVulkanaliaMultiStreamingVideoPresentDecodeSnapshot, String> {
    if options.sources.is_empty() {
        return Err(
            "Vulkanalia multi-source video present requires at least one source".to_owned(),
        );
    }
    let decode_plan = NativeVulkanVulkanaliaMultiVideoDecodePlan::from_sources(&options.sources)?;
    let requested_present_frame_count = decode_plan.requested_present_frame_count;

    let surface_host = NativeVulkanVideoSurfaceHost::connect_wayland(
        options.host.clone(),
        options.wait_configure_roundtrips,
    )?;
    let handles = surface_host.handles();
    let surface_host_snapshot = surface_host.snapshot().clone();

    let mut requested_instance_extensions = REQUIRED_INSTANCE_EXTENSIONS.to_vec();
    requested_instance_extensions.extend_from_slice(OPTIONAL_INSTANCE_EXTENSIONS);
    let vulkan = native_vulkan_vulkanalia_create_instance_with_required_extensions(
        &requested_instance_extensions,
    )?;
    let instance = &vulkan.instance;
    let surface = match create_vulkanalia_wayland_surface(instance, handles) {
        Ok(surface) => surface,
        Err(err) => {
            native_vulkan_vulkanalia_destroy_instance(vulkan);
            return Err(err);
        }
    };
    let result =
        (|| -> Result<NativeVulkanVulkanaliaMultiStreamingVideoPresentDecodeSnapshot, String> {
            let physical_devices = unsafe { instance.enumerate_physical_devices() }.map_err(|err| {
            format!("vkEnumeratePhysicalDevices(vulkanalia multi-source video present runtime): {err:?}")
        })?;
            let selection = select_video_present_physical_device(
                instance,
                surface,
                handles,
                &physical_devices,
                decode_plan.codecs(),
            )?;
            let context = create_video_present_device(
                instance,
                &selection,
                decode_plan.codecs(),
                vulkanalia_surface_maintenance1_enabled(&vulkan),
            )?;
            let result = (|| -> Result<NativeVulkanVulkanaliaMultiStreamingVideoPresentDecodeSnapshot, String> {
            let swapchain_plan = create_vulkanalia_swapchain_plan(
                instance,
                selection.physical_device,
                surface,
                handles.buffer_size,
                vulkanalia_surface_capabilities2_enabled(&vulkan),
                &context.present_feature_selection,
            )?;
            let swapchain = unsafe {
                context
                    .device
                    .create_swapchain_khr(&swapchain_plan.create_info, None)
            }
            .map_err(|err| {
                format!("vkCreateSwapchainKHR(vulkanalia multi-source video present): {err:?}")
            })?;
            let result = (|| -> Result<NativeVulkanVulkanaliaMultiStreamingVideoPresentDecodeSnapshot, String> {
                let swapchain_images = unsafe { context.device.get_swapchain_images_khr(swapchain) }
                    .map_err(|err| {
                        format!(
                            "vkGetSwapchainImagesKHR(vulkanalia multi-source video present): {err:?}"
                        )
                    })?;
                let mut source_slots = Vec::with_capacity(options.sources.len());
                let mut prepared_decodes = Vec::with_capacity(options.sources.len());
                let mut source_create_error = None;
                for (source_index, source) in options.sources.clone().into_iter().enumerate() {
                    match create_multi_video_decode_source_slot(
                        instance,
                        &context,
                        &selection,
                        source_index,
                        source,
                        options.host.clone(),
                        options.wait_configure_roundtrips,
                        options.target_max_fps,
                        options.audio_master_clock,
                        options.clear_color,
                    ) {
                        Ok((slot, prepared)) => {
                            source_slots.push(slot);
                            prepared_decodes.push(prepared);
                        }
                        Err(err) => {
                            source_create_error = Some(err);
                            break;
                        }
                    }
                }
                if let Some(err) = source_create_error {
                    for slot in source_slots.drain(..).rev() {
                        destroy_multi_video_decode_source_slot(&context.device, slot);
                    }
                    return Err(err);
                }
                let descriptor_heap_plan =
                    native_vulkan_vulkanalia_multi_source_descriptor_heap_plan(&source_slots)?;
                let decoded_image_present_pipeline =
                    native_vulkan_vulkanalia_create_decoded_image_present_pipeline_resources(
                        &context.device,
                        swapchain_plan.format.format,
                        swapchain_plan.extent,
                        &descriptor_heap_plan,
                    )?;
                let mut decoded_image_present_pipeline = Some(decoded_image_present_pipeline);
                let decoded_image_present_frame_resources =
                    native_vulkan_vulkanalia_create_decoded_image_present_frame_resources(
                        &context.device,
                        &swapchain_images,
                        swapchain_plan.format.format,
                        selection.present_queue_family_index,
                    )?;
                let mut decoded_image_present_frame_resources =
                    Some(decoded_image_present_frame_resources);
                let mut scene_video_overlay_resources: Option<VulkanaliaSceneVideoOverlayResources> =
                    if scene_video_overlay.is_some() {
                        return Err(native_vulkan_scene_video_overlay_removed_error());
                } else {
                    None
                };
                let decoded_image_present_timing = VulkanaliaDecodedImagePresentTimingConfig::new(
                    swapchain_plan.present_id2_enabled,
                    swapchain_plan.present_wait2_enabled,
                );
                let sequence = run_multi_video_decode_present_sequence(
                    instance,
                    &context,
                    &selection,
                    swapchain,
                    &swapchain_images,
                    swapchain_plan.format.format,
                    swapchain_plan.extent,
                    decoded_image_present_timing,
                    options.clear_color,
                    options.target_max_fps,
                    options.audio_master_clock,
                    requested_present_frame_count,
                    &mut source_slots,
                    prepared_decodes,
                    decoded_image_present_pipeline
                        .as_ref()
                        .expect("multi-source present pipeline is live"),
                    decoded_image_present_frame_resources
                        .as_ref()
                        .expect("multi-source frame resources are live"),
                    scene_video_overlay_resources.as_mut(),
                );
                let (sequence, sequence_error) = match sequence {
                    Ok(sequence) => (Some(sequence), None),
                    Err(err) => (None, Some(err)),
                };
                if let Some(scene_video_overlay_resources) = scene_video_overlay_resources.take() {
                    native_vulkan_vulkanalia_destroy_scene_video_overlay_resources(
                        &context.device,
                        scene_video_overlay_resources,
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
                let mut source_snapshots = Vec::with_capacity(source_slots.len());
                for mut slot in source_slots.drain(..) {
                    slot.snapshot.decoded_image_zero_copy_presented =
                        sequence_error.is_none() && sequence.is_some();
                    source_snapshots.push(slot.snapshot.clone());
                    destroy_multi_video_decode_source_slot(&context.device, slot);
                }
                Ok(NativeVulkanVulkanaliaMultiStreamingVideoPresentDecodeSnapshot {
                    binding: "vulkanalia",
                    route: "multi-source-streaming-video-present-decode",
                    source_count: decode_plan.source_count,
                    codec_count: decode_plan.codec_count(),
                    codecs: decode_plan.codecs.clone(),
                    surface_host: Some(surface_host_snapshot),
                    sources: source_snapshots,
                    decoded_image_present_sequence_requested: true,
                    decoded_image_present_sequence: sequence,
                    decoded_image_present_sequence_error: sequence_error.clone(),
                    decoded_image_zero_copy_presented: sequence_error.is_none(),
                    decoded_image_present_boundary: "N Vulkan Video decode source slots share one Wayland swapchain and one dynamic-rendering present pass; each scene video draw selects its decoded source by resource_index and samples that source's descriptor heap without CPU composition",
                })
            })();
            unsafe {
                context.device.destroy_swapchain_khr(swapchain, None);
            }
            result
        })();
            unsafe {
                context.device.destroy_device(None);
            }
            result
        })();
    unsafe {
        instance.destroy_surface_khr(surface, None);
    }
    native_vulkan_vulkanalia_destroy_instance(vulkan);
    result
}

#[cfg(feature = "native-vulkan-video")]
#[allow(clippy::too_many_arguments)]
fn run_multi_video_decode_present_sequence(
    instance: &Instance,
    context: &NativeVulkanVulkanaliaVideoPresentDeviceContext,
    selection: &super::video_present_device::NativeVulkanVulkanaliaVideoPresentPhysicalDeviceSelection,
    swapchain: vk::SwapchainKHR,
    swapchain_images: &[vk::Image],
    swapchain_format: vk::Format,
    swapchain_extent: vk::Extent2D,
    decoded_image_present_timing: VulkanaliaDecodedImagePresentTimingConfig,
    clear_color: NativeVulkanClearColor,
    target_max_fps: Option<u32>,
    audio_master_clock: NativeVulkanVulkanaliaVideoPresentAudioMasterClock,
    requested_present_frame_count: u32,
    source_slots: &mut [NativeVulkanVulkanaliaMultiVideoDecodeSourceSlot],
    prepared_decodes: Vec<NativeVulkanVulkanaliaPreparedStreamingDecode>,
    decoded_image_present_pipeline: &VulkanaliaDecodedImagePresentPipelineResources,
    decoded_image_present_frame_resources: &VulkanaliaDecodedImagePresentFrameResources,
    scene_video_overlay: Option<&mut VulkanaliaSceneVideoOverlayResources>,
) -> Result<NativeVulkanVulkanaliaDecodedImagePresentSequenceSnapshot, String> {
    if source_slots.is_empty() {
        return Err("multi-source decode/present requires at least one source slot".to_owned());
    }
    if prepared_decodes.len() != source_slots.len() {
        return Err(format!(
            "multi-source prepared decode count {} does not match source slot count {}",
            prepared_decodes.len(),
            source_slots.len()
        ));
    }

    let requested_present_frame_count = requested_present_frame_count.max(1);
    let handoffs = source_slots
        .iter()
        .map(|slot| {
            NativeVulkanVulkanaliaDecodedPresentHandoff::new(
                FFMPEG_VIDEO_PICTURE_QUEUE_SIZE,
                slot.resource_image_array_layers as usize,
            )
        })
        .collect::<Vec<_>>();
    let sequence_started_at = Instant::now();
    let queue_host_access_mutex = Mutex::new(());
    let same_queue_handle = selection.video_queue_family_index
        == selection.present_queue_family_index
        && context.video_queue_index == context.present_queue_index;
    let queue_host_access_lock = same_queue_handle.then_some(&queue_host_access_mutex);
    let decode_queue_host_access_lock = Some(&queue_host_access_mutex);
    let ffmpeg_decode_async_exec_depth =
        native_vulkan_vulkanalia_ffmpeg_decode_async_exec_depth(selection.video_queue_count);
    let source_slot_count = source_slots.len();
    let source_slots_ref: &[NativeVulkanVulkanaliaMultiVideoDecodeSourceSlot] = source_slots;

    let mut completed_sequence_builder = None;
    let thread_result = thread::scope(|scope| -> Result<(), String> {
        let present_handoffs = handoffs.clone();
        let mut scene_video_overlay = scene_video_overlay;
        let present_worker = thread::Builder::new()
            .name("gilder-multi-video-present-worker".to_owned())
            .stack_size(256 * 1024)
            .spawn_scoped(scope, move || {
                let mut sequence_builder =
                    NativeVulkanVulkanaliaDecodedImagePresentSequenceBuilder::new(
                        requested_present_frame_count,
                        sequence_started_at,
                    );
                let mut present_frame_index = 0u32;
                let mut present_frame_timer =
                    NativeVulkanVulkanaliaPresentFrameTimer::new(
                        target_max_fps,
                        audio_master_clock,
                    );
                let mut pending_present_frame_slots = VecDeque::<u32>::new();
                let mut first_frame_preroll_pending = true;
                loop {
                    if present_frame_index >= requested_present_frame_count {
                        break;
                    }
                    let mut frames = Vec::with_capacity(present_handoffs.len());
                    for handoff in &present_handoffs {
                        let frame = if first_frame_preroll_pending {
                            handoff.recv_after_preroll(
                                DECODED_IMAGE_PRESENT_STARTUP_PREROLL_FRAMES,
                            )?
                        } else {
                            handoff.recv()?
                        };
                        let Some(frame) = frame else {
                            return Ok(sequence_builder);
                        };
                        frames.push(frame);
                    }
                    first_frame_preroll_pending = false;
                    if present_frame_index == 0 {
                        let started_at = Instant::now();
                        sequence_builder.started_at = started_at;
                        present_frame_timer.reset(started_at);
                    }
                    let present_frame_slot_count =
                        native_vulkan_vulkanalia_decoded_image_present_frame_slot_count(
                            decoded_image_present_frame_resources,
                        )
                        .max(1);
                    let present_frame_slot =
                        present_frame_index as usize % present_frame_slot_count;
                    native_vulkan_vulkanalia_prepare_decoded_image_present_frame_slot(
                        &context.device,
                        decoded_image_present_frame_resources,
                        present_frame_slot as u32,
                    )?;
                    for handoff in &present_handoffs {
                        handoff.complete_present_frame_slot_releases(present_frame_slot as u32)?;
                    }
                    if let Some(position) = pending_present_frame_slots
                        .iter()
                        .position(|slot| *slot == present_frame_slot as u32)
                    {
                        pending_present_frame_slots.remove(position);
                    }
                    for (slot, frame) in source_slots_ref.iter().zip(frames.iter()) {
                        if frame.sampled_array_layer >= slot.resource_image.snapshot.array_layers {
                            return Err(format!(
                                "multi-source decoded image present source {} sampled layer {} exceeds {} image layers",
                                slot.source.display(),
                                frame.sampled_array_layer,
                                slot.resource_image.snapshot.array_layers
                            ));
                        }
                    }
                    let timing = native_vulkan_vulkanalia_multi_source_frame_timing(&frames);
                    let (pacing_sleep_micros, pacing_clock_model) =
                        present_frame_timer.pace_frame(
                            present_frame_index,
                            timing.source_frame_pts_ns,
                            timing.source_frame_duration_ns,
                            timing.source_frame_pts_ms,
                            timing.source_frame_duration_ms,
                        );
                    let overlay_elapsed_ms = sequence_builder
                        .started_at
                        .elapsed()
                        .as_millis()
                        .min(u128::from(u64::MAX)) as u64;
                    let scene_overlay_draw = if let Some(scene_video_overlay) =
                        scene_video_overlay.as_deref_mut()
                    {
                        scene_video_overlay.frame_draw(
                            &context.device,
                            present_frame_slot as u32,
                            overlay_elapsed_ms,
                            swapchain_extent,
                        )?
                    } else {
                        None
                    };
                    let present_sources = source_slots_ref
                        .iter()
                        .zip(frames.iter())
                        .map(|(slot, frame)| slot.present_source(*frame))
                        .collect::<Vec<_>>();
                    let decode_waits = source_slots_ref
                        .iter()
                        .zip(frames.iter())
                        .map(|(slot, frame)| slot.decode_wait(*frame))
                        .collect::<Vec<_>>();
                    let mut record_layer_present_release = |present_frame_slot: u32| {
                        for (handoff, frame) in present_handoffs.iter().zip(frames.iter()) {
                            handoff.record_layer_present_release(
                                frame.sampled_array_layer,
                                present_frame_slot,
                            )?;
                        }
                        Ok(())
                    };
                    let draw =
                        native_vulkan_vulkanalia_present_decoded_image_frame_with_sources(
                            &context.device,
                            context.present_queue,
                            swapchain,
                            swapchain_images,
                            swapchain_format,
                            swapchain_extent,
                            decoded_image_present_pipeline,
                            decoded_image_present_frame_resources,
                            &present_sources,
                            0,
                            present_frame_index,
                            true,
                            timing.source_frame_pts_ns,
                            timing.source_frame_duration_ns,
                            timing.source_frame_pts_ms,
                            timing.source_frame_duration_ms,
                            i64::from(present_frame_index),
                            "multi-source-present-frame-index",
                            pacing_sleep_micros,
                            pacing_clock_model,
                            decoded_image_present_timing,
                            &decode_waits,
                            queue_host_access_lock,
                            Some(&mut record_layer_present_release),
                            clear_color,
                            scene_overlay_draw,
                        )?;
                    pending_present_frame_slots.push_back(draw.present_frame_slot);
                    let mut pending_slot_index = 0usize;
                    while pending_slot_index < pending_present_frame_slots.len() {
                        let pending_slot = pending_present_frame_slots[pending_slot_index];
                        if native_vulkan_vulkanalia_try_complete_decoded_image_present_frame_slot(
                            &context.device,
                            decoded_image_present_frame_resources,
                            pending_slot,
                        )? {
                            pending_present_frame_slots.remove(pending_slot_index);
                            for handoff in &present_handoffs {
                                handoff.complete_present_frame_slot_releases(pending_slot)?;
                            }
                        } else {
                            pending_slot_index += 1;
                        }
                    }
                    sequence_builder.push(draw);
                    present_frame_index = present_frame_index.saturating_add(1);
                }
                while let Some(present_frame_slot) = pending_present_frame_slots.pop_front() {
                    native_vulkan_vulkanalia_wait_decoded_image_present_frame_slot(
                        &context.device,
                        decoded_image_present_frame_resources,
                        present_frame_slot,
                    )?;
                    for handoff in &present_handoffs {
                        handoff.complete_present_frame_slot_releases(present_frame_slot)?;
                    }
                }
                let present_frame_slot_count =
                    native_vulkan_vulkanalia_decoded_image_present_frame_slot_count(
                        decoded_image_present_frame_resources,
                    );
                for present_frame_slot in 0..present_frame_slot_count {
                    native_vulkan_vulkanalia_wait_decoded_image_present_frame_slot(
                        &context.device,
                        decoded_image_present_frame_resources,
                        present_frame_slot as u32,
                    )?;
                    for handoff in &present_handoffs {
                        handoff.complete_present_frame_slot_releases(present_frame_slot as u32)?;
                    }
                }
                Ok(sequence_builder)
            })
            .map_err(|err| format!("spawn multi-source video present worker: {err}"))?;

        let mut decode_workers = Vec::with_capacity(source_slots.len());
        for ((slot, prepared), handoff) in source_slots_ref
            .iter()
            .zip(prepared_decodes.into_iter())
            .zip(handoffs.iter().cloned())
        {
            decode_workers.push(
                thread::Builder::new()
                    .name(format!("gilder-video-decode-source-{}", slot.source_index))
                    .stack_size(256 * 1024)
                    .spawn_scoped(scope, move || {
                        run_multi_video_decode_source_worker(
                            instance,
                            context,
                            selection,
                            slot,
                            prepared,
                            handoff,
                            requested_present_frame_count,
                            ffmpeg_decode_async_exec_depth,
                            decode_queue_host_access_lock,
                        )
                    })
                    .map_err(|err| format!("spawn multi-source video decode worker: {err}"))?,
            );
        }
        let mut decode_error = None;
        for worker in decode_workers {
            match worker.join() {
                Ok(Ok(())) => {}
                Ok(Err(err)) => {
                    decode_error.get_or_insert(err);
                }
                Err(_) => {
                    decode_error
                        .get_or_insert("multi-source video decode worker panicked".to_owned());
                }
            };
        }
        if let Some(ref err) = decode_error {
            for handoff in &handoffs {
                handoff.fail(err.clone());
            }
        }
        for handoff in &handoffs {
            handoff.close()?;
        }
        let present_result = match present_worker.join() {
            Ok(result) => result,
            Err(_) => Err("multi-source decoded image present worker panicked".to_owned()),
        };
        if let Some(err) = decode_error {
            return Err(err);
        }
        completed_sequence_builder = Some(present_result?);
        Ok(())
    });
    thread_result?;
    let sequence_builder = completed_sequence_builder
        .ok_or_else(|| "multi-source present worker produced no sequence".to_owned())?;
    let handoff_snapshot = native_vulkan_vulkanalia_multi_source_handoff_snapshot(&handoffs)?;
    let execution = NativeVulkanVulkanaliaDecodedImagePresentExecutionEvidence {
        ffmpeg_read_thread_active: true,
        video_decode_worker_active: true,
        present_worker_active: true,
        source_count: source_slot_count.min(u32::MAX as usize) as u32,
        decode_thread_count: source_slot_count.min(u32::MAX as usize) as u32,
        decode_async_exec_depth: ffmpeg_decode_async_exec_depth,
        ffmpeg_retained_avframe_count: 0,
        ffmpeg_retained_avframe_peak_count: 0,
        descriptor_sampler_cache_entry_count: 0,
        descriptor_sampler_cache_peak_entry_count: 0,
        descriptor_sampler_cache_rewrite_count: 0,
        descriptor_sampler_cache_recreate_count: 0,
        descriptor_sampler_cache_resource_heap_bytes: 0,
        descriptor_sampler_cache_sampler_heap_bytes: 0,
    };
    sequence_builder
        .finish(handoff_snapshot, execution)
        .ok_or_else(|| "multi-source present sequence has no rendered frames".to_owned())
}

include!("streaming_decode_present/multi_source_runtime.rs");
