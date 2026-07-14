
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

include!("av1_sequence_contract/sequence_header_and_backend.rs");
