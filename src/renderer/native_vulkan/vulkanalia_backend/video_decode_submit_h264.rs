#![allow(dead_code)]

use serde::Serialize;
use vulkanalia::vk::{self, HasBuilder};

use crate::renderer::native_vulkan::{
    NativeVulkanEncodedAccessUnitPayload, NativeVulkanH264AccessUnitSliceSnapshot,
    NativeVulkanH264DecodeReferencePlanEntrySnapshot, NativeVulkanH264ParameterSetSnapshot,
    NativeVulkanVideoSessionCodec,
};

use super::video_decode_submit::{
    NativeVulkanVulkanaliaDecodeImageViewBindings, NativeVulkanVulkanaliaDecodeSubmitPlan,
    NativeVulkanVulkanaliaPictureResourcePlan, NativeVulkanVulkanaliaReferenceSlotPlan,
    NativeVulkanVulkanaliaReferenceSlotRole,
};

const FFMPEG_H264_PICTURE_REFERENCE: &str = "references/ffmpeg/libavcodec/vulkan_h264.c";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(super) struct NativeVulkanVulkanaliaH264ParameterIds {
    pub seq_parameter_set_id: u8,
    pub pic_parameter_set_id: u8,
}

impl NativeVulkanVulkanaliaH264ParameterIds {
    pub(super) fn from_parameter_sets(
        parameter_sets: &NativeVulkanH264ParameterSetSnapshot,
    ) -> Result<Self, String> {
        Ok(Self {
            seq_parameter_set_id: h264_u8(parameter_sets.sps.id, "seq_parameter_set_id")?,
            pic_parameter_set_id: h264_u8(parameter_sets.pps.id, "pic_parameter_set_id")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct NativeVulkanVulkanaliaH264ReferenceInfoPlan {
    pub slot_index: i32,
    pub frame_num: u16,
    pub field_pic_flag: bool,
    pub bottom_field_flag: bool,
    pub used_for_long_term_reference: bool,
    pub long_term_frame_idx: Option<u16>,
    pub non_existing: bool,
    pub pic_order_cnt: [i32; 2],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct NativeVulkanVulkanaliaH264PictureInfoPlan {
    pub ffmpeg_reference: &'static str,
    pub field_pic_flag: bool,
    pub is_intra: bool,
    pub is_idr: bool,
    pub bottom_field_flag: bool,
    pub is_reference: bool,
    pub seq_parameter_set_id: u8,
    pub pic_parameter_set_id: u8,
    pub frame_num: u16,
    pub idr_pic_id: u16,
    pub pic_order_cnt: [i32; 2],
    pub slice_offsets: Vec<u32>,
    pub references: Vec<NativeVulkanVulkanaliaH264ReferenceInfoPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct NativeVulkanVulkanaliaH264DecodeSubmitPlan {
    pub common: NativeVulkanVulkanaliaDecodeSubmitPlan,
    pub picture: NativeVulkanVulkanaliaH264PictureInfoPlan,
}

pub(super) struct NativeVulkanVulkanaliaH264VkSubmitInfo<'a> {
    pub begin_info: &'a vk::VideoBeginCodingInfoKHR,
    pub decode_info: &'a vk::VideoDecodeInfoKHR,
    pub h264_picture_info: &'a vk::VideoDecodeH264PictureInfoKHR,
    pub std_picture_info: &'a vk::video::StdVideoDecodeH264PictureInfo,
    pub setup_reference_slot: &'a vk::VideoReferenceSlotInfoKHR,
    pub begin_reference_slots: &'a [vk::VideoReferenceSlotInfoKHR],
    pub decode_reference_slots: &'a [vk::VideoReferenceSlotInfoKHR],
}

#[derive(Debug)]
pub struct NativeVulkanVulkanaliaH264ReadyPrefixDecodeInput {
    pub parameter_sets: NativeVulkanH264ParameterSetSnapshot,
    pub requested_frame_count: u32,
    pub frames: Vec<NativeVulkanVulkanaliaH264ReadyPrefixFrameInput>,
}

#[derive(Debug)]
pub struct NativeVulkanVulkanaliaH264ReadyPrefixFrameInput {
    pub entry: NativeVulkanH264DecodeReferencePlanEntrySnapshot,
    pub first_slice: NativeVulkanH264AccessUnitSliceSnapshot,
    pub duration_ms: Option<u64>,
    pub access_unit_payload: NativeVulkanEncodedAccessUnitPayload,
    pub slice_offsets: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaH264ReadyPrefixCommandFrameSnapshot {
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
pub struct NativeVulkanVulkanaliaH264ReadyPrefixCommandSmokeSnapshot {
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
    pub input_payload_model: &'static str,
    pub src_buffer_total_bytes: u64,
    pub retained_frame_telemetry_limit: usize,
    pub retained_frame_telemetry_count: u32,
    pub frame_telemetry_retention_model: &'static str,
    pub max_src_buffer_range: u64,
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
    pub frames: Vec<NativeVulkanVulkanaliaH264ReadyPrefixCommandFrameSnapshot>,
}

pub(super) fn native_vulkan_vulkanalia_h264_ready_prefix_decode_submit_plan(
    extent: vk::Extent2D,
    parameter_ids: NativeVulkanVulkanaliaH264ParameterIds,
    entry: &NativeVulkanH264DecodeReferencePlanEntrySnapshot,
    first_slice: &NativeVulkanH264AccessUnitSliceSnapshot,
    src_buffer_offset: u64,
    src_buffer_range: u64,
    slice_offsets: Vec<u32>,
    reset_control_recorded: bool,
) -> Result<NativeVulkanVulkanaliaH264DecodeSubmitPlan, String> {
    if src_buffer_range == 0 {
        return Err("Vulkanalia H.264 decode submit requires non-empty bitstream range".to_owned());
    }
    if !entry.ready_for_decode_submit {
        return Err(format!(
            "Vulkanalia H.264 AU {} is not ready for decode submit: {}",
            entry.access_unit_index,
            entry
                .unsupported_reason
                .as_deref()
                .unwrap_or("missing references")
        ));
    }
    if slice_offsets.is_empty() {
        return Err(format!(
            "Vulkanalia H.264 AU {} has no slice offsets",
            entry.access_unit_index
        ));
    }
    let pic_order_cnt = entry.current_pic_order_cnt.ok_or_else(|| {
        format!(
            "Vulkanalia H.264 AU {} has no current picture order count",
            entry.access_unit_index
        )
    })?;
    let available_references = entry
        .references
        .iter()
        .filter(|reference| reference.available)
        .collect::<Vec<_>>();
    if available_references.len() as u32 != entry.available_reference_count
        || entry.missing_reference_count != 0
    {
        return Err(format!(
            "Vulkanalia H.264 AU {} still has missing references",
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
                    "Vulkanalia H.264 planned output slot {} exceeds i32",
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
                "Vulkanalia H.264 AU {} reference frame_num {} has no DPB slot",
                entry.access_unit_index, reference.frame_num
            )
        })?;
        let slot_index = i32::try_from(dpb_slot)
            .map_err(|_| format!("Vulkanalia H.264 DPB slot {dpb_slot} exceeds i32"))?;
        reference_infos.push(NativeVulkanVulkanaliaH264ReferenceInfoPlan {
            slot_index,
            frame_num: reference.frame_num,
            field_pic_flag: reference.field_pic_flag,
            bottom_field_flag: reference.bottom_field_flag,
            used_for_long_term_reference: reference.used_for_long_term_reference,
            long_term_frame_idx: reference.long_term_frame_idx,
            non_existing: reference.non_existing,
            pic_order_cnt: reference.pic_order_cnt,
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
        NativeVulkanVideoSessionCodec::H264High8,
        src_buffer_offset,
        src_buffer_range,
        dst_picture_resource,
        setup_reference_slot,
        begin_reference_slots,
        decode_reference_slots,
        reset_control_recorded,
    );
    let picture = NativeVulkanVulkanaliaH264PictureInfoPlan {
        ffmpeg_reference: FFMPEG_H264_PICTURE_REFERENCE,
        field_pic_flag: first_slice.field_pic_flag,
        is_intra: first_slice.is_intra,
        is_idr: first_slice.idr,
        bottom_field_flag: first_slice.bottom_field_flag,
        is_reference: first_slice.is_reference,
        seq_parameter_set_id: parameter_ids.seq_parameter_set_id,
        pic_parameter_set_id: parameter_ids.pic_parameter_set_id,
        frame_num: first_slice.frame_num,
        idr_pic_id: first_slice.idr_pic_id,
        pic_order_cnt,
        slice_offsets,
        references: reference_infos,
    };

    Ok(NativeVulkanVulkanaliaH264DecodeSubmitPlan { common, picture })
}

pub(super) fn native_vulkan_vulkanalia_h264_with_vk_submit_info<R>(
    plan: &NativeVulkanVulkanaliaH264DecodeSubmitPlan,
    video_session: vk::VideoSessionKHR,
    session_parameters: vk::VideoSessionParametersKHR,
    src_buffer: vk::Buffer,
    image_views: &NativeVulkanVulkanaliaDecodeImageViewBindings,
    use_submit_info: impl FnOnce(NativeVulkanVulkanaliaH264VkSubmitInfo<'_>) -> R,
) -> Result<R, String> {
    if image_views.begin_reference_image_views.len() != plan.common.begin_reference_slots.len() {
        return Err(format!(
            "Vulkanalia H.264 begin image-view count {} does not match begin slot count {}",
            image_views.begin_reference_image_views.len(),
            plan.common.begin_reference_slots.len()
        ));
    }
    if image_views.decode_reference_image_views.len() != plan.common.decode_reference_slots.len() {
        return Err(format!(
            "Vulkanalia H.264 decode image-view count {} does not match decode slot count {}",
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
    let std_setup_reference_info = native_vulkan_vulkanalia_h264_std_reference_info(
        plan.picture.frame_num,
        plan.picture.field_pic_flag,
        plan.picture.bottom_field_flag,
        false,
        None,
        false,
        plan.picture.pic_order_cnt,
    );
    let mut setup_h264_slot_info = vk::VideoDecodeH264DpbSlotInfoKHR::builder()
        .std_reference_info(&std_setup_reference_info)
        .build();
    let setup_reference_slot = vk::VideoReferenceSlotInfoKHR::builder()
        .picture_resource(&setup_picture_resource)
        .slot_index(plan.common.setup_reference_slot.slot_index)
        .push_next(&mut setup_h264_slot_info)
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
            native_vulkan_vulkanalia_h264_std_reference_info(
                reference.frame_num,
                reference.field_pic_flag,
                reference.bottom_field_flag,
                reference.used_for_long_term_reference,
                reference.long_term_frame_idx,
                reference.non_existing,
                reference.pic_order_cnt,
            )
        })
        .collect::<Vec<_>>();
    let mut decode_reference_dpb_infos = decode_reference_std_infos
        .iter()
        .map(|std_reference_info| {
            vk::VideoDecodeH264DpbSlotInfoKHR::builder()
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
        .map(|slot| native_vulkan_vulkanalia_h264_begin_reference_source(plan, slot))
        .collect::<Result<Vec<_>, _>>()?;
    let begin_reference_std_infos = begin_reference_sources
        .iter()
        .filter_map(|source| {
            source.map(native_vulkan_vulkanalia_h264_std_reference_info_from_source)
        })
        .collect::<Vec<_>>();
    let mut begin_reference_dpb_infos = begin_reference_std_infos
        .iter()
        .map(|std_reference_info| {
            vk::VideoDecodeH264DpbSlotInfoKHR::builder()
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

    let std_picture_info = vk::video::StdVideoDecodeH264PictureInfo {
        flags: vk::video::StdVideoDecodeH264PictureInfoFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::video::StdVideoDecodeH264PictureInfoFlags::new_bitfield_1(
                h264_bool_u32(plan.picture.field_pic_flag),
                h264_bool_u32(plan.picture.is_intra),
                h264_bool_u32(plan.picture.is_idr),
                h264_bool_u32(plan.picture.bottom_field_flag),
                h264_bool_u32(plan.picture.is_reference),
                0,
            ),
            __bindgen_padding_0: [0; 3],
        },
        seq_parameter_set_id: plan.picture.seq_parameter_set_id,
        pic_parameter_set_id: plan.picture.pic_parameter_set_id,
        reserved1: 0,
        reserved2: 0,
        frame_num: plan.picture.frame_num,
        idr_pic_id: plan.picture.idr_pic_id,
        PicOrderCnt: plan.picture.pic_order_cnt,
    };
    let mut h264_picture_info = vk::VideoDecodeH264PictureInfoKHR::builder()
        .std_picture_info(&std_picture_info)
        .slice_offsets(&plan.picture.slice_offsets)
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
        .push_next(&mut h264_picture_info)
        .build();

    Ok(use_submit_info(NativeVulkanVulkanaliaH264VkSubmitInfo {
        begin_info: &begin_info,
        decode_info: &decode_info,
        h264_picture_info: &h264_picture_info,
        std_picture_info: &std_picture_info,
        setup_reference_slot: &setup_reference_slot,
        begin_reference_slots: &begin_reference_slots,
        decode_reference_slots: &decode_reference_slots,
    }))
}

#[derive(Debug, Clone, Copy)]
struct NativeVulkanVulkanaliaH264ReferenceSource {
    frame_num: u16,
    field_pic_flag: bool,
    bottom_field_flag: bool,
    used_for_long_term_reference: bool,
    long_term_frame_idx: Option<u16>,
    non_existing: bool,
    pic_order_cnt: [i32; 2],
}

fn native_vulkan_vulkanalia_h264_begin_reference_source(
    plan: &NativeVulkanVulkanaliaH264DecodeSubmitPlan,
    slot: &NativeVulkanVulkanaliaReferenceSlotPlan,
) -> Result<Option<NativeVulkanVulkanaliaH264ReferenceSource>, String> {
    if !slot.codec_dpb_info_required {
        return Ok(None);
    }
    match slot.role {
        NativeVulkanVulkanaliaReferenceSlotRole::BeginInactive
        | NativeVulkanVulkanaliaReferenceSlotRole::SetupCurrent => {
            Ok(Some(NativeVulkanVulkanaliaH264ReferenceSource {
                frame_num: plan.picture.frame_num,
                field_pic_flag: plan.picture.field_pic_flag,
                bottom_field_flag: plan.picture.bottom_field_flag,
                used_for_long_term_reference: false,
                long_term_frame_idx: None,
                non_existing: false,
                pic_order_cnt: plan.picture.pic_order_cnt,
            }))
        }
        NativeVulkanVulkanaliaReferenceSlotRole::DecodeReference => plan
            .picture
            .references
            .iter()
            .find(|reference| reference.slot_index == slot.slot_index)
            .map(|reference| {
                Some(NativeVulkanVulkanaliaH264ReferenceSource {
                    frame_num: reference.frame_num,
                    field_pic_flag: reference.field_pic_flag,
                    bottom_field_flag: reference.bottom_field_flag,
                    used_for_long_term_reference: reference.used_for_long_term_reference,
                    long_term_frame_idx: reference.long_term_frame_idx,
                    non_existing: reference.non_existing,
                    pic_order_cnt: reference.pic_order_cnt,
                })
            })
            .ok_or_else(|| {
                format!(
                    "Vulkanalia H.264 begin reference slot {} has no matching decode reference",
                    slot.slot_index
                )
            }),
    }
}

fn native_vulkan_vulkanalia_h264_std_reference_info_from_source(
    source: NativeVulkanVulkanaliaH264ReferenceSource,
) -> vk::video::StdVideoDecodeH264ReferenceInfo {
    native_vulkan_vulkanalia_h264_std_reference_info(
        source.frame_num,
        source.field_pic_flag,
        source.bottom_field_flag,
        source.used_for_long_term_reference,
        source.long_term_frame_idx,
        source.non_existing,
        source.pic_order_cnt,
    )
}

fn native_vulkan_vulkanalia_h264_std_reference_info(
    frame_num: u16,
    field_pic_flag: bool,
    bottom_field_flag: bool,
    used_for_long_term_reference: bool,
    long_term_frame_idx: Option<u16>,
    non_existing: bool,
    pic_order_cnt: [i32; 2],
) -> vk::video::StdVideoDecodeH264ReferenceInfo {
    let top_field_flag = field_pic_flag && !bottom_field_flag;
    let bottom_field_flag = field_pic_flag && bottom_field_flag;
    vk::video::StdVideoDecodeH264ReferenceInfo {
        flags: vk::video::StdVideoDecodeH264ReferenceInfoFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::video::StdVideoDecodeH264ReferenceInfoFlags::new_bitfield_1(
                h264_bool_u32(top_field_flag),
                h264_bool_u32(bottom_field_flag),
                h264_bool_u32(used_for_long_term_reference),
                h264_bool_u32(non_existing),
            ),
            __bindgen_padding_0: [0; 3],
        },
        FrameNum: native_vulkan_vulkanalia_h264_reference_frame_num(
            frame_num,
            used_for_long_term_reference,
            long_term_frame_idx,
        ),
        reserved: 0,
        PicOrderCnt: pic_order_cnt,
    }
}

fn native_vulkan_vulkanalia_h264_reference_frame_num(
    frame_num: u16,
    used_for_long_term_reference: bool,
    long_term_frame_idx: Option<u16>,
) -> u16 {
    if used_for_long_term_reference {
        long_term_frame_idx.unwrap_or(frame_num)
    } else {
        frame_num
    }
}

fn h264_bool_u32(value: bool) -> u32 {
    u32::from(value)
}

fn h264_u8(value: u32, label: &'static str) -> Result<u8, String> {
    u8::try_from(value).map_err(|_| format!("{label} exceeds u8: {value}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::native_vulkan::NativeVulkanH264DecodeReferenceSnapshot;

    #[test]
    fn h264_ready_prefix_plan_matches_ffmpeg_slot_shape() {
        let entry = NativeVulkanH264DecodeReferencePlanEntrySnapshot {
            access_unit_index: 4,
            pts_ms: Some(42),
            nal_type_label: Some("non-idr-slice"),
            current_frame_num: Some(4),
            current_pic_order_cnt_val: Some(8),
            current_pic_order_cnt: Some([8, 8]),
            current_long_term_frame_idx: None,
            planned_output_slot: 2,
            setup_slot_index: None,
            evicted_frame_num: None,
            evicted_long_term_frame_idx: None,
            dropped_reference_frame_nums: Vec::new(),
            dropped_long_term_frame_indices: Vec::new(),
            inferred_non_existing_frame_nums: Vec::new(),
            inferred_non_existing_references: Vec::new(),
            inferred_dropped_reference_frame_nums: Vec::new(),
            inferred_dropped_long_term_frame_indices: Vec::new(),
            inferred_dropped_reference_slots: Vec::new(),
            long_term_reference_conversions: Vec::new(),
            dropped_reference_slots: Vec::new(),
            requested_reference_count: 1,
            references: vec![NativeVulkanH264DecodeReferenceSnapshot {
                frame_num: 3,
                field_pic_flag: false,
                bottom_field_flag: false,
                used_for_long_term_reference: false,
                long_term_frame_idx: None,
                long_term_pic_num: None,
                non_existing: false,
                pic_order_cnt_val: 6,
                pic_order_cnt: [6, 6],
                available: true,
                source_access_unit_index: Some(3),
                dpb_slot: Some(1),
            }],
            available_reference_count: 1,
            missing_reference_count: 0,
            unsupported_reason: None,
            ready_for_decode_submit: true,
        };

        let plan = native_vulkan_vulkanalia_h264_ready_prefix_decode_submit_plan(
            vk::Extent2D {
                width: 640,
                height: 368,
            },
            NativeVulkanVulkanaliaH264ParameterIds {
                seq_parameter_set_id: 0,
                pic_parameter_set_id: 0,
            },
            &entry,
            &test_h264_slice(false, false),
            1024,
            4096,
            vec![0],
            false,
        )
        .unwrap();

        assert_eq!(plan.common.src_buffer_offset, 1024);
        assert_eq!(plan.common.src_buffer_range, 4096);
        assert_eq!(plan.common.setup_reference_slot.slot_index, 2);
        assert_eq!(plan.common.decode_reference_slots[0].slot_index, 1);
        assert_eq!(
            plan.common.begin_reference_slots.last().unwrap().slot_index,
            -1
        );
        assert_eq!(plan.picture.frame_num, 4);
        assert_eq!(plan.picture.pic_order_cnt, [8, 8]);
        assert_eq!(plan.picture.references[0].frame_num, 3);
        assert!(!plan.common.reset_control_recorded);
        assert!(plan.common.command_order.contains(&"cmd_decode_video_khr"));
    }

    #[test]
    fn h264_ready_prefix_plan_lowers_to_vulkanalia_decode_info() {
        let mut entry = test_h264_entry_with_reference();
        entry.planned_output_slot = 4;
        entry.setup_slot_index = Some(4);
        let plan = native_vulkan_vulkanalia_h264_ready_prefix_decode_submit_plan(
            vk::Extent2D {
                width: 1280,
                height: 720,
            },
            NativeVulkanVulkanaliaH264ParameterIds {
                seq_parameter_set_id: 1,
                pic_parameter_set_id: 2,
            },
            &entry,
            &test_h264_slice(false, true),
            2048,
            8192,
            vec![16],
            true,
        )
        .unwrap();
        let image_views = NativeVulkanVulkanaliaDecodeImageViewBindings::repeated(
            vk::ImageView::default(),
            plan.common.begin_reference_slots.len(),
            plan.common.decode_reference_slots.len(),
        );

        native_vulkan_vulkanalia_h264_with_vk_submit_info(
            &plan,
            vk::VideoSessionKHR::default(),
            vk::VideoSessionParametersKHR::default(),
            vk::Buffer::default(),
            &image_views,
            |vk_info| {
                assert_eq!(vk_info.begin_info.reference_slot_count, 2);
                assert_eq!(vk_info.decode_info.src_buffer_offset, 2048);
                assert_eq!(vk_info.decode_info.src_buffer_range, 8192);
                assert_eq!(vk_info.decode_info.reference_slot_count, 1);
                assert!(!vk_info.decode_info.next.is_null());
                assert_eq!(vk_info.h264_picture_info.slice_count, 1);
                assert_eq!(vk_info.std_picture_info.frame_num, 4);
                assert_eq!(vk_info.std_picture_info.seq_parameter_set_id, 1);
                assert_eq!(vk_info.std_picture_info.pic_parameter_set_id, 2);
                assert_eq!(vk_info.std_picture_info.PicOrderCnt, [8, 8]);
                assert_eq!(vk_info.setup_reference_slot.slot_index, 4);
                assert!(!vk_info.setup_reference_slot.next.is_null());
                assert_eq!(vk_info.decode_reference_slots[0].slot_index, 1);
                assert!(!vk_info.decode_reference_slots[0].next.is_null());
                assert_eq!(vk_info.begin_reference_slots[0].slot_index, 1);
                assert!(!vk_info.begin_reference_slots[0].next.is_null());
                assert_eq!(vk_info.begin_reference_slots[1].slot_index, -1);
                assert!(!vk_info.begin_reference_slots[1].next.is_null());
            },
        )
        .unwrap();
    }

    fn test_h264_entry_with_reference() -> NativeVulkanH264DecodeReferencePlanEntrySnapshot {
        NativeVulkanH264DecodeReferencePlanEntrySnapshot {
            access_unit_index: 4,
            pts_ms: Some(50),
            nal_type_label: Some("non-idr-slice"),
            current_frame_num: Some(4),
            current_pic_order_cnt_val: Some(8),
            current_pic_order_cnt: Some([8, 8]),
            current_long_term_frame_idx: None,
            planned_output_slot: 2,
            setup_slot_index: None,
            evicted_frame_num: None,
            evicted_long_term_frame_idx: None,
            dropped_reference_frame_nums: Vec::new(),
            dropped_long_term_frame_indices: Vec::new(),
            inferred_non_existing_frame_nums: Vec::new(),
            inferred_non_existing_references: Vec::new(),
            inferred_dropped_reference_frame_nums: Vec::new(),
            inferred_dropped_long_term_frame_indices: Vec::new(),
            inferred_dropped_reference_slots: Vec::new(),
            long_term_reference_conversions: Vec::new(),
            dropped_reference_slots: Vec::new(),
            requested_reference_count: 1,
            references: vec![NativeVulkanH264DecodeReferenceSnapshot {
                frame_num: 3,
                field_pic_flag: false,
                bottom_field_flag: false,
                used_for_long_term_reference: false,
                long_term_frame_idx: None,
                long_term_pic_num: None,
                non_existing: false,
                pic_order_cnt_val: 6,
                pic_order_cnt: [6, 6],
                available: true,
                source_access_unit_index: Some(3),
                dpb_slot: Some(1),
            }],
            available_reference_count: 1,
            missing_reference_count: 0,
            unsupported_reason: None,
            ready_for_decode_submit: true,
        }
    }

    fn test_h264_slice(idr: bool, reference: bool) -> NativeVulkanH264AccessUnitSliceSnapshot {
        NativeVulkanH264AccessUnitSliceSnapshot {
            nal_type: if idr { 5 } else { 1 },
            nal_type_label: if idr { "idr-slice" } else { "non-idr-slice" },
            nal_ref_idc: if reference { 1 } else { 0 },
            first_mb_in_slice: 0,
            first_slice_segment_in_pic_flag: true,
            slice_type: if idr { 2 } else { 0 },
            slice_type_normalized: if idr { 2 } else { 0 },
            pps_id: 0,
            frame_num: 4,
            idr_pic_id: if idr { 7 } else { 0 },
            num_ref_idx_l0_active_minus1: Some(0),
            num_ref_idx_l1_active_minus1: None,
            ref_pic_list_modification_l0: false,
            ref_pic_list_modifications_l0: Vec::new(),
            ref_pic_list_modification_l1: false,
            ref_pic_list_modifications_l1: Vec::new(),
            adaptive_ref_pic_marking_mode_flag: false,
            memory_management_control_operations: Vec::new(),
            field_pic_flag: false,
            bottom_field_flag: false,
            is_reference: reference,
            is_intra: idr,
            is_p: !idr,
            is_b: false,
            long_term_reference_flag: false,
            pic_order_cnt: [8, 8],
            slice_offsets: vec![0],
            idr,
            irap: idr,
        }
    }
}
