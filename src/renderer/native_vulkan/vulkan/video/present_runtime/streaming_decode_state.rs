use std::collections::VecDeque;
use std::env;

#[cfg(feature = "native-vulkan-video")]
use std::path::PathBuf;
#[cfg(feature = "native-vulkan-video")]
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{
    self, HasBuilder, KhrSurfaceExtensionInstanceCommands, KhrSwapchainExtensionDeviceCommands,
};

#[cfg(feature = "native-vulkan-video")]
use crate::engine::scene::SceneMediaPlaybackState;

#[cfg(feature = "native-vulkan-video")]
use crate::renderer::native_vulkan::video::codec_reference::{
    NativeVulkanAv1DecodeReferencePlanner, NativeVulkanAv1StreamingBootstrap,
    NativeVulkanH264DecodeReferencePlanner, NativeVulkanH264StreamingBootstrap,
    NativeVulkanH265DecodeReferencePlanner, NativeVulkanH265StreamingBootstrap,
    native_vulkan_av1_align_streaming_bootstrap, native_vulkan_h264_align_streaming_bootstrap,
    native_vulkan_h265_align_streaming_bootstrap,
};
#[cfg(feature = "native-vulkan-video")]
use crate::renderer::native_vulkan::video::event_source::{
    NativeVulkanMediaEventRuntime, NativeVulkanMediaEventRuntimeSnapshot,
    NativeVulkanVideoEventSample,
};
#[cfg(feature = "native-vulkan-video")]
use crate::renderer::native_vulkan::video::extract::{
    native_vulkan_start_av1_streaming_packet_queue,
    native_vulkan_start_h264_streaming_packet_queue,
    native_vulkan_start_h265_streaming_packet_queue,
};
#[cfg(feature = "native-vulkan-video")]
use crate::renderer::native_vulkan::video::ffmpeg_hw::{
    NativeVulkanFfmpegDecodedGpuFrame, NativeVulkanFfmpegDecodedGpuFrameDescriptorSource,
    NativeVulkanFfmpegVulkanHwDecoder, NativeVulkanFfmpegVulkanHwDecoderSnapshot,
    NativeVulkanFfmpegVulkanHwDevice, NativeVulkanFfmpegVulkanHwDeviceBorrow,
};
#[cfg(feature = "native-vulkan-video")]
use crate::renderer::native_vulkan::video::vulkan_extract::native_vulkan_vulkanalia_av1_frame_submit_input_from_temporal_unit;
#[cfg(feature = "native-vulkan-video")]
use crate::renderer::native_vulkan::{
    NativeVulkanAv1ActiveDpbReference, NativeVulkanAv1StreamingPacketQueue,
    NativeVulkanH264StreamingPacketQueue, NativeVulkanH265StreamingPacketQueue,
    native_vulkan_av1_update_active_dpb_refs_after_display_handoff,
};
use crate::renderer::native_vulkan::{NativeVulkanClearColor, NativeVulkanVideoSessionCodec};
#[cfg(feature = "native-vulkan-video")]
use crate::renderer::native_vulkan::{
    NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
    NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
};
#[cfg(feature = "native-vulkan-video")]
use crate::renderer::native_vulkan::{
    audio::clock::NativeVulkanAudioClockRuntimeSnapshot,
    audio::event_source::NativeVulkanAudioEventChannel, audio::policy::NativeVulkanAudioOutputMode,
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
    VulkanaliaDecodedImagePresentPipelineResources, VulkanaliaDecodedImagePresentSamplerResources,
    VulkanaliaDecodedImagePresentTimingConfig, VulkanaliaSceneVideoOverlayFrameDraw,
    native_vulkan_vulkanalia_create_decoded_image_present_frame_resources,
    native_vulkan_vulkanalia_create_decoded_image_present_pipeline_resources,
    native_vulkan_vulkanalia_create_decoded_image_present_sampler_resources,
    native_vulkan_vulkanalia_decoded_image_present_frame_slot_count,
    native_vulkan_vulkanalia_destroy_decoded_image_present_frame_resources,
    native_vulkan_vulkanalia_destroy_decoded_image_present_pipeline_resources,
    native_vulkan_vulkanalia_destroy_decoded_image_present_sampler_resources,
    native_vulkan_vulkanalia_prepare_decoded_image_present_frame_slot,
    native_vulkan_vulkanalia_present_decoded_image_frame,
    native_vulkan_vulkanalia_present_decoded_image_once,
    native_vulkan_vulkanalia_retarget_decoded_image_present_sampler_layer,
    native_vulkan_vulkanalia_try_complete_decoded_image_present_frame_slot,
    native_vulkan_vulkanalia_wait_decoded_image_present_frame_slot,
};
#[cfg(feature = "native-vulkan-video")]
use super::render_present::{
    VulkanaliaDecodedImagePresentFrameResources, VulkanaliaDecodedImagePresentImageSource,
    native_vulkan_vulkanalia_create_ffmpeg_decoded_gpu_frame_present_sampler_resources,
    native_vulkan_vulkanalia_present_decoded_image_frame_with_sources,
};
use super::swapchain::{
    OPTIONAL_INSTANCE_EXTENSIONS, REQUIRED_INSTANCE_EXTENSIONS, create_vulkanalia_swapchain_plan,
    create_vulkanalia_wayland_surface, vulkanalia_surface_capabilities2_enabled,
    vulkanalia_surface_maintenance1_enabled,
};
use super::video_decode_submit::FFMPEG_VULKAN_DECODE_REFERENCE;
use super::video_decode_submit_av1::NativeVulkanVulkanaliaAv1CommandSmokeSnapshot;
use super::video_decode_submit_h264::NativeVulkanVulkanaliaH264ReadyPrefixCommandSmokeSnapshot;
#[cfg(feature = "native-vulkan-video")]
use super::video_decode_submit_h264::NativeVulkanVulkanaliaH264ReadyPrefixFrameInput;
use super::video_decode_submit_h265::NativeVulkanVulkanaliaH265ReadyPrefixCommandSmokeSnapshot;
#[cfg(feature = "native-vulkan-video")]
use super::video_decode_submit_h265::NativeVulkanVulkanaliaH265ReadyPrefixFrameInput;
#[cfg(feature = "native-vulkan-video")]
use super::video_media_runtime::{
    NativeVulkanFfmpegVideoAudioClockPrepareOptions,
    native_vulkan_ffmpeg_prepare_audio_clock_for_video_present,
};
use super::video_present_device::{
    NativeVulkanVulkanaliaVideoPresentAudioMasterClock,
    NativeVulkanVulkanaliaVideoPresentDeviceContext,
    NativeVulkanVulkanaliaVideoPresentSessionProbeOptions,
    NativeVulkanVulkanaliaVideoPresentSessionProbeSnapshot, create_video_present_device,
    decoded_image_resource_sharing_model, device_snapshot_from_selection,
    select_video_present_physical_device, swapchain_plan_snapshot,
    video_present_queue_family_indices,
};
#[cfg(feature = "native-vulkan-video")]
use super::video_present_handoff::NativeVulkanVulkanaliaDecodedPresentHandoffFrame;
use super::video_present_handoff::{
    NativeVulkanVulkanaliaDecodedPresentHandoff, NativeVulkanVulkanaliaDecodedPresentHandoffRecv,
    NativeVulkanVulkanaliaDecodedPresentHandoffSnapshot,
};
use super::video_profile_labels::video_decode_capability_flag_labels;
use super::video_session::{
    NativeVulkanVulkanaliaVideoSessionMemoryBindingResources,
    native_vulkan_vulkanalia_bind_video_session_memory_resources,
    native_vulkan_vulkanalia_create_video_session, native_vulkan_vulkanalia_destroy_video_session,
    native_vulkan_vulkanalia_destroy_video_session_memory_binding_resources,
    native_vulkan_vulkanalia_video_session_create_flags,
};
#[cfg(feature = "native-vulkan-video")]
use super::video_session_bind::{
    NativeVulkanVulkanaliaAv1StreamingDecodeInput, NativeVulkanVulkanaliaAv1StreamingFrameInput,
    NativeVulkanVulkanaliaH264StreamingDecodeInput, NativeVulkanVulkanaliaH265StreamingDecodeInput,
    native_vulkan_vulkanalia_record_av1_streaming_decode_into_image,
    native_vulkan_vulkanalia_record_h264_streaming_decode_into_image,
    native_vulkan_vulkanalia_record_h265_streaming_decode_into_image,
};
use super::video_session_capabilities::{
    native_vulkan_vulkanalia_video_session_effective_picture_format,
    native_vulkan_vulkanalia_video_session_extent_supported,
    native_vulkan_vulkanalia_video_session_max_active_reference_pictures,
    native_vulkan_vulkanalia_video_session_max_dpb_slots,
    with_native_vulkan_vulkanalia_video_session_capabilities,
};
use super::video_session_images::{
    NativeVulkanVulkanaliaVideoSessionResourceImageSmokeSnapshot,
    VulkanaliaVideoSessionResourceImage,
    native_vulkan_vulkanalia_create_video_session_resource_image,
    native_vulkan_vulkanalia_destroy_video_session_resource_image,
};
use super::video_surface_host::NativeVulkanVideoSurfaceHost;
#[cfg(feature = "native-vulkan-video")]
use super::video_surface_host::NativeVulkanVideoSurfaceHostSnapshot;

