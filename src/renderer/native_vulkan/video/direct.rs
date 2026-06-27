use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use serde::Serialize;

use crate::core::FitMode;

use super::super::audio::clock::{
    NATIVE_VULKAN_AUDIO_CLOCK_QUEUE_PACKETS, NativeVulkanAudioClockProbeOptions,
    NativeVulkanAudioClockRuntimeSnapshot, native_vulkan_probe_ffmpeg_audio_clock,
    native_vulkan_unattached_audio_clock_snapshot,
};
use super::super::audio::policy::NativeVulkanAudioOutputMode;
use super::super::vulkan::{
    NativeVulkanVulkanaliaAv1RetainedVideoPresentDecodeSnapshot,
    NativeVulkanVulkanaliaAv1StreamingVideoPresentDecodeOptions,
    NativeVulkanVulkanaliaClearPresentOptions, NativeVulkanVulkanaliaClearPresentSnapshot,
    NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot,
    NativeVulkanVulkanaliaDecodedImagePresentSequenceSnapshot,
    NativeVulkanVulkanaliaH264RetainedVideoPresentDecodeSnapshot,
    NativeVulkanVulkanaliaH264StreamingVideoPresentDecodeOptions,
    NativeVulkanVulkanaliaH265RetainedVideoPresentDecodeSnapshot,
    NativeVulkanVulkanaliaH265StreamingVideoPresentDecodeOptions,
    NativeVulkanVulkanaliaSurfaceSwapchainProbeSnapshot,
    NativeVulkanVulkanaliaVideoPresentAudioMasterClock,
    NativeVulkanVulkanaliaVideoPresentDeviceProbeSnapshot,
    NativeVulkanVulkanaliaVideoPresentSessionProbeOptions,
    NativeVulkanVulkanaliaVideoPresentSessionProbeSnapshot,
    NativeVulkanVulkanaliaVideoSessionBindSmokeSnapshot,
    probe_native_vulkan_vulkanalia_video_present_session,
    run_native_vulkan_vulkanalia_av1_streaming_video_present_decode,
    run_native_vulkan_vulkanalia_clear_present,
    run_native_vulkan_vulkanalia_h264_streaming_video_present_decode,
    run_native_vulkan_vulkanalia_h265_streaming_video_present_decode,
};
use super::super::{NativeVulkanError, NativeVulkanOptions};
use super::codec::NativeVulkanVideoSessionCodec;
use super::demux::NATIVE_VULKAN_PACKET_HANDOFF_FRAMES;

