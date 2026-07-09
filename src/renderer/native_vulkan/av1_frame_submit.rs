
#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_obu_ranges(bytes: &[u8]) -> Result<Vec<NativeVulkanAv1ObuRange>, String> {
    let mut ranges = Vec::new();
    let mut offset = 0usize;
    while offset < bytes.len() {
        let header_offset = offset;
        let header = bytes[offset];
        if header & 0x80 != 0 {
            return Err(format!(
                "AV1 OBU forbidden bit set at byte offset {header_offset}"
            ));
        }
        let obu_type = (header >> 3) & 0x0f;
        let has_extension = header & 0x04 != 0;
        let has_size_field = header & 0x02 != 0;
        if header & 0x01 != 0 {
            return Err(format!(
                "AV1 OBU reserved bit set at byte offset {header_offset}"
            ));
        }
        offset += 1;
        if has_extension {
            if offset >= bytes.len() {
                return Err("AV1 OBU extension flag set without extension byte".to_owned());
            }
            offset += 1;
        }
        if !has_size_field {
            return Err(format!(
                "AV1 OBU at byte offset {header_offset} has no size field; annexb AV1 extraction is not supported yet"
            ));
        }
        let (payload_size, leb_size) = native_vulkan_av1_read_leb128(&bytes[offset..])?;
        offset = offset
            .checked_add(leb_size)
            .ok_or_else(|| "AV1 OBU offset overflow after LEB128".to_owned())?;
        let payload_size_usize = usize::try_from(payload_size)
            .map_err(|_| format!("AV1 OBU payload size {payload_size} exceeds usize"))?;
        let payload_end = offset
            .checked_add(payload_size_usize)
            .ok_or_else(|| "AV1 OBU payload end overflow".to_owned())?;
        if payload_end > bytes.len() {
            return Err(format!(
                "AV1 OBU payload at byte offset {offset} extends past sample end"
            ));
        }
        ranges.push(NativeVulkanAv1ObuRange {
            offset: header_offset,
            end: payload_end,
            obu_type,
        });
        offset = payload_end;
    }
    Ok(ranges)
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_split_ffmpeg_packet_frame_ranges(
    bytes: &[u8],
) -> Result<Vec<std::ops::Range<usize>>, String> {
    let ranges = native_vulkan_av1_obu_ranges(bytes)?;
    let mut units = Vec::<std::ops::Range<usize>>::new();
    let mut pending_prefix = None::<std::ops::Range<usize>>;
    let mut current_frame = None::<std::ops::Range<usize>>;

    for range in ranges {
        match range.obu_type {
            1 | 2 => {
                if let Some(unit) = current_frame.take() {
                    units.push(unit);
                }
                native_vulkan_av1_extend_range(&mut pending_prefix, range.offset, range.end);
            }
            3 => {
                if let Some(unit) = current_frame.take() {
                    units.push(unit);
                }
                current_frame = Some(native_vulkan_av1_take_prefixed_range(
                    &mut pending_prefix,
                    range.offset,
                    range.end,
                ));
            }
            4 => {
                if let Some(unit) = current_frame.as_mut() {
                    unit.end = range.end;
                } else {
                    current_frame = Some(native_vulkan_av1_take_prefixed_range(
                        &mut pending_prefix,
                        range.offset,
                        range.end,
                    ));
                }
            }
            6 => {
                if let Some(unit) = current_frame.take() {
                    units.push(unit);
                }
                units.push(native_vulkan_av1_take_prefixed_range(
                    &mut pending_prefix,
                    range.offset,
                    range.end,
                ));
            }
            _ => {
                if let Some(unit) = current_frame.as_mut() {
                    unit.end = range.end;
                } else {
                    native_vulkan_av1_extend_range(&mut pending_prefix, range.offset, range.end);
                }
            }
        }
    }

    if let Some(unit) = current_frame.take() {
        units.push(unit);
    }
    if units.is_empty() {
        if let Some(prefix) = pending_prefix.take() {
            units.push(prefix);
        }
    }
    Ok(units)
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_extend_range(
    range: &mut Option<std::ops::Range<usize>>,
    offset: usize,
    end: usize,
) {
    match range {
        Some(existing) => existing.end = end,
        None => *range = Some(offset..end),
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_take_prefixed_range(
    prefix: &mut Option<std::ops::Range<usize>>,
    offset: usize,
    end: usize,
) -> std::ops::Range<usize> {
    let start = prefix.take().map(|prefix| prefix.start).unwrap_or(offset);
    start..end
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_first_frame_submit_snapshot(
    bytes: &[u8],
    obus: &[NativeVulkanAv1ObuSnapshot],
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
) -> Option<NativeVulkanAv1FrameSubmitSnapshot> {
    let frame_obu = obus.iter().find(|obu| obu.obu_type == 6);
    if let Some(frame_obu) = frame_obu {
        return Some(native_vulkan_av1_frame_submit_from_frame_obu(
            bytes,
            frame_obu,
            sequence_header,
        ));
    }

    let frame_header_obu = obus.iter().find(|obu| obu.obu_type == 3)?;
    let tile_group_obu = obus.iter().find(|obu| obu.obu_type == 4);
    Some(native_vulkan_av1_frame_submit_from_split_obus(
        bytes,
        frame_header_obu,
        tile_group_obu,
        sequence_header,
    ))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_frame_submit_from_frame_obu(
    bytes: &[u8],
    frame_obu: &NativeVulkanAv1ObuSnapshot,
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
) -> NativeVulkanAv1FrameSubmitSnapshot {
    let payload_offset = frame_obu.payload_offset as usize;
    let payload_end = payload_offset.saturating_add(frame_obu.payload_size as usize);
    let payload = bytes.get(payload_offset..payload_end).unwrap_or_default();
    match native_vulkan_parse_av1_frame_header_for_submit(payload, sequence_header) {
        Ok(header) => {
            let tile_payload_offset = header.frame_header_bytes;
            let tile_payload = payload.get(tile_payload_offset..).unwrap_or_default();
            match native_vulkan_av1_tile_group_offsets_from_payload(
                frame_obu.payload_offset,
                tile_payload_offset,
                tile_payload,
                &header,
            ) {
                Ok((tile_offsets, tile_sizes)) => {
                    native_vulkan_av1_frame_submit_snapshot_from_header(
                        frame_obu,
                        frame_obu.payload_offset,
                        frame_obu.payload_size,
                        header,
                        tile_offsets,
                        tile_sizes,
                        !tile_payload.is_empty(),
                    )
                }
                Err(reason) => {
                    let mut snapshot = native_vulkan_av1_frame_submit_snapshot_from_header(
                        frame_obu,
                        frame_obu.payload_offset,
                        frame_obu.payload_size,
                        header,
                        Vec::new(),
                        Vec::new(),
                        false,
                    );
                    if snapshot.unsupported_reason.is_none() {
                        snapshot.unsupported_reason = Some(format!(
                            "AV1 frame OBU tile table is not submit-ready: {reason}"
                        ));
                    }
                    snapshot.vulkan_submit_candidate = false;
                    snapshot
                }
            }
        }
        Err(reason) => native_vulkan_av1_unsupported_frame_submit_snapshot(
            frame_obu,
            frame_obu.payload_offset,
            frame_obu.payload_size,
            format!("AV1 frame OBU header is not submit-ready: {reason}"),
        ),
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_frame_submit_from_split_obus(
    bytes: &[u8],
    frame_header_obu: &NativeVulkanAv1ObuSnapshot,
    tile_group_obu: Option<&NativeVulkanAv1ObuSnapshot>,
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
) -> NativeVulkanAv1FrameSubmitSnapshot {
    let payload_offset = frame_header_obu.payload_offset as usize;
    let payload_end = payload_offset.saturating_add(frame_header_obu.payload_size as usize);
    let payload = bytes.get(payload_offset..payload_end).unwrap_or_default();
    match native_vulkan_parse_av1_frame_header_for_submit(payload, sequence_header) {
        Ok(header) => {
            let Some(tile_group_obu) = tile_group_obu else {
                let mut snapshot = native_vulkan_av1_frame_submit_snapshot_from_header(
                    frame_header_obu,
                    frame_header_obu.payload_offset,
                    frame_header_obu.payload_size,
                    header,
                    Vec::new(),
                    Vec::new(),
                    false,
                );
                if !snapshot.show_existing_frame {
                    snapshot.unsupported_reason =
                        Some("AV1 frame-header OBU has no following tile-group OBU".to_owned());
                }
                snapshot.vulkan_submit_candidate = false;
                return snapshot;
            };
            let tile_payload_offset = tile_group_obu.payload_offset as usize;
            let tile_payload_end =
                tile_payload_offset.saturating_add(tile_group_obu.payload_size as usize);
            let tile_payload = bytes
                .get(tile_payload_offset..tile_payload_end)
                .unwrap_or_default();
            match native_vulkan_av1_tile_group_offsets_from_payload(
                tile_group_obu.payload_offset,
                0,
                tile_payload,
                &header,
            ) {
                Ok((tile_offsets, tile_sizes)) => {
                    native_vulkan_av1_frame_submit_snapshot_from_header(
                        frame_header_obu,
                        frame_header_obu.payload_offset,
                        frame_header_obu.payload_size,
                        header,
                        tile_offsets,
                        tile_sizes,
                        !tile_payload.is_empty(),
                    )
                }
                Err(reason) => {
                    let mut snapshot = native_vulkan_av1_frame_submit_snapshot_from_header(
                        frame_header_obu,
                        frame_header_obu.payload_offset,
                        frame_header_obu.payload_size,
                        header,
                        Vec::new(),
                        Vec::new(),
                        false,
                    );
                    if snapshot.unsupported_reason.is_none() {
                        snapshot.unsupported_reason = Some(format!(
                            "AV1 tile-group OBU table is not submit-ready: {reason}"
                        ));
                    }
                    snapshot.vulkan_submit_candidate = false;
                    snapshot
                }
            }
        }
        Err(reason) => native_vulkan_av1_unsupported_frame_submit_snapshot(
            frame_header_obu,
            frame_header_obu.payload_offset,
            frame_header_obu.payload_size,
            format!("AV1 frame-header OBU is not submit-ready: {reason}"),
        ),
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_frame_submit_snapshot_from_header(
    frame_header_obu: &NativeVulkanAv1ObuSnapshot,
    frame_header_payload_offset: u64,
    frame_obu_payload_bytes: u64,
    header: NativeVulkanAv1ParsedFrameHeader,
    tile_offsets: Vec<u32>,
    tile_sizes: Vec<u32>,
    found_tile_payload: bool,
) -> NativeVulkanAv1FrameSubmitSnapshot {
    let found_frame_header = true;
    let tile_payload_total_bytes = tile_sizes.iter().map(|size| u64::from(*size)).sum::<u64>();
    let unsupported_reason = if header.unsupported_reason.is_some() {
        header.unsupported_reason.clone()
    } else if !found_tile_payload {
        Some("AV1 first frame has no tile payload bytes".to_owned())
    } else if header.tile_count != tile_offsets.len() as u32
        || tile_offsets.len() != tile_sizes.len()
    {
        Some(format!(
            "AV1 tile table mismatch: header tile_count={}, offsets={}, sizes={}",
            header.tile_count,
            tile_offsets.len(),
            tile_sizes.len()
        ))
    } else {
        None
    };
    let vulkan_submit_candidate = unsupported_reason.is_none()
        && found_frame_header
        && found_tile_payload
        && header.tile_count > 0
        && !header.show_existing_frame;

    NativeVulkanAv1FrameSubmitSnapshot {
        parser: "native-rust-av1-first-frame-submit",
        frame_header_obu_offset: frame_header_obu.offset,
        frame_header_payload_offset,
        frame_header_payload_size: frame_header_obu.payload_size,
        frame_header_offset_for_vulkan: u32::try_from(frame_header_obu.offset).unwrap_or(0),
        tile_count: header.tile_count,
        tile_columns: header.tile_columns,
        tile_rows: header.tile_rows,
        tile_size_bytes: header.tile_size_bytes,
        tile_offsets,
        tile_sizes,
        tile_payload_total_bytes,
        frame_obu_payload_bytes,
        frame_type: header.frame_type,
        frame_type_label: native_vulkan_av1_frame_type_label(header.frame_type),
        show_existing_frame: header.show_existing_frame,
        frame_to_show_map_idx: header.frame_to_show_map_idx,
        display_frame_id: header.display_frame_id,
        current_frame_id: header.current_frame_id,
        expected_frame_ids: header.expected_frame_ids,
        show_frame: header.show_frame,
        showable_frame: header.showable_frame,
        error_resilient_mode: header.error_resilient_mode,
        disable_cdf_update: header.disable_cdf_update,
        allow_screen_content_tools: header.allow_screen_content_tools,
        force_integer_mv: header.force_integer_mv,
        allow_high_precision_mv: header.allow_high_precision_mv,
        interpolation_filter: header.interpolation_filter.0 as u32,
        interpolation_filter_label: native_vulkan_av1_interpolation_filter_label(
            header.interpolation_filter,
        ),
        is_filter_switchable: header.is_filter_switchable,
        is_motion_mode_switchable: header.is_motion_mode_switchable,
        use_ref_frame_mvs: header.use_ref_frame_mvs,
        reference_select: header.reference_select,
        skip_mode_present: header.skip_mode_present,
        allow_warped_motion: header.allow_warped_motion,
        order_hint: header.order_hint,
        primary_ref_frame: header.primary_ref_frame,
        refresh_frame_flags: header.refresh_frame_flags,
        reference_order_hints: header.reference_order_hints,
        frame_refs_short_signaling: header.frame_refs_short_signaling,
        last_frame_idx: header.last_frame_idx,
        gold_frame_idx: header.gold_frame_idx,
        ref_frame_indices: header.ref_frame_indices,
        render_and_frame_size_different: header.render_and_frame_size_different,
        frame_width: header.frame_width,
        frame_height: header.frame_height,
        render_width: header.render_width,
        render_height: header.render_height,
        found_frame_header,
        found_tile_payload,
        vulkan_submit_candidate,
        unsupported_reason,
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_unsupported_frame_submit_snapshot(
    frame_header_obu: &NativeVulkanAv1ObuSnapshot,
    frame_header_payload_offset: u64,
    frame_obu_payload_bytes: u64,
    reason: String,
) -> NativeVulkanAv1FrameSubmitSnapshot {
    NativeVulkanAv1FrameSubmitSnapshot {
        parser: "native-rust-av1-first-frame-submit",
        frame_header_obu_offset: frame_header_obu.offset,
        frame_header_payload_offset,
        frame_header_payload_size: frame_header_obu.payload_size,
        frame_header_offset_for_vulkan: u32::try_from(frame_header_obu.offset).unwrap_or(0),
        tile_count: 0,
        tile_columns: 0,
        tile_rows: 0,
        tile_size_bytes: 0,
        tile_offsets: Vec::new(),
        tile_sizes: Vec::new(),
        tile_payload_total_bytes: 0,
        frame_obu_payload_bytes,
        frame_type: u8::MAX,
        frame_type_label: "unknown",
        show_existing_frame: false,
        frame_to_show_map_idx: None,
        display_frame_id: None,
        current_frame_id: None,
        expected_frame_ids: Vec::new(),
        show_frame: false,
        showable_frame: false,
        error_resilient_mode: false,
        disable_cdf_update: false,
        allow_screen_content_tools: 0,
        force_integer_mv: 0,
        allow_high_precision_mv: false,
        interpolation_filter: vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_EIGHTTAP.0 as u32,
        interpolation_filter_label: "eighttap",
        is_filter_switchable: false,
        is_motion_mode_switchable: false,
        use_ref_frame_mvs: false,
        reference_select: false,
        skip_mode_present: false,
        allow_warped_motion: false,
        order_hint: None,
        primary_ref_frame: None,
        refresh_frame_flags: 0,
        reference_order_hints: Vec::new(),
        frame_refs_short_signaling: false,
        last_frame_idx: None,
        gold_frame_idx: None,
        ref_frame_indices: Vec::new(),
        render_and_frame_size_different: None,
        frame_width: None,
        frame_height: None,
        render_width: None,
        render_height: None,
        found_frame_header: false,
        found_tile_payload: false,
        vulkan_submit_candidate: false,
        unsupported_reason: Some(reason),
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeVulkanAv1ParsedFrameHeader {
    frame_header_bytes: usize,
    tile_count: u32,
    tile_columns: u32,
    tile_rows: u32,
    tile_size_bytes: u32,
    tile_bits: u32,
    tile_info: NativeVulkanAv1ParsedTileInfo,
    frame_type: u8,
    show_existing_frame: bool,
    frame_to_show_map_idx: Option<u8>,
    display_frame_id: Option<u32>,
    current_frame_id: Option<u32>,
    expected_frame_ids: Vec<u32>,
    show_frame: bool,
    showable_frame: bool,
    error_resilient_mode: bool,
    disable_cdf_update: bool,
    disable_frame_end_update_cdf: bool,
    allow_screen_content_tools: u8,
    force_integer_mv: u8,
    allow_high_precision_mv: bool,
    interpolation_filter: vk::video::StdVideoAV1InterpolationFilter,
    is_filter_switchable: bool,
    is_motion_mode_switchable: bool,
    use_ref_frame_mvs: bool,
    reference_select: bool,
    skip_mode_present: bool,
    allow_warped_motion: bool,
    frame_size_override_flag: bool,
    order_hint: Option<u8>,
    primary_ref_frame: Option<u8>,
    refresh_frame_flags: u8,
    reference_order_hints: Vec<u8>,
    frame_refs_short_signaling: bool,
    last_frame_idx: Option<u8>,
    gold_frame_idx: Option<u8>,
    ref_frame_indices: Vec<i8>,
    use_superres: bool,
    coded_denom: u8,
    render_and_frame_size_different: Option<bool>,
    frame_width: Option<u32>,
    frame_height: Option<u32>,
    render_width: Option<u32>,
    render_height: Option<u32>,
    quantization: NativeVulkanAv1ParsedQuantization,
    segmentation: NativeVulkanAv1ParsedSegmentation,
    delta_q: NativeVulkanAv1ParsedDeltaQ,
    delta_lf: NativeVulkanAv1ParsedDeltaLf,
    loop_filter: NativeVulkanAv1ParsedLoopFilter,
    cdef: NativeVulkanAv1ParsedCdef,
    loop_restoration: NativeVulkanAv1ParsedLoopRestoration,
    global_motion: NativeVulkanAv1ParsedGlobalMotion,
    tx_mode_select: bool,
    reduced_tx_set: bool,
    unsupported_reason: Option<String>,
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy)]
struct NativeVulkanAv1ParsedFrameHeaderPrefix {
    frame_type: u8,
    show_existing_frame: bool,
    frame_to_show_map_idx: Option<u8>,
    display_frame_id: Option<u32>,
    current_frame_id: Option<u32>,
    show_frame: bool,
    showable_frame: bool,
    error_resilient_mode: bool,
    disable_cdf_update: bool,
    disable_frame_end_update_cdf: bool,
    allow_screen_content_tools: u8,
    force_integer_mv: u8,
    allow_high_precision_mv: bool,
    interpolation_filter: vk::video::StdVideoAV1InterpolationFilter,
    is_filter_switchable: bool,
    is_motion_mode_switchable: bool,
    use_ref_frame_mvs: bool,
    reference_select: bool,
    skip_mode_present: bool,
    allow_warped_motion: bool,
    frame_size_override_flag: bool,
    order_hint: Option<u8>,
    primary_ref_frame: Option<u8>,
    refresh_frame_flags: u8,
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_partial_frame_header(
    bits: &NativeVulkanAv1BitReader<'_>,
    prefix: NativeVulkanAv1ParsedFrameHeaderPrefix,
    expected_frame_ids: Vec<u32>,
    reference_order_hints: Vec<u8>,
    frame_refs_short_signaling: bool,
    last_frame_idx: Option<u8>,
    gold_frame_idx: Option<u8>,
    ref_frame_indices: Vec<i8>,
    reason: String,
) -> NativeVulkanAv1ParsedFrameHeader {
    NativeVulkanAv1ParsedFrameHeader {
        frame_header_bytes: bits.byte_offset(),
        tile_count: 0,
        tile_columns: 0,
        tile_rows: 0,
        tile_size_bytes: 0,
        tile_bits: 0,
        tile_info: NativeVulkanAv1ParsedTileInfo {
            tile_count: 0,
            tile_columns: 0,
            tile_rows: 0,
            tile_size_bytes: 0,
            tile_bits: 0,
            uniform_tile_spacing_flag: false,
            context_update_tile_id: 0,
            mi_col_starts: Vec::new(),
            mi_row_starts: Vec::new(),
            width_in_sbs_minus_1: Vec::new(),
            height_in_sbs_minus_1: Vec::new(),
        },
        frame_type: prefix.frame_type,
        show_existing_frame: prefix.show_existing_frame,
        frame_to_show_map_idx: prefix.frame_to_show_map_idx,
        display_frame_id: prefix.display_frame_id,
        current_frame_id: prefix.current_frame_id,
        expected_frame_ids,
        show_frame: prefix.show_frame,
        showable_frame: prefix.showable_frame,
        error_resilient_mode: prefix.error_resilient_mode,
        disable_cdf_update: prefix.disable_cdf_update,
        disable_frame_end_update_cdf: prefix.disable_frame_end_update_cdf,
        allow_screen_content_tools: prefix.allow_screen_content_tools,
        force_integer_mv: prefix.force_integer_mv,
        allow_high_precision_mv: prefix.allow_high_precision_mv,
        interpolation_filter: prefix.interpolation_filter,
        is_filter_switchable: prefix.is_filter_switchable,
        is_motion_mode_switchable: prefix.is_motion_mode_switchable,
        use_ref_frame_mvs: prefix.use_ref_frame_mvs,
        reference_select: prefix.reference_select,
        skip_mode_present: prefix.skip_mode_present,
        allow_warped_motion: prefix.allow_warped_motion,
        frame_size_override_flag: prefix.frame_size_override_flag,
        order_hint: prefix.order_hint,
        primary_ref_frame: prefix.primary_ref_frame,
        refresh_frame_flags: prefix.refresh_frame_flags,
        reference_order_hints,
        frame_refs_short_signaling,
        last_frame_idx,
        gold_frame_idx,
        ref_frame_indices,
        use_superres: false,
        coded_denom: 8,
        render_and_frame_size_different: None,
        frame_width: None,
        frame_height: None,
        render_width: None,
        render_height: None,
        quantization: NativeVulkanAv1ParsedQuantization {
            base_q_idx: 0,
            delta_q_y_dc: 0,
            delta_q_u_dc: 0,
            delta_q_u_ac: 0,
            delta_q_v_dc: 0,
            delta_q_v_ac: 0,
            using_qmatrix: false,
            diff_uv_delta: false,
            qm_y: 0,
            qm_u: 0,
            qm_v: 0,
        },
        segmentation: NativeVulkanAv1ParsedSegmentation {
            enabled: false,
            update_map: false,
            temporal_update: false,
            update_data: false,
            feature_enabled: [0; 8],
            feature_data: [[0; 8]; 8],
        },
        delta_q: NativeVulkanAv1ParsedDeltaQ {
            present: false,
            res: 0,
        },
        delta_lf: NativeVulkanAv1ParsedDeltaLf {
            present: false,
            res: 0,
            multi: false,
        },
        loop_filter: NativeVulkanAv1ParsedLoopFilter {
            level: [0; 4],
            sharpness: 0,
            delta_enabled: false,
            delta_update: false,
            update_ref_delta: 0,
            ref_deltas: [1, 0, 0, 0, -1, 0, -1, -1],
            update_mode_delta: 0,
            mode_deltas: [0, 0],
        },
        cdef: NativeVulkanAv1ParsedCdef {
            damping_minus_3: 0,
            bits: 0,
            y_pri_strength: [0; 8],
            y_sec_strength: [0; 8],
            uv_pri_strength: [0; 8],
            uv_sec_strength: [0; 8],
        },
        loop_restoration: NativeVulkanAv1ParsedLoopRestoration {
            frame_restoration_type: [
                vk::video::STD_VIDEO_AV1_FRAME_RESTORATION_TYPE_NONE.0 as u32,
                vk::video::STD_VIDEO_AV1_FRAME_RESTORATION_TYPE_NONE.0 as u32,
                vk::video::STD_VIDEO_AV1_FRAME_RESTORATION_TYPE_NONE.0 as u32,
            ],
            loop_restoration_size: [0; 3],
            uses_lr: false,
            uses_chroma_lr: false,
        },
        global_motion: native_vulkan_av1_default_global_motion(),
        tx_mode_select: false,
        reduced_tx_set: false,
        unsupported_reason: Some(reason),
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeVulkanAv1ParsedQuantization {
    base_q_idx: u8,
    delta_q_y_dc: i8,
    delta_q_u_dc: i8,
    delta_q_u_ac: i8,
    delta_q_v_dc: i8,
    delta_q_v_ac: i8,
    using_qmatrix: bool,
    diff_uv_delta: bool,
    qm_y: u8,
    qm_u: u8,
    qm_v: u8,
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeVulkanAv1ParsedSegmentation {
    enabled: bool,
    update_map: bool,
    temporal_update: bool,
    update_data: bool,
    feature_enabled: [u8; 8],
    feature_data: [[i16; 8]; 8],
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeVulkanAv1ParsedDeltaQ {
    present: bool,
    res: u8,
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeVulkanAv1ParsedDeltaLf {
    present: bool,
    res: u8,
    multi: bool,
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeVulkanAv1ParsedLoopFilter {
    level: [u8; 4],
    sharpness: u8,
    delta_enabled: bool,
    delta_update: bool,
    update_ref_delta: u8,
    ref_deltas: [i8; 8],
    update_mode_delta: u8,
    mode_deltas: [i8; 2],
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeVulkanAv1ParsedCdef {
    damping_minus_3: u8,
    bits: u8,
    y_pri_strength: [u8; 8],
    y_sec_strength: [u8; 8],
    uv_pri_strength: [u8; 8],
    uv_sec_strength: [u8; 8],
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeVulkanAv1ParsedLoopRestoration {
    frame_restoration_type: [u32; 3],
    loop_restoration_size: [u16; 3],
    uses_lr: bool,
    uses_chroma_lr: bool,
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeVulkanAv1ParsedGlobalMotion {
    gm_type: [u8; 8],
    gm_params: [[i32; 6]; 8],
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_frame_header_for_submit(
    payload: &[u8],
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
) -> Result<NativeVulkanAv1ParsedFrameHeader, String> {
    native_vulkan_parse_av1_frame_header_for_submit_with_context(payload, sequence_header, None)
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_frame_header_for_submit_with_context(
    payload: &[u8],
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
    reference_context: Option<&NativeVulkanAv1FrameHeaderReferenceContext>,
) -> Result<NativeVulkanAv1ParsedFrameHeader, String> {
    if !sequence_header.vulkan_std_session_parameters_ready {
        return Err("AV1 sequence header is not Vulkan STD ready".to_owned());
    }
    if sequence_header.decoder_model_info_present_flag {
        return Err("AV1 decoder model frame header fields are not parsed yet".to_owned());
    }
    if sequence_header.enable_superres {
        return Err("AV1 superres frame headers are not parsed yet".to_owned());
    }
    if sequence_header.film_grain_params_present {
        return Err("AV1 film grain frame headers are not parsed yet".to_owned());
    }

    let mut bits = NativeVulkanAv1BitReader::new(payload);
    let mut show_existing_frame = false;
    let mut frame_to_show_map_idx = None;
    let mut display_frame_id = None;
    if !sequence_header.reduced_still_picture_header {
        show_existing_frame = bits.read_bool("show_existing_frame")?;
        if show_existing_frame {
            frame_to_show_map_idx = Some(native_vulkan_av1_u8(
                bits.read_bits(3, "frame_to_show_map_idx")?,
                "frame_to_show_map_idx",
            )?);
            if sequence_header.frame_id_numbers_present_flag {
                let frame_id_bits = u32::from(
                    sequence_header
                        .additional_frame_id_length_minus_1
                        .unwrap_or(0),
                ) + u32::from(
                    sequence_header.delta_frame_id_length_minus_2.unwrap_or(0),
                ) + 3;
                display_frame_id = Some(bits.read_bits(frame_id_bits, "display_frame_id")?);
            }
            let prefix = NativeVulkanAv1ParsedFrameHeaderPrefix {
                frame_type: u8::MAX,
                show_existing_frame,
                frame_to_show_map_idx,
                display_frame_id,
                current_frame_id: None,
                show_frame: true,
                showable_frame: false,
                error_resilient_mode: false,
                disable_cdf_update: true,
                disable_frame_end_update_cdf: true,
                allow_screen_content_tools: 0,
                force_integer_mv: 2,
                allow_high_precision_mv: false,
                interpolation_filter: vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_EIGHTTAP,
                is_filter_switchable: false,
                is_motion_mode_switchable: false,
                use_ref_frame_mvs: false,
                reference_select: false,
                skip_mode_present: false,
                allow_warped_motion: false,
                frame_size_override_flag: false,
                order_hint: None,
                primary_ref_frame: None,
                refresh_frame_flags: 0,
            };
            return Ok(native_vulkan_av1_partial_frame_header(
                &bits,
                prefix,
                Vec::new(),
                Vec::new(),
                false,
                None,
                None,
                Vec::new(),
                "AV1 show_existing_frame map index parsed; display handoff waits for reference map availability".to_owned(),
            ));
        }
    }

    let (frame_type, show_frame, showable_frame) = if sequence_header.reduced_still_picture_header {
        (0u8, true, false)
    } else {
        let frame_type = native_vulkan_av1_u8(bits.read_bits(2, "frame_type")?, "frame_type")?;
        let show_frame = bits.read_bool("show_frame")?;
        let showable_frame = if show_frame {
            frame_type != 0
        } else {
            bits.read_bool("showable_frame")?
        };
        (frame_type, show_frame, showable_frame)
    };
    let frame_is_intra = matches!(frame_type, 0 | 2);
    let error_resilient_mode = if sequence_header.reduced_still_picture_header
        || frame_type == 3
        || (frame_type == 0 && show_frame)
    {
        true
    } else {
        bits.read_bool("error_resilient_mode")?
    };
    let disable_cdf_update = bits.read_bool("disable_cdf_update")?;

    let allow_screen_content_tools = if sequence_header.seq_force_screen_content_tools == 2 {
        u8::from(bits.read_bool("allow_screen_content_tools")?)
    } else {
        sequence_header.seq_force_screen_content_tools
    };
    let force_integer_mv = if allow_screen_content_tools > 0 {
        if sequence_header.seq_force_integer_mv == 2 {
            u8::from(bits.read_bool("force_integer_mv")?)
        } else {
            sequence_header.seq_force_integer_mv
        }
    } else {
        0
    };

    let current_frame_id = if sequence_header.frame_id_numbers_present_flag {
        let frame_id_bits = u32::from(sequence_header.delta_frame_id_length_minus_2.unwrap_or(0))
            + u32::from(
                sequence_header
                    .additional_frame_id_length_minus_1
                    .unwrap_or(0),
            )
            + 3;
        Some(bits.read_bits(frame_id_bits, "current_frame_id")?)
    } else {
        None
    };

    let frame_size_override_flag =
        if frame_type != 3 && !sequence_header.reduced_still_picture_header {
            bits.read_bool("frame_size_override_flag")?
        } else {
            false
        };
    let order_hint = if sequence_header.enable_order_hint {
        let order_hint_bits = u32::from(sequence_header.order_hint_bits_minus_1.unwrap_or(0)) + 1;
        Some(native_vulkan_av1_u8(
            bits.read_bits(order_hint_bits, "order_hint")?,
            "order_hint",
        )?)
    } else {
        None
    };
    let primary_ref_frame = if !error_resilient_mode && !frame_is_intra {
        Some(native_vulkan_av1_u8(
            bits.read_bits(3, "primary_ref_frame")?,
            "primary_ref_frame",
        )?)
    } else {
        None
    };

    let refresh_frame_flags = if frame_type == 0 && show_frame {
        0xff
    } else if frame_type == 3 {
        0xff
    } else {
        native_vulkan_av1_u8(
            bits.read_bits(8, "refresh_frame_flags")?,
            "refresh_frame_flags",
        )?
    };

    let mut reference_order_hints = Vec::new();
    if !frame_is_intra || refresh_frame_flags != 0xff {
        if error_resilient_mode && sequence_header.enable_order_hint {
            let order_hint_bits =
                u32::from(sequence_header.order_hint_bits_minus_1.unwrap_or(0)) + 1;
            for _ in 0..8 {
                reference_order_hints.push(native_vulkan_av1_u8(
                    bits.read_bits(order_hint_bits, "ref_order_hint")?,
                    "ref_order_hint",
                )?);
            }
        }
    }

    let prefix = NativeVulkanAv1ParsedFrameHeaderPrefix {
        frame_type,
        show_existing_frame,
        frame_to_show_map_idx,
        display_frame_id,
        current_frame_id,
        show_frame,
        showable_frame,
        error_resilient_mode,
        disable_cdf_update,
        disable_frame_end_update_cdf: true,
        allow_screen_content_tools,
        force_integer_mv,
        allow_high_precision_mv: false,
        interpolation_filter: vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_EIGHTTAP,
        is_filter_switchable: false,
        is_motion_mode_switchable: false,
        use_ref_frame_mvs: false,
        reference_select: false,
        skip_mode_present: false,
        allow_warped_motion: false,
        frame_size_override_flag,
        order_hint,
        primary_ref_frame,
        refresh_frame_flags,
    };

    let mut frame_refs_short_signaling = false;
    let mut last_frame_idx = None;
    let mut gold_frame_idx = None;
    let mut ref_frame_indices = Vec::new();
    let mut expected_frame_ids = Vec::new();
    let mut allow_high_precision_mv = false;
    let mut interpolation_filter = vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_EIGHTTAP;
    let mut is_filter_switchable = false;
    let mut is_motion_mode_switchable = false;
    let mut use_ref_frame_mvs = false;
    let mut reference_select = false;
    let mut skip_mode_present = false;
    let mut allow_warped_motion = false;

    let (
        frame_width,
        frame_height,
        render_width,
        render_height,
        render_and_frame_size_different,
        use_superres,
        coded_denom,
        allow_intrabc,
    ) = if frame_is_intra {
        let frame_size = native_vulkan_parse_av1_frame_size(
            &mut bits,
            sequence_header,
            frame_size_override_flag,
        )?;
        let (use_superres, coded_denom) =
            native_vulkan_parse_av1_superres_params(&mut bits, sequence_header)?;
        let render_size = native_vulkan_parse_av1_render_size(&mut bits, frame_size)?;
        let allow_intrabc = allow_screen_content_tools > 0 && bits.read_bool("allow_intrabc")?;
        (
            Some(frame_size.0),
            Some(frame_size.1),
            Some(render_size.1),
            Some(render_size.2),
            Some(render_size.0),
            use_superres,
            coded_denom,
            allow_intrabc,
        )
    } else {
        if sequence_header.enable_order_hint {
            frame_refs_short_signaling = bits.read_bool("frame_refs_short_signaling")?;
            if frame_refs_short_signaling {
                last_frame_idx = Some(native_vulkan_av1_u8(
                    bits.read_bits(3, "last_frame_idx")?,
                    "last_frame_idx",
                )?);
                gold_frame_idx = Some(native_vulkan_av1_u8(
                    bits.read_bits(3, "gold_frame_idx")?,
                    "gold_frame_idx",
                )?);
            }
        }
        if frame_refs_short_signaling {
            return Ok(native_vulkan_av1_partial_frame_header(
                &bits,
                prefix,
                expected_frame_ids,
                reference_order_hints,
                frame_refs_short_signaling,
                last_frame_idx,
                gold_frame_idx,
                ref_frame_indices,
                "AV1 inter frame short reference signaling needs set_frame_refs slot expansion"
                    .to_owned(),
            ));
        }
        ref_frame_indices.reserve(7);
        for _ in 0..7 {
            ref_frame_indices.push(native_vulkan_av1_i8(
                bits.read_bits(3, "ref_frame_idx")?,
                "ref_frame_idx",
            )?);
            if sequence_header.frame_id_numbers_present_flag {
                let delta_frame_id_bits =
                    u32::from(sequence_header.delta_frame_id_length_minus_2.unwrap_or(0)) + 2;
                let delta_frame_id_minus_1 =
                    bits.read_bits(delta_frame_id_bits, "delta_frame_id_minus_1")?;
                let frame_id_bits =
                    u32::from(sequence_header.delta_frame_id_length_minus_2.unwrap_or(0))
                        + u32::from(
                            sequence_header
                                .additional_frame_id_length_minus_1
                                .unwrap_or(0),
                        )
                        + 3;
                let modulus = 1u64.checked_shl(frame_id_bits).unwrap_or(0).max(1);
                let current = u64::from(current_frame_id.unwrap_or(0));
                let delta = u64::from(delta_frame_id_minus_1).saturating_add(1);
                expected_frame_ids.push(((current + modulus - (delta % modulus)) % modulus) as u32);
            }
        }
        let inter_tail_parse = (|| -> Result<
            (
                (u32, u32),
                (bool, u32, u32),
                bool,
                u8,
                bool,
                vk::video::StdVideoAV1InterpolationFilter,
                bool,
                bool,
                bool,
                bool,
            ),
            String,
        > {
            let (frame_size, render_size, use_superres, coded_denom) =
                if frame_size_override_flag && !error_resilient_mode {
                    native_vulkan_parse_av1_frame_size_with_refs(
                        &mut bits,
                        sequence_header,
                        reference_context,
                    )?
                } else {
                let frame_size = native_vulkan_parse_av1_frame_size(
                    &mut bits,
                    sequence_header,
                    frame_size_override_flag,
                )?;
                let (use_superres, coded_denom) =
                    native_vulkan_parse_av1_superres_params(&mut bits, sequence_header)?;
                let render_size = native_vulkan_parse_av1_render_size(&mut bits, frame_size)?;
                (frame_size, render_size, use_superres, coded_denom)
                };

            let allow_high_precision_mv = if force_integer_mv != 1 {
                bits.read_bool("allow_high_precision_mv")?
            } else {
                false
            };
            let (interpolation_filter, is_filter_switchable) =
                native_vulkan_parse_av1_interpolation_filter(&mut bits)?;
            let is_motion_mode_switchable = bits.read_bool("is_motion_mode_switchable")?;
            let use_ref_frame_mvs = if !error_resilient_mode && sequence_header.enable_ref_frame_mvs
            {
                bits.read_bool("use_ref_frame_mvs")?
            } else {
                false
            };
            let allow_warped_motion = false;
            Ok((
                frame_size,
                render_size,
                use_superres,
                coded_denom,
                allow_high_precision_mv,
                interpolation_filter,
                is_filter_switchable,
                is_motion_mode_switchable,
                allow_warped_motion,
                use_ref_frame_mvs,
            ))
        })();
        let (
            frame_size,
            render_size,
            parsed_use_superres,
            parsed_coded_denom,
            parsed_allow_high_precision_mv,
            parsed_interpolation_filter,
            parsed_is_filter_switchable,
            parsed_is_motion_mode_switchable,
            parsed_allow_warped_motion,
            parsed_use_ref_frame_mvs,
        ) = match inter_tail_parse {
            Ok(parsed) => parsed,
            Err(reason) => {
                return Ok(native_vulkan_av1_partial_frame_header(
                    &bits,
                    prefix,
                    expected_frame_ids,
                    reference_order_hints,
                    frame_refs_short_signaling,
                    last_frame_idx,
                    gold_frame_idx,
                    ref_frame_indices,
                    format!(
                        "AV1 inter frame reference indices parsed; inter submit fields are not ready: {reason}"
                    ),
                ));
            }
        };
        allow_high_precision_mv = parsed_allow_high_precision_mv;
        interpolation_filter = parsed_interpolation_filter;
        is_filter_switchable = parsed_is_filter_switchable;
        is_motion_mode_switchable = parsed_is_motion_mode_switchable;
        allow_warped_motion = parsed_allow_warped_motion;
        use_ref_frame_mvs = parsed_use_ref_frame_mvs;
        (
            Some(frame_size.0),
            Some(frame_size.1),
            Some(render_size.1),
            Some(render_size.2),
            Some(render_size.0),
            parsed_use_superres,
            parsed_coded_denom,
            false,
        )
    };
    if allow_intrabc {
        return Err(
            "AV1 intra block copy is not supported by the first direct submit gate".to_owned(),
        );
    }
    let disable_frame_end_update_cdf = if !disable_cdf_update {
        bits.read_bool("disable_frame_end_update_cdf")?
    } else {
        true
    };

    let primary_reference_history =
        reference_context.and_then(|context| context.primary_reference_history(primary_ref_frame));

    let tile_info = native_vulkan_parse_av1_tile_info(
        &mut bits,
        sequence_header,
        frame_width.unwrap_or(sequence_header.max_frame_width),
        frame_height.unwrap_or(sequence_header.max_frame_height),
    )?;
    let quantization = native_vulkan_parse_av1_quantization_params(&mut bits, sequence_header)?;
    let segmentation = native_vulkan_parse_av1_segmentation_params(
        &mut bits,
        primary_ref_frame,
        primary_reference_history,
    )?;
    let delta_q = native_vulkan_parse_av1_delta_q_params(&mut bits)?;
    let delta_lf = native_vulkan_parse_av1_delta_lf_params(&mut bits, delta_q.present)?;
    let loop_filter = native_vulkan_parse_av1_loop_filter_params(
        &mut bits,
        sequence_header,
        primary_reference_history,
    )?;
    let cdef = native_vulkan_parse_av1_cdef_params(&mut bits, sequence_header)?;
    let loop_restoration =
        native_vulkan_parse_av1_loop_restoration_params(&mut bits, sequence_header)?;
    let tx_mode_select = native_vulkan_parse_av1_tx_mode(&mut bits)?;
    let mut global_motion = native_vulkan_av1_default_global_motion();
    let reduced_tx_set;
    if !frame_is_intra {
        reference_select = bits.read_bool("reference_select")?;
        let skip_mode_allowed = reference_context
            .and_then(|context| {
                native_vulkan_av1_skip_mode_frame_from_order_hints(
                    sequence_header,
                    frame_type,
                    error_resilient_mode,
                    reference_select,
                    order_hint.unwrap_or(0),
                    context.reference_name_order_hints,
                    context.reference_name_slot_indices,
                )
            })
            .is_some();
        if !native_vulkan_av1_skip_mode_parse_disabled()
            && (skip_mode_allowed
                || (reference_context.is_none()
                    && native_vulkan_av1_skip_mode_present_field_allowed(
                        sequence_header,
                        error_resilient_mode,
                        frame_type,
                    )))
        {
            skip_mode_present = bits.read_bool("skip_mode_present")?;
        }
        allow_warped_motion = if sequence_header.enable_warped_motion && !error_resilient_mode {
            bits.read_bool("allow_warped_motion")?
        } else {
            false
        };
        reduced_tx_set = bits.read_bool("reduced_tx_set")?;
        global_motion = native_vulkan_parse_av1_global_motion_params(
            &mut bits,
            sequence_header,
            allow_warped_motion,
        )?;
    } else {
        reduced_tx_set = bits.read_bool("reduced_tx_set")?;
    }
    let alignment_reason =
        native_vulkan_av1_zero_align_to_byte_with_reason(&mut bits, "frame_header_byte_alignment")?;

    Ok(NativeVulkanAv1ParsedFrameHeader {
        frame_header_bytes: bits.byte_offset(),
        tile_count: tile_info.tile_count,
        tile_columns: tile_info.tile_columns,
        tile_rows: tile_info.tile_rows,
        tile_size_bytes: tile_info.tile_size_bytes,
        tile_bits: tile_info.tile_bits,
        tile_info,
        frame_type,
        show_existing_frame,
        frame_to_show_map_idx,
        display_frame_id,
        current_frame_id,
        expected_frame_ids,
        show_frame,
        showable_frame,
        error_resilient_mode,
        disable_cdf_update,
        disable_frame_end_update_cdf,
        allow_screen_content_tools,
        force_integer_mv,
        allow_high_precision_mv,
        interpolation_filter,
        is_filter_switchable,
        is_motion_mode_switchable,
        use_ref_frame_mvs,
        reference_select,
        skip_mode_present,
        allow_warped_motion,
        frame_size_override_flag,
        order_hint,
        primary_ref_frame,
        refresh_frame_flags,
        reference_order_hints,
        frame_refs_short_signaling,
        last_frame_idx,
        gold_frame_idx,
        ref_frame_indices,
        use_superres,
        coded_denom,
        render_and_frame_size_different,
        frame_width,
        frame_height,
        render_width,
        render_height,
        quantization,
        segmentation,
        delta_q,
        delta_lf,
        loop_filter,
        cdef,
        loop_restoration,
        global_motion,
        tx_mode_select,
        reduced_tx_set,
        unsupported_reason: alignment_reason,
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_frame_size(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
    frame_size_override_flag: bool,
) -> Result<(u32, u32), String> {
    let (width_minus_1, height_minus_1) = if frame_size_override_flag {
        let width_bits = u32::from(sequence_header.frame_width_bits_minus_1) + 1;
        let height_bits = u32::from(sequence_header.frame_height_bits_minus_1) + 1;
        (
            bits.read_bits(width_bits, "frame_width_minus_1")?,
            bits.read_bits(height_bits, "frame_height_minus_1")?,
        )
    } else {
        (
            sequence_header.max_frame_width_minus_1,
            sequence_header.max_frame_height_minus_1,
        )
    };
    Ok((
        width_minus_1
            .checked_add(1)
            .ok_or_else(|| "AV1 frame width overflow".to_owned())?,
        height_minus_1
            .checked_add(1)
            .ok_or_else(|| "AV1 frame height overflow".to_owned())?,
    ))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_render_size(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    frame_size: (u32, u32),
) -> Result<(bool, u32, u32), String> {
    let render_and_frame_size_different = bits.read_bool("render_and_frame_size_different")?;
    if render_and_frame_size_different {
        Ok((
            true,
            bits.read_bits(16, "render_width_minus_1")?
                .checked_add(1)
                .ok_or_else(|| "AV1 render width overflow".to_owned())?,
            bits.read_bits(16, "render_height_minus_1")?
                .checked_add(1)
                .ok_or_else(|| "AV1 render height overflow".to_owned())?,
        ))
    } else {
        Ok((false, frame_size.0, frame_size.1))
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_frame_size_with_refs(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
    reference_context: Option<&NativeVulkanAv1FrameHeaderReferenceContext>,
) -> Result<((u32, u32), (bool, u32, u32), bool, u8), String> {
    for reference_index in 0..vk::MAX_VIDEO_AV1_REFERENCES_PER_FRAME_KHR {
        if bits.read_bool("found_ref")? {
            let (use_superres, coded_denom) =
                native_vulkan_parse_av1_superres_params(bits, sequence_header)?;
            let history = reference_context
                .and_then(|context| context.reference_histories[reference_index])
                .ok_or_else(|| {
                    format!(
                        "AV1 frame_size_with_refs selected reference {} but no reference size history is available",
                        reference_index + 1
                    )
                })?;
            let frame_size = (history.frame_width, history.frame_height);
            return Ok((
                frame_size,
                (false, history.render_width, history.render_height),
                use_superres,
                coded_denom,
            ));
        }
    }
    let frame_size = native_vulkan_parse_av1_frame_size(bits, sequence_header, true)?;
    let (use_superres, coded_denom) =
        native_vulkan_parse_av1_superres_params(bits, sequence_header)?;
    let render_size = native_vulkan_parse_av1_render_size(bits, frame_size)?;
    Ok((frame_size, render_size, use_superres, coded_denom))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_superres_params(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
) -> Result<(bool, u8), String> {
    const SUPERRES_NUM: u8 = 8;
    const SUPERRES_DENOM_MIN: u8 = 9;
    const SUPERRES_DENOM_BITS: u32 = 3;

    if !sequence_header.enable_superres {
        return Ok((false, SUPERRES_NUM));
    }

    let use_superres = bits.read_bool("use_superres")?;
    if !use_superres {
        return Ok((false, SUPERRES_NUM));
    }

    let denom = native_vulkan_av1_u8(
        bits.read_bits(SUPERRES_DENOM_BITS, "coded_denom")?,
        "coded_denom",
    )?
    .saturating_add(SUPERRES_DENOM_MIN);
    Err(format!(
        "AV1 superres coded_denom {denom} is not supported by the direct Vulkan submit path yet"
    ))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_interpolation_filter(
    bits: &mut NativeVulkanAv1BitReader<'_>,
) -> Result<(vk::video::StdVideoAV1InterpolationFilter, bool), String> {
    let is_filter_switchable = bits.read_bool("is_filter_switchable")?;
    if is_filter_switchable {
        return Ok((
            vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_SWITCHABLE,
            true,
        ));
    }
    let filter = match bits.read_bits(2, "interpolation_filter")? {
        0 => vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_EIGHTTAP,
        1 => vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_EIGHTTAP_SMOOTH,
        2 => vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_EIGHTTAP_SHARP,
        3 => vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_BILINEAR,
        other => return Err(format!("AV1 interpolation_filter {other} is invalid")),
    };
    Ok((filter, false))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_skip_mode_present_field_allowed(
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
    error_resilient_mode: bool,
    frame_type: u8,
) -> bool {
    sequence_header.enable_order_hint && !error_resilient_mode && frame_type == 1
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_global_motion_params(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
    parse_global_motion: bool,
) -> Result<NativeVulkanAv1ParsedGlobalMotion, String> {
    let mut global_motion = native_vulkan_av1_default_global_motion();
    if !parse_global_motion || !sequence_header.enable_warped_motion {
        return Ok(global_motion);
    }
    for reference_index in 1..=7 {
        if bits.read_bool("is_global")? {
            let is_rot_zoom = bits.read_bool("is_rot_zoom")?;
            let gm_type = if is_rot_zoom {
                2
            } else if bits.read_bool("is_translation")? {
                1
            } else {
                3
            };
            global_motion.gm_type[reference_index] = gm_type;
            match gm_type {
                1 => {
                    global_motion.gm_params[reference_index][0] =
                        native_vulkan_av1_read_global_param(bits, gm_type, 0, 0)?;
                    global_motion.gm_params[reference_index][1] =
                        native_vulkan_av1_read_global_param(bits, gm_type, 1, 0)?;
                }
                2 => {
                    let gm2 = native_vulkan_av1_read_global_param(
                        bits,
                        gm_type,
                        2,
                        1 << AV1_WARPEDMODEL_PREC_BITS,
                    )?;
                    let gm3 = native_vulkan_av1_read_global_param(bits, gm_type, 3, 0)?;
                    global_motion.gm_params[reference_index][2] = gm2;
                    global_motion.gm_params[reference_index][3] = gm3;
                    global_motion.gm_params[reference_index][4] = -gm3;
                    global_motion.gm_params[reference_index][5] = gm2;
                    global_motion.gm_params[reference_index][0] =
                        native_vulkan_av1_read_global_param(bits, gm_type, 0, 0)?;
                    global_motion.gm_params[reference_index][1] =
                        native_vulkan_av1_read_global_param(bits, gm_type, 1, 0)?;
                }
                3 => {
                    for param_index in 2..=5 {
                        let default = if param_index == 2 || param_index == 5 {
                            1 << AV1_WARPEDMODEL_PREC_BITS
                        } else {
                            0
                        };
                        global_motion.gm_params[reference_index][param_index] =
                            native_vulkan_av1_read_global_param(
                                bits,
                                gm_type,
                                param_index,
                                default,
                            )?;
                    }
                    global_motion.gm_params[reference_index][0] =
                        native_vulkan_av1_read_global_param(bits, gm_type, 0, 0)?;
                    global_motion.gm_params[reference_index][1] =
                        native_vulkan_av1_read_global_param(bits, gm_type, 1, 0)?;
                }
                _ => return Err(format!("AV1 global motion type {gm_type} is invalid")),
            }
        }
    }
    Ok(global_motion)
}

#[cfg(any(feature = "native-vulkan-video", test))]
const AV1_GM_ABS_TRANS_BITS: u32 = 12;
#[cfg(any(feature = "native-vulkan-video", test))]
const AV1_GM_ABS_TRANS_ONLY_BITS: u32 = 9;
#[cfg(any(feature = "native-vulkan-video", test))]
const AV1_GM_ABS_ALPHA_BITS: u32 = 12;
#[cfg(any(feature = "native-vulkan-video", test))]
const AV1_GM_ALPHA_PREC_BITS: u32 = 15;
#[cfg(any(feature = "native-vulkan-video", test))]
const AV1_GM_TRANS_PREC_BITS: u32 = 6;
#[cfg(any(feature = "native-vulkan-video", test))]
const AV1_GM_TRANS_ONLY_PREC_BITS: u32 = 3;
#[cfg(any(feature = "native-vulkan-video", test))]
const AV1_WARPEDMODEL_PREC_BITS: u32 = 16;
#[cfg(any(feature = "native-vulkan-video", test))]
const AV1_SUBEXP_K: u32 = 3;

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_default_global_motion() -> NativeVulkanAv1ParsedGlobalMotion {
    let mut gm_params = [[0i32; 6]; 8];
    for params in &mut gm_params {
        params[2] = 1 << AV1_WARPEDMODEL_PREC_BITS;
        params[5] = 1 << AV1_WARPEDMODEL_PREC_BITS;
    }
    NativeVulkanAv1ParsedGlobalMotion {
        gm_type: [0; 8],
        gm_params,
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_read_global_param(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    gm_type: u8,
    param_index: usize,
    previous_value: i32,
) -> Result<i32, String> {
    let (abs_bits, prec_bits) = if param_index < 2 {
        if gm_type == 1 {
            (AV1_GM_ABS_TRANS_ONLY_BITS, AV1_GM_TRANS_ONLY_PREC_BITS)
        } else {
            (AV1_GM_ABS_TRANS_BITS, AV1_GM_TRANS_PREC_BITS)
        }
    } else {
        (AV1_GM_ABS_ALPHA_BITS, AV1_GM_ALPHA_PREC_BITS)
    };
    let precision_diff = AV1_WARPEDMODEL_PREC_BITS
        .checked_sub(prec_bits)
        .ok_or_else(|| "AV1 global motion precision underflow".to_owned())?;
    let round = if param_index == 2 || param_index == 5 {
        1 << AV1_WARPEDMODEL_PREC_BITS
    } else {
        0
    };
    let reference = (previous_value - round) >> precision_diff;
    let mx = 1i32
        .checked_shl(abs_bits)
        .ok_or_else(|| "AV1 global motion mx overflow".to_owned())?;
    let value = native_vulkan_av1_decode_signed_subexp_with_ref(
        bits,
        -mx,
        mx + 1,
        AV1_SUBEXP_K,
        reference,
        "global_motion_param",
    )?;
    value
        .checked_shl(precision_diff)
        .and_then(|value| value.checked_add(round))
        .ok_or_else(|| "AV1 global motion parameter overflow".to_owned())
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_decode_signed_subexp_with_ref(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    low: i32,
    high: i32,
    k: u32,
    reference: i32,
    label: &'static str,
) -> Result<i32, String> {
    if high <= low {
        return Err(format!(
            "{label} has invalid signed subexp range {low}..{high}"
        ));
    }
    let range = u32::try_from(high - low).map_err(|_| format!("{label} range exceeds u32"))?;
    let reference = (reference - low).clamp(0, high - low - 1) as u32;
    let value =
        native_vulkan_av1_decode_unsigned_subexp_with_ref(bits, range, k, reference, label)?;
    i32::try_from(value)
        .map(|value| value + low)
        .map_err(|_| format!("{label} value exceeds i32"))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_decode_unsigned_subexp_with_ref(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    mx: u32,
    k: u32,
    reference: u32,
    label: &'static str,
) -> Result<u32, String> {
    let value = native_vulkan_av1_decode_subexp(bits, mx, k, label)?;
    if reference.saturating_mul(2) <= mx {
        native_vulkan_av1_inverse_recenter(reference, value)
    } else {
        let recentered = native_vulkan_av1_inverse_recenter(mx - 1 - reference, value)?;
        Ok(mx - 1 - recentered)
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_decode_subexp(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    num_syms: u32,
    k: u32,
    label: &'static str,
) -> Result<u32, String> {
    let mut index = 0u32;
    let mut mk = 0u32;
    loop {
        let b = if index == 0 { k } else { k + index - 1 };
        let a = 1u32
            .checked_shl(b)
            .ok_or_else(|| format!("{label} subexp shift overflow"))?;
        if num_syms <= mk.saturating_add(3u32.saturating_mul(a)) {
            return Ok(mk + bits.read_quniform(num_syms - mk, label)?);
        }
        if bits.read_bool(label)? {
            index = index.saturating_add(1);
            mk = mk.saturating_add(a);
        } else {
            return Ok(mk + bits.read_bits(b, label)?);
        }
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_inverse_recenter(reference: u32, value: u32) -> Result<u32, String> {
    if value > reference.saturating_mul(2) {
        return Ok(value);
    }
    if value.is_multiple_of(2) {
        reference
            .checked_add(value / 2)
            .ok_or_else(|| "AV1 inverse_recenter overflow".to_owned())
    } else {
        reference
            .checked_sub(value.div_ceil(2))
            .ok_or_else(|| "AV1 inverse_recenter underflow".to_owned())
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeVulkanAv1ParsedTileInfo {
    tile_count: u32,
    tile_columns: u32,
    tile_rows: u32,
    tile_size_bytes: u32,
    tile_bits: u32,
    uniform_tile_spacing_flag: bool,
    context_update_tile_id: u16,
    mi_col_starts: Vec<u16>,
    mi_row_starts: Vec<u16>,
    width_in_sbs_minus_1: Vec<u16>,
    height_in_sbs_minus_1: Vec<u16>,
}
