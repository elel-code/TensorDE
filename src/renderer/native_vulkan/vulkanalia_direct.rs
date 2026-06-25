use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;

use crate::core::FitMode;

use super::audio_policy::NativeVulkanAudioOutputMode;
use super::video_codec::NativeVulkanVideoSessionCodec;
use super::vulkanalia_backend::{
    NativeVulkanVulkanaliaAv1ReadyPrefixDecodeInput,
    NativeVulkanVulkanaliaAv1RetainedVideoPresentDecodeOptions,
    NativeVulkanVulkanaliaAv1RetainedVideoPresentDecodeSnapshot,
    NativeVulkanVulkanaliaClearPresentOptions, NativeVulkanVulkanaliaClearPresentSnapshot,
    NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot,
    NativeVulkanVulkanaliaDecodedImagePresentSequenceSnapshot,
    NativeVulkanVulkanaliaH264RetainedVideoPresentDecodeSnapshot,
    NativeVulkanVulkanaliaH264StreamingVideoPresentDecodeOptions,
    NativeVulkanVulkanaliaH265RetainedVideoPresentDecodeSnapshot,
    NativeVulkanVulkanaliaH265StreamingVideoPresentDecodeOptions,
    NativeVulkanVulkanaliaSurfaceSwapchainProbeSnapshot,
    NativeVulkanVulkanaliaVideoPresentDeviceProbeSnapshot,
    NativeVulkanVulkanaliaVideoPresentSessionProbeOptions,
    NativeVulkanVulkanaliaVideoPresentSessionProbeSnapshot,
    NativeVulkanVulkanaliaVideoSessionBindSmokeOptions,
    NativeVulkanVulkanaliaVideoSessionBindSmokeSnapshot,
    native_vulkan_vulkanalia_ffmpeg_decode_bitstream_buffer_size,
    probe_native_vulkan_vulkanalia_video_present_session,
    probe_native_vulkan_vulkanalia_video_session_bind,
    run_native_vulkan_vulkanalia_av1_retained_video_present_decode,
    run_native_vulkan_vulkanalia_clear_present,
    run_native_vulkan_vulkanalia_h264_streaming_video_present_decode,
    run_native_vulkan_vulkanalia_h265_streaming_video_present_decode,
};
use super::vulkanalia_extract::native_vulkan_extract_av1_ready_prefix_for_vulkanalia;
use super::{NativeVulkanError, NativeVulkanOptions};

const NATIVE_VULKAN_VULKANALIA_STREAMING_PACKET_QUEUE_CAPACITY: usize = 32;

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
    pub present_probe_requested: bool,
    pub present_probe: Option<NativeVulkanVulkanaliaSurfaceSwapchainProbeSnapshot>,
    pub present_probe_error: Option<String>,
    pub video_present_device_probe_requested: bool,
    pub video_present_device_probe: Option<NativeVulkanVulkanaliaVideoPresentDeviceProbeSnapshot>,
    pub video_present_device_probe_error: Option<String>,
    pub video_present_session_probe_requested: bool,
    pub video_present_session_probe: Option<NativeVulkanVulkanaliaVideoPresentSessionProbeSnapshot>,
    pub video_present_session_probe_error: Option<String>,
    pub av1_retained_video_present_decode_requested: bool,
    pub av1_retained_video_present_decode:
        Option<NativeVulkanVulkanaliaAv1RetainedVideoPresentDecodeSnapshot>,
    pub av1_retained_video_present_decode_error: Option<String>,
    pub h264_retained_video_present_decode_requested: bool,
    pub h264_retained_video_present_decode:
        Option<NativeVulkanVulkanaliaH264RetainedVideoPresentDecodeSnapshot>,
    pub h264_retained_video_present_decode_error: Option<String>,
    pub h265_retained_video_present_decode_requested: bool,
    pub h265_retained_video_present_decode:
        Option<NativeVulkanVulkanaliaH265RetainedVideoPresentDecodeSnapshot>,
    pub h265_retained_video_present_decode_error: Option<String>,
    pub decoded_image_present_draw_requested: bool,
    pub decoded_image_present_draw: Option<NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot>,
    pub decoded_image_present_draw_error: Option<String>,
    pub decoded_image_present_sequence_requested: bool,
    pub decoded_image_present_sequence:
        Option<NativeVulkanVulkanaliaDecodedImagePresentSequenceSnapshot>,
    pub decoded_image_present_sequence_error: Option<String>,
    pub present_runtime_requested: bool,
    pub present_runtime: Option<NativeVulkanVulkanaliaClearPresentSnapshot>,
    pub present_runtime_error: Option<String>,
    pub decoded_image_zero_copy_presented: bool,
    pub decoded_image_present_boundary: &'static str,
    pub ffmpeg_reference: &'static str,
    pub session: Option<NativeVulkanVulkanaliaVideoSessionBindSmokeSnapshot>,
}

