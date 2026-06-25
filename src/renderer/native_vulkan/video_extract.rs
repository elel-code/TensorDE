//! Encoded video extraction for Vulkanalia-facing decode setup.
//!
//! The boundary here is intentionally packet-queue shaped: GStreamer remains a
//! replaceable demux/parser frontend, while native code owns parameter sets,
//! reference plans, payload windows, and Vulkanalia decode inputs.

use std::path::Path;

use super::*;

pub(super) struct NativeVulkanVideoBitstreamExtract {
    pub(super) h264_access_unit_payloads: Vec<Vec<u8>>,
    pub(super) h265_access_unit_payloads: Vec<Vec<u8>>,
    pub(super) av1_temporal_unit_payloads: Vec<Vec<u8>>,
    pub(super) snapshot: NativeVulkanVideoBitstreamExtractSnapshot,
}

pub(super) fn native_vulkan_extract_video_bitstream(
    options: &NativeVulkanVideoSessionSmokeOptions,
) -> Result<NativeVulkanVideoBitstreamExtract, NativeVulkanError> {
    let source = options.bitstream_source.as_deref().ok_or_else(|| {
        NativeVulkanError::Video("--extract-bitstream requires --source".to_owned())
    })?;
    if !source.is_file() {
        return Err(NativeVulkanError::Video(format!(
            "bitstream source does not exist: {}",
            source.display()
        )));
    }
    match options.codec {
        NativeVulkanVideoSessionCodec::H264High8 => {
            let extract = native_vulkan_extract_h264_bitstream(
                source,
                options.bitstream_extract_max_samples.max(1),
            )?;
            native_vulkan_validate_h264_ready_prefix(
                &extract.snapshot,
                options.h264_required_ready_prefix_access_units,
            )?;
            Ok(extract)
        }
        NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::H265Main10 => {
            let extract = native_vulkan_extract_h265_bitstream(
                source,
                options.bitstream_extract_max_samples.max(1),
            )?;
            native_vulkan_validate_h265_ready_prefix(
                &extract.snapshot,
                options.h265_required_ready_prefix_access_units,
            )?;
            Ok(extract)
        }
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10 => {
            native_vulkan_extract_av1_bitstream(
                source,
                options.bitstream_extract_max_samples.max(1),
            )
        }
    }
}

fn native_vulkan_extract_h264_bitstream(
    source: &Path,
    max_samples: u32,
) -> Result<NativeVulkanVideoBitstreamExtract, NativeVulkanError> {
    let queue = native_vulkan_start_h264_streaming_packet_queue(source, max_samples as usize)?;
    native_vulkan_h264_bitstream_extract_from_queue(source, max_samples, queue)
}

fn native_vulkan_extract_h265_bitstream(
    source: &Path,
    max_samples: u32,
) -> Result<NativeVulkanVideoBitstreamExtract, NativeVulkanError> {
    let queue = native_vulkan_start_h265_streaming_packet_queue(source, max_samples as usize)?;
    native_vulkan_h265_bitstream_extract_from_queue(source, max_samples, queue)
}

fn native_vulkan_extract_av1_bitstream(
    source: &Path,
    max_samples: u32,
) -> Result<NativeVulkanVideoBitstreamExtract, NativeVulkanError> {
    let queue = native_vulkan_start_av1_streaming_packet_queue(source, max_samples as usize)?;
    native_vulkan_av1_bitstream_extract_from_queue(source, max_samples, queue)
}

pub(super) fn native_vulkan_start_h264_streaming_packet_queue(
    source: &Path,
    capacity: usize,
) -> Result<NativeVulkanH264StreamingPacketQueue, NativeVulkanError> {
    native_vulkan_start_streaming_packet_queue::<NativeVulkanH264AccessUnitExtract>(
        source, capacity,
    )
}

pub(super) fn native_vulkan_start_h265_streaming_packet_queue(
    source: &Path,
    capacity: usize,
) -> Result<NativeVulkanH265StreamingPacketQueue, NativeVulkanError> {
    native_vulkan_start_streaming_packet_queue::<NativeVulkanH265AccessUnitExtract>(
        source, capacity,
    )
}

pub(super) fn native_vulkan_start_av1_streaming_packet_queue(
    source: &Path,
    capacity: usize,
) -> Result<NativeVulkanAv1StreamingPacketQueue, NativeVulkanError> {
    native_vulkan_start_streaming_packet_queue::<NativeVulkanAv1TemporalUnitExtract>(
        source, capacity,
    )
}

