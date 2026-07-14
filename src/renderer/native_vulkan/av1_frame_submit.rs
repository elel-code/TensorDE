
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

include!("av1_frame_submit/frame_header_parser.rs");
