//! Temporary Vulkanalia bridge into the existing GST bitstream extractor.
//!
//! This keeps new Vulkanalia-facing extraction API out of the legacy renderer
//! file while the demux/codec layers are split properly.

use std::path::PathBuf;

use super::codec_snapshots::{
    NativeVulkanAv1SequenceHeaderSnapshot, NativeVulkanH264ParameterSetSnapshot,
    NativeVulkanH265ParameterSetSnapshot,
};
use super::vulkanalia_backend::{
    NativeVulkanVulkanaliaH265ReadyPrefixDecodeInput,
    NativeVulkanVulkanaliaH265ReadyPrefixFrameInput,
};
use super::{
    NativeVulkanError, NativeVulkanVideoSessionCodec, NativeVulkanVideoSessionSmokeOptions,
    native_vulkan_extract_video_bitstream, native_vulkan_h265_ready_prefix_bitstream_window,
    native_vulkan_h265_ready_prefix_bitstream_window_mode,
    native_vulkan_validate_h265_ready_prefix,
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
