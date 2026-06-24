//! Temporary Vulkanalia bridge into the existing GST bitstream extractor.
//!
//! This keeps new Vulkanalia-facing extraction API out of the legacy renderer
//! file while the demux/codec layers are split properly.

use std::path::PathBuf;

use super::codec_snapshots::{
    NativeVulkanH264ParameterSetSnapshot, NativeVulkanH265ParameterSetSnapshot,
};
use super::{
    NativeVulkanError, NativeVulkanVideoSessionCodec, NativeVulkanVideoSessionSmokeOptions,
    native_vulkan_extract_video_bitstream,
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
