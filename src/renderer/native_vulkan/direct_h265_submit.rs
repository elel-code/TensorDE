use std::ffi::c_void;
use std::time::Instant;

use ash::vk;

use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn native_vulkan_decode_h265_ready_prefix_frame_to_image(
    device: &ash::Device,
    video_queue_device: &ash::khr::video_queue::Device,
    video_decode_queue_device: &ash::khr::video_decode_queue::Device,
    video_queue: vk::Queue,
    command_buffer: vk::CommandBuffer,
    session: vk::VideoSessionKHR,
    session_parameters: vk::VideoSessionParametersKHR,
    extent: vk::Extent2D,
    image: &NativeVulkanVideoResourceImage,
    buffer: &NativeVulkanVideoBitstreamBuffer,
    bitstream_buffer_barrier_size: u64,
    parameter_sets: &NativeVulkanH265ParameterSetSnapshot,
    entry: &NativeVulkanH265DecodeReferencePlanEntrySnapshot,
    access_unit: &NativeVulkanH265AccessUnitSnapshot,
    span: &NativeVulkanH265ReadyPrefixBitstreamSpan,
    active_dpb_refs: &[Option<NativeVulkanH265ActiveDpbReference>],
    begin_slot_policy: NativeVulkanH265BeginSlotPolicy,
    image_layer_layouts: &mut [vk::ImageLayout],
    reset_before_decode: bool,
    pts_delta_ms: Option<u64>,
    signal_semaphore: vk::Semaphore,
    playback_frame_index: u32,
    playback_loop_index: u32,
    ready_prefix_frame_index: u32,
    loop_boundary_reset: bool,
    bitstream_upload_elapsed_us: u64,
) -> Result<NativeVulkanDirectH265ReadyPrefixFrameSnapshot, NativeVulkanError> {
    let first_slice = access_unit.first_slice.as_ref().ok_or_else(|| {
        NativeVulkanError::Video(format!(
            "direct H.265 visible AU {} has no parsed first slice",
            access_unit.index
        ))
    })?;
    if access_unit.first_slice_parse_error.is_some() {
        return Err(NativeVulkanError::Video(format!(
            "direct H.265 visible AU {} first slice parse failed",
            access_unit.index
        )));
    }
    let current_poc = entry.current_poc.ok_or_else(|| {
        NativeVulkanError::Video(format!(
            "direct H.265 visible AU {} has no current POC",
            access_unit.index
        ))
    })?;
    if entry.planned_output_slot >= image.snapshot.array_layers {
        return Err(NativeVulkanError::Video(format!(
            "direct H.265 visible AU {} planned DPB slot {} exceeds image array layers {}",
            access_unit.index, entry.planned_output_slot, image.snapshot.array_layers
        )));
    }
    let available_references = entry
        .references
        .iter()
        .filter(|reference| reference.available)
        .collect::<Vec<_>>();
    if available_references.len() != entry.references.len() {
        return Err(NativeVulkanError::Video(format!(
            "direct H.265 visible AU {} is not fully reference-ready",
            access_unit.index
        )));
    }
    if available_references.len() > 8 {
        return Err(NativeVulkanError::Video(format!(
            "direct H.265 visible AU {} has {} references; first smoke supports at most 8",
            access_unit.index,
            available_references.len()
        )));
    }

    let dst_picture_resource = native_vulkan_video_picture_resource_info_for_image(
        image,
        extent,
        entry.planned_output_slot,
    )?;
    let std_setup_reference_info = vk::native::StdVideoDecodeH265ReferenceInfo {
        flags: vk::native::StdVideoDecodeH265ReferenceInfoFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::native::StdVideoDecodeH265ReferenceInfoFlags::new_bitfield_1(0, 0),
            __bindgen_padding_0: [0; 3],
        },
        PicOrderCntVal: current_poc,
    };
    let mut setup_h265_slot_info =
        vk::VideoDecodeH265DpbSlotInfoKHR::default().std_reference_info(&std_setup_reference_info);
    let setup_slot_index = entry
        .setup_slot_index
        .unwrap_or(entry.planned_output_slot as i32);
    let setup_reference_slot = vk::VideoReferenceSlotInfoKHR::default()
        .picture_resource(&dst_picture_resource)
        .slot_index(setup_slot_index)
        .push_next(&mut setup_h265_slot_info);

    let reference_resources = available_references
        .iter()
        .map(|reference| {
            let dpb_slot = reference.dpb_slot.ok_or_else(|| {
                NativeVulkanError::Video(format!(
                    "direct H.265 visible AU {} reference POC {} has no DPB slot",
                    access_unit.index, reference.poc
                ))
            })?;
            native_vulkan_video_picture_resource_info_for_image(image, extent, dpb_slot)
        })
        .collect::<Result<Vec<_>, NativeVulkanError>>()?;
    let reference_std_infos = available_references
        .iter()
        .map(|reference| vk::native::StdVideoDecodeH265ReferenceInfo {
            flags: vk::native::StdVideoDecodeH265ReferenceInfoFlags {
                _bitfield_align_1: [],
                _bitfield_1: vk::native::StdVideoDecodeH265ReferenceInfoFlags::new_bitfield_1(
                    native_vulkan_bool_u32(reference.used_for_long_term_reference),
                    0,
                ),
                __bindgen_padding_0: [0; 3],
            },
            PicOrderCntVal: reference.poc,
        })
        .collect::<Vec<_>>();
    let mut reference_h265_slot_infos = reference_std_infos
        .iter()
        .map(|std_reference_info| {
            vk::VideoDecodeH265DpbSlotInfoKHR::default().std_reference_info(std_reference_info)
        })
        .collect::<Vec<_>>();
    let reference_slots = available_references
        .iter()
        .enumerate()
        .map(|(reference_slot_index, reference)| {
            let dpb_slot = reference
                .dpb_slot
                .expect("available references were checked for DPB slots");
            let mut slot = vk::VideoReferenceSlotInfoKHR::default()
                .picture_resource(&reference_resources[reference_slot_index])
                .slot_index(dpb_slot as i32);
            slot.p_next = (&mut reference_h265_slot_infos[reference_slot_index]
                as *mut vk::VideoDecodeH265DpbSlotInfoKHR<'_>)
                .cast::<c_void>();
            slot
        })
        .collect::<Vec<_>>();
    let ref_pic_set_st_curr_before =
        native_vulkan_h265_ref_pic_set_st_curr_before(access_unit.index, &available_references)?;
    let ref_pic_set_st_curr_after =
        native_vulkan_h265_ref_pic_set_st_curr_after(access_unit.index, &available_references)?;
    let ref_pic_set_lt_curr =
        native_vulkan_h265_ref_pic_set_lt_curr(access_unit.index, &available_references)?;
    let num_delta_pocs_of_ref_rps_idx =
        native_vulkan_h265_num_delta_pocs_of_ref_rps_idx(first_slice);
    let num_bits_for_st_ref_pic_set_in_slice =
        native_vulkan_h265_num_bits_for_st_ref_pic_set_in_slice(first_slice);
    let std_picture_info = vk::native::StdVideoDecodeH265PictureInfo {
        flags: vk::native::StdVideoDecodeH265PictureInfoFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::native::StdVideoDecodeH265PictureInfoFlags::new_bitfield_1(
                native_vulkan_bool_u32(first_slice.irap),
                native_vulkan_bool_u32(first_slice.idr),
                1,
                native_vulkan_bool_u32(first_slice.short_term_ref_pic_set_sps_flag),
            ),
            __bindgen_padding_0: [0; 3],
        },
        sps_video_parameter_set_id: parameter_sets.sps.vps_id,
        pps_seq_parameter_set_id: native_vulkan_h265_u8(
            parameter_sets.pps.sps_id,
            "pps_seq_parameter_set_id",
        )
        .map_err(NativeVulkanError::Video)?,
        pps_pic_parameter_set_id: native_vulkan_h265_u8(
            parameter_sets.pps.id,
            "pps_pic_parameter_set_id",
        )
        .map_err(NativeVulkanError::Video)?,
        NumDeltaPocsOfRefRpsIdx: num_delta_pocs_of_ref_rps_idx,
        PicOrderCntVal: current_poc,
        NumBitsForSTRefPicSetInSlice: num_bits_for_st_ref_pic_set_in_slice,
        reserved: 0,
        RefPicSetStCurrBefore: ref_pic_set_st_curr_before,
        RefPicSetStCurrAfter: ref_pic_set_st_curr_after,
        RefPicSetLtCurr: ref_pic_set_lt_curr,
    };
    let slice_segment_offsets = native_vulkan_h265_slice_segment_offsets(span)?;
    let mut h265_picture_info = vk::VideoDecodeH265PictureInfoKHR::default()
        .std_picture_info(&std_picture_info)
        .slice_segment_offsets(&slice_segment_offsets);
    let decode_info = vk::VideoDecodeInfoKHR::default()
        .src_buffer(buffer.buffer)
        .src_buffer_offset(span.offset)
        .src_buffer_range(span.range)
        .dst_picture_resource(dst_picture_resource)
        .setup_reference_slot(&setup_reference_slot)
        .reference_slots(&reference_slots)
        .push_next(&mut h265_picture_info);
    let begin_slot_refs = native_vulkan_h265_begin_slot_refs(
        active_dpb_refs,
        &entry.references,
        reset_before_decode,
        begin_slot_policy,
    );
    let begin_reference_resources = begin_slot_refs
        .iter()
        .map(|(slot, _)| native_vulkan_video_picture_resource_info_for_image(image, extent, *slot))
        .collect::<Result<Vec<_>, _>>()?;
    let begin_reference_std_infos = begin_slot_refs
        .iter()
        .map(
            |(_, reference)| vk::native::StdVideoDecodeH265ReferenceInfo {
                flags: vk::native::StdVideoDecodeH265ReferenceInfoFlags {
                    _bitfield_align_1: [],
                    _bitfield_1: vk::native::StdVideoDecodeH265ReferenceInfoFlags::new_bitfield_1(
                        native_vulkan_bool_u32(
                            reference
                                .is_some_and(|reference| reference.used_for_long_term_reference),
                        ),
                        0,
                    ),
                    __bindgen_padding_0: [0; 3],
                },
                PicOrderCntVal: reference.map(|reference| reference.poc).unwrap_or(0),
            },
        )
        .collect::<Vec<_>>();
    let mut begin_reference_h265_slot_infos = begin_reference_std_infos
        .iter()
        .map(|std_reference_info| {
            vk::VideoDecodeH265DpbSlotInfoKHR::default().std_reference_info(std_reference_info)
        })
        .collect::<Vec<_>>();
    let mut begin_reference_slots = Vec::with_capacity(begin_slot_refs.len());
    for (index, (slot, reference)) in begin_slot_refs.iter().enumerate() {
        let mut reference_slot = vk::VideoReferenceSlotInfoKHR::default()
            .picture_resource(&begin_reference_resources[index])
            .slot_index(if reference.is_some() {
                *slot as i32
            } else {
                -1
            });
        if reference.is_some() {
            reference_slot.p_next = (&mut begin_reference_h265_slot_infos[index]
                as *mut vk::VideoDecodeH265DpbSlotInfoKHR<'_>)
                .cast::<c_void>();
        }
        begin_reference_slots.push(reference_slot);
    }
    let control_info =
        vk::VideoCodingControlInfoKHR::default().flags(vk::VideoCodingControlFlagsKHR::RESET);
    if begin_slot_policy.include_setup_slot {
        begin_reference_slots.push(setup_reference_slot);
    }
    let begin_info = vk::VideoBeginCodingInfoKHR::default()
        .video_session(session)
        .video_session_parameters(session_parameters)
        .reference_slots(&begin_reference_slots);

    let started_at = Instant::now();
    unsafe {
        device
            .reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty())
            .map_err(|result| NativeVulkanError::Vulkan {
                operation: "vkResetCommandBuffer(direct h265 visible frame decode)",
                result,
            })?;
        let command_begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        device
            .begin_command_buffer(command_buffer, &command_begin_info)
            .map_err(|result| NativeVulkanError::Vulkan {
                operation: "vkBeginCommandBuffer(direct h265 visible frame decode)",
                result,
            })?;
        native_vulkan_video_decode_prepare_frame_barriers(
            device,
            command_buffer,
            image,
            buffer.buffer,
            bitstream_buffer_barrier_size,
            image_layer_layouts,
        )?;
        (video_queue_device.fp().cmd_begin_video_coding_khr)(command_buffer, &begin_info);
        if reset_before_decode {
            (video_queue_device.fp().cmd_control_video_coding_khr)(command_buffer, &control_info);
        }
        (video_decode_queue_device.fp().cmd_decode_video_khr)(command_buffer, &decode_info);
        (video_queue_device.fp().cmd_end_video_coding_khr)(
            command_buffer,
            &vk::VideoEndCodingInfoKHR::default(),
        );
        device
            .end_command_buffer(command_buffer)
            .map_err(|result| NativeVulkanError::Vulkan {
                operation: "vkEndCommandBuffer(direct h265 visible frame decode)",
                result,
            })?;
        let command_buffers = [command_buffer];
        let signal_semaphores = [signal_semaphore];
        let submit_info = vk::SubmitInfo::default()
            .command_buffers(&command_buffers)
            .signal_semaphores(&signal_semaphores);
        device
            .queue_submit(video_queue, &[submit_info], vk::Fence::null())
            .map_err(|result| NativeVulkanError::Vulkan {
                operation: "vkQueueSubmit(direct h265 visible frame decode)",
                result,
            })?;
    }

    Ok(NativeVulkanDirectH265ReadyPrefixFrameSnapshot {
        playback_frame_index,
        playback_loop_index,
        ready_prefix_frame_index,
        loop_boundary_reset,
        access_unit_index: access_unit.index,
        pts_ms: access_unit.pts_ms,
        pts_delta_ms,
        nal_type: first_slice.nal_type,
        nal_type_label: first_slice.nal_type_label,
        slice_type: first_slice.slice_type,
        pic_order_cnt_val: current_poc,
        idr: first_slice.idr,
        irap: first_slice.irap,
        short_term_ref_pic_set_sps_flag: first_slice.short_term_ref_pic_set_sps_flag,
        num_delta_pocs_of_ref_rps_idx,
        num_bits_for_st_ref_pic_set_in_slice,
        reset_before_decode,
        src_buffer_offset: span.offset,
        src_buffer_range: span.range,
        bitstream_payload_bytes: span.payload_bytes,
        bitstream_ring_allocation_index: span.ring_allocation_index,
        bitstream_ring_wrap_count: span.ring_wrap_count,
        bitstream_upload_elapsed_us,
        fence_wait_elapsed_us: 0,
        dst_base_array_layer: entry.planned_output_slot,
        setup_slot_index,
        decode_reference_slot_count: reference_slots.len() as u32,
        decode_elapsed_us: native_vulkan_elapsed_us(started_at.elapsed()),
        descriptor_update_elapsed_us: 0,
        acquire_elapsed_us: 0,
        record_elapsed_us: 0,
        submit_elapsed_us: 0,
        queue_present_elapsed_us: 0,
        present_elapsed_us: 0,
        present_result_since_start_us: 0,
    })
}
