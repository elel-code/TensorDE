//! Temporary Vulkanalia bridge into the existing GST bitstream extractor.
//!
//! This keeps new Vulkanalia-facing extraction API out of the legacy renderer
//! file while the demux/codec layers are split properly.

use std::path::PathBuf;

use super::codec_snapshots::{
    NativeVulkanAv1SequenceHeaderSnapshot, NativeVulkanH264ParameterSetSnapshot,
    NativeVulkanH265ParameterSetSnapshot,
};
use super::video_extract::{
    native_vulkan_extract_video_bitstream, native_vulkan_h265_ready_prefix_bitstream_window,
    native_vulkan_h265_ready_prefix_bitstream_window_mode,
    native_vulkan_validate_h264_ready_prefix, native_vulkan_validate_h265_ready_prefix,
};
use super::vulkanalia_backend::{
    NativeVulkanVulkanaliaAv1CdefPlan, NativeVulkanVulkanaliaAv1FrameSubmitInput,
    NativeVulkanVulkanaliaAv1GlobalMotionPlan, NativeVulkanVulkanaliaAv1LoopFilterPlan,
    NativeVulkanVulkanaliaAv1LoopRestorationPlan, NativeVulkanVulkanaliaAv1QuantizationPlan,
    NativeVulkanVulkanaliaAv1ReadyPrefixDecodeInput,
    NativeVulkanVulkanaliaAv1ReadyPrefixFrameInput, NativeVulkanVulkanaliaAv1ReferenceInfoPlan,
    NativeVulkanVulkanaliaAv1SegmentationPlan, NativeVulkanVulkanaliaAv1TileInfoPlan,
    NativeVulkanVulkanaliaH264ReadyPrefixDecodeInput,
    NativeVulkanVulkanaliaH264ReadyPrefixFrameInput,
    NativeVulkanVulkanaliaH265ReadyPrefixDecodeInput,
    NativeVulkanVulkanaliaH265ReadyPrefixFrameInput,
};
use super::{
    NativeVulkanError, NativeVulkanVideoSessionCodec, NativeVulkanVideoSessionSmokeOptions,
};

pub fn native_vulkan_extract_h264_parameter_sets_for_vulkanalia(
    source: PathBuf,
    max_samples: u32,
) -> Result<NativeVulkanH264ParameterSetSnapshot, NativeVulkanError> {
    let mut options = NativeVulkanVideoSessionSmokeOptions {
        codec: NativeVulkanVideoSessionCodec::H264High8,
        extract_bitstream: true,
        bitstream_source: Some(source),
        bitstream_extract_max_samples: max_samples.max(1),
        ..NativeVulkanVideoSessionSmokeOptions::default()
    };
    options.allocate_bitstream_buffer = false;
    let extract = native_vulkan_extract_video_bitstream(&options)?;
    extract.snapshot.h264_parameter_sets.ok_or_else(|| {
        NativeVulkanError::Video(
            "Vulkanalia real H.264 session parameters require parsed SPS/PPS".to_owned(),
        )
    })
}