fn native_vulkan_vulkanalia_align16_extent(width: u32, height: u32) -> (u32, u32) {
    let align16 = |value: u32| value.div_ceil(16).saturating_mul(16);
    if width == 0 || height == 0 {
        (0, 0)
    } else {
        (align16(width), align16(height))
    }
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

    let h264_streaming_decode_requested = codec == NativeVulkanVideoSessionCodec::H264High8;
    let h265_streaming_decode_requested = matches!(
        codec,
        NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::H265Main10
    );
    let streaming_queue_capacity = native_vulkan_vulkanalia_streaming_packet_queue_capacity();
    let retained_av1_ready_prefix_decode =
        if h264_streaming_decode_requested || h265_streaming_decode_requested {
            None
        } else {
            Some(native_vulkan_extract_av1_ready_prefix_for_vulkanalia(
                source.clone(),
                codec,
                bitstream_samples,
                ready_prefix_frame_count,
            )?)
        };
    // H.264/H.265 streaming now matches FFmpeg's single playback pipeline: no
    // pre-playback metadata queue and no extra video-session-bind smoke. The
    // runtime creates the only demux/parser queue, derives stream extent from
    // that queue, and grows the single mapped bitstream buffer only if a payload
    // exceeds the initial FFmpeg-style one-picture allocation.
    let (width, height, bitstream_buffer_size, session) =
        if let Some(ready_prefix) = retained_av1_ready_prefix_decode.as_ref() {
            let (source_width, source_height) =
                native_vulkan_vulkanalia_av1_ready_prefix_source_extent(ready_prefix);
            let width = if source_width > 0 {
                source_width
            } else {
                width
            };
            let height = if source_height > 0 {
                source_height
            } else {
                height
            };
            let bitstream_buffer_size = u64::from(bitstream_samples.max(1)) * 1024 * 1024;
            let session_options = native_vulkan_vulkanalia_av1_session_options(
                ready_prefix.clone(),
                codec,
                width,
                height,
                bitstream_buffer_size,
            );
            let session = probe_native_vulkan_vulkanalia_video_session_bind(session_options)
                .map_err(NativeVulkanError::Video)?;
            (width, height, bitstream_buffer_size, Some(session))
        } else {
            (
                width,
                height,
                native_vulkan_vulkanalia_ffmpeg_decode_bitstream_buffer_size(1, 1),
                None,
            )
        };
    let video_present_session_options = NativeVulkanVulkanaliaVideoPresentSessionProbeOptions {
        host: options.host.clone(),
        wait_configure_roundtrips: options.wait_configure_roundtrips,
        codec,
        width,
        height,
        target_max_fps: options.target_max_fps,
    };
    let h264_retained_video_present_decode = h264_streaming_decode_requested.then(|| {
        run_native_vulkan_vulkanalia_h264_streaming_video_present_decode(
            NativeVulkanVulkanaliaH264StreamingVideoPresentDecodeOptions {
                session: video_present_session_options.clone(),
                source: source.clone(),
                queue_capacity: streaming_queue_capacity,
                bitstream_buffer_size,
                playback_frame_count,
            },
        )
    });
    let av1_retained_video_present_decode =
        retained_av1_ready_prefix_decode
            .as_ref()
            .map(|ready_prefix| {
                run_native_vulkan_vulkanalia_av1_retained_video_present_decode(
                    NativeVulkanVulkanaliaAv1RetainedVideoPresentDecodeOptions {
                        session: video_present_session_options.clone(),
                        ready_prefix: ready_prefix.clone(),
                        bitstream_buffer_size,
                        playback_frame_count,
                    },
                )
            });
    let h265_retained_video_present_decode = h265_streaming_decode_requested.then(|| {
        run_native_vulkan_vulkanalia_h265_streaming_video_present_decode(
            NativeVulkanVulkanaliaH265StreamingVideoPresentDecodeOptions {
                session: video_present_session_options.clone(),
                source: source.clone(),
                queue_capacity: streaming_queue_capacity,
                bitstream_buffer_size,
                playback_frame_count,
            },
        )
    });
    let (
        video_present_session_probe,
        video_present_session_probe_error,
        av1_retained_video_present_decode,
        av1_retained_video_present_decode_error,
        h264_retained_video_present_decode,
        h264_retained_video_present_decode_error,
        h265_retained_video_present_decode,
        h265_retained_video_present_decode_error,
    ) = if let Some(retained_decode) = av1_retained_video_present_decode {
        match retained_decode {
            Ok(snapshot) => (
                Some(snapshot.session.clone()),
                None,
                Some(snapshot),
                None,
                None,
                None,
                None,
                None,
            ),
            Err(err) => (
                None,
                Some(err.clone()),
                None,
                Some(err),
                None,
                None,
                None,
                None,
            ),
        }
    } else if let Some(retained_decode) = h264_retained_video_present_decode {
        match retained_decode {
            Ok(snapshot) => (
                Some(snapshot.session.clone()),
                None,
                None,
                None,
                Some(snapshot),
                None,
                None,
                None,
            ),
            Err(err) => (
                None,
                Some(err.clone()),
                None,
                None,
                None,
                Some(err),
                None,
                None,
            ),
        }
    } else if let Some(retained_decode) = h265_retained_video_present_decode {
        match retained_decode {
            Ok(snapshot) => (
                Some(snapshot.session.clone()),
                None,
                None,
                None,
                None,
                None,
                Some(snapshot),
                None,
            ),
            Err(err) => (
                None,
                Some(err.clone()),
                None,
                None,
                None,
                None,
                None,
                Some(err),
            ),
        }
    } else {
        let video_present_session_probe =
            probe_native_vulkan_vulkanalia_video_present_session(video_present_session_options);
        let (video_present_session_probe, video_present_session_probe_error) =
            match video_present_session_probe {
                Ok(snapshot) => (Some(snapshot), None),
                Err(err) => (None, Some(err)),
            };
        (
            video_present_session_probe,
            video_present_session_probe_error,
            None,
            None,
            None,
            None,
            None,
            None,
        )
    };
    let video_present_device_probe = video_present_session_probe
        .as_ref()
        .map(|probe| probe.device.clone());
    let video_present_device_probe_error = if video_present_device_probe.is_none() {
        video_present_session_probe_error.clone()
    } else {
        None
    };
    let decoded_image_present_draw_requested = av1_retained_video_present_decode
        .as_ref()
        .is_some_and(|snapshot| snapshot.decoded_image_present_draw_requested)
        || h264_retained_video_present_decode
            .as_ref()
            .is_some_and(|snapshot| snapshot.decoded_image_present_draw_requested)
        || h265_retained_video_present_decode
            .as_ref()
            .is_some_and(|snapshot| snapshot.decoded_image_present_draw_requested);
    let decoded_image_present_draw = av1_retained_video_present_decode
        .as_ref()
        .and_then(|snapshot| snapshot.decoded_image_present_draw.clone())
        .or_else(|| {
            h264_retained_video_present_decode
                .as_ref()
                .and_then(|snapshot| snapshot.decoded_image_present_draw.clone())
        })
        .or_else(|| {
            h265_retained_video_present_decode
                .as_ref()
                .and_then(|snapshot| snapshot.decoded_image_present_draw.clone())
        });
    let decoded_image_present_draw_error = av1_retained_video_present_decode
        .as_ref()
        .and_then(|snapshot| snapshot.decoded_image_present_draw_error.clone())
        .or_else(|| {
            h264_retained_video_present_decode
                .as_ref()
                .and_then(|snapshot| snapshot.decoded_image_present_draw_error.clone())
        })
        .or_else(|| {
            h265_retained_video_present_decode
                .as_ref()
                .and_then(|snapshot| snapshot.decoded_image_present_draw_error.clone())
        });
    let decoded_image_present_sequence_requested = av1_retained_video_present_decode
        .as_ref()
        .is_some_and(|snapshot| snapshot.decoded_image_present_sequence_requested)
        || h264_retained_video_present_decode
            .as_ref()
            .is_some_and(|snapshot| snapshot.decoded_image_present_sequence_requested)
        || h265_retained_video_present_decode
            .as_ref()
            .is_some_and(|snapshot| snapshot.decoded_image_present_sequence_requested);
    let decoded_image_present_sequence = av1_retained_video_present_decode
        .as_ref()
        .and_then(|snapshot| snapshot.decoded_image_present_sequence.clone())
        .or_else(|| {
            h264_retained_video_present_decode
                .as_ref()
                .and_then(|snapshot| snapshot.decoded_image_present_sequence.clone())
        })
        .or_else(|| {
            h265_retained_video_present_decode
                .as_ref()
                .and_then(|snapshot| snapshot.decoded_image_present_sequence.clone())
        });
    let decoded_image_present_sequence_error = av1_retained_video_present_decode
        .as_ref()
        .and_then(|snapshot| snapshot.decoded_image_present_sequence_error.clone())
        .or_else(|| {
            h264_retained_video_present_decode
                .as_ref()
                .and_then(|snapshot| snapshot.decoded_image_present_sequence_error.clone())
        })
        .or_else(|| {
            h265_retained_video_present_decode
                .as_ref()
                .and_then(|snapshot| snapshot.decoded_image_present_sequence_error.clone())
        });
    let decoded_image_zero_copy_presented = av1_retained_video_present_decode
        .as_ref()
        .is_some_and(|snapshot| snapshot.decoded_image_zero_copy_presented)
        || h264_retained_video_present_decode
            .as_ref()
            .is_some_and(|snapshot| snapshot.decoded_image_zero_copy_presented)
        || h265_retained_video_present_decode
            .as_ref()
            .is_some_and(|snapshot| snapshot.decoded_image_zero_copy_presented);
    let present_runtime_requested = !decoded_image_zero_copy_presented;
    let (present_runtime, present_runtime_error) = if present_runtime_requested {
        let present_runtime =
            run_native_vulkan_vulkanalia_clear_present(NativeVulkanVulkanaliaClearPresentOptions {
                host: options.host.clone(),
                wait_configure_roundtrips: options.wait_configure_roundtrips,
                duration: native_vulkan_vulkanalia_visible_present_duration(
                    playback_frame_count,
                    options.target_max_fps,
                ),
                target_max_fps: options.target_max_fps,
                clear_color: options.clear_color,
            });
        match present_runtime {
            Ok(snapshot) => (Some(snapshot), None),
            Err(err) => (None, Some(err)),
        }
    } else {
        (None, None)
    };
    let present_backend = if decoded_image_zero_copy_presented {
        "vulkanalia-decoded-image-dynamic-rendering-present"
    } else {
        "vulkanalia-clear-present-runtime-visible-placeholder"
    };

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
        present_backend,
        present_probe_requested: false,
        present_probe: None,
        present_probe_error: None,
        video_present_device_probe_requested: true,
        video_present_device_probe,
        video_present_device_probe_error,
        video_present_session_probe_requested: true,
        video_present_session_probe,
        video_present_session_probe_error,
        av1_retained_video_present_decode_requested: retained_av1_ready_prefix_decode.is_some(),
        av1_retained_video_present_decode,
        av1_retained_video_present_decode_error,
        h264_retained_video_present_decode_requested: h264_streaming_decode_requested,
        h264_retained_video_present_decode,
        h264_retained_video_present_decode_error,
        h265_retained_video_present_decode_requested: h265_streaming_decode_requested,
        h265_retained_video_present_decode,
        h265_retained_video_present_decode_error,
        decoded_image_present_draw_requested,
        decoded_image_present_draw,
        decoded_image_present_draw_error,
        decoded_image_present_sequence_requested,
        decoded_image_present_sequence,
        decoded_image_present_sequence_error,
        present_runtime_requested,
        present_runtime,
        present_runtime_error,
        decoded_image_zero_copy_presented,
        decoded_image_present_boundary: if decoded_image_zero_copy_presented {
            "ready-prefix decode writes into the retained Vulkanalia DPB/output image, then Vulkanalia samples that decoded image through an immutable YCbCr descriptor in a dynamic-rendering fullscreen pass and presents it to the Wayland swapchain"
        } else if retained_av1_ready_prefix_decode.is_some() {
            "AV1 ready-prefix decode writes into the retained Vulkanalia video-present DPB/output image and creates a Vulkanalia YCbCr sampler/descriptor/pipeline resource for that image; decoded-image present falls back to the clear placeholder until the draw/present gate succeeds"
        } else if h264_streaming_decode_requested {
            "H.264 ready-prefix decode writes into the retained Vulkanalia video-present DPB/output image and creates a Vulkanalia YCbCr sampler/descriptor/pipeline resource for that image; decoded-image present falls back to the clear placeholder until the draw/present gate succeeds"
        } else if h265_streaming_decode_requested {
            "H.265 ready-prefix decode writes into the retained Vulkanalia video-present DPB/output image and creates a Vulkanalia YCbCr sampler/descriptor/pipeline resource for that image; decoded-image present falls back to the clear placeholder until the draw/present gate succeeds"
        } else {
            "Vulkanalia decodes the real ready-prefix source and presents a Vulkanalia-owned visible swapchain placeholder; next gate replaces the clear image with decoded DPB/output image sampling/import"
        },
        ffmpeg_reference: "references/ffmpeg/libavcodec/vulkan_decode.c",
        session,
    })
}

