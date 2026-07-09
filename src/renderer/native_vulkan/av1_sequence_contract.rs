
#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_tile_info(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
    frame_width: u32,
    frame_height: u32,
) -> Result<NativeVulkanAv1ParsedTileInfo, String> {
    let sb_size = if sequence_header.use_128x128_superblock {
        128
    } else {
        64
    };
    let sb_cols = frame_width.div_ceil(sb_size);
    let sb_rows = frame_height.div_ceil(sb_size);
    let mi_size_per_sb = if sequence_header.use_128x128_superblock {
        32u32
    } else {
        16u32
    };
    let max_tile_width_sb: u32 = if sequence_header.use_128x128_superblock {
        32
    } else {
        64
    };
    let max_tile_area_sb: u32 = if sequence_header.use_128x128_superblock {
        576
    } else {
        2304
    };
    let uniform_tile_spacing_flag = bits.read_bool("uniform_tile_spacing_flag")?;
    if uniform_tile_spacing_flag {
        let mut min_log2_tile_cols = 0u32;
        while (max_tile_width_sb << min_log2_tile_cols) < sb_cols {
            min_log2_tile_cols = min_log2_tile_cols.saturating_add(1);
        }
        let max_log2_tile_cols = native_vulkan_av1_ceil_log2(sb_cols);
        let mut tile_cols_log2 = min_log2_tile_cols;
        while tile_cols_log2 < max_log2_tile_cols && bits.read_bool("increment_tile_cols_log2")? {
            tile_cols_log2 = tile_cols_log2.saturating_add(1);
        }
        let tile_width_divisor = 1u32
            .checked_shl(tile_cols_log2)
            .ok_or_else(|| "AV1 tile_cols_log2 overflow".to_owned())?;
        let tile_width_sb =
            sb_cols.saturating_add(tile_width_divisor).saturating_sub(1) / tile_width_divisor;
        let tile_columns = sb_cols.div_ceil(tile_width_sb.max(1));

        let mut min_log2_tile_rows = 0u32;
        while max_tile_area_sb
            .checked_shr(tile_cols_log2.saturating_add(min_log2_tile_rows))
            .unwrap_or(0)
            < tile_width_sb.saturating_mul(sb_rows)
        {
            min_log2_tile_rows = min_log2_tile_rows.saturating_add(1);
        }
        let max_log2_tile_rows = native_vulkan_av1_ceil_log2(sb_rows);
        let mut tile_rows_log2 = min_log2_tile_rows.min(max_log2_tile_rows);
        while tile_rows_log2 < max_log2_tile_rows && bits.read_bool("increment_tile_rows_log2")? {
            tile_rows_log2 = tile_rows_log2.saturating_add(1);
        }
        let tile_height_divisor = 1u32
            .checked_shl(tile_rows_log2)
            .ok_or_else(|| "AV1 tile_rows_log2 overflow".to_owned())?;
        let tile_height_sb = sb_rows
            .saturating_add(tile_height_divisor)
            .saturating_sub(1)
            / tile_height_divisor;
        let tile_rows = sb_rows.div_ceil(tile_height_sb.max(1));
        let tile_count = tile_columns.saturating_mul(tile_rows);
        let tile_bits = native_vulkan_av1_ceil_log2(tile_columns)
            .saturating_add(native_vulkan_av1_ceil_log2(tile_rows));
        let (context_update_tile_id, tile_size_bytes) = if tile_count > 1 {
            let context_update_tile_id = native_vulkan_av1_u16(
                bits.read_bits(tile_bits, "context_update_tile_id")?,
                "context_update_tile_id",
            )?;
            let tile_size_bytes = bits
                .read_bits(2, "tile_size_bytes_minus_1")?
                .saturating_add(1);
            (context_update_tile_id, tile_size_bytes)
        } else {
            (0, 0)
        };
        let tile_col_widths = native_vulkan_av1_uniform_tile_sizes(sb_cols, tile_width_sb);
        let tile_row_heights = native_vulkan_av1_uniform_tile_sizes(sb_rows, tile_height_sb);
        let (mi_col_starts, width_in_sbs_minus_1) =
            native_vulkan_av1_tile_axis_layout(&tile_col_widths, mi_size_per_sb)?;
        let (mi_row_starts, height_in_sbs_minus_1) =
            native_vulkan_av1_tile_axis_layout(&tile_row_heights, mi_size_per_sb)?;
        return Ok(NativeVulkanAv1ParsedTileInfo {
            tile_count,
            tile_columns,
            tile_rows,
            tile_size_bytes,
            tile_bits,
            uniform_tile_spacing_flag,
            context_update_tile_id,
            mi_col_starts,
            mi_row_starts,
            width_in_sbs_minus_1,
            height_in_sbs_minus_1,
        });
    }

    let mut tile_col_widths = Vec::new();
    let mut widest_tile_sb = 0u32;
    let mut sofar = 0u32;
    while sofar < sb_cols {
        let max_width = max_tile_width_sb.min(sb_cols - sofar);
        let width = bits
            .read_quniform(max_width, "width_in_sbs_minus_1")?
            .saturating_add(1);
        tile_col_widths.push(width);
        sofar = sofar.saturating_add(width);
        widest_tile_sb = widest_tile_sb.max(width);
    }

    let max_tile_height_sb = (max_tile_area_sb / widest_tile_sb.max(1)).max(1);
    let mut tile_row_heights = Vec::new();
    sofar = 0;
    while sofar < sb_rows {
        let max_height = max_tile_height_sb.min(sb_rows - sofar);
        let height = bits
            .read_quniform(max_height, "height_in_sbs_minus_1")?
            .saturating_add(1);
        tile_row_heights.push(height);
        sofar = sofar.saturating_add(height);
    }

    let tile_columns = tile_col_widths.len() as u32;
    let tile_rows = tile_row_heights.len() as u32;
    let tile_count = tile_columns.saturating_mul(tile_rows);
    let tile_bits = native_vulkan_av1_ceil_log2(tile_columns)
        .saturating_add(native_vulkan_av1_ceil_log2(tile_rows));
    let (context_update_tile_id, tile_size_bytes) = if tile_count > 1 {
        let context_update_tile_id = native_vulkan_av1_u16(
            bits.read_bits(tile_bits, "context_update_tile_id")?,
            "context_update_tile_id",
        )?;
        let tile_size_bytes = bits
            .read_bits(2, "tile_size_bytes_minus_1")?
            .saturating_add(1);
        (context_update_tile_id, tile_size_bytes)
    } else {
        (0, 0)
    };
    let (mi_col_starts, width_in_sbs_minus_1) =
        native_vulkan_av1_tile_axis_layout(&tile_col_widths, mi_size_per_sb)?;
    let (mi_row_starts, height_in_sbs_minus_1) =
        native_vulkan_av1_tile_axis_layout(&tile_row_heights, mi_size_per_sb)?;
    Ok(NativeVulkanAv1ParsedTileInfo {
        tile_count,
        tile_columns,
        tile_rows,
        tile_size_bytes,
        tile_bits,
        uniform_tile_spacing_flag,
        context_update_tile_id,
        mi_col_starts,
        mi_row_starts,
        width_in_sbs_minus_1,
        height_in_sbs_minus_1,
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_uniform_tile_sizes(total_sb: u32, tile_size_sb: u32) -> Vec<u32> {
    let tile_size_sb = tile_size_sb.max(1);
    let mut sizes = Vec::new();
    let mut remaining = total_sb;
    while remaining > 0 {
        let size = remaining.min(tile_size_sb);
        sizes.push(size);
        remaining -= size;
    }
    sizes
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_tile_axis_layout(
    sizes_in_sb: &[u32],
    mi_size_per_sb: u32,
) -> Result<(Vec<u16>, Vec<u16>), String> {
    let mut starts = Vec::with_capacity(sizes_in_sb.len().saturating_add(1));
    let mut sizes_minus_1 = Vec::with_capacity(sizes_in_sb.len());
    let mut cursor = 0u32;
    starts.push(0);
    for size in sizes_in_sb.iter().copied() {
        if size == 0 {
            return Err("AV1 tile axis has a zero-sized tile".to_owned());
        }
        sizes_minus_1.push(u16::try_from(size - 1).map_err(|_| {
            format!(
                "AV1 tile axis size_in_sbs_minus_1 {} exceeds u16 range",
                size - 1
            )
        })?);
        cursor = cursor
            .checked_add(size.saturating_mul(mi_size_per_sb))
            .ok_or_else(|| "AV1 tile axis MI cursor overflow".to_owned())?;
        starts.push(u16::try_from(cursor).map_err(|_| {
            format!("AV1 tile axis MI start {cursor} exceeds Vulkan STD u16 range")
        })?);
    }
    Ok((starts, sizes_minus_1))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_tile_group_offsets_from_payload(
    absolute_payload_base_offset: u64,
    tile_payload_offset: usize,
    tile_payload: &[u8],
    header: &NativeVulkanAv1ParsedFrameHeader,
) -> Result<(Vec<u32>, Vec<u32>), String> {
    if header.tile_count == 0 {
        return Err("AV1 tile_count is zero".to_owned());
    }
    let absolute_payload_base_offset = u32::try_from(absolute_payload_base_offset)
        .map_err(|_| "AV1 tile payload base offset exceeds u32 range".to_owned())?;
    let tile_payload_offset = u32::try_from(tile_payload_offset)
        .map_err(|_| "AV1 tile payload offset exceeds u32 range".to_owned())?;
    let absolute_tile_payload_offset = absolute_payload_base_offset
        .checked_add(tile_payload_offset)
        .ok_or_else(|| "AV1 absolute tile payload offset overflow".to_owned())?;

    if header.tile_count == 1 {
        let leading_padding =
            native_vulkan_av1_single_tile_leading_padding_bytes(header, tile_payload);
        let absolute_tile_offset = absolute_tile_payload_offset
            .checked_add(
                u32::try_from(leading_padding)
                    .map_err(|_| "AV1 single tile padding exceeds u32 range".to_owned())?,
            )
            .ok_or_else(|| "AV1 single tile absolute offset overflow".to_owned())?;
        let size = u32::try_from(tile_payload.len().saturating_sub(leading_padding))
            .map_err(|_| "AV1 single tile payload exceeds u32 range".to_owned())?;
        return Ok((vec![absolute_tile_offset], vec![size]));
    }
    if header.tile_size_bytes == 0 {
        return Err("AV1 multi-tile payload has zero tile_size_bytes".to_owned());
    }

    let mut bits = NativeVulkanAv1BitReader::new(tile_payload);
    let tile_start_and_end_present_flag = bits.read_bool("tile_start_and_end_present_flag")?;
    let (tile_start, tile_end) = if tile_start_and_end_present_flag {
        (
            bits.read_bits(header.tile_bits, "tg_start")?,
            bits.read_bits(header.tile_bits, "tg_end")?,
        )
    } else {
        (0, header.tile_count.saturating_sub(1))
    };
    if tile_start != 0 || tile_end.saturating_add(1) != header.tile_count {
        return Err(format!(
            "AV1 first-frame tile group covers {tile_start}..={tile_end}, expected full 0..={}",
            header.tile_count.saturating_sub(1)
        ));
    }
    bits.zero_align_to_byte("tile_group_header_byte_alignment")?;
    let mut cursor = bits.byte_offset();
    let mut tile_offsets = Vec::with_capacity(header.tile_count as usize);
    let mut tile_sizes = Vec::with_capacity(header.tile_count as usize);
    for tile_index in 0..header.tile_count {
        if cursor > tile_payload.len() {
            return Err("AV1 tile table cursor moved past payload".to_owned());
        }
        let tile_size = if tile_index + 1 == header.tile_count {
            tile_payload.len().saturating_sub(cursor)
        } else {
            let size_bytes = header.tile_size_bytes as usize;
            let size_end = cursor
                .checked_add(size_bytes)
                .ok_or_else(|| "AV1 tile size cursor overflow".to_owned())?;
            let size_field = tile_payload
                .get(cursor..size_end)
                .ok_or_else(|| format!("AV1 tile {tile_index} size field exceeds tile payload"))?;
            cursor = size_end;
            native_vulkan_av1_read_le_uint(size_field)
                .and_then(|value| value.checked_add(1).ok_or(()))
                .map_err(|_| format!("AV1 tile {tile_index} size overflow"))? as usize
        };
        let absolute_offset = absolute_tile_payload_offset
            .checked_add(
                u32::try_from(cursor)
                    .map_err(|_| "AV1 tile offset cursor exceeds u32 range".to_owned())?,
            )
            .ok_or_else(|| "AV1 tile absolute offset overflow".to_owned())?;
        let tile_size_u32 = u32::try_from(tile_size)
            .map_err(|_| format!("AV1 tile {tile_index} size exceeds u32 range"))?;
        tile_offsets.push(absolute_offset);
        tile_sizes.push(tile_size_u32);
        cursor = cursor
            .checked_add(tile_size)
            .ok_or_else(|| "AV1 tile cursor overflow".to_owned())?;
    }
    if cursor != tile_payload.len() {
        return Err(format!(
            "AV1 tile table consumed {cursor} bytes but payload has {} bytes",
            tile_payload.len()
        ));
    }
    Ok((tile_offsets, tile_sizes))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_single_tile_leading_padding_bytes(
    header: &NativeVulkanAv1ParsedFrameHeader,
    tile_payload: &[u8],
) -> usize {
    if header.frame_type == 1
        && header.tile_count == 1
        && header.tile_columns == 1
        && header.tile_rows == 1
        && tile_payload.len() > 1
        && tile_payload.first().copied() == Some(0)
    {
        1
    } else {
        0
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_quantization_params(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
) -> Result<NativeVulkanAv1ParsedQuantization, String> {
    let base_q_idx = native_vulkan_av1_u8(bits.read_bits(8, "base_q_idx")?, "base_q_idx")?;
    let delta_q_y_dc = native_vulkan_av1_read_delta_q(bits, "delta_q_y_dc")?;
    let mut delta_q_u_dc = 0;
    let mut delta_q_u_ac = 0;
    let mut delta_q_v_dc = 0;
    let mut delta_q_v_ac = 0;
    let mut diff_uv_delta = false;
    if sequence_header.color_config.num_planes > 1 {
        diff_uv_delta = if sequence_header.color_config.separate_uv_delta_q {
            bits.read_bool("diff_uv_delta")?
        } else {
            false
        };
        delta_q_u_dc = native_vulkan_av1_read_delta_q(bits, "delta_q_u_dc")?;
        delta_q_u_ac = native_vulkan_av1_read_delta_q(bits, "delta_q_u_ac")?;
        if diff_uv_delta {
            delta_q_v_dc = native_vulkan_av1_read_delta_q(bits, "delta_q_v_dc")?;
            delta_q_v_ac = native_vulkan_av1_read_delta_q(bits, "delta_q_v_ac")?;
        } else {
            delta_q_v_dc = delta_q_u_dc;
            delta_q_v_ac = delta_q_u_ac;
        }
    }
    let using_qmatrix = bits.read_bool("using_qmatrix")?;
    let mut qm_y = 0;
    let mut qm_u = 0;
    let mut qm_v = 0;
    if using_qmatrix {
        qm_y = native_vulkan_av1_u8(bits.read_bits(4, "qm_y")?, "qm_y")?;
        qm_u = native_vulkan_av1_u8(bits.read_bits(4, "qm_u")?, "qm_u")?;
        if sequence_header.color_config.separate_uv_delta_q {
            qm_v = native_vulkan_av1_u8(bits.read_bits(4, "qm_v")?, "qm_v")?;
        } else {
            qm_v = qm_u;
        }
    }
    Ok(NativeVulkanAv1ParsedQuantization {
        base_q_idx,
        delta_q_y_dc,
        delta_q_u_dc,
        delta_q_u_ac,
        delta_q_v_dc,
        delta_q_v_ac,
        using_qmatrix,
        diff_uv_delta,
        qm_y,
        qm_u,
        qm_v,
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_segmentation_params(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    primary_ref_frame: Option<u8>,
    primary_reference_history: Option<NativeVulkanAv1ReferenceHistory>,
) -> Result<NativeVulkanAv1ParsedSegmentation, String> {
    let segmentation_enabled = bits.read_bool("segmentation_enabled")?;
    let mut segmentation = NativeVulkanAv1ParsedSegmentation {
        enabled: segmentation_enabled,
        update_map: false,
        temporal_update: false,
        update_data: false,
        feature_enabled: [0; 8],
        feature_data: [[0; 8]; 8],
    };
    if !segmentation_enabled {
        return Ok(segmentation);
    }

    let primary_ref_none = native_vulkan_av1_primary_ref_none(primary_ref_frame);
    let segmentation_update_map = if primary_ref_none {
        true
    } else {
        bits.read_bool("segmentation_update_map")?
    };
    segmentation.update_map = segmentation_update_map;
    if segmentation_update_map && !primary_ref_none {
        segmentation.temporal_update = bits.read_bool("segmentation_temporal_update")?;
    }
    let segmentation_update_data = if primary_ref_none {
        true
    } else {
        bits.read_bool("segmentation_update_data")?
    };
    segmentation.update_data = segmentation_update_data;
    if segmentation_update_data {
        const AV1_SEGMENT_FEATURE_BITS: [u32; 8] = [8, 6, 6, 6, 6, 3, 0, 0];
        const AV1_SEGMENT_FEATURE_SIGNED: [bool; 8] =
            [true, true, true, true, true, false, false, false];
        for segment_index in 0..8 {
            for feature_index in 0..8 {
                if bits.read_bool("segmentation_feature_enabled")? {
                    segmentation.feature_enabled[segment_index] |= 1u8 << feature_index;
                    let feature_bits = AV1_SEGMENT_FEATURE_BITS[feature_index];
                    let mut feature_value = if feature_bits > 0 {
                        i16::try_from(bits.read_bits(feature_bits, "segmentation_feature_value")?)
                            .map_err(|_| "AV1 segmentation feature value exceeds i16".to_owned())?
                    } else {
                        0
                    };
                    if AV1_SEGMENT_FEATURE_SIGNED[feature_index]
                        && feature_value != 0
                        && bits.read_bool("segmentation_feature_sign")?
                    {
                        feature_value = -feature_value;
                    }
                    segmentation.feature_data[segment_index][feature_index] = feature_value;
                }
            }
        }
    } else if let Some(history) = primary_reference_history {
        segmentation.feature_enabled = history.segmentation.feature_enabled;
        segmentation.feature_data = history.segmentation.feature_data;
    }
    Ok(segmentation)
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_zero_align_to_byte_with_reason(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    label: &'static str,
) -> Result<Option<String>, String> {
    while !bits.bit_offset.is_multiple_of(8) {
        let _ = bits.read_bool(label)?;
    }
    Ok(None)
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_delta_q_params(
    bits: &mut NativeVulkanAv1BitReader<'_>,
) -> Result<NativeVulkanAv1ParsedDeltaQ, String> {
    let delta_q_present = bits.read_bool("delta_q_present")?;
    let mut delta_q_res = 0;
    if delta_q_present {
        delta_q_res = native_vulkan_av1_u8(bits.read_bits(2, "delta_q_res")?, "delta_q_res")?;
    }
    Ok(NativeVulkanAv1ParsedDeltaQ {
        present: delta_q_present,
        res: delta_q_res,
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_delta_lf_params(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    delta_q_present: bool,
) -> Result<NativeVulkanAv1ParsedDeltaLf, String> {
    let mut delta_lf = NativeVulkanAv1ParsedDeltaLf {
        present: false,
        res: 0,
        multi: false,
    };
    if delta_q_present {
        let delta_lf_present = bits.read_bool("delta_lf_present")?;
        delta_lf.present = delta_lf_present;
        if delta_lf_present {
            delta_lf.res =
                native_vulkan_av1_u8(bits.read_bits(2, "delta_lf_res")?, "delta_lf_res")?;
            delta_lf.multi = bits.read_bool("delta_lf_multi")?;
        }
    }
    Ok(delta_lf)
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_loop_filter_params(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
    primary_reference_history: Option<NativeVulkanAv1ReferenceHistory>,
) -> Result<NativeVulkanAv1ParsedLoopFilter, String> {
    let loop_filter_level_0 = native_vulkan_av1_u8(
        bits.read_bits(6, "loop_filter_level_0")?,
        "loop_filter_level_0",
    )?;
    let loop_filter_level_1 = native_vulkan_av1_u8(
        bits.read_bits(6, "loop_filter_level_1")?,
        "loop_filter_level_1",
    )?;
    let mut level = [loop_filter_level_0, loop_filter_level_1, 0, 0];
    if sequence_header.color_config.num_planes > 1
        && (loop_filter_level_0 > 0 || loop_filter_level_1 > 0)
    {
        level[2] = native_vulkan_av1_u8(
            bits.read_bits(6, "loop_filter_level_2")?,
            "loop_filter_level_2",
        )?;
        level[3] = native_vulkan_av1_u8(
            bits.read_bits(6, "loop_filter_level_3")?,
            "loop_filter_level_3",
        )?;
    }
    let sharpness = native_vulkan_av1_u8(
        bits.read_bits(3, "loop_filter_sharpness")?,
        "loop_filter_sharpness",
    )?;
    let loop_filter_delta_enabled = bits.read_bool("loop_filter_delta_enabled")?;
    let inherited_ref_deltas = primary_reference_history
        .map(|history| history.loop_filter_ref_deltas)
        .unwrap_or([1, 0, 0, 0, -1, 0, -1, -1]);
    let inherited_mode_deltas = primary_reference_history
        .map(|history| history.loop_filter_mode_deltas)
        .unwrap_or([0, 0]);
    let mut loop_filter = NativeVulkanAv1ParsedLoopFilter {
        level,
        sharpness,
        delta_enabled: loop_filter_delta_enabled,
        delta_update: false,
        update_ref_delta: 0,
        ref_deltas: inherited_ref_deltas,
        update_mode_delta: 0,
        mode_deltas: inherited_mode_deltas,
    };
    if loop_filter_delta_enabled {
        let loop_filter_delta_update = bits.read_bool("loop_filter_delta_update")?;
        loop_filter.delta_update = loop_filter_delta_update;
        if loop_filter_delta_update {
            for ref_index in 0..8 {
                if bits.read_bool("update_ref_delta")? {
                    loop_filter.update_ref_delta |= 1u8 << ref_index;
                    loop_filter.ref_deltas[ref_index] =
                        native_vulkan_av1_read_signed_literal(bits, 7, "loop_filter_ref_delta")?;
                }
            }
            for mode_index in 0..2 {
                if bits.read_bool("update_mode_delta")? {
                    loop_filter.update_mode_delta |= 1u8 << mode_index;
                    loop_filter.mode_deltas[mode_index] =
                        native_vulkan_av1_read_signed_literal(bits, 7, "loop_filter_mode_delta")?;
                }
            }
        }
    }
    Ok(loop_filter)
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_cdef_params(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
) -> Result<NativeVulkanAv1ParsedCdef, String> {
    let mut cdef = NativeVulkanAv1ParsedCdef {
        damping_minus_3: 0,
        bits: 0,
        y_pri_strength: [0; 8],
        y_sec_strength: [0; 8],
        uv_pri_strength: [0; 8],
        uv_sec_strength: [0; 8],
    };
    if sequence_header.enable_cdef {
        cdef.damping_minus_3 = native_vulkan_av1_u8(
            bits.read_bits(2, "cdef_damping_minus_3")?,
            "cdef_damping_minus_3",
        )?;
        cdef.bits = native_vulkan_av1_u8(bits.read_bits(2, "cdef_bits")?, "cdef_bits")?;
        for index in 0..(1usize << cdef.bits) {
            let y_strength =
                native_vulkan_av1_u8(bits.read_bits(6, "cdef_y_strength")?, "cdef_y_strength")?;
            cdef.y_pri_strength[index] = y_strength >> 2;
            cdef.y_sec_strength[index] = y_strength & 0x03;
            if cdef.y_sec_strength[index] == 3 {
                cdef.y_sec_strength[index] = 4;
            }
            if sequence_header.color_config.num_planes > 1 {
                let uv_strength = native_vulkan_av1_u8(
                    bits.read_bits(6, "cdef_uv_strength")?,
                    "cdef_uv_strength",
                )?;
                cdef.uv_pri_strength[index] = uv_strength >> 2;
                cdef.uv_sec_strength[index] = uv_strength & 0x03;
                if cdef.uv_sec_strength[index] == 3 {
                    cdef.uv_sec_strength[index] = 4;
                }
            }
        }
    }
    Ok(cdef)
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_loop_restoration_params(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
) -> Result<NativeVulkanAv1ParsedLoopRestoration, String> {
    let mut loop_restoration = NativeVulkanAv1ParsedLoopRestoration {
        frame_restoration_type: [
            vk::video::STD_VIDEO_AV1_FRAME_RESTORATION_TYPE_NONE.0 as u32,
            vk::video::STD_VIDEO_AV1_FRAME_RESTORATION_TYPE_NONE.0 as u32,
            vk::video::STD_VIDEO_AV1_FRAME_RESTORATION_TYPE_NONE.0 as u32,
        ],
        loop_restoration_size: [0; 3],
        uses_lr: false,
        uses_chroma_lr: false,
    };
    if sequence_header.enable_restoration {
        let planes = sequence_header.color_config.num_planes.max(1);
        let mut use_lrf = false;
        let mut use_chroma_lrf = false;
        for plane in 0..usize::from(planes) {
            let restoration_type = native_vulkan_av1_std_frame_restoration_type(
                bits.read_bits(2, "frame_restoration_type")?,
            )?;
            loop_restoration.frame_restoration_type[plane] = restoration_type;
            if restoration_type != 0 {
                use_lrf = true;
                if plane > 0 {
                    use_chroma_lrf = true;
                }
            }
        }
        if use_lrf {
            let lr_unit_shift = if sequence_header.use_128x128_superblock {
                true
            } else {
                bits.read_bool("lr_unit_shift")?
            };
            let lr_unit_extra_shift = if lr_unit_shift {
                bits.read_bool("lr_unit_extra_shift")?
            } else {
                false
            };
            let luma_size =
                native_vulkan_av1_loop_restoration_size(lr_unit_shift, lr_unit_extra_shift, false)?;
            loop_restoration.loop_restoration_size[0] = luma_size;
            if planes > 1 {
                loop_restoration.loop_restoration_size[1] = luma_size;
                loop_restoration.loop_restoration_size[2] = luma_size;
            }
            if use_chroma_lrf
                && sequence_header.color_config.subsampling_x
                && sequence_header.color_config.subsampling_y
            {
                let lr_uv_shift = bits.read_bool("lr_uv_shift")?;
                let chroma_size = native_vulkan_av1_loop_restoration_size(
                    lr_unit_shift,
                    lr_unit_extra_shift,
                    lr_uv_shift,
                )?;
                loop_restoration.loop_restoration_size[1] = chroma_size;
                loop_restoration.loop_restoration_size[2] = chroma_size;
            }
        }
        loop_restoration.uses_lr = use_lrf;
        loop_restoration.uses_chroma_lr = use_chroma_lrf;
    }
    Ok(loop_restoration)
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_tx_mode(
    bits: &mut NativeVulkanAv1BitReader<'_>,
) -> Result<bool, String> {
    bits.read_bool("tx_mode_select")
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_read_delta_q(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    label: &'static str,
) -> Result<i8, String> {
    if bits.read_bool(label)? {
        native_vulkan_av1_read_signed_literal(bits, 7, label)
    } else {
        Ok(0)
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_read_signed_literal(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    count: u32,
    label: &'static str,
) -> Result<i8, String> {
    if count == 0 || count > 8 {
        return Err(format!(
            "{label} requested invalid signed literal width {count}"
        ));
    }
    let value = bits.read_bits(count, label)? as i32;
    let sign_bit = 1i32 << (count - 1);
    let signed = if value & sign_bit != 0 {
        value - (sign_bit << 1)
    } else {
        value
    };
    i8::try_from(signed).map_err(|_| format!("{label}={signed} exceeds i8 range"))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_std_frame_restoration_type(value: u32) -> Result<u32, String> {
    match value {
        0 => Ok(vk::video::STD_VIDEO_AV1_FRAME_RESTORATION_TYPE_NONE.0 as u32),
        1 => Ok(vk::video::STD_VIDEO_AV1_FRAME_RESTORATION_TYPE_WIENER.0 as u32),
        2 => Ok(vk::video::STD_VIDEO_AV1_FRAME_RESTORATION_TYPE_SGRPROJ.0 as u32),
        3 => Ok(vk::video::STD_VIDEO_AV1_FRAME_RESTORATION_TYPE_SWITCHABLE.0 as u32),
        other => Err(format!("unsupported AV1 frame_restoration_type {other}")),
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_loop_restoration_size(
    lr_unit_shift: bool,
    lr_unit_extra_shift: bool,
    lr_uv_shift: bool,
) -> Result<u16, String> {
    let mut size = 256u32;
    if lr_unit_shift {
        size >>= 1;
    }
    if lr_unit_extra_shift {
        size >>= 1;
    }
    if lr_uv_shift {
        size >>= 1;
    }
    u16::try_from(size).map_err(|_| format!("AV1 loop restoration size {size} exceeds u16 range"))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_ceil_log2(value: u32) -> u32 {
    if value <= 1 {
        0
    } else {
        u32::BITS - (value - 1).leading_zeros()
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_read_le_uint(bytes: &[u8]) -> Result<u32, ()> {
    if bytes.len() > 4 {
        return Err(());
    }
    let mut value = 0u32;
    for (index, byte) in bytes.iter().copied().enumerate() {
        value |= u32::from(byte) << (index * 8);
    }
    Ok(value)
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_sequence_header(
    payload: &[u8],
) -> Result<NativeVulkanAv1SequenceHeaderSnapshot, String> {
    let mut bits = NativeVulkanAv1BitReader::new(payload);
    let seq_profile = native_vulkan_av1_u8(bits.read_bits(3, "seq_profile")?, "seq_profile")?;
    if seq_profile > 2 {
        return Err(format!("AV1 seq_profile {seq_profile} is reserved"));
    }
    let still_picture = bits.read_bool("still_picture")?;
    let reduced_still_picture_header = bits.read_bool("reduced_still_picture_header")?;

    let timing_info_present_flag;
    let mut timing_info = None;
    let mut decoder_model_info_present_flag = false;
    let mut buffer_delay_length_minus_1 = 0u8;
    let mut frame_presentation_time_length_minus_1 = 0u8;
    let initial_display_delay_present_flag;
    let operating_points_cnt_minus_1;
    let mut operating_points = Vec::new();

    if reduced_still_picture_header {
        timing_info_present_flag = false;
        initial_display_delay_present_flag = false;
        operating_points_cnt_minus_1 = 0;
        let seq_level_idx =
            native_vulkan_av1_u8(bits.read_bits(5, "seq_level_idx")?, "seq_level_idx")?;
        operating_points.push(NativeVulkanAv1OperatingPointSnapshot {
            index: 0,
            idc: 0,
            seq_level_idx,
            seq_level_label: native_vulkan_av1_sequence_level_idx_label(seq_level_idx),
            seq_tier: false,
            decoder_model_present_for_this_op: false,
            initial_display_delay_present_for_this_op: false,
            initial_display_delay_minus_1: None,
        });
    } else {
        timing_info_present_flag = bits.read_bool("timing_info_present_flag")?;
        if timing_info_present_flag {
            let num_units_in_display_tick = bits.read_bits(32, "num_units_in_display_tick")?;
            let time_scale = bits.read_bits(32, "time_scale")?;
            let equal_picture_interval = bits.read_bool("equal_picture_interval")?;
            let num_ticks_per_picture_minus_1 = if equal_picture_interval {
                Some(bits.read_uvlc("num_ticks_per_picture_minus_1")?)
            } else {
                None
            };
            timing_info = Some(NativeVulkanAv1TimingInfoSnapshot {
                num_units_in_display_tick,
                time_scale,
                equal_picture_interval,
                num_ticks_per_picture_minus_1,
            });
            decoder_model_info_present_flag = bits.read_bool("decoder_model_info_present_flag")?;
            if decoder_model_info_present_flag {
                buffer_delay_length_minus_1 = native_vulkan_av1_u8(
                    bits.read_bits(5, "buffer_delay_length_minus_1")?,
                    "buffer_delay_length_minus_1",
                )?;
                bits.skip_bits(32, "num_units_in_decoding_tick")?;
                bits.skip_bits(5, "buffer_removal_time_length_minus_1")?;
                frame_presentation_time_length_minus_1 = native_vulkan_av1_u8(
                    bits.read_bits(5, "frame_presentation_time_length_minus_1")?,
                    "frame_presentation_time_length_minus_1",
                )?;
            }
        }
        initial_display_delay_present_flag =
            bits.read_bool("initial_display_delay_present_flag")?;
        operating_points_cnt_minus_1 = native_vulkan_av1_u8(
            bits.read_bits(5, "operating_points_cnt_minus_1")?,
            "operating_points_cnt_minus_1",
        )?;
        for index in 0..=operating_points_cnt_minus_1 {
            let idc = native_vulkan_av1_u16(
                bits.read_bits(12, "operating_point_idc")?,
                "operating_point_idc",
            )?;
            let seq_level_idx =
                native_vulkan_av1_u8(bits.read_bits(5, "seq_level_idx")?, "seq_level_idx")?;
            let seq_tier = seq_level_idx > 7 && bits.read_bool("seq_tier")?;
            let decoder_model_present_for_this_op = if decoder_model_info_present_flag {
                bits.read_bool("decoder_model_present_for_this_op")?
            } else {
                false
            };
            if decoder_model_present_for_this_op {
                let delay_bits = u32::from(buffer_delay_length_minus_1) + 1;
                bits.skip_bits(delay_bits, "decoder_buffer_delay")?;
                bits.skip_bits(delay_bits, "encoder_buffer_delay")?;
                bits.read_bool("low_delay_mode_flag")?;
            }
            let mut initial_display_delay_present_for_this_op = false;
            let mut initial_display_delay_minus_1 = None;
            if initial_display_delay_present_flag {
                initial_display_delay_present_for_this_op =
                    bits.read_bool("initial_display_delay_present_for_this_op")?;
                if initial_display_delay_present_for_this_op {
                    initial_display_delay_minus_1 = Some(native_vulkan_av1_u8(
                        bits.read_bits(4, "initial_display_delay_minus_1")?,
                        "initial_display_delay_minus_1",
                    )?);
                }
            }
            operating_points.push(NativeVulkanAv1OperatingPointSnapshot {
                index,
                idc,
                seq_level_idx,
                seq_level_label: native_vulkan_av1_sequence_level_idx_label(seq_level_idx),
                seq_tier,
                decoder_model_present_for_this_op,
                initial_display_delay_present_for_this_op,
                initial_display_delay_minus_1,
            });
        }
    }

    let frame_width_bits_minus_1 = native_vulkan_av1_u8(
        bits.read_bits(4, "frame_width_bits_minus_1")?,
        "frame_width_bits_minus_1",
    )?;
    let frame_height_bits_minus_1 = native_vulkan_av1_u8(
        bits.read_bits(4, "frame_height_bits_minus_1")?,
        "frame_height_bits_minus_1",
    )?;
    let frame_width_bits = u32::from(frame_width_bits_minus_1) + 1;
    let frame_height_bits = u32::from(frame_height_bits_minus_1) + 1;
    let max_frame_width_minus_1 = bits.read_bits(frame_width_bits, "max_frame_width_minus_1")?;
    let max_frame_height_minus_1 = bits.read_bits(frame_height_bits, "max_frame_height_minus_1")?;
    let max_frame_width = max_frame_width_minus_1
        .checked_add(1)
        .ok_or_else(|| "AV1 max_frame_width overflow".to_owned())?;
    let max_frame_height = max_frame_height_minus_1
        .checked_add(1)
        .ok_or_else(|| "AV1 max_frame_height overflow".to_owned())?;

    let mut delta_frame_id_length_minus_2 = None;
    let mut additional_frame_id_length_minus_1 = None;
    let frame_id_numbers_present_flag = if reduced_still_picture_header {
        false
    } else {
        let present = bits.read_bool("frame_id_numbers_present_flag")?;
        if present {
            delta_frame_id_length_minus_2 = Some(native_vulkan_av1_u8(
                bits.read_bits(4, "delta_frame_id_length_minus_2")?,
                "delta_frame_id_length_minus_2",
            )?);
            additional_frame_id_length_minus_1 = Some(native_vulkan_av1_u8(
                bits.read_bits(3, "additional_frame_id_length_minus_1")?,
                "additional_frame_id_length_minus_1",
            )?);
        }
        present
    };

    let use_128x128_superblock = bits.read_bool("use_128x128_superblock")?;
    let enable_filter_intra = bits.read_bool("enable_filter_intra")?;
    let enable_intra_edge_filter = bits.read_bool("enable_intra_edge_filter")?;

    let (
        enable_interintra_compound,
        enable_masked_compound,
        enable_warped_motion,
        enable_dual_filter,
        enable_order_hint,
        enable_jnt_comp,
        enable_ref_frame_mvs,
        seq_force_screen_content_tools,
        seq_force_integer_mv,
        order_hint_bits_minus_1,
    ) = if reduced_still_picture_header {
        (false, false, false, false, false, false, false, 2, 2, None)
    } else {
        let enable_interintra_compound = bits.read_bool("enable_interintra_compound")?;
        let enable_masked_compound = bits.read_bool("enable_masked_compound")?;
        let enable_warped_motion = bits.read_bool("enable_warped_motion")?;
        let enable_dual_filter = bits.read_bool("enable_dual_filter")?;
        let enable_order_hint = bits.read_bool("enable_order_hint")?;
        let (enable_jnt_comp, enable_ref_frame_mvs) = if enable_order_hint {
            (
                bits.read_bool("enable_jnt_comp")?,
                bits.read_bool("enable_ref_frame_mvs")?,
            )
        } else {
            (false, false)
        };
        let seq_choose_screen_content_tools = bits.read_bool("seq_choose_screen_content_tools")?;
        let seq_force_screen_content_tools = if seq_choose_screen_content_tools {
            2
        } else {
            native_vulkan_av1_u8(
                bits.read_bits(1, "seq_force_screen_content_tools")?,
                "seq_force_screen_content_tools",
            )?
        };
        let seq_force_integer_mv = if seq_force_screen_content_tools > 0 {
            let seq_choose_integer_mv = bits.read_bool("seq_choose_integer_mv")?;
            if seq_choose_integer_mv {
                2
            } else {
                native_vulkan_av1_u8(
                    bits.read_bits(1, "seq_force_integer_mv")?,
                    "seq_force_integer_mv",
                )?
            }
        } else {
            2
        };
        let order_hint_bits_minus_1 = if enable_order_hint {
            Some(native_vulkan_av1_u8(
                bits.read_bits(3, "order_hint_bits_minus_1")?,
                "order_hint_bits_minus_1",
            )?)
        } else {
            None
        };
        (
            enable_interintra_compound,
            enable_masked_compound,
            enable_warped_motion,
            enable_dual_filter,
            enable_order_hint,
            enable_jnt_comp,
            enable_ref_frame_mvs,
            seq_force_screen_content_tools,
            seq_force_integer_mv,
            order_hint_bits_minus_1,
        )
    };

    let enable_superres = bits.read_bool("enable_superres")?;
    let enable_cdef = bits.read_bool("enable_cdef")?;
    let enable_restoration = bits.read_bool("enable_restoration")?;
    let color_config = native_vulkan_parse_av1_color_config(&mut bits, seq_profile)?;
    let film_grain_params_present = bits.read_bool("film_grain_params_present")?;

    let requested_profile_compatible = seq_profile == 0
        && matches!(color_config.bit_depth, 8 | 10)
        && color_config.num_planes == 3
        && color_config.subsampling_x
        && color_config.subsampling_y;
    let vulkan_std_session_parameters_ready = requested_profile_compatible
        && !film_grain_params_present
        && max_frame_width > 0
        && max_frame_height > 0
        && !operating_points.is_empty();

    Ok(NativeVulkanAv1SequenceHeaderSnapshot {
        parser: "native-rust-av1-sequence-header",
        seq_profile,
        seq_profile_label: native_vulkan_av1_profile_label(seq_profile),
        still_picture,
        reduced_still_picture_header,
        timing_info_present_flag,
        timing_info,
        decoder_model_info_present_flag,
        buffer_delay_length_minus_1,
        frame_presentation_time_length_minus_1,
        initial_display_delay_present_flag,
        operating_points_cnt_minus_1,
        operating_points,
        frame_width_bits_minus_1,
        frame_height_bits_minus_1,
        max_frame_width_minus_1,
        max_frame_height_minus_1,
        max_frame_width,
        max_frame_height,
        frame_id_numbers_present_flag,
        delta_frame_id_length_minus_2,
        additional_frame_id_length_minus_1,
        use_128x128_superblock,
        enable_filter_intra,
        enable_intra_edge_filter,
        enable_interintra_compound,
        enable_masked_compound,
        enable_warped_motion,
        enable_dual_filter,
        enable_order_hint,
        enable_jnt_comp,
        enable_ref_frame_mvs,
        seq_force_screen_content_tools,
        seq_force_integer_mv,
        order_hint_bits_minus_1,
        enable_superres,
        enable_cdef,
        enable_restoration,
        film_grain_params_present,
        color_config,
        requested_profile_compatible,
        vulkan_std_session_parameters_ready,
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_color_config(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    seq_profile: u8,
) -> Result<NativeVulkanAv1ColorConfigSnapshot, String> {
    let high_bitdepth = bits.read_bool("high_bitdepth")?;
    let twelve_bit;
    let bit_depth;
    if seq_profile == 2 && high_bitdepth {
        twelve_bit = bits.read_bool("twelve_bit")?;
        bit_depth = if twelve_bit { 12 } else { 10 };
    } else {
        twelve_bit = false;
        bit_depth = if high_bitdepth { 10 } else { 8 };
    }

    let mono_chrome = if seq_profile == 1 {
        false
    } else {
        bits.read_bool("mono_chrome")?
    };
    let num_planes = if mono_chrome { 1 } else { 3 };
    let color_description_present_flag = bits.read_bool("color_description_present_flag")?;
    let (color_primaries, transfer_characteristics, matrix_coefficients) =
        if color_description_present_flag {
            (
                native_vulkan_av1_u8(bits.read_bits(8, "color_primaries")?, "color_primaries")?,
                native_vulkan_av1_u8(
                    bits.read_bits(8, "transfer_characteristics")?,
                    "transfer_characteristics",
                )?,
                native_vulkan_av1_u8(
                    bits.read_bits(8, "matrix_coefficients")?,
                    "matrix_coefficients",
                )?,
            )
        } else {
            (2, 2, 2)
        };

    if mono_chrome {
        let color_range = bits.read_bool("color_range")?;
        return Ok(NativeVulkanAv1ColorConfigSnapshot {
            high_bitdepth,
            twelve_bit,
            mono_chrome,
            color_description_present_flag,
            color_primaries,
            transfer_characteristics,
            matrix_coefficients,
            color_range,
            subsampling_x: true,
            subsampling_y: true,
            chroma_sample_position: 0,
            separate_uv_delta_q: false,
            bit_depth,
            num_planes,
        });
    }

    let mut chroma_sample_position = 0u8;
    let (color_range, subsampling_x, subsampling_y) =
        if color_primaries == 1 && transfer_characteristics == 13 && matrix_coefficients == 0 {
            (true, false, false)
        } else {
            let color_range = bits.read_bool("color_range")?;
            let (subsampling_x, subsampling_y) = match seq_profile {
                0 => (true, true),
                1 => (false, false),
                2 if bit_depth == 12 => {
                    let subsampling_x = bits.read_bool("subsampling_x")?;
                    let subsampling_y = subsampling_x && bits.read_bool("subsampling_y")?;
                    (subsampling_x, subsampling_y)
                }
                2 => (true, false),
                _ => return Err(format!("AV1 seq_profile {seq_profile} is reserved")),
            };
            if subsampling_x && subsampling_y {
                chroma_sample_position = native_vulkan_av1_u8(
                    bits.read_bits(2, "chroma_sample_position")?,
                    "chroma_sample_position",
                )?;
            }
            (color_range, subsampling_x, subsampling_y)
        };
    let separate_uv_delta_q = bits.read_bool("separate_uv_delta_q")?;

    Ok(NativeVulkanAv1ColorConfigSnapshot {
        high_bitdepth,
        twelve_bit,
        mono_chrome,
        color_description_present_flag,
        color_primaries,
        transfer_characteristics,
        matrix_coefficients,
        color_range,
        subsampling_x,
        subsampling_y,
        chroma_sample_position,
        separate_uv_delta_q,
        bit_depth,
        num_planes,
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
struct NativeVulkanAv1BitReader<'a> {
    bytes: &'a [u8],
    bit_offset: usize,
}

#[cfg(any(feature = "native-vulkan-video", test))]
impl<'a> NativeVulkanAv1BitReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            bit_offset: 0,
        }
    }

    fn read_bool(&mut self, label: &'static str) -> Result<bool, String> {
        Ok(self.read_bits(1, label)? != 0)
    }

    fn skip_bits(&mut self, count: u32, label: &'static str) -> Result<(), String> {
        let mut remaining = count;
        while remaining > 0 {
            let chunk = remaining.min(32);
            self.read_bits(chunk, label)?;
            remaining -= chunk;
        }
        Ok(())
    }

    fn byte_offset(&self) -> usize {
        self.bit_offset.div_ceil(8)
    }

    fn zero_align_to_byte(&mut self, label: &'static str) -> Result<(), String> {
        while !self.bit_offset.is_multiple_of(8) {
            if self.read_bool(label)? {
                return Err(format!("{label} expected zero padding bit"));
            }
        }
        Ok(())
    }

    fn read_bits(&mut self, count: u32, label: &'static str) -> Result<u32, String> {
        if count > 32 {
            return Err(format!("{label} requested too many bits: {count}"));
        }
        let end = self
            .bit_offset
            .checked_add(count as usize)
            .ok_or_else(|| format!("{label} bit offset overflow"))?;
        if end > self.bytes.len() * 8 {
            return Err(format!("{label} exceeds AV1 OBU payload length"));
        }
        let mut value = 0u32;
        for _ in 0..count {
            let byte = self.bytes[self.bit_offset / 8];
            let shift = 7 - (self.bit_offset % 8);
            value = (value << 1) | u32::from((byte >> shift) & 1);
            self.bit_offset += 1;
        }
        Ok(value)
    }

    fn read_uvlc(&mut self, label: &'static str) -> Result<u32, String> {
        let mut leading_zero_bits = 0u32;
        while leading_zero_bits < 32 && !self.read_bool(label)? {
            leading_zero_bits += 1;
        }
        if leading_zero_bits == 32 {
            return Ok(u32::MAX);
        }
        if leading_zero_bits == 0 {
            return Ok(0);
        }
        let suffix = self.read_bits(leading_zero_bits, label)?;
        Ok((1u32 << leading_zero_bits) - 1 + suffix)
    }

    fn read_quniform(&mut self, n: u32, label: &'static str) -> Result<u32, String> {
        if n <= 1 {
            return Ok(0);
        }
        let l = 32 - n.leading_zeros();
        let m = (1u32 << l) - n;
        let value = self.read_bits(l - 1, label)?;
        if value < m {
            Ok(value)
        } else {
            let extra = self.read_bits(1, label)?;
            Ok(m + ((value - m) << 1) + extra)
        }
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_u8(value: u32, label: &'static str) -> Result<u8, String> {
    u8::try_from(value).map_err(|_| format!("{label}={value} exceeds u8 range"))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_i8(value: u32, label: &'static str) -> Result<i8, String> {
    i8::try_from(value).map_err(|_| format!("{label}={value} exceeds i8 range"))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_u16(value: u32, label: &'static str) -> Result<u16, String> {
    u16::try_from(value).map_err(|_| format!("{label}={value} exceeds u16 range"))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_read_leb128(bytes: &[u8]) -> Result<(u64, usize), String> {
    let mut value = 0u64;
    for (index, byte) in bytes.iter().copied().take(8).enumerate() {
        value |= u64::from(byte & 0x7f) << (index * 7);
        if byte & 0x80 == 0 {
            return Ok((value, index + 1));
        }
    }
    Err("AV1 LEB128 size field is missing terminator within 8 bytes".to_owned())
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_obu_type_label(obu_type: u8) -> &'static str {
    match obu_type {
        1 => "sequence-header",
        2 => "temporal-delimiter",
        3 => "frame-header",
        4 => "tile-group",
        5 => "metadata",
        6 => "frame",
        7 => "redundant-frame-header",
        8 => "tile-list",
        15 => "padding",
        _ => "reserved",
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_frame_type_label(frame_type: u8) -> &'static str {
    match frame_type {
        0 => "key",
        1 => "inter",
        2 => "intra-only",
        3 => "switch",
        _ => "unknown",
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_interpolation_filter_label(
    filter: vk::video::StdVideoAV1InterpolationFilter,
) -> &'static str {
    match filter {
        vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_EIGHTTAP => "eighttap",
        vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_EIGHTTAP_SMOOTH => "eighttap-smooth",
        vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_EIGHTTAP_SHARP => "eighttap-sharp",
        vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_BILINEAR => "bilinear",
        vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_SWITCHABLE => "switchable",
        _ => "invalid",
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_profile_label(profile: u8) -> &'static str {
    match profile {
        0 => "main",
        1 => "high",
        2 => "professional",
        _ => "reserved",
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_sequence_level_idx_label(level_idx: u8) -> Option<&'static str> {
    match level_idx {
        0 => Some("2.0"),
        1 => Some("2.1"),
        2 => Some("2.2"),
        3 => Some("2.3"),
        4 => Some("3.0"),
        5 => Some("3.1"),
        6 => Some("3.2"),
        7 => Some("3.3"),
        8 => Some("4.0"),
        9 => Some("4.1"),
        10 => Some("4.2"),
        11 => Some("4.3"),
        12 => Some("5.0"),
        13 => Some("5.1"),
        14 => Some("5.2"),
        15 => Some("5.3"),
        16 => Some("6.0"),
        17 => Some("6.1"),
        18 => Some("6.2"),
        19 => Some("6.3"),
        20 => Some("7.0"),
        21 => Some("7.1"),
        22 => Some("7.2"),
        23 => Some("7.3"),
        _ => None,
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h265_nal_stats(bytes: &[u8]) -> NativeVulkanH265NalStats {
    let mut stats = NativeVulkanH265NalStats {
        bytes: bytes.len() as u64,
        ..Default::default()
    };
    let mut offset = 0usize;
    while let Some((start_code_offset, payload_offset)) =
        native_vulkan_next_annex_b_start_code(bytes, offset)
    {
        stats.has_annex_b_start_codes = true;
        let next_search_offset = payload_offset;
        let next_start = native_vulkan_next_annex_b_start_code(bytes, next_search_offset)
            .map(|(next_start, _)| next_start)
            .unwrap_or(bytes.len());
        if payload_offset < next_start {
            if let Some(nal_type) = bytes.get(payload_offset).map(|header| (header >> 1) & 0x3f) {
                match nal_type {
                    32 => stats.vps_count = stats.vps_count.saturating_add(1),
                    33 => stats.sps_count = stats.sps_count.saturating_add(1),
                    34 => stats.pps_count = stats.pps_count.saturating_add(1),
                    19 | 20 => {
                        stats.idr_count = stats.idr_count.saturating_add(1);
                        stats.slice_count = stats.slice_count.saturating_add(1);
                    }
                    0..=31 => stats.slice_count = stats.slice_count.saturating_add(1),
                    _ => {}
                }
                if nal_type <= 31 && stats.first_slice.is_none() {
                    let slice_segment_offset = native_vulkan_h265_annex_b_slice_segment_offset(
                        start_code_offset,
                        payload_offset,
                    );
                    if let Ok(slice_segment_offset) = u32::try_from(slice_segment_offset) {
                        stats.first_slice = Some(NativeVulkanH265SlicePayloadSummary {
                            nal_type,
                            slice_segment_offset,
                            payload_start: payload_offset,
                            payload_end: next_start,
                        });
                    }
                }
            }
        }
        offset = next_start;
    }
    stats
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h264_nal_stats(bytes: &[u8]) -> NativeVulkanH264NalStats {
    let mut stats = NativeVulkanH264NalStats {
        bytes: bytes.len() as u64,
        ..Default::default()
    };
    let mut offset = 0usize;
    while let Some((start_code_offset, payload_offset)) =
        native_vulkan_next_annex_b_start_code(bytes, offset)
    {
        stats.has_annex_b_start_codes = true;
        let next_start = native_vulkan_next_annex_b_start_code(bytes, payload_offset)
            .map(|(next_start, _)| next_start)
            .unwrap_or(bytes.len());
        if payload_offset < next_start
            && let Some(header) = bytes.get(payload_offset).copied()
        {
            let nal_type = header & 0x1f;
            match nal_type {
                1..=5 => {
                    stats.slice_count = stats.slice_count.saturating_add(1);
                    if nal_type == 5 {
                        stats.idr_count = stats.idr_count.saturating_add(1);
                    }
                    let slice_offset =
                        native_vulkan_h264_annex_b_slice_offset(start_code_offset, payload_offset);
                    if let Ok(slice_offset_u32) = u32::try_from(slice_offset) {
                        stats.slice_offsets.push(slice_offset_u32);
                        if stats.first_slice.is_none() {
                            stats.first_slice = Some(NativeVulkanH264SlicePayloadSummary {
                                nal_type,
                                nal_ref_idc: (header >> 5) & 0x03,
                                payload_start: payload_offset,
                                payload_end: next_start,
                            });
                        }
                    }
                }
                7 => stats.sps_count = stats.sps_count.saturating_add(1),
                8 => stats.pps_count = stats.pps_count.saturating_add(1),
                _ => {}
            }
        }
        offset = next_start;
    }
    stats
}

fn native_vulkan_next_annex_b_start_code(bytes: &[u8], from: usize) -> Option<(usize, usize)> {
    let mut index = from.min(bytes.len());
    while index + 3 <= bytes.len() {
        // Match FFmpeg's H.264/H.265 parser shape: first jump to a zero byte,
        // then check whether it starts a three- or four-byte Annex-B prefix.
        // See references/ffmpeg/libavcodec/h2645_parse.c:37-180.
        let zero_offset = native_vulkan_memchr_zero(&bytes[index..])?;
        index = index.saturating_add(zero_offset);
        if index + 3 > bytes.len() {
            return None;
        }
        if bytes[index] == 0 && bytes[index + 1] == 0 {
            if bytes[index + 2] == 1 {
                return Some((index, index + 3));
            }
            if index + 4 <= bytes.len() && bytes[index + 2] == 0 && bytes[index + 3] == 1 {
                return Some((index, index + 4));
            }
        }
        index += 1;
    }
    None
}

#[cfg(any(
    feature = "native-vulkan-renderer",
    feature = "native-vulkan-video",
    test
))]
fn native_vulkan_memchr_zero(bytes: &[u8]) -> Option<usize> {
    #[cfg(target_family = "unix")]
    {
        let ptr = unsafe {
            native_vulkan_c_memchr(bytes.as_ptr().cast::<std::ffi::c_void>(), 0, bytes.len())
        };
        if ptr.is_null() {
            None
        } else {
            let offset = unsafe { ptr.cast::<u8>().offset_from(bytes.as_ptr()) };
            usize::try_from(offset).ok()
        }
    }
    #[cfg(not(target_family = "unix"))]
    {
        bytes.iter().position(|byte| *byte == 0)
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h264_nal_type_label(nal_type: u8) -> &'static str {
    match nal_type {
        1 => "non-idr",
        2 => "data-partition-a",
        3 => "data-partition-b",
        4 => "data-partition-c",
        5 => "idr",
        6 => "sei",
        7 => "sps",
        8 => "pps",
        9 => "aud",
        10 => "end-of-sequence",
        11 => "end-of-stream",
        12 => "filler",
        13 => "sps-extension",
        14 => "prefix",
        15 => "subset-sps",
        19 => "auxiliary-slice",
        20 => "extension-slice",
        21 => "depth-extension-slice",
        22..=23 => "reserved",
        24..=31 => "unspecified",
        _ => "unknown",
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h265_nal_type_label(nal_type: u8) -> &'static str {
    match nal_type {
        0 => "trail-n",
        1 => "trail-r",
        16 => "bla-w-lp",
        17 => "bla-w-radl",
        18 => "bla-n-lp",
        19 => "idr-w-radl",
        20 => "idr-n-lp",
        21 => "cra-nut",
        32 => "vps",
        33 => "sps",
        34 => "pps",
        35 => "aud",
        36 => "eos",
        37 => "eob",
        38 => "fd",
        39 => "prefix-sei",
        40 => "suffix-sei",
        41..=47 => "reserved",
        48..=63 => "unspecified",
        _ => "slice-or-extension",
    }
}
fn native_vulkan_align_up(value: u64, alignment: u64) -> u64 {
    if alignment <= 1 {
        return value;
    }
    let remainder = value % alignment;
    if remainder == 0 {
        value
    } else {
        value.saturating_add(alignment - remainder)
    }
}

fn native_vulkan_h264_sps_dpb_slot_count(sps: &NativeVulkanH264SpsSnapshot) -> u32 {
    sps.max_num_ref_frames.saturating_add(1).max(1)
}

fn native_vulkan_h265_sps_dpb_slot_count(sps: &NativeVulkanH265SpsSnapshot) -> u32 {
    let layer_count = usize::from(sps.max_sub_layers_minus1).saturating_add(1);
    sps.dec_pic_buf_mgr
        .max_dec_pic_buffering_minus1
        .iter()
        .take(layer_count.min(sps.dec_pic_buf_mgr.max_dec_pic_buffering_minus1.len()))
        .copied()
        .max()
        .map(|value| u32::from(value).saturating_add(1))
        .unwrap_or(1)
        .max(1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeVulkanWallpaperType {
    StaticImage,
    Video,
    Web,
    Scene,
    Shader,
    Playlist,
}

pub const WALLPAPER_TYPE_CONTRACT: &[NativeVulkanWallpaperType] = &[
    NativeVulkanWallpaperType::StaticImage,
    NativeVulkanWallpaperType::Video,
    NativeVulkanWallpaperType::Web,
    NativeVulkanWallpaperType::Scene,
    NativeVulkanWallpaperType::Shader,
    NativeVulkanWallpaperType::Playlist,
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanWallpaperTypeSupport {
    pub wallpaper_type: NativeVulkanWallpaperType,
    pub current_vulkan_item: bool,
    pub current_renderer_status: &'static str,
    pub target_vulkan_path: &'static str,
}

pub fn wallpaper_type_support_matrix() -> Vec<NativeVulkanWallpaperTypeSupport> {
    vec![
        NativeVulkanWallpaperTypeSupport {
            wallpaper_type: NativeVulkanWallpaperType::StaticImage,
            current_vulkan_item: true,
            current_renderer_status: "--run-static lowers static images into a single scene sampled-image layer, then uses Vulkanalia sampled-image dynamic rendering; ash static session and staging-copy runtime are removed",
            target_vulkan_path: "decode image once -> retained sampled Vulkan image -> fit-aware dynamic-rendering pass shared with scene/image layers",
        },
        NativeVulkanWallpaperTypeSupport {
            wallpaper_type: NativeVulkanWallpaperType::Video,
            current_vulkan_item: true,
            current_renderer_status: "--run-video targets FFmpeg Vulkan HW decode as the mainline; the legacy Vulkanalia Vulkan Video route is compatibility-only",
            target_vulkan_path: "FFmpeg demux/parser/avcodec Vulkan hwaccel -> AV_PIX_FMT_VULKAN/AVVkFrame -> VK_EXT_descriptor_heap Y/UV sampling -> Wayland present",
        },
        NativeVulkanWallpaperTypeSupport {
            wallpaper_type: NativeVulkanWallpaperType::Web,
            current_vulkan_item: false,
            current_renderer_status: "helper contract only; current render plan may fall back to static image",
            target_vulkan_path: "Web helper -> DMABuf/EGLImage/shared-frame handoff -> Vulkan composite",
        },
        NativeVulkanWallpaperTypeSupport {
            wallpaper_type: NativeVulkanWallpaperType::Scene,
            current_vulkan_item: true,
            current_renderer_status: "deterministic scene snapshot layers carried by Vulkan render item; static images lower into single-image scene layers; native draw-pass plan, fast-clear color path, color/rectangle quads and sampled-image geometry exist, text/path rasterization remains pending",
            target_vulkan_path: "deterministic scene snapshot -> Vulkan shape/image/text passes",
        },
        NativeVulkanWallpaperTypeSupport {
            wallpaper_type: NativeVulkanWallpaperType::Shader,
            current_vulkan_item: false,
            current_renderer_status: "shader contract only; current render plan may fall back to static image",
            target_vulkan_path: "fullscreen triangle -> GLSL/WGSL-derived SPIR-V -> time/property uniforms",
        },
        NativeVulkanWallpaperTypeSupport {
            wallpaper_type: NativeVulkanWallpaperType::Playlist,
            current_vulkan_item: false,
            current_renderer_status: "playlist selection remains in core render sync; selected child maps to Vulkan item",
            target_vulkan_path: "core playlist decision -> selected child item -> same Vulkan runtime path",
        },
    ]
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanBackendContract {
    pub backend_name: &'static str,
    pub default_renderer_candidate: bool,
    pub wallpaper_types: &'static [NativeVulkanWallpaperType],
    pub wallpaper_type_support: Vec<NativeVulkanWallpaperTypeSupport>,
    pub layer_shell_host: &'static str,
    pub render_plan_boundary: &'static str,
    pub lifecycle_boundary: &'static str,
    pub resource_telemetry_boundary: &'static str,
    pub required_instance_extensions: Vec<&'static str>,
    pub required_device_extensions: Vec<&'static str>,
    pub video_pipeline: pipeline::NativeVulkanVideoPipelineContract,
    pub video_flow: video_flow::NativeVulkanVideoFlowContract,
    #[cfg(feature = "native-vulkan-video")]
    pub ffmpeg_hw_decode: NativeVulkanFfmpegHwDecodeBackendContract,
    pub video_interop: NativeVulkanVideoInteropContract,
    pub web_interop: NativeVulkanWebInteropContract,
    pub vulkan_backend: NativeVulkanBackendPlan,
}

pub fn backend_contract() -> NativeVulkanBackendContract {
    NativeVulkanBackendContract {
        backend_name: "native-vulkan",
        default_renderer_candidate: false,
        wallpaper_types: WALLPAPER_TYPE_CONTRACT,
        wallpaper_type_support: wallpaper_type_support_matrix(),
        layer_shell_host: "reuse NativeWaylandHost raw wl_display/wl_surface first, then move ownership here",
        render_plan_boundary: "consume existing renderer plans; do not introduce Vulkan-only manifest semantics",
        lifecycle_boundary: "pause-dynamic, hidden/fullscreen/session release, resize, and output selection stay backend-neutral",
        resource_telemetry_boundary: "report CPU/RSS/PSS/private_dirty/GPU resource counts through stable renderer telemetry",
        required_instance_extensions: required_instance_extensions(),
        required_device_extensions: required_device_extensions(),
        video_pipeline: pipeline::native_vulkan_video_pipeline_contract(),
        video_flow: video_flow::native_vulkan_video_flow_contract(),
        #[cfg(feature = "native-vulkan-video")]
        ffmpeg_hw_decode: ffmpeg_hw::native_vulkan_ffmpeg_hw_decode_backend_contract(),
        video_interop: video_interop_contract(),
        web_interop: web_interop_contract(),
        vulkan_backend: native_vulkan_backend_plan(),
    }
}

pub fn required_instance_extensions() -> Vec<&'static str> {
    vec!["VK_KHR_surface", "VK_KHR_wayland_surface"]
}

pub fn required_device_extensions() -> Vec<&'static str> {
    vec![
        "VK_KHR_swapchain",
        "VK_KHR_external_memory_fd",
        "VK_KHR_external_semaphore_fd",
        "VK_KHR_timeline_semaphore",
        "VK_EXT_external_memory_dma_buf",
        "VK_EXT_image_drm_format_modifier",
        "VK_KHR_video_queue",
        "VK_KHR_video_decode_queue",
        "VK_KHR_video_decode_h264",
        "VK_KHR_video_decode_h265",
        "VK_KHR_video_decode_av1",
        "VK_EXT_descriptor_heap",
    ]
}

#[cfg(test)]
mod tests {
    include!("codec_parser_tests.rs");
    include!("reference_planner_tests.rs");
    include!("backend_contract_tests.rs");
}
