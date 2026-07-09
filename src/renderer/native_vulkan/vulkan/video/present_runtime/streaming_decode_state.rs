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
use crate::renderer::native_vulkan::video::codec_reference::{
    NativeVulkanAv1DecodeReferencePlanner, NativeVulkanAv1StreamingBootstrap,
    NativeVulkanH264DecodeReferencePlanner, NativeVulkanH264StreamingBootstrap,
    NativeVulkanH265DecodeReferencePlanner, NativeVulkanH265StreamingBootstrap,
    native_vulkan_av1_align_streaming_bootstrap, native_vulkan_h264_align_streaming_bootstrap,
    native_vulkan_h265_align_streaming_bootstrap,
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
    audio::clock::NativeVulkanAudioClockRuntimeSnapshot, audio::policy::NativeVulkanAudioOutputMode,
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
    release_ack: Option<mpsc::SyncSender<()>>,
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanFfmpegDecodedGpuFrameHandoff {
    fn new(
        decoded_frame: NativeVulkanFfmpegDecodedGpuFrame,
        release_ack: Option<mpsc::SyncSender<()>>,
    ) -> Self {
        Self {
            decoded_frame,
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

#[cfg(feature = "native-vulkan-video")]
struct NativeVulkanFfmpegPresentedFrameSetRetentionQueue<'a> {
    device: &'a Device,
    frame_resources: &'a VulkanaliaDecodedImagePresentFrameResources,
    frames: VecDeque<NativeVulkanFfmpegPresentedFrameSetRetention>,
    peak_frame_count: usize,
}

#[cfg(feature = "native-vulkan-video")]
impl<'a> NativeVulkanFfmpegPresentedFrameSetRetentionQueue<'a> {
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
        decoded_frames: Vec<NativeVulkanFfmpegDecodedGpuFrame>,
    ) -> Result<(), String> {
        self.release_completed_slot(present_frame_slot);
        self.frames
            .push_back(NativeVulkanFfmpegPresentedFrameSetRetention {
                present_frame_slot,
                _decoded_frames: decoded_frames,
            });
        self.peak_frame_count = self.peak_frame_count.max(self.retained_frame_ref_count());
        self.release_completed_frames()
    }

    fn release_completed_slot(&mut self, present_frame_slot: u32) {
        if let Some(index) = self
            .frames
            .iter()
            .position(|frame| frame.present_frame_slot == present_frame_slot)
        {
            self.frames.remove(index);
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
                self.frames.remove(index);
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
            drop(frame);
        }
        Ok(())
    }

    fn retained_frame_ref_count(&self) -> usize {
        self.frames.iter().fold(0usize, |sum, frame| {
            sum.saturating_add(frame._decoded_frames.len())
        })
    }

    fn frame_count(&self) -> u32 {
        self.retained_frame_ref_count().min(u32::MAX as usize) as u32
    }

    fn peak_frame_count(&self) -> u32 {
        self.peak_frame_count.min(u32::MAX as usize) as u32
    }
}

#[cfg(feature = "native-vulkan-video")]
impl Drop for NativeVulkanFfmpegPresentedFrameSetRetentionQueue<'_> {
    fn drop(&mut self) {
        if !self.frames.is_empty() {
            let _ = unsafe { self.device.device_wait_idle() };
        }
        self.frames.clear();
    }
}

#[cfg(feature = "native-vulkan-video")]
struct NativeVulkanFfmpegPresentSamplerCacheEntry {
    image: vk::Image,
    picture_format: vk::Format,
    array_layers: u32,
    sampler: VulkanaliaDecodedImagePresentSamplerResources,
}

#[cfg(feature = "native-vulkan-video")]
struct NativeVulkanFfmpegPresentSamplerCache<'a> {
    device: &'a Device,
    memory_properties: &'a vk::PhysicalDeviceMemoryProperties,
    video_queue_family_index: u32,
    present_queue_family_index: u32,
    descriptor_heap_enabled: bool,
    descriptor_heap_properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    entries: Vec<NativeVulkanFfmpegPresentSamplerCacheEntry>,
    descriptor_rewrite_count: u32,
    descriptor_recreate_count: u32,
    peak_entry_count: usize,
}

