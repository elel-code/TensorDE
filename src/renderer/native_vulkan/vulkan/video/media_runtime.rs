use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use crate::renderer::native_vulkan::{
    audio::clock::{
        NATIVE_VULKAN_AUDIO_CLOCK_QUEUE_PACKETS, NativeVulkanAudioClockProbeOptions,
        NativeVulkanAudioClockRuntimeSnapshot, native_vulkan_probe_ffmpeg_audio_clock,
        native_vulkan_unattached_audio_clock_snapshot,
    },
    audio::policy::NativeVulkanAudioOutputMode,
    video::direct::native_vulkan_audio_runtime_packet_budget,
};

const FFMPEG_AUDIO_OUTPUT_WORKER_STACK_BYTES: usize = 256 * 1024;

pub(in crate::renderer::native_vulkan::vulkan) type NativeVulkanFfmpegAudioOutputWorker =
    thread::JoinHandle<Result<NativeVulkanAudioClockRuntimeSnapshot, String>>;

pub(in crate::renderer::native_vulkan::vulkan) struct NativeVulkanFfmpegVideoAudioClockPrepareOptions
{
    pub source: PathBuf,
    pub playback_frame_count: u32,
    pub target_max_fps: Option<u32>,
    pub audio_clock_probe_requested: bool,
    pub audio_output_mode: NativeVulkanAudioOutputMode,
}

pub(in crate::renderer::native_vulkan::vulkan) struct NativeVulkanFfmpegVideoAudioClockPreparation {
    pub clock: Option<NativeVulkanAudioClockRuntimeSnapshot>,
    pub worker: Option<NativeVulkanFfmpegAudioOutputWorker>,
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_ffmpeg_prepare_audio_clock_for_video_present(
    options: NativeVulkanFfmpegVideoAudioClockPrepareOptions,
) -> Result<NativeVulkanFfmpegVideoAudioClockPreparation, String> {
    if options.audio_clock_probe_requested {
        let mut probe_options =
            NativeVulkanAudioClockProbeOptions::clock_only(options.source.clone());
        let audio_playback_duration = native_vulkan_ffmpeg_visible_present_duration(
            options.playback_frame_count,
            options.target_max_fps,
        );
        probe_options.output_mode = options.audio_output_mode;
        probe_options.target_playback_clock_ns =
            Some(native_vulkan_duration_ns_u64(audio_playback_duration).max(1));
        probe_options.loop_on_eos = true;
        probe_options.packets_to_probe = native_vulkan_audio_runtime_packet_budget(
            audio_playback_duration,
            options.playback_frame_count,
        );

        if options.audio_output_mode == NativeVulkanAudioOutputMode::Auto {
            let mut clock_probe_options = probe_options.clone();
            clock_probe_options.output_mode = NativeVulkanAudioOutputMode::ClockOnly;
            clock_probe_options.packets_to_probe = NATIVE_VULKAN_AUDIO_CLOCK_QUEUE_PACKETS as u32;
            clock_probe_options.target_playback_clock_ns = None;
            let clock = native_vulkan_probe_ffmpeg_audio_clock(clock_probe_options)
                .map_err(|err| err.to_string())?;
            let worker = thread::Builder::new()
                .name("gilder-ffmpeg-pipewire-audio-output".to_owned())
                .stack_size(FFMPEG_AUDIO_OUTPUT_WORKER_STACK_BYTES)
                .spawn(move || {
                    native_vulkan_probe_ffmpeg_audio_clock(probe_options)
                        .map_err(|err| err.to_string())
                })
                .map_err(|err| format!("spawn PipeWire audio output worker: {err}"))?;
            return Ok(NativeVulkanFfmpegVideoAudioClockPreparation {
                clock: Some(clock),
                worker: Some(worker),
            });
        }

        let clock =
            native_vulkan_probe_ffmpeg_audio_clock(probe_options).map_err(|err| err.to_string())?;
        return Ok(NativeVulkanFfmpegVideoAudioClockPreparation {
            clock: Some(clock),
            worker: None,
        });
    }

    if options.audio_output_mode == NativeVulkanAudioOutputMode::ClockOnly {
        return Ok(NativeVulkanFfmpegVideoAudioClockPreparation {
            clock: Some(native_vulkan_unattached_audio_clock_snapshot(
                options.audio_output_mode,
            )),
            worker: None,
        });
    }

    Ok(NativeVulkanFfmpegVideoAudioClockPreparation {
        clock: None,
        worker: None,
    })
}

fn native_vulkan_ffmpeg_visible_present_duration(
    playback_frame_count: u32,
    target_max_fps: Option<u32>,
) -> Duration {
    let fps = target_max_fps.unwrap_or(240).max(1);
    let nanos = u128::from(playback_frame_count.max(1)) * 1_000_000_000u128 / u128::from(fps);
    Duration::from_nanos(nanos.min(u128::from(u64::MAX)) as u64)
}

fn native_vulkan_duration_ns_u64(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}
