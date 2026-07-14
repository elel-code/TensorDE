#[derive(Debug)]
pub struct NativeVulkanVulkanaliaVideoSessionBindSmokeOptions {
    pub codec: NativeVulkanVideoSessionCodec,
    pub width: u32,
    pub height: u32,
    pub allocate_video_images: bool,
    pub allocate_bitstream_buffer: bool,
    pub create_empty_session_parameters: bool,
    pub create_session_parameters: bool,
    pub h264_parameter_sets: Option<NativeVulkanH264ParameterSetSnapshot>,
    pub h265_parameter_sets: Option<NativeVulkanH265ParameterSetSnapshot>,
    pub av1_sequence_header: Option<NativeVulkanAv1SequenceHeaderSnapshot>,
}

impl Default for NativeVulkanVulkanaliaVideoSessionBindSmokeOptions {
    fn default() -> Self {
        Self {
            codec: NativeVulkanVideoSessionCodec::H265Main8,
            width: 3840,
            height: 2160,
            allocate_video_images: false,
            allocate_bitstream_buffer: false,
            create_empty_session_parameters: false,
            create_session_parameters: false,
            h264_parameter_sets: None,
            h265_parameter_sets: None,
            av1_sequence_header: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaVideoSessionBindSmokeSnapshot {
    pub binding: &'static str,
    pub loader: String,
    pub requested_api_version: String,
    pub requested_codec: NativeVulkanVideoSessionCodec,
    pub requested_extent: (u32, u32),
    pub selected_physical_device_index: usize,
    pub selected_physical_device_name: String,
    pub selected_physical_device_type: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub api_version: String,
    pub driver_version: u32,
    pub selected_queue_family_index: u32,
    pub selected_queue_count: u32,
    pub selected_queue_flags: Vec<&'static str>,
    pub enabled_device_extensions: Vec<&'static str>,
    pub synchronization2_enabled: bool,
    pub dynamic_rendering_enabled: bool,
    pub video_maintenance1_enabled: bool,
    pub video_maintenance2_enabled: bool,
    pub inline_session_parameters_enabled: bool,
    pub video_session_create_inline_session_parameters: bool,
    pub video_session_create_flags_bits: u32,
    pub inline_session_parameter_codecs: Vec<&'static str>,
    pub ffmpeg_submit_model: &'static str,
    pub video_codec_operation: Vec<&'static str>,
    pub profile: &'static str,
    pub format_probe_profile: &'static str,
    pub picture_format: String,
    pub reference_picture_format: String,
    pub target_picture_dpb_supported: bool,
    pub target_picture_sampled_output_supported: bool,
    pub target_resource_plan: NativeVulkanVulkanaliaVideoSessionResourceProbePlan,
    pub capability_flags: Vec<&'static str>,
    pub decode_capability_flags: Vec<&'static str>,
    pub min_bitstream_buffer_offset_alignment: u64,
    pub min_bitstream_buffer_size_alignment: u64,
    pub picture_access_granularity: (u32, u32),
    pub min_coded_extent: (u32, u32),
    pub max_coded_extent: (u32, u32),
    pub requested_extent_supported: bool,
    pub driver_max_dpb_slots: u32,
    pub driver_max_active_reference_pictures: u32,
    pub session_max_dpb_slots: u32,
    pub session_max_active_reference_pictures: u32,
    pub codec_max_level: Option<&'static str>,
    pub codec_max_level_raw: Option<i32>,
    pub std_header_version_name: String,
    pub std_header_version_spec_version: u32,
    pub memory_binding: NativeVulkanVulkanaliaVideoSessionMemoryBindingSmokeSnapshot,
    pub resource_image_requested: bool,
    pub resource_image: Option<NativeVulkanVulkanaliaVideoSessionResourceImageSmokeSnapshot>,
    pub bitstream_buffer_requested: bool,
    pub bitstream_buffer: Option<NativeVulkanVulkanaliaVideoSessionBitstreamBufferSmokeSnapshot>,
    pub session_parameters_requested: bool,
    pub session_parameters: Option<NativeVulkanVulkanaliaVideoSessionParametersSmokeSnapshot>,
}

pub fn probe_native_vulkan_vulkanalia_video_session_bind(
    options: NativeVulkanVulkanaliaVideoSessionBindSmokeOptions,
) -> Result<NativeVulkanVulkanaliaVideoSessionBindSmokeSnapshot, String> {
    let vulkan = native_vulkan_vulkanalia_create_instance()?;
    let result = probe_native_vulkan_vulkanalia_video_session_bind_inner(
        &vulkan.instance,
        vulkan.loader_name,
        options,
    );
    native_vulkan_vulkanalia_destroy_instance(vulkan);
    result
}

fn probe_native_vulkan_vulkanalia_video_session_bind_inner(
    instance: &Instance,
    loader_name: &'static str,
    options: NativeVulkanVulkanaliaVideoSessionBindSmokeOptions,
) -> Result<NativeVulkanVulkanaliaVideoSessionBindSmokeSnapshot, String> {
    let selection =
        native_vulkan_vulkanalia_select_video_decode_physical_device(instance, options.codec)?;
    let requested_extent = vk::Extent2D {
        width: options.width,
        height: options.height,
    };
    let h264_parameter_sets = options.h264_parameter_sets.clone();
    let av1_sequence_header = options.av1_sequence_header.clone();
    let picture_format = native_vulkan_vulkanalia_video_session_effective_picture_format(
        options.codec,
        av1_sequence_header.as_ref(),
    );
    let picture_format_label = format!("{picture_format:?}");
    let video_format_capabilities = native_vulkan_vulkanalia_video_format_probe(
        instance,
        selection.physical_device,
        &selection.device_extensions,
        true,
    );
    let format_probe_profile =
        native_vulkan_vulkanalia_video_session_effective_format_probe_profile(
            options.codec,
            h264_parameter_sets.as_ref(),
            av1_sequence_header.as_ref(),
        )?;
    let target_resource_plan =
        native_vulkan_vulkanalia_video_session_resource_plans_from_format_probe(
            &video_format_capabilities,
        )
        .into_iter()
        .find(|plan| {
            plan.codec == vulkanalia_video_session_codec_name(options.codec)
                && plan.profile == format_probe_profile
        })
        .ok_or_else(|| {
            format!(
                "missing Vulkanalia video format resource plan for {} {}",
                vulkanalia_video_session_codec_name(options.codec),
                format_probe_profile
            )
        })?;
    let target_picture_sampled_output_supported = video_format_probe_includes_format(
        &video_format_capabilities.decode_output_sampled_formats,
        vulkanalia_video_session_codec_name(options.codec),
        format_probe_profile,
        &picture_format_label,
    );
    let target_picture_dpb_supported = video_format_probe_includes_format(
        &video_format_capabilities.dpb_formats,
        vulkanalia_video_session_codec_name(options.codec),
        format_probe_profile,
        &picture_format_label,
    );
    if !target_picture_sampled_output_supported || !target_picture_dpb_supported {
        return Err(format!(
            "{} lacks {picture_format_label} decode sampled-output/DPB support in Vulkanalia probe",
            vulkanalia_video_session_label(options.codec),
        ));
    }

    let video_decode_device = native_vulkan_vulkanalia_create_video_decode_device(
        instance,
        selection.physical_device,
        selection.queue_family_index,
        options.codec,
        &selection.device_extensions,
        vulkanalia_video_session_decode_submit_requested(&options),
    )?;

    let memory_properties =
        unsafe { instance.get_physical_device_memory_properties(selection.physical_device) };
    let result = with_native_vulkan_vulkanalia_video_session_capabilities(
        instance,
        selection.physical_device,
        options.codec,
        h264_parameter_sets.as_ref(),
        av1_sequence_header.as_ref(),
        |profile_info, queried| {
            smoke_bind_vulkanalia_video_session_profile(
                instance,
                &video_decode_device.device,
                video_decode_device.queue,
                &memory_properties,
                &selection,
                loader_name,
                options,
                requested_extent,
                picture_format,
                target_picture_dpb_supported,
                target_picture_sampled_output_supported,
                target_resource_plan,
                video_decode_device.enabled_device_extensions.clone(),
                video_decode_device.feature_selection,
                profile_info,
                queried,
            )
        },
    );

    native_vulkan_vulkanalia_destroy_video_decode_device(video_decode_device);
    result
}

fn vulkanalia_video_session_decode_submit_requested(
    _options: &NativeVulkanVulkanaliaVideoSessionBindSmokeOptions,
) -> bool {
    false
}

fn smoke_bind_vulkanalia_video_session_profile(
    instance: &Instance,
    device: &Device,
    _queue: vk::Queue,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    selection: &NativeVulkanVulkanaliaVideoPhysicalDeviceSelection,
    loader_name: &'static str,
    options: NativeVulkanVulkanaliaVideoSessionBindSmokeOptions,
    requested_extent: vk::Extent2D,
    picture_format: vk::Format,
    target_picture_dpb_supported: bool,
    target_picture_sampled_output_supported: bool,
    target_resource_plan: NativeVulkanVulkanaliaVideoSessionResourceProbePlan,
    enabled_device_extensions: Vec<&'static str>,
    feature_selection: NativeVulkanVulkanaliaVideoDeviceFeatureSelection,
    profile_info: &vk::VideoProfileInfoKHR,
    queried: VulkanaliaVideoSessionCapabilityQuery,
) -> Result<NativeVulkanVulkanaliaVideoSessionBindSmokeSnapshot, String> {
    let capabilities = queried.capabilities;
    let effective_profile_label = native_vulkan_vulkanalia_video_session_effective_profile_label(
        options.codec,
        options.h264_parameter_sets.as_ref(),
        options.av1_sequence_header.as_ref(),
    )?;
    let requested_extent_supported =
        native_vulkan_vulkanalia_video_session_extent_supported(requested_extent, capabilities);
    if !requested_extent_supported {
        return Err(format!(
            "requested Vulkanalia video extent {}x{} is outside ({}, {})..({}, {}) or is not aligned to ({}, {})",
            requested_extent.width,
            requested_extent.height,
            capabilities.min_coded_extent.width,
            capabilities.min_coded_extent.height,
            capabilities.max_coded_extent.width,
            capabilities.max_coded_extent.height,
            capabilities.picture_access_granularity.width,
            capabilities.picture_access_granularity.height,
        ));
    }

    let session_max_dpb_slots =
        native_vulkan_vulkanalia_video_session_max_dpb_slots(capabilities.max_dpb_slots);
    let session_max_active_reference_pictures =
        native_vulkan_vulkanalia_video_session_max_active_reference_pictures(
            capabilities.max_active_reference_pictures,
            session_max_dpb_slots,
        );
    let video_session_create_flags = native_vulkan_vulkanalia_video_session_create_flags(
        feature_selection.inline_session_parameters_enabled,
    );
    let create_info = vk::VideoSessionCreateInfoKHR::builder()
        .flags(video_session_create_flags)
        .queue_family_index(selection.queue_family_index)
        .video_profile(profile_info)
        .picture_format(picture_format)
        .reference_picture_format(picture_format)
        .max_coded_extent(requested_extent)
        .max_dpb_slots(session_max_dpb_slots)
        .max_active_reference_pictures(session_max_active_reference_pictures)
        .std_header_version(&capabilities.std_header_version)
        .build();
    let session = native_vulkan_vulkanalia_create_video_session(device, &create_info)?;
    let mut memory_resources = None;
    let result = (|| -> Result<NativeVulkanVulkanaliaVideoSessionBindSmokeSnapshot, String> {
        let resources = native_vulkan_vulkanalia_bind_video_session_memory_resources(
            device,
            memory_properties,
            session,
        )?;
        let memory_binding = resources.snapshot.clone();
        memory_resources = Some(resources);
        let resource_image = if options.allocate_video_images {
            Some(
                native_vulkan_vulkanalia_smoke_create_video_session_resource_image(
                    instance,
                    device,
                    memory_properties,
                    selection.physical_device,
                    profile_info,
                    requested_extent,
                    session_max_dpb_slots.max(1),
                    picture_format,
                    queried.decode_capability_flags,
                    &[selection.queue_family_index],
                )?,
            )
        } else {
            None
        };
        let bitstream_buffer = if options.allocate_bitstream_buffer {
            Some(
                native_vulkan_vulkanalia_smoke_create_video_session_bitstream_buffer(
                    device,
                    memory_properties,
                    profile_info,
                    native_vulkan_vulkanalia_ffmpeg_decode_bitstream_buffer_size(
                        1,
                        capabilities.min_bitstream_buffer_size_alignment,
                    ),
                    capabilities.min_bitstream_buffer_size_alignment,
                    selection.properties.limits.non_coherent_atom_size,
                    None,
                    false,
                )?,
            )
        } else {
            None
        };
        let session_parameters = if options.create_session_parameters {
            Some(match options.codec {
                NativeVulkanVideoSessionCodec::H264High8 => {
                    let parameter_sets = options.h264_parameter_sets.as_ref().ok_or_else(|| {
                        "Vulkanalia real H.264 session parameters require parsed H.264 parameter sets"
                            .to_owned()
                    })?;
                    native_vulkan_vulkanalia_smoke_create_h264_video_session_parameters(
                        device,
                        session,
                        options.codec,
                        parameter_sets,
                    )
                }
                NativeVulkanVideoSessionCodec::H265Main8
                | NativeVulkanVideoSessionCodec::H265Main10 => {
                    let parameter_sets = options.h265_parameter_sets.as_ref().ok_or_else(|| {
                        "Vulkanalia real H.265 session parameters require parsed H.265 parameter sets"
                            .to_owned()
                    })?;
                    native_vulkan_vulkanalia_smoke_create_h265_video_session_parameters(
                        device,
                        session,
                        options.codec,
                        parameter_sets,
                    )
                }
                NativeVulkanVideoSessionCodec::Av1Main8
                | NativeVulkanVideoSessionCodec::Av1Main10 => {
                    let sequence_header = options.av1_sequence_header.as_ref().ok_or_else(|| {
                        "Vulkanalia real AV1 session parameters require parsed AV1 sequence header"
                            .to_owned()
                    })?;
                    native_vulkan_vulkanalia_smoke_create_av1_video_session_parameters(
                        device,
                        session,
                        options.codec,
                        sequence_header,
                    )
                }
            })
        } else if options.create_empty_session_parameters {
            Some(
                native_vulkan_vulkanalia_smoke_create_empty_video_session_parameters(
                    device,
                    session,
                    options.codec,
                ),
            )
        } else {
            None
        };
        Ok(NativeVulkanVulkanaliaVideoSessionBindSmokeSnapshot {
            binding: "vulkanalia",
            loader: loader_name.to_owned(),
            requested_api_version: Version::V1_4_0.to_string(),
            requested_codec: options.codec,
            requested_extent: (requested_extent.width, requested_extent.height),
            selected_physical_device_index: selection.physical_device_index,
            selected_physical_device_name: selection
                .properties
                .device_name
                .to_string_lossy()
                .into_owned(),
            selected_physical_device_type: format!("{:?}", selection.properties.device_type),
            vendor_id: selection.properties.vendor_id,
            device_id: selection.properties.device_id,
            api_version: Version::from(selection.properties.api_version).to_string(),
            driver_version: selection.properties.driver_version,
            selected_queue_family_index: selection.queue_family_index,
            selected_queue_count: selection.queue_count,
            selected_queue_flags: queue_flag_labels(selection.queue_flags),
            enabled_device_extensions,
            synchronization2_enabled: feature_selection.synchronization2_enabled,
            dynamic_rendering_enabled: feature_selection.dynamic_rendering_enabled,
            video_maintenance1_enabled: feature_selection.video_maintenance1_enabled,
            video_maintenance2_enabled: feature_selection.video_maintenance2_enabled,
            inline_session_parameters_enabled: feature_selection.inline_session_parameters_enabled,
            video_session_create_inline_session_parameters: feature_selection
                .inline_session_parameters_enabled,
            video_session_create_flags_bits: video_session_create_flags.bits(),
            inline_session_parameter_codecs: feature_selection.inline_session_parameter_codecs(),
            ffmpeg_submit_model: "references/ffmpeg/libavutil/vulkan.c: VkSubmitInfo2 + QueueSubmit2",
            video_codec_operation: video_codec_operation_labels(
                vulkanalia_video_session_codec_operation(options.codec),
            ),
            profile: effective_profile_label,
            format_probe_profile:
                native_vulkan_vulkanalia_video_session_effective_format_probe_profile(
                    options.codec,
                    options.h264_parameter_sets.as_ref(),
                    options.av1_sequence_header.as_ref(),
                )?,
            picture_format: format!("{picture_format:?}"),
            reference_picture_format: format!("{picture_format:?}"),
            target_picture_dpb_supported,
            target_picture_sampled_output_supported,
            target_resource_plan,
            capability_flags: video_capability_flag_labels(capabilities.flags),
            decode_capability_flags: video_decode_capability_flag_labels(
                queried.decode_capability_flags,
            ),
            min_bitstream_buffer_offset_alignment: capabilities
                .min_bitstream_buffer_offset_alignment,
            min_bitstream_buffer_size_alignment: capabilities.min_bitstream_buffer_size_alignment,
            picture_access_granularity: (
                capabilities.picture_access_granularity.width,
                capabilities.picture_access_granularity.height,
            ),
            min_coded_extent: (
                capabilities.min_coded_extent.width,
                capabilities.min_coded_extent.height,
            ),
            max_coded_extent: (
                capabilities.max_coded_extent.width,
                capabilities.max_coded_extent.height,
            ),
            requested_extent_supported,
            driver_max_dpb_slots: capabilities.max_dpb_slots,
            driver_max_active_reference_pictures: capabilities.max_active_reference_pictures,
            session_max_dpb_slots,
            session_max_active_reference_pictures,
            codec_max_level: queried.codec_max_level,
            codec_max_level_raw: queried.codec_max_level_raw,
            std_header_version_name: capabilities
                .std_header_version
                .extension_name
                .to_string_lossy()
                .into_owned(),
            std_header_version_spec_version: capabilities.std_header_version.spec_version,
            memory_binding,
            resource_image_requested: options.allocate_video_images,
            resource_image,
            bitstream_buffer_requested: options.allocate_bitstream_buffer,
            bitstream_buffer,
            session_parameters_requested: options.create_empty_session_parameters
                || options.create_session_parameters,
            session_parameters,
        })
    })();

    if let Some(resources) = memory_resources.take() {
        native_vulkan_vulkanalia_destroy_video_session_memory_binding_resources(device, resources);
    }
    native_vulkan_vulkanalia_destroy_video_session(device, session);

    result
}

#[allow(clippy::too_many_arguments)]
pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_record_av1_streaming_decode_into_image(
    device: &Device,
    queue: vk::Queue,
    queue_host_access_lock: Option<&Mutex<()>>,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    queue_family_index: u32,
    profile_info: &vk::VideoProfileInfoKHR,
    extent: vk::Extent2D,
    capabilities: vk::VideoCapabilitiesKHR,
    session: vk::VideoSessionKHR,
    codec: NativeVulkanVideoSessionCodec,
    array_layers: u32,
    exec_ring_depth: u32,
    non_coherent_atom_size: u64,
    input: NativeVulkanVulkanaliaAv1StreamingDecodeInput<'_>,
    image: &super::video_session_images::VulkanaliaVideoSessionResourceImage,
    mut before_output_slot_reuse: Option<NativeVulkanVulkanaliaBeforeOutputSlotReuse<'_>>,
    mut after_frame_submitted: Option<NativeVulkanVulkanaliaAfterFrameSubmitted<'_>>,
    decode_complete_semaphore: vk::Semaphore,
    decode_complete_value: &std::cell::Cell<u64>,
) -> Result<NativeVulkanVulkanaliaAv1CommandSmokeSnapshot, String> {
    if !matches!(
        codec,
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10
    ) {
        return Err("Vulkanalia AV1 streaming decode requires an AV1 codec".into());
    }
    let requested_frame_count = input.requested_frame_count;
    if requested_frame_count == 0 {
        return Err("Vulkanalia AV1 streaming decode requires at least one frame".to_owned());
    }
    let array_layers = array_layers.max(1);
    let exec_ring_depth = exec_ring_depth.max(1);
    let sequence_header = input.sequence_header;
    let inline_session_parameters =
        native_vulkan_vulkanalia_av1_inline_session_parameters(codec, &sequence_header)?;
    let command_buffer = native_vulkan_vulkanalia_create_decode_command_buffers(
        device,
        queue_family_index,
        exec_ring_depth,
    )?;
    let mut command_buffer = Some(command_buffer);
    let mut submit_ring =
        NativeVulkanVulkanaliaStreamingDecodeSubmitRing::new(exec_ring_depth as usize);
    let mut bitstream_buffers =
        NativeVulkanVulkanaliaFfmpegSlicesBufferPool::new(exec_ring_depth as usize);

    let result = (|| -> Result<NativeVulkanVulkanaliaAv1CommandSmokeSnapshot, String> {
        let command_buffer_ref = command_buffer
            .as_ref()
            .expect("Vulkanalia streaming command buffer is alive during AV1 decode");
        let mut initialized_slots = vec![false; array_layers as usize];
        let mut layer_decode_complete_values = vec![0u64; array_layers as usize];
        let mut frame_telemetry = NativeVulkanVulkanaliaDecodeFrameTelemetry::new();
        let mut last_tile_offsets = Vec::new();
        let mut last_tile_sizes = Vec::new();
        let mut av1_reference_infos = Vec::<
            super::video_decode_submit_av1::NativeVulkanVulkanaliaAv1ReferenceInfoPlan,
        >::new();
        let mut command_buffer_recorded = true;
        let mut submitted = true;
        let mut uses_synchronization2 = true;
        let mut uses_submit2 = true;
        let mut ffmpeg_reference = "references/ffmpeg/libavcodec/vulkan_decode.c";
        let mut src_buffer_total_bytes = 0u64;
        let mut displayed_frame_count = 0u32;
        let mut show_existing_frame_count = 0u32;
        let mut hidden_frame_count = 0u32;
        let decode_loop_started_at = Instant::now();
        let mut streaming_decode_timing = NativeVulkanVulkanaliaStreamingDecodeTiming::default();

        while displayed_frame_count < requested_frame_count {
            let mut frame_timing = NativeVulkanVulkanaliaStreamingDecodeFrameTiming::default();
            let stage_started_at = Instant::now();
            let mut frame = (input.next_frame)()?;
            frame_timing.next_frame_micros =
                native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
            let (display_order_key, display_order_key_source) =
                native_vulkan_vulkanalia_av1_display_order_key(
                    &frame.entry,
                    frame.pts_ns,
                    frame.pts_ms,
                    displayed_frame_count,
                );

            if frame.entry.ready_for_display_handoff {
                let sampled_array_layer = frame.entry.displayed_slot.ok_or_else(|| {
                    format!(
                        "Vulkanalia AV1 TU {} display handoff has no displayed DPB slot",
                        frame.entry.temporal_unit_index
                    )
                })?;
                if sampled_array_layer >= array_layers {
                    return Err(format!(
                        "Vulkanalia AV1 display handoff slot {sampled_array_layer} exceeds image layers {array_layers}"
                    ));
                }
                let decode_complete_value_for_frame =
                    layer_decode_complete_values[sampled_array_layer as usize];
                if decode_complete_value_for_frame == 0 {
                    return Err(format!(
                        "Vulkanalia AV1 TU {} show_existing_frame references layer {sampled_array_layer} before any decode completed there",
                        frame.entry.temporal_unit_index
                    ));
                }
                if let Some(after_frame_submitted) = after_frame_submitted.as_deref_mut() {
                    let stage_started_at = Instant::now();
                    after_frame_submitted(
                        displayed_frame_count,
                        sampled_array_layer,
                        frame.pts_ns,
                        frame.duration_ns,
                        frame.pts_ms,
                        frame.duration_ms,
                        display_order_key,
                        display_order_key_source,
                        decode_complete_value_for_frame,
                    )?;
                    frame_timing.after_frame_submitted_micros =
                        native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
                }
                displayed_frame_count = displayed_frame_count.saturating_add(1);
                show_existing_frame_count = show_existing_frame_count.saturating_add(1);
                streaming_decode_timing.push(frame_timing);
                continue;
            }

            let submit_frame = frame.frame.take().ok_or_else(|| {
                format!(
                    "Vulkanalia AV1 TU {} has no decode payload and is not a display handoff",
                    frame.entry.temporal_unit_index
                )
            })?;
            let submit_frame_show_frame = submit_frame.show_frame;
            let reset_control_recorded = submit_frame.frame_type == 0;
            let output_slot = frame.entry.output_slot.ok_or_else(|| {
                format!(
                    "Vulkanalia AV1 TU {} has no planned output slot",
                    frame.entry.temporal_unit_index
                )
            })?;
            if output_slot >= array_layers {
                return Err(format!(
                    "Vulkanalia AV1 streaming planned output slot {output_slot} exceeds image layers {array_layers}"
                ));
            }
            for slot in &frame.entry.decode_reference_slots {
                if let Ok(slot) = u32::try_from(*slot)
                    && slot >= array_layers
                {
                    return Err(format!(
                        "Vulkanalia AV1 streaming reference slot {slot} exceeds image layers {array_layers}"
                    ));
                }
            }

            let submit_slot =
                submit_ring.exec_slot_for_frame(frame_telemetry.submitted_frame_count);
            frame_timing.exec_slot_reuse_wait_micros =
                submit_ring.wait_for_slot_reuse(device, command_buffer_ref, submit_slot)?;
            if let Some(before_output_slot_reuse) = before_output_slot_reuse.as_deref_mut() {
                let stage_started_at = Instant::now();
                before_output_slot_reuse(output_slot)?;
                frame_timing.output_slot_reuse_wait_micros =
                    native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
            }
            let payload_len = frame.access_unit_payload.len() as u64;
            let stage_started_at = Instant::now();
            let bitstream_buffer_ref = bitstream_buffers.buffer_for_payload(
                device,
                memory_properties,
                profile_info,
                submit_slot,
                payload_len,
                capabilities.min_bitstream_buffer_size_alignment,
                non_coherent_atom_size,
            )?;
            frame_timing.bitstream_buffer_micros =
                native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
            let stage_started_at = Instant::now();
            let (src_buffer_offset, src_buffer_range) =
                native_vulkan_vulkanalia_write_ffmpeg_picture_slices_buffer(
                    device,
                    bitstream_buffer_ref,
                    frame.access_unit_payload.bytes(),
                    capabilities.min_bitstream_buffer_size_alignment,
                    non_coherent_atom_size,
                )?;
            frame_timing.payload_write_micros =
                native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
            frame.access_unit_payload.clear();
            src_buffer_total_bytes = src_buffer_total_bytes.saturating_add(payload_len);

            let stage_started_at = Instant::now();
            let plan = native_vulkan_vulkanalia_av1_decode_submit_plan(
                extent,
                codec,
                &frame.entry,
                submit_frame,
                src_buffer_offset,
                src_buffer_range,
                reset_control_recorded,
                &mut av1_reference_infos,
            )?;
            frame_timing.decode_plan_micros =
                native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
            ffmpeg_reference = plan.common.ffmpeg_reference;
            let stage_started_at = Instant::now();
            let image_views =
                native_vulkan_vulkanalia_av1_decode_image_view_bindings(image, &plan)?;
            frame_timing.image_view_bind_micros =
                native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
            let dst_slot = plan.common.dst_picture_resource.base_array_layer as usize;
            let transition_dst_from_undefined = !initialized_slots[dst_slot];
            let decode_command_buffer = command_buffer_ref.command_buffer_at(submit_slot)?;
            let stage_started_at = Instant::now();
            let record_plan = unsafe {
                native_vulkan_vulkanalia_record_av1_decode_command_buffer(
                    device,
                    decode_command_buffer,
                    image.image,
                    &plan,
                    session,
                    inline_session_parameters.inline_info(),
                    bitstream_buffer_ref.buffer,
                    &image_views,
                    submit_ring.reset_command_buffer_before_record(submit_slot)?,
                    transition_dst_from_undefined,
                )
            }?;
            submit_ring.mark_recorded(submit_slot)?;
            frame_timing.record_command_buffer_micros =
                native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
            let decode_complete_value_for_frame = decode_complete_value.get() + 1;
            decode_complete_value.set(decode_complete_value_for_frame);
            let stage_started_at = Instant::now();
            let queue_host_access_guard = if let Some(lock) = queue_host_access_lock {
                Some(lock.lock().map_err(|_| {
                    "Vulkanalia AV1 decode queue host-access lock is poisoned".to_owned()
                })?)
            } else {
                None
            };
            let submit_plan = unsafe {
                native_vulkan_vulkanalia_submit_decode_command_buffer2(
                    device,
                    queue,
                    decode_command_buffer,
                    command_buffer_ref.submit_fence_at(submit_slot)?,
                    false,
                    false,
                    decode_complete_semaphore,
                    decode_complete_value_for_frame,
                )
            }?;
            drop(queue_host_access_guard);
            submit_ring.mark_submitted(submit_slot)?;
            frame_timing.submit_wait_micros =
                native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
            initialized_slots[dst_slot] = true;
            layer_decode_complete_values[dst_slot] = decode_complete_value_for_frame;
            command_buffer_recorded &= record_plan.command_order.contains(&"vkEndCommandBuffer");
            submitted &= submit_plan.command_order.contains(&"queue_submit2");
            uses_synchronization2 &= record_plan.uses_synchronization2;
            uses_submit2 &= submit_plan.uses_submit2;

            if submit_frame_show_frame {
                let sampled_array_layer = frame.entry.displayed_slot.ok_or_else(|| {
                    format!(
                        "Vulkanalia AV1 TU {} is show_frame but has no displayed slot",
                        frame.entry.temporal_unit_index
                    )
                })?;
                if let Some(after_frame_submitted) = after_frame_submitted.as_deref_mut() {
                    let stage_started_at = Instant::now();
                    after_frame_submitted(
                        displayed_frame_count,
                        sampled_array_layer,
                        frame.pts_ns,
                        frame.duration_ns,
                        frame.pts_ms,
                        frame.duration_ms,
                        display_order_key,
                        display_order_key_source,
                        decode_complete_value_for_frame,
                    )?;
                    frame_timing.after_frame_submitted_micros =
                        native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
                }
                displayed_frame_count = displayed_frame_count.saturating_add(1);
            } else {
                hidden_frame_count = hidden_frame_count.saturating_add(1);
            }

            let begin_reference_slot_count = plan.common.begin_reference_slot_count as u32;
            let decode_reference_slot_count = plan.common.decode_reference_slot_count as u32;
            last_tile_offsets = plan.picture.tile_offsets;
            last_tile_sizes = plan.picture.tile_sizes;
            frame_telemetry.push(NativeVulkanVulkanaliaDecodeFrameLastFields {
                src_buffer_offset: plan.common.src_buffer_offset,
                src_buffer_range: plan.common.src_buffer_range,
                dst_base_array_layer: plan.common.dst_picture_resource.base_array_layer,
                setup_slot_index: plan.common.setup_reference_slot.slot_index,
                begin_reference_slot_count,
                decode_reference_slot_count,
                reset_control_recorded,
            });
            streaming_decode_timing.push(frame_timing);
        }
        let last_frame =
            frame_telemetry.last_frame("Vulkanalia AV1 streaming submitted no decode frames")?;
        let final_drain_wait_micros = submit_ring.wait_all_submitted(device, command_buffer_ref)?;
        let streaming_decode_timing = streaming_decode_timing.finish(
            native_vulkan_vulkanalia_elapsed_micros(decode_loop_started_at),
            final_drain_wait_micros,
        );

        Ok(NativeVulkanVulkanaliaAv1CommandSmokeSnapshot {
            requested_frame_count,
            recorded_frame_count: frame_telemetry.submitted_frame_count,
            submitted_frame_count: frame_telemetry.submitted_frame_count,
            displayed_frame_count,
            show_existing_frame_count,
            hidden_frame_count,
            ffmpeg_reference,
            command_buffer_recorded,
            submitted,
            uses_synchronization2,
            uses_submit2,
            uses_inline_session_parameters: true,
            video_session_parameters_handle_used: false,
            session_parameter_strategy: NATIVE_VULKAN_VULKANALIA_INLINE_SESSION_PARAMETER_STRATEGY,
            wait_idle_after_submit: false,
            wait_fence_after_submit: false,
            batch_wait_fence_after_submit: true,
            uses_submit_fence: true,
            submit_sync_model: NATIVE_VULKAN_VULKANALIA_STREAMING_DECODE_SUBMIT_FENCE_SYNC_MODEL,
            submit_command_order:
                native_vulkan_vulkanalia_streaming_decode_submit_fence_command_order(),
            queue_family_index,
            bitstream_buffer_model: "ffmpeg-picture-slices-buffer-pool-exec-owned",
            ffmpeg_slices_buffer_pool_slot_count: bitstream_buffers.slot_count(),
            ffmpeg_slices_buffer_pool_allocated_slot_count: bitstream_buffers
                .allocated_slot_count(),
            ffmpeg_slices_buffer_pool_capacity_bytes: bitstream_buffers.total_capacity_bytes(),
            ffmpeg_slices_buffer_pool_max_slot_bytes: bitstream_buffers.max_slot_capacity_bytes(),
            input_payload_model: "bounded-streaming-packet-queue-per-frame-upload",
            src_buffer_total_bytes,
            streaming_decode_timing,
            retained_frame_telemetry_limit:
                NATIVE_VULKAN_VULKANALIA_DECODE_FRAME_TELEMETRY_RETAINED_FRAMES,
            retained_frame_telemetry_count: frame_telemetry.retained_frame_count(),
            frame_telemetry_retention_model:
                NATIVE_VULKAN_VULKANALIA_DECODE_FRAME_TELEMETRY_RETENTION_MODEL,
            max_src_buffer_range: frame_telemetry.max_src_buffer_range,
            first_frame_reset_control_recorded: frame_telemetry
                .first_frame_reset_control_recorded
                .unwrap_or(false),
            reset_control_recorded_frame_count: frame_telemetry.reset_control_recorded_frame_count,
            p_frame_count: frame_telemetry.p_frame_count,
            b_frame_count: frame_telemetry.b_frame_count,
            max_begin_reference_slot_count: frame_telemetry.max_begin_reference_slot_count,
            max_decode_reference_slot_count: frame_telemetry.max_decode_reference_slot_count,
            src_buffer_offset: last_frame.src_buffer_offset,
            src_buffer_range: last_frame.src_buffer_range,
            dst_base_array_layer: last_frame.dst_base_array_layer,
            setup_slot_index: last_frame.setup_slot_index,
            begin_reference_slot_count: last_frame.begin_reference_slot_count,
            decode_reference_slot_count: last_frame.decode_reference_slot_count,
            reset_control_recorded: last_frame.reset_control_recorded,
            tile_count: last_tile_offsets.len() as u32,
            tile_offsets: last_tile_offsets,
            tile_sizes: last_tile_sizes,
            frames: Vec::new(),
        })
    })();

    if result.is_err()
        && let Some(command_buffer_ref) = command_buffer.as_ref()
    {
        let _ = submit_ring.wait_all_submitted(device, command_buffer_ref);
    }
    bitstream_buffers.destroy_all(device);
    if let Some(command_buffer) = command_buffer.take() {
        native_vulkan_vulkanalia_destroy_decode_command_buffer(device, command_buffer);
    }
    result
}