#[cfg(feature = "native-vulkan-video")]
impl<'a> NativeVulkanFfmpegPresentSamplerCache<'a> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        device: &'a Device,
        memory_properties: &'a vk::PhysicalDeviceMemoryProperties,
        video_queue_family_index: u32,
        present_queue_family_index: u32,
        descriptor_heap_enabled: bool,
        descriptor_heap_properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    ) -> Self {
        Self {
            device,
            memory_properties,
            video_queue_family_index,
            present_queue_family_index,
            descriptor_heap_enabled,
            descriptor_heap_properties,
            entries: Vec::new(),
            descriptor_rewrite_count: 0,
            descriptor_recreate_count: 0,
            peak_entry_count: 0,
        }
    }

    fn ensure_for_descriptor_source(
        &mut self,
        descriptor_source: &NativeVulkanFfmpegDecodedGpuFrameDescriptorSource,
    ) -> Result<usize, String> {
        let [plane] = descriptor_source.planes.as_slice() else {
            return Err(format!(
                "FFmpeg AVVkFrame sampler cache requires one multiplane image, got {}",
                descriptor_source.planes.len()
            ));
        };
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.image == plane.image)
        {
            let entry = self
                .entries
                .get_mut(index)
                .expect("image cache index came from position");
            if entry.picture_format == descriptor_source.picture_format
                && entry.array_layers == descriptor_source.array_layers
            {
                return Ok(index);
            }
            let entry = self.entries.remove(index);
            native_vulkan_vulkanalia_destroy_decoded_image_present_sampler_resources(
                self.device,
                entry.sampler,
            );
            self.descriptor_recreate_count = self.descriptor_recreate_count.saturating_add(1);
        }

        let sampler =
            native_vulkan_vulkanalia_create_ffmpeg_decoded_gpu_frame_present_sampler_resources(
                self.device,
                self.memory_properties,
                descriptor_source,
                0,
                self.video_queue_family_index,
                self.present_queue_family_index,
                self.descriptor_heap_enabled,
                self.descriptor_heap_properties,
            )?;
        self.entries
            .push(NativeVulkanFfmpegPresentSamplerCacheEntry {
                image: plane.image,
                picture_format: descriptor_source.picture_format,
                array_layers: descriptor_source.array_layers,
                sampler,
            });
        self.peak_entry_count = self.peak_entry_count.max(self.entries.len());
        Ok(self.entries.len().saturating_sub(1))
    }

    fn sampler(
        &self,
        index: usize,
    ) -> Result<&VulkanaliaDecodedImagePresentSamplerResources, String> {
        self.entries
            .get(index)
            .map(|entry| &entry.sampler)
            .ok_or_else(|| format!("FFmpeg AVVkFrame sampler cache index {index} is unavailable"))
    }

    fn descriptor_heap_plan(
        &self,
        index: usize,
    ) -> Result<&NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot, String> {
        self.entries
            .get(index)
            .map(|entry| &entry.sampler.snapshot.descriptor_heap_plan)
            .ok_or_else(|| {
                format!("FFmpeg AVVkFrame sampler cache index {index} has no descriptor plan")
            })
    }

    fn entry_count(&self) -> u32 {
        self.entries.len().min(u32::MAX as usize) as u32
    }

    fn peak_entry_count(&self) -> u32 {
        self.peak_entry_count.min(u32::MAX as usize) as u32
    }

    fn descriptor_rewrite_count(&self) -> u32 {
        self.descriptor_rewrite_count
    }

    fn descriptor_recreate_count(&self) -> u32 {
        self.descriptor_recreate_count
    }

    fn resource_heap_bytes(&self) -> u64 {
        self.entries.iter().fold(0u64, |sum, entry| {
            sum.saturating_add(entry.sampler.descriptor_heap.plan.resource_heap_bytes)
        })
    }

    fn sampler_heap_bytes(&self) -> u64 {
        self.entries.iter().fold(0u64, |sum, entry| {
            sum.saturating_add(entry.sampler.descriptor_heap.plan.sampler_heap_bytes)
        })
    }
}