pub(in crate::renderer::native_vulkan::vulkan) const VIDEO_PRESENT_SESSION_RETAINED_RESOURCE_ROUTE: &str =
    "video-present-session-retained-resource";
const FFMPEG_VIDEO_PICTURE_QUEUE_SIZE: usize = 3;
const FFMPEG_VULKAN_HWDECODE_FRAME_QUEUE_SIZE_DEFAULT: usize = 1;
const FFMPEG_VULKAN_HWDECODE_FRAME_QUEUE_SIZE_ENV: &str =
    "GILDER_FFMPEG_VULKAN_HWDECODE_FRAME_QUEUE_SIZE";
const DECODED_IMAGE_PRESENT_STARTUP_PREROLL_FRAMES: usize = 1;
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

#[derive(Debug, Clone, PartialEq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanVulkanaliaSceneVideoOverlayInput;

struct VulkanaliaSceneVideoOverlayResources;

impl VulkanaliaSceneVideoOverlayResources {
    fn frame_draw(
        &mut self,
        _device: &Device,
        _present_frame_slot: u32,
        _elapsed_ms: u64,
        _swapchain_extent: vk::Extent2D,
    ) -> Result<Option<VulkanaliaSceneVideoOverlayFrameDraw<'static>>, String> {
        Err(native_vulkan_scene_video_overlay_removed_error())
    }
}

