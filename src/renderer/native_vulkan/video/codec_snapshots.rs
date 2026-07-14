//! Snapshot data types extracted from the native Vulkan renderer.

use std::ops::Deref;

use serde::ser::SerializeSeq;
use serde::{Serialize, Serializer};

const NATIVE_VULKAN_H264_INLINE_SLICE_OFFSETS: usize = 32;
const NATIVE_VULKAN_H264_INLINE_DECODE_REFERENCES: usize = 4;
const NATIVE_VULKAN_H265_INLINE_REFERENCE_DELTAS: usize = 4;
const NATIVE_VULKAN_H265_INLINE_DECODE_REFERENCES: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeVulkanH264SliceOffsets {
    inline: [u32; NATIVE_VULKAN_H264_INLINE_SLICE_OFFSETS],
    len: u8,
    overflow: Vec<u32>,
}

impl NativeVulkanH264SliceOffsets {
    pub fn new() -> Self {
        Self {
            inline: [0; NATIVE_VULKAN_H264_INLINE_SLICE_OFFSETS],
            len: 0,
            overflow: Vec::new(),
        }
    }

    pub fn single(value: u32) -> Self {
        let mut offsets = Self::new();
        offsets.push(value);
        offsets
    }

    pub fn from_vec(values: Vec<u32>) -> Self {
        if values.len() > NATIVE_VULKAN_H264_INLINE_SLICE_OFFSETS {
            return Self {
                inline: [0; NATIVE_VULKAN_H264_INLINE_SLICE_OFFSETS],
                len: 0,
                overflow: values,
            };
        }

        let mut offsets = Self::new();
        for value in values {
            offsets.push(value);
        }
        offsets
    }

    pub fn push(&mut self, value: u32) {
        if !self.overflow.is_empty() {
            self.overflow.push(value);
            return;
        }

        let len = usize::from(self.len);
        if len < NATIVE_VULKAN_H264_INLINE_SLICE_OFFSETS {
            self.inline[len] = value;
            self.len += 1;
            return;
        }

        self.overflow =
            Vec::with_capacity(NATIVE_VULKAN_H264_INLINE_SLICE_OFFSETS.saturating_mul(2));
        self.overflow
            .extend_from_slice(&self.inline[..NATIVE_VULKAN_H264_INLINE_SLICE_OFFSETS]);
        self.overflow.push(value);
        self.len = 0;
    }

    pub fn as_slice(&self) -> &[u32] {
        if self.overflow.is_empty() {
            &self.inline[..usize::from(self.len)]
        } else {
            &self.overflow
        }
    }
}