fn native_vulkan_h264_bitstream_extract_from_queue(
    source: &Path,
    max_samples: u32,
    queue: NativeVulkanH264StreamingPacketQueue,
) -> Result<NativeVulkanVideoBitstreamExtract, NativeVulkanError> {
    let selected_index = queue
        .queued
        .iter()
        .position(|packet| packet.access_unit.stats.parameter_sets_present())
        .unwrap_or(0);
    let selected = queue.queued.get(selected_index).ok_or_else(|| {
        NativeVulkanError::Video("H.264 streaming queue produced no packets".to_owned())
    })?;
    let parameter_sets = queue.parameter_sets.clone();
    let h264_access_units = queue
        .queued
        .iter()
        .map(|packet| packet.snapshot.clone())
        .collect::<Vec<_>>();
    let h264_idr_decode_ready_count = h264_access_units
        .iter()
        .filter(|access_unit| access_unit.idr_decode_ready)
        .count() as u32;
    let h264_idr_decode_ready_prefix_count = h264_access_units
        .iter()
        .take_while(|access_unit| access_unit.idr_decode_ready)
        .count() as u32;
    let h264_idr_decode_first_unready = h264_access_units
        .iter()
        .find(|access_unit| !access_unit.idr_decode_ready);
    let h264_idr_decode_first_unready_access_unit_index =
        h264_idr_decode_first_unready.map(|access_unit| access_unit.index);
    let h264_idr_decode_first_unready_reason = h264_idr_decode_first_unready.map(|access_unit| {
        access_unit
            .first_slice_parse_error
            .clone()
            .or_else(|| {
                access_unit.first_slice.as_ref().map(|slice| {
                    format!(
                        "H.264 AU {} is not IDR intra-only: nal={}, slice_type={}, idr={}, intra={}",
                        access_unit.index,
                        slice.nal_type_label,
                        slice.slice_type,
                        slice.idr,
                        slice.is_intra
                    )
                })
            })
            .unwrap_or_else(|| format!("H.264 AU {} has no parsed first slice", access_unit.index))
    });
    let max_h264_dpb_slots = native_vulkan_h264_sps_dpb_slot_count(&parameter_sets.sps).max(2);
    let max_h264_references = parameter_sets.sps.max_num_ref_frames.max(1);
    let max_h264_frame_num = native_vulkan_h264_sps_max_frame_num(&parameter_sets.sps);
    let (h264_reference_plan_dpb_slots, h264_decode_reference_plan) =
        native_vulkan_h264_min_decodable_dpb_plan_with_gaps(
            &h264_access_units,
            max_h264_dpb_slots,
            max_h264_references,
            max_h264_frame_num,
            parameter_sets.sps.gaps_in_frame_num_value_allowed_flag,
        );
    let h264_decode_ready_count = h264_decode_reference_plan
        .iter()
        .filter(|entry| entry.ready_for_decode_submit)
        .count() as u32;
    let h264_decode_ready_prefix_count = h264_decode_reference_plan
        .iter()
        .take_while(|entry| entry.ready_for_decode_submit)
        .count() as u32;
    let h264_decode_first_unready = h264_decode_reference_plan
        .iter()
        .find(|entry| !entry.ready_for_decode_submit);
    let h264_decode_first_unready_access_unit_index =
        h264_decode_first_unready.map(|entry| entry.access_unit_index);
    let h264_decode_first_unready_reason = h264_decode_first_unready.map(|entry| {
        entry.unsupported_reason.clone().unwrap_or_else(|| {
            format!(
                "missing {} active H.264 reference(s)",
                entry.missing_reference_count
            )
        })
    });
    let total_bytes = queue
        .queued
        .iter()
        .map(|packet| packet.access_unit.payload.len() as u64)
        .sum();

    let snapshot = NativeVulkanVideoBitstreamExtractSnapshot {
        source: source.display().to_string(),
        frontend: "gstreamer-demux-h264parse-streaming-queue",
        requested_max_samples: max_samples,
        samples: queue.queued.len() as u32,
        total_bytes,
        selected_access_unit_index: selected_index as u32,
        selected_access_unit_bytes: selected.access_unit.stats.bytes,
        selected_access_unit_pts_ms: selected.access_unit.pts_ms,
        selected_access_unit_duration_ms: selected.access_unit.duration_ms,
        caps: selected.access_unit.caps.clone(),
        stream_format: selected.access_unit.stream_format.clone(),
        alignment: selected.access_unit.alignment.clone(),
        width: selected.access_unit.width,
        height: selected.access_unit.height,
        framerate: selected.access_unit.framerate.clone(),
        has_annex_b_start_codes: selected.access_unit.stats.has_annex_b_start_codes,
        h264_sps_count: selected.access_unit.stats.sps_count,
        h264_pps_count: selected.access_unit.stats.pps_count,
        h264_idr_count: selected.access_unit.stats.idr_count,
        h264_slice_count: selected.access_unit.stats.slice_count,
        h264_parameter_sets_present: selected.access_unit.stats.parameter_sets_present(),
        h264_parameter_sets: Some(parameter_sets),
        h264_access_units,
        h264_idr_decode_ready_count,
        h264_idr_decode_ready_prefix_count,
        h264_idr_decode_first_unready_access_unit_index,
        h264_idr_decode_first_unready_reason,
        h264_reference_plan_dpb_slots,
        h264_decode_ready_count,
        h264_decode_ready_prefix_count,
        h264_decode_first_unready_access_unit_index,
        h264_decode_first_unready_reason,
        h264_decode_reference_plan,
        h265_vps_count: 0,
        h265_sps_count: 0,
        h265_pps_count: 0,
        h265_idr_count: 0,
        h265_slice_count: 0,
        h265_parameter_sets_present: false,
        h265_parameter_sets: None,
        h265_nal_units: Vec::new(),
        h265_access_units: Vec::new(),
        h265_reference_plan_dpb_slots: 0,
        h265_decode_ready_count: 0,
        h265_decode_ready_prefix_count: 0,
        h265_decode_first_unready_access_unit_index: None,
        h265_decode_first_unready_missing_reference_pocs: Vec::new(),
        h265_decode_reference_plan: Vec::new(),
        av1_obu_count: 0,
        av1_sequence_header_count: 0,
        av1_temporal_delimiter_count: 0,
        av1_frame_header_count: 0,
        av1_tile_group_count: 0,
        av1_frame_count: 0,
        av1_decode_candidate: false,
        av1_tile_payload_bytes: 0,
        av1_frame_payload_bytes: 0,
        av1_first_frame_header_obu_offset: None,
        av1_first_tile_group_obu_offset: None,
        av1_sequence_header_present: false,
        av1_sequence_header: None,
        av1_first_frame_submit: None,
        av1_obus: Vec::new(),
        av1_temporal_units: Vec::new(),
        av1_reference_plan_dpb_slots: 0,
        av1_decode_ready_count: 0,
        av1_decode_ready_leading_count: 0,
        av1_decode_first_unready_temporal_unit_index: None,
        av1_decode_first_unready_reason: None,
        av1_decode_reference_plan: Vec::new(),
    };
    let h264_access_unit_payloads = queue
        .queued
        .into_iter()
        .map(|packet| packet.access_unit.payload.into_vec())
        .collect();

    Ok(NativeVulkanVideoBitstreamExtract {
        h264_access_unit_payloads,
        h265_access_unit_payloads: Vec::new(),
        av1_temporal_unit_payloads: Vec::new(),
        snapshot,
    })
}

