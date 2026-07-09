
#[allow(clippy::too_many_arguments)]
pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_record_h265_streaming_decode_into_image(
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
    input: NativeVulkanVulkanaliaH265StreamingDecodeInput<'_>,
    image: &super::video_session_images::VulkanaliaVideoSessionResourceImage,
    mut before_output_slot_reuse: Option<NativeVulkanVulkanaliaBeforeOutputSlotReuse<'_>>,
    mut after_frame_submitted: Option<NativeVulkanVulkanaliaAfterFrameSubmitted<'_>>,
    decode_complete_semaphore: vk::Semaphore,
    decode_complete_value: &std::cell::Cell<u64>,
) -> Result<NativeVulkanVulkanaliaH265ReadyPrefixCommandSmokeSnapshot, String> {
    if !matches!(
        codec,
        NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::H265Main10
    ) {
        return Err("Vulkanalia H.265 streaming decode requires an H.265 codec".into());
    }
    let requested_frame_count = input.requested_frame_count;
    if requested_frame_count == 0 {
        return Err("Vulkanalia H.265 streaming decode requires at least one frame".to_owned());
    }
    let array_layers = array_layers.max(1);
    let exec_ring_depth = exec_ring_depth.max(1);
    let parameter_sets = input.parameter_sets;
    let inline_session_parameters =
        native_vulkan_vulkanalia_h265_inline_session_parameters(codec, &parameter_sets)?;
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

    let result =
        (|| -> Result<NativeVulkanVulkanaliaH265ReadyPrefixCommandSmokeSnapshot, String> {
            let parameter_ids =
                NativeVulkanVulkanaliaH265ParameterIds::from_parameter_sets(&parameter_sets)?;
            let command_buffer_ref = command_buffer
                .as_ref()
                .expect("Vulkanalia streaming command buffer is alive during decode");
            let mut initialized_slots = vec![false; array_layers as usize];
            let mut frame_telemetry = NativeVulkanVulkanaliaDecodeFrameTelemetry::new();
            let mut last_slice_segment_offsets = Vec::new();
            let mut h265_reference_infos = Vec::<
                super::video_decode_submit_h265::NativeVulkanVulkanaliaH265ReferenceInfoPlan,
            >::new();
            let mut command_buffer_recorded = true;
            let mut submitted = true;
            let mut uses_synchronization2 = true;
            let mut uses_submit2 = true;
            let mut ffmpeg_reference = "references/ffmpeg/libavcodec/vulkan_decode.c";
            let mut src_buffer_total_bytes = 0u64;
            let decode_loop_started_at = Instant::now();
            let mut streaming_decode_timing =
                NativeVulkanVulkanaliaStreamingDecodeTiming::default();
            let mut display_handoff = NativeVulkanVulkanaliaDisplayOrderHandoff::fixed(
                native_vulkan_vulkanalia_h265_max_output_reorder_pics(&parameter_sets),
            );

            for frame_index in 0..requested_frame_count {
                let mut frame_timing = NativeVulkanVulkanaliaStreamingDecodeFrameTiming::default();
                let stage_started_at = Instant::now();
                let mut frame = (input.next_frame)()?;
                frame_timing.next_frame_micros =
                    native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
                let submit_slot = submit_ring.exec_slot_for_frame(frame_index);
                frame_timing.exec_slot_reuse_wait_micros =
                    submit_ring.wait_for_slot_reuse(device, command_buffer_ref, submit_slot)?;
                if frame.entry.planned_output_slot >= array_layers {
                    return Err(format!(
                        "Vulkanalia H.265 streaming planned output slot {} exceeds image layers {array_layers}",
                        frame.entry.planned_output_slot
                    ));
                }
                for reference in &frame.entry.references {
                    if let Some(dpb_slot) = reference.dpb_slot
                        && dpb_slot >= array_layers
                    {
                        return Err(format!(
                            "Vulkanalia H.265 streaming reference slot {dpb_slot} exceeds image layers {array_layers}"
                        ));
                    }
                }
                if let Some(before_output_slot_reuse) = before_output_slot_reuse.as_deref_mut() {
                    let stage_started_at = Instant::now();
                    before_output_slot_reuse(frame.entry.planned_output_slot)?;
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

                let reset_control_recorded = frame.first_slice.idr || frame.first_slice.irap;
                let stage_started_at = Instant::now();
                let slice_segment_offsets = [frame.slice_segment_offset];
                let plan = native_vulkan_vulkanalia_h265_ready_prefix_decode_submit_plan(
                    extent,
                    parameter_ids,
                    &frame.entry,
                    &frame.first_slice,
                    src_buffer_offset,
                    src_buffer_range,
                    &slice_segment_offsets,
                    reset_control_recorded,
                    &mut h265_reference_infos,
                )?;
                frame_timing.decode_plan_micros =
                    native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
                ffmpeg_reference = plan.common.ffmpeg_reference;
                let stage_started_at = Instant::now();
                let image_views =
                    native_vulkan_vulkanalia_h265_decode_image_view_bindings(image, &plan)?;
                frame_timing.image_view_bind_micros =
                    native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
                let dst_slot = plan.common.dst_picture_resource.base_array_layer as usize;
                let transition_dst_from_undefined = !initialized_slots[dst_slot];
                let decode_command_buffer = command_buffer_ref.command_buffer_at(submit_slot)?;
                let stage_started_at = Instant::now();
                let record_plan = unsafe {
                    native_vulkan_vulkanalia_record_h265_decode_command_buffer(
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
                        "Vulkanalia H.265 decode queue host-access lock is poisoned".to_owned()
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
                command_buffer_recorded &=
                    record_plan.command_order.contains(&"vkEndCommandBuffer");
                submitted &= submit_plan.command_order.contains(&"queue_submit2");
                uses_synchronization2 &= record_plan.uses_synchronization2;
                uses_submit2 &= submit_plan.uses_submit2;
                let (display_order_key, display_order_key_source) =
                    native_vulkan_vulkanalia_h265_display_order_key(
                        &frame.entry,
                        frame.pts_ns,
                        frame_index,
                    );

                if let Some(after_frame_submitted) = after_frame_submitted.as_deref_mut() {
                    let stage_started_at = Instant::now();
                    display_handoff.push(
                        NativeVulkanVulkanaliaDisplayOrderHandoffFrame {
                            decode_frame_index: frame_index,
                            sampled_array_layer: plan.common.dst_picture_resource.base_array_layer,
                            h264_b_picture: false,
                            source_frame_pts_ns: frame.pts_ns,
                            source_frame_duration_ns: frame.duration_ns,
                            source_frame_pts_ms: frame.entry.pts_ms,
                            source_frame_duration_ms: frame.duration_ms,
                            display_order_key,
                            display_order_key_source,
                            decode_complete_value: decode_complete_value_for_frame,
                        },
                        after_frame_submitted,
                    )?;
                    frame_timing.after_frame_submitted_micros =
                        native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
                }

                let begin_reference_slot_count = plan.common.begin_reference_slot_count as u32;
                let decode_reference_slot_count = plan.common.decode_reference_slot_count as u32;
                last_slice_segment_offsets.clear();
                last_slice_segment_offsets.extend_from_slice(plan.picture.slice_segment_offsets);
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
            if let Some(after_frame_submitted) = after_frame_submitted.as_deref_mut() {
                display_handoff.flush(after_frame_submitted)?;
            }
            let last_frame =
                frame_telemetry.last_frame("Vulkanalia H.265 streaming submitted no frames")?;
            let final_drain_wait_micros =
                submit_ring.wait_all_submitted(device, command_buffer_ref)?;
            let streaming_decode_timing = streaming_decode_timing.finish(
                native_vulkan_vulkanalia_elapsed_micros(decode_loop_started_at),
                final_drain_wait_micros,
            );

            Ok(NativeVulkanVulkanaliaH265ReadyPrefixCommandSmokeSnapshot {
                requested_frame_count,
                recorded_frame_count: frame_telemetry.submitted_frame_count,
                submitted_frame_count: frame_telemetry.submitted_frame_count,
                ffmpeg_reference,
                command_buffer_recorded,
                submitted,
                uses_synchronization2,
                uses_submit2,
                uses_inline_session_parameters: true,
                video_session_parameters_handle_used: false,
                session_parameter_strategy:
                    NATIVE_VULKAN_VULKANALIA_INLINE_SESSION_PARAMETER_STRATEGY,
                wait_idle_after_submit: false,
                wait_fence_after_submit: false,
                batch_wait_fence_after_submit: true,
                uses_submit_fence: true,
                submit_sync_model:
                    NATIVE_VULKAN_VULKANALIA_STREAMING_DECODE_SUBMIT_FENCE_SYNC_MODEL,
                submit_command_order:
                    native_vulkan_vulkanalia_streaming_decode_submit_fence_command_order(),
                queue_family_index,
                bitstream_buffer_model: "ffmpeg-picture-slices-buffer-pool-exec-owned",
                ffmpeg_slices_buffer_pool_slot_count: bitstream_buffers.slot_count(),
                ffmpeg_slices_buffer_pool_allocated_slot_count: bitstream_buffers
                    .allocated_slot_count(),
                ffmpeg_slices_buffer_pool_capacity_bytes: bitstream_buffers.total_capacity_bytes(),
                ffmpeg_slices_buffer_pool_max_slot_bytes: bitstream_buffers
                    .max_slot_capacity_bytes(),
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
                reset_control_recorded_frame_count: frame_telemetry
                    .reset_control_recorded_frame_count,
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
                slice_segment_count: last_slice_segment_offsets.len() as u32,
                slice_segment_offsets: last_slice_segment_offsets,
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

#[allow(clippy::too_many_arguments)]
pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_record_h264_streaming_decode_into_image(
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
    input: NativeVulkanVulkanaliaH264StreamingDecodeInput<'_>,
    image: &super::video_session_images::VulkanaliaVideoSessionResourceImage,
    mut before_output_slot_reuse: Option<NativeVulkanVulkanaliaBeforeOutputSlotReuse<'_>>,
    mut after_frame_submitted: Option<NativeVulkanVulkanaliaAfterFrameSubmitted<'_>>,
    decode_complete_semaphore: vk::Semaphore,
    decode_complete_value: &std::cell::Cell<u64>,
) -> Result<NativeVulkanVulkanaliaH264ReadyPrefixCommandSmokeSnapshot, String> {
    if codec != NativeVulkanVideoSessionCodec::H264High8 {
        return Err("Vulkanalia H.264 streaming decode requires H.264 high-8".into());
    }
    let requested_frame_count = input.requested_frame_count;
    if requested_frame_count == 0 {
        return Err("Vulkanalia H.264 streaming decode requires at least one frame".to_owned());
    }
    let array_layers = array_layers.max(1);
    let exec_ring_depth = exec_ring_depth.max(1);
    let parameter_sets = input.parameter_sets;
    let inline_session_parameters =
        native_vulkan_vulkanalia_h264_inline_session_parameters(codec, &parameter_sets)?;
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

    let result =
        (|| -> Result<NativeVulkanVulkanaliaH264ReadyPrefixCommandSmokeSnapshot, String> {
            let parameter_ids =
                NativeVulkanVulkanaliaH264ParameterIds::from_parameter_sets(&parameter_sets)?;
            let command_buffer_ref = command_buffer
                .as_ref()
                .expect("Vulkanalia streaming command buffer is alive during decode");
            let mut initialized_slots = vec![false; array_layers as usize];
            let mut frame_telemetry = NativeVulkanVulkanaliaDecodeFrameTelemetry::new();
            let mut last_slice_segment_offsets = Vec::new();
            let mut h264_reference_infos = Vec::<
                super::video_decode_submit_h264::NativeVulkanVulkanaliaH264ReferenceInfoPlan,
            >::new();
            let mut command_buffer_recorded = true;
            let mut submitted = true;
            let mut uses_synchronization2 = true;
            let mut uses_submit2 = true;
            let mut ffmpeg_reference = "references/ffmpeg/libavcodec/vulkan_decode.c";
            let mut src_buffer_total_bytes = 0u64;
            let decode_loop_started_at = Instant::now();
            let mut streaming_decode_timing =
                NativeVulkanVulkanaliaStreamingDecodeTiming::default();
            let mut display_handoff = NativeVulkanVulkanaliaDisplayOrderHandoff::h264_ffmpeg(
                native_vulkan_vulkanalia_h264_initial_has_b_frames(&parameter_sets),
            );

            for frame_index in 0..requested_frame_count {
                let mut frame_timing = NativeVulkanVulkanaliaStreamingDecodeFrameTiming::default();
                let stage_started_at = Instant::now();
                let mut frame = (input.next_frame)()?;
                frame_timing.next_frame_micros =
                    native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
                let submit_slot = submit_ring.exec_slot_for_frame(frame_index);
                frame_timing.exec_slot_reuse_wait_micros =
                    submit_ring.wait_for_slot_reuse(device, command_buffer_ref, submit_slot)?;
                if frame.entry.planned_output_slot >= array_layers {
                    return Err(format!(
                        "Vulkanalia H.264 streaming planned output slot {} exceeds image layers {array_layers}",
                        frame.entry.planned_output_slot
                    ));
                }
                for reference in &frame.entry.references {
                    if let Some(dpb_slot) = reference.dpb_slot
                        && dpb_slot >= array_layers
                    {
                        return Err(format!(
                            "Vulkanalia H.264 streaming reference slot {dpb_slot} exceeds image layers {array_layers}"
                        ));
                    }
                }
                if frame.first_slice.slice_offsets.is_empty() {
                    return Err(format!(
                        "Vulkanalia H.264 streaming AU {} has no slice offsets",
                        frame.entry.access_unit_index
                    ));
                }
                if let Some(before_output_slot_reuse) = before_output_slot_reuse.as_deref_mut() {
                    let stage_started_at = Instant::now();
                    before_output_slot_reuse(frame.entry.planned_output_slot)?;
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

                let reset_control_recorded = frame.first_slice.idr;
                let stage_started_at = Instant::now();
                let plan = native_vulkan_vulkanalia_h264_ready_prefix_decode_submit_plan(
                    extent,
                    parameter_ids,
                    &frame.entry,
                    &frame.first_slice,
                    src_buffer_offset,
                    src_buffer_range,
                    &frame.first_slice.slice_offsets,
                    reset_control_recorded,
                    &mut h264_reference_infos,
                )?;
                frame_timing.decode_plan_micros =
                    native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
                ffmpeg_reference = plan.common.ffmpeg_reference;
                let stage_started_at = Instant::now();
                let image_views =
                    native_vulkan_vulkanalia_h264_decode_image_view_bindings(image, &plan)?;
                frame_timing.image_view_bind_micros =
                    native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
                let dst_slot = plan.common.dst_picture_resource.base_array_layer as usize;
                let transition_dst_from_undefined = !initialized_slots[dst_slot];
                let decode_command_buffer = command_buffer_ref.command_buffer_at(submit_slot)?;
                let stage_started_at = Instant::now();
                let record_plan = unsafe {
                    native_vulkan_vulkanalia_record_h264_decode_command_buffer(
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
                        "Vulkanalia H.264 decode queue host-access lock is poisoned".to_owned()
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
                command_buffer_recorded &=
                    record_plan.command_order.contains(&"vkEndCommandBuffer");
                submitted &= submit_plan.command_order.contains(&"queue_submit2");
                uses_synchronization2 &= record_plan.uses_synchronization2;
                uses_submit2 &= submit_plan.uses_submit2;
                let (display_order_key, display_order_key_source) =
                    native_vulkan_vulkanalia_h264_display_order_key(
                        &frame.entry,
                        frame.pts_ns,
                        frame_index,
                    );

                if let Some(after_frame_submitted) = after_frame_submitted.as_deref_mut() {
                    let stage_started_at = Instant::now();
                    display_handoff.push(
                        NativeVulkanVulkanaliaDisplayOrderHandoffFrame {
                            decode_frame_index: frame_index,
                            sampled_array_layer: plan.common.dst_picture_resource.base_array_layer,
                            h264_b_picture: frame.first_slice.is_b,
                            source_frame_pts_ns: frame.pts_ns,
                            source_frame_duration_ns: frame.duration_ns,
                            source_frame_pts_ms: frame.entry.pts_ms,
                            source_frame_duration_ms: frame.duration_ms,
                            display_order_key,
                            display_order_key_source,
                            decode_complete_value: decode_complete_value_for_frame,
                        },
                        after_frame_submitted,
                    )?;
                    frame_timing.after_frame_submitted_micros =
                        native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
                }

                let begin_reference_slot_count = plan.common.begin_reference_slot_count as u32;
                let decode_reference_slot_count = plan.common.decode_reference_slot_count as u32;
                last_slice_segment_offsets.clear();
                last_slice_segment_offsets.extend_from_slice(plan.picture.slice_offsets);
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
            if let Some(after_frame_submitted) = after_frame_submitted.as_deref_mut() {
                display_handoff.flush(after_frame_submitted)?;
            }
            let last_frame =
                frame_telemetry.last_frame("Vulkanalia H.264 streaming submitted no frames")?;
            let final_drain_wait_micros =
                submit_ring.wait_all_submitted(device, command_buffer_ref)?;
            let streaming_decode_timing = streaming_decode_timing.finish(
                native_vulkan_vulkanalia_elapsed_micros(decode_loop_started_at),
                final_drain_wait_micros,
            );

            Ok(NativeVulkanVulkanaliaH264ReadyPrefixCommandSmokeSnapshot {
                requested_frame_count,
                recorded_frame_count: frame_telemetry.submitted_frame_count,
                submitted_frame_count: frame_telemetry.submitted_frame_count,
                ffmpeg_reference,
                command_buffer_recorded,
                submitted,
                uses_synchronization2,
                uses_submit2,
                uses_inline_session_parameters: true,
                video_session_parameters_handle_used: false,
                session_parameter_strategy:
                    NATIVE_VULKAN_VULKANALIA_INLINE_SESSION_PARAMETER_STRATEGY,
                wait_idle_after_submit: false,
                wait_fence_after_submit: false,
                batch_wait_fence_after_submit: true,
                uses_submit_fence: true,
                submit_sync_model:
                    NATIVE_VULKAN_VULKANALIA_STREAMING_DECODE_SUBMIT_FENCE_SYNC_MODEL,
                submit_command_order:
                    native_vulkan_vulkanalia_streaming_decode_submit_fence_command_order(),
                queue_family_index,
                bitstream_buffer_model: "ffmpeg-picture-slices-buffer-pool-exec-owned",
                ffmpeg_slices_buffer_pool_slot_count: bitstream_buffers.slot_count(),
                ffmpeg_slices_buffer_pool_allocated_slot_count: bitstream_buffers
                    .allocated_slot_count(),
                ffmpeg_slices_buffer_pool_capacity_bytes: bitstream_buffers.total_capacity_bytes(),
                ffmpeg_slices_buffer_pool_max_slot_bytes: bitstream_buffers
                    .max_slot_capacity_bytes(),
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
                reset_control_recorded_frame_count: frame_telemetry
                    .reset_control_recorded_frame_count,
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
                slice_segment_count: last_slice_segment_offsets.len() as u32,
                slice_segment_offsets: last_slice_segment_offsets,
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

fn native_vulkan_vulkanalia_h264_decode_image_view_bindings(
    image: &super::video_session_images::VulkanaliaVideoSessionResourceImage,
    plan: &super::video_decode_submit_h264::NativeVulkanVulkanaliaH264DecodeSubmitPlan,
) -> Result<NativeVulkanVulkanaliaDecodeImageViewBindings, String> {
    Ok(NativeVulkanVulkanaliaDecodeImageViewBindings {
        dst_picture_image_view: native_vulkan_vulkanalia_layer_view(
            image,
            plan.common.dst_picture_resource.base_array_layer,
        )?,
        setup_reference_image_view: image.view,
        begin_reference_image_view: image.view,
        begin_reference_image_view_count: plan.common.begin_reference_slot_count,
        decode_reference_image_view: image.view,
        decode_reference_image_view_count: plan.common.decode_reference_slot_count,
    })
}

fn native_vulkan_vulkanalia_h265_decode_image_view_bindings(
    image: &super::video_session_images::VulkanaliaVideoSessionResourceImage,
    plan: &super::video_decode_submit_h265::NativeVulkanVulkanaliaH265DecodeSubmitPlan,
) -> Result<NativeVulkanVulkanaliaDecodeImageViewBindings, String> {
    Ok(NativeVulkanVulkanaliaDecodeImageViewBindings {
        dst_picture_image_view: native_vulkan_vulkanalia_layer_view(
            image,
            plan.common.dst_picture_resource.base_array_layer,
        )?,
        setup_reference_image_view: image.view,
        begin_reference_image_view: image.view,
        begin_reference_image_view_count: plan.common.begin_reference_slot_count,
        decode_reference_image_view: image.view,
        decode_reference_image_view_count: plan.common.decode_reference_slot_count,
    })
}

fn native_vulkan_vulkanalia_av1_decode_image_view_bindings(
    image: &super::video_session_images::VulkanaliaVideoSessionResourceImage,
    plan: &super::video_decode_submit_av1::NativeVulkanVulkanaliaAv1DecodeSubmitPlan,
) -> Result<NativeVulkanVulkanaliaDecodeImageViewBindings, String> {
    Ok(NativeVulkanVulkanaliaDecodeImageViewBindings {
        dst_picture_image_view: native_vulkan_vulkanalia_layer_view(
            image,
            plan.common.dst_picture_resource.base_array_layer,
        )?,
        setup_reference_image_view: image.view,
        begin_reference_image_view: image.view,
        begin_reference_image_view_count: plan.common.begin_reference_slot_count,
        decode_reference_image_view: image.view,
        decode_reference_image_view_count: plan.common.decode_reference_slot_count,
    })
}

fn native_vulkan_vulkanalia_layer_view(
    image: &super::video_session_images::VulkanaliaVideoSessionResourceImage,
    layer: u32,
) -> Result<vk::ImageView, String> {
    image
        .layer_views
        .get(layer as usize)
        .copied()
        .ok_or_else(|| {
            format!(
                "Vulkanalia video image has {} layer views but layer {layer} was requested",
                image.layer_views.len()
            )
        })
}

fn queue_flag_labels(flags: vk::QueueFlags) -> Vec<&'static str> {
    [
        (vk::QueueFlags::GRAPHICS, "graphics"),
        (vk::QueueFlags::COMPUTE, "compute"),
        (vk::QueueFlags::TRANSFER, "transfer"),
        (vk::QueueFlags::SPARSE_BINDING, "sparse-binding"),
        (vk::QueueFlags::PROTECTED, "protected"),
        (vk::QueueFlags::VIDEO_DECODE_KHR, "video-decode"),
        (vk::QueueFlags::VIDEO_ENCODE_KHR, "video-encode"),
    ]
    .into_iter()
    .filter_map(|(flag, label)| flags.contains(flag).then_some(label))
    .collect()
}

fn video_codec_operation_labels(flags: vk::VideoCodecOperationFlagsKHR) -> Vec<&'static str> {
    [
        (vk::VideoCodecOperationFlagsKHR::DECODE_H264, "decode-h264"),
        (vk::VideoCodecOperationFlagsKHR::DECODE_H265, "decode-h265"),
        (vk::VideoCodecOperationFlagsKHR::DECODE_AV1, "decode-av1"),
    ]
    .into_iter()
    .filter_map(|(flag, label)| flags.contains(flag).then_some(label))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::super::video_codec::{
        native_vulkan_vulkanalia_video_session_format_probe_profile as vulkanalia_video_session_format_probe_profile,
        native_vulkan_vulkanalia_video_session_picture_format as vulkanalia_video_session_picture_format,
        native_vulkan_vulkanalia_video_session_profile_label as vulkanalia_video_session_profile_label,
    };
    use super::super::video_device::native_vulkan_vulkanalia_video_decode_required_device_extensions;
    use super::*;
    use vulkanalia::vk::Handle;

    #[test]
    fn session_bind_smoke_maps_codec_extensions_and_formats() {
        assert_eq!(
            native_vulkan_vulkanalia_video_decode_required_device_extensions(
                NativeVulkanVideoSessionCodec::H265Main10
            ),
            vec![
                "VK_KHR_video_queue",
                "VK_KHR_video_decode_queue",
                "VK_KHR_video_decode_h265"
            ]
        );
        assert_eq!(
            vulkanalia_video_session_picture_format(NativeVulkanVideoSessionCodec::Av1Main10),
            vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16
        );
        assert_eq!(
            vulkanalia_video_session_format_probe_profile(NativeVulkanVideoSessionCodec::H264High8),
            "high"
        );
        assert_eq!(
            vulkanalia_video_session_profile_label(NativeVulkanVideoSessionCodec::H264High8),
            "high-8"
        );
    }

    #[test]
    fn h264_decode_bindings_use_ffmpeg_dst_layer_view_and_layered_refs() {
        let reference_infos = Vec::new();
        let plan = super::super::video_decode_submit_h264::NativeVulkanVulkanaliaH264DecodeSubmitPlan {
            common: super::super::video_decode_submit::NativeVulkanVulkanaliaDecodeSubmitPlan::new(
                NativeVulkanVideoSessionCodec::H264High8,
                0,
                0,
                super::super::video_decode_submit::NativeVulkanVulkanaliaPictureResourcePlan::new(
                    vk::Extent2D {
                        width: 1280,
                        height: 720,
                    },
                    2,
                ),
                super::super::video_decode_submit::NativeVulkanVulkanaliaReferenceSlotPlan::setup_current(
                    2,
                    super::super::video_decode_submit::NativeVulkanVulkanaliaPictureResourcePlan::new(
                        vk::Extent2D {
                            width: 1280,
                            height: 720,
                        },
                        2,
                    ),
                ),
                0,
                0,
                false,
            ),
            picture: super::super::video_decode_submit_h264::NativeVulkanVulkanaliaH264PictureInfoPlan {
                ffmpeg_reference: super::super::video_decode_submit::FFMPEG_VULKAN_DECODE_REFERENCE,
                seq_parameter_set_id: 0,
                pic_parameter_set_id: 0,
                field_pic_flag: false,
                bottom_field_flag: false,
                is_intra: false,
                is_idr: false,
                is_reference: false,
                frame_num: 0,
                idr_pic_id: 0,
                pic_order_cnt: [0, 0],
                slice_offsets: &[0],
                references: reference_infos.as_slice(),
            },
        };
        let image = super::super::video_session_images::VulkanaliaVideoSessionResourceImage {
            image: vk::Image::null(),
            memory: vk::DeviceMemory::null(),
            view: vk::ImageView::from_raw(100),
            layer_views: vec![
                vk::ImageView::from_raw(101),
                vk::ImageView::from_raw(102),
                vk::ImageView::from_raw(103),
            ],
            snapshot: super::super::video_session_images::NativeVulkanVulkanaliaVideoSessionResourceImageSnapshot {
                role: "coincident-dpb-output-sampled-video",
                format: "G8_B8R8_2PLANE_420_UNORM".to_owned(),
                image_type: "_2D".to_owned(),
                image_tiling: "OPTIMAL".to_owned(),
                image_usage_flags: vec!["sampled", "video-decode-dst", "video-decode-dpb"],
                image_create_flags: vec!["mutable-format"],
                extent: (1280, 720, 1),
                array_layers: 3,
                image_view_type: "2d-array",
                image_view_created: true,
                layer_view_count: 3,
                memory_size: 0,
                memory_alignment: 0,
                memory_type_bits: 0,
                selected_memory_type_index: 0,
                selected_memory_property_flags: vec![],
            },
        };

        let bindings = native_vulkan_vulkanalia_h264_decode_image_view_bindings(&image, &plan)
            .expect("bindings should resolve");

        assert_eq!(
            bindings.dst_picture_image_view,
            vk::ImageView::from_raw(103)
        );
        assert_eq!(
            bindings.setup_reference_image_view,
            vk::ImageView::from_raw(100)
        );
    }
}
