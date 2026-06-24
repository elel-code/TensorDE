#![allow(dead_code)]

use serde::Serialize;
use vulkanalia::vk;

use crate::renderer::native_vulkan::{
    NativeVulkanH265AccessUnitSliceSnapshot, NativeVulkanH265DecodeReferencePlanEntrySnapshot,
    NativeVulkanH265ParameterSetSnapshot, NativeVulkanVideoSessionCodec,
};

use super::video_decode_submit::{
    NativeVulkanVulkanaliaDecodeSubmitPlan, NativeVulkanVulkanaliaPictureResourcePlan,
    NativeVulkanVulkanaliaReferenceSlotPlan,
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
