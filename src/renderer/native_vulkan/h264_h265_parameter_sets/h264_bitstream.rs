#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_h264_vui_parameters(
    bits: &mut NativeVulkanH264BitReader<'_>,
    rbsp_payload: &[u8],
) -> Result<NativeVulkanH264VuiSnapshot, String> {
    let aspect_ratio_info_present_flag = bits.read_bool("aspect_ratio_info_present_flag")?;
    let mut aspect_ratio_idc = 0;
    let mut sar_width = 0;
    let mut sar_height = 0;
    if aspect_ratio_info_present_flag {
        aspect_ratio_idc = bits.read_bits(8, "aspect_ratio_idc")?;
        if aspect_ratio_idc == 255 {
            sar_width = bits.read_bits(16, "sar_width")?;
            sar_height = bits.read_bits(16, "sar_height")?;
        }
    }

    let overscan_info_present_flag = bits.read_bool("overscan_info_present_flag")?;
    let overscan_appropriate_flag = if overscan_info_present_flag {
        bits.read_bool("overscan_appropriate_flag")?
    } else {
        false
    };

    let video_signal_type_present_flag = bits.read_bool("video_signal_type_present_flag")?;
    let mut video_format = 5;
    let mut video_full_range_flag = false;
    let mut colour_description_present_flag = false;
    let mut colour_primaries = 2;
    let mut transfer_characteristics = 2;
    let mut matrix_coeffs = 2;
    if video_signal_type_present_flag {
        video_format = bits.read_bits(3, "video_format")?;
        video_full_range_flag = bits.read_bool("video_full_range_flag")?;
        colour_description_present_flag = bits.read_bool("colour_description_present_flag")?;
        if colour_description_present_flag {
            colour_primaries = bits.read_bits(8, "colour_primaries")?;
            transfer_characteristics = bits.read_bits(8, "transfer_characteristics")?;
            matrix_coeffs = bits.read_bits(8, "matrix_coeffs")?;
        }
    }

    let chroma_loc_info_present_flag = bits.read_bool("chroma_loc_info_present_flag")?;
    let mut chroma_sample_loc_type_top_field = 0;
    let mut chroma_sample_loc_type_bottom_field = 0;
    if chroma_loc_info_present_flag {
        chroma_sample_loc_type_top_field = bits.read_ue("chroma_sample_loc_type_top_field")?;
        chroma_sample_loc_type_bottom_field =
            bits.read_ue("chroma_sample_loc_type_bottom_field")?;
    }

    let timing_info_present_flag = bits.read_bool("timing_info_present_flag")?;
    let mut num_units_in_tick = 0;
    let mut time_scale = 0;
    let mut fixed_frame_rate_flag = false;
    if timing_info_present_flag {
        num_units_in_tick = bits.read_bits(32, "num_units_in_tick")?;
        time_scale = bits.read_bits(32, "time_scale")?;
        fixed_frame_rate_flag = bits.read_bool("fixed_frame_rate_flag")?;
    }

    let nal_hrd_parameters_present_flag = bits.read_bool("nal_hrd_parameters_present_flag")?;
    if nal_hrd_parameters_present_flag {
        native_vulkan_skip_h264_hrd_parameters(bits)?;
    }
    let vcl_hrd_parameters_present_flag = bits.read_bool("vcl_hrd_parameters_present_flag")?;
    if vcl_hrd_parameters_present_flag {
        native_vulkan_skip_h264_hrd_parameters(bits)?;
    }
    let low_delay_hrd_flag = if nal_hrd_parameters_present_flag || vcl_hrd_parameters_present_flag {
        bits.read_bool("low_delay_hrd_flag")?
    } else {
        false
    };
    let pic_struct_present_flag = bits.read_bool("pic_struct_present_flag")?;

    let mut bitstream_restriction_flag = false;
    let mut motion_vectors_over_pic_boundaries_flag = false;
    let mut max_bytes_per_pic_denom = 0;
    let mut max_bits_per_mb_denom = 0;
    let mut log2_max_mv_length_horizontal = 0;
    let mut log2_max_mv_length_vertical = 0;
    let mut num_reorder_frames = 0;
    let mut max_dec_frame_buffering = 0;
    if native_vulkan_rbsp_more_data(rbsp_payload, bits.bit_offset()) {
        bitstream_restriction_flag = bits.read_bool("bitstream_restriction_flag")?;
        if bitstream_restriction_flag {
            motion_vectors_over_pic_boundaries_flag =
                bits.read_bool("motion_vectors_over_pic_boundaries_flag")?;
            max_bytes_per_pic_denom = bits.read_ue("max_bytes_per_pic_denom")?;
            max_bits_per_mb_denom = bits.read_ue("max_bits_per_mb_denom")?;
            log2_max_mv_length_horizontal = bits.read_ue("log2_max_mv_length_horizontal")?;
            log2_max_mv_length_vertical = bits.read_ue("log2_max_mv_length_vertical")?;
            num_reorder_frames = bits.read_ue("num_reorder_frames")?;
            max_dec_frame_buffering = bits.read_ue("max_dec_frame_buffering")?;
            if num_reorder_frames > 16 {
                return Err(format!(
                    "H.264 num_reorder_frames {num_reorder_frames} exceeds Vulkan Video DPB bound"
                ));
            }
        }
    }

    Ok(NativeVulkanH264VuiSnapshot {
        aspect_ratio_info_present_flag,
        aspect_ratio_idc,
        sar_width,
        sar_height,
        overscan_info_present_flag,
        overscan_appropriate_flag,
        video_signal_type_present_flag,
        video_format,
        video_full_range_flag,
        colour_description_present_flag,
        colour_primaries,
        transfer_characteristics,
        matrix_coeffs,
        chroma_loc_info_present_flag,
        chroma_sample_loc_type_top_field,
        chroma_sample_loc_type_bottom_field,
        timing_info_present_flag,
        num_units_in_tick,
        time_scale,
        fixed_frame_rate_flag,
        nal_hrd_parameters_present_flag,
        vcl_hrd_parameters_present_flag,
        low_delay_hrd_flag,
        pic_struct_present_flag,
        bitstream_restriction_flag,
        motion_vectors_over_pic_boundaries_flag,
        max_bytes_per_pic_denom,
        max_bits_per_mb_denom,
        log2_max_mv_length_horizontal,
        log2_max_mv_length_vertical,
        num_reorder_frames,
        max_dec_frame_buffering,
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_skip_h264_hrd_parameters(
    bits: &mut NativeVulkanH264BitReader<'_>,
) -> Result<(), String> {
    let cpb_cnt_minus1 = bits.read_ue("cpb_cnt_minus1")?;
    if cpb_cnt_minus1 > 31 {
        return Err(format!(
            "H.264 cpb_cnt_minus1 {cpb_cnt_minus1} exceeds HRD bound"
        ));
    }
    bits.read_bits(4, "bit_rate_scale")?;
    bits.read_bits(4, "cpb_size_scale")?;
    for _ in 0..=cpb_cnt_minus1 {
        bits.read_ue("bit_rate_value_minus1")?;
        bits.read_ue("cpb_size_value_minus1")?;
        bits.read_bool("cbr_flag")?;
    }
    bits.read_bits(5, "initial_cpb_removal_delay_length_minus1")?;
    bits.read_bits(5, "cpb_removal_delay_length_minus1")?;
    bits.read_bits(5, "dpb_output_delay_length_minus1")?;
    bits.read_bits(5, "time_offset_length")?;
    Ok(())
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h264_chroma_format_label(chroma_format_idc: u32) -> &'static str {
    match chroma_format_idc {
        0 => "monochrome",
        1 => "4:2:0",
        2 => "4:2:2",
        3 => "4:4:4",
        _ => "unknown",
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h264_level_idc_byte_label(level_idc: u8) -> Option<&'static str> {
    match level_idc {
        10 => Some("1.0"),
        11 => Some("1.1"),
        12 => Some("1.2"),
        13 => Some("1.3"),
        20 => Some("2.0"),
        21 => Some("2.1"),
        22 => Some("2.2"),
        30 => Some("3.0"),
        31 => Some("3.1"),
        32 => Some("3.2"),
        40 => Some("4.0"),
        41 => Some("4.1"),
        42 => Some("4.2"),
        50 => Some("5.0"),
        51 => Some("5.1"),
        52 => Some("5.2"),
        60 => Some("6.0"),
        61 => Some("6.1"),
        62 => Some("6.2"),
        _ => None,
    }
}

pub(super) fn native_vulkan_h264_u8(value: u32, label: &'static str) -> Result<u8, String> {
    u8::try_from(value).map_err(|_| format!("{label}={value} exceeds u8 range"))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h264_u16(value: u32, label: &'static str) -> Result<u16, String> {
    u16::try_from(value).map_err(|_| format!("{label}={value} exceeds u16 range"))
}

pub(super) fn native_vulkan_h264_i8(value: i32, label: &'static str) -> Result<i8, String> {
    i8::try_from(value).map_err(|_| format!("{label}={value} exceeds i8 range"))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h264_rbsp(payload: &[u8]) -> Result<Cow<'_, [u8]>, String> {
    if payload.is_empty() {
        return Err("H.264 NAL payload is empty".to_owned());
    }
    Ok(native_vulkan_rbsp_unescape(payload))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_rbsp_unescape(payload: &[u8]) -> Cow<'_, [u8]> {
    let mut zero_count = 0u8;
    let mut first_escape = None;
    for (index, byte) in payload.iter().copied().enumerate() {
        if zero_count == 2 && byte == 0x03 {
            first_escape = Some(index);
            break;
        }
        if byte == 0 {
            zero_count = zero_count.saturating_add(1).min(2);
        } else {
            zero_count = 0;
        }
    }

    let Some(first_escape) = first_escape else {
        return Cow::Borrowed(payload);
    };

    let mut rbsp = Vec::with_capacity(payload.len());
    rbsp.extend_from_slice(&payload[..first_escape]);
    zero_count = 2;
    for byte in payload[first_escape..].iter().copied() {
        if zero_count == 2 && byte == 0x03 {
            zero_count = 0;
            continue;
        }
        rbsp.push(byte);
        if byte == 0 {
            zero_count = zero_count.saturating_add(1).min(2);
        } else {
            zero_count = 0;
        }
    }
    Cow::Owned(rbsp)
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_rbsp_more_data(bytes: &[u8], bit_offset: usize) -> bool {
    let total_bits = bytes.len().saturating_mul(8);
    if bit_offset >= total_bits {
        return false;
    }
    let mut last_one_bit = None;
    for bit in bit_offset..total_bits {
        let byte = bytes[bit / 8];
        let shift = 7 - (bit % 8);
        if ((byte >> shift) & 1) != 0 {
            last_one_bit = Some(bit);
        }
    }
    last_one_bit.is_some_and(|last| bit_offset < last)
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy)]
struct NativeVulkanH264NalPayload<'a> {
    nal_type: u8,
    nal_ref_idc: u8,
    payload: &'a [u8],
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h264_nal_payloads(bytes: &[u8]) -> Vec<NativeVulkanH264NalPayload<'_>> {
    let mut payloads = Vec::new();
    let mut offset = 0usize;
    while let Some((_, payload_offset)) = native_vulkan_next_annex_b_start_code(bytes, offset) {
        let next_start = native_vulkan_next_annex_b_start_code(bytes, payload_offset)
            .map(|(next_start, _)| next_start)
            .unwrap_or(bytes.len());
        if payload_offset < next_start
            && let Some(header) = bytes.get(payload_offset).copied()
        {
            payloads.push(NativeVulkanH264NalPayload {
                nal_type: header & 0x1f,
                nal_ref_idc: (header >> 5) & 0x03,
                payload: &bytes[payload_offset..next_start],
            });
        }
        offset = next_start;
    }
    payloads
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h264_annex_b_slice_offset(
    start_code_offset: usize,
    payload_offset: usize,
) -> usize {
    payload_offset
        .checked_sub(3)
        .filter(|offset| *offset >= start_code_offset)
        .unwrap_or(start_code_offset)
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_read_bits_be(
    bytes: &[u8],
    bit_offset: &mut usize,
    count: u32,
    label: &'static str,
    bounds_label: &'static str,
) -> Result<u32, String> {
    if count > 32 {
        return Err(format!("{label} requested too many bits: {count}"));
    }
    if count == 0 {
        return Ok(0);
    }
    let start = *bit_offset;
    let end = start
        .checked_add(count as usize)
        .ok_or_else(|| format!("{label} bit offset overflow"))?;
    if end > bytes.len() * 8 {
        return Err(format!("{label} exceeds {bounds_label}"));
    }

    // FFmpeg's get_bits() reads a cached word and advances the bit index
    // (references/ffmpeg/libavcodec/get_bits.h:337-350). Keep the same shape
    // instead of looping one bit at a time in H.264/H.265 slice parsing.
    let byte_start = start / 8;
    let bit_in_byte = start % 8;
    let byte_count = bit_in_byte
        .checked_add(count as usize)
        .ok_or_else(|| format!("{label} bit window overflow"))?
        .div_ceil(8);
    let mut window = 0u64;
    for byte in &bytes[byte_start..byte_start + byte_count] {
        window = (window << 8) | u64::from(*byte);
    }
    let total_bits = byte_count * 8;
    let shift = total_bits - bit_in_byte - count as usize;
    let mask = if count == 32 {
        u64::from(u32::MAX)
    } else {
        (1u64 << count) - 1
    };
    *bit_offset = end;
    Ok(((window >> shift) & mask) as u32)
}

struct NativeVulkanH264BitReader<'a> {
    bytes: &'a [u8],
    bit_offset: usize,
}

#[cfg(any(feature = "native-vulkan-video", test))]
impl<'a> NativeVulkanH264BitReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            bit_offset: 0,
        }
    }

    fn bit_offset(&self) -> usize {
        self.bit_offset
    }

    fn read_bool(&mut self, label: &'static str) -> Result<bool, String> {
        Ok(self.read_bits(1, label)? != 0)
    }

    fn read_bits(&mut self, count: u32, label: &'static str) -> Result<u32, String> {
        native_vulkan_read_bits_be(
            self.bytes,
            &mut self.bit_offset,
            count,
            label,
            "H.264 RBSP length",
        )
    }

    fn read_ue(&mut self, label: &'static str) -> Result<u32, String> {
        let mut leading_zero_bits = 0u32;
        while !self.read_bool(label)? {
            leading_zero_bits += 1;
            if leading_zero_bits > 31 {
                return Err(format!("{label} Exp-Golomb code is too large"));
            }
        }
        if leading_zero_bits == 0 {
            return Ok(0);
        }
        let suffix = self.read_bits(leading_zero_bits, label)?;
        Ok((1u32 << leading_zero_bits) - 1 + suffix)
    }

    fn read_se(&mut self, label: &'static str) -> Result<i32, String> {
        let value = self.read_ue(label)?;
        let signed = value.div_ceil(2) as i32;
        if value % 2 == 0 {
            Ok(-signed)
        } else {
            Ok(signed)
        }
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h264_chroma_array_type(sps: &NativeVulkanH264SpsSnapshot) -> u32 {
    if sps.separate_colour_plane_flag {
        0
    } else {
        sps.chroma_format_idc
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h264_skip_pred_weight_table(
    bits: &mut NativeVulkanH264BitReader<'_>,
    parameter_sets: &NativeVulkanH264ParameterSetSnapshot,
    is_p: bool,
    is_b: bool,
    num_ref_idx_l0_active_minus1: Option<u32>,
    num_ref_idx_l1_active_minus1: Option<u32>,
) -> Result<(), String> {
    let weighted_p = parameter_sets.pps.weighted_pred_flag && is_p;
    let explicit_weighted_b = parameter_sets.pps.weighted_bipred_idc == 1 && is_b;
    if !weighted_p && !explicit_weighted_b {
        return Ok(());
    }

    bits.read_ue("luma_log2_weight_denom")?;
    let has_chroma = native_vulkan_h264_chroma_array_type(&parameter_sets.sps) != 0;
    if has_chroma {
        bits.read_ue("chroma_log2_weight_denom")?;
    }
    let l0_count = num_ref_idx_l0_active_minus1
        .ok_or_else(|| "H.264 weighted prediction table is missing L0 ref count".to_owned())?
        .checked_add(1)
        .ok_or_else(|| "H.264 weighted prediction L0 ref count overflow".to_owned())?;
    native_vulkan_h264_skip_pred_weight_table_entries(bits, l0_count, has_chroma)?;
    if explicit_weighted_b {
        let l1_count = num_ref_idx_l1_active_minus1
            .ok_or_else(|| "H.264 weighted prediction table is missing L1 ref count".to_owned())?
            .checked_add(1)
            .ok_or_else(|| "H.264 weighted prediction L1 ref count overflow".to_owned())?;
        native_vulkan_h264_skip_pred_weight_table_entries(bits, l1_count, has_chroma)?;
    }

    Ok(())
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h264_skip_pred_weight_table_entries(
    bits: &mut NativeVulkanH264BitReader<'_>,
    count: u32,
    has_chroma: bool,
) -> Result<(), String> {
    if count > 32 {
        return Err(format!(
            "H.264 weighted prediction ref count {count} exceeds supported parser bound"
        ));
    }
    for _ in 0..count {
        if bits.read_bool("luma_weight_flag")? {
            bits.read_se("luma_weight")?;
            bits.read_se("luma_offset")?;
        }
        if has_chroma && bits.read_bool("chroma_weight_flag")? {
            for _ in 0..2 {
                bits.read_se("chroma_weight")?;
                bits.read_se("chroma_offset")?;
            }
        }
    }

    Ok(())
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h264_read_ref_pic_list_modifications(
    bits: &mut NativeVulkanH264BitReader<'_>,
    flag_label: &'static str,
    list_label: &'static str,
) -> Result<(bool, Vec<NativeVulkanH264RefPicListModificationSnapshot>), String> {
    let modification_flag = bits.read_bool(flag_label)?;
    let mut modifications = Vec::<NativeVulkanH264RefPicListModificationSnapshot>::new();
    if !modification_flag {
        return Ok((false, modifications));
    }

    loop {
        let modification_of_pic_nums_idc = bits.read_ue("modification_of_pic_nums_idc")?;
        if modification_of_pic_nums_idc == 3 {
            break;
        }
        let (abs_diff_pic_num_minus1, long_term_pic_num) = match modification_of_pic_nums_idc {
            0 | 1 => (Some(bits.read_ue("abs_diff_pic_num_minus1")?), None),
            2 => (None, Some(bits.read_ue("long_term_pic_num")?)),
            other => {
                return Err(format!(
                    "H.264 ref_pic_list_modification_{list_label} idc {other} is not supported"
                ));
            }
        };
        modifications.push(NativeVulkanH264RefPicListModificationSnapshot {
            modification_of_pic_nums_idc,
            abs_diff_pic_num_minus1,
            long_term_pic_num,
        });
    }

    Ok((true, modifications))
}

#[derive(Debug, Clone, Copy)]
struct NativeVulkanH265NalPayload<'a> {
    nal_type: u8,
    #[cfg_attr(not(test), allow(dead_code))]
    start_code_offset: usize,
    slice_segment_offset: usize,
    #[cfg_attr(not(test), allow(dead_code))]
    payload_offset: usize,
    #[cfg_attr(not(feature = "native-vulkan-video"), allow(dead_code))]
    payload: &'a [u8],
}

fn native_vulkan_h265_nal_payloads(bytes: &[u8]) -> Vec<NativeVulkanH265NalPayload<'_>> {
    let mut payloads = Vec::new();
    let mut offset = 0usize;
    while let Some((start_code_offset, payload_offset)) =
        native_vulkan_next_annex_b_start_code(bytes, offset)
    {
        let next_start = native_vulkan_next_annex_b_start_code(bytes, payload_offset)
            .map(|(next_start, _)| next_start)
            .unwrap_or(bytes.len());
        if payload_offset < next_start
            && let Some(nal_type) = bytes.get(payload_offset).map(|header| (header >> 1) & 0x3f)
        {
            payloads.push(NativeVulkanH265NalPayload {
                nal_type,
                start_code_offset,
                slice_segment_offset: native_vulkan_h265_annex_b_slice_segment_offset(
                    start_code_offset,
                    payload_offset,
                ),
                payload_offset,
                payload: &bytes[payload_offset..next_start],
            });
        }
        offset = next_start;
    }
    payloads
}

fn native_vulkan_h265_annex_b_slice_segment_offset(
    start_code_offset: usize,
    payload_offset: usize,
) -> usize {
    payload_offset
        .checked_sub(3)
        .filter(|offset| *offset >= start_code_offset)
        .unwrap_or(start_code_offset)
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeVulkanH264FirstFrameDecodeInfo {
    nal_type: u8,
    nal_type_label: &'static str,
    nal_ref_idc: u8,
    first_mb_in_slice: u32,
    first_slice_segment_in_pic_flag: bool,
    slice_type: u32,
    slice_type_normalized: u32,
    pps_id: u32,
    frame_num: u16,
    idr_pic_id: u16,
    num_ref_idx_l0_active_minus1: Option<u32>,
    num_ref_idx_l1_active_minus1: Option<u32>,
    ref_pic_list_modification_l0: bool,
    ref_pic_list_modifications_l0: Vec<NativeVulkanH264RefPicListModificationSnapshot>,
    ref_pic_list_modification_l1: bool,
    ref_pic_list_modifications_l1: Vec<NativeVulkanH264RefPicListModificationSnapshot>,
    adaptive_ref_pic_marking_mode_flag: bool,
    memory_management_control_operations:
        Vec<NativeVulkanH264MemoryManagementControlOperationSnapshot>,
    field_pic_flag: bool,
    bottom_field_flag: bool,
    is_reference: bool,
    is_intra: bool,
    is_p: bool,
    is_b: bool,
    long_term_reference_flag: bool,
    pic_order_cnt: [i32; 2],
    slice_offsets: NativeVulkanH264SliceOffsets,
    idr: bool,
    irap: bool,
}

#[cfg(test)]
fn native_vulkan_h264_first_frame_decode_info(
    access_unit: &[u8],
    parameter_sets: &NativeVulkanH264ParameterSetSnapshot,
) -> Result<NativeVulkanH264FirstFrameDecodeInfo, String> {
    let picture = native_vulkan_h264_picture_decode_info(access_unit, parameter_sets, 0)?;
    if !picture.idr {
        return Err(format!(
            "H.264 first-frame decode currently supports IDR only, got {}",
            picture.nal_type_label
        ));
    }
    if !picture.is_intra {
        return Err(format!(
            "H.264 IDR first slice must be I-slice for the first decode subset, got {}",
            picture.slice_type
        ));
    }
    Ok(picture)
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h264_picture_decode_info_from_stats(
    access_unit: &[u8],
    stats: &NativeVulkanH264NalStats,
    parameter_sets: &NativeVulkanH264ParameterSetSnapshot,
) -> Result<NativeVulkanH264FirstFrameDecodeInfo, String> {
    let first_slice = stats
        .first_slice
        .ok_or_else(|| "H.264 access unit has no slice NAL".to_owned())?;
    if stats.slice_offsets.is_empty() {
        return Err("H.264 access unit has no slice offsets".to_owned());
    }
    if first_slice.payload_start >= first_slice.payload_end
        || first_slice.payload_end > access_unit.len()
    {
        return Err("H.264 first slice payload range exceeds access-unit bounds".to_owned());
    }

    let slice = NativeVulkanH264NalPayload {
        nal_type: first_slice.nal_type,
        nal_ref_idc: first_slice.nal_ref_idc,
        payload: &access_unit[first_slice.payload_start..first_slice.payload_end],
    };
    let mut first_slice = native_vulkan_h264_slice_decode_info(&slice, parameter_sets)?;
    first_slice.slice_offsets = stats.slice_offsets.clone();
    Ok(first_slice)
}

#[cfg(test)]
fn native_vulkan_h264_picture_decode_info(
    access_unit: &[u8],
    parameter_sets: &NativeVulkanH264ParameterSetSnapshot,
    _slice_count_hint: usize,
) -> Result<NativeVulkanH264FirstFrameDecodeInfo, String> {
    let mut first_slice = None;
    let mut slice_offsets = NativeVulkanH264SliceOffsets::new();
    let mut offset = 0usize;
    while let Some((start_code_offset, payload_offset)) =
        native_vulkan_next_annex_b_start_code(access_unit, offset)
    {
        let next_start = native_vulkan_next_annex_b_start_code(access_unit, payload_offset)
            .map(|(next_start, _)| next_start)
            .unwrap_or(access_unit.len());
        if payload_offset < next_start
            && let Some(header) = access_unit.get(payload_offset).copied()
        {
            let nal_type = header & 0x1f;
            if matches!(nal_type, 1..=5) {
                let slice = NativeVulkanH264NalPayload {
                    nal_type,
                    nal_ref_idc: (header >> 5) & 0x03,
                    payload: &access_unit[payload_offset..next_start],
                };
                let slice_offset =
                    native_vulkan_h264_annex_b_slice_offset(start_code_offset, payload_offset);
                if first_slice.is_none() {
                    first_slice = Some(native_vulkan_h264_slice_decode_info(
                        &slice,
                        parameter_sets,
                    )?);
                }
                slice_offsets.push(
                    u32::try_from(slice_offset)
                        .map_err(|_| "H.264 slice offset exceeds u32 range".to_owned())?,
                );
            }
        }
        offset = next_start;
    }
    let mut first_slice =
        first_slice.ok_or_else(|| "H.264 access unit has no slice NAL".to_owned())?;
    if slice_offsets.is_empty() {
        return Err("H.264 access unit has no slice offsets".to_owned());
    }
    // FFmpeg's Vulkan H.264 path takes the already-parsed first slice context
    // as picture info and appends every NAL through ff_vk_decode_add_slice(),
    // which only grows the reusable slice-offset array.
    // See references/ffmpeg/libavcodec/vulkan_h264.c:481-495 and
    // references/ffmpeg/libavcodec/vulkan_decode.c:309-340.
    first_slice.slice_offsets = slice_offsets;
    Ok(first_slice)
}
