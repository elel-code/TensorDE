// FFmpeg AVVkFrame present state and retained GPU-resource ownership.

use std::collections::VecDeque;
use std::env;
use std::path::PathBuf;
use std::sync::{mpsc, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{
    self, KhrSurfaceExtensionInstanceCommands, KhrSwapchainExtensionDeviceCommands,
};

use crate::engine::scene::SceneMediaPlaybackState;
use crate::renderer::native_vulkan::video::event_source::{
    NativeVulkanMediaEventRuntime, NativeVulkanMediaEventRuntimeSnapshot,
    NativeVulkanVideoEventSample,
};
use crate::renderer::native_vulkan::video::ffmpeg_hw::{
    NativeVulkanFfmpegDecodedGpuFrame, NativeVulkanFfmpegDecodedGpuFrameDescriptorSource,
    NativeVulkanFfmpegVulkanHwDecoder, NativeVulkanFfmpegVulkanHwDecoderSnapshot,
    NativeVulkanFfmpegVulkanHwDevice, NativeVulkanFfmpegVulkanHwDeviceBorrow,
};
use crate::renderer::native_vulkan::{
    NativeVulkanClearColor, NativeVulkanVideoSessionCodec,
    NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
    NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    audio::clock::NativeVulkanAudioClockRuntimeSnapshot,
    audio::event_source::NativeVulkanAudioEventChannel,
    audio::policy::NativeVulkanAudioOutputMode,
};

use super::instance::{
    NativeVulkanVulkanaliaInstance,
    native_vulkan_vulkanalia_create_instance_with_required_extensions,
    native_vulkan_vulkanalia_destroy_instance,
};
use super::render_present::{
    DECODED_IMAGE_PRESENT_TELEMETRY_RETAINED_FRAMES,
    NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot,
    NativeVulkanVulkanaliaDecodedImagePresentSequenceSnapshot,
    NativeVulkanVulkanaliaDecodedImagePresentSlowFrameSnapshot,
    VulkanaliaDecodedImagePresentFrameResources,
    VulkanaliaDecodedImagePresentImageSource,
    VulkanaliaDecodedImagePresentSamplerResources,
    VulkanaliaDecodedImagePresentTimingConfig,
    native_vulkan_vulkanalia_create_decoded_image_present_frame_resources,
    native_vulkan_vulkanalia_create_decoded_image_present_pipeline_resources,
    native_vulkan_vulkanalia_create_ffmpeg_decoded_gpu_frame_present_sampler_resources,
    native_vulkan_vulkanalia_decoded_image_present_frame_slot_count,
    native_vulkan_vulkanalia_destroy_decoded_image_present_frame_resources,
    native_vulkan_vulkanalia_destroy_decoded_image_present_pipeline_resources,
    native_vulkan_vulkanalia_destroy_decoded_image_present_sampler_resources,
    native_vulkan_vulkanalia_prepare_decoded_image_present_frame_slot,
    native_vulkan_vulkanalia_present_decoded_image_frame_with_sources,
    native_vulkan_vulkanalia_try_complete_decoded_image_present_frame_slot,
    native_vulkan_vulkanalia_wait_decoded_image_present_frame_slot,
};
use super::swapchain::{
    REQUIRED_INSTANCE_EXTENSIONS, create_vulkanalia_swapchain_plan,
    create_vulkanalia_wayland_surface,
};
use super::video_media_runtime::{
    NativeVulkanFfmpegVideoAudioClockPrepareOptions,
    native_vulkan_ffmpeg_prepare_audio_clock_for_video_present,
};
use super::video_present_device::{
    NativeVulkanVulkanaliaVideoPresentAudioMasterClock,
    NativeVulkanVulkanaliaVideoPresentDeviceContext, create_video_present_device,
    device_snapshot_from_selection, select_video_present_physical_device,
    swapchain_plan_snapshot,
};
use super::video_present_handoff::NativeVulkanVulkanaliaDecodedPresentHandoffSnapshot;
use super::video_surface_host::{
    NativeVulkanVideoSurfaceHost, NativeVulkanVideoSurfaceHostSnapshot,
};

const FFMPEG_VULKAN_DECODE_REFERENCE: &str =
    "references/ffmpeg/libavcodec/vulkan_decode.c";
const FFMPEG_VULKAN_HWDECODE_FRAME_QUEUE_SIZE_DEFAULT: usize = 1;
const FFMPEG_VULKAN_HWDECODE_FRAME_QUEUE_SIZE_ENV: &str =
    "GILDER_FFMPEG_VULKAN_HWDECODE_FRAME_QUEUE_SIZE";
const FFMPEG_SINGLE_DECODE_THREAD_COUNT: u32 = 1;
const FFMPEG_FFPLAY_FRAME_QUEUE_REFERENCE: &str =
    "references/ffmpeg/fftools/ffplay.c:125-179,2205-2210";
const FFMPEG_AV_SYNC_THRESHOLD_MAX: Duration = Duration::from_millis(100);
const DECODED_IMAGE_PRESENT_SLOW_FRAME_THRESHOLD_MICROS: u64 = 6_250;
const DECODED_IMAGE_PRESENT_SLOW_FRAME_TELEMETRY_LIMIT: usize = 0;
const VIDEO_PRESENT_SLEEP_GUARD_DEFAULT_MICROS: u64 = 0;
const VIDEO_PRESENT_SPIN_GUARD_DEFAULT_MICROS: u64 = 0;
const VIDEO_PRESENT_SLEEP_GUARD_ENV: &str = "GILDER_VIDEO_PRESENT_SLEEP_GUARD_MICROS";
const VIDEO_PRESENT_SPIN_GUARD_ENV: &str = "GILDER_VIDEO_PRESENT_SPIN_GUARD_MICROS";

pub struct NativeVulkanFfmpegVulkanHwVideoPresentOptions {
    pub host: crate::renderer::native_wayland::NativeWaylandHostOptions,
    pub wait_configure_roundtrips: usize,
    pub source: PathBuf,
    pub codec: NativeVulkanVideoSessionCodec,
    pub playback_frame_count: u32,
    pub target_max_fps: Option<u32>,
    pub audio_clock_probe_requested: bool,
    pub audio_output_mode: NativeVulkanAudioOutputMode,
    pub audio_master_clock: NativeVulkanVulkanaliaVideoPresentAudioMasterClock,
    pub clear_color: NativeVulkanClearColor,
}

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanFfmpegVulkanHwVideoPresentSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub source: PathBuf,
    pub codec: NativeVulkanVideoSessionCodec,
    pub requested_present_frame_count: u32,
    pub device: super::video_present_device::NativeVulkanVulkanaliaVideoPresentDeviceProbeSnapshot,
    pub surface_host: Option<NativeVulkanVideoSurfaceHostSnapshot>,
    pub decoder: NativeVulkanFfmpegVulkanHwDecoderSnapshot,
    pub audio_clock_probe_requested: bool,
    pub audio_output_mode: &'static str,
    pub audio_clock: Option<NativeVulkanAudioClockRuntimeSnapshot>,
    pub audio_master_clock_enabled: bool,
    pub audio_master_clock_start_ns: Option<u64>,
    pub media_events: NativeVulkanMediaEventRuntimeSnapshot,
    pub decoded_image_present_sequence_requested: bool,
    pub decoded_image_present_sequence:
        Option<NativeVulkanVulkanaliaDecodedImagePresentSequenceSnapshot>,
    pub decoded_image_present_sequence_error: Option<String>,
    pub decoded_image_zero_copy_presented: bool,
    pub descriptor_heap_only: bool,
    pub zero_copy_scope: &'static str,
    pub ffmpeg_reference: &'static str,
}

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, PartialEq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanFfmpegVulkanHwSceneVideoPresentSourceOptions
{
    pub source: PathBuf,
    pub codec: NativeVulkanVideoSessionCodec,
    pub playback_frame_count: u32,
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) struct NativeVulkanFfmpegVulkanHwSceneVideoPresentOptions {
    pub host: crate::renderer::native_wayland::NativeWaylandHostOptions,
    pub wait_configure_roundtrips: usize,
    pub target_max_fps: Option<u32>,
    pub audio_clock_probe_requested: bool,
    pub audio_output_mode: NativeVulkanAudioOutputMode,
    pub clear_color: NativeVulkanClearColor,
    pub sources: Vec<NativeVulkanFfmpegVulkanHwSceneVideoPresentSourceOptions>,
}

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanFfmpegVulkanHwSceneVideoPresentSourceSnapshot {
    pub source_index: usize,
    pub source: PathBuf,
    pub codec: NativeVulkanVideoSessionCodec,
    pub requested_present_frame_count: u32,
    pub decoder: NativeVulkanFfmpegVulkanHwDecoderSnapshot,
    pub decoded_image_zero_copy_presented: bool,
}

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanFfmpegVulkanHwSceneVideoPresentSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub source_count: usize,
    pub codec_count: usize,
    pub codecs: Vec<NativeVulkanVideoSessionCodec>,
    pub surface_host: Option<NativeVulkanVideoSurfaceHostSnapshot>,
    pub sources: Vec<NativeVulkanFfmpegVulkanHwSceneVideoPresentSourceSnapshot>,
    pub audio_clock_probe_requested: bool,
    pub audio_output_mode: &'static str,
    pub audio_clock: Option<NativeVulkanAudioClockRuntimeSnapshot>,
    pub audio_master_clock_enabled: bool,
    pub audio_master_clock_start_ns: Option<u64>,
    pub decoded_image_present_sequence_requested: bool,
    pub decoded_image_present_sequence:
        Option<NativeVulkanVulkanaliaDecodedImagePresentSequenceSnapshot>,
    pub decoded_image_present_sequence_error: Option<String>,
    pub decoded_image_zero_copy_presented: bool,
    pub descriptor_heap_only: bool,
    pub zero_copy_scope: &'static str,
    pub ffmpeg_reference: &'static str,
}