fn native_vulkan_scene_video_overlay_removed_error() -> String {
    "old native-vulkan scene video overlay was deleted; scene/video composition must be rebuilt through engine::scene_engine RenderingServer -> RendererSceneRender -> RenderingDevice".to_owned()
}

fn native_vulkan_vulkanalia_destroy_scene_video_overlay_resources(
    _device: &Device,
    _resources: VulkanaliaSceneVideoOverlayResources,
) {
}

pub(in crate::renderer::native_vulkan::vulkan) struct NativeVulkanVulkanaliaVideoPresentSessionRuntime
{
    resources: Option<NativeVulkanVulkanaliaVideoPresentSessionRuntimeResources>,
    snapshot: NativeVulkanVulkanaliaVideoPresentSessionProbeSnapshot,
}

impl NativeVulkanVulkanaliaVideoPresentSessionRuntime {
    pub(in crate::renderer::native_vulkan::vulkan) fn snapshot(
        &self,
    ) -> &NativeVulkanVulkanaliaVideoPresentSessionProbeSnapshot {
        &self.snapshot
    }
}

struct NativeVulkanVulkanaliaVideoPresentSessionRuntimeResources {
    _surface_host: NativeVulkanVideoSurfaceHost,
    vulkan: Option<NativeVulkanVulkanaliaInstance>,
    surface: vk::SurfaceKHR,
    context: Option<NativeVulkanVulkanaliaVideoPresentDeviceContext>,
    swapchain: vk::SwapchainKHR,
    swapchain_images: Vec<vk::Image>,
    swapchain_format: vk::Format,
    swapchain_extent: vk::Extent2D,
    decoded_image_present_timing: VulkanaliaDecodedImagePresentTimingConfig,
    clear_color: NativeVulkanClearColor,
    present_queue_family_index: u32,
    picture_format: vk::Format,
    session: vk::VideoSessionKHR,
    memory_resources: Option<NativeVulkanVulkanaliaVideoSessionMemoryBindingResources>,
    resource_image: Option<VulkanaliaVideoSessionResourceImage>,
    decoded_image_present_pipeline: Option<VulkanaliaDecodedImagePresentPipelineResources>,
    decoded_image_present_sampler: Option<VulkanaliaDecodedImagePresentSamplerResources>,
    scene_video_overlay: Option<VulkanaliaSceneVideoOverlayResources>,
    decoded_image_present_sequence:
        Option<NativeVulkanVulkanaliaDecodedImagePresentSequenceSnapshot>,
    decoded_image_present_sequence_error: Option<String>,
    h264_ready_prefix_decode: Option<NativeVulkanVulkanaliaH264ReadyPrefixCommandSmokeSnapshot>,
    h265_ready_prefix_decode: Option<NativeVulkanVulkanaliaH265ReadyPrefixCommandSmokeSnapshot>,
    av1_ready_prefix_decode: Option<NativeVulkanVulkanaliaAv1CommandSmokeSnapshot>,
}

