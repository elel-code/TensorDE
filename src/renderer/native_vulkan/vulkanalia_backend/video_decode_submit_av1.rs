#![allow(dead_code)]

use std::ptr;

use serde::Serialize;
use vulkanalia::vk::{self, HasBuilder};

use crate::renderer::native_vulkan::{
    NativeVulkanAv1DecodeReferencePlanEntrySnapshot, NativeVulkanAv1SequenceHeaderSnapshot,
    NativeVulkanVideoSessionCodec,
};

use super::video_decode_submit::{
    NativeVulkanVulkanaliaDecodeImageViewBindings, NativeVulkanVulkanaliaDecodeSubmitPlan,
    NativeVulkanVulkanaliaPictureResourcePlan, NativeVulkanVulkanaliaReferenceSlotPlan,
    NativeVulkanVulkanaliaReferenceSlotRole, NativeVulkanVulkanaliaStreamingDecodeTimingSnapshot,
};

const FFMPEG_AV1_PICTURE_REFERENCE: &str = "references/ffmpeg/libavcodec/vulkan_av1.c";
const AV1_REFERENCE_NAME_COUNT: usize = vk::MAX_VIDEO_AV1_REFERENCES_PER_FRAME_KHR;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaAv1DecodeFrameBatchInput {
    pub codec: NativeVulkanVideoSessionCodec,
    pub sequence_header: NativeVulkanAv1SequenceHeaderSnapshot,
    pub requested_frame_count: u32,
    pub frames: Vec<NativeVulkanVulkanaliaAv1DecodeFrameInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaAv1DecodeFrameInput {
    pub entry: NativeVulkanAv1DecodeReferencePlanEntrySnapshot,
    pub frame: NativeVulkanVulkanaliaAv1FrameSubmitInput,
    pub pts_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    pub access_unit_payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaAv1CommandFrameSnapshot {
    pub frame_index: u32,
    pub temporal_unit_index: u32,
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
    pub tile_count: u32,
    pub tile_offsets: Vec<u32>,
    pub tile_sizes: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaAv1CommandSmokeSnapshot {
    pub requested_frame_count: u32,
    pub recorded_frame_count: u32,
    pub submitted_frame_count: u32,
    pub displayed_frame_count: u32,
    pub show_existing_frame_count: u32,
    pub hidden_frame_count: u32,
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
    pub tile_count: u32,
    pub tile_offsets: Vec<u32>,
    pub tile_sizes: Vec<u32>,
    pub frames: Vec<NativeVulkanVulkanaliaAv1CommandFrameSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaAv1FrameSubmitInput {
    pub temporal_unit_index: u32,
    pub frame_header_offset_for_vulkan: u32,
    pub tile_offsets: Vec<u32>,
    pub tile_sizes: Vec<u32>,
    pub tile_info: NativeVulkanVulkanaliaAv1TileInfoPlan,
    pub frame_type: u8,
    pub show_existing_frame: bool,
    pub show_frame: bool,
    pub error_resilient_mode: bool,
    pub disable_cdf_update: bool,
    pub disable_frame_end_update_cdf: bool,
    pub use_superres: bool,
    pub render_and_frame_size_different: bool,
    pub allow_screen_content_tools: bool,
    pub is_filter_switchable: bool,
    pub force_integer_mv: bool,
    pub frame_size_override_flag: bool,
    pub allow_intrabc: bool,
    pub frame_refs_short_signaling: bool,
    pub allow_high_precision_mv: bool,
    pub is_motion_mode_switchable: bool,
    pub use_ref_frame_mvs: bool,
    pub allow_warped_motion: bool,
    pub reduced_tx_set: bool,
    pub reference_select: bool,
    pub skip_mode_present: bool,
    pub delta_q_present: bool,
    pub delta_lf_present: bool,
    pub delta_lf_multi: bool,
    pub apply_grain: bool,
    pub current_frame_id: Option<u32>,
    pub order_hint: Option<u8>,
    pub primary_ref_frame: Option<u8>,
    pub refresh_frame_flags: u8,
    pub interpolation_filter: u32,
    pub tx_mode_select: bool,
    pub delta_q_res: u8,
    pub delta_lf_res: u8,
    pub skip_mode_frame: [u8; 2],
    pub coded_denom: u8,
    pub picture_order_hints: [u8; 8],
    pub expected_frame_ids: Vec<u32>,
    pub reference_name_slot_indices: Vec<i32>,
    pub quantization: NativeVulkanVulkanaliaAv1QuantizationPlan,
    pub segmentation: NativeVulkanVulkanaliaAv1SegmentationPlan,
    pub loop_filter: NativeVulkanVulkanaliaAv1LoopFilterPlan,
    pub cdef: NativeVulkanVulkanaliaAv1CdefPlan,
    pub loop_restoration: NativeVulkanVulkanaliaAv1LoopRestorationPlan,
    pub global_motion: NativeVulkanVulkanaliaAv1GlobalMotionPlan,
    pub setup_reference: NativeVulkanVulkanaliaAv1ReferenceInfoPlan,
    pub references: Vec<NativeVulkanVulkanaliaAv1ReferenceInfoPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaAv1TileInfoPlan {
    pub uniform_tile_spacing_flag: bool,
    pub tile_columns: u8,
    pub tile_rows: u8,
    pub context_update_tile_id: u16,
    pub tile_size_bytes_minus_1: u8,
    pub mi_col_starts: Vec<u16>,
    pub mi_row_starts: Vec<u16>,
    pub width_in_sbs_minus_1: Vec<u16>,
    pub height_in_sbs_minus_1: Vec<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaAv1QuantizationPlan {
    pub using_qmatrix: bool,
    pub diff_uv_delta: bool,
    pub base_q_idx: u8,
    pub delta_q_y_dc: i8,
    pub delta_q_u_dc: i8,
    pub delta_q_u_ac: i8,
    pub delta_q_v_dc: i8,
    pub delta_q_v_ac: i8,
    pub qm_y: u8,
    pub qm_u: u8,
    pub qm_v: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaAv1SegmentationPlan {
    pub enabled: bool,
    pub update_map: bool,
    pub temporal_update: bool,
    pub update_data: bool,
    pub feature_enabled: [u8; 8],
    pub feature_data: [[i16; 8]; 8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaAv1LoopFilterPlan {
    pub delta_enabled: bool,
    pub delta_update: bool,
    pub level: [u8; 4],
    pub sharpness: u8,
    pub update_ref_delta: u8,
    pub ref_deltas: [i8; 8],
    pub update_mode_delta: u8,
    pub mode_deltas: [i8; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaAv1CdefPlan {
    pub damping_minus_3: u8,
    pub bits: u8,
    pub y_pri_strength: [u8; 8],
    pub y_sec_strength: [u8; 8],
    pub uv_pri_strength: [u8; 8],
    pub uv_sec_strength: [u8; 8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaAv1LoopRestorationPlan {
    pub frame_restoration_type: [u32; 3],
    pub loop_restoration_size: [u16; 3],
    pub uses_lr: bool,
    pub uses_chroma_lr: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaAv1GlobalMotionPlan {
    pub gm_type: [u8; 8],
    pub gm_params: [[i32; 6]; 8],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaAv1ReferenceInfoPlan {
    pub slot_index: i32,
    pub frame_type: u8,
    pub ref_frame_sign_bias: u8,
    pub order_hint: u8,
    pub saved_order_hints: [u8; 8],
    pub disable_frame_end_update_cdf: bool,
    pub segmentation_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct NativeVulkanVulkanaliaAv1PictureInfoPlan {
    pub ffmpeg_reference: &'static str,
    pub frame_header_offset: u32,
    pub tile_offsets: Vec<u32>,
    pub tile_sizes: Vec<u32>,
    pub tile_info: NativeVulkanVulkanaliaAv1TileInfoPlan,
    pub frame_type: u8,
    pub show_frame: bool,
    pub error_resilient_mode: bool,
    pub disable_cdf_update: bool,
    pub disable_frame_end_update_cdf: bool,
    pub use_superres: bool,
    pub render_and_frame_size_different: bool,
    pub allow_screen_content_tools: bool,
    pub is_filter_switchable: bool,
    pub force_integer_mv: bool,
    pub frame_size_override_flag: bool,
    pub allow_intrabc: bool,
    pub frame_refs_short_signaling: bool,
    pub allow_high_precision_mv: bool,
    pub is_motion_mode_switchable: bool,
    pub use_ref_frame_mvs: bool,
    pub allow_warped_motion: bool,
    pub reduced_tx_set: bool,
    pub reference_select: bool,
    pub skip_mode_present: bool,
    pub delta_q_present: bool,
    pub delta_lf_present: bool,
    pub delta_lf_multi: bool,
    pub apply_grain: bool,
    pub current_frame_id: u32,
    pub order_hint: u8,
    pub primary_ref_frame: u8,
    pub refresh_frame_flags: u8,
    pub interpolation_filter: u32,
    pub tx_mode_select: bool,
    pub delta_q_res: u8,
    pub delta_lf_res: u8,
    pub skip_mode_frame: [u8; 2],
    pub coded_denom: u8,
    pub picture_order_hints: [u8; 8],
    pub expected_frame_ids: [u32; 8],
    pub reference_name_slot_indices: [i32; AV1_REFERENCE_NAME_COUNT],
    pub quantization: NativeVulkanVulkanaliaAv1QuantizationPlan,
    pub segmentation: NativeVulkanVulkanaliaAv1SegmentationPlan,
    pub loop_filter: NativeVulkanVulkanaliaAv1LoopFilterPlan,
    pub cdef: NativeVulkanVulkanaliaAv1CdefPlan,
    pub loop_restoration: NativeVulkanVulkanaliaAv1LoopRestorationPlan,
    pub global_motion: NativeVulkanVulkanaliaAv1GlobalMotionPlan,
    pub setup_reference: NativeVulkanVulkanaliaAv1ReferenceInfoPlan,
    pub references: Vec<NativeVulkanVulkanaliaAv1ReferenceInfoPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct NativeVulkanVulkanaliaAv1DecodeSubmitPlan {
    pub common: NativeVulkanVulkanaliaDecodeSubmitPlan,
    pub picture: NativeVulkanVulkanaliaAv1PictureInfoPlan,
}

pub(super) struct NativeVulkanVulkanaliaAv1VkSubmitInfo<'a> {
    pub begin_info: &'a vk::VideoBeginCodingInfoKHR,
    pub decode_info: &'a vk::VideoDecodeInfoKHR,
    pub av1_picture_info: &'a vk::VideoDecodeAV1PictureInfoKHR,
    pub std_picture_info: &'a vk::video::StdVideoDecodeAV1PictureInfo,
    pub setup_reference_slot: &'a vk::VideoReferenceSlotInfoKHR,
    pub begin_reference_slots: &'a [vk::VideoReferenceSlotInfoKHR],
    pub decode_reference_slots: &'a [vk::VideoReferenceSlotInfoKHR],
}

pub(super) fn native_vulkan_vulkanalia_av1_decode_submit_plan(
    extent: vk::Extent2D,
    codec: NativeVulkanVideoSessionCodec,
    entry: &NativeVulkanAv1DecodeReferencePlanEntrySnapshot,
    frame: NativeVulkanVulkanaliaAv1FrameSubmitInput,
    src_buffer_offset: u64,
    src_buffer_range: u64,
    reset_control_recorded: bool,
) -> Result<NativeVulkanVulkanaliaAv1DecodeSubmitPlan, String> {
    if !matches!(
        codec,
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10
    ) {
        return Err("Vulkanalia AV1 decode submit requires an AV1 session codec".to_owned());
    }
    if src_buffer_range == 0 {
        return Err("Vulkanalia AV1 decode submit requires non-empty bitstream range".to_owned());
    }
    if frame.show_existing_frame {
        return Err(format!(
            "Vulkanalia AV1 TU {} show_existing_frame has no decode payload",
            entry.temporal_unit_index
        ));
    }
    if !entry.ready_for_decode_submit {
        return Err(format!(
            "Vulkanalia AV1 TU {} is not ready for decode submit: {}",
            entry.temporal_unit_index,
            entry
                .unsupported_reason
                .as_deref()
                .unwrap_or("missing references or submit fields")
        ));
    }
    if frame.tile_offsets.is_empty() || frame.tile_offsets.len() != frame.tile_sizes.len() {
        return Err(format!(
            "Vulkanalia AV1 TU {} has invalid tile offsets/sizes",
            entry.temporal_unit_index
        ));
    }
    let output_slot = entry.output_slot.ok_or_else(|| {
        format!(
            "Vulkanalia AV1 TU {} has no planned output slot",
            entry.temporal_unit_index
        )
    })?;
    let setup_slot_index = i32::try_from(output_slot)
        .map_err(|_| format!("Vulkanalia AV1 output slot {output_slot} exceeds i32"))?;
    let dst_picture_resource = NativeVulkanVulkanaliaPictureResourcePlan::new(extent, output_slot);
    let setup_reference_slot = NativeVulkanVulkanaliaReferenceSlotPlan::setup_current(
        setup_slot_index,
        dst_picture_resource.clone(),
    );

    let mut decode_reference_slot_ids = entry
        .decode_reference_slots
        .iter()
        .filter_map(|slot| u32::try_from(*slot).ok())
        .collect::<Vec<_>>();
    decode_reference_slot_ids.sort_unstable();
    decode_reference_slot_ids.dedup();
    if decode_reference_slot_ids.len() > frame.references.len() {
        return Err(format!(
            "Vulkanalia AV1 TU {} has {} decode slots but only {} reference infos",
            entry.temporal_unit_index,
            decode_reference_slot_ids.len(),
            frame.references.len()
        ));
    }
    let decode_reference_slots = decode_reference_slot_ids
        .iter()
        .map(|slot| {
            let slot_index = i32::try_from(*slot)
                .map_err(|_| format!("Vulkanalia AV1 DPB slot {slot} exceeds i32"))?;
            Ok(NativeVulkanVulkanaliaReferenceSlotPlan::decode_reference(
                slot_index,
                NativeVulkanVulkanaliaPictureResourcePlan::new(extent, *slot),
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut begin_reference_slots = decode_reference_slots.clone();
    begin_reference_slots.push(NativeVulkanVulkanaliaReferenceSlotPlan::begin_inactive(
        dst_picture_resource.clone(),
    ));

    let common = NativeVulkanVulkanaliaDecodeSubmitPlan::new(
        codec,
        src_buffer_offset,
        src_buffer_range,
        dst_picture_resource,
        setup_reference_slot,
        begin_reference_slots,
        decode_reference_slots,
        reset_control_recorded,
    );
    let picture = NativeVulkanVulkanaliaAv1PictureInfoPlan {
        ffmpeg_reference: FFMPEG_AV1_PICTURE_REFERENCE,
        frame_header_offset: frame.frame_header_offset_for_vulkan,
        tile_offsets: frame.tile_offsets,
        tile_sizes: frame.tile_sizes,
        tile_info: frame.tile_info,
        frame_type: frame.frame_type,
        show_frame: frame.show_frame,
        error_resilient_mode: frame.error_resilient_mode,
        disable_cdf_update: frame.disable_cdf_update,
        disable_frame_end_update_cdf: frame.disable_frame_end_update_cdf,
        use_superres: frame.use_superres,
        render_and_frame_size_different: frame.render_and_frame_size_different,
        allow_screen_content_tools: frame.allow_screen_content_tools,
        is_filter_switchable: frame.is_filter_switchable,
        force_integer_mv: frame.force_integer_mv,
        frame_size_override_flag: frame.frame_size_override_flag,
        allow_intrabc: frame.allow_intrabc,
        frame_refs_short_signaling: frame.frame_refs_short_signaling,
        allow_high_precision_mv: frame.allow_high_precision_mv,
        is_motion_mode_switchable: frame.is_motion_mode_switchable,
        use_ref_frame_mvs: frame.use_ref_frame_mvs,
        allow_warped_motion: frame.allow_warped_motion,
        reduced_tx_set: frame.reduced_tx_set,
        reference_select: frame.reference_select,
        skip_mode_present: frame.skip_mode_present,
        delta_q_present: frame.delta_q_present,
        delta_lf_present: frame.delta_lf_present,
        delta_lf_multi: frame.delta_lf_multi,
        apply_grain: frame.apply_grain,
        current_frame_id: frame.current_frame_id.unwrap_or(0),
        order_hint: frame.order_hint.unwrap_or(0),
        primary_ref_frame: frame.primary_ref_frame.unwrap_or(7),
        refresh_frame_flags: frame.refresh_frame_flags,
        interpolation_filter: frame.interpolation_filter,
        tx_mode_select: frame.tx_mode_select,
        delta_q_res: frame.delta_q_res,
        delta_lf_res: frame.delta_lf_res,
        skip_mode_frame: frame.skip_mode_frame,
        coded_denom: frame.coded_denom,
        picture_order_hints: frame.picture_order_hints,
        expected_frame_ids: av1_expected_frame_ids_array(&frame.expected_frame_ids),
        reference_name_slot_indices: av1_reference_name_slot_indices(
            &frame.reference_name_slot_indices,
            &entry.decode_reference_slots,
        ),
        quantization: frame.quantization,
        segmentation: frame.segmentation,
        loop_filter: frame.loop_filter,
        cdef: frame.cdef,
        loop_restoration: frame.loop_restoration,
        global_motion: frame.global_motion,
        setup_reference: frame.setup_reference,
        references: frame
            .references
            .into_iter()
            .take(decode_reference_slot_ids.len())
            .collect(),
    };

    Ok(NativeVulkanVulkanaliaAv1DecodeSubmitPlan { common, picture })
}

pub(super) fn native_vulkan_vulkanalia_av1_with_vk_submit_info<R>(
    plan: &NativeVulkanVulkanaliaAv1DecodeSubmitPlan,
    video_session: vk::VideoSessionKHR,
    session_parameters: vk::VideoSessionParametersKHR,
    src_buffer: vk::Buffer,
    image_views: &NativeVulkanVulkanaliaDecodeImageViewBindings,
    use_submit_info: impl FnOnce(NativeVulkanVulkanaliaAv1VkSubmitInfo<'_>) -> R,
) -> Result<R, String> {
    if image_views.begin_reference_image_views.len() != plan.common.begin_reference_slots.len() {
        return Err(format!(
            "Vulkanalia AV1 begin image-view count {} does not match begin slot count {}",
            image_views.begin_reference_image_views.len(),
            plan.common.begin_reference_slots.len()
        ));
    }
    if image_views.decode_reference_image_views.len() != plan.common.decode_reference_slots.len() {
        return Err(format!(
            "Vulkanalia AV1 decode image-view count {} does not match decode slot count {}",
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
    let std_setup_reference_info = av1_std_reference_info(&plan.picture.setup_reference)?;
    let mut setup_av1_slot_info = vk::VideoDecodeAV1DpbSlotInfoKHR::builder()
        .std_reference_info(&std_setup_reference_info)
        .build();
    let setup_reference_slot = vk::VideoReferenceSlotInfoKHR::builder()
        .picture_resource(&setup_picture_resource)
        .slot_index(plan.common.setup_reference_slot.slot_index)
        .push_next(&mut setup_av1_slot_info)
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
        .map(av1_std_reference_info)
        .collect::<Result<Vec<_>, _>>()?;
    let mut decode_reference_dpb_infos = decode_reference_std_infos
        .iter()
        .map(|std_reference_info| {
            vk::VideoDecodeAV1DpbSlotInfoKHR::builder()
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
        .map(|slot| native_vulkan_vulkanalia_av1_begin_reference_source(plan, slot))
        .collect::<Result<Vec<_>, _>>()?;
    let begin_reference_std_infos = begin_reference_sources
        .iter()
        .filter_map(|source| source.as_ref())
        .map(av1_std_reference_info)
        .collect::<Result<Vec<_>, _>>()?;
    let mut begin_reference_dpb_infos = begin_reference_std_infos
        .iter()
        .map(|std_reference_info| {
            vk::VideoDecodeAV1DpbSlotInfoKHR::builder()
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

    let tile_info = &plan.picture.tile_info;
    let std_tile_info = vk::video::StdVideoAV1TileInfo {
        flags: vk::video::StdVideoAV1TileInfoFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::video::StdVideoAV1TileInfoFlags::new_bitfield_1(
                bool_u32(tile_info.uniform_tile_spacing_flag),
                0,
            ),
        },
        TileCols: tile_info.tile_columns,
        TileRows: tile_info.tile_rows,
        context_update_tile_id: tile_info.context_update_tile_id,
        tile_size_bytes_minus_1: tile_info.tile_size_bytes_minus_1,
        reserved1: [0; 7],
        pMiColStarts: tile_info.mi_col_starts.as_ptr(),
        pMiRowStarts: tile_info.mi_row_starts.as_ptr(),
        pWidthInSbsMinus1: tile_info.width_in_sbs_minus_1.as_ptr(),
        pHeightInSbsMinus1: tile_info.height_in_sbs_minus_1.as_ptr(),
    };
    let quantization = plan.picture.quantization;
    let std_quantization = vk::video::StdVideoAV1Quantization {
        flags: vk::video::StdVideoAV1QuantizationFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::video::StdVideoAV1QuantizationFlags::new_bitfield_1(
                bool_u32(quantization.using_qmatrix),
                bool_u32(quantization.diff_uv_delta),
                0,
            ),
        },
        base_q_idx: quantization.base_q_idx,
        DeltaQYDc: quantization.delta_q_y_dc,
        DeltaQUDc: quantization.delta_q_u_dc,
        DeltaQUAc: quantization.delta_q_u_ac,
        DeltaQVDc: quantization.delta_q_v_dc,
        DeltaQVAc: quantization.delta_q_v_ac,
        qm_y: quantization.qm_y,
        qm_u: quantization.qm_u,
        qm_v: quantization.qm_v,
    };
    let segmentation = plan.picture.segmentation;
    let std_segmentation = vk::video::StdVideoAV1Segmentation {
        FeatureEnabled: segmentation.feature_enabled,
        FeatureData: segmentation.feature_data,
    };
    let loop_filter = plan.picture.loop_filter;
    let std_loop_filter = vk::video::StdVideoAV1LoopFilter {
        flags: vk::video::StdVideoAV1LoopFilterFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::video::StdVideoAV1LoopFilterFlags::new_bitfield_1(
                bool_u32(loop_filter.delta_enabled),
                bool_u32(loop_filter.delta_update),
                0,
            ),
        },
        loop_filter_level: loop_filter.level,
        loop_filter_sharpness: loop_filter.sharpness,
        update_ref_delta: loop_filter.update_ref_delta,
        loop_filter_ref_deltas: loop_filter.ref_deltas,
        update_mode_delta: loop_filter.update_mode_delta,
        loop_filter_mode_deltas: loop_filter.mode_deltas,
    };
    let cdef = plan.picture.cdef;
    let std_cdef = vk::video::StdVideoAV1CDEF {
        cdef_damping_minus_3: cdef.damping_minus_3,
        cdef_bits: cdef.bits,
        cdef_y_pri_strength: cdef.y_pri_strength,
        cdef_y_sec_strength: cdef.y_sec_strength,
        cdef_uv_pri_strength: cdef.uv_pri_strength,
        cdef_uv_sec_strength: cdef.uv_sec_strength,
    };
    let loop_restoration = plan.picture.loop_restoration;
    let std_loop_restoration = vk::video::StdVideoAV1LoopRestoration {
        FrameRestorationType: [
            av1_restoration_type(loop_restoration.frame_restoration_type[0])?,
            av1_restoration_type(loop_restoration.frame_restoration_type[1])?,
            av1_restoration_type(loop_restoration.frame_restoration_type[2])?,
        ],
        LoopRestorationSize: loop_restoration.loop_restoration_size,
    };
    let global_motion = plan.picture.global_motion;
    let std_global_motion = vk::video::StdVideoAV1GlobalMotion {
        GmType: global_motion.gm_type,
        gm_params: global_motion.gm_params,
    };
    let std_picture_info = vk::video::StdVideoDecodeAV1PictureInfo {
        flags: vk::video::StdVideoDecodeAV1PictureInfoFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::video::StdVideoDecodeAV1PictureInfoFlags::new_bitfield_1(
                bool_u32(plan.picture.error_resilient_mode),
                bool_u32(plan.picture.disable_cdf_update),
                bool_u32(plan.picture.use_superres),
                bool_u32(plan.picture.render_and_frame_size_different),
                bool_u32(plan.picture.allow_screen_content_tools),
                bool_u32(plan.picture.is_filter_switchable),
                bool_u32(plan.picture.force_integer_mv),
                bool_u32(plan.picture.frame_size_override_flag),
                0,
                bool_u32(plan.picture.allow_intrabc),
                bool_u32(plan.picture.frame_refs_short_signaling),
                bool_u32(plan.picture.allow_high_precision_mv),
                bool_u32(plan.picture.is_motion_mode_switchable),
                bool_u32(plan.picture.use_ref_frame_mvs),
                bool_u32(plan.picture.disable_frame_end_update_cdf),
                bool_u32(plan.picture.allow_warped_motion),
                bool_u32(plan.picture.reduced_tx_set),
                bool_u32(plan.picture.reference_select),
                bool_u32(plan.picture.skip_mode_present),
                bool_u32(plan.picture.delta_q_present),
                bool_u32(plan.picture.delta_lf_present),
                bool_u32(plan.picture.delta_lf_multi),
                bool_u32(plan.picture.segmentation.enabled),
                bool_u32(plan.picture.segmentation.update_map),
                bool_u32(plan.picture.segmentation.temporal_update),
                bool_u32(plan.picture.segmentation.update_data),
                bool_u32(plan.picture.loop_restoration.uses_lr),
                bool_u32(plan.picture.loop_restoration.uses_chroma_lr),
                bool_u32(plan.picture.apply_grain),
                0,
            ),
        },
        frame_type: av1_frame_type(plan.picture.frame_type)?,
        current_frame_id: plan.picture.current_frame_id,
        OrderHint: plan.picture.order_hint,
        primary_ref_frame: plan.picture.primary_ref_frame,
        refresh_frame_flags: plan.picture.refresh_frame_flags,
        reserved1: 0,
        interpolation_filter: av1_interpolation_filter(plan.picture.interpolation_filter)?,
        TxMode: if plan.picture.tx_mode_select {
            vk::video::STD_VIDEO_AV1_TX_MODE_SELECT
        } else {
            vk::video::STD_VIDEO_AV1_TX_MODE_LARGEST
        },
        delta_q_res: plan.picture.delta_q_res,
        delta_lf_res: plan.picture.delta_lf_res,
        SkipModeFrame: plan.picture.skip_mode_frame,
        coded_denom: plan.picture.coded_denom,
        reserved2: [0; 3],
        OrderHints: plan.picture.picture_order_hints,
        expectedFrameId: plan.picture.expected_frame_ids,
        pTileInfo: &std_tile_info,
        pQuantization: &std_quantization,
        pSegmentation: &std_segmentation,
        pLoopFilter: &std_loop_filter,
        pCDEF: &std_cdef,
        pLoopRestoration: &std_loop_restoration,
        pGlobalMotion: &std_global_motion,
        pFilmGrain: ptr::null(),
    };
    let mut av1_picture_info = vk::VideoDecodeAV1PictureInfoKHR::builder()
        .std_picture_info(&std_picture_info)
        .reference_name_slot_indices(plan.picture.reference_name_slot_indices)
        .frame_header_offset(plan.picture.frame_header_offset)
        .tile_offsets(&plan.picture.tile_offsets)
        .tile_sizes(&plan.picture.tile_sizes)
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
        .push_next(&mut av1_picture_info)
        .build();

    Ok(use_submit_info(NativeVulkanVulkanaliaAv1VkSubmitInfo {
        begin_info: &begin_info,
        decode_info: &decode_info,
        av1_picture_info: &av1_picture_info,
        std_picture_info: &std_picture_info,
        setup_reference_slot: &setup_reference_slot,
        begin_reference_slots: &begin_reference_slots,
        decode_reference_slots: &decode_reference_slots,
    }))
}

fn native_vulkan_vulkanalia_av1_begin_reference_source(
    plan: &NativeVulkanVulkanaliaAv1DecodeSubmitPlan,
    slot: &NativeVulkanVulkanaliaReferenceSlotPlan,
) -> Result<Option<NativeVulkanVulkanaliaAv1ReferenceInfoPlan>, String> {
    if !slot.codec_dpb_info_required {
        return Ok(None);
    }
    match slot.role {
        NativeVulkanVulkanaliaReferenceSlotRole::BeginInactive
        | NativeVulkanVulkanaliaReferenceSlotRole::SetupCurrent => {
            Ok(Some(plan.picture.setup_reference.clone()))
        }
        NativeVulkanVulkanaliaReferenceSlotRole::DecodeReference => plan
            .picture
            .references
            .iter()
            .find(|reference| reference.slot_index == slot.slot_index)
            .cloned()
            .map(Some)
            .ok_or_else(|| {
                format!(
                    "Vulkanalia AV1 begin reference slot {} has no matching decode reference",
                    slot.slot_index
                )
            }),
    }
}

fn av1_std_reference_info(
    reference: &NativeVulkanVulkanaliaAv1ReferenceInfoPlan,
) -> Result<vk::video::StdVideoDecodeAV1ReferenceInfo, String> {
    Ok(vk::video::StdVideoDecodeAV1ReferenceInfo {
        flags: vk::video::StdVideoDecodeAV1ReferenceInfoFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::video::StdVideoDecodeAV1ReferenceInfoFlags::new_bitfield_1(
                bool_u32(reference.disable_frame_end_update_cdf),
                bool_u32(reference.segmentation_enabled),
                0,
            ),
        },
        frame_type: reference.frame_type,
        RefFrameSignBias: reference.ref_frame_sign_bias,
        OrderHint: reference.order_hint,
        SavedOrderHints: reference.saved_order_hints,
    })
}

fn av1_expected_frame_ids_array(ids: &[u32]) -> [u32; 8] {
    let mut values = [0u32; 8];
    for (index, id) in ids.iter().take(8).enumerate() {
        values[index] = *id;
    }
    values
}

fn av1_reference_name_slot_indices(
    preferred: &[i32],
    fallback: &[i32],
) -> [i32; AV1_REFERENCE_NAME_COUNT] {
    let mut slots = [-1i32; AV1_REFERENCE_NAME_COUNT];
    let source = if preferred.is_empty() {
        fallback
    } else {
        preferred
    };
    for (index, slot) in source.iter().take(AV1_REFERENCE_NAME_COUNT).enumerate() {
        slots[index] = *slot;
    }
    slots
}

fn bool_u32(value: bool) -> u32 {
    u32::from(value)
}

fn av1_frame_type(frame_type: u8) -> Result<vk::video::StdVideoAV1FrameType, String> {
    match frame_type {
        0 => Ok(vk::video::STD_VIDEO_AV1_FRAME_TYPE_KEY),
        1 => Ok(vk::video::STD_VIDEO_AV1_FRAME_TYPE_INTER),
        2 => Ok(vk::video::STD_VIDEO_AV1_FRAME_TYPE_INTRA_ONLY),
        3 => Ok(vk::video::STD_VIDEO_AV1_FRAME_TYPE_SWITCH),
        other => Err(format!("unsupported Vulkanalia AV1 frame_type {other}")),
    }
}

fn av1_interpolation_filter(
    interpolation_filter: u32,
) -> Result<vk::video::StdVideoAV1InterpolationFilter, String> {
    match interpolation_filter {
        0 => Ok(vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_EIGHTTAP),
        1 => Ok(vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_EIGHTTAP_SMOOTH),
        2 => Ok(vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_EIGHTTAP_SHARP),
        3 => Ok(vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_BILINEAR),
        4 => Ok(vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_SWITCHABLE),
        other => Err(format!(
            "unsupported Vulkanalia AV1 interpolation_filter {other}"
        )),
    }
}

fn av1_restoration_type(
    restoration_type: u32,
) -> Result<vk::video::StdVideoAV1FrameRestorationType, String> {
    match restoration_type {
        0 => Ok(vk::video::STD_VIDEO_AV1_FRAME_RESTORATION_TYPE_NONE),
        1 => Ok(vk::video::STD_VIDEO_AV1_FRAME_RESTORATION_TYPE_WIENER),
        2 => Ok(vk::video::STD_VIDEO_AV1_FRAME_RESTORATION_TYPE_SGRPROJ),
        3 => Ok(vk::video::STD_VIDEO_AV1_FRAME_RESTORATION_TYPE_SWITCHABLE),
        other => Err(format!(
            "unsupported Vulkanalia AV1 restoration_type {other}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn av1_decode_submit_plan_matches_ffmpeg_slot_shape() {
        let entry = test_av1_entry();
        let frame = test_av1_frame();

        let plan = native_vulkan_vulkanalia_av1_decode_submit_plan(
            vk::Extent2D {
                width: 1920,
                height: 1080,
            },
            NativeVulkanVideoSessionCodec::Av1Main10,
            &entry,
            frame,
            4096,
            8192,
            false,
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
        assert_eq!(plan.picture.ffmpeg_reference, FFMPEG_AV1_PICTURE_REFERENCE);
        assert_eq!(plan.picture.frame_type, 1);
        assert_eq!(plan.picture.order_hint, 6);
        assert_eq!(plan.picture.tile_offsets, vec![128]);
        assert_eq!(plan.picture.reference_name_slot_indices[0], 1);
        assert!(!plan.common.reset_control_recorded);
        assert!(plan.common.command_order.contains(&"cmd_decode_video_khr"));
    }

    #[test]
    fn av1_decode_submit_plan_lowers_to_vulkanalia_decode_info() {
        let plan = native_vulkan_vulkanalia_av1_decode_submit_plan(
            vk::Extent2D {
                width: 1280,
                height: 720,
            },
            NativeVulkanVideoSessionCodec::Av1Main8,
            &test_av1_entry(),
            test_av1_frame(),
            2048,
            4096,
            true,
        )
        .unwrap();
        let image_views = NativeVulkanVulkanaliaDecodeImageViewBindings::repeated(
            vk::ImageView::default(),
            plan.common.begin_reference_slots.len(),
            plan.common.decode_reference_slots.len(),
        );

        native_vulkan_vulkanalia_av1_with_vk_submit_info(
            &plan,
            vk::VideoSessionKHR::default(),
            vk::VideoSessionParametersKHR::default(),
            vk::Buffer::default(),
            &image_views,
            |vk_info| {
                assert_eq!(vk_info.begin_info.reference_slot_count, 2);
                assert_eq!(vk_info.decode_info.src_buffer_offset, 2048);
                assert_eq!(vk_info.decode_info.src_buffer_range, 4096);
                assert_eq!(vk_info.decode_info.reference_slot_count, 1);
                assert!(!vk_info.decode_info.next.is_null());
                assert_eq!(vk_info.av1_picture_info.tile_count, 1);
                assert_eq!(vk_info.av1_picture_info.frame_header_offset, 64);
                assert_eq!(vk_info.av1_picture_info.reference_name_slot_indices[0], 1);
                assert_eq!(
                    vk_info.std_picture_info.frame_type,
                    vk::video::STD_VIDEO_AV1_FRAME_TYPE_INTER
                );
                assert_eq!(vk_info.std_picture_info.OrderHint, 6);
                assert_eq!(vk_info.std_picture_info.primary_ref_frame, 0);
                assert_eq!(vk_info.setup_reference_slot.slot_index, 2);
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

    fn test_av1_entry() -> NativeVulkanAv1DecodeReferencePlanEntrySnapshot {
        NativeVulkanAv1DecodeReferencePlanEntrySnapshot {
            temporal_unit_index: 4,
            frame_type_label: "inter",
            show_existing_frame: false,
            frame_to_show_map_idx: None,
            show_frame: true,
            order_hint: Some(6),
            current_frame_id: Some(9),
            expected_frame_ids: vec![0; 8],
            refresh_frame_flags: 0x04,
            output_slot: Some(2),
            displayed_slot: Some(2),
            reference_name_slot_indices: vec![1, -1, -1, -1, -1, -1, -1],
            reference_name_order_hints: vec![None, Some(4), None, None, None, None, None, None],
            map_order_hints: vec![None; 8],
            ref_frame_indices: vec![0],
            decode_reference_slots: vec![1, -1, -1, -1, -1, -1, -1],
            refreshed_reference_names: vec![2],
            missing_reference_names: Vec::new(),
            missing_reference_count: 0,
            references_resolved: true,
            submit_fields_ready: true,
            ready_for_decode_submit: true,
            ready_for_display_handoff: true,
            unsupported_reason: None,
            map_slot_indices_after: vec![-1, 1, 2, -1, -1, -1, -1, -1],
            map_order_hints_after: vec![None, Some(4), Some(6), None, None, None, None, None],
        }
    }

    fn test_av1_frame() -> NativeVulkanVulkanaliaAv1FrameSubmitInput {
        NativeVulkanVulkanaliaAv1FrameSubmitInput {
            temporal_unit_index: 4,
            frame_header_offset_for_vulkan: 64,
            tile_offsets: vec![128],
            tile_sizes: vec![2048],
            tile_info: NativeVulkanVulkanaliaAv1TileInfoPlan {
                uniform_tile_spacing_flag: true,
                tile_columns: 1,
                tile_rows: 1,
                context_update_tile_id: 0,
                tile_size_bytes_minus_1: 0,
                mi_col_starts: vec![0],
                mi_row_starts: vec![0],
                width_in_sbs_minus_1: vec![119],
                height_in_sbs_minus_1: vec![67],
            },
            frame_type: 1,
            show_existing_frame: false,
            show_frame: true,
            error_resilient_mode: false,
            disable_cdf_update: false,
            disable_frame_end_update_cdf: false,
            use_superres: false,
            render_and_frame_size_different: false,
            allow_screen_content_tools: true,
            is_filter_switchable: true,
            force_integer_mv: false,
            frame_size_override_flag: false,
            allow_intrabc: false,
            frame_refs_short_signaling: false,
            allow_high_precision_mv: true,
            is_motion_mode_switchable: true,
            use_ref_frame_mvs: true,
            allow_warped_motion: false,
            reduced_tx_set: false,
            reference_select: true,
            skip_mode_present: false,
            delta_q_present: false,
            delta_lf_present: false,
            delta_lf_multi: false,
            apply_grain: false,
            current_frame_id: Some(9),
            order_hint: Some(6),
            primary_ref_frame: Some(0),
            refresh_frame_flags: 0x04,
            interpolation_filter: 4,
            tx_mode_select: true,
            delta_q_res: 0,
            delta_lf_res: 0,
            skip_mode_frame: [0; 2],
            coded_denom: 8,
            picture_order_hints: [0, 4, 0, 0, 0, 0, 0, 0],
            expected_frame_ids: vec![0; 8],
            reference_name_slot_indices: vec![1, -1, -1, -1, -1, -1, -1],
            quantization: NativeVulkanVulkanaliaAv1QuantizationPlan {
                using_qmatrix: false,
                diff_uv_delta: false,
                base_q_idx: 120,
                delta_q_y_dc: 0,
                delta_q_u_dc: 0,
                delta_q_u_ac: 0,
                delta_q_v_dc: 0,
                delta_q_v_ac: 0,
                qm_y: 0,
                qm_u: 0,
                qm_v: 0,
            },
            segmentation: NativeVulkanVulkanaliaAv1SegmentationPlan {
                enabled: false,
                update_map: false,
                temporal_update: false,
                update_data: false,
                feature_enabled: [0; 8],
                feature_data: [[0; 8]; 8],
            },
            loop_filter: NativeVulkanVulkanaliaAv1LoopFilterPlan {
                delta_enabled: false,
                delta_update: false,
                level: [8, 8, 4, 4],
                sharpness: 0,
                update_ref_delta: 0,
                ref_deltas: [1, 0, 0, 0, -1, 0, -1, -1],
                update_mode_delta: 0,
                mode_deltas: [0, 0],
            },
            cdef: NativeVulkanVulkanaliaAv1CdefPlan {
                damping_minus_3: 3,
                bits: 2,
                y_pri_strength: [0; 8],
                y_sec_strength: [0; 8],
                uv_pri_strength: [0; 8],
                uv_sec_strength: [0; 8],
            },
            loop_restoration: NativeVulkanVulkanaliaAv1LoopRestorationPlan {
                frame_restoration_type: [0; 3],
                loop_restoration_size: [0; 3],
                uses_lr: false,
                uses_chroma_lr: false,
            },
            global_motion: NativeVulkanVulkanaliaAv1GlobalMotionPlan {
                gm_type: [0; 8],
                gm_params: [[0; 6]; 8],
            },
            setup_reference: NativeVulkanVulkanaliaAv1ReferenceInfoPlan {
                slot_index: 2,
                frame_type: 1,
                ref_frame_sign_bias: 0,
                order_hint: 6,
                saved_order_hints: [0, 4, 6, 0, 0, 0, 0, 0],
                disable_frame_end_update_cdf: false,
                segmentation_enabled: false,
            },
            references: vec![NativeVulkanVulkanaliaAv1ReferenceInfoPlan {
                slot_index: 1,
                frame_type: 0,
                ref_frame_sign_bias: 0,
                order_hint: 4,
                saved_order_hints: [0, 4, 0, 0, 0, 0, 0, 0],
                disable_frame_end_update_cdf: false,
                segmentation_enabled: false,
            }],
        }
    }
}