#[cfg(feature = "native-vulkan-video")]
impl Drop for NativeVulkanFfmpegPresentSamplerCache<'_> {
    fn drop(&mut self) {
        for entry in self.entries.drain(..) {
            native_vulkan_vulkanalia_destroy_decoded_image_present_sampler_resources(
                self.device,
                entry.sampler,
            );
        }
    }
}

// Source slots are built before scoped workers start and destroyed only after
// all workers join. The descriptor heap mapped pointer is not mutated by decode
// workers; present only binds the immutable heap handles while decode writes the
// source's separate Vulkan Video image through queue synchronization.
#[cfg(feature = "native-vulkan-video")]
unsafe impl Send for NativeVulkanVulkanaliaMultiVideoDecodeSourceSlot {}

#[cfg(feature = "native-vulkan-video")]
unsafe impl Sync for NativeVulkanVulkanaliaMultiVideoDecodeSourceSlot {}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanVulkanaliaMultiVideoDecodeSourceSlot {
    fn decode_wait(
        &self,
        frame: NativeVulkanVulkanaliaDecodedPresentHandoffFrame,
    ) -> super::render_present::VulkanaliaDecodedImagePresentDecodeWait {
        super::render_present::VulkanaliaDecodedImagePresentDecodeWait {
            semaphore: self.decode_complete,
            value: frame.decode_complete_value,
        }
    }

    fn present_source(
        &self,
        frame: NativeVulkanVulkanaliaDecodedPresentHandoffFrame,
    ) -> super::render_present::VulkanaliaDecodedImagePresentSource<'_> {
        super::render_present::VulkanaliaDecodedImagePresentSource {
            image: super::render_present::VulkanaliaDecodedImagePresentImageSource {
                image: self.resource_image.image,
                array_layers: self.resource_image.snapshot.array_layers,
                current_layout: vk::ImageLayout::VIDEO_DECODE_DPB_KHR,
                restore_layout: vk::ImageLayout::VIDEO_DECODE_DPB_KHR,
                queue_family_index: vk::QUEUE_FAMILY_IGNORED,
            },
            sampler: &self.sampler,
            sampled_array_layer: frame.sampled_array_layer,
        }
    }
}