fn native_vulkan_h265_bitstream_extract_from_queue(
    source: &Path,
    max_samples: u32,
    queue: NativeVulkanH265StreamingPacketQueue,
) -> Result<NativeVulkanVideoBitstreamExtract, NativeVulkanError> {
    let selected_index = queue
        .queued
        .iter()
        .position(|packet| packet.access_unit.stats.parameter_sets_present())
        .unwrap_or(0);
    let selected = queue.queued.get(selected_index).ok_or_else(|| {
        NativeVulkanError::Video("H.265 streaming queue produced no packets".to_owned())
    })?;
    let parameter_sets = queue.parameter_sets.clone();
    let h265_access_units = queue
        .queued
        .iter()
        .map(|packet| packet.snapshot.clone())
        .collect::<Vec<_>>();
    let h265_reference_plan_dpb_slots = native_vulkan_h265_sps_dpb_slot_count(&parameter_sets.sps);
    let h265_decode_reference_plan = native_vulkan_h265_decode_reference_plan(
        &h265_access_units,
        h265_reference_plan_dpb_slots,
        native_vulkan_h265_sps_max_pic_order_cnt_lsb(&parameter_sets.sps),
    );
    let h265_decode_ready_count = h265_decode_reference_plan
        .iter()
        .filter(|entry| entry.ready_for_decode_submit)
        .count() as u32;
    let h265_decode_ready_prefix_count = h265_decode_reference_plan
        .iter()
        .take_while(|entry| entry.ready_for_decode_submit)
        .count() as u32;
    let h265_decode_first_unready = h265_decode_reference_plan
        .iter()
        .find(|entry| !entry.ready_for_decode_submit);
    let h265_decode_first_unready_access_unit_index =
        h265_decode_first_unready.map(|entry| entry.access_unit_index);
    let h265_decode_first_unready_missing_reference_pocs = h265_decode_first_unready
        .map(|entry| entry.missing_reference_pocs.clone())
        .unwrap_or_default();
    let total_bytes = queue
        .queued
        .iter()
        .map(|packet| packet.access_unit.payload.len() as u64)
        .sum();

    let snapshot = NativeVulkanVideoBitstreamExtractSnapshot {
        source: source.display().to_string(),
        frontend: "gstreamer-demux-h265parse-streaming-queue",
        requested_max_samples: max_samples,
        samples: queue.queued.len() as u32,
        total_bytes,
        selected_access_unit_index: selected_index as u32,
        selected_access_unit_bytes: selected.access_unit.stats.bytes,
        selected_access_unit_pts_ms: selected.access_unit.pts_ms,
        selected_access_unit_duration_ms: selected.access_unit.duration_ms,
        caps: selected.access_unit.caps.clone(),
        stream_format: selected.access_unit.stream_format.clone(),
        alignment: selected.access_unit.alignment.clone(),
        width: selected.access_unit.width,
        height: selected.access_unit.height,
        framerate: selected.access_unit.framerate.clone(),
        has_annex_b_start_codes: selected.access_unit.stats.has_annex_b_start_codes,
        h264_sps_count: 0,
        h264_pps_count: 0,
        h264_idr_count: 0,
        h264_slice_count: 0,
        h264_parameter_sets_present: false,
        h264_parameter_sets: None,
        h264_access_units: Vec::new(),
        h264_idr_decode_ready_count: 0,
        h264_idr_decode_ready_prefix_count: 0,
        h264_idr_decode_first_unready_access_unit_index: None,
        h264_idr_decode_first_unready_reason: None,
        h264_reference_plan_dpb_slots: 0,
        h264_decode_ready_count: 0,
        h264_decode_ready_prefix_count: 0,
        h264_decode_first_unready_access_unit_index: None,
        h264_decode_first_unready_reason: None,
        h264_decode_reference_plan: Vec::new(),
        h265_vps_count: selected.access_unit.stats.vps_count,
        h265_sps_count: selected.access_unit.stats.sps_count,
        h265_pps_count: selected.access_unit.stats.pps_count,
        h265_idr_count: selected.access_unit.stats.idr_count,
        h265_slice_count: selected.access_unit.stats.slice_count,
        h265_parameter_sets_present: selected.access_unit.stats.parameter_sets_present(),
        h265_parameter_sets: Some(parameter_sets),
        h265_nal_units: selected.access_unit.stats.nal_units.clone(),
        h265_access_units,
        h265_reference_plan_dpb_slots,
        h265_decode_ready_count,
        h265_decode_ready_prefix_count,
        h265_decode_first_unready_access_unit_index,
        h265_decode_first_unready_missing_reference_pocs,
        h265_decode_reference_plan,
        av1_obu_count: 0,
        av1_sequence_header_count: 0,
        av1_temporal_delimiter_count: 0,
        av1_frame_header_count: 0,
        av1_tile_group_count: 0,
        av1_frame_count: 0,
        av1_decode_candidate: false,
        av1_tile_payload_bytes: 0,
        av1_frame_payload_bytes: 0,
        av1_first_frame_header_obu_offset: None,
        av1_first_tile_group_obu_offset: None,
        av1_sequence_header_present: false,
        av1_sequence_header: None,
        av1_first_frame_submit: None,
        av1_obus: Vec::new(),
        av1_temporal_units: Vec::new(),
        av1_reference_plan_dpb_slots: 0,
        av1_decode_ready_count: 0,
        av1_decode_ready_leading_count: 0,
        av1_decode_first_unready_temporal_unit_index: None,
        av1_decode_first_unready_reason: None,
        av1_decode_reference_plan: Vec::new(),
    };
    let h265_access_unit_payloads = queue
        .queued
        .into_iter()
        .map(|packet| packet.access_unit.payload.into_vec())
        .collect();

    Ok(NativeVulkanVideoBitstreamExtract {
        h264_access_unit_payloads: Vec::new(),
        h265_access_unit_payloads,
        av1_temporal_unit_payloads: Vec::new(),
        snapshot,
    })
}

