#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanAv1FrameSubmitSnapshot {
    pub parser: &'static str,
    pub frame_header_obu_offset: u64,
    pub frame_header_payload_offset: u64,
    pub frame_header_payload_size: u64,
    pub frame_header_offset_for_vulkan: u32,
    pub tile_count: u32,
    pub tile_columns: u32,
    pub tile_rows: u32,
    pub tile_size_bytes: u32,
    pub tile_offsets: Vec<u32>,
    pub tile_sizes: Vec<u32>,
    pub tile_payload_total_bytes: u64,
    pub frame_obu_payload_bytes: u64,
    pub frame_type: u8,
    pub frame_type_label: &'static str,
    pub show_existing_frame: bool,
    pub frame_to_show_map_idx: Option<u8>,
    pub display_frame_id: Option<u32>,
    pub current_frame_id: Option<u32>,
    pub expected_frame_ids: Vec<u32>,
    pub show_frame: bool,
    pub showable_frame: bool,
    pub error_resilient_mode: bool,
    pub disable_cdf_update: bool,
    pub allow_screen_content_tools: u8,
    pub force_integer_mv: u8,
    pub allow_high_precision_mv: bool,
    pub interpolation_filter: u32,
    pub interpolation_filter_label: &'static str,
    pub is_filter_switchable: bool,
    pub is_motion_mode_switchable: bool,
    pub use_ref_frame_mvs: bool,
    pub reference_select: bool,
    pub skip_mode_present: bool,
    pub allow_warped_motion: bool,
    pub order_hint: Option<u8>,
    pub primary_ref_frame: Option<u8>,
    pub refresh_frame_flags: u8,
    pub reference_order_hints: Vec<u8>,
    pub frame_refs_short_signaling: bool,
    pub last_frame_idx: Option<u8>,
    pub gold_frame_idx: Option<u8>,
    pub ref_frame_indices: Vec<i8>,
    pub render_and_frame_size_different: Option<bool>,
    pub frame_width: Option<u32>,
    pub frame_height: Option<u32>,
    pub render_width: Option<u32>,
    pub render_height: Option<u32>,
    pub found_frame_header: bool,
    pub found_tile_payload: bool,
    pub vulkan_submit_candidate: bool,
    pub unsupported_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanAv1DecodeReferencePlanEntrySnapshot {
    pub temporal_unit_index: u32,
    pub frame_type_label: &'static str,
    pub show_existing_frame: bool,
    pub frame_to_show_map_idx: Option<u8>,
    pub show_frame: bool,
    pub order_hint: Option<u8>,
    pub current_frame_id: Option<u32>,
    pub expected_frame_ids: Vec<u32>,
    pub refresh_frame_flags: u8,
    pub output_slot: Option<u32>,
    pub displayed_slot: Option<u32>,
    pub reference_name_slot_indices: Vec<i32>,
    pub reference_name_order_hints: Vec<Option<u8>>,
    pub map_order_hints: Vec<Option<u8>>,
    pub ref_frame_indices: Vec<i8>,
    pub decode_reference_slots: Vec<i32>,
    pub refreshed_reference_names: Vec<u8>,
    pub missing_reference_names: Vec<u8>,
    pub missing_reference_count: u32,
    pub references_resolved: bool,
    pub submit_fields_ready: bool,
    pub ready_for_decode_submit: bool,
    pub ready_for_display_handoff: bool,
    pub unsupported_reason: Option<String>,
    pub map_slot_indices_after: Vec<i32>,
    pub map_order_hints_after: Vec<Option<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanAv1OperatingPointSnapshot {
    pub index: u8,
    pub idc: u16,
    pub seq_level_idx: u8,
    pub seq_level_label: Option<&'static str>,
    pub seq_tier: bool,
    pub decoder_model_present_for_this_op: bool,
    pub initial_display_delay_present_for_this_op: bool,
    pub initial_display_delay_minus_1: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanAv1TimingInfoSnapshot {
    pub num_units_in_display_tick: u32,
    pub time_scale: u32,
    pub equal_picture_interval: bool,
    pub num_ticks_per_picture_minus_1: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanAv1ColorConfigSnapshot {
    pub high_bitdepth: bool,
    pub twelve_bit: bool,
    pub mono_chrome: bool,
    pub color_description_present_flag: bool,
    pub color_primaries: u8,
    pub transfer_characteristics: u8,
    pub matrix_coefficients: u8,
    pub color_range: bool,
    pub subsampling_x: bool,
    pub subsampling_y: bool,
    pub chroma_sample_position: u8,
    pub separate_uv_delta_q: bool,
    pub bit_depth: u8,
    pub num_planes: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanH265AccessUnitSliceSnapshot {
    pub nal_type: u8,
    pub nal_type_label: &'static str,
    pub slice_segment_offset: u32,
    pub first_slice_segment_in_pic_flag: bool,
    pub slice_type: u32,
    pub pps_id: u32,
    pub pic_order_cnt_lsb: Option<u32>,
    pub short_term_ref_pic_set_sps_flag: bool,
    pub short_term_ref_pic_set_idx: Option<u32>,
    pub num_delta_pocs_of_ref_rps_idx: u8,
    pub num_bits_for_st_ref_pic_set_in_slice: u16,
    pub short_term_reference_delta_pocs: NativeVulkanH265ReferenceDeltas,
    pub long_term_references: Vec<NativeVulkanH265LongTermReferenceSnapshot>,
    pub idr: bool,
    pub irap: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanH265LongTermReferenceSnapshot {
    pub from_sps: bool,
    pub lt_idx_sps: Option<u32>,
    pub poc_lsb: u32,
    pub used_by_current: bool,
    pub delta_poc_msb_present_flag: bool,
    pub delta_poc_msb_cycle_lt: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanH265ShortTermRefPicSetSnapshot {
    pub inter_ref_pic_set_prediction_flag: bool,
    pub delta_idx_minus1: Option<u32>,
    pub delta_rps_sign: Option<bool>,
    pub abs_delta_rps_minus1: Option<u32>,
    pub num_delta_pocs_of_ref_rps_idx: u32,
    pub use_delta_flags: Vec<bool>,
    pub used_by_current_flags: Vec<bool>,
    pub num_negative_pics: u32,
    pub num_positive_pics: u32,
    pub negative_delta_pocs: Vec<i32>,
    pub negative_used_by_curr_pic: Vec<bool>,
    pub used_negative_delta_pocs: Vec<i32>,
    pub positive_delta_pocs: Vec<i32>,
    pub positive_used_by_curr_pic: Vec<bool>,
    pub used_positive_delta_pocs: Vec<i32>,
    pub used_by_current_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanH265DecodeReferencePlanEntrySnapshot {
    pub access_unit_index: u32,
    pub pts_ms: Option<u64>,
    pub nal_type_label: Option<&'static str>,
    pub current_poc: Option<i32>,
    pub planned_output_slot: u32,
    pub setup_slot_index: Option<i32>,
    pub evicted_poc: Option<i32>,
    pub references: NativeVulkanH265DecodeReferences,
    pub available_reference_count: u32,
    pub missing_reference_count: u32,
    pub missing_reference_pocs: Vec<i32>,
    pub unsupported_reason: Option<String>,
    pub ready_for_decode_submit: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanH265DecodeReferenceSnapshot {
    pub delta_poc: i32,
    pub poc: i32,
    pub used_for_long_term_reference: bool,
    pub available: bool,
    pub source_access_unit_index: Option<u32>,
    pub dpb_slot: Option<u32>,
}

impl NativeVulkanH265DecodeReferenceSnapshot {
    const EMPTY: Self = Self {
        delta_poc: 0,
        poc: 0,
        used_for_long_term_reference: false,
        available: false,
        source_access_unit_index: None,
        dpb_slot: None,
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeVulkanH265DecodeReferences {
    inline: [NativeVulkanH265DecodeReferenceSnapshot; NATIVE_VULKAN_H265_INLINE_DECODE_REFERENCES],
    len: u8,
    overflow: Vec<NativeVulkanH265DecodeReferenceSnapshot>,
}

impl NativeVulkanH265DecodeReferences {
    pub fn new() -> Self {
        Self {
            inline: [NativeVulkanH265DecodeReferenceSnapshot::EMPTY;
                NATIVE_VULKAN_H265_INLINE_DECODE_REFERENCES],
            len: 0,
            overflow: Vec::new(),
        }
    }

    pub fn from_vec(values: Vec<NativeVulkanH265DecodeReferenceSnapshot>) -> Self {
        if values.len() > NATIVE_VULKAN_H265_INLINE_DECODE_REFERENCES {
            return Self {
                inline: [NativeVulkanH265DecodeReferenceSnapshot::EMPTY;
                    NATIVE_VULKAN_H265_INLINE_DECODE_REFERENCES],
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

    pub fn push(&mut self, value: NativeVulkanH265DecodeReferenceSnapshot) {
        if !self.overflow.is_empty() {
            self.overflow.push(value);
            return;
        }

        let len = usize::from(self.len);
        if len < NATIVE_VULKAN_H265_INLINE_DECODE_REFERENCES {
            self.inline[len] = value;
            self.len += 1;
            return;
        }

        self.overflow =
            Vec::with_capacity(NATIVE_VULKAN_H265_INLINE_DECODE_REFERENCES.saturating_mul(2));
        self.overflow
            .extend_from_slice(&self.inline[..NATIVE_VULKAN_H265_INLINE_DECODE_REFERENCES]);
        self.overflow.push(value);
        self.len = 0;
    }

    pub fn as_slice(&self) -> &[NativeVulkanH265DecodeReferenceSnapshot] {
        if self.overflow.is_empty() {
            &self.inline[..usize::from(self.len)]
        } else {
            &self.overflow
        }
    }
}

impl Default for NativeVulkanH265DecodeReferences {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Vec<NativeVulkanH265DecodeReferenceSnapshot>> for NativeVulkanH265DecodeReferences {
    fn from(values: Vec<NativeVulkanH265DecodeReferenceSnapshot>) -> Self {
        Self::from_vec(values)
    }
}

impl FromIterator<NativeVulkanH265DecodeReferenceSnapshot> for NativeVulkanH265DecodeReferences {
    fn from_iter<T: IntoIterator<Item = NativeVulkanH265DecodeReferenceSnapshot>>(iter: T) -> Self {
        let mut references = Self::new();
        for value in iter {
            references.push(value);
        }
        references
    }
}

impl Deref for NativeVulkanH265DecodeReferences {
    type Target = [NativeVulkanH265DecodeReferenceSnapshot];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<'a> IntoIterator for &'a NativeVulkanH265DecodeReferences {
    type Item = &'a NativeVulkanH265DecodeReferenceSnapshot;
    type IntoIter = std::slice::Iter<'a, NativeVulkanH265DecodeReferenceSnapshot>;

    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter()
    }
}

impl Serialize for NativeVulkanH265DecodeReferences {
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
pub struct NativeVulkanH264ParameterSetSnapshot {
    pub parser: &'static str,
    pub sps: NativeVulkanH264SpsSnapshot,
    pub pps: NativeVulkanH264PpsSnapshot,
    pub requested_profile_compatible: bool,
    pub vulkan_std_session_parameters_ready: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanH264SpsSnapshot {
    pub id: u32,
    pub profile_idc: u8,
    pub profile_label: &'static str,
    pub constraint_set0_flag: bool,
    pub constraint_set1_flag: bool,
    pub constraint_set2_flag: bool,
    pub constraint_set3_flag: bool,
    pub constraint_set4_flag: bool,
    pub constraint_set5_flag: bool,
    pub level_idc: u8,
    pub level_label: Option<&'static str>,
    pub chroma_format_idc: u32,
    pub chroma_format_label: &'static str,
    pub separate_colour_plane_flag: bool,
    pub bit_depth_luma_minus8: u32,
    pub bit_depth_chroma_minus8: u32,
    pub qpprime_y_zero_transform_bypass_flag: bool,
    pub seq_scaling_matrix_present_flag: bool,
    pub log2_max_frame_num_minus4: u32,
    pub pic_order_cnt_type: u32,
    pub log2_max_pic_order_cnt_lsb_minus4: u32,
    pub delta_pic_order_always_zero_flag: bool,
    pub offset_for_non_ref_pic: i32,
    pub offset_for_top_to_bottom_field: i32,
    pub offset_for_ref_frame: Vec<i32>,
    pub max_num_ref_frames: u32,
    pub gaps_in_frame_num_value_allowed_flag: bool,
    pub pic_width_in_mbs_minus1: u32,
    pub pic_height_in_map_units_minus1: u32,
    pub frame_mbs_only_flag: bool,
    pub mb_adaptive_frame_field_flag: bool,
    pub direct_8x8_inference_flag: bool,
    pub frame_cropping_flag: bool,
    pub frame_crop_left_offset: u32,
    pub frame_crop_right_offset: u32,
    pub frame_crop_top_offset: u32,
    pub frame_crop_bottom_offset: u32,
    pub vui_parameters_present_flag: bool,
    pub vui: Option<NativeVulkanH264VuiSnapshot>,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanH264VuiSnapshot {
    pub aspect_ratio_info_present_flag: bool,
    pub aspect_ratio_idc: u32,
    pub sar_width: u32,
    pub sar_height: u32,
    pub overscan_info_present_flag: bool,
    pub overscan_appropriate_flag: bool,
    pub video_signal_type_present_flag: bool,
    pub video_format: u32,
    pub video_full_range_flag: bool,
    pub colour_description_present_flag: bool,
    pub colour_primaries: u32,
    pub transfer_characteristics: u32,
    pub matrix_coeffs: u32,
    pub chroma_loc_info_present_flag: bool,
    pub chroma_sample_loc_type_top_field: u32,
    pub chroma_sample_loc_type_bottom_field: u32,
    pub timing_info_present_flag: bool,
    pub num_units_in_tick: u32,
    pub time_scale: u32,
    pub fixed_frame_rate_flag: bool,
    pub nal_hrd_parameters_present_flag: bool,
    pub vcl_hrd_parameters_present_flag: bool,
    pub low_delay_hrd_flag: bool,
    pub pic_struct_present_flag: bool,
    pub bitstream_restriction_flag: bool,
    pub motion_vectors_over_pic_boundaries_flag: bool,
    pub max_bytes_per_pic_denom: u32,
    pub max_bits_per_mb_denom: u32,
    pub log2_max_mv_length_horizontal: u32,
    pub log2_max_mv_length_vertical: u32,
    pub num_reorder_frames: u32,
    pub max_dec_frame_buffering: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanH264PpsSnapshot {
    pub id: u32,
    pub sps_id: u32,
    pub entropy_coding_mode_flag: bool,
    pub bottom_field_pic_order_in_frame_present_flag: bool,
    pub num_slice_groups_minus1: u32,
    pub num_ref_idx_l0_default_active_minus1: u32,
    pub num_ref_idx_l1_default_active_minus1: u32,
    pub weighted_pred_flag: bool,
    pub weighted_bipred_idc: u32,
    pub pic_init_qp_minus26: i32,
    pub pic_init_qs_minus26: i32,
    pub chroma_qp_index_offset: i32,
    pub deblocking_filter_control_present_flag: bool,
    pub constrained_intra_pred_flag: bool,
    pub redundant_pic_cnt_present_flag: bool,
    pub transform_8x8_mode_flag: bool,
    pub pic_scaling_matrix_present_flag: bool,
    pub second_chroma_qp_index_offset: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanH265ParameterSetSnapshot {
    pub parser: &'static str,
    pub vps: NativeVulkanH265VpsSnapshot,
    pub sps: NativeVulkanH265SpsSnapshot,
    pub pps: NativeVulkanH265PpsSnapshot,
    pub requested_profile_compatible: bool,
    pub vulkan_std_session_parameters_ready: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanH265VpsSnapshot {
    pub id: u8,
    pub max_layers_minus1: u8,
    pub max_sub_layers_minus1: u8,
    pub temporal_id_nesting_flag: bool,
    pub sub_layer_ordering_info_present_flag: bool,
    pub profile_idc: u8,
    pub profile_label: &'static str,
    pub tier_flag: bool,
    pub progressive_source_flag: bool,
    pub interlaced_source_flag: bool,
    pub non_packed_constraint_flag: bool,
    pub frame_only_constraint_flag: bool,
    pub level_idc: u8,
    pub level_label: Option<&'static str>,
    pub dec_pic_buf_mgr: NativeVulkanH265DecPicBufMgrSnapshot,
    pub timing_info_present_flag: bool,
    pub poc_proportional_to_timing_flag: bool,
    pub num_units_in_tick: Option<u32>,
    pub time_scale: Option<u32>,
    pub num_ticks_poc_diff_one_minus1: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanH265DecPicBufMgrSnapshot {
    pub max_latency_increase_plus1: [u32; 7],
    pub max_dec_pic_buffering_minus1: [u8; 7],
    pub max_num_reorder_pics: [u8; 7],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanH265SpsSnapshot {
    pub id: u32,
    pub vps_id: u8,
    pub max_sub_layers_minus1: u8,
    pub temporal_id_nesting_flag: bool,
    pub sub_layer_ordering_info_present_flag: bool,
    pub profile_idc: u8,
    pub profile_label: &'static str,
    pub tier_flag: bool,
    pub progressive_source_flag: bool,
    pub interlaced_source_flag: bool,
    pub non_packed_constraint_flag: bool,
    pub frame_only_constraint_flag: bool,
    pub level_idc: u8,
    pub level_label: Option<&'static str>,
    pub dec_pic_buf_mgr: NativeVulkanH265DecPicBufMgrSnapshot,
    pub chroma_format_idc: u32,
    pub chroma_format_label: &'static str,
    pub separate_colour_plane_flag: bool,
    pub width: u32,
    pub height: u32,
    pub conformance_window_flag: bool,
    pub conf_win_left_offset: u32,
    pub conf_win_right_offset: u32,
    pub conf_win_top_offset: u32,
    pub conf_win_bottom_offset: u32,
    pub bit_depth_luma_minus8: u32,
    pub bit_depth_chroma_minus8: u32,
    pub log2_max_pic_order_cnt_lsb_minus4: u32,
    pub log2_min_luma_coding_block_size_minus3: u32,
    pub log2_diff_max_min_luma_coding_block_size: u32,
    pub log2_min_luma_transform_block_size_minus2: u32,
    pub log2_diff_max_min_luma_transform_block_size: u32,
    pub max_transform_hierarchy_depth_inter: u32,
    pub max_transform_hierarchy_depth_intra: u32,
    pub scaling_list_enabled_flag: bool,
    pub sps_scaling_list_data_present_flag: bool,
    pub amp_enabled_flag: bool,
    pub sample_adaptive_offset_enabled_flag: bool,
    pub pcm_enabled_flag: bool,
    pub pcm_loop_filter_disabled_flag: bool,
    pub num_short_term_ref_pic_sets: u32,
    pub short_term_ref_pic_sets: Vec<NativeVulkanH265ShortTermRefPicSetSnapshot>,
    pub long_term_ref_pics_present_flag: bool,
    pub long_term_ref_pics_sps: Vec<NativeVulkanH265LongTermRefPicSpsSnapshot>,
    pub temporal_mvp_enabled_flag: bool,
    pub strong_intra_smoothing_enabled_flag: bool,
    pub vui_parameters_present_flag: bool,
    pub vui: Option<NativeVulkanH265VuiSnapshot>,
    pub sps_extension_present_flag: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanH265LongTermRefPicSpsSnapshot {
    pub lt_ref_pic_poc_lsb_sps: u32,
    pub used_by_curr_pic_lt_sps_flag: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanH265VuiSnapshot {
    pub aspect_ratio_info_present_flag: bool,
    pub aspect_ratio_idc: u32,
    pub sar_width: u16,
    pub sar_height: u16,
    pub overscan_info_present_flag: bool,
    pub overscan_appropriate_flag: bool,
    pub video_signal_type_present_flag: bool,
    pub video_format: u8,
    pub video_full_range_flag: bool,
    pub colour_description_present_flag: bool,
    pub colour_primaries: u8,
    pub transfer_characteristics: u8,
    pub matrix_coeffs: u8,
    pub chroma_loc_info_present_flag: bool,
    pub chroma_sample_loc_type_top_field: u8,
    pub chroma_sample_loc_type_bottom_field: u8,
    pub neutral_chroma_indication_flag: bool,
    pub field_seq_flag: bool,
    pub frame_field_info_present_flag: bool,
    pub default_display_window_flag: bool,
    pub def_disp_win_left_offset: u16,
    pub def_disp_win_right_offset: u16,
    pub def_disp_win_top_offset: u16,
    pub def_disp_win_bottom_offset: u16,
    pub vui_timing_info_present_flag: bool,
    pub vui_num_units_in_tick: u32,
    pub vui_time_scale: u32,
    pub vui_poc_proportional_to_timing_flag: bool,
    pub vui_num_ticks_poc_diff_one_minus1: u32,
    pub vui_hrd_parameters_present_flag: bool,
    pub bitstream_restriction_flag: bool,
    pub tiles_fixed_structure_flag: bool,
    pub motion_vectors_over_pic_boundaries_flag: bool,
    pub restricted_ref_pic_lists_flag: bool,
    pub min_spatial_segmentation_idc: u16,
    pub max_bytes_per_pic_denom: u8,
    pub max_bits_per_min_cu_denom: u8,
    pub log2_max_mv_length_horizontal: u8,
    pub log2_max_mv_length_vertical: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanH265PpsSnapshot {
    pub id: u32,
    pub sps_id: u32,
    pub dependent_slice_segments_enabled_flag: bool,
    pub output_flag_present_flag: bool,
    pub num_extra_slice_header_bits: u8,
    pub sign_data_hiding_enabled_flag: bool,
    pub cabac_init_present_flag: bool,
    pub num_ref_idx_l0_default_active_minus1: u32,
    pub num_ref_idx_l1_default_active_minus1: u32,
    pub init_qp_minus26: i32,
    pub constrained_intra_pred_flag: bool,
    pub transform_skip_enabled_flag: bool,
    pub cu_qp_delta_enabled_flag: bool,
    pub diff_cu_qp_delta_depth: Option<u32>,
    pub cb_qp_offset: i32,
    pub cr_qp_offset: i32,
    pub slice_chroma_qp_offsets_present_flag: bool,
    pub weighted_pred_flag: bool,
    pub weighted_bipred_flag: bool,
    pub transquant_bypass_enabled_flag: bool,
    pub tiles_enabled_flag: bool,
    pub entropy_coding_sync_enabled_flag: bool,
    pub uniform_spacing_flag: bool,
    pub num_tile_columns_minus1: u32,
    pub num_tile_rows_minus1: u32,
    pub loop_filter_across_tiles_enabled_flag: Option<bool>,
    pub loop_filter_across_slices_enabled_flag: bool,
    pub deblocking_filter_control_present_flag: bool,
    pub deblocking_filter_override_enabled_flag: Option<bool>,
    pub pps_deblocking_filter_disabled_flag: Option<bool>,
    pub pps_beta_offset_div2: i32,
    pub pps_tc_offset_div2: i32,
    pub pps_scaling_list_data_present_flag: bool,
    pub lists_modification_present_flag: bool,
    pub log2_parallel_merge_level_minus2: u32,
    pub slice_segment_header_extension_present_flag: bool,
    pub pps_extension_present_flag: bool,
}