#[cfg(feature = "native-vulkan-video")]
fn destroy_multi_video_decode_source_slot(
    device: &Device,
    slot: NativeVulkanVulkanaliaMultiVideoDecodeSourceSlot,
) {
    native_vulkan_vulkanalia_destroy_decoded_image_present_sampler_resources(device, slot.sampler);
    unsafe {
        device.destroy_semaphore(slot.decode_complete, None);
    }
    native_vulkan_vulkanalia_destroy_video_session_resource_image(device, slot.resource_image);
    native_vulkan_vulkanalia_destroy_video_session_memory_binding_resources(
        device,
        slot.memory_resources,
    );
    native_vulkan_vulkanalia_destroy_video_session(device, slot.session);
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_vulkanalia_prepare_streaming_decode_requests(
    requests: NativeVulkanVulkanaliaStreamingDecodeRequests,
    codec: NativeVulkanVideoSessionCodec,
    session_max_dpb_slots: u32,
) -> Result<NativeVulkanVulkanaliaPreparedStreamingDecode, String> {
    let h264 = if let Some(request) = requests.h264 {
        if codec != NativeVulkanVideoSessionCodec::H264High8 {
            return Err(
                "H.264 streaming decode request does not match the video session codec".to_owned(),
            );
        }
        let mut queue = native_vulkan_start_h264_streaming_packet_queue(
            &request.source,
            request.queue_capacity.max(1),
        )
        .map_err(|err| err.to_string())?;
        let parameter_sets = queue.parameter_sets.clone();
        let bootstrap = native_vulkan_h264_align_streaming_bootstrap(&mut queue, &parameter_sets)
            .map_err(|err| err.to_string())?;
        native_vulkan_vulkanalia_require_streaming_dpb_slots(
            "H.264",
            bootstrap.stream_dpb_slots,
            session_max_dpb_slots,
        )?;
        Some(NativeVulkanVulkanaliaPreparedH264StreamingDecode {
            request,
            queue,
            parameter_sets,
            bootstrap,
        })
    } else {
        None
    };
    let h265 = if let Some(request) = requests.h265 {
        if !matches!(
            codec,
            NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::H265Main10
        ) {
            return Err(
                "H.265 streaming decode request does not match the video session codec".to_owned(),
            );
        }
        let mut queue = native_vulkan_start_h265_streaming_packet_queue(
            &request.source,
            request.queue_capacity.max(1),
        )
        .map_err(|err| err.to_string())?;
        let parameter_sets = queue.parameter_sets.clone();
        let bootstrap = native_vulkan_h265_align_streaming_bootstrap(&mut queue, &parameter_sets)
            .map_err(|err| err.to_string())?;
        native_vulkan_vulkanalia_require_streaming_dpb_slots(
            "H.265",
            bootstrap.stream_dpb_slots,
            session_max_dpb_slots,
        )?;
        Some(NativeVulkanVulkanaliaPreparedH265StreamingDecode {
            request,
            queue,
            parameter_sets,
            bootstrap,
        })
    } else {
        None
    };
    let av1 = if let Some(request) = requests.av1 {
        if !matches!(
            codec,
            NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10
        ) {
            return Err(
                "AV1 streaming decode request does not match the video session codec".to_owned(),
            );
        }
        let mut queue = native_vulkan_start_av1_streaming_packet_queue(
            &request.source,
            request.queue_capacity.max(1),
        )
        .map_err(|err| err.to_string())?;
        let sequence_header = queue.parameter_sets.clone();
        let bootstrap = native_vulkan_av1_align_streaming_bootstrap(&mut queue, &sequence_header)
            .map_err(|err| err.to_string())?;
        native_vulkan_vulkanalia_require_streaming_dpb_slots(
            "AV1",
            bootstrap.stream_dpb_slots,
            session_max_dpb_slots,
        )?;
        Some(NativeVulkanVulkanaliaPreparedAv1StreamingDecode {
            request,
            queue,
            sequence_header,
            bootstrap,
        })
    } else {
        None
    };
    Ok(NativeVulkanVulkanaliaPreparedStreamingDecode { h264, h265, av1 })
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_vulkanalia_streaming_decode_requests_for_source(
    source: &NativeVulkanVulkanaliaStreamingVideoPresentDecodeSourceOptions,
    session: NativeVulkanVulkanaliaVideoPresentSessionProbeOptions,
) -> NativeVulkanVulkanaliaStreamingDecodeRequests {
    match source.codec {
        NativeVulkanVideoSessionCodec::H264High8 => NativeVulkanVulkanaliaStreamingDecodeRequests {
            h264: Some(
                NativeVulkanVulkanaliaH264StreamingVideoPresentDecodeOptions {
                    session,
                    source: source.source.clone(),
                    queue_capacity: source.queue_capacity,
                    playback_frame_count: source.playback_frame_count,
                },
            ),
            h265: None,
            av1: None,
        },
        NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::H265Main10 => {
            NativeVulkanVulkanaliaStreamingDecodeRequests {
                h264: None,
                h265: Some(
                    NativeVulkanVulkanaliaH265StreamingVideoPresentDecodeOptions {
                        session,
                        source: source.source.clone(),
                        queue_capacity: source.queue_capacity,
                        playback_frame_count: source.playback_frame_count,
                    },
                ),
                av1: None,
            }
        }
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10 => {
            NativeVulkanVulkanaliaStreamingDecodeRequests {
                h264: None,
                h265: None,
                av1: Some(
                    NativeVulkanVulkanaliaAv1StreamingVideoPresentDecodeOptions {
                        session,
                        source: source.source.clone(),
                        queue_capacity: source.queue_capacity,
                        playback_frame_count: source.playback_frame_count,
                    },
                ),
            }
        }
    }
}

#[cfg(feature = "native-vulkan-video")]
#[allow(clippy::too_many_arguments)]
fn create_multi_video_decode_source_slot(
    instance: &Instance,
    context: &NativeVulkanVulkanaliaVideoPresentDeviceContext,
    selection: &super::video_present_device::NativeVulkanVulkanaliaVideoPresentPhysicalDeviceSelection,
    source_index: usize,
    source: NativeVulkanVulkanaliaStreamingVideoPresentDecodeSourceOptions,
    host: crate::renderer::native_wayland::NativeWaylandHostOptions,
    wait_configure_roundtrips: usize,
    target_max_fps: Option<u32>,
    audio_master_clock: NativeVulkanVulkanaliaVideoPresentAudioMasterClock,
    clear_color: NativeVulkanClearColor,
) -> Result<
    (
        NativeVulkanVulkanaliaMultiVideoDecodeSourceSlot,
        NativeVulkanVulkanaliaPreparedStreamingDecode,
    ),
    String,
> {
    if source.width == 0 || source.height == 0 {
        return Err(format!(
            "multi-source video source {} requires non-zero extent",
            source.source.display()
        ));
    }
    let session_options = NativeVulkanVulkanaliaVideoPresentSessionProbeOptions {
        host,
        wait_configure_roundtrips,
        codec: source.codec,
        width: source.width,
        height: source.height,
        target_max_fps,
        audio_master_clock,
        clear_color,
    };
    let requests =
        native_vulkan_vulkanalia_streaming_decode_requests_for_source(&source, session_options);
    with_native_vulkan_vulkanalia_video_session_capabilities(
        instance,
        selection.physical_device,
        source.codec,
        None,
        None,
        |profile_info, queried| {
            let driver_session_max_dpb_slots = native_vulkan_vulkanalia_video_session_max_dpb_slots(
                queried.capabilities.max_dpb_slots,
            );
            let driver_session_max_active_reference_pictures =
                native_vulkan_vulkanalia_video_session_max_active_reference_pictures(
                    queried.capabilities.max_active_reference_pictures,
                    driver_session_max_dpb_slots,
                );
            let prepared_streaming_decode =
                native_vulkan_vulkanalia_prepare_streaming_decode_requests(
                    requests,
                    source.codec,
                    driver_session_max_dpb_slots,
                )?;
            let requested_extent =
                prepared_streaming_decode
                    .coded_extent()
                    .unwrap_or(vk::Extent2D {
                        width: source.width,
                        height: source.height,
                    });
            let av1_sequence_header = prepared_streaming_decode.av1_sequence_header();
            if !native_vulkan_vulkanalia_video_session_extent_supported(
                requested_extent,
                queried.capabilities,
            ) {
                return Err(format!(
                    "multi-source video source {} extent {}x{} is outside driver capabilities",
                    source.source.display(),
                    requested_extent.width,
                    requested_extent.height
                ));
            }
            let required_dpb_slots =
                prepared_streaming_decode.required_resource_image_array_layers();
            let session_max_dpb_slots = native_vulkan_vulkanalia_select_stream_session_dpb_slots(
                required_dpb_slots,
                driver_session_max_dpb_slots,
            )?;
            let required_active_reference_pictures =
                prepared_streaming_decode.required_max_active_reference_pictures();
            let session_max_active_reference_pictures =
                native_vulkan_vulkanalia_select_stream_session_active_reference_pictures(
                    required_active_reference_pictures,
                    driver_session_max_active_reference_pictures,
                    session_max_dpb_slots,
                )?;
            let resource_image_array_layers =
                native_vulkan_vulkanalia_select_stream_resource_image_array_layers(
                    required_dpb_slots,
                    session_max_dpb_slots,
                )?;
            let picture_format = native_vulkan_vulkanalia_video_session_effective_picture_format(
                source.codec,
                av1_sequence_header,
            );
            let video_session_create_flags = native_vulkan_vulkanalia_video_session_create_flags(
                context
                    .video_feature_selection
                    .inline_session_parameters_enabled,
            );
            let create_info = vk::VideoSessionCreateInfoKHR::builder()
                .flags(video_session_create_flags)
                .queue_family_index(selection.video_queue_family_index)
                .video_profile(profile_info)
                .picture_format(picture_format)
                .reference_picture_format(picture_format)
                .max_coded_extent(requested_extent)
                .max_dpb_slots(session_max_dpb_slots)
                .max_active_reference_pictures(session_max_active_reference_pictures)
                .std_header_version(&queried.capabilities.std_header_version)
                .build();
            let session =
                native_vulkan_vulkanalia_create_video_session(&context.device, &create_info)?;
            let mut memory_resources = None;
            let mut resource_image = None;
            let mut sampler = None;
            let mut decode_complete = None;
            let result = (|| {
                let memory_properties = unsafe {
                    instance.get_physical_device_memory_properties(selection.physical_device)
                };
                memory_resources = Some(
                    native_vulkan_vulkanalia_bind_video_session_memory_resources(
                        &context.device,
                        &memory_properties,
                        session,
                    )?,
                );
                let resource_queue_family_indices = video_present_queue_family_indices(
                    selection.video_queue_family_index,
                    selection.present_queue_family_index,
                );
                resource_image = Some(
                    native_vulkan_vulkanalia_create_video_session_resource_image(
                        instance,
                        &context.device,
                        &memory_properties,
                        selection.physical_device,
                        profile_info,
                        requested_extent,
                        resource_image_array_layers,
                        picture_format,
                        queried.decode_capability_flags,
                        &resource_queue_family_indices,
                    )?,
                );
                sampler = Some(
                    native_vulkan_vulkanalia_create_decoded_image_present_sampler_resources(
                        &context.device,
                        &memory_properties,
                        resource_image
                            .as_ref()
                            .expect("multi-source resource image is live"),
                        picture_format,
                        0,
                        selection.video_queue_family_index,
                        selection.present_queue_family_index,
                        context
                            .video_feature_selection
                            .core_features
                            .descriptor_heap,
                        context.video_feature_selection.descriptor_heap_properties,
                    )?,
                );
                decode_complete = Some(native_vulkan_vulkanalia_create_decode_timeline_semaphore(
                    &context.device,
                )?);
                let snapshot =
                    NativeVulkanVulkanaliaMultiStreamingVideoPresentDecodeSourceSnapshot {
                        source_index,
                        source: source.source.clone(),
                        codec: source.codec,
                        requested_extent: (requested_extent.width, requested_extent.height),
                        playback_frame_count: source.playback_frame_count,
                        decoded_into_retained_resource_image: true,
                        decoded_image_zero_copy_presented: false,
                    };
                Ok((
                    NativeVulkanVulkanaliaMultiVideoDecodeSourceSlot {
                        source_index,
                        source: source.source.clone(),
                        codec: source.codec,
                        requested_extent,
                        picture_format,
                        memory_properties,
                        resource_image_array_layers,
                        session,
                        memory_resources: memory_resources
                            .take()
                            .expect("multi-source session memory is live"),
                        resource_image: resource_image
                            .take()
                            .expect("multi-source resource image is live"),
                        sampler: sampler.take().expect("multi-source sampler is live"),
                        decode_complete: decode_complete
                            .take()
                            .expect("multi-source decode semaphore is live"),
                        snapshot,
                    },
                    prepared_streaming_decode,
                ))
            })();
            if result.is_err() {
                if let Some(sampler) = sampler.take() {
                    native_vulkan_vulkanalia_destroy_decoded_image_present_sampler_resources(
                        &context.device,
                        sampler,
                    );
                }
                if let Some(decode_complete) = decode_complete.take() {
                    unsafe {
                        context.device.destroy_semaphore(decode_complete, None);
                    }
                }
                if let Some(resource_image) = resource_image.take() {
                    native_vulkan_vulkanalia_destroy_video_session_resource_image(
                        &context.device,
                        resource_image,
                    );
                }
                if let Some(memory_resources) = memory_resources.take() {
                    native_vulkan_vulkanalia_destroy_video_session_memory_binding_resources(
                        &context.device,
                        memory_resources,
                    );
                }
                native_vulkan_vulkanalia_destroy_video_session(&context.device, session);
            }
            result
        },
    )
}

#[cfg(not(feature = "native-vulkan-video"))]
fn native_vulkan_vulkanalia_prepare_streaming_decode_requests(
    _requests: NativeVulkanVulkanaliaStreamingDecodeRequests,
    _codec: NativeVulkanVideoSessionCodec,
    _session_max_dpb_slots: u32,
) -> Result<(), String> {
    Ok(())
}

fn native_vulkan_vulkanalia_require_streaming_dpb_slots(
    codec: &'static str,
    required_dpb_slots: u32,
    session_max_dpb_slots: u32,
) -> Result<(), String> {
    if session_max_dpb_slots == 0 || required_dpb_slots <= session_max_dpb_slots {
        return Ok(());
    }
    Err(format!(
        "{codec} streaming decode requires {required_dpb_slots} DPB slot(s), but the selected Vulkan video session exposes only {session_max_dpb_slots}"
    ))
}

#[cfg(feature = "native-vulkan-video")]
struct NativeVulkanVulkanaliaStreamingPtsState {
    source_loop_index: u32,
    pts_offset_ns: u64,
    loop_base_source_pts_ns: Option<u64>,
    last_adjusted_pts_ns: Option<u64>,
    last_duration_ns: Option<u64>,
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanVulkanaliaStreamingPtsState {
    fn new(source_loop_index: u32) -> Self {
        Self {
            source_loop_index,
            pts_offset_ns: 0,
            loop_base_source_pts_ns: None,
            last_adjusted_pts_ns: None,
            last_duration_ns: None,
        }
    }

    fn sync_loop(&mut self, source_loop_index: u32) -> bool {
        if source_loop_index == self.source_loop_index {
            return false;
        }
        self.source_loop_index = source_loop_index;
        self.pts_offset_ns = self
            .last_adjusted_pts_ns
            .map(|pts| pts.saturating_add(self.last_duration_ns.unwrap_or(1).max(1)))
            .unwrap_or(self.pts_offset_ns);
        self.loop_base_source_pts_ns = None;
        true
    }

    fn adjusted_pts_ns(
        &mut self,
        source_pts_ns: Option<u64>,
        source_pts_ms: Option<u64>,
        source_duration_ns: Option<u64>,
        source_duration_ms: Option<u64>,
    ) -> Option<u64> {
        let pts_ns =
            source_pts_ns.or_else(|| source_pts_ms.map(|pts| pts.saturating_mul(1_000_000)));
        let duration_ns = source_duration_ns
            .or_else(|| source_duration_ms.map(|duration| duration.saturating_mul(1_000_000)));
        let adjusted = pts_ns.map(|pts| {
            let base = *self.loop_base_source_pts_ns.get_or_insert(pts);
            pts.saturating_sub(base).saturating_add(self.pts_offset_ns)
        });
        if let Some(adjusted) = adjusted {
            self.last_adjusted_pts_ns = Some(adjusted);
        }
        if let Some(duration) = duration_ns {
            self.last_duration_ns = Some(duration);
        }
        adjusted
    }
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_vulkanalia_next_h264_streaming_frame(
    queue: &mut NativeVulkanH264StreamingPacketQueue,
    planner: &mut NativeVulkanH264DecodeReferencePlanner,
    pts_state: &mut NativeVulkanVulkanaliaStreamingPtsState,
) -> Result<NativeVulkanVulkanaliaH264ReadyPrefixFrameInput, String> {
    let packet = queue.next_packet(true).map_err(|err| err.to_string())?;
    if pts_state.sync_loop(packet.source_loop_index) {
        planner.reset();
    }
    let mut snapshot = packet.snapshot;
    let mut entry = planner.plan_next(&snapshot);
    let pts_ns = pts_state.adjusted_pts_ns(
        snapshot.pts_ns,
        snapshot.pts_ms,
        snapshot.duration_ns,
        snapshot.duration_ms,
    );
    entry.pts_ms = pts_ns.map(|pts| pts / 1_000_000).or(snapshot.pts_ms);
    if !entry.ready_for_decode_submit {
        let references = entry
            .references
            .iter()
            .map(|reference| {
                format!(
                    "frame_num={} slot={:?} available={} source_au={:?}",
                    reference.frame_num,
                    reference.dpb_slot,
                    reference.available,
                    reference.source_access_unit_index
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!(
            "Vulkanalia H.264 streaming AU {} is not decode-ready: {}; frame_num={:?}; requested_refs={}; available_refs={}; missing_refs={}; planned_output_slot={}; refs=[{}]",
            entry.access_unit_index,
            entry
                .unsupported_reason
                .as_deref()
                .unwrap_or("missing references"),
            entry.current_frame_num,
            entry.requested_reference_count,
            entry.available_reference_count,
            entry.missing_reference_count,
            entry.planned_output_slot,
            references
        ));
    }
    if let Some(err) = &snapshot.first_slice_parse_error {
        return Err(format!(
            "Vulkanalia H.264 streaming AU {} first slice parse failed: {err}",
            snapshot.index
        ));
    }
    let first_slice = snapshot.first_slice.take().ok_or_else(|| {
        format!(
            "Vulkanalia H.264 streaming AU {} has no parsed first slice",
            snapshot.index
        )
    })?;
    if first_slice.slice_offsets.is_empty() {
        return Err(format!(
            "Vulkanalia H.264 streaming AU {} has no slice offsets",
            snapshot.index
        ));
    }
    Ok(NativeVulkanVulkanaliaH264ReadyPrefixFrameInput {
        entry,
        first_slice,
        pts_ns,
        duration_ns: snapshot.duration_ns,
        duration_ms: snapshot.duration_ms,
        access_unit_payload: packet.access_unit.payload,
    })
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_vulkanalia_next_h265_streaming_frame(
    queue: &mut NativeVulkanH265StreamingPacketQueue,
    planner: &mut NativeVulkanH265DecodeReferencePlanner,
    pts_state: &mut NativeVulkanVulkanaliaStreamingPtsState,
) -> Result<NativeVulkanVulkanaliaH265ReadyPrefixFrameInput, String> {
    let packet = queue.next_packet(true).map_err(|err| err.to_string())?;
    if pts_state.sync_loop(packet.source_loop_index) {
        planner.reset_for_idr();
    }
    let mut snapshot = packet.snapshot;
    let mut entry = planner.plan_next(&snapshot);
    let pts_ns = pts_state.adjusted_pts_ns(
        snapshot.pts_ns,
        snapshot.pts_ms,
        snapshot.duration_ns,
        snapshot.duration_ms,
    );
    entry.pts_ms = pts_ns.map(|pts| pts / 1_000_000).or(snapshot.pts_ms);
    if !entry.ready_for_decode_submit {
        return Err(format!(
            "Vulkanalia H.265 streaming AU {} is not decode-ready; missing POCs {:?}",
            entry.access_unit_index, entry.missing_reference_pocs
        ));
    }
    if let Some(err) = &snapshot.first_slice_parse_error {
        return Err(format!(
            "Vulkanalia H.265 streaming AU {} first slice parse failed: {err}",
            snapshot.index
        ));
    }
    let first_slice = snapshot.first_slice.take().ok_or_else(|| {
        format!(
            "Vulkanalia H.265 streaming AU {} has no parsed first slice",
            snapshot.index
        )
    })?;
    let slice_segment_offset = first_slice.slice_segment_offset;
    Ok(NativeVulkanVulkanaliaH265ReadyPrefixFrameInput {
        entry,
        first_slice,
        pts_ns,
        duration_ns: snapshot.duration_ns,
        duration_ms: snapshot.duration_ms,
        access_unit_payload: packet.access_unit.payload,
        slice_segment_offset,
    })
}