pub fn native_vulkan_extract_h264_ready_prefix_for_vulkanalia(
    source: PathBuf,
    max_samples: u32,
    frame_count: u32,
) -> Result<NativeVulkanVulkanaliaH264ReadyPrefixDecodeInput, NativeVulkanError> {
    if frame_count == 0 {
        return Err(NativeVulkanError::Video(
            "Vulkanalia H.264 ready-prefix extraction requires at least one frame".to_owned(),
        ));
    }

    let mut options = NativeVulkanVideoSessionSmokeOptions {
        codec: NativeVulkanVideoSessionCodec::H264High8,
        extract_bitstream: true,
        bitstream_source: Some(source),
        bitstream_extract_max_samples: max_samples.max(frame_count).max(1),
        ..NativeVulkanVideoSessionSmokeOptions::default()
    };
    options.allocate_bitstream_buffer = false;
    let extract = native_vulkan_extract_video_bitstream(&options)?;
    native_vulkan_validate_h264_ready_prefix(&extract.snapshot, frame_count)?;

    let parameter_sets = extract
        .snapshot
        .h264_parameter_sets
        .clone()
        .ok_or_else(|| {
            NativeVulkanError::Video(
                "Vulkanalia H.264 ready-prefix extraction requires parsed SPS/PPS".to_owned(),
            )
        })?;
    let entries = extract
        .snapshot
        .h264_decode_reference_plan
        .get(..frame_count as usize)
        .ok_or_else(|| {
            NativeVulkanError::Video(format!(
                "H.264 reference plan has {} frames but {frame_count} were requested",
                extract.snapshot.h264_decode_reference_plan.len()
            ))
        })?;
    let access_units = extract
        .snapshot
        .h264_access_units
        .get(..frame_count as usize)
        .ok_or_else(|| {
            NativeVulkanError::Video(format!(
                "H.264 access unit snapshot has {} frames but {frame_count} were requested",
                extract.snapshot.h264_access_units.len()
            ))
        })?;
    let payloads = extract
        .h264_access_unit_payloads
        .get(..frame_count as usize)
        .ok_or_else(|| {
            NativeVulkanError::Video(format!(
                "H.264 bitstream has {} payloads but {frame_count} ready-prefix frames were requested",
                extract.h264_access_unit_payloads.len()
            ))
        })?;

    let mut frames = Vec::with_capacity(frame_count as usize);
    for ((entry, access_unit), payload) in entries.iter().zip(access_units).zip(payloads) {
        let first_slice = access_unit.first_slice.clone().ok_or_else(|| {
            NativeVulkanError::Video(format!(
                "H.264 AU {} has no parsed first slice",
                access_unit.index
            ))
        })?;
        if let Some(err) = &access_unit.first_slice_parse_error {
            return Err(NativeVulkanError::Video(format!(
                "H.264 AU {} first slice parse failed: {err}",
                access_unit.index
            )));
        }
        if first_slice.slice_offsets.is_empty() {
            return Err(NativeVulkanError::Video(format!(
                "H.264 AU {} has no slice offsets",
                access_unit.index
            )));
        }
        frames.push(NativeVulkanVulkanaliaH264ReadyPrefixFrameInput {
            entry: entry.clone(),
            slice_offsets: first_slice.slice_offsets.clone(),
            first_slice,
            duration_ms: access_unit.duration_ms,
            access_unit_payload: payload.clone(),
        });
    }

    Ok(NativeVulkanVulkanaliaH264ReadyPrefixDecodeInput {
        parameter_sets,
        requested_frame_count: frame_count,
        frames,
    })
}

pub fn native_vulkan_extract_av1_sequence_header_for_vulkanalia(
    source: PathBuf,
    codec: NativeVulkanVideoSessionCodec,
    max_samples: u32,
) -> Result<NativeVulkanAv1SequenceHeaderSnapshot, NativeVulkanError> {
    if !matches!(
        codec,
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10
    ) {
        return Err(NativeVulkanError::Video(
            "Vulkanalia real session-parameter extraction currently supports AV1 only".to_owned(),
        ));
    }

    let mut options = NativeVulkanVideoSessionSmokeOptions {
        codec,
        extract_bitstream: true,
        bitstream_source: Some(source),
        bitstream_extract_max_samples: max_samples.max(1),
        ..NativeVulkanVideoSessionSmokeOptions::default()
    };
    options.allocate_bitstream_buffer = false;
    let extract = native_vulkan_extract_video_bitstream(&options)?;
    extract.snapshot.av1_sequence_header.ok_or_else(|| {
        NativeVulkanError::Video(
            "Vulkanalia real AV1 session parameters require parsed sequence header".to_owned(),
        )
    })
}