#[cfg(feature = "native-vulkan-video")]

struct NativeVulkanFfmpegPresentedFrameRetention {
    present_frame_slot: u32,
    _decoded_frame: NativeVulkanFfmpegDecodedGpuFrame,
}

#[cfg(feature = "native-vulkan-video")]
struct NativeVulkanFfmpegPresentedFrameSetRetention {
    present_frame_slot: u32,
    _decoded_frames: Vec<NativeVulkanFfmpegDecodedGpuFrame>,
}

#[cfg(feature = "native-vulkan-video")]
struct NativeVulkanFfmpegDecodedGpuFrameHandoff {
    decoded_frame: NativeVulkanFfmpegDecodedGpuFrame,
    media_generation: u64,
    release_ack: Option<mpsc::SyncSender<()>>,
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanFfmpegDecodedGpuFrameHandoff {
    fn new(
        decoded_frame: NativeVulkanFfmpegDecodedGpuFrame,
        media_generation: u64,
        release_ack: Option<mpsc::SyncSender<()>>,
    ) -> Self {
        Self {
            decoded_frame,
            media_generation,
            release_ack,
        }
    }

    fn release(mut self) {
        let release_ack = self.release_ack.take();
        drop(self.decoded_frame);
        if let Some(release_ack) = release_ack {
            let _ = release_ack.send(());
        }
    }

    fn into_retained_frame(mut self) -> NativeVulkanFfmpegDecodedGpuFrame {
        if let Some(release_ack) = self.release_ack.take() {
            let _ = release_ack.send(());
        }
        self.decoded_frame
    }
}

#[cfg(feature = "native-vulkan-video")]
struct NativeVulkanFfmpegPresentedFrameRetentionQueue<'a> {
    device: &'a Device,
    frame_resources: &'a VulkanaliaDecodedImagePresentFrameResources,
    frames: VecDeque<NativeVulkanFfmpegPresentedFrameRetention>,
    peak_frame_count: usize,
}