impl NativeVulkanVulkanaliaVideoPresentSessionRuntimeResources {
    fn present_decoded_image_once(
        &mut self,
        sampled_array_layer: u32,
    ) -> Result<NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot, String> {
        let context = self.context.as_ref().ok_or_else(|| {
            "Vulkanalia video present context has already been released".to_owned()
        })?;
        let resource_image = self.resource_image.as_ref().ok_or_else(|| {
            "Vulkanalia decoded image resource has already been released".to_owned()
        })?;
        native_vulkan_vulkanalia_retarget_decoded_image_present_sampler_layer(
            &context.device,
            resource_image,
            self.picture_format,
            self.decoded_image_present_sampler.as_mut().ok_or_else(|| {
                "Vulkanalia decoded image present sampler is unavailable".to_owned()
            })?,
            sampled_array_layer,
        )?;
        let sampler = self
            .decoded_image_present_sampler
            .as_ref()
            .ok_or_else(|| "Vulkanalia decoded image present sampler is unavailable".to_owned())?;
        let pipeline = self
            .decoded_image_present_pipeline
            .as_ref()
            .ok_or_else(|| "Vulkanalia decoded image present pipeline is unavailable".to_owned())?;
        native_vulkan_vulkanalia_present_decoded_image_once(
            &context.device,
            context.present_queue,
            self.present_queue_family_index,
            self.swapchain,
            &self.swapchain_images,
            self.swapchain_format,
            self.swapchain_extent,
            resource_image,
            sampler,
            pipeline,
            self.decoded_image_present_timing,
            self.clear_color,
        )
    }

    fn decoded_image_present_result(
        &mut self,
        fallback_sampled_array_layer: u32,
    ) -> NativeVulkanVulkanaliaRetainedPresentResult {
        if let Some(sequence) = self.decoded_image_present_sequence.clone() {
            let draw = sequence.latest_draw.clone();
            let sequence_error = self.decoded_image_present_sequence_error.clone();
            let zero_copy_presented = sequence_error.is_none()
                && sequence.all_zero_copy_presented
                && sequence.presented_frame_count == sequence.requested_present_frame_count
                && draw.is_some();
            return NativeVulkanVulkanaliaRetainedPresentResult {
                sequence: Some(sequence),
                sequence_error: sequence_error.clone(),
                draw,
                draw_error: sequence_error,
                zero_copy_presented,
            };
        }

        let draw = self.present_decoded_image_once(fallback_sampled_array_layer);
        let (draw, draw_error) = match draw {
            Ok(snapshot) => (Some(snapshot), None),
            Err(err) => (None, Some(err)),
        };
        let zero_copy_presented = draw
            .as_ref()
            .is_some_and(|snapshot| snapshot.zero_copy_presented);
        NativeVulkanVulkanaliaRetainedPresentResult {
            sequence: None,
            sequence_error: self.decoded_image_present_sequence_error.clone(),
            draw,
            draw_error,
            zero_copy_presented,
        }
    }
}