#[cfg(feature = "native-vulkan-gst-video")]
pub fn native_vulkan_extract_av1_ready_prefix_for_vulkanalia(
    source: PathBuf,
    codec: NativeVulkanVideoSessionCodec,
    max_samples: u32,
    frame_count: u32,
) -> Result<NativeVulkanVulkanaliaAv1ReadyPrefixDecodeInput, NativeVulkanError> {
    if !matches!(
        codec,
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10
    ) {
        return Err(NativeVulkanError::Video(
            "Vulkanalia AV1 ready-prefix extraction requires an AV1 codec".to_owned(),
        ));
    }
    if frame_count == 0 {
        return Err(NativeVulkanError::Video(
            "Vulkanalia AV1 ready-prefix extraction requires at least one frame".to_owned(),
        ));
    }

    let mut options = NativeVulkanVideoSessionSmokeOptions {
        codec,
        extract_bitstream: true,
        bitstream_source: Some(source),
        bitstream_extract_max_samples: max_samples.max(frame_count.saturating_mul(2)).max(1),
        ..NativeVulkanVideoSessionSmokeOptions::default()
    };
    options.allocate_bitstream_buffer = false;
    let extract = native_vulkan_extract_video_bitstream(&options)?;
    let sequence_header = extract
        .snapshot
        .av1_sequence_header
        .clone()
        .ok_or_else(|| {
            NativeVulkanError::Video(
                "Vulkanalia AV1 ready-prefix extraction requires parsed sequence header".to_owned(),
            )
        })?;
    if extract.snapshot.av1_decode_reference_plan.len() != extract.snapshot.av1_temporal_units.len()
        || extract.snapshot.av1_temporal_units.len() != extract.av1_temporal_unit_payloads.len()
    {
        return Err(NativeVulkanError::Video(format!(
            "AV1 extract mismatch: reference_plan={}, temporal_units={}, payloads={}",
            extract.snapshot.av1_decode_reference_plan.len(),
            extract.snapshot.av1_temporal_units.len(),
            extract.av1_temporal_unit_payloads.len()
        )));
    }

    let mut active_dpb_refs = vec![
        None::<super::NativeVulkanAv1ActiveDpbReference>;
        extract.snapshot.av1_reference_plan_dpb_slots.max(1) as usize
    ];
    let order_hint_offset_enabled = super::native_vulkan_av1_order_hint_offset_enabled(0);
    let mut frames = Vec::with_capacity(frame_count as usize);
    for ((entry, temporal_unit), payload) in extract
        .snapshot
        .av1_decode_reference_plan
        .iter()
        .zip(extract.snapshot.av1_temporal_units.iter())
        .zip(extract.av1_temporal_unit_payloads.iter())
    {
        if frames.len() >= frame_count as usize {
            break;
        }
        if !entry.ready_for_decode_submit {
            if entry.ready_for_display_handoff {
                continue;
            }
            return Err(NativeVulkanError::Video(format!(
                "Vulkanalia AV1 TU {} is not decode-ready: {}",
                entry.temporal_unit_index,
                entry
                    .unsupported_reason
                    .as_deref()
                    .unwrap_or("display handoff or unresolved references")
            )));
        }
        let prepared_reference_context =
            super::native_vulkan_av1_prepared_reference_context(entry, &active_dpb_refs);
        let decode_info = super::native_vulkan_av1_temporal_unit_decode_info(
            payload,
            &temporal_unit.obus,
            &sequence_header,
            Some(&prepared_reference_context.reference_context),
        )
        .map_err(NativeVulkanError::Video)?;
        let order_hint = decode_info.header.order_hint.unwrap_or(0);
        let ref_frame_sign_bias = super::native_vulkan_av1_dpb_reference_sign_bias(
            &sequence_header,
            decode_info.header.frame_type,
            order_hint,
            prepared_reference_context.reference_name_order_hints,
        );
        let setup_saved_order_hints = super::native_vulkan_av1_current_setup_saved_order_hints(
            prepared_reference_context.reference_name_order_hints,
            decode_info.header.refresh_frame_flags,
            order_hint,
        );
        let skip_mode_frame = super::native_vulkan_av1_skip_mode_frame_from_order_hints(
            &sequence_header,
            decode_info.header.frame_type,
            decode_info.header.error_resilient_mode,
            decode_info.header.reference_select,
            order_hint,
            prepared_reference_context.reference_name_order_hints,
            prepared_reference_context.reference_name_dpb_slot_indices,
        )
        .or_else(|| (!decode_info.header.skip_mode_present).then_some([0; 2]))
        .ok_or_else(|| {
            NativeVulkanError::Video(format!(
                "AV1 TU {} signaled skip_mode_present but no skip-mode reference pair was available",
                entry.temporal_unit_index
            ))
        })?;
        let mut reference_name_ref_frame_indices = Vec::new();
        for ref_frame_index in decode_info.header.ref_frame_indices.iter().take(7) {
            reference_name_ref_frame_indices.push(i32::from(*ref_frame_index));
        }
        while reference_name_ref_frame_indices.len() < 7 {
            reference_name_ref_frame_indices.push(-1);
        }
        let reference_name_slot_indices = if matches!(
            std::env::var("GILDER_VULKAN_AV1_REFERENCE_NAME_INDICES")
                .ok()
                .as_deref(),
            Some("ref-frame-idx") | Some("frame-store") | Some("bitstream")
        ) {
            reference_name_ref_frame_indices
        } else {
            prepared_reference_context
                .reference_name_dpb_slot_indices
                .to_vec()
        };
        let mut decode_reference_slot_ids = entry
            .decode_reference_slots
            .iter()
            .filter_map(|slot| u32::try_from(*slot).ok())
            .collect::<Vec<_>>();
        decode_reference_slot_ids.sort_unstable();
        decode_reference_slot_ids.dedup();
        let references = decode_reference_slot_ids
            .iter()
            .map(|slot| {
                let reference = active_dpb_refs
                    .get(*slot as usize)
                    .and_then(|reference| *reference)
                    .ok_or_else(|| {
                        NativeVulkanError::Video(format!(
                            "Vulkanalia AV1 TU {} references inactive DPB slot {}",
                            entry.temporal_unit_index, slot
                        ))
                    })?;
                Ok(native_vulkan_vulkanalia_av1_reference_info_from_active(
                    *slot as i32,
                    reference,
                    order_hint_offset_enabled,
                ))
            })
            .collect::<Result<Vec<_>, NativeVulkanError>>()?;
        let setup_reference = native_vulkan_vulkanalia_av1_reference_info_from_decode_info(
            entry.output_slot.ok_or_else(|| {
                NativeVulkanError::Video(format!(
                    "Vulkanalia AV1 TU {} has no output slot",
                    entry.temporal_unit_index
                ))
            })? as i32,
            &decode_info,
            ref_frame_sign_bias,
            setup_saved_order_hints,
            order_hint_offset_enabled,
        );
        let tile_size_bytes_minus_1 =
            u8::try_from(decode_info.header.tile_size_bytes.saturating_sub(1)).map_err(|_| {
                NativeVulkanError::Video(format!(
                    "AV1 TU {} tile_size_bytes {} exceeds Vulkanalia u8 range",
                    entry.temporal_unit_index, decode_info.header.tile_size_bytes
                ))
            })?;
        let frame_header_offset_for_vulkan =
            super::native_vulkan_av1_frame_header_offset_for_vulkan(&decode_info)?;
        let tile_offsets =
            super::native_vulkan_av1_offsets_for_vulkan(&decode_info.tile_offsets, 0)?;
        let frame = NativeVulkanVulkanaliaAv1FrameSubmitInput {
            temporal_unit_index: entry.temporal_unit_index,
            frame_header_offset_for_vulkan,
            tile_offsets,
            tile_sizes: decode_info.tile_sizes.clone(),
            tile_info: NativeVulkanVulkanaliaAv1TileInfoPlan {
                uniform_tile_spacing_flag: decode_info.header.tile_info.uniform_tile_spacing_flag,
                tile_columns: u8::try_from(decode_info.header.tile_info.tile_columns).map_err(
                    |_| {
                        NativeVulkanError::Video(format!(
                            "AV1 TU {} tile_columns {} exceeds u8 range",
                            entry.temporal_unit_index, decode_info.header.tile_info.tile_columns
                        ))
                    },
                )?,
                tile_rows: u8::try_from(decode_info.header.tile_info.tile_rows).map_err(|_| {
                    NativeVulkanError::Video(format!(
                        "AV1 TU {} tile_rows {} exceeds u8 range",
                        entry.temporal_unit_index, decode_info.header.tile_info.tile_rows
                    ))
                })?,
                context_update_tile_id: decode_info.header.tile_info.context_update_tile_id,
                tile_size_bytes_minus_1,
                mi_col_starts: decode_info.header.tile_info.mi_col_starts.clone(),
                mi_row_starts: decode_info.header.tile_info.mi_row_starts.clone(),
                width_in_sbs_minus_1: decode_info.header.tile_info.width_in_sbs_minus_1.clone(),
                height_in_sbs_minus_1: decode_info.header.tile_info.height_in_sbs_minus_1.clone(),
            },
            frame_type: decode_info.header.frame_type,
            show_existing_frame: decode_info.header.show_existing_frame,
            show_frame: decode_info.header.show_frame,
            error_resilient_mode: decode_info.header.error_resilient_mode,
            disable_cdf_update: decode_info.header.disable_cdf_update,
            disable_frame_end_update_cdf: decode_info.header.disable_frame_end_update_cdf,
            use_superres: decode_info.header.use_superres,
            render_and_frame_size_different: decode_info
                .header
                .render_and_frame_size_different
                .unwrap_or(false),
            allow_screen_content_tools: decode_info.header.allow_screen_content_tools > 0,
            is_filter_switchable: decode_info.header.is_filter_switchable,
            force_integer_mv: super::native_vulkan_av1_final_force_integer_mv(
                decode_info.header.frame_type,
                decode_info.header.force_integer_mv,
            ),
            frame_size_override_flag: decode_info.header.frame_size_override_flag,
            allow_intrabc: false,
            frame_refs_short_signaling: decode_info.header.frame_refs_short_signaling,
            allow_high_precision_mv: decode_info.header.allow_high_precision_mv,
            is_motion_mode_switchable: decode_info.header.is_motion_mode_switchable,
            use_ref_frame_mvs: decode_info.header.use_ref_frame_mvs
                && !super::native_vulkan_av1_submit_ref_frame_mvs_disabled(),
            allow_warped_motion: decode_info.header.allow_warped_motion
                && !super::native_vulkan_av1_submit_warped_motion_disabled(),
            reduced_tx_set: decode_info.header.reduced_tx_set,
            reference_select: decode_info.header.reference_select,
            skip_mode_present: decode_info.header.skip_mode_present,
            delta_q_present: decode_info.header.delta_q.present,
            delta_lf_present: decode_info.header.delta_lf.present,
            delta_lf_multi: decode_info.header.delta_lf.multi,
            apply_grain: false,
            current_frame_id: decode_info.header.current_frame_id,
            order_hint: decode_info.header.order_hint,
            primary_ref_frame: decode_info.header.primary_ref_frame,
            refresh_frame_flags: decode_info.header.refresh_frame_flags,
            interpolation_filter: decode_info.header.interpolation_filter.0 as u32,
            tx_mode_select: decode_info.header.tx_mode_select,
            delta_q_res: decode_info.header.delta_q.res,
            delta_lf_res: decode_info.header.delta_lf.res,
            skip_mode_frame,
            coded_denom: decode_info.header.coded_denom,
            picture_order_hints: super::native_vulkan_av1_picture_order_hints_for_submit(
                prepared_reference_context.reference_name_order_hints,
                order_hint_offset_enabled,
            ),
            expected_frame_ids: decode_info.header.expected_frame_ids.clone(),
            reference_name_slot_indices,
            quantization: NativeVulkanVulkanaliaAv1QuantizationPlan {
                using_qmatrix: decode_info.header.quantization.using_qmatrix,
                diff_uv_delta: decode_info.header.quantization.diff_uv_delta,
                base_q_idx: decode_info.header.quantization.base_q_idx,
                delta_q_y_dc: decode_info.header.quantization.delta_q_y_dc,
                delta_q_u_dc: decode_info.header.quantization.delta_q_u_dc,
                delta_q_u_ac: decode_info.header.quantization.delta_q_u_ac,
                delta_q_v_dc: decode_info.header.quantization.delta_q_v_dc,
                delta_q_v_ac: decode_info.header.quantization.delta_q_v_ac,
                qm_y: decode_info.header.quantization.qm_y,
                qm_u: decode_info.header.quantization.qm_u,
                qm_v: decode_info.header.quantization.qm_v,
            },
            segmentation: NativeVulkanVulkanaliaAv1SegmentationPlan {
                enabled: decode_info.header.segmentation.enabled,
                update_map: decode_info.header.segmentation.update_map,
                temporal_update: decode_info.header.segmentation.temporal_update,
                update_data: decode_info.header.segmentation.update_data,
                feature_enabled: decode_info.header.segmentation.feature_enabled,
                feature_data: decode_info.header.segmentation.feature_data,
            },
            loop_filter: NativeVulkanVulkanaliaAv1LoopFilterPlan {
                delta_enabled: decode_info.header.loop_filter.delta_enabled,
                delta_update: decode_info.header.loop_filter.delta_update,
                level: decode_info.header.loop_filter.level,
                sharpness: decode_info.header.loop_filter.sharpness,
                update_ref_delta: decode_info.header.loop_filter.update_ref_delta,
                ref_deltas: decode_info.header.loop_filter.ref_deltas,
                update_mode_delta: decode_info.header.loop_filter.update_mode_delta,
                mode_deltas: decode_info.header.loop_filter.mode_deltas,
            },
            cdef: NativeVulkanVulkanaliaAv1CdefPlan {
                damping_minus_3: decode_info.header.cdef.damping_minus_3,
                bits: decode_info.header.cdef.bits,
                y_pri_strength: decode_info.header.cdef.y_pri_strength,
                y_sec_strength: decode_info.header.cdef.y_sec_strength,
                uv_pri_strength: decode_info.header.cdef.uv_pri_strength,
                uv_sec_strength: decode_info.header.cdef.uv_sec_strength,
            },
            loop_restoration: NativeVulkanVulkanaliaAv1LoopRestorationPlan {
                frame_restoration_type: [
                    decode_info.header.loop_restoration.frame_restoration_type[0] as u32,
                    decode_info.header.loop_restoration.frame_restoration_type[1] as u32,
                    decode_info.header.loop_restoration.frame_restoration_type[2] as u32,
                ],
                loop_restoration_size: decode_info.header.loop_restoration.loop_restoration_size,
                uses_lr: decode_info.header.loop_restoration.uses_lr,
                uses_chroma_lr: decode_info.header.loop_restoration.uses_chroma_lr,
            },
            global_motion: NativeVulkanVulkanaliaAv1GlobalMotionPlan {
                gm_type: decode_info.header.global_motion.gm_type,
                gm_params: decode_info.header.global_motion.gm_params,
            },
            setup_reference: setup_reference.clone(),
            references,
        };

        native_vulkan_vulkanalia_update_av1_active_dpb_refs_after_decode(
            &mut active_dpb_refs,
            entry,
            &decode_info,
            ref_frame_sign_bias,
            prepared_reference_context.reference_name_order_hints,
            &sequence_header,
        );
        frames.push(NativeVulkanVulkanaliaAv1ReadyPrefixFrameInput {
            entry: entry.clone(),
            frame,
            pts_ms: temporal_unit.pts_ms,
            duration_ms: temporal_unit.duration_ms,
            access_unit_payload: payload.clone(),
        });
    }
    if frames.len() < frame_count as usize {
        return Err(NativeVulkanError::Video(format!(
            "AV1 reference plan produced only {} decode-submit frames but {frame_count} were requested: {}",
            frames.len(),
            extract
                .snapshot
                .av1_decode_first_unready_reason
                .as_deref()
                .unwrap_or("not enough decode-ready temporal units")
        )));
    }

    Ok(NativeVulkanVulkanaliaAv1ReadyPrefixDecodeInput {
        codec,
        sequence_header,
        requested_frame_count: frame_count,
        frames,
    })
}