#[cfg(feature = "native-vulkan-video")]
impl<'a> NativeVulkanFfmpegPresentedFrameRetentionQueue<'a> {
    fn new(
        device: &'a Device,
        frame_resources: &'a VulkanaliaDecodedImagePresentFrameResources,
    ) -> Self {
        Self {
            device,
            frame_resources,
            frames: VecDeque::new(),
            peak_frame_count: 0,
        }
    }

    fn push_after_submit(
        &mut self,
        present_frame_slot: u32,
        decoded_frame: NativeVulkanFfmpegDecodedGpuFrame,
    ) -> Result<(), String> {
        self.release_completed_slot(present_frame_slot);
        self.frames
            .push_back(NativeVulkanFfmpegPresentedFrameRetention {
                present_frame_slot,
                _decoded_frame: decoded_frame,
            });
        self.peak_frame_count = self.peak_frame_count.max(self.frames.len());
        self.release_completed_frames()
    }

    fn release_completed_slot(&mut self, present_frame_slot: u32) {
        if let Some(index) = self
            .frames
            .iter()
            .position(|frame| frame.present_frame_slot == present_frame_slot)
        {
            if let Some(frame) = self.frames.remove(index) {
                self.destroy_retained_frame(frame);
            }
        }
    }

    fn release_completed_frames(&mut self) -> Result<(), String> {
        let mut index = 0usize;
        while index < self.frames.len() {
            let present_frame_slot = self.frames[index].present_frame_slot;
            if native_vulkan_vulkanalia_try_complete_decoded_image_present_frame_slot(
                self.device,
                self.frame_resources,
                present_frame_slot,
            )? {
                if let Some(frame) = self.frames.remove(index) {
                    self.destroy_retained_frame(frame);
                }
            } else {
                index += 1;
            }
        }
        Ok(())
    }