impl Default for NativeVulkanH264SliceOffsets {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Vec<u32>> for NativeVulkanH264SliceOffsets {
    fn from(values: Vec<u32>) -> Self {
        Self::from_vec(values)
    }
}

impl Deref for NativeVulkanH264SliceOffsets {
    type Target = [u32];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl PartialEq<Vec<u32>> for NativeVulkanH264SliceOffsets {
    fn eq(&self, other: &Vec<u32>) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl Serialize for NativeVulkanH264SliceOffsets {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let values = self.as_slice();
        let mut seq = serializer.serialize_seq(Some(values.len()))?;
        for value in values {
            seq.serialize_element(value)?;
        }
        seq.end()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeVulkanH265ReferenceDeltas {
    inline: [i32; NATIVE_VULKAN_H265_INLINE_REFERENCE_DELTAS],
    len: u8,
    overflow: Vec<i32>,
}

impl NativeVulkanH265ReferenceDeltas {
    pub fn new() -> Self {
        Self {
            inline: [0; NATIVE_VULKAN_H265_INLINE_REFERENCE_DELTAS],
            len: 0,
            overflow: Vec::new(),
        }
    }

    pub fn push(&mut self, value: i32) {
        if !self.overflow.is_empty() {
            self.overflow.push(value);
            return;
        }

        let len = usize::from(self.len);
        if len < NATIVE_VULKAN_H265_INLINE_REFERENCE_DELTAS {
            self.inline[len] = value;
            self.len += 1;
            return;
        }

        self.overflow =
            Vec::with_capacity(NATIVE_VULKAN_H265_INLINE_REFERENCE_DELTAS.saturating_mul(2));
        self.overflow
            .extend_from_slice(&self.inline[..NATIVE_VULKAN_H265_INLINE_REFERENCE_DELTAS]);
        self.overflow.push(value);
        self.len = 0;
    }

    pub fn extend_used_ref_pic_set(
        &mut self,
        ref_pic_set: &NativeVulkanH265ShortTermRefPicSetSnapshot,
    ) {
        for delta_poc in ref_pic_set
            .used_negative_delta_pocs
            .iter()
            .chain(ref_pic_set.used_positive_delta_pocs.iter())
        {
            self.push(*delta_poc);
        }
    }

    pub fn as_slice(&self) -> &[i32] {
        if self.overflow.is_empty() {
            &self.inline[..usize::from(self.len)]
        } else {
            &self.overflow
        }
    }
}

impl Default for NativeVulkanH265ReferenceDeltas {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for NativeVulkanH265ReferenceDeltas {
    type Target = [i32];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl Serialize for NativeVulkanH265ReferenceDeltas {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let values = self.as_slice();
        let mut seq = serializer.serialize_seq(Some(values.len()))?;
        for value in values {
            seq.serialize_element(value)?;
        }
        seq.end()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoBitstreamExtractSnapshot {
    pub source: String,
    pub frontend: &'static str,
    pub requested_max_samples: u32,
    pub samples: u32,
    pub total_bytes: u64,
    pub selected_access_unit_index: u32,
    pub selected_access_unit_bytes: u64,
    pub selected_access_unit_pts_ms: Option<u64>,
    pub selected_access_unit_duration_ms: Option<u64>,
    pub caps: Option<String>,
    pub stream_format: Option<String>,
    pub alignment: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub framerate: Option<String>,
    pub has_annex_b_start_codes: bool,
    pub h264_sps_count: u32,
    pub h264_pps_count: u32,
    pub h264_idr_count: u32,
    pub h264_slice_count: u32,
    pub h264_parameter_sets_present: bool,
    pub h264_parameter_sets: Option<NativeVulkanH264ParameterSetSnapshot>,
    pub h264_access_units: Vec<NativeVulkanH264AccessUnitSnapshot>,
    pub h264_idr_decode_ready_count: u32,
    pub h264_idr_decode_ready_prefix_count: u32,
    pub h264_idr_decode_first_unready_access_unit_index: Option<u32>,
    pub h264_idr_decode_first_unready_reason: Option<String>,
    pub h264_reference_plan_dpb_slots: u32,
    pub h264_decode_ready_count: u32,
    pub h264_decode_ready_prefix_count: u32,
    pub h264_decode_first_unready_access_unit_index: Option<u32>,
    pub h264_decode_first_unready_reason: Option<String>,
    pub h264_decode_reference_plan: Vec<NativeVulkanH264DecodeReferencePlanEntrySnapshot>,
    pub h265_vps_count: u32,
    pub h265_sps_count: u32,
    pub h265_pps_count: u32,
    pub h265_idr_count: u32,
    pub h265_slice_count: u32,
    pub h265_parameter_sets_present: bool,
    pub h265_parameter_sets: Option<NativeVulkanH265ParameterSetSnapshot>,
    pub h265_nal_units: Vec<NativeVulkanH265NalUnitSnapshot>,
    pub h265_access_units: Vec<NativeVulkanH265AccessUnitSnapshot>,
    pub h265_reference_plan_dpb_slots: u32,
    pub h265_decode_ready_count: u32,
    pub h265_decode_ready_prefix_count: u32,
    pub h265_decode_first_unready_access_unit_index: Option<u32>,
    pub h265_decode_first_unready_missing_reference_pocs: Vec<i32>,
    pub h265_decode_reference_plan: Vec<NativeVulkanH265DecodeReferencePlanEntrySnapshot>,
    pub av1_obu_count: u32,
    pub av1_sequence_header_count: u32,
    pub av1_temporal_delimiter_count: u32,
    pub av1_frame_header_count: u32,
    pub av1_tile_group_count: u32,
    pub av1_frame_count: u32,
    pub av1_decode_candidate: bool,
    pub av1_tile_payload_bytes: u64,
    pub av1_frame_payload_bytes: u64,
    pub av1_first_frame_header_obu_offset: Option<u64>,
    pub av1_first_tile_group_obu_offset: Option<u64>,
    pub av1_sequence_header_present: bool,
    pub av1_sequence_header: Option<NativeVulkanAv1SequenceHeaderSnapshot>,
    pub av1_first_frame_submit: Option<NativeVulkanAv1FrameSubmitSnapshot>,
    pub av1_obus: Vec<NativeVulkanAv1ObuSnapshot>,
    pub av1_temporal_units: Vec<NativeVulkanAv1TemporalUnitSnapshot>,
    pub av1_reference_plan_dpb_slots: u32,
    pub av1_decode_ready_count: u32,
    pub av1_decode_ready_leading_count: u32,
    pub av1_decode_first_unready_temporal_unit_index: Option<u32>,
    pub av1_decode_first_unready_reason: Option<String>,
    pub av1_decode_reference_plan: Vec<NativeVulkanAv1DecodeReferencePlanEntrySnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanH265NalUnitSnapshot {
    pub offset: u64,
    pub size: u64,
    pub nal_type: u8,
    pub nal_type_label: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanH265AccessUnitSnapshot {
    pub index: u32,
    pub bytes: u64,
    pub byte_hash: u64,
    pub pts_ns: Option<u64>,
    pub duration_ns: Option<u64>,
    pub pts_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    pub has_annex_b_start_codes: bool,
    pub has_parameter_sets: bool,
    pub h265_vps_count: u32,
    pub h265_sps_count: u32,
    pub h265_pps_count: u32,
    pub h265_idr_count: u32,
    pub h265_slice_count: u32,
    pub first_slice: Option<NativeVulkanH265AccessUnitSliceSnapshot>,
    pub first_slice_parse_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanH264AccessUnitSnapshot {
    pub index: u32,
    pub bytes: u64,
    pub byte_hash: u64,
    pub pts_ns: Option<u64>,
    pub duration_ns: Option<u64>,
    pub pts_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    pub has_annex_b_start_codes: bool,
    pub has_parameter_sets: bool,
    pub h264_sps_count: u32,
    pub h264_pps_count: u32,
    pub h264_idr_count: u32,
    pub h264_slice_count: u32,
    pub first_slice: Option<NativeVulkanH264AccessUnitSliceSnapshot>,
    pub first_slice_parse_error: Option<String>,
    pub idr_decode_ready: bool,
    pub decode_ready: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanH264AccessUnitSliceSnapshot {
    pub nal_type: u8,
    pub nal_type_label: &'static str,
    pub nal_ref_idc: u8,
    pub first_mb_in_slice: u32,
    pub first_slice_segment_in_pic_flag: bool,
    pub slice_type: u32,
    pub slice_type_normalized: u32,
    pub pps_id: u32,
    pub frame_num: u16,
    pub idr_pic_id: u16,
    pub num_ref_idx_l0_active_minus1: Option<u32>,
    pub num_ref_idx_l1_active_minus1: Option<u32>,
    pub ref_pic_list_modification_l0: bool,
    pub ref_pic_list_modifications_l0: Vec<NativeVulkanH264RefPicListModificationSnapshot>,
    pub ref_pic_list_modification_l1: bool,
    pub ref_pic_list_modifications_l1: Vec<NativeVulkanH264RefPicListModificationSnapshot>,
    pub adaptive_ref_pic_marking_mode_flag: bool,
    pub memory_management_control_operations:
        Vec<NativeVulkanH264MemoryManagementControlOperationSnapshot>,
    pub field_pic_flag: bool,
    pub bottom_field_flag: bool,
    pub is_reference: bool,
    pub is_intra: bool,
    pub is_p: bool,
    pub is_b: bool,
    pub long_term_reference_flag: bool,
    pub pic_order_cnt: [i32; 2],
    pub slice_offsets: NativeVulkanH264SliceOffsets,
    pub idr: bool,
    pub irap: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanH264RefPicListModificationSnapshot {
    pub modification_of_pic_nums_idc: u32,
    pub abs_diff_pic_num_minus1: Option<u32>,
    pub long_term_pic_num: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanH264MemoryManagementControlOperationSnapshot {
    pub memory_management_control_operation: u32,
    pub difference_of_pic_nums_minus1: Option<u32>,
    pub long_term_pic_num: Option<u32>,
    pub long_term_frame_idx: Option<u32>,
    pub max_long_term_frame_idx_plus1: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanH264DecodeReferencePlanEntrySnapshot {
    pub access_unit_index: u32,
    pub pts_ms: Option<u64>,
    pub nal_type_label: Option<&'static str>,
    pub current_frame_num: Option<u16>,
    pub current_pic_order_cnt_val: Option<i32>,
    pub current_pic_order_cnt: Option<[i32; 2]>,
    pub current_long_term_frame_idx: Option<u16>,
    pub planned_output_slot: u32,
    pub setup_slot_index: Option<i32>,
    pub evicted_frame_num: Option<u16>,
    pub evicted_long_term_frame_idx: Option<u16>,
    pub dropped_reference_frame_nums: Vec<u16>,
    pub dropped_long_term_frame_indices: Vec<u16>,
    pub inferred_non_existing_frame_nums: Vec<u16>,
    pub inferred_non_existing_references: Vec<NativeVulkanH264InferredNonExistingReferenceSnapshot>,
    pub inferred_dropped_reference_frame_nums: Vec<u16>,
    pub inferred_dropped_long_term_frame_indices: Vec<u16>,
    pub inferred_dropped_reference_slots: Vec<u32>,
    pub long_term_reference_conversions: Vec<NativeVulkanH264LongTermReferenceConversionSnapshot>,
    pub dropped_reference_slots: Vec<u32>,
    pub requested_reference_count: u32,
    pub references: NativeVulkanH264DecodeReferences,
    pub available_reference_count: u32,
    pub missing_reference_count: u32,
    pub unsupported_reason: Option<String>,
    pub ready_for_decode_submit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanH264InferredNonExistingReferenceSnapshot {
    pub frame_num: u16,
    pub field_pic_flag: bool,
    pub bottom_field_flag: bool,
    pub pic_order_cnt_val: i32,
    pub pic_order_cnt: [i32; 2],
    pub dpb_slot: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanH264DecodeReferenceSnapshot {
    pub frame_num: u16,
    pub field_pic_flag: bool,
    pub bottom_field_flag: bool,
    pub used_for_long_term_reference: bool,
    pub long_term_frame_idx: Option<u16>,
    pub long_term_pic_num: Option<u16>,
    pub non_existing: bool,
    pub pic_order_cnt_val: i32,
    pub pic_order_cnt: [i32; 2],
    pub available: bool,
    pub source_access_unit_index: Option<u32>,
    pub dpb_slot: Option<u32>,
}

impl NativeVulkanH264DecodeReferenceSnapshot {
    const EMPTY: Self = Self {
        frame_num: 0,
        field_pic_flag: false,
        bottom_field_flag: false,
        used_for_long_term_reference: false,
        long_term_frame_idx: None,
        long_term_pic_num: None,
        non_existing: false,
        pic_order_cnt_val: 0,
        pic_order_cnt: [0, 0],
        available: false,
        source_access_unit_index: None,
        dpb_slot: None,
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeVulkanH264DecodeReferences {
    inline: [NativeVulkanH264DecodeReferenceSnapshot; NATIVE_VULKAN_H264_INLINE_DECODE_REFERENCES],
    len: u8,
    overflow: Vec<NativeVulkanH264DecodeReferenceSnapshot>,
}

impl NativeVulkanH264DecodeReferences {
    pub fn new() -> Self {
        Self {
            inline: [NativeVulkanH264DecodeReferenceSnapshot::EMPTY;
                NATIVE_VULKAN_H264_INLINE_DECODE_REFERENCES],
            len: 0,
            overflow: Vec::new(),
        }
    }

    pub fn from_vec(values: Vec<NativeVulkanH264DecodeReferenceSnapshot>) -> Self {
        if values.len() > NATIVE_VULKAN_H264_INLINE_DECODE_REFERENCES {
            return Self {
                inline: [NativeVulkanH264DecodeReferenceSnapshot::EMPTY;
                    NATIVE_VULKAN_H264_INLINE_DECODE_REFERENCES],
                len: 0,
                overflow: values,
            };
        }

        let mut references = Self::new();
        for value in values {
            references.push(value);
        }
        references
    }

    pub fn push(&mut self, value: NativeVulkanH264DecodeReferenceSnapshot) {
        if !self.overflow.is_empty() {
            self.overflow.push(value);
            return;
        }

        let len = usize::from(self.len);
        if len < NATIVE_VULKAN_H264_INLINE_DECODE_REFERENCES {
            self.inline[len] = value;
            self.len += 1;
            return;
        }

        self.overflow =
            Vec::with_capacity(NATIVE_VULKAN_H264_INLINE_DECODE_REFERENCES.saturating_mul(2));
        self.overflow
            .extend_from_slice(&self.inline[..NATIVE_VULKAN_H264_INLINE_DECODE_REFERENCES]);
        self.overflow.push(value);
        self.len = 0;
    }

    pub fn as_slice(&self) -> &[NativeVulkanH264DecodeReferenceSnapshot] {
        if self.overflow.is_empty() {
            &self.inline[..usize::from(self.len)]
        } else {
            &self.overflow
        }
    }
}

impl Default for NativeVulkanH264DecodeReferences {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Vec<NativeVulkanH264DecodeReferenceSnapshot>> for NativeVulkanH264DecodeReferences {
    fn from(values: Vec<NativeVulkanH264DecodeReferenceSnapshot>) -> Self {
        Self::from_vec(values)
    }
}

impl FromIterator<NativeVulkanH264DecodeReferenceSnapshot> for NativeVulkanH264DecodeReferences {
    fn from_iter<T: IntoIterator<Item = NativeVulkanH264DecodeReferenceSnapshot>>(iter: T) -> Self {
        let mut references = Self::new();
        for value in iter {
            references.push(value);
        }
        references
    }
}

impl Deref for NativeVulkanH264DecodeReferences {
    type Target = [NativeVulkanH264DecodeReferenceSnapshot];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<'a> IntoIterator for &'a NativeVulkanH264DecodeReferences {
    type Item = &'a NativeVulkanH264DecodeReferenceSnapshot;
    type IntoIter = std::slice::Iter<'a, NativeVulkanH264DecodeReferenceSnapshot>;

    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter()
    }
}

impl Serialize for NativeVulkanH264DecodeReferences {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let values = self.as_slice();
        let mut seq = serializer.serialize_seq(Some(values.len()))?;
        for value in values {
            seq.serialize_element(value)?;
        }
        seq.end()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanH264LongTermReferenceConversionSnapshot {
    pub frame_num: u16,
    pub long_term_frame_idx: u16,
    pub dpb_slot: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanAv1ObuSnapshot {
    pub offset: u64,
    pub header_size: u64,
    pub payload_offset: u64,
    pub payload_size: u64,
    pub obu_type: u8,
    pub obu_type_label: &'static str,
    pub has_extension: bool,
    pub has_size_field: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanAv1TemporalUnitSnapshot {
    pub index: u32,
    pub bytes: u64,
    pub byte_hash: u64,
    pub pts_ns: Option<u64>,
    pub duration_ns: Option<u64>,
    pub pts_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    pub obu_count: u32,
    pub sequence_header_count: u32,
    pub temporal_delimiter_count: u32,
    pub frame_header_count: u32,
    pub tile_group_count: u32,
    pub frame_count: u32,
    pub decode_candidate: bool,
    pub tile_payload_bytes: u64,
    pub frame_payload_bytes: u64,
    pub first_frame_header_obu_offset: Option<u64>,
    pub first_tile_group_obu_offset: Option<u64>,
    pub sequence_header_present: bool,
    pub sequence_header: Option<NativeVulkanAv1SequenceHeaderSnapshot>,
    pub first_frame_submit: Option<NativeVulkanAv1FrameSubmitSnapshot>,
    pub obus: Vec<NativeVulkanAv1ObuSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanAv1SequenceHeaderSnapshot {
    pub parser: &'static str,
    pub seq_profile: u8,
    pub seq_profile_label: &'static str,
    pub still_picture: bool,
    pub reduced_still_picture_header: bool,
    pub timing_info_present_flag: bool,
    pub timing_info: Option<NativeVulkanAv1TimingInfoSnapshot>,
    pub decoder_model_info_present_flag: bool,
    pub buffer_delay_length_minus_1: u8,
    pub frame_presentation_time_length_minus_1: u8,
    pub initial_display_delay_present_flag: bool,
    pub operating_points_cnt_minus_1: u8,
    pub operating_points: Vec<NativeVulkanAv1OperatingPointSnapshot>,
    pub frame_width_bits_minus_1: u8,
    pub frame_height_bits_minus_1: u8,
    pub max_frame_width_minus_1: u32,
    pub max_frame_height_minus_1: u32,
    pub max_frame_width: u32,
    pub max_frame_height: u32,
    pub frame_id_numbers_present_flag: bool,
    pub delta_frame_id_length_minus_2: Option<u8>,
    pub additional_frame_id_length_minus_1: Option<u8>,
    pub use_128x128_superblock: bool,
    pub enable_filter_intra: bool,
    pub enable_intra_edge_filter: bool,
    pub enable_interintra_compound: bool,
    pub enable_masked_compound: bool,
    pub enable_warped_motion: bool,
    pub enable_dual_filter: bool,
    pub enable_order_hint: bool,
    pub enable_jnt_comp: bool,
    pub enable_ref_frame_mvs: bool,
    pub seq_force_screen_content_tools: u8,
    pub seq_force_integer_mv: u8,
    pub order_hint_bits_minus_1: Option<u8>,
    pub enable_superres: bool,
    pub enable_cdef: bool,
    pub enable_restoration: bool,
    pub film_grain_params_present: bool,
    pub color_config: NativeVulkanAv1ColorConfigSnapshot,
    pub requested_profile_compatible: bool,
    pub vulkan_std_session_parameters_ready: bool,
}

include!("codec_snapshots/av1_and_h26x.rs");
