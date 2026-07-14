#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, Copy)]
struct NativeVulkanVulkanaliaMultiSourceFrameTiming {
    source_frame_pts_ns: Option<u64>,
    source_frame_duration_ns: Option<u64>,
    source_frame_pts_ms: Option<u64>,
    source_frame_duration_ms: Option<u64>,
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_vulkanalia_multi_source_frame_timing(
    frames: &[NativeVulkanVulkanaliaDecodedPresentHandoffFrame],
) -> NativeVulkanVulkanaliaMultiSourceFrameTiming {
    NativeVulkanVulkanaliaMultiSourceFrameTiming {
        source_frame_pts_ns: frames
            .iter()
            .filter_map(|frame| frame.source_frame_pts_ns)
            .min(),
        source_frame_duration_ns: frames
            .iter()
            .filter_map(|frame| frame.source_frame_duration_ns)
            .min(),
        source_frame_pts_ms: frames
            .iter()
            .filter_map(|frame| frame.source_frame_pts_ms)
            .min(),
        source_frame_duration_ms: frames
            .iter()
            .filter_map(|frame| frame.source_frame_duration_ms)
            .min(),
    }
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_vulkanalia_multi_source_handoff_snapshot(
    handoffs: &[NativeVulkanVulkanaliaDecodedPresentHandoff],
) -> Result<NativeVulkanVulkanaliaDecodedPresentHandoffSnapshot, String> {
    let mut snapshots = Vec::with_capacity(handoffs.len());
    for handoff in handoffs {
        snapshots.push(handoff.snapshot(
            "multi-source-decoded-image-present-worker-layer-rings",
            "N FFmpeg FrameQueue-style decoded-frame handoffs: each decode source owns a fixed 3-frame ring and present consumes one frame from every source per swapchain frame",
            "no frame drop in ready-prefix evidence; decoded layer reuse waits on render-fence/final-drain completion per source",
            "present worker drains one FIFO metadata frame from every source before recording a shared dynamic-rendering pass",
            "frame pixels stay in each source's Vulkan decode image and are sampled through that source's descriptor heap",
            FFMPEG_FFPLAY_FRAME_QUEUE_REFERENCE,
        )?);
    }
    let mut aggregate = NativeVulkanVulkanaliaDecodedPresentHandoffSnapshot {
        binding: "vulkanalia",
        route: "multi-source-decoded-image-present-worker-layer-rings",
        model: "N independent FFmpeg FrameQueue-style decoded-frame handoffs feeding one shared Vulkan present worker",
        capacity_frames: 0,
        queued_frame_count_before_drain: 0,
        enqueued_frame_count: 0,
        dropped_frame_count: 0,
        drained_frame_count: 0,
        peak_depth: 0,
        keep_last_overwrite_enabled: true,
        drop_policy: "no frame drop in ready-prefix evidence; decoded layer reuse waits on render-fence/final-drain completion per source",
        drain_order: "present worker drains one FIFO metadata frame from every source before recording a shared dynamic-rendering pass",
        zero_copy_scope: "frame pixels stay in each source's Vulkan decode image and are sampled through that source's descriptor heap",
        ffmpeg_reference: FFMPEG_FFPLAY_FRAME_QUEUE_REFERENCE,
    };
    for snapshot in snapshots {
        aggregate.capacity_frames = aggregate
            .capacity_frames
            .saturating_add(snapshot.capacity_frames);
        aggregate.queued_frame_count_before_drain = aggregate
            .queued_frame_count_before_drain
            .saturating_add(snapshot.queued_frame_count_before_drain);
        aggregate.enqueued_frame_count = aggregate
            .enqueued_frame_count
            .saturating_add(snapshot.enqueued_frame_count);
        aggregate.dropped_frame_count = aggregate
            .dropped_frame_count
            .saturating_add(snapshot.dropped_frame_count);
        aggregate.drained_frame_count = aggregate
            .drained_frame_count
            .saturating_add(snapshot.drained_frame_count);
        aggregate.peak_depth = aggregate.peak_depth.max(snapshot.peak_depth);
        aggregate.keep_last_overwrite_enabled &= snapshot.keep_last_overwrite_enabled;
    }
    Ok(aggregate)
}

#[cfg(feature = "native-vulkan-video")]
#[allow(clippy::too_many_arguments)]
fn run_multi_video_decode_source_worker(
    instance: &Instance,
    context: &NativeVulkanVulkanaliaVideoPresentDeviceContext,
    selection: &super::video_present_device::NativeVulkanVulkanaliaVideoPresentPhysicalDeviceSelection,
    slot: &NativeVulkanVulkanaliaMultiVideoDecodeSourceSlot,
    mut prepared_streaming_decode: NativeVulkanVulkanaliaPreparedStreamingDecode,
    handoff: NativeVulkanVulkanaliaDecodedPresentHandoff,
    requested_present_frame_count: u32,
    ffmpeg_decode_async_exec_depth: u32,
    queue_host_access_lock: Option<&Mutex<()>>,
) -> Result<(), String> {
    let av1_sequence_header = prepared_streaming_decode
        .av1
        .as_ref()
        .map(|prepared| prepared.sequence_header.clone());
    with_native_vulkan_vulkanalia_video_session_capabilities(
        instance,
        selection.physical_device,
        slot.codec,
        None,
        av1_sequence_header.as_ref(),
        |profile_info, queried| {
            let decode_complete_value = std::cell::Cell::new(
                unsafe {
                    context
                        .device
                        .get_semaphore_counter_value(slot.decode_complete)
                }
                .map_err(|err| {
                    format!(
                        "vkGetSemaphoreCounterValue(multi-source decode_complete {}): {err:?}",
                        slot.source.display()
                    )
                })?,
            );
            let mut wait_for_output_slot_present_release =
                |sampled_array_layer: u32| -> Result<(), String> {
                    handoff.wait_layer_present_release_completed(sampled_array_layer)
                };
            let mut enqueue_decoded_frame = |decode_frame_index: u32,
                                             sampled_array_layer: u32,
                                             source_frame_pts_ns: Option<u64>,
                                             source_frame_duration_ns: Option<u64>,
                                             source_frame_pts_ms: Option<u64>,
                                             source_frame_duration_ms: Option<u64>,
                                             display_order_key: i64,
                                             display_order_key_source: &'static str,
                                             decode_complete_value: u64|
             -> Result<(), String> {
                if decode_frame_index >= requested_present_frame_count {
                    return Ok(());
                }
                handoff.enqueue(NativeVulkanVulkanaliaDecodedPresentHandoffFrame {
                    decode_frame_index,
                    sampled_array_layer,
                    source_frame_pts_ns,
                    source_frame_duration_ns,
                    source_frame_pts_ms,
                    source_frame_duration_ms,
                    display_order_key,
                    display_order_key_source,
                    decode_complete_value,
                })
            };
            match slot.codec {
                NativeVulkanVideoSessionCodec::H264High8 => {
                    let Some(prepared) = prepared_streaming_decode.h264.take() else {
                        return Err(format!(
                            "multi-source H.264 decode source {} has no prepared stream",
                            slot.source.display()
                        ));
                    };
                    let NativeVulkanVulkanaliaPreparedH264StreamingDecode {
                        request: _,
                        mut queue,
                        parameter_sets,
                        bootstrap,
                    } = prepared;
                    let mut planner = NativeVulkanH264DecodeReferencePlanner::new(
                        slot.resource_image_array_layers,
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
                    native_vulkan_vulkanalia_record_h264_streaming_decode_into_image(
                        &context.device,
                        context.video_queue,
                        queue_host_access_lock,
                        &slot.memory_properties,
                        selection.video_queue_family_index,
                        profile_info,
                        slot.requested_extent,
                        queried.capabilities,
                        slot.session,
                        slot.codec,
                        slot.resource_image_array_layers,
                        ffmpeg_decode_async_exec_depth,
                        selection.properties.limits.non_coherent_atom_size,
                        NativeVulkanVulkanaliaH264StreamingDecodeInput {
                            parameter_sets,
                            requested_frame_count: requested_present_frame_count,
                            next_frame: &mut next_frame,
                        },
                        &slot.resource_image,
                        Some(&mut wait_for_output_slot_present_release),
                        Some(&mut enqueue_decoded_frame),
                        slot.decode_complete,
                        &decode_complete_value,
                    )?;
                }
                NativeVulkanVideoSessionCodec::H265Main8
                | NativeVulkanVideoSessionCodec::H265Main10 => {
                    let Some(prepared) = prepared_streaming_decode.h265.take() else {
                        return Err(format!(
                            "multi-source H.265 decode source {} has no prepared stream",
                            slot.source.display()
                        ));
                    };
                    let NativeVulkanVulkanaliaPreparedH265StreamingDecode {
                        request: _,
                        mut queue,
                        parameter_sets,
                        bootstrap,
                    } = prepared;
                    let mut planner = NativeVulkanH265DecodeReferencePlanner::new(
                        slot.resource_image_array_layers,
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
                    native_vulkan_vulkanalia_record_h265_streaming_decode_into_image(
                        &context.device,
                        context.video_queue,
                        queue_host_access_lock,
                        &slot.memory_properties,
                        selection.video_queue_family_index,
                        profile_info,
                        slot.requested_extent,
                        queried.capabilities,
                        slot.session,
                        slot.codec,
                        slot.resource_image_array_layers,
                        ffmpeg_decode_async_exec_depth,
                        selection.properties.limits.non_coherent_atom_size,
                        NativeVulkanVulkanaliaH265StreamingDecodeInput {
                            parameter_sets,
                            requested_frame_count: requested_present_frame_count,
                            next_frame: &mut next_frame,
                        },
                        &slot.resource_image,
                        Some(&mut wait_for_output_slot_present_release),
                        Some(&mut enqueue_decoded_frame),
                        slot.decode_complete,
                        &decode_complete_value,
                    )?;
                }
                NativeVulkanVideoSessionCodec::Av1Main8
                | NativeVulkanVideoSessionCodec::Av1Main10 => {
                    let Some(prepared) = prepared_streaming_decode.av1.take() else {
                        return Err(format!(
                            "multi-source AV1 decode source {} has no prepared stream",
                            slot.source.display()
                        ));
                    };
                    let NativeVulkanVulkanaliaPreparedAv1StreamingDecode {
                        request: _,
                        mut queue,
                        sequence_header,
                        bootstrap: _,
                    } = prepared;
                    let av1_planner_dpb_slots = slot.resource_image_array_layers.max(1);
                    let mut planner =
                        NativeVulkanAv1DecodeReferencePlanner::new(av1_planner_dpb_slots);
                    let mut active_dpb_refs = vec![
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
                    native_vulkan_vulkanalia_record_av1_streaming_decode_into_image(
                        &context.device,
                        context.video_queue,
                        queue_host_access_lock,
                        &slot.memory_properties,
                        selection.video_queue_family_index,
                        profile_info,
                        slot.requested_extent,
                        queried.capabilities,
                        slot.session,
                        slot.codec,
                        slot.resource_image_array_layers,
                        ffmpeg_decode_async_exec_depth,
                        selection.properties.limits.non_coherent_atom_size,
                        NativeVulkanVulkanaliaAv1StreamingDecodeInput {
                            sequence_header: sequence_header.clone(),
                            requested_frame_count: requested_present_frame_count,
                            next_frame: &mut next_frame,
                        },
                        &slot.resource_image,
                        Some(&mut wait_for_output_slot_present_release),
                        Some(&mut enqueue_decoded_frame),
                        slot.decode_complete,
                        &decode_complete_value,
                    )?;
                }
            }
            Ok(())
        },
    )
}

pub(in crate::renderer::native_vulkan::vulkan) fn create_native_vulkan_vulkanalia_video_present_session_runtime(
    options: NativeVulkanVulkanaliaVideoPresentSessionProbeOptions,
) -> Result<NativeVulkanVulkanaliaVideoPresentSessionRuntime, String> {
    create_native_vulkan_vulkanalia_video_present_session_runtime_with_ready_prefix_decode(
        options,
        NativeVulkanVulkanaliaStreamingDecodeRequests::default(),
        0,
        None,
    )
}

fn create_native_vulkan_vulkanalia_video_present_session_runtime_with_ready_prefix_decode(
    options: NativeVulkanVulkanaliaVideoPresentSessionProbeOptions,
    streaming_decode: NativeVulkanVulkanaliaStreamingDecodeRequests,
    requested_present_frame_count: u32,
    scene_video_overlay_input: Option<NativeVulkanVulkanaliaSceneVideoOverlayInput>,
) -> Result<NativeVulkanVulkanaliaVideoPresentSessionRuntime, String> {
    if options.width == 0 || options.height == 0 {
        return Err("Vulkanalia video present session runtime requires non-zero extent".to_owned());
    }

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

    let physical_devices = match unsafe { instance.enumerate_physical_devices() } {
        Ok(physical_devices) => physical_devices,
        Err(err) => {
            unsafe {
                instance.destroy_surface_khr(surface, None);
            }
            native_vulkan_vulkanalia_destroy_instance(vulkan);
            return Err(format!(
                "vkEnumeratePhysicalDevices(vulkanalia video present runtime): {err:?}"
            ));
        }
    };
    let codec_set = [options.codec];
    let selection = match select_video_present_physical_device(
        instance,
        surface,
        handles,
        &physical_devices,
        &codec_set,
    ) {
        Ok(selection) => selection,
        Err(err) => {
            unsafe {
                instance.destroy_surface_khr(surface, None);
            }
            native_vulkan_vulkanalia_destroy_instance(vulkan);
            return Err(err);
        }
    };
    let context = match create_video_present_device(
        instance,
        &selection,
        &codec_set,
        vulkanalia_surface_maintenance1_enabled(&vulkan),
    ) {
        Ok(context) => context,
        Err(err) => {
            unsafe {
                instance.destroy_surface_khr(surface, None);
            }
            native_vulkan_vulkanalia_destroy_instance(vulkan);
            return Err(err);
        }
    };
    let swapchain_plan = match create_vulkanalia_swapchain_plan(
        instance,
        selection.physical_device,
        surface,
        handles.buffer_size,
        vulkanalia_surface_capabilities2_enabled(&vulkan),
        &context.present_feature_selection,
    ) {
        Ok(plan) => plan,
        Err(err) => {
            unsafe {
                context.device.destroy_device(None);
                instance.destroy_surface_khr(surface, None);
            }
            native_vulkan_vulkanalia_destroy_instance(vulkan);
            return Err(err);
        }
    };
    let swapchain = match unsafe {
        context
            .device
            .create_swapchain_khr(&swapchain_plan.create_info, None)
    } {
        Ok(swapchain) => swapchain,
        Err(err) => {
            unsafe {
                context.device.destroy_device(None);
                instance.destroy_surface_khr(surface, None);
            }
            native_vulkan_vulkanalia_destroy_instance(vulkan);
            return Err(format!(
                "vkCreateSwapchainKHR(vulkanalia retained video present): {err:?}"
            ));
        }
    };
    let swapchain_images = match unsafe { context.device.get_swapchain_images_khr(swapchain) } {
        Ok(images) => images,
        Err(err) => {
            unsafe {
                context.device.destroy_swapchain_khr(swapchain, None);
                context.device.destroy_device(None);
                instance.destroy_surface_khr(surface, None);
            }
            native_vulkan_vulkanalia_destroy_instance(vulkan);
            return Err(format!(
                "vkGetSwapchainImagesKHR(vulkanalia retained video present): {err:?}"
            ));
        }
    };

    // FFmpeg/ffplay drives cadence from the frame queue and PTS-derived refresh
    // timer (references/ffmpeg/fftools/ffplay.c:1609-1743,1796-1823). WSI
    // present-id2/wait2 still remain enabled for modern present telemetry and
    // optional diagnostic waits when the swapchain was created with them.
    let decoded_image_present_timing = VulkanaliaDecodedImagePresentTimingConfig::new(
        swapchain_plan.present_id2_enabled,
        swapchain_plan.present_wait2_enabled,
    );

    let pieces = match create_video_present_session_pieces(
        instance,
        &vulkan,
        &context,
        &selection,
        options.codec,
        options.width,
        options.height,
        swapchain,
        &swapchain_images,
        swapchain_plan.extent,
        swapchain_plan.format.format,
        options.target_max_fps,
        options.audio_master_clock,
        decoded_image_present_timing,
        options.clear_color,
        swapchain_plan_snapshot(&swapchain_plan, swapchain_images.len()),
        streaming_decode,
        requested_present_frame_count,
        scene_video_overlay_input,
    ) {
        Ok(pieces) => pieces,
        Err(err) => {
            unsafe {
                context.device.destroy_swapchain_khr(swapchain, None);
                context.device.destroy_device(None);
                instance.destroy_surface_khr(surface, None);
            }
            native_vulkan_vulkanalia_destroy_instance(vulkan);
            return Err(err);
        }
    };

    let NativeVulkanVulkanaliaVideoPresentSessionPieces {
        session,
        memory_resources,
        resource_image,
        decoded_image_present_pipeline,
        decoded_image_present_sampler,
        scene_video_overlay,
        mut snapshot,
        decoded_image_present_sequence,
        decoded_image_present_sequence_error,
        h264_ready_prefix_decode,
        h265_ready_prefix_decode,
        av1_ready_prefix_decode,
    } = pieces;
    snapshot.surface_host = Some(surface_host_snapshot);
    Ok(NativeVulkanVulkanaliaVideoPresentSessionRuntime {
        resources: Some(NativeVulkanVulkanaliaVideoPresentSessionRuntimeResources {
            _surface_host: surface_host,
            vulkan: Some(vulkan),
            surface,
            context: Some(context),
            swapchain,
            swapchain_images,
            swapchain_format: swapchain_plan.format.format,
            swapchain_extent: swapchain_plan.extent,
            decoded_image_present_timing,
            clear_color: options.clear_color,
            present_queue_family_index: selection.present_queue_family_index,
            picture_format: native_vulkan_vulkanalia_video_session_effective_picture_format(
                options.codec,
                None,
            ),
            session,
            memory_resources: Some(memory_resources),
            resource_image: Some(resource_image),
            decoded_image_present_pipeline,
            decoded_image_present_sampler,
            scene_video_overlay,
            decoded_image_present_sequence,
            decoded_image_present_sequence_error,
            h264_ready_prefix_decode,
            h265_ready_prefix_decode,
            av1_ready_prefix_decode,
        }),
        snapshot,
    })
}