    fn drain_after_waits(&mut self) -> Result<(), String> {
        while let Some(frame) = self.frames.pop_front() {
            native_vulkan_vulkanalia_wait_decoded_image_present_frame_slot(
                self.device,
                self.frame_resources,
                frame.present_frame_slot,
            )?;
            self.destroy_retained_frame(frame);
        }
        Ok(())
    }

    fn clear_retained_frames(&mut self) {
        while let Some(frame) = self.frames.pop_front() {
            self.destroy_retained_frame(frame);
        }
    }

    fn destroy_retained_frame(&self, frame: NativeVulkanFfmpegPresentedFrameRetention) {
        drop(frame);
    }

    fn frame_count(&self) -> u32 {
        self.frames.len().min(u32::MAX as usize) as u32
    }

    fn peak_frame_count(&self) -> u32 {
        self.peak_frame_count.min(u32::MAX as usize) as u32
    }
}

#[cfg(feature = "native-vulkan-video")]
impl Drop for NativeVulkanFfmpegPresentedFrameRetentionQueue<'_> {
    fn drop(&mut self) {
        if !self.frames.is_empty() {
            let _ = unsafe { self.device.device_wait_idle() };
        }
        while let Some(frame) = self.frames.pop_front() {
            self.destroy_retained_frame(frame);
        }
    }
}

include!("ffmpeg_present_state/retention_and_sampler.rs");

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, Copy)]
struct NativeVulkanVulkanaliaMultiSourceFrameTiming {
    source_frame_pts_ns: Option<u64>,
    source_frame_duration_ns: Option<u64>,
    source_frame_pts_ms: Option<u64>,
    source_frame_duration_ms: Option<u64>,
}
