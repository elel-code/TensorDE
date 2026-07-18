pub fn run_native_vulkan_ffmpeg_vulkan_hw_video_present(
    mut options: NativeVulkanFfmpegVulkanHwVideoPresentOptions,
) -> Result<NativeVulkanFfmpegVulkanHwVideoPresentSnapshot, String> {
    if options.playback_frame_count == 0 {
        return Err(
            "FFmpeg Vulkan HW video present requires at least one playback frame".to_owned(),
        );
    }
    if !options.source.is_file() {
        return Err(format!(
            "FFmpeg Vulkan HW video source does not exist: {}",
            options.source.display()
        ));
    }

    let audio_clock_preparation = native_vulkan_ffmpeg_prepare_audio_clock_for_video_present(
        NativeVulkanFfmpegVideoAudioClockPrepareOptions {
            source: options.source.clone(),
            playback_frame_count: options.playback_frame_count,
            target_max_fps: options.target_max_fps,
            audio_clock_probe_requested: options.audio_clock_probe_requested,
            audio_output_mode: options.audio_output_mode,
        },
    )?;
    let mut audio_clock = audio_clock_preparation.clock;
    let audio_output_worker = audio_clock_preparation.worker;
    let media_audio_events = audio_clock_preparation.event_channel;
    let audio_master_clock_enabled = audio_clock
        .as_ref()
        .is_some_and(|clock| clock.video_master_clock_ready);
    let audio_master_clock_start_ns = audio_clock
        .as_ref()
        .and_then(|clock| clock.video_master_start_clock_ns);
    options.audio_master_clock = if audio_master_clock_enabled {
        NativeVulkanVulkanaliaVideoPresentAudioMasterClock::clock_only(audio_master_clock_start_ns)
    } else {
        NativeVulkanVulkanaliaVideoPresentAudioMasterClock::DISABLED
    };
    let audio_clock_probe_requested = options.audio_clock_probe_requested;
    let audio_output_mode = options.audio_output_mode.as_str();

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

    let result = (|| -> Result<NativeVulkanFfmpegVulkanHwVideoPresentSnapshot, String> {
        let physical_devices = unsafe { instance.enumerate_physical_devices() }.map_err(|err| {
            format!("vkEnumeratePhysicalDevices(FFmpeg Vulkan HW video present): {err:?}")
        })?;
        let codec_set = [options.codec];
        let selection = select_video_present_physical_device(
            instance,
            surface,
            handles,
            &physical_devices,
            &codec_set,
        )?;
        let context = create_video_present_device(
            instance,
            &selection,
            &codec_set,
            vulkanalia_surface_maintenance1_enabled(&vulkan),
        )?;
        let context_result =
            (|| -> Result<NativeVulkanFfmpegVulkanHwVideoPresentSnapshot, String> {
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
                    format!("vkCreateSwapchainKHR(FFmpeg Vulkan HW video present): {err:?}")
                })?;
                let swapchain_result =
                    (|| -> Result<NativeVulkanFfmpegVulkanHwVideoPresentSnapshot, String> {
                        let swapchain_images = unsafe {
                            context.device.get_swapchain_images_khr(swapchain)
                        }
                        .map_err(|err| {
                            format!(
                                "vkGetSwapchainImagesKHR(FFmpeg Vulkan HW video present): {err:?}"
                            )
                        })?;
                        let swapchain_snapshot =
                            swapchain_plan_snapshot(&swapchain_plan, swapchain_images.len());
                        let device_snapshot = device_snapshot_from_selection(
                            &vulkan,
                            &selection,
                            &context,
                            options.codec,
                            swapchain_snapshot,
                        );
                        run_native_vulkan_ffmpeg_vulkan_hw_video_present_on_device(
                            instance,
                            &vulkan,
                            &context,
                            &selection,
                            swapchain,
                            &swapchain_images,
                            swapchain_plan.format.format,
                            swapchain_plan.extent,
                            device_snapshot,
                            swapchain_plan.present_id2_enabled,
                            swapchain_plan.present_wait2_enabled,
                            media_audio_events.clone(),
                            options,
                        )
                    })();
                unsafe {
                    context.device.destroy_swapchain_khr(swapchain, None);
                }
                swapchain_result
            })();
        let _ = unsafe { context.device.device_wait_idle() };
        unsafe {
            context.device.destroy_device(None);
        }
        context_result
    })();

    unsafe {
        instance.destroy_surface_khr(surface, None);
    }
    native_vulkan_vulkanalia_destroy_instance(vulkan);
    drop(surface_host);
    let audio_output_result = if let Some(worker) = audio_output_worker {
        Some(
            worker
                .join()
                .map_err(|_| "PipeWire audio output worker panicked".to_owned())
                .and_then(|result| result),
        )
    } else {
        None
    };
    if let Some(audio_output_result) = audio_output_result {
        audio_clock = Some(audio_output_result?);
    }
    result.map(|mut snapshot| {
        snapshot.audio_clock_probe_requested = audio_clock_probe_requested;
        snapshot.audio_output_mode = audio_output_mode;
        snapshot.audio_master_clock_enabled = audio_master_clock_enabled;
        snapshot.audio_master_clock_start_ns = audio_master_clock_start_ns;
        snapshot.audio_clock = audio_clock;
        snapshot.surface_host = Some(surface_host_snapshot);
        snapshot
    })
}

