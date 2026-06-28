#![allow(dead_code)]

use std::collections::VecDeque;
#[cfg(feature = "native-vulkan-video")]
use std::path::PathBuf;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{
    self, HasBuilder, KhrSurfaceExtensionInstanceCommands, KhrSwapchainExtensionDeviceCommands,
};

use crate::renderer::native_vulkan::NativeVulkanClearColor;
use crate::renderer::native_vulkan::NativeVulkanVideoSessionCodec;
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
use crate::renderer::native_vulkan::video::vulkan_extract::native_vulkan_vulkanalia_av1_frame_submit_input_from_temporal_unit;
#[cfg(feature = "native-vulkan-video")]
use crate::renderer::native_vulkan::{
    NativeVulkanAv1ActiveDpbReference, NativeVulkanAv1StreamingPacketQueue,
    NativeVulkanH264StreamingPacketQueue, NativeVulkanH265StreamingPacketQueue,
    native_vulkan_av1_update_active_dpb_refs_after_display_handoff,
};
use crate::renderer::native_wayland::NativeWaylandHost;

use super::super::scene::present::{
    NativeVulkanVulkanaliaSceneVideoOverlayInput, VulkanaliaSceneVideoOverlayResources,
    native_vulkan_vulkanalia_create_scene_video_overlay_resources,
    native_vulkan_vulkanalia_destroy_scene_video_overlay_resources,
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
    VulkanaliaDecodedImagePresentTimingConfig,
    native_vulkan_vulkanalia_create_decoded_image_present_frame_resources,
    native_vulkan_vulkanalia_create_decoded_image_present_pipeline_resources,
    native_vulkan_vulkanalia_create_decoded_image_present_sampler_resources,
    native_vulkan_vulkanalia_decoded_image_present_command_pool,
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

pub(in crate::renderer::native_vulkan::vulkan) const VIDEO_PRESENT_SESSION_RETAINED_RESOURCE_ROUTE: &str =
    "video-present-session-retained-resource";
const FFMPEG_VIDEO_PICTURE_QUEUE_SIZE: usize = 3;
const DECODED_IMAGE_PRESENT_STARTUP_PREROLL_FRAMES: usize = 1;
const FFMPEG_SINGLE_DECODE_THREAD_COUNT: u32 = 1;
const FFMPEG_FFPLAY_FRAME_QUEUE_REFERENCE: &str =
    "references/ffmpeg/fftools/ffplay.c:125-179,2205-2210";
const FFMPEG_AV_SYNC_THRESHOLD_MAX: Duration = Duration::from_millis(100);
const DECODED_IMAGE_PRESENT_SLOW_FRAME_THRESHOLD_MICROS: u64 = 6_250;
const DECODED_IMAGE_PRESENT_SLOW_FRAME_TELEMETRY_LIMIT: usize = 0;
const VIDEO_PRESENT_SLEEP_GUARD: Duration = Duration::from_micros(300);
const VIDEO_PRESENT_SPIN_GUARD: Duration = Duration::from_micros(80);

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
    _host: NativeWaylandHost,
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

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_vulkanalia_next_av1_streaming_frame(
    queue: &mut NativeVulkanAv1StreamingPacketQueue,
    planner: &mut NativeVulkanAv1DecodeReferencePlanner,
    active_dpb_refs: &mut [Option<NativeVulkanAv1ActiveDpbReference>],
    sequence_header: &crate::renderer::native_vulkan::NativeVulkanAv1SequenceHeaderSnapshot,
    pts_state: &mut NativeVulkanVulkanaliaStreamingPtsState,
) -> Result<NativeVulkanVulkanaliaAv1StreamingFrameInput, String> {
    let packet = queue.next_packet(true).map_err(|err| err.to_string())?;
    if pts_state.sync_loop(packet.source_loop_index) {
        *planner = NativeVulkanAv1DecodeReferencePlanner::new(planner.dpb_slots);
        active_dpb_refs.fill(None);
    }
    let entry = planner.plan_next(&packet.snapshot);
    let pts_ns = pts_state.adjusted_pts_ns(
        packet.snapshot.pts_ns,
        packet.snapshot.pts_ms,
        packet.snapshot.duration_ns,
        packet.snapshot.duration_ms,
    );
    let pts_ms = pts_ns.map(|pts| pts / 1_000_000).or(packet.snapshot.pts_ms);
    if entry.ready_for_display_handoff {
        native_vulkan_av1_update_active_dpb_refs_after_display_handoff(active_dpb_refs, &entry)?;
        return Ok(NativeVulkanVulkanaliaAv1StreamingFrameInput {
            entry,
            frame: None,
            pts_ns,
            duration_ns: packet.snapshot.duration_ns,
            pts_ms,
            duration_ms: packet.snapshot.duration_ms,
            access_unit_payload: packet.access_unit.payload,
        });
    }
    if !entry.ready_for_decode_submit {
        return Err(format!(
            "Vulkanalia AV1 streaming TU {} is not decode-ready: {}",
            entry.temporal_unit_index,
            entry
                .unsupported_reason
                .as_deref()
                .unwrap_or("missing references or submit fields")
        ));
    }
    let frame = native_vulkan_vulkanalia_av1_frame_submit_input_from_temporal_unit(
        sequence_header,
        active_dpb_refs,
        &entry,
        &packet.snapshot,
        packet.access_unit.payload.bytes(),
    )
    .map_err(|err| err.to_string())?;
    Ok(NativeVulkanVulkanaliaAv1StreamingFrameInput {
        entry,
        frame: Some(frame),
        pts_ns,
        duration_ns: packet.snapshot.duration_ns,
        pts_ms,
        duration_ms: packet.snapshot.duration_ms,
        access_unit_payload: packet.access_unit.payload,
    })
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanVulkanaliaPreparedStreamingDecode {
    fn coded_extent(&self) -> Option<vk::Extent2D> {
        let (width, height) = self
            .h264
            .as_ref()
            .map(|prepared| {
                (
                    prepared.parameter_sets.sps.width,
                    prepared.parameter_sets.sps.height,
                )
            })
            .or_else(|| {
                self.h265.as_ref().map(|prepared| {
                    (
                        prepared.parameter_sets.sps.width,
                        prepared.parameter_sets.sps.height,
                    )
                })
            })
            .or_else(|| {
                self.av1.as_ref().map(|prepared| {
                    (
                        prepared.sequence_header.max_frame_width,
                        prepared.sequence_header.max_frame_height,
                    )
                })
            })?;
        let width = native_vulkan_vulkanalia_align16(width);
        let height = native_vulkan_vulkanalia_align16(height);
        (width > 0 && height > 0).then_some(vk::Extent2D { width, height })
    }

    fn av1_sequence_header(
        &self,
    ) -> Option<&crate::renderer::native_vulkan::NativeVulkanAv1SequenceHeaderSnapshot> {
        self.av1.as_ref().map(|prepared| &prepared.sequence_header)
    }

    fn required_resource_image_array_layers(&self) -> u32 {
        self.h264
            .as_ref()
            .map(|prepared| prepared.bootstrap.stream_dpb_slots)
            .or_else(|| {
                self.h265
                    .as_ref()
                    .map(|prepared| prepared.bootstrap.stream_dpb_slots)
            })
            .or_else(|| {
                self.av1
                    .as_ref()
                    .map(|prepared| prepared.bootstrap.stream_dpb_slots)
            })
            .unwrap_or(1)
            .max(1)
    }

    fn required_max_active_reference_pictures(&self) -> u32 {
        self.h264
            .as_ref()
            .map(|prepared| prepared.bootstrap.stream_max_active_reference_pictures)
            .or_else(|| {
                self.h265
                    .as_ref()
                    .map(|prepared| prepared.bootstrap.stream_max_active_reference_pictures)
            })
            .or_else(|| {
                self.av1
                    .as_ref()
                    .map(|prepared| prepared.bootstrap.stream_max_active_reference_pictures)
            })
            .unwrap_or(1)
            .max(1)
    }
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_vulkanalia_streaming_decode_requested(
    prepared: &NativeVulkanVulkanaliaPreparedStreamingDecode,
) -> bool {
    prepared.h264.is_some() || prepared.h265.is_some() || prepared.av1.is_some()
}

#[cfg(not(feature = "native-vulkan-video"))]
fn native_vulkan_vulkanalia_streaming_decode_requested(_prepared: &()) -> bool {
    false
}

#[cfg(not(feature = "native-vulkan-video"))]
trait NativeVulkanVulkanaliaNoStreamingDecodeLayers {
    fn coded_extent(&self) -> Option<vk::Extent2D>;
    fn av1_sequence_header(
        &self,
    ) -> Option<&crate::renderer::native_vulkan::NativeVulkanAv1SequenceHeaderSnapshot>;
    fn required_resource_image_array_layers(&self) -> u32;
    fn required_max_active_reference_pictures(&self) -> u32;
}

#[cfg(not(feature = "native-vulkan-video"))]
impl NativeVulkanVulkanaliaNoStreamingDecodeLayers for () {
    fn coded_extent(&self) -> Option<vk::Extent2D> {
        None
    }

    fn av1_sequence_header(
        &self,
    ) -> Option<&crate::renderer::native_vulkan::NativeVulkanAv1SequenceHeaderSnapshot> {
        None
    }

    fn required_resource_image_array_layers(&self) -> u32 {
        1
    }

    fn required_max_active_reference_pictures(&self) -> u32 {
        1
    }
}

fn native_vulkan_vulkanalia_align16(value: u32) -> u32 {
    value.div_ceil(16).saturating_mul(16)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanVulkanaliaH265RetainedVideoPresentDecodeSnapshot {
    pub session: NativeVulkanVulkanaliaVideoPresentSessionProbeSnapshot,
    pub decode: NativeVulkanVulkanaliaH265ReadyPrefixCommandSmokeSnapshot,
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

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanVulkanaliaAv1RetainedVideoPresentDecodeSnapshot {
    pub session: NativeVulkanVulkanaliaVideoPresentSessionProbeSnapshot,
    pub decode: NativeVulkanVulkanaliaAv1CommandSmokeSnapshot,
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

pub(in crate::renderer::native_vulkan::vulkan) fn probe_native_vulkan_vulkanalia_retained_video_present_session(
    options: NativeVulkanVulkanaliaVideoPresentSessionProbeOptions,
) -> Result<NativeVulkanVulkanaliaVideoPresentSessionProbeSnapshot, String> {
    let runtime = create_native_vulkan_vulkanalia_video_present_session_runtime(options)?;
    Ok(runtime.snapshot().clone())
}

#[cfg(feature = "native-vulkan-video")]
pub fn run_native_vulkan_vulkanalia_h264_streaming_video_present_decode(
    options: NativeVulkanVulkanaliaH264StreamingVideoPresentDecodeOptions,
) -> Result<NativeVulkanVulkanaliaH264RetainedVideoPresentDecodeSnapshot, String> {
    run_native_vulkan_vulkanalia_h264_streaming_video_present_decode_with_scene_video_overlay(
        options, None,
    )
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn run_native_vulkan_vulkanalia_h264_streaming_video_present_decode_with_scene_video_overlay(
    options: NativeVulkanVulkanaliaH264StreamingVideoPresentDecodeOptions,
    scene_video_overlay: Option<NativeVulkanVulkanaliaSceneVideoOverlayInput>,
) -> Result<NativeVulkanVulkanaliaH264RetainedVideoPresentDecodeSnapshot, String> {
    if options.session.codec != NativeVulkanVideoSessionCodec::H264High8 {
        return Err(
            "Vulkanalia streaming video-present decode currently supports H.264 high-8 only"
                .to_owned(),
        );
    }
    let playback_frame_count = options.playback_frame_count;
    let session_options = options.session.clone();
    let mut runtime =
        create_native_vulkan_vulkanalia_video_present_session_runtime_with_ready_prefix_decode(
            session_options,
            NativeVulkanVulkanaliaStreamingDecodeRequests {
                h264: Some(options),
                h265: None,
                av1: None,
            },
            playback_frame_count,
            scene_video_overlay,
        )?;
    let decode = runtime
        .resources
        .as_ref()
        .and_then(|resources| resources.h264_ready_prefix_decode.clone())
        .ok_or_else(|| {
            "Vulkanalia streaming H.264 video-present decode produced no decode snapshot".to_owned()
        })?;
    let present = runtime
        .resources
        .as_mut()
        .ok_or_else(|| "Vulkanalia retained runtime resources are unavailable".to_owned())?
        .decoded_image_present_result(decode.dst_base_array_layer);
    Ok(
        NativeVulkanVulkanaliaH264RetainedVideoPresentDecodeSnapshot {
            session: runtime.snapshot().clone(),
            decode,
            decoded_into_retained_resource_image: true,
            decoded_image_present_sequence_requested: true,
            decoded_image_present_sequence: present.sequence,
            decoded_image_present_sequence_error: present.sequence_error,
            decoded_image_present_draw_requested: true,
            decoded_image_present_draw: present.draw,
            decoded_image_present_draw_error: present.draw_error,
            decoded_image_zero_copy_presented: present.zero_copy_presented,
        },
    )
}

#[cfg(feature = "native-vulkan-video")]
pub fn run_native_vulkan_vulkanalia_h265_streaming_video_present_decode(
    options: NativeVulkanVulkanaliaH265StreamingVideoPresentDecodeOptions,
) -> Result<NativeVulkanVulkanaliaH265RetainedVideoPresentDecodeSnapshot, String> {
    run_native_vulkan_vulkanalia_h265_streaming_video_present_decode_with_scene_video_overlay(
        options, None,
    )
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn run_native_vulkan_vulkanalia_h265_streaming_video_present_decode_with_scene_video_overlay(
    options: NativeVulkanVulkanaliaH265StreamingVideoPresentDecodeOptions,
    scene_video_overlay: Option<NativeVulkanVulkanaliaSceneVideoOverlayInput>,
) -> Result<NativeVulkanVulkanaliaH265RetainedVideoPresentDecodeSnapshot, String> {
    if !matches!(
        options.session.codec,
        NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::H265Main10
    ) {
        return Err(
            "Vulkanalia streaming video-present decode currently supports H.265 only".to_owned(),
        );
    }
    let playback_frame_count = options.playback_frame_count;
    let session_options = options.session.clone();
    let mut runtime =
        create_native_vulkan_vulkanalia_video_present_session_runtime_with_ready_prefix_decode(
            session_options,
            NativeVulkanVulkanaliaStreamingDecodeRequests {
                h264: None,
                h265: Some(options),
                av1: None,
            },
            playback_frame_count,
            scene_video_overlay,
        )?;
    let decode = runtime
        .resources
        .as_ref()
        .and_then(|resources| resources.h265_ready_prefix_decode.clone())
        .ok_or_else(|| {
            "Vulkanalia streaming H.265 video-present decode produced no decode snapshot".to_owned()
        })?;
    let present = runtime
        .resources
        .as_mut()
        .ok_or_else(|| "Vulkanalia retained runtime resources are unavailable".to_owned())?
        .decoded_image_present_result(decode.dst_base_array_layer);
    Ok(
        NativeVulkanVulkanaliaH265RetainedVideoPresentDecodeSnapshot {
            session: runtime.snapshot().clone(),
            decode,
            decoded_into_retained_resource_image: true,
            decoded_image_present_sequence_requested: true,
            decoded_image_present_sequence: present.sequence,
            decoded_image_present_sequence_error: present.sequence_error,
            decoded_image_present_draw_requested: true,
            decoded_image_present_draw: present.draw,
            decoded_image_present_draw_error: present.draw_error,
            decoded_image_zero_copy_presented: present.zero_copy_presented,
        },
    )
}

#[cfg(feature = "native-vulkan-video")]
pub fn run_native_vulkan_vulkanalia_av1_streaming_video_present_decode(
    options: NativeVulkanVulkanaliaAv1StreamingVideoPresentDecodeOptions,
) -> Result<NativeVulkanVulkanaliaAv1RetainedVideoPresentDecodeSnapshot, String> {
    run_native_vulkan_vulkanalia_av1_streaming_video_present_decode_with_scene_video_overlay(
        options, None,
    )
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn run_native_vulkan_vulkanalia_av1_streaming_video_present_decode_with_scene_video_overlay(
    options: NativeVulkanVulkanaliaAv1StreamingVideoPresentDecodeOptions,
    scene_video_overlay: Option<NativeVulkanVulkanaliaSceneVideoOverlayInput>,
) -> Result<NativeVulkanVulkanaliaAv1RetainedVideoPresentDecodeSnapshot, String> {
    if !matches!(
        options.session.codec,
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10
    ) {
        return Err(
            "Vulkanalia streaming video-present decode currently supports AV1 only".to_owned(),
        );
    }
    let playback_frame_count = options.playback_frame_count;
    let session_options = options.session.clone();
    let mut runtime =
        create_native_vulkan_vulkanalia_video_present_session_runtime_with_ready_prefix_decode(
            session_options,
            NativeVulkanVulkanaliaStreamingDecodeRequests {
                h264: None,
                h265: None,
                av1: Some(options),
            },
            playback_frame_count,
            scene_video_overlay,
        )?;
    let decode = runtime
        .resources
        .as_ref()
        .and_then(|resources| resources.av1_ready_prefix_decode.clone())
        .ok_or_else(|| {
            "Vulkanalia streaming AV1 video-present decode produced no decode snapshot".to_owned()
        })?;
    let present = runtime
        .resources
        .as_mut()
        .ok_or_else(|| "Vulkanalia retained runtime resources are unavailable".to_owned())?
        .decoded_image_present_result(decode.dst_base_array_layer);
    Ok(
        NativeVulkanVulkanaliaAv1RetainedVideoPresentDecodeSnapshot {
            session: runtime.snapshot().clone(),
            decode,
            decoded_into_retained_resource_image: true,
            decoded_image_present_sequence_requested: true,
            decoded_image_present_sequence: present.sequence,
            decoded_image_present_sequence_error: present.sequence_error,
            decoded_image_present_draw_requested: true,
            decoded_image_present_draw: present.draw,
            decoded_image_present_draw_error: present.draw_error,
            decoded_image_zero_copy_presented: present.zero_copy_presented,
        },
    )
}

pub(in crate::renderer::native_vulkan::vulkan) fn create_native_vulkan_vulkanalia_video_present_session_runtime(
    options: NativeVulkanVulkanaliaVideoPresentSessionProbeOptions,
) -> Result<NativeVulkanVulkanaliaVideoPresentSessionRuntime, String> {
    create_native_vulkan_vulkanalia_video_present_session_runtime_with_ready_prefix_decode(
        options,
        NativeVulkanVulkanaliaStreamingDecodeRequests::default(),
        0,
        None,
    )
}

fn create_native_vulkan_vulkanalia_video_present_session_runtime_with_ready_prefix_decode(
    options: NativeVulkanVulkanaliaVideoPresentSessionProbeOptions,
    streaming_decode: NativeVulkanVulkanaliaStreamingDecodeRequests,
    requested_present_frame_count: u32,
    scene_video_overlay_input: Option<NativeVulkanVulkanaliaSceneVideoOverlayInput>,
) -> Result<NativeVulkanVulkanaliaVideoPresentSessionRuntime, String> {
    if options.width == 0 || options.height == 0 {
        return Err("Vulkanalia video present session runtime requires non-zero extent".to_owned());
    }

    let mut host =
        NativeWaylandHost::connect(options.host.clone()).map_err(|err| err.to_string())?;
    host.wait_until_configured(options.wait_configure_roundtrips)
        .map_err(|err| err.to_string())?;
    let handles = host.surface_handles().map_err(|err| err.to_string())?;

    let mut requested_instance_extensions = REQUIRED_INSTANCE_EXTENSIONS.to_vec();
    requested_instance_extensions.extend_from_slice(OPTIONAL_INSTANCE_EXTENSIONS);
    let vulkan = native_vulkan_vulkanalia_create_instance_with_required_extensions(
        &requested_instance_extensions,
    )?;
    let instance = &vulkan.instance;
    let surface = match create_vulkanalia_wayland_surface(instance, handles) {
        Ok(surface) => surface,
        Err(err) => {
            native_vulkan_vulkanalia_destroy_instance(vulkan);
            return Err(err);
        }
    };

    let physical_devices = match unsafe { instance.enumerate_physical_devices() } {
        Ok(physical_devices) => physical_devices,
        Err(err) => {
            unsafe {
                instance.destroy_surface_khr(surface, None);
            }
            native_vulkan_vulkanalia_destroy_instance(vulkan);
            return Err(format!(
                "vkEnumeratePhysicalDevices(vulkanalia video present runtime): {err:?}"
            ));
        }
    };
    let selection = match select_video_present_physical_device(
        instance,
        surface,
        handles,
        &physical_devices,
        options.codec,
    ) {
        Ok(selection) => selection,
        Err(err) => {
            unsafe {
                instance.destroy_surface_khr(surface, None);
            }
            native_vulkan_vulkanalia_destroy_instance(vulkan);
            return Err(err);
        }
    };
    let context = match create_video_present_device(
        instance,
        &selection,
        options.codec,
        vulkanalia_surface_maintenance1_enabled(&vulkan),
    ) {
        Ok(context) => context,
        Err(err) => {
            unsafe {
                instance.destroy_surface_khr(surface, None);
            }
            native_vulkan_vulkanalia_destroy_instance(vulkan);
            return Err(err);
        }
    };
    let swapchain_plan = match create_vulkanalia_swapchain_plan(
        instance,
        selection.physical_device,
        surface,
        handles.buffer_size,
        vulkanalia_surface_capabilities2_enabled(&vulkan),
        &context.present_feature_selection,
    ) {
        Ok(plan) => plan,
        Err(err) => {
            unsafe {
                context.device.destroy_device(None);
                instance.destroy_surface_khr(surface, None);
            }
            native_vulkan_vulkanalia_destroy_instance(vulkan);
            return Err(err);
        }
    };
    let swapchain = match unsafe {
        context
            .device
            .create_swapchain_khr(&swapchain_plan.create_info, None)
    } {
        Ok(swapchain) => swapchain,
        Err(err) => {
            unsafe {
                context.device.destroy_device(None);
                instance.destroy_surface_khr(surface, None);
            }
            native_vulkan_vulkanalia_destroy_instance(vulkan);
            return Err(format!(
                "vkCreateSwapchainKHR(vulkanalia retained video present): {err:?}"
            ));
        }
    };
    let swapchain_images = match unsafe { context.device.get_swapchain_images_khr(swapchain) } {
        Ok(images) => images,
        Err(err) => {
            unsafe {
                context.device.destroy_swapchain_khr(swapchain, None);
                context.device.destroy_device(None);
                instance.destroy_surface_khr(surface, None);
            }
            native_vulkan_vulkanalia_destroy_instance(vulkan);
            return Err(format!(
                "vkGetSwapchainImagesKHR(vulkanalia retained video present): {err:?}"
            ));
        }
    };

    // FFmpeg/ffplay drives cadence from the frame queue and PTS-derived refresh
    // timer (references/ffmpeg/fftools/ffplay.c:1609-1743,1796-1823). WSI
    // present-id2/wait2 still remain enabled for modern present telemetry and
    // optional diagnostic waits when the swapchain was created with them.
    let decoded_image_present_timing = VulkanaliaDecodedImagePresentTimingConfig::new(
        swapchain_plan.present_id2_enabled,
        swapchain_plan.present_wait2_enabled,
    );

    let pieces = match create_video_present_session_pieces(
        instance,
        &vulkan,
        &context,
        &selection,
        options.codec,
        options.width,
        options.height,
        swapchain,
        &swapchain_images,
        swapchain_plan.extent,
        swapchain_plan.format.format,
        options.target_max_fps,
        options.audio_master_clock,
        decoded_image_present_timing,
        options.clear_color,
        swapchain_plan_snapshot(&swapchain_plan, swapchain_images.len()),
        streaming_decode,
        requested_present_frame_count,
        scene_video_overlay_input,
    ) {
        Ok(pieces) => pieces,
        Err(err) => {
            unsafe {
                context.device.destroy_swapchain_khr(swapchain, None);
                context.device.destroy_device(None);
                instance.destroy_surface_khr(surface, None);
            }
            native_vulkan_vulkanalia_destroy_instance(vulkan);
            return Err(err);
        }
    };

    let NativeVulkanVulkanaliaVideoPresentSessionPieces {
        session,
        memory_resources,
        resource_image,
        decoded_image_present_pipeline,
        decoded_image_present_sampler,
        scene_video_overlay,
        snapshot,
        decoded_image_present_sequence,
        decoded_image_present_sequence_error,
        h264_ready_prefix_decode,
        h265_ready_prefix_decode,
        av1_ready_prefix_decode,
    } = pieces;
    Ok(NativeVulkanVulkanaliaVideoPresentSessionRuntime {
        resources: Some(NativeVulkanVulkanaliaVideoPresentSessionRuntimeResources {
            _host: host,
            vulkan: Some(vulkan),
            surface,
            context: Some(context),
            swapchain,
            swapchain_images,
            swapchain_format: swapchain_plan.format.format,
            swapchain_extent: swapchain_plan.extent,
            decoded_image_present_timing,
            clear_color: options.clear_color,
            present_queue_family_index: selection.present_queue_family_index,
            picture_format: native_vulkan_vulkanalia_video_session_effective_picture_format(
                options.codec,
                None,
            ),
            session,
            memory_resources: Some(memory_resources),
            resource_image: Some(resource_image),
            decoded_image_present_pipeline,
            decoded_image_present_sampler,
            scene_video_overlay,
            decoded_image_present_sequence,
            decoded_image_present_sequence_error,
            h264_ready_prefix_decode,
            h265_ready_prefix_decode,
            av1_ready_prefix_decode,
        }),
        snapshot,
    })
}

#[allow(clippy::too_many_arguments)]
fn create_video_present_session_pieces(
    instance: &Instance,
    vulkan: &NativeVulkanVulkanaliaInstance,
    context: &NativeVulkanVulkanaliaVideoPresentDeviceContext,
    selection: &super::video_present_device::NativeVulkanVulkanaliaVideoPresentPhysicalDeviceSelection,
    codec: NativeVulkanVideoSessionCodec,
    width: u32,
    height: u32,
    swapchain_handle: vk::SwapchainKHR,
    swapchain_images: &[vk::Image],
    swapchain_extent: vk::Extent2D,
    swapchain_format: vk::Format,
    target_max_fps: Option<u32>,
    audio_master_clock: NativeVulkanVulkanaliaVideoPresentAudioMasterClock,
    decoded_image_present_timing: VulkanaliaDecodedImagePresentTimingConfig,
    clear_color: NativeVulkanClearColor,
    swapchain: super::swapchain::NativeVulkanVulkanaliaSwapchainSnapshot,
    streaming_decode: NativeVulkanVulkanaliaStreamingDecodeRequests,
    requested_present_frame_count: u32,
    scene_video_overlay_input: Option<NativeVulkanVulkanaliaSceneVideoOverlayInput>,
) -> Result<NativeVulkanVulkanaliaVideoPresentSessionPieces, String> {
    with_native_vulkan_vulkanalia_video_session_capabilities(
        instance,
        selection.physical_device,
        codec,
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
            #[cfg(feature = "native-vulkan-video")]
            let mut prepared_streaming_decode =
                native_vulkan_vulkanalia_prepare_streaming_decode_requests(
                    streaming_decode,
                    codec,
                    driver_session_max_dpb_slots,
                )?;
            #[cfg(not(feature = "native-vulkan-video"))]
            let prepared_streaming_decode =
                native_vulkan_vulkanalia_prepare_streaming_decode_requests(
                    streaming_decode,
                    codec,
                    driver_session_max_dpb_slots,
                )?;
            let requested_extent = prepared_streaming_decode
                .coded_extent()
                .unwrap_or(vk::Extent2D { width, height });
            let av1_sequence_header = prepared_streaming_decode.av1_sequence_header();
            if !native_vulkan_vulkanalia_video_session_extent_supported(
                requested_extent,
                queried.capabilities,
            ) {
                return Err(format!(
                    "requested Vulkanalia video present session extent {}x{} is outside driver capabilities",
                    requested_extent.width, requested_extent.height
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
                codec,
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
            let mut session = Some(session);
            let mut memory_resources = None;
            let mut resource_image = None;
            let mut decoded_image_present_pipeline = None;
            let mut decoded_image_present_sampler = None;
            let mut decoded_image_present_frame_resources = None;
            let mut scene_video_overlay = None;
            let result = (|| -> Result<NativeVulkanVulkanaliaVideoPresentSessionPieces, String> {
                let memory_properties = unsafe {
                    instance.get_physical_device_memory_properties(selection.physical_device)
                };
                let resources = native_vulkan_vulkanalia_bind_video_session_memory_resources(
                    &context.device,
                    &memory_properties,
                    session
                        .as_ref()
                        .copied()
                        .expect("Vulkanalia video session is live"),
                )?;
                memory_resources = Some(resources);
                let resource_queue_family_indices = video_present_queue_family_indices(
                    selection.video_queue_family_index,
                    selection.present_queue_family_index,
                );
                let image = native_vulkan_vulkanalia_create_video_session_resource_image(
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
                )?;
                resource_image = Some(image);
                let resource_image_ref = resource_image
                    .as_ref()
                    .expect("Vulkanalia resource image has just been created");
                let resource_image_snapshot =
                    NativeVulkanVulkanaliaVideoSessionResourceImageSmokeSnapshot {
                        image_created: true,
                        memory_bound: true,
                        image_view_created: resource_image_ref.view != vk::ImageView::default(),
                        layer_view_count: resource_image_ref.layer_views.len(),
                        resource_image: resource_image_ref.snapshot.clone(),
                    };
                let same_queue_family =
                    selection.video_queue_family_index == selection.present_queue_family_index;
                let same_queue_handle =
                    same_queue_family && context.video_queue_index == context.present_queue_index;
                let (decoded_image_present_sampler_snapshot, decoded_image_present_sampler_error) =
                    match native_vulkan_vulkanalia_create_decoded_image_present_sampler_resources(
                        &context.device,
                        &memory_properties,
                        resource_image_ref,
                        picture_format,
                        0,
                        selection.video_queue_family_index,
                        selection.present_queue_family_index,
                        context
                            .video_feature_selection
                            .core_features
                            .descriptor_heap,
                        context.video_feature_selection.descriptor_heap_properties,
                    ) {
                        Ok(resources) => {
                            let snapshot = resources.snapshot.clone();
                            decoded_image_present_sampler = Some(resources);
                            (Some(snapshot), None)
                        }
                        Err(err) => (None, Some(err)),
                    };
                let (decoded_image_present_pipeline_snapshot, decoded_image_present_pipeline_error) =
                    if !context.video_feature_selection.dynamic_rendering_enabled {
                        (
                            None,
                            Some(
                                "dynamicRendering feature is unavailable on selected Vulkanalia video+present device"
                                    .to_owned(),
                            ),
                        )
                    } else if let Some(sampler) = decoded_image_present_sampler.as_ref() {
                        let target_extent = vk::Extent2D {
                            width: swapchain.extent.0,
                            height: swapchain.extent.1,
                        };
                        match native_vulkan_vulkanalia_create_decoded_image_present_pipeline_resources(
                            &context.device,
                            swapchain_format,
                            target_extent,
                            &sampler.snapshot.descriptor_heap_plan,
                        ) {
                            Ok(resources) => {
                                let snapshot = resources.snapshot.clone();
                                decoded_image_present_pipeline = Some(resources);
                                (Some(snapshot), None)
                            }
                            Err(err) => (None, Some(err)),
                        }
                    } else {
                        (
                            None,
                            Some(
                                "decoded image present pipeline requires a live plane descriptor-heap sampler resource"
                                    .to_owned(),
                            ),
                        )
                    };
                let decoded_image_present_sequence_requested =
                    native_vulkan_vulkanalia_streaming_decode_requested(&prepared_streaming_decode);
                let mut decoded_image_present_sequence_error = None;
                let mut decoded_image_present_sequence = None;
                if decoded_image_present_sequence_requested {
                    if decoded_image_present_sampler.is_none() {
                        decoded_image_present_sequence_error = Some(
                            "decoded image present sequence requires a live plane descriptor-heap sampler resource"
                                .to_owned(),
                        );
                    } else if decoded_image_present_pipeline.is_none() {
                        decoded_image_present_sequence_error = Some(
                            "decoded image present sequence requires a live dynamic-rendering pipeline"
                                .to_owned(),
                        );
                    } else {
                        match native_vulkan_vulkanalia_create_decoded_image_present_frame_resources(
                            &context.device,
                            swapchain_images,
                            swapchain_format,
                            selection.present_queue_family_index,
                        ) {
                            Ok(resources) => {
                                decoded_image_present_frame_resources = Some(resources);
                            }
                            Err(err) => {
                                decoded_image_present_sequence_error = Some(err);
                            }
                        }
                    }
                }
                if let Some(scene_video_overlay_input) = scene_video_overlay_input {
                    if decoded_image_present_sequence_error.is_none() {
                        if let Some(frame_resources) =
                            decoded_image_present_frame_resources.as_ref()
                        {
                            scene_video_overlay =
                                native_vulkan_vulkanalia_create_scene_video_overlay_resources(
                                    &context.device,
                                    &memory_properties,
                                    native_vulkan_vulkanalia_decoded_image_present_command_pool(
                                        frame_resources,
                                    ),
                                    context.present_queue,
                                    swapchain_format,
                                    swapchain_extent,
                                    native_vulkan_vulkanalia_decoded_image_present_frame_slot_count(
                                        frame_resources,
                                    ),
                                    context
                                        .video_feature_selection
                                        .core_features
                                        .texture_compression_bc,
                                    context
                                        .video_feature_selection
                                        .core_features
                                        .descriptor_heap,
                                    context.video_feature_selection.descriptor_heap_properties,
                                    scene_video_overlay_input,
                                )?;
                        } else {
                            decoded_image_present_sequence_error = Some(
                                "scene video overlay requires decoded-image present frame resources"
                                    .to_owned(),
                            );
                        }
                    }
                }
                let memory_binding = memory_resources
                    .as_ref()
                    .expect("Vulkanalia session memory resources are live")
                    .snapshot
                    .clone();
                let (h264_ready_prefix_decode, h265_ready_prefix_decode, av1_ready_prefix_decode) = {
                    let requested_present_frame_count_for_sequence =
                        requested_present_frame_count.max(1);
                    let sequence_started_at = Instant::now();
                    let mut sequence_builder = if decoded_image_present_sequence_error.is_none()
                        && decoded_image_present_sequence_requested
                    {
                        Some(
                            NativeVulkanVulkanaliaDecodedImagePresentSequenceBuilder::new(
                                requested_present_frame_count_for_sequence,
                                sequence_started_at,
                            ),
                        )
                    } else {
                        None
                    };
                    let present_handoff = NativeVulkanVulkanaliaDecodedPresentHandoff::new(
                        FFMPEG_VIDEO_PICTURE_QUEUE_SIZE,
                        resource_image_array_layers as usize,
                    );
                    #[cfg(feature = "native-vulkan-video")]
                    let ffmpeg_decode_async_exec_depth =
                        native_vulkan_vulkanalia_ffmpeg_decode_async_exec_depth(
                            selection.video_queue_count,
                        );
                    let queue_host_access_mutex = Mutex::new(());
                    let queue_host_access_lock =
                        same_queue_handle.then_some(&queue_host_access_mutex);
                    #[cfg(feature = "native-vulkan-video")]
                    let decode_async_exec_depth_for_sequence = ffmpeg_decode_async_exec_depth;
                    #[cfg(not(feature = "native-vulkan-video"))]
                    let decode_async_exec_depth_for_sequence = 0;
                    let sequence_execution_evidence =
                        NativeVulkanVulkanaliaDecodedImagePresentExecutionEvidence {
                            ffmpeg_read_thread_active: decoded_image_present_sequence_requested,
                            video_decode_worker_active: decoded_image_present_sequence_requested,
                            present_worker_active: sequence_builder.is_some(),
                            decode_thread_count: FFMPEG_SINGLE_DECODE_THREAD_COUNT,
                            decode_async_exec_depth: decode_async_exec_depth_for_sequence,
                        };
                    // Persistent timeline semaphore shared by the decode submits and the
                    // present submits. Seed the per-frame counter from its current value so
                    // signalled values stay strictly increasing across present sequences.
                    let decode_complete_semaphore = decoded_image_present_frame_resources
                        .as_ref()
                        .map(|frame_resources| frame_resources.decode_complete_semaphore())
                        .unwrap_or_else(vk::Semaphore::null);
                    #[cfg(feature = "native-vulkan-video")]
                    let decode_complete_value = std::cell::Cell::new(
                        if decode_complete_semaphore != vk::Semaphore::null() {
                            unsafe {
                                context
                                    .device
                                    .get_semaphore_counter_value(decode_complete_semaphore)
                            }
                            .map_err(|err| {
                                format!("vkGetSemaphoreCounterValue(decode_complete): {err:?}")
                            })?
                        } else {
                            0
                        },
                    );
                    let mut completed_sequence_builder = None;
                    let (
                        h264_ready_prefix_decode,
                        h265_ready_prefix_decode,
                        av1_ready_prefix_decode,
                    ) = thread::scope(|scope| -> Result<_, String> {
                        let present_worker = if let Some(mut worker_sequence_builder) =
                            sequence_builder.take()
                        {
                            let worker_handoff = present_handoff.clone();
                            let resource_image_ref = resource_image
                                .as_ref()
                                .expect("Vulkanalia resource image is live");
                            let sampler =
                                decoded_image_present_sampler.as_mut().ok_or_else(|| {
                                    "Vulkanalia decoded image present sampler is unavailable"
                                        .to_owned()
                                })?;
                            let pipeline =
                                decoded_image_present_pipeline.as_ref().ok_or_else(|| {
                                    "Vulkanalia decoded image present pipeline is unavailable"
                                        .to_owned()
                                })?;
                            let frame_resources = decoded_image_present_frame_resources
                                .as_ref()
                                .ok_or_else(|| {
                                    "decoded image present sequence has no reusable frame resources"
                                        .to_owned()
                                })?;
                            let mut scene_video_overlay = scene_video_overlay.as_mut();
                            let device = &context.device;
                            let present_queue = context.present_queue;
                            Some(
                                thread::Builder::new()
                                    .name("gilder-ffmpeg-video-present-worker".to_owned())
                                    .stack_size(256 * 1024)
                                    .spawn_scoped(scope, move || {
                                let worker_result = (|| -> Result<_, String> {
                                    let mut present_frame_index = 0u32;
                                    let mut present_frame_timer =
                                        NativeVulkanVulkanaliaPresentFrameTimer::new(
                                            target_max_fps,
                                            audio_master_clock,
                                        );
                                    let mut pending_present_frame_slots = VecDeque::<u32>::new();
                                    let mut first_frame_preroll_pending = true;
                                    loop {
                                        let frame = if first_frame_preroll_pending {
                                            first_frame_preroll_pending = false;
                                            worker_handoff.recv_after_preroll(
                                                DECODED_IMAGE_PRESENT_STARTUP_PREROLL_FRAMES,
                                            )?
                                        } else {
                                            match worker_handoff.try_recv()? {
                                                Some(frame) => Some(frame),
                                                None => {
                                                    if pending_present_frame_slots.is_empty() {
                                                        worker_handoff.recv()?
                                                    } else {
                                                        match worker_handoff
                                                            .recv_or_release_waiter()?
                                                        {
                                                            NativeVulkanVulkanaliaDecodedPresentHandoffRecv::Frame(frame) => {
                                                                Some(frame)
                                                            }
                                                            NativeVulkanVulkanaliaDecodedPresentHandoffRecv::ReleaseWaiter => {
                                                                let Some(present_frame_slot) =
                                                                    pending_present_frame_slots.pop_front()
                                                                else {
                                                                    continue;
                                                                };
                                                                native_vulkan_vulkanalia_wait_decoded_image_present_frame_slot(
                                                                    device,
                                                                    frame_resources,
                                                                    present_frame_slot,
                                                                )?;
                                                                worker_handoff.complete_present_frame_slot_releases(
                                                                    present_frame_slot,
                                                                )?;
                                                                continue;
                                                            }
                                                            NativeVulkanVulkanaliaDecodedPresentHandoffRecv::Closed => None,
                                                        }
                                                    }
                                                }
                                            }
                                        };
                                        let Some(frame) = frame else {
                                            break;
                                        };
                                        if present_frame_index
                                            >= requested_present_frame_count_for_sequence
                                        {
                                            worker_handoff
                                                .mark_frame_released(frame.sampled_array_layer)?;
                                            continue;
                                        }
                                        if present_frame_index == 0 {
                                            let worker_sequence_started_at = Instant::now();
                                            worker_sequence_builder.started_at =
                                                worker_sequence_started_at;
                                            present_frame_timer.reset(worker_sequence_started_at);
                                        }
                                        let present_frame_slot_count =
                                            native_vulkan_vulkanalia_decoded_image_present_frame_slot_count(
                                                frame_resources,
                                            )
                                            .max(1);
                                        let present_frame_slot =
                                            present_frame_index as usize % present_frame_slot_count;
                                        native_vulkan_vulkanalia_prepare_decoded_image_present_frame_slot(
                                            device,
                                            frame_resources,
                                            present_frame_slot as u32,
                                        )?;
                                        worker_handoff.complete_present_frame_slot_releases(
                                            present_frame_slot as u32,
                                        )?;
                                        if let Some(position) = pending_present_frame_slots
                                            .iter()
                                            .position(|slot| *slot == present_frame_slot as u32)
                                        {
                                            pending_present_frame_slots.remove(position);
                                        }
                                        if frame.sampled_array_layer
                                            >= resource_image_ref.snapshot.array_layers
                                        {
                                            return Err(format!(
                                                "decoded image present sampled layer {} exceeds {} image layers",
                                                frame.sampled_array_layer,
                                                resource_image_ref.snapshot.array_layers
                                            ));
                                        }
                                        let (pacing_sleep_micros, pacing_clock_model) =
                                            present_frame_timer.pace_frame(
                                                present_frame_index,
                                                frame.source_frame_pts_ns,
                                                frame.source_frame_duration_ns,
                                                frame.source_frame_pts_ms,
                                                frame.source_frame_duration_ms,
                                            );
                                        let mut record_layer_present_release =
                                            |present_frame_slot: u32| {
                                                worker_handoff.record_layer_present_release(
                                                    frame.sampled_array_layer,
                                                    present_frame_slot,
                                                )
                                            };
                                        let overlay_elapsed_ms = worker_sequence_builder
                                            .started_at
                                            .elapsed()
                                            .as_millis()
                                            .min(u128::from(u64::MAX))
                                            as u64;
                                        let scene_overlay_draw =
                                            if let Some(scene_video_overlay) =
                                                scene_video_overlay.as_deref_mut()
                                            {
                                                scene_video_overlay.frame_draw(
                                                    device,
                                                    present_frame_slot,
                                                    overlay_elapsed_ms,
                                                    swapchain_extent,
                                                )?
                                            } else {
                                                None
                                            };
                                        let draw =
                                            native_vulkan_vulkanalia_present_decoded_image_frame(
                                                device,
                                                present_queue,
                                                swapchain_handle,
                                                swapchain_images,
                                                swapchain_format,
                                                swapchain_extent,
                                                resource_image_ref,
                                                sampler,
                                                pipeline,
                                                frame_resources,
                                                frame.sampled_array_layer,
                                                present_frame_index,
                                                true,
                                                frame.source_frame_pts_ns,
                                                frame.source_frame_duration_ns,
                                                frame.source_frame_pts_ms,
                                                frame.source_frame_duration_ms,
                                                frame.display_order_key,
                                                frame.display_order_key_source,
                                                pacing_sleep_micros,
                                                pacing_clock_model,
                                                decoded_image_present_timing,
                                                decode_complete_semaphore,
                                                frame.decode_complete_value,
                                                queue_host_access_lock,
                                                Some(&mut record_layer_present_release),
                                                clear_color,
                                                scene_overlay_draw,
                                            )?;
                                        pending_present_frame_slots.push_back(draw.present_frame_slot);
                                        // FFmpeg FrameQueue releases the displayed AVFrame as
                                        // soon as display handoff has advanced; for Vulkan, keep
                                        // the decoded layer only until the render fence signals,
                                        // not until the same WSI frame slot is reused
                                        // (references/ffmpeg/fftools/ffplay.c:788-800,
                                        // references/ffmpeg/fftools/ffplay_renderer.c:780-786).
                                        let mut pending_slot_index = 0usize;
                                        while pending_slot_index < pending_present_frame_slots.len()
                                        {
                                            let present_frame_slot =
                                                pending_present_frame_slots[pending_slot_index];
                                            if native_vulkan_vulkanalia_try_complete_decoded_image_present_frame_slot(
                                                device,
                                                frame_resources,
                                                present_frame_slot,
                                            )? {
                                                pending_present_frame_slots
                                                    .remove(pending_slot_index);
                                                worker_handoff
                                                    .complete_present_frame_slot_releases(
                                                        present_frame_slot,
                                                    )?;
                                            } else {
                                                pending_slot_index += 1;
                                            }
                                        }
                                        worker_sequence_builder.push(draw);
                                        present_frame_index =
                                            present_frame_index.saturating_add(1);
                                    }
                                    while let Some(present_frame_slot) =
                                        pending_present_frame_slots.pop_front()
                                    {
                                        native_vulkan_vulkanalia_wait_decoded_image_present_frame_slot(
                                            device,
                                            frame_resources,
                                            present_frame_slot,
                                        )?;
                                        worker_handoff.complete_present_frame_slot_releases(
                                            present_frame_slot,
                                        )?;
                                    }
                                    let present_frame_slot_count =
                                        native_vulkan_vulkanalia_decoded_image_present_frame_slot_count(
                                            frame_resources,
                                        );
                                    for present_frame_slot in 0..present_frame_slot_count {
                                        native_vulkan_vulkanalia_wait_decoded_image_present_frame_slot(
                                            device,
                                            frame_resources,
                                            present_frame_slot as u32,
                                        )?;
                                        worker_handoff.complete_present_frame_slot_releases(
                                            present_frame_slot as u32,
                                        )?;
                                    }
                                    Ok(worker_sequence_builder)
                                })();
                                if let Err(err) = &worker_result {
                                    worker_handoff.fail(err.clone());
                                }
                                worker_result
                                    })
                                    .map_err(|err| {
                                        format!(
                                            "spawn FFmpeg-style video present worker: {err}"
                                        )
                                    })?,
                            )
                        } else {
                            None
                        };

                        #[cfg(feature = "native-vulkan-video")]
                        let decode_handoff = present_handoff.clone();
                        #[cfg(feature = "native-vulkan-video")]
                        let decoded_image_present_sequence_failed =
                            decoded_image_present_sequence_error.is_some();
                        #[cfg(feature = "native-vulkan-video")]
                        let decode_device = &context.device;
                        #[cfg(feature = "native-vulkan-video")]
                        let decode_video_queue = context.video_queue;
                        #[cfg(feature = "native-vulkan-video")]
                        let decode_video_queue_family_index = selection.video_queue_family_index;
                        #[cfg(feature = "native-vulkan-video")]
                        let decode_capabilities = queried.capabilities;
                        #[cfg(feature = "native-vulkan-video")]
                        let decode_memory_properties = &memory_properties;
                        #[cfg(feature = "native-vulkan-video")]
                        let decode_video_session = session
                            .as_ref()
                            .copied()
                            .expect("Vulkanalia video session is live");
                        #[cfg(feature = "native-vulkan-video")]
                        let decode_resource_image = resource_image
                            .as_ref()
                            .expect("Vulkanalia resource image is live");
                        #[cfg(feature = "native-vulkan-video")]
                        let decode_non_coherent_atom_size =
                            selection.properties.limits.non_coherent_atom_size;
                        #[cfg(feature = "native-vulkan-video")]
                        let decode_codec = codec;
                        let decode_worker = thread::Builder::new()
                            .name("gilder-ffmpeg-video-decode-worker".to_owned())
                            .stack_size(256 * 1024)
                            .spawn_scoped(scope, move || {
                            #[cfg(feature = "native-vulkan-video")]
                            let mut wait_for_output_slot_present_release =
                                |sampled_array_layer: u32| -> Result<(), String> {
                                    decode_handoff
                                        .wait_layer_present_release_completed(sampled_array_layer)
                                };
                            #[cfg(feature = "native-vulkan-video")]
                            let mut enqueue_decoded_frame =
                                |decode_frame_index: u32,
                                 sampled_array_layer: u32,
                                 source_frame_pts_ns: Option<u64>,
                                 source_frame_duration_ns: Option<u64>,
                                 source_frame_pts_ms: Option<u64>,
                                 source_frame_duration_ms: Option<u64>,
                                 display_order_key: i64,
                                 display_order_key_source: &'static str,
                                 decode_complete_value: u64|
                                 -> Result<(), String> {
                                    if decoded_image_present_sequence_failed {
                                        return Ok(());
                                    }
                                    if decode_frame_index
                                        >= requested_present_frame_count_for_sequence
                                    {
                                        return Ok(());
                                    }
                                    decode_handoff.enqueue(
                                        NativeVulkanVulkanaliaDecodedPresentHandoffFrame {
                                            decode_frame_index,
                                            sampled_array_layer,
                                            source_frame_pts_ns,
                                            source_frame_duration_ns,
                                            source_frame_pts_ms,
                                            source_frame_duration_ms,
                                            display_order_key,
                                            display_order_key_source,
                                            decode_complete_value,
                                        },
                                    )
                                };
                            (|| -> Result<_, String> {
                            #[cfg(feature = "native-vulkan-video")]
                            let h264_ready_prefix_decode = if let Some(prepared) =
                                prepared_streaming_decode.h264.take()
                            {
                                let NativeVulkanVulkanaliaPreparedH264StreamingDecode {
                                    request,
                                    mut queue,
                                    parameter_sets,
                                    bootstrap,
                                } = prepared;
                                let mut planner = NativeVulkanH264DecodeReferencePlanner::new(
                                    resource_image_array_layers,
                                    bootstrap.stream_max_active_reference_pictures,
                                    bootstrap.max_frame_num,
                                    parameter_sets.sps.gaps_in_frame_num_value_allowed_flag,
                                );
                                let mut pts_state =
                                    NativeVulkanVulkanaliaStreamingPtsState::new(queue.loop_count);
                                let mut next_frame = || {
                                    native_vulkan_vulkanalia_next_h264_streaming_frame(
                                        &mut queue,
                                        &mut planner,
                                        &mut pts_state,
                                    )
                                };
                                Some(
                                            native_vulkan_vulkanalia_record_h264_streaming_decode_into_image(
                                                decode_device,
                                                decode_video_queue,
                                                queue_host_access_lock,
                                                decode_memory_properties,
                                                decode_video_queue_family_index,
                                                profile_info,
                                                requested_extent,
                                                decode_capabilities,
                                                decode_video_session,
                                                decode_codec,
                                                resource_image_array_layers,
                                                ffmpeg_decode_async_exec_depth,
                                                decode_non_coherent_atom_size,
                                                NativeVulkanVulkanaliaH264StreamingDecodeInput {
                                                    parameter_sets,
                                                    requested_frame_count: request.playback_frame_count,
                                                    next_frame: &mut next_frame,
                                                },
                                                decode_resource_image,
                                                Some(&mut wait_for_output_slot_present_release),
                                                Some(&mut enqueue_decoded_frame),
                                                decode_complete_semaphore,
                                                &decode_complete_value,
                                            )?,
                                        )
                            } else {
                                None
                            };
                            #[cfg(not(feature = "native-vulkan-video"))]
                            let h264_ready_prefix_decode = None;
                            #[cfg(feature = "native-vulkan-video")]
                            let h265_ready_prefix_decode = if let Some(prepared) =
                                prepared_streaming_decode.h265.take()
                            {
                                let NativeVulkanVulkanaliaPreparedH265StreamingDecode {
                                    request,
                                    mut queue,
                                    parameter_sets,
                                    bootstrap,
                                } = prepared;
                                let mut planner = NativeVulkanH265DecodeReferencePlanner::new(
                                    resource_image_array_layers,
                                    bootstrap.stream_max_pic_order_cnt_lsb,
                                );
                                let mut pts_state =
                                    NativeVulkanVulkanaliaStreamingPtsState::new(queue.loop_count);
                                let mut next_frame = || {
                                    native_vulkan_vulkanalia_next_h265_streaming_frame(
                                        &mut queue,
                                        &mut planner,
                                        &mut pts_state,
                                    )
                                };
                                Some(
                                            native_vulkan_vulkanalia_record_h265_streaming_decode_into_image(
                                                decode_device,
                                                decode_video_queue,
                                                queue_host_access_lock,
                                                decode_memory_properties,
                                                decode_video_queue_family_index,
                                                profile_info,
                                                requested_extent,
                                                decode_capabilities,
                                                decode_video_session,
                                                decode_codec,
                                                resource_image_array_layers,
                                                ffmpeg_decode_async_exec_depth,
                                                decode_non_coherent_atom_size,
                                                NativeVulkanVulkanaliaH265StreamingDecodeInput {
                                                    parameter_sets,
                                                    requested_frame_count: request.playback_frame_count,
                                                    next_frame: &mut next_frame,
                                                },
                                                decode_resource_image,
                                                Some(&mut wait_for_output_slot_present_release),
                                                Some(&mut enqueue_decoded_frame),
                                                decode_complete_semaphore,
                                                &decode_complete_value,
                                            )?,
                                        )
                            } else {
                                None
                            };
                            #[cfg(not(feature = "native-vulkan-video"))]
                            let h265_ready_prefix_decode = None;
                            #[cfg(feature = "native-vulkan-video")]
                            let av1_ready_prefix_decode = if let Some(prepared) =
                                prepared_streaming_decode.av1.take()
                            {
                                let NativeVulkanVulkanaliaPreparedAv1StreamingDecode {
                                    request,
                                    mut queue,
                                    sequence_header,
                                    bootstrap: _,
                                } = prepared;
                                let av1_planner_dpb_slots = resource_image_array_layers.max(1);
                                let mut planner = NativeVulkanAv1DecodeReferencePlanner::new(
                                    av1_planner_dpb_slots,
                                );
                                let mut active_dpb_refs =
                                    vec![
                                        None::<NativeVulkanAv1ActiveDpbReference>;
                                        av1_planner_dpb_slots as usize
                                    ];
                                let mut pts_state =
                                    NativeVulkanVulkanaliaStreamingPtsState::new(queue.loop_count);
                                let mut next_frame = || {
                                    native_vulkan_vulkanalia_next_av1_streaming_frame(
                                        &mut queue,
                                        &mut planner,
                                        &mut active_dpb_refs,
                                        &sequence_header,
                                        &mut pts_state,
                                    )
                                };
                                Some(
                                        native_vulkan_vulkanalia_record_av1_streaming_decode_into_image(
                                            decode_device,
                                            decode_video_queue,
                                            queue_host_access_lock,
                                            decode_memory_properties,
                                            decode_video_queue_family_index,
                                            profile_info,
                                            requested_extent,
                                            decode_capabilities,
                                            decode_video_session,
                                            decode_codec,
                                            resource_image_array_layers,
                                            ffmpeg_decode_async_exec_depth,
                                            decode_non_coherent_atom_size,
                                            NativeVulkanVulkanaliaAv1StreamingDecodeInput {
                                                sequence_header: sequence_header.clone(),
                                                requested_frame_count: request.playback_frame_count,
                                                next_frame: &mut next_frame,
                                            },
                                            decode_resource_image,
                                            Some(&mut wait_for_output_slot_present_release),
                                            Some(&mut enqueue_decoded_frame),
                                            decode_complete_semaphore,
                                            &decode_complete_value,
                                        )?,
                                    )
                            } else {
                                None
                            };
                            #[cfg(not(feature = "native-vulkan-video"))]
                            let av1_ready_prefix_decode = None;
                            Ok((
                                h264_ready_prefix_decode,
                                h265_ready_prefix_decode,
                                av1_ready_prefix_decode,
                            ))
                            })()
                            })
                            .map_err(|err| {
                                format!("spawn FFmpeg-style video decode worker: {err}")
                            })?;
                        let decode_result = match decode_worker.join() {
                            Ok(result) => result,
                            Err(_) => Err("video decode worker panicked".to_owned()),
                        };
                        let close_result = present_handoff.close();
                        let present_result = if let Some(present_worker) = present_worker {
                            match present_worker.join() {
                                Ok(result) => result.map(Some),
                                Err(_) => Err("decoded image present worker panicked".to_owned()),
                            }
                        } else {
                            Ok(None)
                        };
                        close_result?;
                        let decode_result = decode_result?;
                        if let Some(builder) = present_result? {
                            completed_sequence_builder = Some(builder);
                        }
                        Ok(decode_result)
                    })?;
                    sequence_builder = completed_sequence_builder;
                    if let Some(sequence_builder) = sequence_builder.take() {
                        let handoff_snapshot = present_handoff.snapshot(
                            "decoded-image-present-worker-layer-ring",
                            "FFmpeg FrameQueue-style decoded-frame handoff: decode enqueues FIFO metadata into a fixed 3-frame ring and present starts as soon as the first display frame is available",
                            "no frame drop in ready-prefix evidence; decoded layer reuse waits on render-fence/final-drain completion instead of retaining stale copied frames",
                            "present worker drains FIFO metadata carrying FFmpeg-style PTS/POC/order-hint keys without a startup preroll gate; decoded layer release is fence driven",
                            "frame pixels are sampled from the Vulkan decode image through VK_EXT_descriptor_heap, then the swapchain image owns the displayed result",
                            FFMPEG_FFPLAY_FRAME_QUEUE_REFERENCE,
                        )?;
                        decoded_image_present_sequence =
                            sequence_builder.finish(handoff_snapshot, sequence_execution_evidence);
                    }
                    (
                        h264_ready_prefix_decode,
                        h265_ready_prefix_decode,
                        av1_ready_prefix_decode,
                    )
                };
                Ok(NativeVulkanVulkanaliaVideoPresentSessionPieces {
                    session: session.take().expect("Vulkanalia video session is live"),
                    memory_resources: memory_resources
                        .take()
                        .expect("Vulkanalia session memory resources are live"),
                    resource_image: resource_image
                        .take()
                        .expect("Vulkanalia resource image is live"),
                    decoded_image_present_pipeline: decoded_image_present_pipeline.take(),
                    decoded_image_present_sampler: decoded_image_present_sampler.take(),
                    scene_video_overlay: scene_video_overlay.take(),
                    decoded_image_present_sequence,
                    decoded_image_present_sequence_error,
                    h264_ready_prefix_decode,
                    snapshot: NativeVulkanVulkanaliaVideoPresentSessionProbeSnapshot {
                        binding: "vulkanalia",
                        route: VIDEO_PRESENT_SESSION_RETAINED_RESOURCE_ROUTE,
                        codec,
                        requested_extent: (requested_extent.width, requested_extent.height),
                        device: device_snapshot_from_selection(
                            vulkan, selection, context, codec, swapchain,
                        ),
                        video_session_created: true,
                        video_session_create_inline_session_parameters: context
                            .video_feature_selection
                            .inline_session_parameters_enabled,
                        video_session_create_flags_bits: video_session_create_flags.bits(),
                        memory_binding,
                        resource_image: resource_image_snapshot,
                        picture_format: format!("{picture_format:?}"),
                        decode_capability_flags: video_decode_capability_flag_labels(
                            queried.decode_capability_flags,
                        ),
                        session_max_dpb_slots,
                        session_max_active_reference_pictures,
                        resource_queue_family_indices,
                        resource_queue_sharing_model: decoded_image_resource_sharing_model(
                            same_queue_family,
                        ),
                        decoded_image_zero_copy_presentable_candidate: true,
                        decoded_image_present_sampler: decoded_image_present_sampler_snapshot,
                        decoded_image_present_sampler_error,
                        decoded_image_present_pipeline: decoded_image_present_pipeline_snapshot,
                        decoded_image_present_pipeline_error,
                        decoded_image_present_boundary: "retained Vulkanalia runtime owns video session memory, coincident sampled DPB/output image, descriptor-heap Y/UV plane sampler resources, and Wayland swapchain until the caller drops the runtime; next step records the dynamic-rendering fullscreen draw into the graphics present pass",
                        ffmpeg_reference: FFMPEG_VULKAN_DECODE_REFERENCE,
                    },
                    h265_ready_prefix_decode,
                    av1_ready_prefix_decode,
                })
            })();

            if let Some(scene_video_overlay) = scene_video_overlay.take() {
                native_vulkan_vulkanalia_destroy_scene_video_overlay_resources(
                    &context.device,
                    scene_video_overlay,
                );
            }
            if let Some(frame_resources) = decoded_image_present_frame_resources.take() {
                native_vulkan_vulkanalia_destroy_decoded_image_present_frame_resources(
                    &context.device,
                    frame_resources,
                );
            }
            if let Some(pipeline) = decoded_image_present_pipeline.take() {
                native_vulkan_vulkanalia_destroy_decoded_image_present_pipeline_resources(
                    &context.device,
                    pipeline,
                );
            }
            if let Some(sampler) = decoded_image_present_sampler.take() {
                native_vulkan_vulkanalia_destroy_decoded_image_present_sampler_resources(
                    &context.device,
                    sampler,
                );
            }
            if let Some(image) = resource_image.take() {
                native_vulkan_vulkanalia_destroy_video_session_resource_image(
                    &context.device,
                    image,
                );
            }
            if let Some(resources) = memory_resources.take() {
                native_vulkan_vulkanalia_destroy_video_session_memory_binding_resources(
                    &context.device,
                    resources,
                );
            }
            if let Some(session) = session.take() {
                native_vulkan_vulkanalia_destroy_video_session(&context.device, session);
            }

            result
        },
    )
}

struct NativeVulkanVulkanaliaDecodedImagePresentSequenceBuilder {
    requested_present_frame_count: u32,
    started_at: Instant,
    first_presented_at: Option<Instant>,
    last_presented_at: Option<Instant>,
    present_delta_min_micros: Option<u64>,
    present_delta_max_micros: Option<u64>,
    present_delta_over_6250us_count: u32,
    present_delta_over_8334us_count: u32,
    slow_frames: Vec<NativeVulkanVulkanaliaDecodedImagePresentSlowFrameSnapshot>,
    submitted_present_frame_count: u32,
    presented_frame_count: u32,
    frame_sleep_count: u32,
    missed_frame_pacing_count: u32,
    total_pacing_sleep_micros: u64,
    total_present_call_micros: u64,
    max_present_call_micros: u64,
    total_present_wait_frame_slot_micros: u64,
    max_present_wait_frame_slot_micros: u64,
    total_present_acquire_next_image_micros: u64,
    max_present_acquire_next_image_micros: u64,
    total_present_record_command_buffer_micros: u64,
    max_present_record_command_buffer_micros: u64,
    total_present_submit_command_buffer_micros: u64,
    max_present_submit_command_buffer_micros: u64,
    total_present_queue_present_micros: u64,
    max_present_queue_present_micros: u64,
    total_present_wait_after_queue_present_micros: u64,
    max_present_wait_after_queue_present_micros: u64,
    pts_monotonic: bool,
    last_pts_ns: Option<u64>,
    source_frame_pts_delta_min_ns: Option<u64>,
    source_frame_pts_delta_max_ns: Option<u64>,
    last_pts_ms: Option<u64>,
    source_frame_pts_delta_min_ms: Option<u64>,
    source_frame_pts_delta_max_ms: Option<u64>,
    display_order_monotonic: bool,
    last_display_order_key: Option<i64>,
    uses_present_id2: bool,
    present_wait2_available: bool,
    present_wait_after_present: bool,
    all_zero_copy_presented: bool,
    sampled_array_layer_mask: u128,
    latest_draw: Option<NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot>,
    draws_head: Vec<NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot>,
    draws_tail: Vec<NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot>,
}

#[derive(Debug, Clone, Copy)]
struct NativeVulkanVulkanaliaDecodedImagePresentExecutionEvidence {
    ffmpeg_read_thread_active: bool,
    video_decode_worker_active: bool,
    present_worker_active: bool,
    decode_thread_count: u32,
    decode_async_exec_depth: u32,
}

impl NativeVulkanVulkanaliaDecodedImagePresentSequenceBuilder {
    fn new(requested_present_frame_count: u32, started_at: Instant) -> Self {
        Self {
            requested_present_frame_count,
            started_at,
            first_presented_at: None,
            last_presented_at: None,
            present_delta_min_micros: None,
            present_delta_max_micros: None,
            present_delta_over_6250us_count: 0,
            present_delta_over_8334us_count: 0,
            slow_frames: Vec::new(),
            submitted_present_frame_count: 0,
            presented_frame_count: 0,
            frame_sleep_count: 0,
            missed_frame_pacing_count: 0,
            total_pacing_sleep_micros: 0,
            total_present_call_micros: 0,
            max_present_call_micros: 0,
            total_present_wait_frame_slot_micros: 0,
            max_present_wait_frame_slot_micros: 0,
            total_present_acquire_next_image_micros: 0,
            max_present_acquire_next_image_micros: 0,
            total_present_record_command_buffer_micros: 0,
            max_present_record_command_buffer_micros: 0,
            total_present_submit_command_buffer_micros: 0,
            max_present_submit_command_buffer_micros: 0,
            total_present_queue_present_micros: 0,
            max_present_queue_present_micros: 0,
            total_present_wait_after_queue_present_micros: 0,
            max_present_wait_after_queue_present_micros: 0,
            pts_monotonic: true,
            last_pts_ns: None,
            source_frame_pts_delta_min_ns: None,
            source_frame_pts_delta_max_ns: None,
            last_pts_ms: None,
            source_frame_pts_delta_min_ms: None,
            source_frame_pts_delta_max_ms: None,
            display_order_monotonic: true,
            last_display_order_key: None,
            uses_present_id2: false,
            present_wait2_available: false,
            present_wait_after_present: false,
            all_zero_copy_presented: true,
            sampled_array_layer_mask: 0,
            latest_draw: None,
            draws_head: Vec::with_capacity(DECODED_IMAGE_PRESENT_TELEMETRY_RETAINED_FRAMES),
            draws_tail: Vec::with_capacity(DECODED_IMAGE_PRESENT_TELEMETRY_RETAINED_FRAMES),
        }
    }

    fn push(&mut self, draw: NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot) {
        if draw.submitted {
            self.submitted_present_frame_count =
                self.submitted_present_frame_count.saturating_add(1);
        }
        if draw.presented {
            let presented_at = Instant::now();
            self.first_presented_at.get_or_insert(presented_at);
            if let Some(last_presented_at) = self.last_presented_at {
                let delta_micros =
                    duration_micros_u64(presented_at.saturating_duration_since(last_presented_at));
                self.present_delta_min_micros = Some(
                    self.present_delta_min_micros
                        .map(|current| current.min(delta_micros))
                        .unwrap_or(delta_micros),
                );
                self.present_delta_max_micros = Some(
                    self.present_delta_max_micros
                        .map(|current| current.max(delta_micros))
                        .unwrap_or(delta_micros),
                );
                if delta_micros > 6_250 {
                    self.present_delta_over_6250us_count =
                        self.present_delta_over_6250us_count.saturating_add(1);
                }
                if delta_micros > 8_334 {
                    self.present_delta_over_8334us_count =
                        self.present_delta_over_8334us_count.saturating_add(1);
                }
                if delta_micros > DECODED_IMAGE_PRESENT_SLOW_FRAME_THRESHOLD_MICROS
                    && self.slow_frames.len() < DECODED_IMAGE_PRESENT_SLOW_FRAME_TELEMETRY_LIMIT
                {
                    self.slow_frames.push(
                        NativeVulkanVulkanaliaDecodedImagePresentSlowFrameSnapshot {
                            present_frame_index: draw.present_frame_index,
                            present_frame_slot: draw.present_frame_slot,
                            sampled_array_layer: draw.sampled_array_layer,
                            delta_micros,
                            present_call_total_micros: draw.present_call_total_micros,
                            present_record_command_buffer_micros: draw
                                .present_record_command_buffer_micros,
                            present_submit_command_buffer_micros: draw
                                .present_submit_command_buffer_micros,
                            present_queue_present_micros: draw.present_queue_present_micros,
                            present_wait_frame_slot_micros: draw.present_wait_frame_slot_micros,
                            source_frame_pts_ns: draw.source_frame_pts_ns,
                            display_order_key: draw.display_order_key,
                        },
                    );
                }
            }
            self.last_presented_at = Some(presented_at);
            self.presented_frame_count = self.presented_frame_count.saturating_add(1);
        }
        self.total_pacing_sleep_micros = self
            .total_pacing_sleep_micros
            .saturating_add(draw.pacing_sleep_micros);
        if draw.pacing_sleep_micros > 0 {
            self.frame_sleep_count = self.frame_sleep_count.saturating_add(1);
        }
        if draw.pacing_clock_model == "audio-clock-master-video-late-no-sleep" {
            self.missed_frame_pacing_count = self.missed_frame_pacing_count.saturating_add(1);
        }
        self.total_present_call_micros = self
            .total_present_call_micros
            .saturating_add(draw.present_call_total_micros);
        self.max_present_call_micros = self
            .max_present_call_micros
            .max(draw.present_call_total_micros);
        self.total_present_wait_frame_slot_micros = self
            .total_present_wait_frame_slot_micros
            .saturating_add(draw.present_wait_frame_slot_micros);
        self.max_present_wait_frame_slot_micros = self
            .max_present_wait_frame_slot_micros
            .max(draw.present_wait_frame_slot_micros);
        self.total_present_acquire_next_image_micros = self
            .total_present_acquire_next_image_micros
            .saturating_add(draw.present_acquire_next_image_micros);
        self.max_present_acquire_next_image_micros = self
            .max_present_acquire_next_image_micros
            .max(draw.present_acquire_next_image_micros);
        self.total_present_record_command_buffer_micros = self
            .total_present_record_command_buffer_micros
            .saturating_add(draw.present_record_command_buffer_micros);
        self.max_present_record_command_buffer_micros = self
            .max_present_record_command_buffer_micros
            .max(draw.present_record_command_buffer_micros);
        self.total_present_submit_command_buffer_micros = self
            .total_present_submit_command_buffer_micros
            .saturating_add(draw.present_submit_command_buffer_micros);
        self.max_present_submit_command_buffer_micros = self
            .max_present_submit_command_buffer_micros
            .max(draw.present_submit_command_buffer_micros);
        self.total_present_queue_present_micros = self
            .total_present_queue_present_micros
            .saturating_add(draw.present_queue_present_micros);
        self.max_present_queue_present_micros = self
            .max_present_queue_present_micros
            .max(draw.present_queue_present_micros);
        self.total_present_wait_after_queue_present_micros = self
            .total_present_wait_after_queue_present_micros
            .saturating_add(draw.present_wait_after_queue_present_micros);
        self.max_present_wait_after_queue_present_micros = self
            .max_present_wait_after_queue_present_micros
            .max(draw.present_wait_after_queue_present_micros);
        if let Some(pts_ms) = draw.source_frame_pts_ms {
            if let Some(last) = self.last_pts_ms {
                if last > pts_ms {
                    self.pts_monotonic = false;
                } else {
                    let delta = pts_ms.saturating_sub(last);
                    if delta > 0 {
                        self.source_frame_pts_delta_min_ms = Some(
                            self.source_frame_pts_delta_min_ms
                                .map(|current| current.min(delta))
                                .unwrap_or(delta),
                        );
                        self.source_frame_pts_delta_max_ms = Some(
                            self.source_frame_pts_delta_max_ms
                                .map(|current| current.max(delta))
                                .unwrap_or(delta),
                        );
                    }
                }
            }
            self.last_pts_ms = Some(pts_ms);
        }
        if self
            .last_display_order_key
            .is_some_and(|last| last > draw.display_order_key)
        {
            self.display_order_monotonic = false;
        }
        self.last_display_order_key = Some(draw.display_order_key);
        self.uses_present_id2 |= draw.uses_present_id2;
        self.present_wait2_available |= draw.present_wait2_available;
        self.present_wait_after_present |= draw.present_wait_after_present;
        self.all_zero_copy_presented &= draw.zero_copy_presented;
        if draw.sampled_array_layer < 128 {
            self.sampled_array_layer_mask |= 1u128 << draw.sampled_array_layer;
        }
        if let Some(pts_ns) = draw.source_frame_pts_ns {
            if let Some(last) = self.last_pts_ns {
                if last > pts_ns {
                    self.pts_monotonic = false;
                } else {
                    let delta = pts_ns.saturating_sub(last);
                    if delta > 0 {
                        self.source_frame_pts_delta_min_ns = Some(
                            self.source_frame_pts_delta_min_ns
                                .map(|current| current.min(delta))
                                .unwrap_or(delta),
                        );
                        self.source_frame_pts_delta_max_ns = Some(
                            self.source_frame_pts_delta_max_ns
                                .map(|current| current.max(delta))
                                .unwrap_or(delta),
                        );
                    }
                }
            }
            self.last_pts_ns = Some(pts_ns);
        }

        if DECODED_IMAGE_PRESENT_TELEMETRY_RETAINED_FRAMES == 0 {
            self.latest_draw = Some(draw);
            return;
        }
        self.latest_draw = Some(draw.clone());
        if self.draws_head.len() < DECODED_IMAGE_PRESENT_TELEMETRY_RETAINED_FRAMES {
            self.draws_head.push(draw.clone());
        }
        if self.draws_tail.len() == DECODED_IMAGE_PRESENT_TELEMETRY_RETAINED_FRAMES {
            self.draws_tail.remove(0);
        }
        self.draws_tail.push(draw);
    }

    fn finish(
        self,
        present_handoff: NativeVulkanVulkanaliaDecodedPresentHandoffSnapshot,
        execution: NativeVulkanVulkanaliaDecodedImagePresentExecutionEvidence,
    ) -> Option<NativeVulkanVulkanaliaDecodedImagePresentSequenceSnapshot> {
        let latest_draw = self.latest_draw;
        if latest_draw.is_none() {
            return None;
        }
        let teardown_inclusive_elapsed = self.started_at.elapsed();
        let average_present_teardown_inclusive_fps =
            if self.presented_frame_count == 0 || teardown_inclusive_elapsed.is_zero() {
                0.0
            } else {
                f64::from(self.presented_frame_count) / teardown_inclusive_elapsed.as_secs_f64()
            };
        let present_interval_elapsed = match (
            self.first_presented_at,
            self.last_presented_at,
            self.presented_frame_count,
        ) {
            (Some(first), Some(last), presented_frame_count) if presented_frame_count > 1 => {
                last.saturating_duration_since(first)
            }
            _ => Duration::ZERO,
        };
        let average_present_fps =
            if self.presented_frame_count > 1 && !present_interval_elapsed.is_zero() {
                f64::from(self.presented_frame_count.saturating_sub(1))
                    / present_interval_elapsed.as_secs_f64()
            } else {
                average_present_teardown_inclusive_fps
            };
        Some(NativeVulkanVulkanaliaDecodedImagePresentSequenceSnapshot {
            binding: "vulkanalia",
            route: "decoded-image-dynamic-rendering-present-sequence",
            execution_model: "FFmpeg-style read thread -> bounded packet queue -> single video decode worker -> bounded decoded-frame handoff -> present worker",
            ffmpeg_thread_model: "one FFmpeg packet read thread per streaming source, one native video decode worker, one native present worker; decode thread_count stays 1 while Vulkan async-depth follows FFmpeg Vulkan decode formula",
            ffmpeg_read_thread_active: execution.ffmpeg_read_thread_active,
            video_decode_worker_active: execution.video_decode_worker_active,
            present_worker_active: execution.present_worker_active,
            decode_thread_count: execution.decode_thread_count,
            decode_async_exec_depth: execution.decode_async_exec_depth,
            requested_present_frame_count: self.requested_present_frame_count,
            submitted_present_frame_count: self.submitted_present_frame_count,
            presented_frame_count: self.presented_frame_count,
            average_present_fps,
            average_present_teardown_inclusive_fps,
            present_interval_elapsed_micros: duration_micros_u64(present_interval_elapsed),
            present_teardown_inclusive_elapsed_micros: duration_micros_u64(
                teardown_inclusive_elapsed,
            ),
            present_delta_min_micros: self.present_delta_min_micros,
            present_delta_max_micros: self.present_delta_max_micros,
            present_delta_over_6250us_count: self.present_delta_over_6250us_count,
            present_delta_over_8334us_count: self.present_delta_over_8334us_count,
            slow_frame_telemetry_limit: DECODED_IMAGE_PRESENT_SLOW_FRAME_TELEMETRY_LIMIT,
            slow_frames: self.slow_frames,
            retained_frame_telemetry_limit: DECODED_IMAGE_PRESENT_TELEMETRY_RETAINED_FRAMES,
            distinct_sampled_array_layer_count: self.sampled_array_layer_mask.count_ones(),
            sampled_array_layers_head: self
                .draws_head
                .iter()
                .map(|draw| draw.sampled_array_layer)
                .collect(),
            sampled_array_layers_tail: self
                .draws_tail
                .iter()
                .map(|draw| draw.sampled_array_layer)
                .collect(),
            source_frame_pts_ns_head: self
                .draws_head
                .iter()
                .map(|draw| draw.source_frame_pts_ns)
                .collect(),
            source_frame_pts_ns_tail: self
                .draws_tail
                .iter()
                .map(|draw| draw.source_frame_pts_ns)
                .collect(),
            source_frame_pts_delta_min_ns: self.source_frame_pts_delta_min_ns,
            source_frame_pts_delta_max_ns: self.source_frame_pts_delta_max_ns,
            source_frame_duration_ns_head: self
                .draws_head
                .iter()
                .map(|draw| draw.source_frame_duration_ns)
                .collect(),
            source_frame_duration_ns_tail: self
                .draws_tail
                .iter()
                .map(|draw| draw.source_frame_duration_ns)
                .collect(),
            source_frame_pts_ms_head: self
                .draws_head
                .iter()
                .map(|draw| draw.source_frame_pts_ms)
                .collect(),
            source_frame_pts_ms_tail: self
                .draws_tail
                .iter()
                .map(|draw| draw.source_frame_pts_ms)
                .collect(),
            source_frame_pts_delta_min_ms: self.source_frame_pts_delta_min_ms,
            source_frame_pts_delta_max_ms: self.source_frame_pts_delta_max_ms,
            source_frame_duration_ms_head: self
                .draws_head
                .iter()
                .map(|draw| draw.source_frame_duration_ms)
                .collect(),
            source_frame_duration_ms_tail: self
                .draws_tail
                .iter()
                .map(|draw| draw.source_frame_duration_ms)
                .collect(),
            display_order_keys_head: self
                .draws_head
                .iter()
                .map(|draw| draw.display_order_key)
                .collect(),
            display_order_keys_tail: self
                .draws_tail
                .iter()
                .map(|draw| draw.display_order_key)
                .collect(),
            display_order_key_sources_head: self
                .draws_head
                .iter()
                .map(|draw| draw.display_order_key_source)
                .collect(),
            display_order_key_sources_tail: self
                .draws_tail
                .iter()
                .map(|draw| draw.display_order_key_source)
                .collect(),
            present_ids_head: self.draws_head.iter().map(|draw| draw.present_id).collect(),
            present_ids_tail: self.draws_tail.iter().map(|draw| draw.present_id).collect(),
            frame_sleep_count: self.frame_sleep_count,
            missed_frame_pacing_count: self.missed_frame_pacing_count,
            total_pacing_sleep_micros: self.total_pacing_sleep_micros,
            total_present_call_micros: self.total_present_call_micros,
            max_present_call_micros: self.max_present_call_micros,
            total_present_wait_frame_slot_micros: self.total_present_wait_frame_slot_micros,
            max_present_wait_frame_slot_micros: self.max_present_wait_frame_slot_micros,
            total_present_acquire_next_image_micros: self.total_present_acquire_next_image_micros,
            max_present_acquire_next_image_micros: self.max_present_acquire_next_image_micros,
            total_present_record_command_buffer_micros: self
                .total_present_record_command_buffer_micros,
            max_present_record_command_buffer_micros: self.max_present_record_command_buffer_micros,
            total_present_submit_command_buffer_micros: self
                .total_present_submit_command_buffer_micros,
            max_present_submit_command_buffer_micros: self.max_present_submit_command_buffer_micros,
            total_present_queue_present_micros: self.total_present_queue_present_micros,
            max_present_queue_present_micros: self.max_present_queue_present_micros,
            total_present_wait_after_queue_present_micros: self
                .total_present_wait_after_queue_present_micros,
            max_present_wait_after_queue_present_micros: self
                .max_present_wait_after_queue_present_micros,
            pts_monotonic: self.pts_monotonic,
            display_order_monotonic: self.display_order_monotonic,
            uses_present_id2: self.uses_present_id2,
            present_wait2_available: self.present_wait2_available,
            present_wait_after_present: self.present_wait_after_present,
            present_handoff,
            latest_draw,
            draws_head: self.draws_head,
            draws_tail: self.draws_tail,
            frame_order_model: "FFmpeg-style display queue: decode submissions enqueue FIFO metadata carrying PTS/POC/order-hint keys with decode-index fallback; ready-prefix windows may be looped as metadata-only sampled-layer references before Vulkanalia dynamic rendering",
            present_resource_reuse_model: "one swapchain image-view set, one command pool, one semaphore pair, one fence set and one bounded decoded-frame handoff reused across decoded-image present frames",
            telemetry_retention_model: "compact head/tail/latest frame telemetry only; hot video runtime does not retain every draw snapshot",
            all_zero_copy_presented: self.all_zero_copy_presented,
            uses_dynamic_rendering: true,
            uses_synchronization2: true,
            uses_submit2: true,
            ffmpeg_reference: FFMPEG_VULKAN_DECODE_REFERENCE,
        })
    }
}

#[derive(Debug, Clone)]
struct NativeVulkanVulkanaliaPresentFrameTimer {
    frame_timer: Option<Instant>,
    target_max_fps: Option<u32>,
    audio_master_clock: NativeVulkanVulkanaliaVideoPresentAudioMasterClock,
    audio_master_started_at: Option<Instant>,
    last_pts_ns: Option<u64>,
    last_duration_ns: Option<u64>,
}

impl NativeVulkanVulkanaliaPresentFrameTimer {
    fn new(
        target_max_fps: Option<u32>,
        audio_master_clock: NativeVulkanVulkanaliaVideoPresentAudioMasterClock,
    ) -> Self {
        Self {
            frame_timer: None,
            target_max_fps: target_max_fps.filter(|fps| *fps > 0),
            audio_master_clock,
            audio_master_started_at: None,
            last_pts_ns: None,
            last_duration_ns: None,
        }
    }

    fn reset(&mut self, now: Instant) {
        self.frame_timer = Some(now);
        self.audio_master_started_at = self.audio_master_clock.enabled.then_some(now);
        self.last_pts_ns = None;
        self.last_duration_ns = None;
    }

    fn pace_frame(
        &mut self,
        present_frame_index: u32,
        source_frame_pts_ns: Option<u64>,
        source_frame_duration_ns: Option<u64>,
        source_frame_pts_ms: Option<u64>,
        source_frame_duration_ms: Option<u64>,
    ) -> (u64, &'static str) {
        let pts_ns = source_frame_pts_ns
            .or_else(|| source_frame_pts_ms.map(|pts| pts.saturating_mul(1_000_000)));
        let duration_ns = source_frame_duration_ns.or_else(|| {
            source_frame_duration_ms.map(|duration| duration.saturating_mul(1_000_000))
        });
        let now = Instant::now();
        if self.frame_timer.is_none() || present_frame_index == 0 {
            self.frame_timer = Some(now);
            self.audio_master_started_at = self.audio_master_clock.enabled.then_some(now);
            self.last_pts_ns = pts_ns;
            self.last_duration_ns = duration_ns;
            return (
                0,
                if self.audio_master_clock.enabled {
                    "audio-clock-master-first-frame"
                } else {
                    "ffmpeg-frame-timer-first-frame"
                },
            );
        }

        if let Some((delay, clock_model)) =
            self.audio_master_delay_for_frame(now, present_frame_index, pts_ns, duration_ns)
        {
            if delay.is_zero() {
                self.last_pts_ns = pts_ns;
                self.last_duration_ns = duration_ns;
                return (0, clock_model);
            }
            let deadline = now + delay;
            let slept = native_vulkan_vulkanalia_wait_until_video_present_deadline(deadline);
            self.frame_timer = Some(deadline);
            self.last_pts_ns = pts_ns;
            self.last_duration_ns = duration_ns;
            return (
                u64::try_from(slept.as_micros()).unwrap_or(u64::MAX),
                clock_model,
            );
        }

        let (delay, clock_model) = self.next_delay(pts_ns, duration_ns);
        if delay.is_zero() {
            self.last_pts_ns = pts_ns;
            self.last_duration_ns = duration_ns;
            return (0, clock_model);
        }
        let frame_timer = self.frame_timer.unwrap_or(now);
        let deadline = frame_timer + delay;
        let wait_started_at = Instant::now();
        let slept = if deadline > wait_started_at {
            native_vulkan_vulkanalia_wait_until_video_present_deadline(deadline)
        } else {
            Duration::ZERO
        };
        self.frame_timer = Some(deadline);
        let after_wait = Instant::now();
        if after_wait > deadline
            && after_wait.duration_since(deadline) > FFMPEG_AV_SYNC_THRESHOLD_MAX
        {
            // FFmpeg's video_refresh() advances frame_timer by the nominal
            // delay, then only resynchronizes on large lateness
            // (references/ffmpeg/fftools/ffplay.c:1665-1683).
            self.frame_timer = Some(after_wait);
        }
        self.last_pts_ns = pts_ns;
        self.last_duration_ns = duration_ns;
        (
            u64::try_from(slept.as_micros()).unwrap_or(u64::MAX),
            clock_model,
        )
    }

    fn next_delay(
        &self,
        pts_ns: Option<u64>,
        duration_ns: Option<u64>,
    ) -> (Duration, &'static str) {
        if let (Some(last_pts_ns), Some(pts_ns)) = (self.last_pts_ns, pts_ns) {
            if pts_ns > last_pts_ns {
                return (
                    Duration::from_nanos(pts_ns - last_pts_ns),
                    "ffmpeg-frame-timer-pts-delta-sleep",
                );
            }
        }
        if let Some(last_duration_ns) = self.last_duration_ns.filter(|duration| *duration > 0) {
            return (
                Duration::from_nanos(last_duration_ns),
                "ffmpeg-frame-timer-last-duration-sleep",
            );
        }
        if let Some(duration_ns) = duration_ns.filter(|duration| *duration > 0) {
            return (
                Duration::from_nanos(duration_ns),
                "ffmpeg-frame-timer-duration-sleep",
            );
        }
        if let Some(target_max_fps) = self.target_max_fps {
            return (
                native_vulkan_vulkanalia_frame_count_duration(1, target_max_fps),
                "ffmpeg-frame-timer-target-fps-sleep",
            );
        }
        (Duration::ZERO, "unpaced-no-video-clock")
    }

    fn audio_master_delay_for_frame(
        &self,
        now: Instant,
        present_frame_index: u32,
        pts_ns: Option<u64>,
        duration_ns: Option<u64>,
    ) -> Option<(Duration, &'static str)> {
        if !self.audio_master_clock.enabled || present_frame_index == 0 {
            return None;
        }
        let master_clock_ns = self.audio_master_clock_ns(now)?;
        let video_clock_ns =
            self.current_video_clock_ns(present_frame_index, pts_ns, duration_ns)?;
        if video_clock_ns <= master_clock_ns {
            return Some((Duration::ZERO, "audio-clock-master-video-late-no-sleep"));
        }
        let delay_ns = video_clock_ns.saturating_sub(master_clock_ns);
        Some((
            Duration::from_nanos(delay_ns),
            "audio-clock-master-pts-sync-sleep",
        ))
    }

    fn audio_master_clock_ns(&self, now: Instant) -> Option<u64> {
        let started_at = self.audio_master_started_at?;
        Some(
            self.audio_master_clock
                .start_clock_ns
                .unwrap_or(0)
                .saturating_add(
                    u64::try_from(now.duration_since(started_at).as_nanos()).unwrap_or(u64::MAX),
                ),
        )
    }

    fn current_video_clock_ns(
        &self,
        present_frame_index: u32,
        pts_ns: Option<u64>,
        duration_ns: Option<u64>,
    ) -> Option<u64> {
        if let Some(pts_ns) = pts_ns {
            return Some(pts_ns);
        }
        if let (Some(last_pts_ns), Some(duration_ns)) =
            (self.last_pts_ns, self.last_duration_ns.or(duration_ns))
        {
            if duration_ns > 0 {
                return Some(last_pts_ns.saturating_add(duration_ns));
            }
        }
        self.target_max_fps.filter(|fps| *fps > 0).map(|fps| {
            let clock_ns = (u128::from(present_frame_index) * 1_000_000_000u128) / u128::from(fps);
            u64::try_from(clock_ns).unwrap_or(u64::MAX)
        })
    }
}

fn native_vulkan_vulkanalia_wait_until_video_present_deadline(deadline: Instant) -> Duration {
    let started_at = Instant::now();
    loop {
        let now = Instant::now();
        if now >= deadline {
            return now.saturating_duration_since(started_at);
        }
        let remaining = deadline.duration_since(now);
        if remaining > VIDEO_PRESENT_SLEEP_GUARD {
            thread::sleep(remaining - VIDEO_PRESENT_SLEEP_GUARD);
        } else if remaining > VIDEO_PRESENT_SPIN_GUARD {
            thread::yield_now();
        } else {
            std::hint::spin_loop();
        }
    }
}

fn native_vulkan_vulkanalia_frame_count_duration(
    frame_count: u32,
    target_max_fps: u32,
) -> Duration {
    let fps = u128::from(target_max_fps.max(1));
    let nanos = u128::from(frame_count).saturating_mul(1_000_000_000u128) / fps;
    Duration::from_nanos(nanos.min(u128::from(u64::MAX)) as u64)
}

fn duration_micros_u64(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

fn native_vulkan_vulkanalia_ffmpeg_decode_async_exec_depth(video_queue_count: u32) -> u32 {
    let queue_context_count = video_queue_count.max(1);
    let thread_count = FFMPEG_SINGLE_DECODE_THREAD_COUNT.max(1);
    // Exact FFmpeg Vulkan decode async-depth formula for this runtime's single
    // decode worker thread (references/ffmpeg/libavcodec/vulkan_decode.c:1368-1378).
    queue_context_count
        .saturating_mul(2)
        .min(thread_count.saturating_mul(2))
        .max(thread_count)
        .max(1)
}

fn native_vulkan_vulkanalia_select_stream_session_dpb_slots(
    required_dpb_slots: u32,
    driver_session_max_dpb_slots: u32,
) -> Result<u32, String> {
    let required_dpb_slots = required_dpb_slots.max(1);
    if driver_session_max_dpb_slots != 0 && required_dpb_slots > driver_session_max_dpb_slots {
        return Err(format!(
            "streaming decode requires {required_dpb_slots} DPB slot(s), but the selected Vulkan video session exposes only {driver_session_max_dpb_slots}"
        ));
    }
    // Keep the session sized to the stream, not the driver's advertised ceiling.
    // FFmpeg adds a small fixed output-frame reserve after
    // avcodec_get_hw_frames_parameters()
    // (references/ffmpeg/libavcodec/decode.c:1088-1095). This runtime owns one
    // coincident decoded-image array for DPB/output/sampling, so retaining the
    // driver's full maxDpbSlots here only pins unused image/session memory.
    Ok(required_dpb_slots)
}

fn native_vulkan_vulkanalia_select_stream_resource_image_array_layers(
    required_dpb_slots: u32,
    session_max_dpb_slots: u32,
) -> Result<u32, String> {
    let required_dpb_slots = required_dpb_slots.max(1);
    if session_max_dpb_slots != 0 && required_dpb_slots > session_max_dpb_slots {
        return Err(format!(
            "streaming decode requires {required_dpb_slots} resource image layer(s), but the selected Vulkan video session exposes only {session_max_dpb_slots}"
        ));
    }
    // FFmpeg's separate layered DPB path may allocate caps.maxDpbSlots
    // (references/ffmpeg/libavcodec/vulkan_decode.c:1388-1431). This runtime
    // deliberately uses one coincident sampled image array, so layer count must
    // track the stream-required DPB/output ring instead of the driver ceiling.
    Ok(required_dpb_slots)
}

fn native_vulkan_vulkanalia_select_stream_session_active_reference_pictures(
    required_active_reference_pictures: u32,
    driver_session_max_active_reference_pictures: u32,
    session_max_dpb_slots: u32,
) -> Result<u32, String> {
    if session_max_dpb_slots == 0 {
        return Ok(0);
    }
    let required_active_reference_pictures = required_active_reference_pictures
        .max(1)
        .min(session_max_dpb_slots);
    if driver_session_max_active_reference_pictures != 0
        && required_active_reference_pictures > driver_session_max_active_reference_pictures
    {
        return Err(format!(
            "streaming decode requires {required_active_reference_pictures} active reference picture(s), but the selected Vulkan video session exposes only {driver_session_max_active_reference_pictures}"
        ));
    }
    Ok(required_active_reference_pictures)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_session_sizing_uses_stream_required_dpb_and_driver_as_ceiling() {
        assert_eq!(
            native_vulkan_vulkanalia_select_stream_session_dpb_slots(3, 16).unwrap(),
            3
        );
        assert_eq!(
            native_vulkan_vulkanalia_select_stream_session_dpb_slots(3, 5).unwrap(),
            3
        );
        assert_eq!(
            native_vulkan_vulkanalia_select_stream_resource_image_array_layers(3, 16).unwrap(),
            3
        );
        assert_eq!(
            native_vulkan_vulkanalia_select_stream_resource_image_array_layers(3, 5).unwrap(),
            3
        );
        assert_eq!(
            native_vulkan_vulkanalia_select_stream_session_active_reference_pictures(3, 16, 3)
                .unwrap(),
            3
        );
    }

    #[test]
    fn av1_stream_resource_layers_follow_stream_dpb_slots() {
        assert_eq!(
            native_vulkan_vulkanalia_select_stream_session_dpb_slots(9, 16).unwrap(),
            9
        );
        assert_eq!(
            native_vulkan_vulkanalia_select_stream_resource_image_array_layers(9, 9).unwrap(),
            9
        );
    }

    #[test]
    fn ffmpeg_decode_async_exec_depth_matches_single_decode_thread_formula() {
        assert_eq!(
            native_vulkan_vulkanalia_ffmpeg_decode_async_exec_depth(1),
            2
        );
        assert_eq!(
            native_vulkan_vulkanalia_ffmpeg_decode_async_exec_depth(4),
            2
        );
        assert_eq!(
            native_vulkan_vulkanalia_ffmpeg_decode_async_exec_depth(0),
            2
        );
    }

    #[test]
    fn present_frame_timer_uses_ffmpeg_pts_delta_before_duration_fallback() {
        let mut timer = NativeVulkanVulkanaliaPresentFrameTimer::new(
            Some(240),
            NativeVulkanVulkanaliaVideoPresentAudioMasterClock::DISABLED,
        );
        timer.last_pts_ns = Some(1_000_000_000);
        timer.last_duration_ns = Some(4_166_667);

        assert_eq!(
            timer.next_delay(Some(1_004_000_000), Some(5_000_000)),
            (
                Duration::from_nanos(4_000_000),
                "ffmpeg-frame-timer-pts-delta-sleep"
            )
        );
        assert_eq!(
            timer.next_delay(None, Some(5_000_000)),
            (
                Duration::from_nanos(4_166_667),
                "ffmpeg-frame-timer-last-duration-sleep"
            )
        );
    }

    #[test]
    fn present_frame_timer_falls_back_to_target_fps_without_pts_or_duration() {
        let timer = NativeVulkanVulkanaliaPresentFrameTimer::new(
            Some(240),
            NativeVulkanVulkanaliaVideoPresentAudioMasterClock::DISABLED,
        );

        assert_eq!(
            timer.next_delay(None, None),
            (
                Duration::from_nanos(4_166_666),
                "ffmpeg-frame-timer-target-fps-sleep"
            )
        );
    }

    #[test]
    fn present_frame_timer_audio_master_uses_rebased_video_pts() {
        let mut timer = NativeVulkanVulkanaliaPresentFrameTimer::new(
            Some(240),
            NativeVulkanVulkanaliaVideoPresentAudioMasterClock::clock_only(None),
        );
        let started_at = Instant::now();
        timer.reset(started_at);

        assert_eq!(
            timer.audio_master_delay_for_frame(
                started_at + Duration::from_micros(1_000),
                1,
                Some(4_166_666),
                None,
            ),
            Some((
                Duration::from_nanos(3_166_666),
                "audio-clock-master-pts-sync-sleep"
            ))
        );
        assert_eq!(
            timer.audio_master_delay_for_frame(
                started_at + Duration::from_micros(5_000),
                1,
                Some(4_166_666),
                None,
            ),
            Some((Duration::ZERO, "audio-clock-master-video-late-no-sleep"))
        );
    }

    #[test]
    fn present_frame_timer_audio_master_starts_from_audio_clock_sample() {
        let mut timer = NativeVulkanVulkanaliaPresentFrameTimer::new(
            Some(240),
            NativeVulkanVulkanaliaVideoPresentAudioMasterClock::clock_only(Some(2_000_000)),
        );
        let started_at = Instant::now();
        timer.reset(started_at);

        assert_eq!(
            timer.audio_master_delay_for_frame(
                started_at + Duration::from_micros(1_000),
                1,
                Some(5_000_000),
                None,
            ),
            Some((
                Duration::from_nanos(2_000_000),
                "audio-clock-master-pts-sync-sleep"
            ))
        );
    }

    #[test]
    fn decoded_present_startup_preroll_is_first_frame_driven() {
        assert_eq!(DECODED_IMAGE_PRESENT_STARTUP_PREROLL_FRAMES, 1);
        assert!(DECODED_IMAGE_PRESENT_STARTUP_PREROLL_FRAMES <= FFMPEG_VIDEO_PICTURE_QUEUE_SIZE);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn streaming_pts_state_rebases_each_source_loop_to_segment_start() {
        let mut pts = NativeVulkanVulkanaliaStreamingPtsState::new(0);

        assert_eq!(
            pts.adjusted_pts_ns(Some(650_000_000), Some(650), Some(4_166_667), Some(4)),
            Some(0)
        );
        assert_eq!(
            pts.adjusted_pts_ns(Some(654_166_667), Some(654), Some(4_166_667), Some(4)),
            Some(4_166_667)
        );

        assert!(pts.sync_loop(1));
        assert_eq!(
            pts.adjusted_pts_ns(Some(650_000_000), Some(650), Some(4_166_667), Some(4)),
            Some(8_333_334)
        );
        assert_eq!(
            pts.adjusted_pts_ns(Some(654_166_667), Some(654), Some(4_166_667), Some(4)),
            Some(12_500_001)
        );
    }

    #[test]
    fn stream_session_sizing_rejects_driver_capability_overflow() {
        let dpb_err = native_vulkan_vulkanalia_select_stream_session_dpb_slots(4, 3)
            .expect_err("driver max must bound DPB sizing");
        assert!(dpb_err.contains("requires 4 DPB slot"));

        let refs_err =
            native_vulkan_vulkanalia_select_stream_session_active_reference_pictures(4, 3, 4)
                .expect_err("driver max must bound active reference sizing");
        assert!(refs_err.contains("requires 4 active reference picture"));
    }
}
