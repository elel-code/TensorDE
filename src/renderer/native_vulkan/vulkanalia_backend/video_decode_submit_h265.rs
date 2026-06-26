#![allow(dead_code)]

use serde::Serialize;
use vulkanalia::vk::{self, HasBuilder};

use crate::renderer::native_vulkan::{
    NativeVulkanEncodedAccessUnitPayload, NativeVulkanH265AccessUnitSliceSnapshot,
    NativeVulkanH265DecodeReferencePlanEntrySnapshot, NativeVulkanH265ParameterSetSnapshot,
    NativeVulkanVideoSessionCodec,
};

use super::video_decode_submit::{
    NativeVulkanVulkanaliaDecodeImageViewBindings, NativeVulkanVulkanaliaDecodeSubmitPlan,
    NativeVulkanVulkanaliaPictureResourcePlan, NativeVulkanVulkanaliaReferenceSlotPlan,
    NativeVulkanVulkanaliaReferenceSlotRole, NativeVulkanVulkanaliaStreamingDecodeTimingSnapshot,
};

const FFMPEG_H265_PICTURE_REFERENCE: &str = "references/ffmpeg/libavcodec/vulkan_hevc.c";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(super) struct NativeVulkanVulkanaliaH265ParameterIds {
    pub sps_video_parameter_set_id: u8,
    pub pps_seq_parameter_set_id: u8,
    pub pps_pic_parameter_set_id: u8,
}