#[cfg(feature = "native-vulkan-video")]
#[allow(clippy::too_many_arguments)]
fn run_native_vulkan_ffmpeg_vulkan_hw_video_present_on_device(
    instance: &Instance,
    vulkan: &NativeVulkanVulkanaliaInstance,
    context: &NativeVulkanVulkanaliaVideoPresentDeviceContext,
    selection: &super::video_present_device::NativeVulkanVulkanaliaVideoPresentPhysicalDeviceSelection,
    swapchain: vk::SwapchainKHR,
    swapchain_images: &[vk::Image],
    swapchain_format: vk::Format,
    swapchain_extent: vk::Extent2D,
    device_snapshot: super::video_present_device::NativeVulkanVulkanaliaVideoPresentDeviceProbeSnapshot,
    present_id2_enabled: bool,
    present_wait2_enabled: bool,
    media_audio_events: NativeVulkanAudioEventChannel,
    options: NativeVulkanFfmpegVulkanHwVideoPresentOptions,
) -> Result<NativeVulkanFfmpegVulkanHwVideoPresentSnapshot, String> {
    let memory_properties =
        unsafe { instance.get_physical_device_memory_properties(selection.physical_device) };
    let same_queue_family =
        selection.video_queue_family_index == selection.present_queue_family_index;
    let ffmpeg_present_queue_count = if same_queue_family {
        1
    } else {
        selection.present_queue_count.min(1).max(1)
    };
    let ffmpeg_visible_device_extensions =
        native_vulkan_ffmpeg_vulkan_hw_visible_device_extensions(context, options.codec);
    let hw_device_borrow = NativeVulkanFfmpegVulkanHwDeviceBorrow {
        instance: &vulkan.instance,
        physical_device: selection.physical_device,
        device: &context.device,
        enabled_instance_extensions: &vulkan.extension_selection.enabled_instance_extensions,
        enabled_device_extensions: &ffmpeg_visible_device_extensions,
        video_queue_family_index: selection.video_queue_family_index,
        video_queue_count: 1,
        video_queue_flags: selection.video_queue_flags,
        video_codec_operations: native_vulkan_ffmpeg_vulkan_hw_codec_operations(options.codec),
        present_queue_family_index: selection.present_queue_family_index,
        present_queue_count: ffmpeg_present_queue_count,
        present_queue_flags: selection.present_queue_flags,
    };
    let hw_device = NativeVulkanFfmpegVulkanHwDevice::borrow_existing(hw_device_borrow)?;
    let decoder =
        NativeVulkanFfmpegVulkanHwDecoder::open(&options.source, options.codec, &hw_device)?;
    let mut decoder_snapshot = decoder.snapshot();
    let time_base = decoder_snapshot.time_base;

    let decoded_image_present_timing =
        VulkanaliaDecodedImagePresentTimingConfig::new(present_id2_enabled, present_wait2_enabled);
    let frame_resources = native_vulkan_vulkanalia_create_decoded_image_present_frame_resources(
        &context.device,
        swapchain_images,
        swapchain_format,
        selection.present_queue_family_index,
    )?;
    let mut frame_resources = Some(frame_resources);
    let mut decoded_image_present_pipeline = None;
    let mut sequence_builder = NativeVulkanVulkanaliaDecodedImagePresentSequenceBuilder::new(
        options.playback_frame_count,
        Instant::now(),
    );
    let mut present_frame_timer = NativeVulkanVulkanaliaPresentFrameTimer::new(
        options.target_max_fps,
        options.audio_master_clock,
    );
    let mut media_event_runtime = NativeVulkanMediaEventRuntime::new(media_audio_events);

    let threaded_decode =
        selection.video_queue_family_index != selection.present_queue_family_index;
    let release_frame_after_render_fence =
        native_vulkan_ffmpeg_vulkan_hwdecode_release_frame_after_render_fence();
    let frame_queue_size = native_vulkan_ffmpeg_vulkan_hwdecode_frame_queue_size();
    let sequence_result = thread::scope(
        |scope| -> Result<
            (
                NativeVulkanVulkanaliaDecodedImagePresentSequenceSnapshot,
                NativeVulkanFfmpegVulkanHwDecoderSnapshot,
            ),
            String,
        > {
            let mut decoder = Some(decoder);
            let mut decode_worker = None;
            let mut frame_receiver = None;
            if threaded_decode {
                let mut worker_decoder = decoder
                    .take()
                    .ok_or_else(|| "FFmpeg Vulkan HW decoder was moved early".to_owned())?;
                let (sender, receiver) = mpsc::sync_channel::<
                    Result<NativeVulkanFfmpegDecodedGpuFrameHandoff, String>,
                >(frame_queue_size);
                let playback_frame_count = options.playback_frame_count;
                decode_worker = Some(
                    thread::Builder::new()
                        .name("gilder-ffmpeg-vulkan-decode-worker".to_owned())
                        .stack_size(256 * 1024)
                        .spawn_scoped(
                            scope,
                            move || -> Result<NativeVulkanFfmpegVulkanHwDecoderSnapshot, String> {
                            for _ in 0..playback_frame_count {
                                let decoded =
                                    worker_decoder.decode_next_frame(true).and_then(|frame| {
                                        frame.ok_or_else(|| {
                                            "FFmpeg Vulkan HW decoder reached EOF before producing a presentable AVVkFrame".to_owned()
                                        })
                                    });
                                match decoded {
                                    Ok(decoded_frame) => {
                                        let media_generation =
                                            u64::from(worker_decoder.loop_count());
                                        if release_frame_after_render_fence {
                                            let (release_sender, release_receiver) =
                                                mpsc::sync_channel::<()>(0);
                                            if sender
                                                .send(Ok(NativeVulkanFfmpegDecodedGpuFrameHandoff::new(
                                                    decoded_frame,
                                                    media_generation,
                                                    Some(release_sender),
                                                )))
                                                .is_err()
                                            {
                                                return Ok(worker_decoder.snapshot());
                                            }
                                            if release_receiver.recv().is_err() {
                                                return Ok(worker_decoder.snapshot());
                                            }
                                        } else if sender
                                            .send(Ok(NativeVulkanFfmpegDecodedGpuFrameHandoff::new(
                                                decoded_frame,
                                                media_generation,
                                                None,
                                            )))
                                            .is_err()
                                        {
                                            return Ok(worker_decoder.snapshot());
                                        }
                                    }
                                    Err(err) => {
                                        if sender.send(Err(err)).is_err() {
                                            return Ok(worker_decoder.snapshot());
                                        }
                                        return Ok(worker_decoder.snapshot());
                                    }
                                }
                            }
                            Ok(worker_decoder.snapshot())
                            },
                        )
                        .map_err(|err| format!("spawn FFmpeg Vulkan HW decode worker: {err}"))?,
                );
                frame_receiver = Some(receiver);
            }

            let frame_resources_ref = frame_resources.as_ref().ok_or_else(|| {
                "FFmpeg decoded present frame resources were released early".to_owned()
            })?;
            let mut retained_frames = NativeVulkanFfmpegPresentedFrameRetentionQueue::new(
                &context.device,
                frame_resources_ref,
            );
            let mut sampler_cache = NativeVulkanFfmpegPresentSamplerCache::new(
                &context.device,
                &memory_properties,
                selection.video_queue_family_index,
                selection.present_queue_family_index,
                context
                    .video_feature_selection
                    .core_features
                    .descriptor_heap,
                context.video_feature_selection.descriptor_heap_properties,
            );
            let mut present_error = None;
            for present_frame_index in 0..options.playback_frame_count {
                let decoded_frame_handoff_result = if let Some(receiver) = frame_receiver.as_ref() {
                    receiver
                        .recv()
                        .map_err(|_| "FFmpeg Vulkan HW decode worker closed before producing the requested frame".to_owned())
                        .and_then(|frame| frame)
                } else {
                    decoder
                        .as_mut()
                        .ok_or_else(|| "FFmpeg Vulkan HW decoder is unavailable".to_owned())
                        .and_then(|decoder| {
                            let decoded_frame = decoder.decode_next_frame(true)?.ok_or_else(|| {
                                "FFmpeg Vulkan HW decoder reached EOF before producing a presentable AVVkFrame".to_owned()
                            })?;
                            let media_generation = u64::from(decoder.loop_count());
                            Ok(NativeVulkanFfmpegDecodedGpuFrameHandoff::new(
                                decoded_frame,
                                media_generation,
                                None,
                            ))
                        })
                };
                let decoded_frame_handoff = match decoded_frame_handoff_result {
                    Ok(decoded_frame_handoff) => decoded_frame_handoff,
                    Err(err) => {
                        present_error = Some(err);
                        break;
                    }
                };
                let frame_result = (|| -> Result<u32, String> {
                    let decoded_frame = &decoded_frame_handoff.decoded_frame;
                    let descriptor_source = decoded_frame.descriptor_source()?;
                    let frame_resources_ref = frame_resources.as_ref().ok_or_else(|| {
                        "FFmpeg decoded present frame resources were released early".to_owned()
                    })?;
                    let present_frame_slot_count =
                        native_vulkan_vulkanalia_decoded_image_present_frame_slot_count(
                            frame_resources_ref,
                        );
                    if present_frame_slot_count == 0 {
                        return Err(
                            "FFmpeg decoded present requires at least one present frame slot"
                                .to_owned(),
                        );
                    }
                    let present_frame_slot =
                        present_frame_index as usize % present_frame_slot_count;
                    native_vulkan_vulkanalia_prepare_decoded_image_present_frame_slot(
                        &context.device,
                        frame_resources_ref,
                        present_frame_slot as u32,
                    )?;
                    retained_frames.release_completed_slot(present_frame_slot as u32);
                    let sampler_index =
                        sampler_cache.ensure_for_descriptor_source(&descriptor_source)?;
                    let sampler = sampler_cache.sampler(sampler_index)?;
                    if decoded_image_present_pipeline.is_none() {
                        let target_extent = swapchain_extent;
                        let pipeline =
                        native_vulkan_vulkanalia_create_decoded_image_present_pipeline_resources(
                            &context.device,
                            swapchain_format,
                            target_extent,
                            &sampler.snapshot.descriptor_heap_plan,
                        )?;
                        decoded_image_present_pipeline = Some(pipeline);
                    }
                    let frame_resources_ref = frame_resources.as_ref().ok_or_else(|| {
                        "FFmpeg decoded present frame resources were released early".to_owned()
                    })?;
                    let present_source = native_vulkan_ffmpeg_decoded_gpu_frame_present_source(
                        &descriptor_source,
                        &sampler,
                    )?;
                    let decode_wait =
                        native_vulkan_ffmpeg_decoded_gpu_frame_decode_wait(&descriptor_source)?;
                    let source_frame_pts_ns = native_vulkan_ffmpeg_time_base_timestamp_ns(
                        descriptor_source.pts_raw,
                        time_base,
                    );
                    let source_frame_duration_ns = native_vulkan_ffmpeg_time_base_timestamp_ns(
                        descriptor_source.duration_raw,
                        time_base,
                    );
                    let source_frame_pts_ms = source_frame_pts_ns.map(|pts_ns| pts_ns / 1_000_000);
                    let source_frame_duration_ms =
                        source_frame_duration_ns.map(|duration_ns| duration_ns / 1_000_000);
                    let (pacing_sleep_micros, pacing_clock_model) = present_frame_timer.pace_frame(
                        present_frame_index,
                        source_frame_pts_ns,
                        source_frame_duration_ns,
                        source_frame_pts_ms,
                        source_frame_duration_ms,
                    );
                    let display_order_key = descriptor_source
                        .pts_raw
                        .unwrap_or_else(|| i64::from(present_frame_index));
                    let display_order_key_source = if descriptor_source.pts_raw.is_some() {
                        "ffmpeg-avframe-pts"
                    } else {
                        "present-frame-index"
                    };
                    let draw = native_vulkan_vulkanalia_present_decoded_image_frame_with_sources(
                        &context.device,
                        context.present_queue,
                        swapchain,
                        swapchain_images,
                        swapchain_format,
                        swapchain_extent,
                        decoded_image_present_pipeline
                            .as_ref()
                            .expect("FFmpeg decoded present pipeline is live"),
                        frame_resources_ref,
                        &[present_source],
                        0,
                        present_frame_index,
                        true,
                        source_frame_pts_ns,
                        source_frame_duration_ns,
                        source_frame_pts_ms,
                        source_frame_duration_ms,
                        display_order_key,
                        display_order_key_source,
                        pacing_sleep_micros,
                        pacing_clock_model,
                        decoded_image_present_timing,
                        &[decode_wait],
                        None,
                        None,
                        options.clear_color,
                        None,
                    )?;
                    let presentation_time_ns = source_frame_pts_ns.unwrap_or_else(|| {
                        source_frame_duration_ns
                            .unwrap_or_default()
                            .saturating_mul(u64::from(present_frame_index))
                    });
                    let frame_identity = u64::from(present_frame_index).saturating_add(1);
                    media_event_runtime.publish_presented_frame(NativeVulkanVideoEventSample {
                        generation: decoded_frame_handoff.media_generation,
                        frame_serial: frame_identity,
                        frame_identity,
                        presentation_time_ns,
                        frame_duration_ns: source_frame_duration_ns,
                        media_duration_ns: None,
                        playback: SceneMediaPlaybackState::Playing,
                        rate_milli: 1_000,
                        loop_index: decoded_frame_handoff.media_generation,
                        ready: draw.presented,
                    });
                    let present_frame_slot = draw.present_frame_slot;
                    sequence_builder.push(draw);
                    Ok(present_frame_slot)
                })();
                match frame_result {
                    Ok(present_frame_slot) => {
                        if release_frame_after_render_fence {
                            let release_result = frame_resources
                                .as_ref()
                                .ok_or_else(|| {
                                    "FFmpeg decoded present frame resources were released early"
                                        .to_owned()
                                })
                                .and_then(|frame_resources_ref| {
                                    native_vulkan_vulkanalia_wait_decoded_image_present_frame_slot(
                                        &context.device,
                                        frame_resources_ref,
                                        present_frame_slot,
                                    )
                                    .map(|_| ())
                                });
                            decoded_frame_handoff.release();
                            if let Err(err) = release_result {
                                present_error = Some(err);
                                break;
                            }
                        } else {
                            retained_frames.push_after_submit(
                                present_frame_slot,
                                decoded_frame_handoff.into_retained_frame(),
                            )?;
                        }
                    }
                    Err(err) => {
                        let _ = unsafe { context.device.device_wait_idle() };
                        decoded_frame_handoff.release();
                        present_error = Some(err);
                        break;
                    }
                }
            }
            drop(frame_receiver.take());
            let decode_worker_result = if let Some(worker) = decode_worker.take() {
                match worker.join() {
                    Ok(result) => result,
                    Err(_) => Err("FFmpeg Vulkan HW decode worker panicked".to_owned()),
                }
            } else {
                decoder
                    .as_ref()
                    .map(|decoder| decoder.snapshot())
                    .ok_or_else(|| "FFmpeg Vulkan HW decoder is unavailable".to_owned())
            };
            if let Some(err) = present_error {
                if let Err(decode_err) = decode_worker_result {
                    return Err(format!("{err}; decode worker also failed: {decode_err}"));
                }
                return Err(err);
            }
            let final_decoder_snapshot = decode_worker_result?;
            retained_frames.drain_after_waits()?;
            let ffmpeg_retained_avframe_count = retained_frames.frame_count();
            let ffmpeg_retained_avframe_peak_count = retained_frames.peak_frame_count();
            let descriptor_sampler_cache_entry_count = sampler_cache.entry_count();
            let descriptor_sampler_cache_peak_entry_count = sampler_cache.peak_entry_count();
            let descriptor_sampler_cache_rewrite_count = sampler_cache.descriptor_rewrite_count();
            let descriptor_sampler_cache_recreate_count = sampler_cache.descriptor_recreate_count();
            let descriptor_sampler_cache_resource_heap_bytes = sampler_cache.resource_heap_bytes();
            let descriptor_sampler_cache_sampler_heap_bytes = sampler_cache.sampler_heap_bytes();
            drop(sampler_cache);
            retained_frames.clear_retained_frames();
            let present_handoff = native_vulkan_ffmpeg_vulkan_hwdecode_direct_handoff_snapshot(
                options.playback_frame_count,
                sequence_builder.presented_frame_count,
                threaded_decode,
                release_frame_after_render_fence,
            );
            let execution = NativeVulkanVulkanaliaDecodedImagePresentExecutionEvidence {
                ffmpeg_read_thread_active: true,
                video_decode_worker_active: true,
                present_worker_active: true,
                source_count: 1,
                decode_thread_count: FFMPEG_SINGLE_DECODE_THREAD_COUNT,
                decode_async_exec_depth: 0,
                ffmpeg_retained_avframe_count,
                ffmpeg_retained_avframe_peak_count,
                descriptor_sampler_cache_entry_count,
                descriptor_sampler_cache_peak_entry_count,
                descriptor_sampler_cache_rewrite_count,
                descriptor_sampler_cache_recreate_count,
                descriptor_sampler_cache_resource_heap_bytes,
                descriptor_sampler_cache_sampler_heap_bytes,
            };
            let sequence = sequence_builder
                .finish(present_handoff, execution)
                .ok_or_else(|| {
                    "FFmpeg Vulkan HW present sequence has no rendered frames".to_owned()
                })?;
            Ok((sequence, final_decoder_snapshot))
        },
    );

    if let Some(pipeline) = decoded_image_present_pipeline.take() {
        native_vulkan_vulkanalia_destroy_decoded_image_present_pipeline_resources(
            &context.device,
            pipeline,
        );
    }
    if let Some(frame_resources) = frame_resources.take() {
        native_vulkan_vulkanalia_destroy_decoded_image_present_frame_resources(
            &context.device,
            frame_resources,
        );
    }

    let (sequence, sequence_error) = match sequence_result {
        Ok((sequence, final_decoder_snapshot)) => {
            decoder_snapshot = final_decoder_snapshot;
            (Some(sequence), None)
        }
        Err(err) => (None, Some(err)),
    };
    Ok(NativeVulkanFfmpegVulkanHwVideoPresentSnapshot {
        binding: "ffmpeg-vulkan-hwdecode",
        route: "ffmpeg-avcodec-avvkframe-descriptor-heap-present",
        source: options.source,
        codec: options.codec,
        requested_present_frame_count: options.playback_frame_count,
        device: device_snapshot,
        surface_host: None,
        decoder: decoder_snapshot,
        audio_clock_probe_requested: false,
        audio_output_mode: "clock-only",
        audio_clock: None,
        audio_master_clock_enabled: false,
        audio_master_clock_start_ns: None,
        media_events: media_event_runtime.snapshot(),
        decoded_image_present_sequence_requested: true,
        decoded_image_present_sequence: sequence.clone(),
        decoded_image_present_sequence_error: sequence_error.clone(),
        decoded_image_zero_copy_presented: sequence_error.is_none()
            && sequence
                .as_ref()
                .is_some_and(|sequence| sequence.all_zero_copy_presented),
        software_decode_fallback: false,
        descriptor_heap_only: true,
        zero_copy_scope: "FFmpeg avcodec Vulkan hwaccel outputs AVVkFrame VkImage handles; Gilder waits AVVkFrame timeline semaphores and samples the images through VK_EXT_descriptor_heap without av_hwframe_transfer_data or CPU pixel upload",
        ffmpeg_reference: FFMPEG_VULKAN_DECODE_REFERENCE,
    })
}

include!("ffmpeg_hw_present/scene_present.rs");
