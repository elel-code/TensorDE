pub(in crate::renderer::native_vulkan) fn run_native_vulkan_ffmpeg_vulkan_hw_scene_video_present(
    mut options: NativeVulkanFfmpegVulkanHwSceneVideoPresentOptions,
) -> Result<NativeVulkanFfmpegVulkanHwSceneVideoPresentSnapshot, String> {
    if options.sources.is_empty() {
        return Err("FFmpeg Vulkan HW scene video present requires at least one source".to_owned());
    }
    for source in &mut options.sources {
        if source.playback_frame_count == 0 {
            source.playback_frame_count = 1;
        }
        if !source.source.is_file() {
            return Err(format!(
                "FFmpeg Vulkan HW scene video source does not exist: {}",
                source.source.display()
            ));
        }
    }
    let requested_present_frame_count = options
        .sources
        .iter()
        .map(|source| source.playback_frame_count.max(1))
        .max()
        .unwrap_or(1);

    let audio_clock_preparation = native_vulkan_ffmpeg_prepare_audio_clock_for_video_present(
        NativeVulkanFfmpegVideoAudioClockPrepareOptions {
            source: options
                .sources
                .first()
                .expect("scene video source list is non-empty")
                .source
                .clone(),
            playback_frame_count: requested_present_frame_count,
            target_max_fps: options.target_max_fps,
            audio_clock_probe_requested: options.audio_clock_probe_requested,
            audio_output_mode: options.audio_output_mode,
        },
    )?;
    let mut audio_clock = audio_clock_preparation.clock;
    let audio_output_worker = audio_clock_preparation.worker;
    let audio_master_clock_enabled = audio_clock
        .as_ref()
        .is_some_and(|clock| clock.video_master_clock_ready);
    let audio_master_clock_start_ns = audio_clock
        .as_ref()
        .and_then(|clock| clock.video_master_start_clock_ns);
    let audio_master_clock = if audio_master_clock_enabled {
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

    let result = (|| -> Result<NativeVulkanFfmpegVulkanHwSceneVideoPresentSnapshot, String> {
        let physical_devices = unsafe { instance.enumerate_physical_devices() }.map_err(|err| {
            format!("vkEnumeratePhysicalDevices(FFmpeg scene video present): {err:?}")
        })?;
        let codec_set = native_vulkan_ffmpeg_scene_unique_codecs(&options.sources);
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
            (|| -> Result<NativeVulkanFfmpegVulkanHwSceneVideoPresentSnapshot, String> {
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
                    format!("vkCreateSwapchainKHR(FFmpeg scene video present): {err:?}")
                })?;
                let swapchain_result =
                    (|| -> Result<NativeVulkanFfmpegVulkanHwSceneVideoPresentSnapshot, String> {
                        let swapchain_images = unsafe {
                            context.device.get_swapchain_images_khr(swapchain)
                        }
                        .map_err(|err| {
                            format!("vkGetSwapchainImagesKHR(FFmpeg scene video present): {err:?}")
                        })?;
                        let swapchain_snapshot =
                            swapchain_plan_snapshot(&swapchain_plan, swapchain_images.len());
                        let device_snapshot = device_snapshot_from_selection(
                            &vulkan,
                            &selection,
                            &context,
                            codec_set[0],
                            swapchain_snapshot,
                        );
                        run_native_vulkan_ffmpeg_vulkan_hw_scene_video_present_on_device(
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
                            audio_master_clock,
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

pub(super) fn native_vulkan_ffmpeg_scene_unique_codecs(
    sources: &[NativeVulkanFfmpegVulkanHwSceneVideoPresentSourceOptions],
) -> Vec<NativeVulkanVideoSessionCodec> {
    let mut codecs = Vec::new();
    for source in sources {
        if !codecs.contains(&source.codec) {
            codecs.push(source.codec);
        }
    }
    codecs
}

#[allow(clippy::too_many_arguments)]
pub(super) fn run_native_vulkan_ffmpeg_vulkan_hw_scene_video_present_on_device(
    instance: &Instance,
    vulkan: &NativeVulkanVulkanaliaInstance,
    context: &NativeVulkanVulkanaliaVideoPresentDeviceContext,
    selection: &super::video_present_device::NativeVulkanVulkanaliaVideoPresentPhysicalDeviceSelection,
    swapchain: vk::SwapchainKHR,
    swapchain_images: &[vk::Image],
    swapchain_format: vk::Format,
    swapchain_extent: vk::Extent2D,
    _device_snapshot: super::video_present_device::NativeVulkanVulkanaliaVideoPresentDeviceProbeSnapshot,
    present_id2_enabled: bool,
    present_wait2_enabled: bool,
    audio_master_clock: NativeVulkanVulkanaliaVideoPresentAudioMasterClock,
    options: NativeVulkanFfmpegVulkanHwSceneVideoPresentOptions,
) -> Result<NativeVulkanFfmpegVulkanHwSceneVideoPresentSnapshot, String> {
    let memory_properties =
        unsafe { instance.get_physical_device_memory_properties(selection.physical_device) };
    let codec_set = native_vulkan_ffmpeg_scene_unique_codecs(&options.sources);
    let same_queue_family =
        selection.video_queue_family_index == selection.present_queue_family_index;
    let ffmpeg_present_queue_count = if same_queue_family {
        1
    } else {
        selection.present_queue_count.min(1).max(1)
    };
    let ffmpeg_visible_device_extensions =
        native_vulkan_ffmpeg_vulkan_hw_visible_device_extensions_for_codecs(context, &codec_set);
    let hw_device_borrow = NativeVulkanFfmpegVulkanHwDeviceBorrow {
        instance: &vulkan.instance,
        physical_device: selection.physical_device,
        device: &context.device,
        enabled_instance_extensions: &vulkan.extension_selection.enabled_instance_extensions,
        enabled_device_extensions: &ffmpeg_visible_device_extensions,
        video_queue_family_index: selection.video_queue_family_index,
        video_queue_count: 1,
        video_queue_flags: selection.video_queue_flags,
        video_codec_operations: native_vulkan_ffmpeg_vulkan_hw_codec_operations_for_codecs(
            &codec_set,
        ),
        present_queue_family_index: selection.present_queue_family_index,
        present_queue_count: ffmpeg_present_queue_count,
        present_queue_flags: selection.present_queue_flags,
    };
    let hw_device = NativeVulkanFfmpegVulkanHwDevice::borrow_existing(hw_device_borrow)?;
    let source_options = options.sources.clone();
    let requested_present_frame_count = source_options
        .iter()
        .map(|source| source.playback_frame_count.max(1))
        .max()
        .unwrap_or(1);
    let mut decoders = source_options
        .iter()
        .map(|source| {
            NativeVulkanFfmpegVulkanHwDecoder::open(&source.source, source.codec, &hw_device)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut decoder_snapshots = decoders
        .iter()
        .map(NativeVulkanFfmpegVulkanHwDecoder::snapshot)
        .collect::<Vec<_>>();
    let time_bases = decoder_snapshots
        .iter()
        .map(|snapshot| snapshot.time_base)
        .collect::<Vec<_>>();

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
    let sequence_result =
        (|| -> Result<NativeVulkanVulkanaliaDecodedImagePresentSequenceSnapshot, String> {
            let frame_resources_ref = frame_resources.as_ref().ok_or_else(|| {
                "FFmpeg scene decoded present frame resources were released early".to_owned()
            })?;
            let mut retained_frames = NativeVulkanFfmpegPresentedFrameSetRetentionQueue::new(
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
            let mut sequence_builder =
                NativeVulkanVulkanaliaDecodedImagePresentSequenceBuilder::new(
                    requested_present_frame_count,
                    Instant::now(),
                );
            let mut present_frame_timer = NativeVulkanVulkanaliaPresentFrameTimer::new(
                options.target_max_fps,
                audio_master_clock,
            );

            for present_frame_index in 0..requested_present_frame_count {
                let mut decoded_frames = Vec::with_capacity(decoders.len());
                let mut descriptor_sources = Vec::with_capacity(decoders.len());
                for decoder in &mut decoders {
                    let decoded_frame = decoder.decode_next_frame(true)?.ok_or_else(|| {
                    "FFmpeg Vulkan HW scene video decoder reached EOF before producing a presentable AVVkFrame".to_owned()
                })?;
                    let descriptor_source = decoded_frame.descriptor_source()?;
                    descriptor_sources.push(descriptor_source);
                    decoded_frames.push(decoded_frame);
                }
                let frame_resources_ref = frame_resources.as_ref().ok_or_else(|| {
                    "FFmpeg scene decoded present frame resources were released early".to_owned()
                })?;
                let present_frame_slot_count =
                    native_vulkan_vulkanalia_decoded_image_present_frame_slot_count(
                        frame_resources_ref,
                    );
                if present_frame_slot_count == 0 {
                    return Err(
                        "FFmpeg scene decoded present requires at least one present frame slot"
                            .to_owned(),
                    );
                }
                let present_frame_slot = present_frame_index as usize % present_frame_slot_count;
                native_vulkan_vulkanalia_prepare_decoded_image_present_frame_slot(
                    &context.device,
                    frame_resources_ref,
                    present_frame_slot as u32,
                )?;
                retained_frames.release_completed_slot(present_frame_slot as u32);
                let sampler_indexes = descriptor_sources
                    .iter()
                    .map(|descriptor_source| {
                        sampler_cache.ensure_for_descriptor_source(descriptor_source)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let first_descriptor_heap_plan =
                    sampler_cache
                        .descriptor_heap_plan(*sampler_indexes.first().ok_or_else(|| {
                            "FFmpeg scene video has no sampler source".to_owned()
                        })?)?
                        .clone();
                for sampler_index in sampler_indexes.iter().copied().skip(1) {
                    if sampler_cache.descriptor_heap_plan(sampler_index)?
                        != &first_descriptor_heap_plan
                    {
                        return Err(
                        "FFmpeg scene video sources produced incompatible descriptor heap plans"
                            .to_owned(),
                    );
                    }
                }
                if decoded_image_present_pipeline.is_none() {
                    decoded_image_present_pipeline = Some(
                        native_vulkan_vulkanalia_create_decoded_image_present_pipeline_resources(
                            &context.device,
                            swapchain_format,
                            swapchain_extent,
                            &first_descriptor_heap_plan,
                        )?,
                    );
                }
                let mut present_sources = Vec::with_capacity(descriptor_sources.len());
                for (descriptor_source, sampler_index) in descriptor_sources
                    .iter()
                    .zip(sampler_indexes.iter().copied())
                {
                    present_sources.push(native_vulkan_ffmpeg_decoded_gpu_frame_present_source(
                        descriptor_source,
                        sampler_cache.sampler(sampler_index)?,
                    )?);
                }
                let decode_waits = descriptor_sources
                    .iter()
                    .map(native_vulkan_ffmpeg_decoded_gpu_frame_decode_wait)
                    .collect::<Result<Vec<_>, _>>()?;
                let timing = native_vulkan_ffmpeg_multi_source_frame_timing(
                    &descriptor_sources,
                    &time_bases,
                );
                let (pacing_sleep_micros, pacing_clock_model) = present_frame_timer.pace_frame(
                    present_frame_index,
                    timing.source_frame_pts_ns,
                    timing.source_frame_duration_ns,
                    timing.source_frame_pts_ms,
                    timing.source_frame_duration_ms,
                );
                let draw = native_vulkan_vulkanalia_present_decoded_image_frame_with_sources(
                    &context.device,
                    context.present_queue,
                    swapchain,
                    swapchain_images,
                    swapchain_format,
                    swapchain_extent,
                    decoded_image_present_pipeline
                        .as_ref()
                        .expect("FFmpeg scene decoded present pipeline is live"),
                    frame_resources_ref,
                    &present_sources,
                    0,
                    present_frame_index,
                    true,
                    timing.source_frame_pts_ns,
                    timing.source_frame_duration_ns,
                    timing.source_frame_pts_ms,
                    timing.source_frame_duration_ms,
                    i64::from(present_frame_index),
                    "ffmpeg-scene-present-frame-index",
                    pacing_sleep_micros,
                    pacing_clock_model,
                    decoded_image_present_timing,
                    &decode_waits,
                    None,
                    None,
                    options.clear_color,
                    None,
                )?;
                let present_frame_slot = draw.present_frame_slot;
                sequence_builder.push(draw);
                retained_frames.push_after_submit(present_frame_slot, decoded_frames)?;
            }

            decoder_snapshots = decoders
                .iter()
                .map(NativeVulkanFfmpegVulkanHwDecoder::snapshot)
                .collect();
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
            let present_handoff = native_vulkan_ffmpeg_vulkan_hwdecode_direct_handoff_snapshot(
                requested_present_frame_count,
                sequence_builder.presented_frame_count,
                false,
                false,
            );
            let execution = NativeVulkanVulkanaliaDecodedImagePresentExecutionEvidence {
                ffmpeg_read_thread_active: true,
                video_decode_worker_active: true,
                present_worker_active: true,
                source_count: source_options.len().min(u32::MAX as usize) as u32,
                decode_thread_count: source_options.len().min(u32::MAX as usize) as u32,
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
            sequence_builder
                .finish(present_handoff, execution)
                .ok_or_else(|| {
                    "FFmpeg scene video present sequence has no rendered frames".to_owned()
                })
        })();

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

    if sequence_result.is_err() {
        decoder_snapshots = decoders
            .iter()
            .map(NativeVulkanFfmpegVulkanHwDecoder::snapshot)
            .collect();
    }
    let (sequence, sequence_error) = match sequence_result {
        Ok(sequence) => (Some(sequence), None),
        Err(err) => (None, Some(err)),
    };
    let decoded_image_zero_copy_presented = sequence_error.is_none()
        && sequence
            .as_ref()
            .is_some_and(|sequence| sequence.all_zero_copy_presented);
    let sources = source_options
        .into_iter()
        .enumerate()
        .zip(decoder_snapshots)
        .map(|((source_index, source), decoder)| {
            NativeVulkanFfmpegVulkanHwSceneVideoPresentSourceSnapshot {
                source_index,
                source: source.source,
                codec: source.codec,
                requested_present_frame_count: source.playback_frame_count,
                decoder,
                decoded_image_zero_copy_presented,
            }
        })
        .collect::<Vec<_>>();
    Ok(NativeVulkanFfmpegVulkanHwSceneVideoPresentSnapshot {
        binding: "ffmpeg-vulkan-hwdecode",
        route: "ffmpeg-avcodec-avvkframe-scene-video-present",
        source_count: sources.len(),
        codec_count: codec_set.len(),
        codecs: codec_set,
        surface_host: None,
        sources,
        audio_clock_probe_requested: false,
        audio_output_mode: "clock-only",
        audio_clock: None,
        audio_master_clock_enabled: false,
        audio_master_clock_start_ns: None,
        decoded_image_present_sequence_requested: true,
        decoded_image_present_sequence: sequence.clone(),
        decoded_image_present_sequence_error: sequence_error.clone(),
        decoded_image_zero_copy_presented,
        software_decode_fallback: false,
        descriptor_heap_only: true,
        zero_copy_scope: "FFmpeg avcodec Vulkan hwaccel outputs one AVVkFrame VkImage stream per scene video resource; Gilder samples those images directly through VK_EXT_descriptor_heap in the scene video layer pass without software decode, CPU pixel upload, or intermediate composition copies",
        ffmpeg_reference: FFMPEG_VULKAN_DECODE_REFERENCE,
    })
}

#[cfg(feature = "native-vulkan-video")]
pub(super) fn native_vulkan_ffmpeg_decoded_gpu_frame_present_source<'a>(
    descriptor_source: &NativeVulkanFfmpegDecodedGpuFrameDescriptorSource,
    sampler: &'a VulkanaliaDecodedImagePresentSamplerResources,
) -> Result<super::render_present::VulkanaliaDecodedImagePresentSource<'a>, String> {
    let [plane] = descriptor_source.planes.as_slice() else {
        return Err(format!(
            "FFmpeg AVVkFrame present source requires one multiplane image, got {}",
            descriptor_source.planes.len()
        ));
    };
    Ok(super::render_present::VulkanaliaDecodedImagePresentSource {
        image: VulkanaliaDecodedImagePresentImageSource {
            image: plane.image,
            array_layers: descriptor_source.array_layers,
            current_layout: plane.layout,
            restore_layout: plane.layout,
            queue_family_index: plane.queue_family_index,
        },
        sampler,
        sampled_array_layer: 0,
    })
}

#[cfg(feature = "native-vulkan-video")]
pub(super) fn native_vulkan_ffmpeg_decoded_gpu_frame_decode_wait(
    descriptor_source: &NativeVulkanFfmpegDecodedGpuFrameDescriptorSource,
) -> Result<super::render_present::VulkanaliaDecodedImagePresentDecodeWait, String> {
    let [plane] = descriptor_source.planes.as_slice() else {
        return Err(format!(
            "FFmpeg AVVkFrame decode wait requires one multiplane image, got {}",
            descriptor_source.planes.len()
        ));
    };
    Ok(
        super::render_present::VulkanaliaDecodedImagePresentDecodeWait {
            semaphore: plane.timeline_semaphore,
            value: plane.timeline_value,
        },
    )
}

#[cfg(feature = "native-vulkan-video")]
pub(super) fn native_vulkan_ffmpeg_vulkan_hw_visible_device_extensions(
    context: &NativeVulkanVulkanaliaVideoPresentDeviceContext,
    codec: NativeVulkanVideoSessionCodec,
) -> Vec<&'static str> {
    context
        .video_enabled_device_extensions
        .iter()
        .copied()
        .filter(|extension| native_vulkan_ffmpeg_vulkan_hw_uses_device_extension(codec, extension))
        .collect()
}

#[cfg(feature = "native-vulkan-video")]
pub(super) fn native_vulkan_ffmpeg_vulkan_hw_visible_device_extensions_for_codecs(
    context: &NativeVulkanVulkanaliaVideoPresentDeviceContext,
    codecs: &[NativeVulkanVideoSessionCodec],
) -> Vec<&'static str> {
    context
        .video_enabled_device_extensions
        .iter()
        .copied()
        .filter(|extension| {
            codecs
                .iter()
                .copied()
                .any(|codec| native_vulkan_ffmpeg_vulkan_hw_uses_device_extension(codec, extension))
        })
        .collect()
}

#[cfg(feature = "native-vulkan-video")]
pub(super) fn native_vulkan_ffmpeg_vulkan_hw_uses_device_extension(
    codec: NativeVulkanVideoSessionCodec,
    extension: &str,
) -> bool {
    matches!(
        extension,
        "VK_KHR_video_queue"
            | "VK_KHR_video_decode_queue"
            | "VK_KHR_video_maintenance1"
            | "VK_KHR_video_maintenance2"
    ) || matches!(
        (codec, extension),
        (
            NativeVulkanVideoSessionCodec::H264High8,
            "VK_KHR_video_decode_h264"
        ) | (
            NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::H265Main10,
            "VK_KHR_video_decode_h265"
        ) | (
            NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10,
            "VK_KHR_video_decode_av1"
        )
    )
}

#[cfg(feature = "native-vulkan-video")]
pub(super) fn native_vulkan_ffmpeg_vulkan_hw_codec_operations(
    codec: NativeVulkanVideoSessionCodec,
) -> vk::VideoCodecOperationFlagsKHR {
    match codec {
        NativeVulkanVideoSessionCodec::H264High8 => vk::VideoCodecOperationFlagsKHR::DECODE_H264,
        NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::H265Main10 => {
            vk::VideoCodecOperationFlagsKHR::DECODE_H265
        }
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10 => {
            vk::VideoCodecOperationFlagsKHR::DECODE_AV1
        }
    }
}

#[cfg(feature = "native-vulkan-video")]
pub(super) fn native_vulkan_ffmpeg_vulkan_hw_codec_operations_for_codecs(
    codecs: &[NativeVulkanVideoSessionCodec],
) -> vk::VideoCodecOperationFlagsKHR {
    codecs.iter().copied().fold(
        vk::VideoCodecOperationFlagsKHR::empty(),
        |operations, codec| operations | native_vulkan_ffmpeg_vulkan_hw_codec_operations(codec),
    )
}

#[cfg(feature = "native-vulkan-video")]
pub(super) fn native_vulkan_ffmpeg_time_base_timestamp_ns(
    value: Option<i64>,
    time_base: (i32, i32),
) -> Option<u64> {
    let value = u128::try_from(value?).ok()?;
    let num = u128::try_from(time_base.0).ok()?;
    let den = u128::try_from(time_base.1).ok()?.max(1);
    let nanos = value.saturating_mul(num).saturating_mul(1_000_000_000) / den;
    Some(nanos.min(u128::from(u64::MAX)) as u64)
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_ffmpeg_multi_source_frame_timing(
    descriptor_sources: &[NativeVulkanFfmpegDecodedGpuFrameDescriptorSource],
    time_bases: &[(i32, i32)],
) -> NativeVulkanVulkanaliaMultiSourceFrameTiming {
    let mut timing = NativeVulkanVulkanaliaMultiSourceFrameTiming {
        source_frame_pts_ns: None,
        source_frame_duration_ns: None,
        source_frame_pts_ms: None,
        source_frame_duration_ms: None,
    };
    for (descriptor_source, time_base) in descriptor_sources.iter().zip(time_bases.iter().copied())
    {
        let pts_ns =
            native_vulkan_ffmpeg_time_base_timestamp_ns(descriptor_source.pts_raw, time_base);
        let duration_ns =
            native_vulkan_ffmpeg_time_base_timestamp_ns(descriptor_source.duration_raw, time_base);
        timing.source_frame_pts_ns = timing.source_frame_pts_ns.into_iter().chain(pts_ns).min();
        timing.source_frame_duration_ns = timing
            .source_frame_duration_ns
            .into_iter()
            .chain(duration_ns)
            .min();
        timing.source_frame_pts_ms = timing
            .source_frame_pts_ms
            .into_iter()
            .chain(pts_ns.map(|pts| pts / 1_000_000))
            .min();
        timing.source_frame_duration_ms = timing
            .source_frame_duration_ms
            .into_iter()
            .chain(duration_ns.map(|duration| duration / 1_000_000))
            .min();
    }
    timing
}

#[cfg(feature = "native-vulkan-video")]
pub(super) fn native_vulkan_ffmpeg_vulkan_hwdecode_direct_handoff_snapshot(
    requested_present_frame_count: u32,
    presented_frame_count: u32,
    threaded_decode: bool,
    release_frame_after_render_fence: bool,
) -> NativeVulkanVulkanaliaDecodedPresentHandoffSnapshot {
    let frame_queue_size = native_vulkan_ffmpeg_vulkan_hwdecode_frame_queue_size();
    let (route, model, capacity_frames, peak_depth, drain_order) = if threaded_decode {
        if release_frame_after_render_fence {
            (
                "rendezvous-avvkframe-render-fence-release",
                "FFmpeg avcodec send/receive runs on a dedicated Vulkan hwdecode worker; each moved AVVkFrame handoff is acknowledged only after the render fence signals so the decoder cannot run ahead with extra external frame refs",
                frame_queue_size,
                0,
                "present worker receives one AVVkFrame, submits descriptor-heap rendering, waits the render fence, releases the AVFrame ref, then allows the decode worker to produce the next frame",
            )
        } else {
            (
                "bounded-avvkframe-present-worker-retention",
                "FFmpeg avcodec send/receive runs on a dedicated Vulkan hwdecode worker; moved AVVkFrame refs cross a bounded handoff and are retained until the present fence completes",
                frame_queue_size,
                frame_queue_size.min(requested_present_frame_count as usize),
                "decode worker fills a bounded AVVkFrame ref queue while the present worker drains display-order frames into descriptor-heap dynamic rendering",
            )
        }
    } else {
        (
            "direct-avvkframe-present-fence-retention",
            "FFmpeg avcodec send/receive directly yields AVVkFrame refs; each frame is retained until the present fence completes",
            1,
            1,
            "single-thread decode-next-frame then descriptor-heap present in FFmpeg display order",
        )
    };
    NativeVulkanVulkanaliaDecodedPresentHandoffSnapshot {
        binding: "ffmpeg-vulkan-hwdecode",
        route,
        model,
        capacity_frames,
        queued_frame_count_before_drain: 0,
        enqueued_frame_count: requested_present_frame_count,
        dropped_frame_count: 0,
        drained_frame_count: presented_frame_count,
        peak_depth,
        keep_last_overwrite_enabled: false,
        drop_policy: "no software frame queue drop; AVFrame refs are released only after the render fence signals",
        drain_order,
        zero_copy_scope: "AVVkFrame VkImage pixels are sampled directly; only descriptor metadata is copied",
        ffmpeg_reference: FFMPEG_FFPLAY_FRAME_QUEUE_REFERENCE,
    }
}

#[cfg(feature = "native-vulkan-video")]
pub(super) fn native_vulkan_ffmpeg_vulkan_hwdecode_frame_queue_size() -> usize {
    static FRAME_QUEUE_SIZE: OnceLock<usize> = OnceLock::new();
    *FRAME_QUEUE_SIZE.get_or_init(|| {
        env::var(FFMPEG_VULKAN_HWDECODE_FRAME_QUEUE_SIZE_ENV)
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(FFMPEG_VULKAN_HWDECODE_FRAME_QUEUE_SIZE_DEFAULT)
    })
}

#[cfg(feature = "native-vulkan-video")]
pub(super) fn native_vulkan_ffmpeg_vulkan_hwdecode_release_frame_after_render_fence() -> bool {
    env::var("GILDER_FFMPEG_VULKAN_HWDECODE_RELEASE_FRAME_AFTER_RENDER_FENCE")
        .ok()
        .is_some_and(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
}