impl NativeVulkanVulkanaliaH265ParameterIds {
    pub(super) fn from_parameter_sets(
        parameter_sets: &NativeVulkanH265ParameterSetSnapshot,
    ) -> Result<Self, String> {
        Ok(Self {
            sps_video_parameter_set_id: parameter_sets.sps.vps_id,
            pps_seq_parameter_set_id: h265_u8(
                parameter_sets.pps.sps_id,
                "pps_seq_parameter_set_id",
            )?,
            pps_pic_parameter_set_id: h265_u8(parameter_sets.pps.id, "pps_pic_parameter_set_id")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct NativeVulkanVulkanaliaH265ReferenceInfoPlan {
    pub slot_index: i32,
    pub delta_poc: i32,
    pub poc: i32,
    pub used_for_long_term_reference: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct NativeVulkanVulkanaliaH265PictureInfoPlan {
    pub ffmpeg_reference: &'static str,
    pub is_irap: bool,
    pub is_idr: bool,
    pub pps_curr_pic_ref_enabled_flag: bool,
    pub short_term_ref_pic_set_sps_flag: bool,
    pub sps_video_parameter_set_id: u8,
    pub pps_seq_parameter_set_id: u8,
    pub pps_pic_parameter_set_id: u8,
    pub num_delta_pocs_of_ref_rps_idx: u8,
    pub pic_order_cnt_val: i32,
    pub num_bits_for_st_ref_pic_set_in_slice: u16,
    pub slice_segment_offsets: Vec<u32>,
    pub references: Vec<NativeVulkanVulkanaliaH265ReferenceInfoPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct NativeVulkanVulkanaliaH265DecodeSubmitPlan {
    pub common: NativeVulkanVulkanaliaDecodeSubmitPlan,
    pub picture: NativeVulkanVulkanaliaH265PictureInfoPlan,
}

pub(super) struct NativeVulkanVulkanaliaH265VkSubmitInfo<'a> {
    pub begin_info: &'a vk::VideoBeginCodingInfoKHR,
    pub decode_info: &'a vk::VideoDecodeInfoKHR,
    pub h265_picture_info: &'a vk::VideoDecodeH265PictureInfoKHR,
    pub std_picture_info: &'a vk::video::StdVideoDecodeH265PictureInfo,
    pub setup_reference_slot: &'a vk::VideoReferenceSlotInfoKHR,
    pub begin_reference_slots: &'a [vk::VideoReferenceSlotInfoKHR],
    pub decode_reference_slots: &'a [vk::VideoReferenceSlotInfoKHR],
}

#[derive(Debug)]
pub struct NativeVulkanVulkanaliaH265ReadyPrefixDecodeInput {
    pub parameter_sets: NativeVulkanH265ParameterSetSnapshot,
    pub requested_frame_count: u32,
    pub frames: Vec<NativeVulkanVulkanaliaH265ReadyPrefixFrameInput>,
}

#[derive(Debug)]
pub struct NativeVulkanVulkanaliaH265ReadyPrefixFrameInput {
    pub entry: NativeVulkanH265DecodeReferencePlanEntrySnapshot,
    pub first_slice: NativeVulkanH265AccessUnitSliceSnapshot,
    pub pts_ns: Option<u64>,
    pub duration_ns: Option<u64>,
    pub duration_ms: Option<u64>,
    pub access_unit_payload: NativeVulkanEncodedAccessUnitPayload,
    pub slice_segment_offset: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaH265ReadyPrefixCommandFrameSnapshot {
    pub frame_index: u32,
    pub access_unit_index: u32,
    pub pts_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    pub display_order_key: i64,
    pub display_order_key_source: &'static str,
    pub src_buffer_offset: u64,
    pub src_buffer_range: u64,
    pub dst_base_array_layer: u32,
    pub setup_slot_index: i32,
    pub begin_reference_slot_count: u32,
    pub decode_reference_slot_count: u32,
    pub reset_control_recorded: bool,
    pub slice_segment_count: u32,
    pub slice_segment_offsets: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaH265ReadyPrefixCommandSmokeSnapshot {
    pub requested_frame_count: u32,
    pub recorded_frame_count: u32,
    pub submitted_frame_count: u32,
    pub ffmpeg_reference: &'static str,
    pub command_buffer_recorded: bool,
    pub submitted: bool,
    pub uses_synchronization2: bool,
    pub uses_submit2: bool,
    pub wait_idle_after_submit: bool,
    pub wait_fence_after_submit: bool,
    pub batch_wait_fence_after_submit: bool,
    pub uses_submit_fence: bool,
    pub submit_sync_model: &'static str,
    pub submit_command_order: Vec<&'static str>,
    pub queue_family_index: u32,
    pub bitstream_buffer_model: &'static str,
    pub ffmpeg_slices_buffer_pool_slot_count: u32,
    pub ffmpeg_slices_buffer_pool_allocated_slot_count: u32,
    pub ffmpeg_slices_buffer_pool_capacity_bytes: u64,
    pub ffmpeg_slices_buffer_pool_max_slot_bytes: u64,
    pub input_payload_model: &'static str,
    pub src_buffer_total_bytes: u64,
    pub streaming_decode_timing: NativeVulkanVulkanaliaStreamingDecodeTimingSnapshot,
    pub retained_frame_telemetry_limit: usize,
    pub retained_frame_telemetry_count: u32,
    pub frame_telemetry_retention_model: &'static str,
    pub max_src_buffer_range: u64,
    pub first_frame_reset_control_recorded: bool,
    pub reset_control_recorded_frame_count: u32,
    pub p_frame_count: u32,
    pub b_frame_count: u32,
    pub max_begin_reference_slot_count: u32,
    pub max_decode_reference_slot_count: u32,
    pub src_buffer_offset: u64,
    pub src_buffer_range: u64,
    pub dst_base_array_layer: u32,
    pub setup_slot_index: i32,
    pub begin_reference_slot_count: u32,
    pub decode_reference_slot_count: u32,
    pub reset_control_recorded: bool,
    pub slice_segment_count: u32,
    pub slice_segment_offsets: Vec<u32>,
    pub frames: Vec<NativeVulkanVulkanaliaH265ReadyPrefixCommandFrameSnapshot>,
}

pub(super) fn native_vulkan_vulkanalia_h265_ready_prefix_decode_submit_plan(
    extent: vk::Extent2D,
    parameter_ids: NativeVulkanVulkanaliaH265ParameterIds,
    entry: &NativeVulkanH265DecodeReferencePlanEntrySnapshot,
    first_slice: &NativeVulkanH265AccessUnitSliceSnapshot,
    src_buffer_offset: u64,
    src_buffer_range: u64,
    slice_segment_offsets: Vec<u32>,
    reset_control_recorded: bool,
) -> Result<NativeVulkanVulkanaliaH265DecodeSubmitPlan, String> {
    if src_buffer_range == 0 {
        return Err("Vulkanalia H.265 decode submit requires non-empty bitstream range".to_owned());
    }
    if !entry.ready_for_decode_submit {
        return Err(format!(
            "Vulkanalia H.265 AU {} is not ready for decode submit",
            entry.access_unit_index
        ));
    }
    let current_poc = entry.current_poc.ok_or_else(|| {
        format!(
            "Vulkanalia H.265 AU {} has no current POC",
            entry.access_unit_index
        )
    })?;
    let available_references = entry
        .references
        .iter()
        .filter(|reference| reference.available)
        .collect::<Vec<_>>();
    if available_references.len() != entry.references.len() {
        return Err(format!(
            "Vulkanalia H.265 AU {} still has missing references",
            entry.access_unit_index
        ));
    }
    if slice_segment_offsets.is_empty() {
        return Err(format!(
            "Vulkanalia H.265 AU {} has no slice segment offsets",
            entry.access_unit_index
        ));
    }

    let dst_picture_resource =
        NativeVulkanVulkanaliaPictureResourcePlan::new(extent, entry.planned_output_slot);
    let setup_slot_index =
        entry
            .setup_slot_index
            .unwrap_or(i32::try_from(entry.planned_output_slot).map_err(|_| {
                format!(
                    "Vulkanalia H.265 planned output slot {} exceeds i32",
                    entry.planned_output_slot
                )
            })?);
    let setup_reference_slot = NativeVulkanVulkanaliaReferenceSlotPlan::setup_current(
        setup_slot_index,
        dst_picture_resource.clone(),
    );

    let mut reference_infos = Vec::with_capacity(available_references.len());
    let mut decode_reference_slots = Vec::with_capacity(available_references.len());
    for reference in available_references {
        let dpb_slot = reference.dpb_slot.ok_or_else(|| {
            format!(
                "Vulkanalia H.265 AU {} reference POC {} has no DPB slot",
                entry.access_unit_index, reference.poc
            )
        })?;
        let slot_index = i32::try_from(dpb_slot)
            .map_err(|_| format!("Vulkanalia H.265 DPB slot {dpb_slot} exceeds i32"))?;
        reference_infos.push(NativeVulkanVulkanaliaH265ReferenceInfoPlan {
            slot_index,
            delta_poc: reference.delta_poc,
            poc: reference.poc,
            used_for_long_term_reference: reference.used_for_long_term_reference,
        });
        decode_reference_slots.push(NativeVulkanVulkanaliaReferenceSlotPlan::decode_reference(
            slot_index,
            NativeVulkanVulkanaliaPictureResourcePlan::new(extent, dpb_slot),
        ));
    }

    let mut begin_reference_slots = decode_reference_slots.clone();
    begin_reference_slots.push(NativeVulkanVulkanaliaReferenceSlotPlan::begin_inactive(
        dst_picture_resource.clone(),
    ));

    let common = NativeVulkanVulkanaliaDecodeSubmitPlan::new(
        NativeVulkanVideoSessionCodec::H265Main8,
        src_buffer_offset,
        src_buffer_range,
        dst_picture_resource,
        setup_reference_slot,
        begin_reference_slots,
        decode_reference_slots,
        reset_control_recorded,
    );
    let picture = NativeVulkanVulkanaliaH265PictureInfoPlan {
        ffmpeg_reference: FFMPEG_H265_PICTURE_REFERENCE,
        is_irap: first_slice.irap,
        is_idr: first_slice.idr,
        pps_curr_pic_ref_enabled_flag: true,
        short_term_ref_pic_set_sps_flag: first_slice.short_term_ref_pic_set_sps_flag,
        sps_video_parameter_set_id: parameter_ids.sps_video_parameter_set_id,
        pps_seq_parameter_set_id: parameter_ids.pps_seq_parameter_set_id,
        pps_pic_parameter_set_id: parameter_ids.pps_pic_parameter_set_id,
        num_delta_pocs_of_ref_rps_idx: first_slice.num_delta_pocs_of_ref_rps_idx,
        pic_order_cnt_val: current_poc,
        num_bits_for_st_ref_pic_set_in_slice: first_slice.num_bits_for_st_ref_pic_set_in_slice,
        slice_segment_offsets,
        references: reference_infos,
    };

    Ok(NativeVulkanVulkanaliaH265DecodeSubmitPlan { common, picture })
}

pub(super) fn native_vulkan_vulkanalia_h265_with_vk_submit_info<R>(
    plan: &NativeVulkanVulkanaliaH265DecodeSubmitPlan,
    video_session: vk::VideoSessionKHR,
    session_parameters: vk::VideoSessionParametersKHR,
    src_buffer: vk::Buffer,
    image_views: &NativeVulkanVulkanaliaDecodeImageViewBindings,
    use_submit_info: impl FnOnce(NativeVulkanVulkanaliaH265VkSubmitInfo<'_>) -> R,
) -> Result<R, String> {
    if image_views.begin_reference_image_views.len() != plan.common.begin_reference_slots.len() {
        return Err(format!(
            "Vulkanalia H.265 begin image-view count {} does not match begin slot count {}",
            image_views.begin_reference_image_views.len(),
            plan.common.begin_reference_slots.len()
        ));
    }
    if image_views.decode_reference_image_views.len() != plan.common.decode_reference_slots.len() {
        return Err(format!(
            "Vulkanalia H.265 decode image-view count {} does not match decode slot count {}",
            image_views.decode_reference_image_views.len(),
            plan.common.decode_reference_slots.len()
        ));
    }

    let dst_picture_resource = plan
        .common
        .dst_picture_resource
        .to_vk_with_base_array_layer(image_views.dst_picture_image_view, 0);
    let setup_picture_resource = plan
        .common
        .setup_reference_slot
        .resource
        .to_vk(image_views.setup_reference_image_view);
    let std_setup_reference_info =
        native_vulkan_vulkanalia_h265_std_reference_info(false, plan.picture.pic_order_cnt_val);
    let mut setup_h265_slot_info = vk::VideoDecodeH265DpbSlotInfoKHR::builder()
        .std_reference_info(&std_setup_reference_info)
        .build();
    let setup_reference_slot = vk::VideoReferenceSlotInfoKHR::builder()
        .picture_resource(&setup_picture_resource)
        .slot_index(plan.common.setup_reference_slot.slot_index)
        .push_next(&mut setup_h265_slot_info)
        .build();

    let decode_reference_resources = plan
        .common
        .decode_reference_slots
        .iter()
        .zip(image_views.decode_reference_image_views.iter().copied())
        .map(|(slot, image_view)| slot.resource.to_vk(image_view))
        .collect::<Vec<_>>();
    let decode_reference_std_infos = plan
        .picture
        .references
        .iter()
        .map(|reference| {
            native_vulkan_vulkanalia_h265_std_reference_info(
                reference.used_for_long_term_reference,
                reference.poc,
            )
        })
        .collect::<Vec<_>>();
    let mut decode_reference_dpb_infos = decode_reference_std_infos
        .iter()
        .map(|std_reference_info| {
            vk::VideoDecodeH265DpbSlotInfoKHR::builder()
                .std_reference_info(std_reference_info)
                .build()
        })
        .collect::<Vec<_>>();
    let mut decode_reference_slots = Vec::with_capacity(plan.common.decode_reference_slots.len());
    for (index, slot) in plan.common.decode_reference_slots.iter().enumerate() {
        decode_reference_slots.push(
            vk::VideoReferenceSlotInfoKHR::builder()
                .picture_resource(&decode_reference_resources[index])
                .slot_index(slot.slot_index)
                .push_next(&mut decode_reference_dpb_infos[index])
                .build(),
        );
    }

    let begin_reference_resources = plan
        .common
        .begin_reference_slots
        .iter()
        .zip(image_views.begin_reference_image_views.iter().copied())
        .map(|(slot, image_view)| slot.resource.to_vk(image_view))
        .collect::<Vec<_>>();
    let begin_reference_sources = plan
        .common
        .begin_reference_slots
        .iter()
        .map(|slot| native_vulkan_vulkanalia_h265_begin_reference_source(plan, slot))
        .collect::<Result<Vec<_>, _>>()?;
    let begin_reference_std_infos = begin_reference_sources
        .iter()
        .filter_map(|source| {
            source.map(|source| {
                native_vulkan_vulkanalia_h265_std_reference_info(
                    source.used_for_long_term_reference,
                    source.poc,
                )
            })
        })
        .collect::<Vec<_>>();
    let mut begin_reference_dpb_infos = begin_reference_std_infos
        .iter()
        .map(|std_reference_info| {
            vk::VideoDecodeH265DpbSlotInfoKHR::builder()
                .std_reference_info(std_reference_info)
                .build()
        })
        .collect::<Vec<_>>();
    let mut begin_reference_slots = Vec::with_capacity(plan.common.begin_reference_slots.len());
    let mut begin_dpb_index = 0usize;
    for (index, slot) in plan.common.begin_reference_slots.iter().enumerate() {
        let mut builder = vk::VideoReferenceSlotInfoKHR::builder()
            .picture_resource(&begin_reference_resources[index])
            .slot_index(slot.slot_index);
        if begin_reference_sources[index].is_some() {
            builder = builder.push_next(&mut begin_reference_dpb_infos[begin_dpb_index]);
            begin_dpb_index += 1;
        }
        begin_reference_slots.push(builder.build());
    }

    let (ref_pic_set_st_curr_before, ref_pic_set_st_curr_after, ref_pic_set_lt_curr) =
        native_vulkan_vulkanalia_h265_ref_pic_sets(&plan.picture.references)?;
    let std_picture_info = vk::video::StdVideoDecodeH265PictureInfo {
        flags: vk::video::StdVideoDecodeH265PictureInfoFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::video::StdVideoDecodeH265PictureInfoFlags::new_bitfield_1(
                h265_bool_u32(plan.picture.is_irap),
                h265_bool_u32(plan.picture.is_idr),
                h265_bool_u32(plan.picture.pps_curr_pic_ref_enabled_flag),
                h265_bool_u32(plan.picture.short_term_ref_pic_set_sps_flag),
            ),
            __bindgen_padding_0: [0; 3],
        },
        sps_video_parameter_set_id: plan.picture.sps_video_parameter_set_id,
        pps_seq_parameter_set_id: plan.picture.pps_seq_parameter_set_id,
        pps_pic_parameter_set_id: plan.picture.pps_pic_parameter_set_id,
        NumDeltaPocsOfRefRpsIdx: plan.picture.num_delta_pocs_of_ref_rps_idx,
        PicOrderCntVal: plan.picture.pic_order_cnt_val,
        NumBitsForSTRefPicSetInSlice: plan.picture.num_bits_for_st_ref_pic_set_in_slice,
        reserved: 0,
        RefPicSetStCurrBefore: ref_pic_set_st_curr_before,
        RefPicSetStCurrAfter: ref_pic_set_st_curr_after,
        RefPicSetLtCurr: ref_pic_set_lt_curr,
    };
    let mut h265_picture_info = vk::VideoDecodeH265PictureInfoKHR::builder()
        .std_picture_info(&std_picture_info)
        .slice_segment_offsets(&plan.picture.slice_segment_offsets)
        .build();
    let begin_info = vk::VideoBeginCodingInfoKHR::builder()
        .video_session(video_session)
        .video_session_parameters(session_parameters)
        .reference_slots(&begin_reference_slots)
        .build();
    let decode_info = vk::VideoDecodeInfoKHR::builder()
        .src_buffer(src_buffer)
        .src_buffer_offset(plan.common.src_buffer_offset)
        .src_buffer_range(plan.common.src_buffer_range)
        .dst_picture_resource(dst_picture_resource)
        .setup_reference_slot(&setup_reference_slot)
        .reference_slots(&decode_reference_slots)
        .push_next(&mut h265_picture_info)
        .build();

    Ok(use_submit_info(NativeVulkanVulkanaliaH265VkSubmitInfo {
        begin_info: &begin_info,
        decode_info: &decode_info,
        h265_picture_info: &h265_picture_info,
        std_picture_info: &std_picture_info,
        setup_reference_slot: &setup_reference_slot,
        begin_reference_slots: &begin_reference_slots,
        decode_reference_slots: &decode_reference_slots,
    }))
}

#[derive(Debug, Clone, Copy)]
struct NativeVulkanVulkanaliaH265ReferenceSource {
    poc: i32,
    used_for_long_term_reference: bool,
}

fn native_vulkan_vulkanalia_h265_begin_reference_source(
    plan: &NativeVulkanVulkanaliaH265DecodeSubmitPlan,
    slot: &NativeVulkanVulkanaliaReferenceSlotPlan,
) -> Result<Option<NativeVulkanVulkanaliaH265ReferenceSource>, String> {
    if !slot.codec_dpb_info_required {
        return Ok(None);
    }
    match slot.role {
        NativeVulkanVulkanaliaReferenceSlotRole::BeginInactive
        | NativeVulkanVulkanaliaReferenceSlotRole::SetupCurrent => {
            Ok(Some(NativeVulkanVulkanaliaH265ReferenceSource {
                poc: plan.picture.pic_order_cnt_val,
                used_for_long_term_reference: false,
            }))
        }
        NativeVulkanVulkanaliaReferenceSlotRole::DecodeReference => plan
            .picture
            .references
            .iter()
            .find(|reference| reference.slot_index == slot.slot_index)
            .map(|reference| {
                Some(NativeVulkanVulkanaliaH265ReferenceSource {
                    poc: reference.poc,
                    used_for_long_term_reference: reference.used_for_long_term_reference,
                })
            })
            .ok_or_else(|| {
                format!(
                    "Vulkanalia H.265 begin reference slot {} has no matching decode reference",
                    slot.slot_index
                )
            }),
    }
}

fn native_vulkan_vulkanalia_h265_std_reference_info(
    used_for_long_term_reference: bool,
    poc: i32,
) -> vk::video::StdVideoDecodeH265ReferenceInfo {
    vk::video::StdVideoDecodeH265ReferenceInfo {
        flags: vk::video::StdVideoDecodeH265ReferenceInfoFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::video::StdVideoDecodeH265ReferenceInfoFlags::new_bitfield_1(
                h265_bool_u32(used_for_long_term_reference),
                0,
            ),
            __bindgen_padding_0: [0; 3],
        },
        PicOrderCntVal: poc,
    }
}

fn native_vulkan_vulkanalia_h265_ref_pic_sets(
    references: &[NativeVulkanVulkanaliaH265ReferenceInfoPlan],
) -> Result<([u8; 8], [u8; 8], [u8; 8]), String> {
    let mut st_curr_before = [0xff; 8];
    let mut st_curr_after = [0xff; 8];
    let mut lt_curr = [0xff; 8];
    let mut before_index = 0usize;
    let mut after_index = 0usize;
    let mut lt_index = 0usize;
    for reference in references {
        let slot_index = u8::try_from(reference.slot_index).map_err(|_| {
            format!(
                "Vulkanalia H.265 reference slot {} cannot fit StdVideo ref-pic-set entry",
                reference.slot_index
            )
        })?;
        if reference.used_for_long_term_reference {
            if lt_index >= lt_curr.len() {
                return Err("Vulkanalia H.265 has more than 8 long-term references".to_owned());
            }
            lt_curr[lt_index] = slot_index;
            lt_index += 1;
        } else if reference.delta_poc < 0 {
            if before_index >= st_curr_before.len() {
                return Err("Vulkanalia H.265 has more than 8 before references".to_owned());
            }
            st_curr_before[before_index] = slot_index;
            before_index += 1;
        } else {
            if after_index >= st_curr_after.len() {
                return Err("Vulkanalia H.265 has more than 8 after references".to_owned());
            }
            st_curr_after[after_index] = slot_index;
            after_index += 1;
        }
    }
    Ok((st_curr_before, st_curr_after, lt_curr))
}

fn h265_bool_u32(value: bool) -> u32 {
    u32::from(value)
}

fn h265_u8(value: u32, label: &'static str) -> Result<u8, String> {
    u8::try_from(value).map_err(|_| format!("{label} exceeds u8: {value}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::native_vulkan::NativeVulkanH265DecodeReferenceSnapshot;

    #[test]
    fn h265_ready_prefix_plan_matches_ffmpeg_slot_shape() {
        let entry = NativeVulkanH265DecodeReferencePlanEntrySnapshot {
            access_unit_index: 7,
            pts_ms: Some(42),
            nal_type_label: Some("TRAIL_R"),
            current_poc: Some(12),
            planned_output_slot: 2,
            setup_slot_index: None,
            evicted_poc: None,
            references: vec![NativeVulkanH265DecodeReferenceSnapshot {
                delta_poc: -4,
                poc: 8,
                used_for_long_term_reference: false,
                available: true,
                source_access_unit_index: Some(3),
                dpb_slot: Some(1),
            }],
            available_reference_count: 1,
            missing_reference_count: 0,
            missing_reference_pocs: Vec::new(),
            ready_for_decode_submit: true,
        };
        let first_slice = test_h265_slice(false, false);

        let plan = native_vulkan_vulkanalia_h265_ready_prefix_decode_submit_plan(
            vk::Extent2D {
                width: 640,
                height: 368,
            },
            NativeVulkanVulkanaliaH265ParameterIds {
                sps_video_parameter_set_id: 0,
                pps_seq_parameter_set_id: 0,
                pps_pic_parameter_set_id: 0,
            },
            &entry,
            &first_slice,
            4096,
            8192,
            vec![first_slice.slice_segment_offset],
            true,
        )
        .unwrap();

        assert_eq!(plan.common.src_buffer_offset, 4096);
        assert_eq!(plan.common.src_buffer_range, 8192);
        assert_eq!(plan.common.setup_reference_slot.slot_index, 2);
        assert_eq!(plan.common.decode_reference_slots[0].slot_index, 1);
        assert_eq!(
            plan.common.begin_reference_slots.last().unwrap().slot_index,
            -1
        );
        assert_eq!(plan.picture.pic_order_cnt_val, 12);
        assert_eq!(plan.picture.references[0].poc, 8);
        assert!(plan.common.reset_control_recorded);
        assert!(plan.common.command_order.contains(&"cmd_decode_video_khr"));
    }

    #[test]
    fn h265_ready_prefix_plan_lowers_to_vulkanalia_decode_info() {
        let entry = NativeVulkanH265DecodeReferencePlanEntrySnapshot {
            access_unit_index: 9,
            pts_ms: Some(50),
            nal_type_label: Some("TRAIL_R"),
            current_poc: Some(20),
            planned_output_slot: 4,
            setup_slot_index: None,
            evicted_poc: None,
            references: vec![
                NativeVulkanH265DecodeReferenceSnapshot {
                    delta_poc: -4,
                    poc: 16,
                    used_for_long_term_reference: false,
                    available: true,
                    source_access_unit_index: Some(5),
                    dpb_slot: Some(1),
                },
                NativeVulkanH265DecodeReferenceSnapshot {
                    delta_poc: 2,
                    poc: 22,
                    used_for_long_term_reference: false,
                    available: true,
                    source_access_unit_index: Some(10),
                    dpb_slot: Some(3),
                },
            ],
            available_reference_count: 2,
            missing_reference_count: 0,
            missing_reference_pocs: Vec::new(),
            ready_for_decode_submit: true,
        };
        let first_slice = test_h265_slice(false, false);
        let plan = native_vulkan_vulkanalia_h265_ready_prefix_decode_submit_plan(
            vk::Extent2D {
                width: 1280,
                height: 720,
            },
            NativeVulkanVulkanaliaH265ParameterIds {
                sps_video_parameter_set_id: 0,
                pps_seq_parameter_set_id: 1,
                pps_pic_parameter_set_id: 2,
            },
            &entry,
            &first_slice,
            2048,
            4096,
            vec![12],
            false,
        )
        .unwrap();
        let image_views = NativeVulkanVulkanaliaDecodeImageViewBindings::repeated(
            vk::ImageView::default(),
            plan.common.begin_reference_slots.len(),
            plan.common.decode_reference_slots.len(),
        );

        native_vulkan_vulkanalia_h265_with_vk_submit_info(
            &plan,
            vk::VideoSessionKHR::default(),
            vk::VideoSessionParametersKHR::default(),
            vk::Buffer::default(),
            &image_views,
            |vk_info| {
                assert_eq!(vk_info.begin_info.reference_slot_count, 3);
                assert_eq!(vk_info.decode_info.src_buffer_offset, 2048);
                assert_eq!(vk_info.decode_info.src_buffer_range, 4096);
                assert_eq!(vk_info.decode_info.reference_slot_count, 2);
                assert!(!vk_info.decode_info.next.is_null());
                assert_eq!(vk_info.h265_picture_info.slice_segment_count, 1);
                assert_eq!(vk_info.std_picture_info.PicOrderCntVal, 20);
                assert_eq!(vk_info.std_picture_info.pps_seq_parameter_set_id, 1);
                assert_eq!(vk_info.std_picture_info.pps_pic_parameter_set_id, 2);
                assert_eq!(vk_info.std_picture_info.RefPicSetStCurrBefore[0], 1);
                assert_eq!(vk_info.std_picture_info.RefPicSetStCurrAfter[0], 3);
                assert_eq!(vk_info.std_picture_info.RefPicSetLtCurr[0], 0xff);
                assert_eq!(vk_info.setup_reference_slot.slot_index, 4);
                assert!(!vk_info.setup_reference_slot.next.is_null());
                assert_eq!(vk_info.decode_reference_slots[0].slot_index, 1);
                assert_eq!(vk_info.decode_reference_slots[1].slot_index, 3);
                assert_eq!(vk_info.begin_reference_slots.last().unwrap().slot_index, -1);
                assert!(!vk_info.begin_reference_slots.last().unwrap().next.is_null());
            },
        )
        .unwrap();
    }

    #[test]
    fn h265_ready_prefix_plan_rejects_missing_reference_slots() {
        let entry = NativeVulkanH265DecodeReferencePlanEntrySnapshot {
            access_unit_index: 8,
            pts_ms: None,
            nal_type_label: Some("TRAIL_R"),
            current_poc: Some(14),
            planned_output_slot: 2,
            setup_slot_index: None,
            evicted_poc: None,
            references: vec![NativeVulkanH265DecodeReferenceSnapshot {
                delta_poc: -2,
                poc: 12,
                used_for_long_term_reference: false,
                available: true,
                source_access_unit_index: Some(7),
                dpb_slot: None,
            }],
            available_reference_count: 1,
            missing_reference_count: 0,
            missing_reference_pocs: Vec::new(),
            ready_for_decode_submit: true,
        };

        let err = native_vulkan_vulkanalia_h265_ready_prefix_decode_submit_plan(
            vk::Extent2D {
                width: 640,
                height: 368,
            },
            NativeVulkanVulkanaliaH265ParameterIds {
                sps_video_parameter_set_id: 0,
                pps_seq_parameter_set_id: 0,
                pps_pic_parameter_set_id: 0,
            },
            &entry,
            &test_h265_slice(false, false),
            0,
            4096,
            vec![0],
            false,
        )
        .unwrap_err();
        assert!(err.contains("has no DPB slot"));
    }

    fn test_h265_slice(idr: bool, irap: bool) -> NativeVulkanH265AccessUnitSliceSnapshot {
        NativeVulkanH265AccessUnitSliceSnapshot {
            nal_type: if idr { 19 } else { 1 },
            nal_type_label: if idr { "IDR_W_RADL" } else { "TRAIL_R" },
            slice_segment_offset: 0,
            first_slice_segment_in_pic_flag: true,
            slice_type: if idr { 2 } else { 1 },
            pps_id: 0,
            pic_order_cnt_lsb: Some(0),
            short_term_ref_pic_set_sps_flag: false,
            short_term_ref_pic_set_idx: None,
            num_delta_pocs_of_ref_rps_idx: 0,
            num_bits_for_st_ref_pic_set_in_slice: 0,
            short_term_ref_pic_set: None,
            long_term_references: Vec::new(),
            idr,
            irap,
        }
    }
}