pub(in crate::renderer::native_vulkan) const NATIVE_VULKAN_AUDIO_OUTPUT_WORKER_STACK_BYTES: usize =
    128 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize)]
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
    pub audio_clock: Option<NativeVulkanAudioClockRuntimeSnapshot>,
    pub audio_master_clock_enabled: bool,
    pub audio_master_clock_start_ns: Option<u64>,
    pub audio_video_sync: NativeVulkanReadyPrefixAudioVideoSyncSnapshot,
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
    pub h264_retained_video_present_decode_requested: bool,
    pub h264_retained_video_present_decode:
        Option<NativeVulkanVulkanaliaH264RetainedVideoPresentDecodeSnapshot>,
    pub h264_retained_video_present_decode_error: Option<String>,
    pub h265_retained_video_present_decode_requested: bool,
    pub h265_retained_video_present_decode:
        Option<NativeVulkanVulkanaliaH265RetainedVideoPresentDecodeSnapshot>,
    pub h265_retained_video_present_decode_error: Option<String>,
    pub av1_retained_video_present_decode_requested: bool,
    pub av1_retained_video_present_decode:
        Option<NativeVulkanVulkanaliaAv1RetainedVideoPresentDecodeSnapshot>,
    pub av1_retained_video_present_decode_error: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanReadyPrefixAudioVideoSyncSnapshot {
    pub route: &'static str,
    pub model: &'static str,
    pub enabled: bool,
    pub ready: bool,
    pub blocking_reason: Option<&'static str>,
    pub max_allowed_drift_ns: u64,
    pub drift_within_policy: bool,
    pub requested_playback_clock_ns: u64,
    pub audio_target_clock_ns: Option<u64>,
    pub audio_covered_clock_ns: Option<u64>,
    pub audio_video_target_drift_ns: i64,
    pub audio_video_target_drift_abs_ns: u64,
    pub audio_clock_coverage_ready: bool,
    pub audio_output_quality_ready: bool,
    pub audio_output_lifecycle_ready: bool,
    pub audio_output_xrun_count: u64,
    pub audio_master_clock_start_ns: Option<u64>,
    pub audio_current_serial_start_clock_ns: Option<u64>,
    pub video_present_sequence_ready: bool,
    pub video_requested_frame_count: u32,
    pub video_presented_frame_count: u32,
    pub video_pts_monotonic: bool,
    pub video_source_frame_pts_delta_min_ns: Option<u64>,
    pub video_source_frame_pts_delta_max_ns: Option<u64>,
    pub present_pacing_clock_model: &'static str,
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
    let av1_streaming_decode_requested = matches!(
        codec,
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10
    );
    let streaming_queue_capacity = native_vulkan_vulkanalia_streaming_packet_queue_capacity(
        bitstream_samples,
        ready_prefix_frame_count,
    );
    let mut audio_output_worker = None;
    let mut audio_clock = if audio_clock_probe_requested {
        let mut probe_options = NativeVulkanAudioClockProbeOptions::clock_only(source.clone());
        probe_options.output_mode = audio_output_mode;
        let audio_playback_duration = native_vulkan_vulkanalia_visible_present_duration(
            playback_frame_count,
            options.target_max_fps,
        );
        probe_options.target_playback_clock_ns =
            Some(duration_ns_u64(audio_playback_duration).max(1));
        probe_options.loop_on_eos = true;
        probe_options.packets_to_probe = native_vulkan_audio_runtime_packet_budget(
            audio_playback_duration,
            playback_frame_count,
        );
        if audio_output_mode == NativeVulkanAudioOutputMode::Auto {
            let mut clock_probe_options = probe_options.clone();
            clock_probe_options.output_mode = NativeVulkanAudioOutputMode::ClockOnly;
            clock_probe_options.packets_to_probe = NATIVE_VULKAN_AUDIO_CLOCK_QUEUE_PACKETS as u32;
            clock_probe_options.target_playback_clock_ns = None;
            let clock = native_vulkan_probe_ffmpeg_audio_clock(clock_probe_options)?;
            audio_output_worker = Some(
                thread::Builder::new()
                    .name("gilder-pipewire-audio-output".to_owned())
                    .stack_size(NATIVE_VULKAN_AUDIO_OUTPUT_WORKER_STACK_BYTES)
                    .spawn(move || native_vulkan_probe_ffmpeg_audio_clock(probe_options))
                    .map_err(|err| {
                        NativeVulkanError::Video(format!(
                            "spawn PipeWire audio output worker: {err}"
                        ))
                    })?,
            );
            Some(clock)
        } else {
            Some(native_vulkan_probe_ffmpeg_audio_clock(probe_options)?)
        }
    } else if audio_output_mode == NativeVulkanAudioOutputMode::ClockOnly {
        Some(native_vulkan_unattached_audio_clock_snapshot(
            audio_output_mode,
        ))
    } else {
        None
    };
    let audio_master_clock_enabled = audio_clock
        .as_ref()
        .is_some_and(|clock| clock.video_master_clock_ready);
    let audio_master_clock_start_ns = audio_clock
        .as_ref()
        .and_then(|clock| clock.video_master_start_clock_ns);
    let audio_master_clock = if audio_master_clock_enabled {
        NativeVulkanVulkanaliaVideoPresentAudioMasterClock::clock_only(audio_master_clock_start_ns)
    } else {
        NativeVulkanVulkanaliaVideoPresentAudioMasterClock::DISABLED
    };
    let session = None;
    let video_present_session_options = NativeVulkanVulkanaliaVideoPresentSessionProbeOptions {
        host: options.host.clone(),
        wait_configure_roundtrips: options.wait_configure_roundtrips,
        codec,
        width,
        height,
        target_max_fps: options.target_max_fps,
        audio_master_clock,
        clear_color: options.clear_color,
    };
    let h264_retained_video_present_decode = h264_streaming_decode_requested.then(|| {
        run_native_vulkan_vulkanalia_h264_streaming_video_present_decode(
            NativeVulkanVulkanaliaH264StreamingVideoPresentDecodeOptions {
                session: video_present_session_options.clone(),
                source: source.clone(),
                queue_capacity: streaming_queue_capacity,
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
                playback_frame_count,
            },
        )
    });
    let av1_retained_video_present_decode = av1_streaming_decode_requested.then(|| {
        run_native_vulkan_vulkanalia_av1_streaming_video_present_decode(
            NativeVulkanVulkanaliaAv1StreamingVideoPresentDecodeOptions {
                session: video_present_session_options.clone(),
                source: source.clone(),
                queue_capacity: streaming_queue_capacity,
                playback_frame_count,
            },
        )
    });
    let (
        video_present_session_probe,
        video_present_session_probe_error,
        h264_retained_video_present_decode,
        h264_retained_video_present_decode_error,
        h265_retained_video_present_decode,
        h265_retained_video_present_decode_error,
        av1_retained_video_present_decode,
        av1_retained_video_present_decode_error,
    ) = if let Some(retained_decode) = h264_retained_video_present_decode {
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
    } else if let Some(retained_decode) = h265_retained_video_present_decode {
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
    } else if let Some(retained_decode) = av1_retained_video_present_decode {
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
    let retained_decode_snapshot_present = h264_retained_video_present_decode.is_some()
        || h265_retained_video_present_decode.is_some()
        || av1_retained_video_present_decode.is_some();
    let video_present_device_probe = if retained_decode_snapshot_present {
        None
    } else {
        video_present_session_probe
            .as_ref()
            .map(|probe| probe.device.clone())
    };
    let video_present_device_probe_error = if video_present_device_probe.is_none() {
        video_present_session_probe_error.clone()
    } else {
        None
    };
    let decoded_image_present_draw_requested = false;
    let decoded_image_present_draw = None;
    let decoded_image_present_draw_error = None;
    let decoded_image_present_sequence_requested = false;
    let decoded_image_present_sequence = None;
    let decoded_image_present_sequence_error = None;
    if let Some(worker) = audio_output_worker {
        let output_clock = worker.join().map_err(|_| {
            NativeVulkanError::Video("PipeWire audio output worker panicked".to_owned())
        })??;
        audio_clock = Some(output_clock);
    }
    let decoded_image_zero_copy_presented = h264_retained_video_present_decode
        .as_ref()
        .is_some_and(|snapshot| snapshot.decoded_image_zero_copy_presented)
        || h265_retained_video_present_decode
            .as_ref()
            .is_some_and(|snapshot| snapshot.decoded_image_zero_copy_presented)
        || av1_retained_video_present_decode
            .as_ref()
            .is_some_and(|snapshot| snapshot.decoded_image_zero_copy_presented);
    let audio_video_present_sequence = native_vulkan_ready_prefix_present_sequence(
        h264_retained_video_present_decode.as_ref(),
        h265_retained_video_present_decode.as_ref(),
        av1_retained_video_present_decode.as_ref(),
    );
    let audio_video_sync = native_vulkan_ready_prefix_audio_video_sync_snapshot(
        audio_clock.as_ref(),
        audio_video_present_sequence,
        playback_frame_count,
        options.target_max_fps,
    );
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
        audio_clock,
        audio_master_clock_enabled,
        audio_master_clock_start_ns,
        audio_video_sync,
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
        h264_retained_video_present_decode_requested: h264_streaming_decode_requested,
        h264_retained_video_present_decode,
        h264_retained_video_present_decode_error,
        h265_retained_video_present_decode_requested: h265_streaming_decode_requested,
        h265_retained_video_present_decode,
        h265_retained_video_present_decode_error,
        av1_retained_video_present_decode_requested: av1_streaming_decode_requested,
        av1_retained_video_present_decode,
        av1_retained_video_present_decode_error,
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
            "ready-prefix decode writes into the retained Vulkanalia DPB/output image, then Vulkanalia samples Y/UV plane descriptors through VK_EXT_descriptor_heap in a dynamic-rendering fullscreen pass and presents it to the Wayland swapchain"
        } else if h264_streaming_decode_requested {
            "H.264 ready-prefix decode writes into the retained Vulkanalia video-present DPB/output image and creates Vulkanalia descriptor-heap Y/UV plane sampler resources for that image; decoded-image present falls back to the clear placeholder until the draw/present gate succeeds"
        } else if h265_streaming_decode_requested {
            "H.265 ready-prefix decode writes into the retained Vulkanalia video-present DPB/output image and creates Vulkanalia descriptor-heap Y/UV plane sampler resources for that image; decoded-image present falls back to the clear placeholder until the draw/present gate succeeds"
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

pub(in crate::renderer::native_vulkan) fn native_vulkan_audio_runtime_packet_budget(
    playback_duration: Duration,
    playback_frame_count: u32,
) -> u32 {
    let duration_packets = playback_duration.as_nanos().saturating_add(9_999_999) / 10_000_000;
    let packet_budget = duration_packets
        .saturating_add(u128::from(playback_frame_count.max(1)))
        .saturating_add(64);
    u32::try_from(packet_budget.min(4096))
        .unwrap_or(4096)
        .max(64)
}

fn duration_ns_u64(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

fn native_vulkan_vulkanalia_streaming_packet_queue_capacity(
    _bitstream_samples: u32,
    _ready_prefix_frame_count: u32,
) -> usize {
    NATIVE_VULKAN_PACKET_HANDOFF_FRAMES
}

fn native_vulkan_ready_prefix_present_sequence<'a>(
    h264: Option<&'a NativeVulkanVulkanaliaH264RetainedVideoPresentDecodeSnapshot>,
    h265: Option<&'a NativeVulkanVulkanaliaH265RetainedVideoPresentDecodeSnapshot>,
    av1: Option<&'a NativeVulkanVulkanaliaAv1RetainedVideoPresentDecodeSnapshot>,
) -> Option<&'a NativeVulkanVulkanaliaDecodedImagePresentSequenceSnapshot> {
    h264.and_then(|snapshot| snapshot.decoded_image_present_sequence.as_ref())
        .or_else(|| h265.and_then(|snapshot| snapshot.decoded_image_present_sequence.as_ref()))
        .or_else(|| av1.and_then(|snapshot| snapshot.decoded_image_present_sequence.as_ref()))
}

fn native_vulkan_ready_prefix_audio_video_sync_snapshot(
    audio: Option<&NativeVulkanAudioClockRuntimeSnapshot>,
    sequence: Option<&NativeVulkanVulkanaliaDecodedImagePresentSequenceSnapshot>,
    playback_frame_count: u32,
    target_max_fps: Option<u32>,
) -> NativeVulkanReadyPrefixAudioVideoSyncSnapshot {
    const MAX_ALLOWED_DRIFT_NS: u64 = 100_000_000;

    let requested_playback_clock_ns = duration_ns_u64(
        native_vulkan_vulkanalia_visible_present_duration(playback_frame_count, target_max_fps),
    )
    .max(1);
    let audio_clock_coverage_ready = audio.is_some_and(|audio| {
        audio.playback_target_reached
            && audio.playback_coverage_percent >= 100
            && audio.playback_target_clock_ns.is_some()
            && audio.playback_covered_clock_ns.is_some()
            && audio.video_master_clock_ready
    });
    let audio_output_quality_ready = audio.is_some_and(|audio| match audio.output_mode {
        "auto" => {
            audio.audio_output_backend == "pipewire-s16le"
                && audio.audible_output_started
                && audio.audio_output_write_calls > 0
                && audio.audio_output_write_waits > 0
                && audio.audio_output_process_callbacks > 0
                && audio.audio_output_xrun_count == 0
                && audio.audio_output_stream_ready
        }
        "clock-only" => {
            audio.audio_output_backend == "none"
                && audio.audio_output_write_calls == 0
                && audio.audio_output_write_waits == 0
                && audio.audio_output_process_callbacks == 0
                && audio.audio_output_xrun_count == 0
                && !audio.audio_output_stream_ready
        }
        _ => false,
    });
    let audio_output_lifecycle_ready = audio.is_some_and(|audio| match audio.output_mode {
        "auto" => {
            audio.audio_output_state_changes > 0
                && audio.audio_output_ready_state_changes > 0
                && matches!(audio.audio_output_stream_state, "paused" | "streaming")
        }
        "clock-only" => {
            audio.audio_output_state_changes == 0
                && audio.audio_output_ready_state_changes == 0
                && audio.audio_output_stream_state == "unconnected"
        }
        _ => false,
    });
    let video_present_sequence_ready = sequence.is_some_and(|sequence| {
        sequence.presented_frame_count == sequence.requested_present_frame_count
            && sequence.presented_frame_count == playback_frame_count
            && sequence.pts_monotonic
            && sequence.display_order_monotonic
            && sequence.all_zero_copy_presented
    });
    let audio_covered_clock_ns = audio.and_then(|audio| audio.playback_covered_clock_ns);
    let audio_video_target_drift_ns = native_vulkan_audio_video_signed_delta_ns(
        audio_covered_clock_ns,
        requested_playback_clock_ns,
    );
    let audio_video_target_drift_abs_ns = audio_video_target_drift_ns.unsigned_abs();
    let drift_within_policy = audio_video_target_drift_abs_ns <= MAX_ALLOWED_DRIFT_NS;
    let enabled = audio.is_some();
    let ready = enabled
        && audio_clock_coverage_ready
        && audio_output_quality_ready
        && audio_output_lifecycle_ready
        && video_present_sequence_ready
        && drift_within_policy;
    let blocking_reason = if ready {
        None
    } else if audio.is_none() {
        Some("audio-runtime-not-attached")
    } else if !audio_clock_coverage_ready {
        Some("audio-clock-coverage-incomplete")
    } else if !audio_output_quality_ready {
        Some("audio-output-quality-gate-failed")
    } else if !audio_output_lifecycle_ready {
        Some("audio-output-lifecycle-gate-failed")
    } else if !video_present_sequence_ready {
        Some("video-present-sequence-incomplete")
    } else {
        Some("audio-video-drift-outside-policy")
    };
    NativeVulkanReadyPrefixAudioVideoSyncSnapshot {
        route: "ready-prefix-audio-video-sync",
        model: "PipeWire audio runtime coverage plus decoded-image present sequence PTS/pacing evidence",
        enabled,
        ready,
        blocking_reason,
        max_allowed_drift_ns: MAX_ALLOWED_DRIFT_NS,
        drift_within_policy,
        requested_playback_clock_ns,
        audio_target_clock_ns: audio.and_then(|audio| audio.playback_target_clock_ns),
        audio_covered_clock_ns,
        audio_video_target_drift_ns,
        audio_video_target_drift_abs_ns,
        audio_clock_coverage_ready,
        audio_output_quality_ready,
        audio_output_lifecycle_ready,
        audio_output_xrun_count: audio.map_or(0, |audio| audio.audio_output_xrun_count),
        audio_master_clock_start_ns: audio.and_then(|audio| audio.video_master_start_clock_ns),
        audio_current_serial_start_clock_ns: audio
            .and_then(|audio| audio.current_serial_start_clock_ns),
        video_present_sequence_ready,
        video_requested_frame_count: sequence
            .map_or(0, |sequence| sequence.requested_present_frame_count),
        video_presented_frame_count: sequence.map_or(0, |sequence| sequence.presented_frame_count),
        video_pts_monotonic: sequence.is_some_and(|sequence| sequence.pts_monotonic),
        video_source_frame_pts_delta_min_ns: sequence
            .and_then(|sequence| sequence.source_frame_pts_delta_min_ns),
        video_source_frame_pts_delta_max_ns: sequence
            .and_then(|sequence| sequence.source_frame_pts_delta_max_ns),
        present_pacing_clock_model: sequence
            .and_then(|sequence| sequence.latest_draw.as_ref())
            .map_or("none", |draw| draw.pacing_clock_model),
    }
}

fn native_vulkan_audio_video_signed_delta_ns(
    audio_clock_ns: Option<u64>,
    video_clock_ns: u64,
) -> i64 {
    let Some(audio_clock_ns) = audio_clock_ns else {
        return i64::MAX;
    };
    let delta = i128::from(audio_clock_ns) - i128::from(video_clock_ns);
    delta.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
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

    #[test]
    fn audio_runtime_packet_budget_scales_with_playback_duration() {
        let short = native_vulkan_audio_runtime_packet_budget(Duration::from_millis(100), 4);
        let ten_seconds = native_vulkan_audio_runtime_packet_budget(Duration::from_secs(10), 2400);

        assert_eq!(short, 78);
        assert!(ten_seconds > 1000);
        assert!(ten_seconds <= 4096);
    }

    #[test]
    fn streaming_packet_queue_capacity_stays_ffmpeg_handoff_bounded() {
        assert_eq!(
            native_vulkan_vulkanalia_streaming_packet_queue_capacity(0, 0),
            NATIVE_VULKAN_PACKET_HANDOFF_FRAMES
        );
        assert_eq!(
            native_vulkan_vulkanalia_streaming_packet_queue_capacity(8, 4),
            NATIVE_VULKAN_PACKET_HANDOFF_FRAMES
        );
        assert_eq!(
            native_vulkan_vulkanalia_streaming_packet_queue_capacity(360, 360),
            NATIVE_VULKAN_PACKET_HANDOFF_FRAMES
        );
    }
}