struct NativeVulkanVulkanaliaRetainedPresentResult {
    sequence: Option<NativeVulkanVulkanaliaDecodedImagePresentSequenceSnapshot>,
    sequence_error: Option<String>,
    draw: Option<NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot>,
    draw_error: Option<String>,
    zero_copy_presented: bool,
}

impl Drop for NativeVulkanVulkanaliaVideoPresentSessionRuntimeResources {
    fn drop(&mut self) {
        if let Some(context) = self.context.take() {
            let device = &context.device;
            let _ = unsafe { device.device_wait_idle() };
            if let Some(pipeline) = self.decoded_image_present_pipeline.take() {
                native_vulkan_vulkanalia_destroy_decoded_image_present_pipeline_resources(
                    device, pipeline,
                );
            }
            if let Some(sampler) = self.decoded_image_present_sampler.take() {
                native_vulkan_vulkanalia_destroy_decoded_image_present_sampler_resources(
                    device, sampler,
                );
            }
            if let Some(scene_video_overlay) = self.scene_video_overlay.take() {
                native_vulkan_vulkanalia_destroy_scene_video_overlay_resources(
                    device,
                    scene_video_overlay,
                );
            }
            if let Some(resource_image) = self.resource_image.take() {
                native_vulkan_vulkanalia_destroy_video_session_resource_image(
                    device,
                    resource_image,
                );
            }
            if let Some(memory_resources) = self.memory_resources.take() {
                native_vulkan_vulkanalia_destroy_video_session_memory_binding_resources(
                    device,
                    memory_resources,
                );
            }
            native_vulkan_vulkanalia_destroy_video_session(device, self.session);
            unsafe {
                device.destroy_swapchain_khr(self.swapchain, None);
                context.device.destroy_device(None);
            }
        }

        if let Some(vulkan) = self.vulkan.take() {
            unsafe {
                vulkan.instance.destroy_surface_khr(self.surface, None);
            }
            native_vulkan_vulkanalia_destroy_instance(vulkan);
        }
    }
}

struct NativeVulkanVulkanaliaVideoPresentSessionPieces {
    session: vk::VideoSessionKHR,
    memory_resources: NativeVulkanVulkanaliaVideoSessionMemoryBindingResources,
    resource_image: VulkanaliaVideoSessionResourceImage,
    decoded_image_present_pipeline: Option<VulkanaliaDecodedImagePresentPipelineResources>,
    decoded_image_present_sampler: Option<VulkanaliaDecodedImagePresentSamplerResources>,
    scene_video_overlay: Option<VulkanaliaSceneVideoOverlayResources>,
    snapshot: NativeVulkanVulkanaliaVideoPresentSessionProbeSnapshot,
    decoded_image_present_sequence:
        Option<NativeVulkanVulkanaliaDecodedImagePresentSequenceSnapshot>,
    decoded_image_present_sequence_error: Option<String>,
    h264_ready_prefix_decode: Option<NativeVulkanVulkanaliaH264ReadyPrefixCommandSmokeSnapshot>,
    h265_ready_prefix_decode: Option<NativeVulkanVulkanaliaH265ReadyPrefixCommandSmokeSnapshot>,
    av1_ready_prefix_decode: Option<NativeVulkanVulkanaliaAv1CommandSmokeSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanVulkanaliaH264RetainedVideoPresentDecodeSnapshot {
    pub session: NativeVulkanVulkanaliaVideoPresentSessionProbeSnapshot,
    pub decode: NativeVulkanVulkanaliaH264ReadyPrefixCommandSmokeSnapshot,
    pub decoded_into_retained_resource_image: bool,
    pub decoded_image_present_sequence_requested: bool,
    pub decoded_image_present_sequence:
        Option<NativeVulkanVulkanaliaDecodedImagePresentSequenceSnapshot>,
    pub decoded_image_present_sequence_error: Option<String>,
    pub decoded_image_present_draw_requested: bool,
    pub decoded_image_present_draw: Option<NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot>,
    pub decoded_image_present_draw_error: Option<String>,
    pub decoded_image_zero_copy_presented: bool,
}

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, PartialEq)]
pub struct NativeVulkanVulkanaliaH264StreamingVideoPresentDecodeOptions {
    pub session: NativeVulkanVulkanaliaVideoPresentSessionProbeOptions,
    pub source: PathBuf,
    pub queue_capacity: usize,
    pub playback_frame_count: u32,
}

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, PartialEq)]
pub struct NativeVulkanVulkanaliaH265StreamingVideoPresentDecodeOptions {
    pub session: NativeVulkanVulkanaliaVideoPresentSessionProbeOptions,
    pub source: PathBuf,
    pub queue_capacity: usize,
    pub playback_frame_count: u32,
}

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, PartialEq)]
pub struct NativeVulkanVulkanaliaAv1StreamingVideoPresentDecodeOptions {
    pub session: NativeVulkanVulkanaliaVideoPresentSessionProbeOptions,
    pub source: PathBuf,
    pub queue_capacity: usize,
    pub playback_frame_count: u32,
}

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, PartialEq)]
pub struct NativeVulkanVulkanaliaStreamingVideoPresentDecodeSourceOptions {
    pub source: PathBuf,
    pub codec: NativeVulkanVideoSessionCodec,
    pub width: u32,
    pub height: u32,
    pub queue_capacity: usize,
    pub playback_frame_count: u32,
}

