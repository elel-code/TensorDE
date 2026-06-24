use std::path::PathBuf;

use serde::Serialize;

use crate::core::FitMode;

use super::audio_policy::NativeVulkanAudioOutputMode;
use super::video_codec::NativeVulkanVideoSessionCodec;
use super::vulkanalia_backend::{
    NativeVulkanVulkanaliaAv1ReadyPrefixDecodeInput,
    NativeVulkanVulkanaliaH264ReadyPrefixDecodeInput,
    NativeVulkanVulkanaliaH265ReadyPrefixDecodeInput,
    NativeVulkanVulkanaliaVideoSessionBindSmokeOptions,
    NativeVulkanVulkanaliaVideoSessionBindSmokeSnapshot,
    probe_native_vulkan_vulkanalia_video_session_bind,
};
use super::vulkanalia_extract::{
    native_vulkan_extract_av1_ready_prefix_for_vulkanalia,
    native_vulkan_extract_h264_ready_prefix_for_vulkanalia,
    native_vulkan_extract_h265_ready_prefix_for_vulkanalia,
};
use super::{NativeVulkanError, NativeVulkanOptions};

#[derive(Debug, Clone, Serialize)]
pub struct NativeVulkanVulkanaliaReadyPrefixRuntimeSnapshot {
    pub route: &'static str,
    pub binding: &'static str,
    pub codec: NativeVulkanVideoSessionCodec,
    pub source: PathBuf,
    pub requested_extent: (u32, u32),
    pub fit: FitMode,
    pub ready_prefix_frame_count: u32,
    pub playback_frame_count: u32,
    pub target_max_fps: Option<u32>,
    pub audio_clock_probe_requested: bool,
    pub audio_output_mode: &'static str,
    pub decode_submit_backend: &'static str,
    pub command_submit_model: &'static str,
    pub present_backend: &'static str,
    pub ffmpeg_reference: &'static str,
    pub session: NativeVulkanVulkanaliaVideoSessionBindSmokeSnapshot,
}

#[allow(clippy::too_many_arguments)]
pub fn run_vulkanalia_ready_prefix_video(
    options: NativeVulkanOptions,
    codec: NativeVulkanVideoSessionCodec,
    source: PathBuf,
    width: u32,
    height: u32,
    fit: FitMode,
    bitstream_samples: u32,
    ready_prefix_frame_count: u32,
    playback_frame_count: u32,
    audio_clock_probe_requested: bool,
    audio_output_mode: NativeVulkanAudioOutputMode,
) -> Result<NativeVulkanVulkanaliaReadyPrefixRuntimeSnapshot, NativeVulkanError> {
    if width == 0 || height == 0 {
        return Err(NativeVulkanError::Video(
            "Vulkanalia ready-prefix run requires a non-zero source extent".to_owned(),
        ));
    }
    if ready_prefix_frame_count == 0 {
        return Err(NativeVulkanError::Video(
            "Vulkanalia ready-prefix run requires at least one ready-prefix frame".to_owned(),
        ));
    }
    if playback_frame_count == 0 {
        return Err(NativeVulkanError::Video(
            "Vulkanalia ready-prefix run requires at least one playback frame".to_owned(),
        ));
    }

    let ready_prefix = native_vulkan_extract_ready_prefix_for_vulkanalia(
        source.clone(),
        codec,
        bitstream_samples,
        ready_prefix_frame_count,
    )?;
    let session_options =
        ready_prefix.into_session_options(codec, width, height, bitstream_samples);
    let session = probe_native_vulkan_vulkanalia_video_session_bind(session_options)
        .map_err(NativeVulkanError::Video)?;

    Ok(NativeVulkanVulkanaliaReadyPrefixRuntimeSnapshot {
        route: "direct-video-ready-prefix",
        binding: "vulkanalia",
        codec,
        source,
        requested_extent: (width, height),
        fit,
        ready_prefix_frame_count,
        playback_frame_count,
        target_max_fps: options.target_max_fps,
        audio_clock_probe_requested,
        audio_output_mode: audio_output_mode.as_str(),
        decode_submit_backend: "vulkanalia-video-session-bind",
        command_submit_model: "CmdPipelineBarrier2 -> CmdBeginVideoCodingKHR -> CmdDecodeVideoKHR -> CmdEndVideoCodingKHR -> QueueSubmit2",
        present_backend: "ash-visible-runtime-pending-vulkanalia-present-migration",
        ffmpeg_reference: "references/ffmpeg/libavcodec/vulkan_decode.c",
        session,
    })
}