fn native_vulkan_av1_bitstream_extract_from_queue(
    source: &Path,
    max_samples: u32,
    queue: NativeVulkanAv1StreamingPacketQueue,
) -> Result<NativeVulkanVideoBitstreamExtract, NativeVulkanError> {
    let selected_index = queue
        .queued
        .iter()
        .position(|packet| packet.access_unit.stats.sequence_header_present())
        .unwrap_or(0);
    let selected = queue.queued.get(selected_index).ok_or_else(|| {
        NativeVulkanError::Video("AV1 streaming queue produced no packets".to_owned())
    })?;
    let parameter_sets = queue.parameter_sets.clone();
    let av1_temporal_units = queue
        .queued
        .iter()
        .map(|packet| packet.snapshot.clone())
        .collect::<Vec<_>>();
    let (av1_reference_plan_dpb_slots, av1_decode_reference_plan) =
        native_vulkan_av1_min_decodable_dpb_plan(&av1_temporal_units, 16);
    let av1_decode_ready_count = av1_decode_reference_plan
        .iter()
        .filter(|entry| entry.ready_for_decode_submit || entry.ready_for_display_handoff)
        .count() as u32;
    let av1_decode_ready_leading_count = av1_decode_reference_plan
        .iter()
        .take_while(|entry| entry.ready_for_decode_submit || entry.ready_for_display_handoff)
        .count() as u32;
    let av1_decode_first_unready = av1_decode_reference_plan
        .iter()
        .find(|entry| !(entry.ready_for_decode_submit || entry.ready_for_display_handoff));
    let av1_decode_first_unready_temporal_unit_index =
        av1_decode_first_unready.map(|entry| entry.temporal_unit_index);
    let av1_decode_first_unready_reason =
        av1_decode_first_unready.and_then(|entry| entry.unsupported_reason.clone());
    let total_bytes = queue
        .queued
        .iter()
        .map(|packet| packet.access_unit.payload.len() as u64)
        .sum();

    let snapshot = NativeVulkanVideoBitstreamExtractSnapshot {
        source: source.display().to_string(),
        frontend: "gstreamer-demux-av1parse-streaming-queue",
        requested_max_samples: max_samples,
        samples: queue.queued.len() as u32,
        total_bytes,
        selected_access_unit_index: selected_index as u32,
        selected_access_unit_bytes: selected.access_unit.stats.bytes,
        selected_access_unit_pts_ms: selected.access_unit.pts_ms,
        selected_access_unit_duration_ms: selected.access_unit.duration_ms,
        caps: selected.access_unit.caps.clone(),
        stream_format: selected.access_unit.stream_format.clone(),
        alignment: selected.access_unit.alignment.clone(),
        width: selected.access_unit.width,
        height: selected.access_unit.height,
        framerate: selected.access_unit.framerate.clone(),
        has_annex_b_start_codes: false,
        h264_sps_count: 0,
        h264_pps_count: 0,
        h264_idr_count: 0,
        h264_slice_count: 0,
        h264_parameter_sets_present: false,
        h264_parameter_sets: None,
        h264_access_units: Vec::new(),
        h264_idr_decode_ready_count: 0,
        h264_idr_decode_ready_prefix_count: 0,
        h264_idr_decode_first_unready_access_unit_index: None,
        h264_idr_decode_first_unready_reason: None,
        h264_reference_plan_dpb_slots: 0,
        h264_decode_ready_count: 0,
        h264_decode_ready_prefix_count: 0,
        h264_decode_first_unready_access_unit_index: None,
        h264_decode_first_unready_reason: None,
        h264_decode_reference_plan: Vec::new(),
        h265_vps_count: 0,
        h265_sps_count: 0,
        h265_pps_count: 0,
        h265_idr_count: 0,
        h265_slice_count: 0,
        h265_parameter_sets_present: false,
        h265_parameter_sets: None,
        h265_nal_units: Vec::new(),
        h265_access_units: Vec::new(),
        h265_reference_plan_dpb_slots: 0,
        h265_decode_ready_count: 0,
        h265_decode_ready_prefix_count: 0,
        h265_decode_first_unready_access_unit_index: None,
        h265_decode_first_unready_missing_reference_pocs: Vec::new(),
        h265_decode_reference_plan: Vec::new(),
        av1_obu_count: selected.access_unit.stats.obu_count,
        av1_sequence_header_count: selected.access_unit.stats.sequence_header_count,
        av1_temporal_delimiter_count: selected.access_unit.stats.temporal_delimiter_count,
        av1_frame_header_count: selected.access_unit.stats.frame_header_count,
        av1_tile_group_count: selected.access_unit.stats.tile_group_count,
        av1_frame_count: selected.access_unit.stats.frame_count,
        av1_decode_candidate: selected.access_unit.stats.decode_candidate(),
        av1_tile_payload_bytes: selected.access_unit.stats.tile_payload_bytes,
        av1_frame_payload_bytes: selected.access_unit.stats.frame_payload_bytes,
        av1_first_frame_header_obu_offset: selected.access_unit.stats.first_frame_header_obu_offset,
        av1_first_tile_group_obu_offset: selected.access_unit.stats.first_tile_group_obu_offset,
        av1_sequence_header_present: selected.access_unit.stats.sequence_header_present(),
        av1_sequence_header: Some(parameter_sets),
        av1_first_frame_submit: selected.access_unit.stats.first_frame_submit.clone(),
        av1_obus: selected.access_unit.stats.obus.clone(),
        av1_temporal_units,
        av1_reference_plan_dpb_slots,
        av1_decode_ready_count,
        av1_decode_ready_leading_count,
        av1_decode_first_unready_temporal_unit_index,
        av1_decode_first_unready_reason,
        av1_decode_reference_plan,
    };
    let av1_temporal_unit_payloads = queue
        .queued
        .into_iter()
        .map(|packet| packet.access_unit.payload.into_vec())
        .collect();

    Ok(NativeVulkanVideoBitstreamExtract {
        h264_access_unit_payloads: Vec::new(),
        h265_access_unit_payloads: Vec::new(),
        av1_temporal_unit_payloads,
        snapshot,
    })
}