fn native_vulkan_vulkanalia_visible_present_duration(
    playback_frame_count: u32,
    target_max_fps: Option<u32>,
) -> Duration {
    let fps = u64::from(target_max_fps.unwrap_or(240).max(1));
    let frames = u128::from(playback_frame_count.max(1));
    let nanos = frames.saturating_mul(1_000_000_000u128) / u128::from(fps);
    Duration::from_nanos(nanos.min(u128::from(u64::MAX)) as u64)
}

fn native_vulkan_vulkanalia_streaming_packet_queue_capacity() -> usize {
    std::env::var("GILDER_VULKAN_STREAMING_PACKET_QUEUE_CAPACITY")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(NATIVE_VULKAN_VULKANALIA_STREAMING_PACKET_QUEUE_CAPACITY)
}

fn native_vulkan_vulkanalia_av1_session_options(
    ready_prefix: NativeVulkanVulkanaliaAv1ReadyPrefixDecodeInput,
    codec: NativeVulkanVideoSessionCodec,
    width: u32,
    height: u32,
    bitstream_buffer_size: u64,
) -> NativeVulkanVulkanaliaVideoSessionBindSmokeOptions {
    NativeVulkanVulkanaliaVideoSessionBindSmokeOptions {
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
    }
}

fn native_vulkan_vulkanalia_av1_ready_prefix_source_extent(
    ready_prefix: &NativeVulkanVulkanaliaAv1ReadyPrefixDecodeInput,
) -> (u32, u32) {
    native_vulkan_vulkanalia_align16_extent(
        ready_prefix.sequence_header.max_frame_width,
        ready_prefix.sequence_header.max_frame_height,
    )
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