#[cfg(feature = "native-vulkan-video")]
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
    pub software_decode_fallback: bool,
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
    pub scene_video_overlay: Option<NativeVulkanVulkanaliaSceneVideoOverlayInput>,
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
    pub software_decode_fallback: bool,
    pub descriptor_heap_only: bool,
    pub zero_copy_scope: &'static str,
    pub ffmpeg_reference: &'static str,
}

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, PartialEq)]
pub struct NativeVulkanVulkanaliaMultiStreamingVideoPresentDecodeOptions {
    pub host: crate::renderer::native_wayland::NativeWaylandHostOptions,
    pub wait_configure_roundtrips: usize,
    pub target_max_fps: Option<u32>,
    pub audio_master_clock: NativeVulkanVulkanaliaVideoPresentAudioMasterClock,
    pub clear_color: NativeVulkanClearColor,
    pub sources: Vec<NativeVulkanVulkanaliaStreamingVideoPresentDecodeSourceOptions>,
}

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, PartialEq)]
struct NativeVulkanVulkanaliaMultiVideoDecodePlan {
    source_count: usize,
    codecs: Vec<NativeVulkanVideoSessionCodec>,
    requested_present_frame_count: u32,
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanVulkanaliaMultiVideoDecodePlan {
    fn from_sources(
        sources: &[NativeVulkanVulkanaliaStreamingVideoPresentDecodeSourceOptions],
    ) -> Result<Self, String> {
        if sources.is_empty() {
            return Err("multi-source scene video requires at least one source".to_owned());
        }
        let mut codecs = Vec::new();
        let mut requested_present_frame_count = 1u32;
        for source in sources {
            if !codecs.contains(&source.codec) {
                codecs.push(source.codec);
            }
            requested_present_frame_count =
                requested_present_frame_count.max(source.playback_frame_count.max(1));
        }
        Ok(Self {
            source_count: sources.len(),
            codecs,
            requested_present_frame_count,
        })
    }

    fn codecs(&self) -> &[NativeVulkanVideoSessionCodec] {
        &self.codecs
    }

    fn codec_count(&self) -> usize {
        self.codecs.len()
    }
}

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanVulkanaliaMultiStreamingVideoPresentDecodeSourceSnapshot {
    pub source_index: usize,
    pub source: PathBuf,
    pub codec: NativeVulkanVideoSessionCodec,
    pub requested_extent: (u32, u32),
    pub playback_frame_count: u32,
    pub decoded_into_retained_resource_image: bool,
    pub decoded_image_zero_copy_presented: bool,
}

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanVulkanaliaMultiStreamingVideoPresentDecodeSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub source_count: usize,
    pub codec_count: usize,
    pub codecs: Vec<NativeVulkanVideoSessionCodec>,
    pub surface_host: Option<NativeVulkanVideoSurfaceHostSnapshot>,
    pub sources: Vec<NativeVulkanVulkanaliaMultiStreamingVideoPresentDecodeSourceSnapshot>,
    pub decoded_image_present_sequence_requested: bool,
    pub decoded_image_present_sequence:
        Option<NativeVulkanVulkanaliaDecodedImagePresentSequenceSnapshot>,
    pub decoded_image_present_sequence_error: Option<String>,
    pub decoded_image_zero_copy_presented: bool,
    pub decoded_image_present_boundary: &'static str,
}