enum NativeVulkanVulkanaliaReadyPrefixInput {
    H264(NativeVulkanVulkanaliaH264ReadyPrefixDecodeInput),
    H265(NativeVulkanVulkanaliaH265ReadyPrefixDecodeInput),
    Av1(NativeVulkanVulkanaliaAv1ReadyPrefixDecodeInput),
}

impl NativeVulkanVulkanaliaReadyPrefixInput {
    fn into_session_options(
        self,
        codec: NativeVulkanVideoSessionCodec,
        width: u32,
        height: u32,
        bitstream_samples: u32,
    ) -> NativeVulkanVulkanaliaVideoSessionBindSmokeOptions {
        let bitstream_buffer_size = u64::from(bitstream_samples.max(1)) * 1024 * 1024;
        match self {
            Self::H264(ready_prefix) => NativeVulkanVulkanaliaVideoSessionBindSmokeOptions {
                codec,
                width,
                height,
                allocate_video_images: true,
                allocate_bitstream_buffer: true,
                bitstream_buffer_size,
                create_empty_session_parameters: false,
                create_session_parameters: true,
                h264_parameter_sets: Some(ready_prefix.parameter_sets.clone()),
                h265_parameter_sets: None,
                av1_sequence_header: None,
                h264_ready_prefix_decode: Some(ready_prefix),
                h265_ready_prefix_decode: None,
                av1_ready_prefix_decode: None,
            },
            Self::H265(ready_prefix) => NativeVulkanVulkanaliaVideoSessionBindSmokeOptions {
                codec,
                width,
                height,
                allocate_video_images: true,
                allocate_bitstream_buffer: true,
                bitstream_buffer_size,
                create_empty_session_parameters: false,
                create_session_parameters: true,
                h264_parameter_sets: None,
                h265_parameter_sets: Some(ready_prefix.parameter_sets.clone()),
                av1_sequence_header: None,
                h264_ready_prefix_decode: None,
                h265_ready_prefix_decode: Some(ready_prefix),
                av1_ready_prefix_decode: None,
            },
            Self::Av1(ready_prefix) => NativeVulkanVulkanaliaVideoSessionBindSmokeOptions {
                codec,
                width,
                height,
                allocate_video_images: true,
                allocate_bitstream_buffer: true,
                bitstream_buffer_size,
                create_empty_session_parameters: false,
                create_session_parameters: true,
                h264_parameter_sets: None,
                h265_parameter_sets: None,
                av1_sequence_header: Some(ready_prefix.sequence_header.clone()),
                h264_ready_prefix_decode: None,
                h265_ready_prefix_decode: None,
                av1_ready_prefix_decode: Some(ready_prefix),
            },
        }
    }
}

fn native_vulkan_extract_ready_prefix_for_vulkanalia(
    source: PathBuf,
    codec: NativeVulkanVideoSessionCodec,
    bitstream_samples: u32,
    ready_prefix_frame_count: u32,
) -> Result<NativeVulkanVulkanaliaReadyPrefixInput, NativeVulkanError> {
    match codec {
        NativeVulkanVideoSessionCodec::H264High8 => {
            native_vulkan_extract_h264_ready_prefix_for_vulkanalia(
                source,
                bitstream_samples,
                ready_prefix_frame_count,
            )
            .map(NativeVulkanVulkanaliaReadyPrefixInput::H264)
        }
        NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::H265Main10 => {
            native_vulkan_extract_h265_ready_prefix_for_vulkanalia(
                source,
                codec,
                bitstream_samples,
                ready_prefix_frame_count,
            )
            .map(NativeVulkanVulkanaliaReadyPrefixInput::H265)
        }
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10 => {
            native_vulkan_extract_av1_ready_prefix_for_vulkanalia(
                source,
                codec,
                bitstream_samples,
                ready_prefix_frame_count,
            )
            .map(NativeVulkanVulkanaliaReadyPrefixInput::Av1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_snapshot_names_vulkanalia_submit_and_pending_present_boundary() {
        let snapshot_type =
            std::any::type_name::<NativeVulkanVulkanaliaReadyPrefixRuntimeSnapshot>();

        assert!(snapshot_type.contains("VulkanaliaReadyPrefixRuntimeSnapshot"));
    }
}