pub(super) fn native_vulkan_validate_h265_ready_prefix(
    snapshot: &NativeVulkanVideoBitstreamExtractSnapshot,
    required_ready_prefix_access_units: u32,
) -> Result<(), NativeVulkanError> {
    if required_ready_prefix_access_units == 0
        || snapshot.h265_decode_ready_prefix_count >= required_ready_prefix_access_units
    {
        return Ok(());
    }

    Err(NativeVulkanError::Video(format!(
        "H.265 decode ready prefix has {} access units but {} required; first unready AU {:?} is missing reference POCs {:?}",
        snapshot.h265_decode_ready_prefix_count,
        required_ready_prefix_access_units,
        snapshot.h265_decode_first_unready_access_unit_index,
        snapshot.h265_decode_first_unready_missing_reference_pocs,
    )))
}

pub(super) fn native_vulkan_validate_h264_ready_prefix(
    snapshot: &NativeVulkanVideoBitstreamExtractSnapshot,
    required_ready_prefix_access_units: u32,
) -> Result<(), NativeVulkanError> {
    if required_ready_prefix_access_units == 0
        || snapshot.h264_decode_ready_prefix_count >= required_ready_prefix_access_units
    {
        return Ok(());
    }

    Err(NativeVulkanError::Video(format!(
        "H.264 decode ready prefix has {} access units but {} required; first unready AU {:?}: {}",
        snapshot.h264_decode_ready_prefix_count,
        required_ready_prefix_access_units,
        snapshot.h264_decode_first_unready_access_unit_index,
        snapshot
            .h264_decode_first_unready_reason
            .as_deref()
            .unwrap_or("unknown reason"),
    )))
}

pub(super) fn native_vulkan_h265_ready_prefix_bitstream_payload(
    payload: Vec<u8>,
) -> Result<(Vec<u8>, u32), NativeVulkanError> {
    let slice_segment_offset = native_vulkan_h265_slice_segment_offset(&payload)?;
    Ok((payload, slice_segment_offset))
}

pub(super) fn native_vulkan_h265_slice_segment_offset(
    payload: &[u8],
) -> Result<u32, NativeVulkanError> {
    let first_slice = native_vulkan_h265_nal_payloads(payload)
        .into_iter()
        .find(|nal| nal.nal_type <= 31)
        .ok_or_else(|| NativeVulkanError::Video("H.265 AU has no VCL slice NAL".to_owned()))?;
    u32::try_from(first_slice.slice_segment_offset)
        .map_err(|_| NativeVulkanError::Video("H.265 slice segment offset exceeds u32".to_owned()))
}