#[derive(Default)]
struct NativeVulkanVulkanaliaStreamingDecodeRequests {
    #[cfg(feature = "native-vulkan-video")]
    h264: Option<NativeVulkanVulkanaliaH264StreamingVideoPresentDecodeOptions>,
    #[cfg(feature = "native-vulkan-video")]
    h265: Option<NativeVulkanVulkanaliaH265StreamingVideoPresentDecodeOptions>,
    #[cfg(feature = "native-vulkan-video")]
    av1: Option<NativeVulkanVulkanaliaAv1StreamingVideoPresentDecodeOptions>,
}

#[cfg(feature = "native-vulkan-video")]
struct NativeVulkanVulkanaliaPreparedStreamingDecode {
    h264: Option<NativeVulkanVulkanaliaPreparedH264StreamingDecode>,
    h265: Option<NativeVulkanVulkanaliaPreparedH265StreamingDecode>,
    av1: Option<NativeVulkanVulkanaliaPreparedAv1StreamingDecode>,
}

#[cfg(feature = "native-vulkan-video")]
struct NativeVulkanVulkanaliaPreparedH264StreamingDecode {
    request: NativeVulkanVulkanaliaH264StreamingVideoPresentDecodeOptions,
    queue: NativeVulkanH264StreamingPacketQueue,
    parameter_sets: crate::renderer::native_vulkan::NativeVulkanH264ParameterSetSnapshot,
    bootstrap: NativeVulkanH264StreamingBootstrap,
}

#[cfg(feature = "native-vulkan-video")]
struct NativeVulkanVulkanaliaPreparedH265StreamingDecode {
    request: NativeVulkanVulkanaliaH265StreamingVideoPresentDecodeOptions,
    queue: NativeVulkanH265StreamingPacketQueue,
    parameter_sets: crate::renderer::native_vulkan::NativeVulkanH265ParameterSetSnapshot,
    bootstrap: NativeVulkanH265StreamingBootstrap,
}

#[cfg(feature = "native-vulkan-video")]
struct NativeVulkanVulkanaliaPreparedAv1StreamingDecode {
    request: NativeVulkanVulkanaliaAv1StreamingVideoPresentDecodeOptions,
    queue: NativeVulkanAv1StreamingPacketQueue,
    sequence_header: crate::renderer::native_vulkan::NativeVulkanAv1SequenceHeaderSnapshot,
    bootstrap: NativeVulkanAv1StreamingBootstrap,
}

#[cfg(feature = "native-vulkan-video")]
struct NativeVulkanVulkanaliaMultiVideoDecodeSourceSlot {
    source_index: usize,
    source: PathBuf,
    codec: NativeVulkanVideoSessionCodec,
    requested_extent: vk::Extent2D,
    picture_format: vk::Format,
    memory_properties: vk::PhysicalDeviceMemoryProperties,
    resource_image_array_layers: u32,
    session: vk::VideoSessionKHR,
    memory_resources: NativeVulkanVulkanaliaVideoSessionMemoryBindingResources,
    resource_image: VulkanaliaVideoSessionResourceImage,
    sampler: VulkanaliaDecodedImagePresentSamplerResources,
    decode_complete: vk::Semaphore,
    snapshot: NativeVulkanVulkanaliaMultiStreamingVideoPresentDecodeSourceSnapshot,
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

include!("streaming_decode_state/retention_and_sampler.rs");