#[cfg(not(feature = "native-vulkan-gst-video"))]
pub fn native_vulkan_extract_av1_ready_prefix_for_vulkanalia(
    _source: PathBuf,
    _codec: NativeVulkanVideoSessionCodec,
    _max_samples: u32,
    _frame_count: u32,
) -> Result<NativeVulkanVulkanaliaAv1ReadyPrefixDecodeInput, NativeVulkanError> {
    Err(NativeVulkanError::Video(
        "Vulkanalia AV1 ready-prefix extraction requires native-vulkan-gst-video".to_owned(),
    ))
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_vulkanalia_av1_reference_info_from_active(
    slot_index: i32,
    reference: super::NativeVulkanAv1ActiveDpbReference,
    order_hint_offset_enabled: bool,
) -> NativeVulkanVulkanaliaAv1ReferenceInfoPlan {
    NativeVulkanVulkanaliaAv1ReferenceInfoPlan {
        slot_index,
        frame_type: reference.frame_type,
        ref_frame_sign_bias: reference.ref_frame_sign_bias,
        order_hint: reference.order_hint,
        saved_order_hints: super::native_vulkan_av1_std_order_hints(
            reference.saved_order_hints,
            order_hint_offset_enabled,
        ),
        disable_frame_end_update_cdf: reference.disable_frame_end_update_cdf,
        segmentation_enabled: reference.segmentation_enabled,
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_vulkanalia_av1_reference_info_from_decode_info(
    slot_index: i32,
    decode_info: &super::NativeVulkanAv1FirstFrameDecodeInfo,
    ref_frame_sign_bias: u8,
    saved_order_hints: [u8; 8],
    order_hint_offset_enabled: bool,
) -> NativeVulkanVulkanaliaAv1ReferenceInfoPlan {
    NativeVulkanVulkanaliaAv1ReferenceInfoPlan {
        slot_index,
        frame_type: decode_info.header.frame_type,
        ref_frame_sign_bias,
        order_hint: decode_info.header.order_hint.unwrap_or(0),
        saved_order_hints: super::native_vulkan_av1_std_order_hints(
            saved_order_hints,
            order_hint_offset_enabled,
        ),
        disable_frame_end_update_cdf: decode_info.header.disable_frame_end_update_cdf,
        segmentation_enabled: decode_info.header.segmentation.enabled,
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_vulkanalia_update_av1_active_dpb_refs_after_decode(
    active_dpb_refs: &mut [Option<super::NativeVulkanAv1ActiveDpbReference>],
    entry: &super::NativeVulkanAv1DecodeReferencePlanEntrySnapshot,
    decode_info: &super::NativeVulkanAv1FirstFrameDecodeInfo,
    ref_frame_sign_bias: u8,
    reference_name_order_hints: [u8; 8],
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
) {
    let active_slots_after = entry
        .map_slot_indices_after
        .iter()
        .filter_map(|slot| u32::try_from(*slot).ok())
        .collect::<Vec<_>>();
    let current_reference = entry.output_slot.and_then(|output_slot| {
        (!entry.refreshed_reference_names.is_empty()).then_some((
            output_slot,
            super::NativeVulkanAv1ActiveDpbReference {
                frame_type: decode_info.header.frame_type,
                order_hint: decode_info.header.order_hint.unwrap_or(0),
                ref_frame_sign_bias,
                saved_order_hints: super::native_vulkan_av1_setup_saved_order_hints(
                    reference_name_order_hints,
                    decode_info.header.refresh_frame_flags,
                    decode_info.header.order_hint.unwrap_or(0),
                ),
                frame_width: decode_info
                    .header
                    .frame_width
                    .unwrap_or(sequence_header.max_frame_width),
                frame_height: decode_info
                    .header
                    .frame_height
                    .unwrap_or(sequence_header.max_frame_height),
                render_width: decode_info
                    .header
                    .render_width
                    .unwrap_or(sequence_header.max_frame_width),
                render_height: decode_info
                    .header
                    .render_height
                    .unwrap_or(sequence_header.max_frame_height),
                disable_frame_end_update_cdf: decode_info.header.disable_frame_end_update_cdf,
                segmentation_enabled: decode_info.header.segmentation.enabled,
                segmentation: decode_info.header.segmentation,
                loop_filter_ref_deltas: decode_info.header.loop_filter.ref_deltas,
                loop_filter_mode_deltas: decode_info.header.loop_filter.mode_deltas,
            },
        ))
    });
    for (slot_index, slot) in active_dpb_refs.iter_mut().enumerate() {
        let slot_index = slot_index as u32;
        if !active_slots_after.contains(&slot_index) {
            *slot = None;
            continue;
        }
        if let Some((output_slot, reference)) = current_reference
            && output_slot == slot_index
        {
            *slot = Some(reference);
        }
    }
}

pub fn native_vulkan_extract_h265_parameter_sets_for_vulkanalia(
    source: PathBuf,
    codec: NativeVulkanVideoSessionCodec,
    max_samples: u32,
) -> Result<NativeVulkanH265ParameterSetSnapshot, NativeVulkanError> {
    if !matches!(
        codec,
        NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::H265Main10
    ) {
        return Err(NativeVulkanError::Video(
            "Vulkanalia real session-parameter extraction currently supports H.265 only".to_owned(),
        ));
    }

    let mut options = NativeVulkanVideoSessionSmokeOptions {
        codec,
        extract_bitstream: true,
        bitstream_source: Some(source),
        bitstream_extract_max_samples: max_samples.max(1),
        ..NativeVulkanVideoSessionSmokeOptions::default()
    };
    options.allocate_bitstream_buffer = false;
    let extract = native_vulkan_extract_video_bitstream(&options)?;
    extract.snapshot.h265_parameter_sets.ok_or_else(|| {
        NativeVulkanError::Video(
            "Vulkanalia real H.265 session parameters require parsed VPS/SPS/PPS".to_owned(),
        )
    })
}

pub fn native_vulkan_extract_h265_ready_prefix_for_vulkanalia(
    source: PathBuf,
    codec: NativeVulkanVideoSessionCodec,
    max_samples: u32,
    frame_count: u32,
) -> Result<NativeVulkanVulkanaliaH265ReadyPrefixDecodeInput, NativeVulkanError> {
    if !matches!(
        codec,
        NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::H265Main10
    ) {
        return Err(NativeVulkanError::Video(
            "Vulkanalia ready-prefix extraction currently supports H.265 only".to_owned(),
        ));
    }
    if frame_count == 0 {
        return Err(NativeVulkanError::Video(
            "Vulkanalia H.265 ready-prefix extraction requires at least one frame".to_owned(),
        ));
    }

    let mut options = NativeVulkanVideoSessionSmokeOptions {
        codec,
        extract_bitstream: true,
        bitstream_source: Some(source),
        bitstream_extract_max_samples: max_samples.max(frame_count).max(1),
        ..NativeVulkanVideoSessionSmokeOptions::default()
    };
    options.allocate_bitstream_buffer = false;
    let extract = native_vulkan_extract_video_bitstream(&options)?;
    native_vulkan_validate_h265_ready_prefix(&extract.snapshot, frame_count)?;

    let parameter_sets = extract
        .snapshot
        .h265_parameter_sets
        .clone()
        .ok_or_else(|| {
            NativeVulkanError::Video(
                "Vulkanalia H.265 ready-prefix extraction requires parsed VPS/SPS/PPS".to_owned(),
            )
        })?;
    let entries = extract
        .snapshot
        .h265_decode_reference_plan
        .get(..frame_count as usize)
        .ok_or_else(|| {
            NativeVulkanError::Video(format!(
                "H.265 reference plan has {} frames but {frame_count} were requested",
                extract.snapshot.h265_decode_reference_plan.len()
            ))
        })?;
    let access_units = extract
        .snapshot
        .h265_access_units
        .get(..frame_count as usize)
        .ok_or_else(|| {
            NativeVulkanError::Video(format!(
                "H.265 access unit snapshot has {} frames but {frame_count} were requested",
                extract.snapshot.h265_access_units.len()
            ))
        })?;
    let payloads = extract
        .h265_access_unit_payloads
        .get(..frame_count as usize)
        .ok_or_else(|| {
            NativeVulkanError::Video(format!(
                "H.265 bitstream has {} payloads but {frame_count} ready-prefix frames were requested",
                extract.h265_access_unit_payloads.len()
            ))
        })?;

    let window_mode = native_vulkan_h265_ready_prefix_bitstream_window_mode();
    let mut frames = Vec::with_capacity(frame_count as usize);
    for ((entry, access_unit), payload) in entries.iter().zip(access_units).zip(payloads) {
        let first_slice = access_unit.first_slice.clone().ok_or_else(|| {
            NativeVulkanError::Video(format!(
                "H.265 AU {} has no parsed first slice",
                access_unit.index
            ))
        })?;
        if access_unit.first_slice_parse_error.is_some() {
            return Err(NativeVulkanError::Video(format!(
                "H.265 AU {} first slice parse failed",
                access_unit.index
            )));
        }
        let (window_payload, slice_segment_offset) =
            native_vulkan_h265_ready_prefix_bitstream_window(payload, window_mode)?;
        frames.push(NativeVulkanVulkanaliaH265ReadyPrefixFrameInput {
            entry: entry.clone(),
            first_slice,
            duration_ms: access_unit.duration_ms,
            access_unit_payload: window_payload.to_vec(),
            slice_segment_offset,
        });
    }

    Ok(NativeVulkanVulkanaliaH265ReadyPrefixDecodeInput {
        parameter_sets,
        requested_frame_count: frame_count,
        frames,
    })
}
