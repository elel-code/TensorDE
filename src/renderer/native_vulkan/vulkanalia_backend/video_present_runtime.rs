#![allow(dead_code)]

use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{
    self, HasBuilder, KhrSurfaceExtensionInstanceCommands, KhrSwapchainExtensionDeviceCommands,
};

use crate::renderer::native_vulkan::{
    NativeVulkanAv1DecodeReferencePlanEntrySnapshot,
    NativeVulkanH264DecodeReferencePlanEntrySnapshot,
    NativeVulkanH265DecodeReferencePlanEntrySnapshot, NativeVulkanVideoSessionCodec,
};
use crate::renderer::native_wayland::NativeWaylandHost;

use super::instance::{
    NativeVulkanVulkanaliaInstance,
    native_vulkan_vulkanalia_create_instance_with_required_extensions,
    native_vulkan_vulkanalia_destroy_instance,
};
use super::render_present::{
    NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot,
    NativeVulkanVulkanaliaDecodedImagePresentSequenceSnapshot,
    VulkanaliaDecodedImagePresentFrameResources, VulkanaliaDecodedImagePresentPipelineResources,
    VulkanaliaDecodedImagePresentSamplerResources, VulkanaliaDecodedImagePresentTimingConfig,
    native_vulkan_vulkanalia_create_decoded_image_present_frame_resources,
    native_vulkan_vulkanalia_create_decoded_image_present_pipeline_resources,
    native_vulkan_vulkanalia_create_decoded_image_present_sampler_resources,
    native_vulkan_vulkanalia_destroy_decoded_image_present_frame_resources,
    native_vulkan_vulkanalia_destroy_decoded_image_present_pipeline_resources,
    native_vulkan_vulkanalia_destroy_decoded_image_present_sampler_resources,
    native_vulkan_vulkanalia_present_decoded_image_frame,
    native_vulkan_vulkanalia_present_decoded_image_once,
    native_vulkan_vulkanalia_retarget_decoded_image_present_sampler_layer,
};
use super::swapchain::{
    OPTIONAL_INSTANCE_EXTENSIONS, REQUIRED_INSTANCE_EXTENSIONS, create_vulkanalia_swapchain_plan,
    create_vulkanalia_wayland_surface, vulkanalia_surface_capabilities2_enabled,
};
use super::video_decode_submit::FFMPEG_VULKAN_DECODE_REFERENCE;
use super::video_decode_submit_av1::{
    NativeVulkanVulkanaliaAv1ReadyPrefixCommandSmokeSnapshot,
    NativeVulkanVulkanaliaAv1ReadyPrefixDecodeInput,
};
use super::video_decode_submit_h264::{
    NativeVulkanVulkanaliaH264ReadyPrefixCommandSmokeSnapshot,
    NativeVulkanVulkanaliaH264ReadyPrefixDecodeInput,
};
use super::video_decode_submit_h265::{
    NativeVulkanVulkanaliaH265ReadyPrefixCommandSmokeSnapshot,
    NativeVulkanVulkanaliaH265ReadyPrefixDecodeInput,
};
use super::video_present_device::{
    NativeVulkanVulkanaliaVideoPresentDeviceContext,
    NativeVulkanVulkanaliaVideoPresentSessionProbeOptions,
    NativeVulkanVulkanaliaVideoPresentSessionProbeSnapshot, create_video_present_device,
    decoded_image_resource_sharing_model, device_snapshot_from_selection,
    select_video_present_physical_device, swapchain_plan_snapshot,
    video_present_queue_family_indices,
};
use super::video_present_handoff::{
    NativeVulkanVulkanaliaDecodedPresentHandoff,
    NativeVulkanVulkanaliaDecodedPresentHandoffSnapshot,
    NativeVulkanVulkanaliaPendingDecodedPresentFrame,
};
use super::video_profile_labels::video_decode_capability_flag_labels;
use super::video_session::{
    NativeVulkanVulkanaliaVideoSessionMemoryBindingResources,
    native_vulkan_vulkanalia_bind_video_session_memory_resources,
    native_vulkan_vulkanalia_create_video_session, native_vulkan_vulkanalia_destroy_video_session,
    native_vulkan_vulkanalia_destroy_video_session_memory_binding_resources,
};
use super::video_session_bind::{
    native_vulkan_vulkanalia_record_av1_ready_prefix_decode_into_image,
    native_vulkan_vulkanalia_record_h264_ready_prefix_decode_into_image,
    native_vulkan_vulkanalia_record_h265_ready_prefix_decode_into_image,
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

pub(super) const VIDEO_PRESENT_SESSION_RETAINED_RESOURCE_ROUTE: &str =
    "video-present-session-retained-resource";

pub(super) struct NativeVulkanVulkanaliaVideoPresentSessionRuntime {
    resources: Option<NativeVulkanVulkanaliaVideoPresentSessionRuntimeResources>,
    snapshot: NativeVulkanVulkanaliaVideoPresentSessionProbeSnapshot,
}

impl NativeVulkanVulkanaliaVideoPresentSessionRuntime {
    pub(super) fn snapshot(&self) -> &NativeVulkanVulkanaliaVideoPresentSessionProbeSnapshot {
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
    present_queue_family_index: u32,
    picture_format: vk::Format,
    session: vk::VideoSessionKHR,
    memory_resources: Option<NativeVulkanVulkanaliaVideoSessionMemoryBindingResources>,
    resource_image: Option<VulkanaliaVideoSessionResourceImage>,
    decoded_image_present_pipeline: Option<VulkanaliaDecodedImagePresentPipelineResources>,
    decoded_image_present_sampler: Option<VulkanaliaDecodedImagePresentSamplerResources>,
    decoded_image_present_sequence:
        Option<NativeVulkanVulkanaliaDecodedImagePresentSequenceSnapshot>,
    decoded_image_present_sequence_error: Option<String>,
    av1_ready_prefix_decode: Option<NativeVulkanVulkanaliaAv1ReadyPrefixCommandSmokeSnapshot>,
    h264_ready_prefix_decode: Option<NativeVulkanVulkanaliaH264ReadyPrefixCommandSmokeSnapshot>,
    h265_ready_prefix_decode: Option<NativeVulkanVulkanaliaH265ReadyPrefixCommandSmokeSnapshot>,
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
        )
    }

    fn decoded_image_present_result(
        &mut self,
        fallback_sampled_array_layer: u32,
    ) -> NativeVulkanVulkanaliaRetainedPresentResult {
        if let Some(sequence) = self.decoded_image_present_sequence.clone() {
            let draw = sequence.draws.last().cloned();
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
    snapshot: NativeVulkanVulkanaliaVideoPresentSessionProbeSnapshot,
    decoded_image_present_sequence:
        Option<NativeVulkanVulkanaliaDecodedImagePresentSequenceSnapshot>,
    decoded_image_present_sequence_error: Option<String>,
    av1_ready_prefix_decode: Option<NativeVulkanVulkanaliaAv1ReadyPrefixCommandSmokeSnapshot>,
    h264_ready_prefix_decode: Option<NativeVulkanVulkanaliaH264ReadyPrefixCommandSmokeSnapshot>,
    h265_ready_prefix_decode: Option<NativeVulkanVulkanaliaH265ReadyPrefixCommandSmokeSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeVulkanVulkanaliaAv1RetainedVideoPresentDecodeOptions {
    pub session: NativeVulkanVulkanaliaVideoPresentSessionProbeOptions,
    pub ready_prefix: NativeVulkanVulkanaliaAv1ReadyPrefixDecodeInput,
    pub bitstream_buffer_size: u64,
    pub playback_frame_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaAv1RetainedVideoPresentDecodeSnapshot {
    pub session: NativeVulkanVulkanaliaVideoPresentSessionProbeSnapshot,
    pub decode: NativeVulkanVulkanaliaAv1ReadyPrefixCommandSmokeSnapshot,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeVulkanVulkanaliaH264RetainedVideoPresentDecodeOptions {
    pub session: NativeVulkanVulkanaliaVideoPresentSessionProbeOptions,
    pub ready_prefix: NativeVulkanVulkanaliaH264ReadyPrefixDecodeInput,
    pub bitstream_buffer_size: u64,
    pub playback_frame_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeVulkanVulkanaliaH265RetainedVideoPresentDecodeOptions {
    pub session: NativeVulkanVulkanaliaVideoPresentSessionProbeOptions,
    pub ready_prefix: NativeVulkanVulkanaliaH265ReadyPrefixDecodeInput,
    pub bitstream_buffer_size: u64,
    pub playback_frame_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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

pub(super) fn probe_native_vulkan_vulkanalia_retained_video_present_session(
    options: NativeVulkanVulkanaliaVideoPresentSessionProbeOptions,
) -> Result<NativeVulkanVulkanaliaVideoPresentSessionProbeSnapshot, String> {
    let runtime = create_native_vulkan_vulkanalia_video_present_session_runtime(options)?;
    Ok(runtime.snapshot().clone())
}

pub fn run_native_vulkan_vulkanalia_av1_retained_video_present_decode(
    options: NativeVulkanVulkanaliaAv1RetainedVideoPresentDecodeOptions,
) -> Result<NativeVulkanVulkanaliaAv1RetainedVideoPresentDecodeSnapshot, String> {
    if !matches!(
        options.session.codec,
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10
    ) {
        return Err(
            "Vulkanalia retained video-present decode currently supports AV1 only".to_owned(),
        );
    }
    let mut runtime =
        create_native_vulkan_vulkanalia_video_present_session_runtime_with_ready_prefix_decode(
            options.session,
            Some((&options.ready_prefix, options.bitstream_buffer_size)),
            None,
            None,
            options.playback_frame_count,
        )?;
    let decode = runtime
        .resources
        .as_ref()
        .and_then(|resources| resources.av1_ready_prefix_decode.clone())
        .ok_or_else(|| {
            "Vulkanalia retained AV1 video-present decode produced no decode snapshot".to_owned()
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

pub fn run_native_vulkan_vulkanalia_h264_retained_video_present_decode(
    options: NativeVulkanVulkanaliaH264RetainedVideoPresentDecodeOptions,
) -> Result<NativeVulkanVulkanaliaH264RetainedVideoPresentDecodeSnapshot, String> {
    if options.session.codec != NativeVulkanVideoSessionCodec::H264High8 {
        return Err(
            "Vulkanalia retained video-present decode currently supports H.264 high-8 only"
                .to_owned(),
        );
    }
    let mut runtime =
        create_native_vulkan_vulkanalia_video_present_session_runtime_with_ready_prefix_decode(
            options.session,
            None,
            Some((&options.ready_prefix, options.bitstream_buffer_size)),
            None,
            options.playback_frame_count,
        )?;
    let decode = runtime
        .resources
        .as_ref()
        .and_then(|resources| resources.h264_ready_prefix_decode.clone())
        .ok_or_else(|| {
            "Vulkanalia retained H.264 video-present decode produced no decode snapshot".to_owned()
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

pub fn run_native_vulkan_vulkanalia_h265_retained_video_present_decode(
    options: NativeVulkanVulkanaliaH265RetainedVideoPresentDecodeOptions,
) -> Result<NativeVulkanVulkanaliaH265RetainedVideoPresentDecodeSnapshot, String> {
    if !matches!(
        options.session.codec,
        NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::H265Main10
    ) {
        return Err(
            "Vulkanalia retained video-present decode currently supports H.265 only".to_owned(),
        );
    }
    let mut runtime =
        create_native_vulkan_vulkanalia_video_present_session_runtime_with_ready_prefix_decode(
            options.session,
            None,
            None,
            Some((&options.ready_prefix, options.bitstream_buffer_size)),
            options.playback_frame_count,
        )?;
    let decode = runtime
        .resources
        .as_ref()
        .and_then(|resources| resources.h265_ready_prefix_decode.clone())
        .ok_or_else(|| {
            "Vulkanalia retained H.265 video-present decode produced no decode snapshot".to_owned()
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

pub(super) fn create_native_vulkan_vulkanalia_video_present_session_runtime(
    options: NativeVulkanVulkanaliaVideoPresentSessionProbeOptions,
) -> Result<NativeVulkanVulkanaliaVideoPresentSessionRuntime, String> {
    create_native_vulkan_vulkanalia_video_present_session_runtime_with_ready_prefix_decode(
        options, None, None, None, 0,
    )
}

fn create_native_vulkan_vulkanalia_video_present_session_runtime_with_ready_prefix_decode(
    options: NativeVulkanVulkanaliaVideoPresentSessionProbeOptions,
    av1_ready_prefix_decode: Option<(&NativeVulkanVulkanaliaAv1ReadyPrefixDecodeInput, u64)>,
    h264_ready_prefix_decode: Option<(&NativeVulkanVulkanaliaH264ReadyPrefixDecodeInput, u64)>,
    h265_ready_prefix_decode: Option<(&NativeVulkanVulkanaliaH265ReadyPrefixDecodeInput, u64)>,
    requested_present_frame_count: u32,
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
    let context = match create_video_present_device(instance, &selection, options.codec) {
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

    let decoded_image_present_timing = VulkanaliaDecodedImagePresentTimingConfig::new(
        context.present_feature_selection.present_id_enabled,
        swapchain_plan.present_id2_enabled,
        context.present_feature_selection.present_wait_enabled,
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
        decoded_image_present_timing,
        swapchain_plan_snapshot(&swapchain_plan, swapchain_images.len()),
        av1_ready_prefix_decode,
        h264_ready_prefix_decode,
        h265_ready_prefix_decode,
        requested_present_frame_count,
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

    let snapshot = pieces.snapshot.clone();
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
            present_queue_family_index: selection.present_queue_family_index,
            picture_format: native_vulkan_vulkanalia_video_session_effective_picture_format(
                options.codec,
                None,
            ),
            session: pieces.session,
            memory_resources: Some(pieces.memory_resources),
            resource_image: Some(pieces.resource_image),
            decoded_image_present_pipeline: pieces.decoded_image_present_pipeline,
            decoded_image_present_sampler: pieces.decoded_image_present_sampler,
            decoded_image_present_sequence: pieces.decoded_image_present_sequence,
            decoded_image_present_sequence_error: pieces.decoded_image_present_sequence_error,
            av1_ready_prefix_decode: pieces.av1_ready_prefix_decode,
            h264_ready_prefix_decode: pieces.h264_ready_prefix_decode,
            h265_ready_prefix_decode: pieces.h265_ready_prefix_decode,
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
    decoded_image_present_timing: VulkanaliaDecodedImagePresentTimingConfig,
    swapchain: super::swapchain::NativeVulkanVulkanaliaSwapchainSnapshot,
    av1_ready_prefix_decode: Option<(&NativeVulkanVulkanaliaAv1ReadyPrefixDecodeInput, u64)>,
    h264_ready_prefix_decode: Option<(&NativeVulkanVulkanaliaH264ReadyPrefixDecodeInput, u64)>,
    h265_ready_prefix_decode: Option<(&NativeVulkanVulkanaliaH265ReadyPrefixDecodeInput, u64)>,
    requested_present_frame_count: u32,
) -> Result<NativeVulkanVulkanaliaVideoPresentSessionPieces, String> {
    with_native_vulkan_vulkanalia_video_session_capabilities(
        instance,
        selection.physical_device,
        codec,
        None,
        None,
        |profile_info, queried| {
            let requested_extent = vk::Extent2D { width, height };
            if !native_vulkan_vulkanalia_video_session_extent_supported(
                requested_extent,
                queried.capabilities,
            ) {
                return Err(format!(
                    "requested Vulkanalia video present session extent {}x{} is outside driver capabilities",
                    requested_extent.width, requested_extent.height
                ));
            }
            let session_max_dpb_slots = native_vulkan_vulkanalia_video_session_max_dpb_slots(
                queried.capabilities.max_dpb_slots,
            );
            let session_max_active_reference_pictures =
                native_vulkan_vulkanalia_video_session_max_active_reference_pictures(
                    queried.capabilities.max_active_reference_pictures,
                    session_max_dpb_slots,
                );
            let resource_image_array_layers =
                native_vulkan_vulkanalia_ready_prefix_resource_image_array_layers(
                    session_max_dpb_slots,
                    av1_ready_prefix_decode.map(|(input, _)| input),
                    h264_ready_prefix_decode.map(|(input, _)| input),
                    h265_ready_prefix_decode.map(|(input, _)| input),
                );
            let picture_format =
                native_vulkan_vulkanalia_video_session_effective_picture_format(codec, None);
            let create_info = vk::VideoSessionCreateInfoKHR::builder()
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
                let (decoded_image_present_sampler_snapshot, decoded_image_present_sampler_error) =
                    if context
                        .video_feature_selection
                        .sampler_ycbcr_conversion_enabled
                    {
                        match native_vulkan_vulkanalia_create_decoded_image_present_sampler_resources(
                            &context.device,
                            &memory_properties,
                            resource_image_ref,
                            picture_format,
                            0,
                            selection.video_queue_family_index,
                            selection.present_queue_family_index,
                            context.video_feature_selection.core_features.descriptor_heap,
                            context
                                .video_feature_selection
                                .descriptor_heap_properties,
                        ) {
                            Ok(resources) => {
                                let snapshot = resources.snapshot.clone();
                                decoded_image_present_sampler = Some(resources);
                                (Some(snapshot), None)
                            }
                            Err(err) => (None, Some(err)),
                        }
                    } else {
                        (
                            None,
                            Some(
                                "samplerYcbcrConversion feature is unavailable on selected Vulkanalia video+present device"
                                    .to_owned(),
                            ),
                        )
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
                            sampler.descriptor_set_layout,
                            sampler
                                .descriptor_heap
                                .as_ref()
                                .map(|_| &sampler.snapshot.descriptor_heap_plan),
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
                                "decoded image present pipeline requires a live YCbCr sampler descriptor resource"
                                    .to_owned(),
                            ),
                        )
                    };
                let decoded_image_present_sequence_requested = av1_ready_prefix_decode.is_some()
                    || h264_ready_prefix_decode.is_some()
                    || h265_ready_prefix_decode.is_some();
                let mut decoded_image_present_sequence_error = None;
                let mut decoded_image_present_draws = Vec::new();
                let mut decoded_image_present_handoff_snapshot = None;
                if decoded_image_present_sequence_requested {
                    if decoded_image_present_sampler.is_none() {
                        decoded_image_present_sequence_error = Some(
                            "decoded image present sequence requires a live YCbCr sampler descriptor resource"
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
                let memory_binding = memory_resources
                    .as_ref()
                    .expect("Vulkanalia session memory resources are live")
                    .snapshot
                    .clone();
                let sequence_started_at = Instant::now();
                let mut first_present_pts_ms = None;
                let (
                    av1_ready_prefix_decode,
                    h264_ready_prefix_decode,
                    h265_ready_prefix_decode,
                    requested_present_frame_count,
                ) = {
                    let present_handoff_capacity =
                        native_vulkan_vulkanalia_present_handoff_capacity(
                            decoded_image_present_frame_count_hint(
                                av1_ready_prefix_decode
                                    .as_ref()
                                    .map(|(input, _)| input.frames.len()),
                                h264_ready_prefix_decode
                                    .as_ref()
                                    .map(|(input, _)| input.frames.len()),
                                h265_ready_prefix_decode
                                    .as_ref()
                                    .map(|(input, _)| input.frames.len()),
                            ),
                            swapchain_images.len(),
                            resource_image_array_layers,
                        );
                    let mut pending_decoded_present_frames =
                        NativeVulkanVulkanaliaDecodedPresentHandoff::new(present_handoff_capacity);
                    let mut enqueue_decoded_frame = |decode_frame_index: u32,
                                                     sampled_array_layer: u32,
                                                     source_frame_pts_ms: Option<u64>,
                                                     source_frame_duration_ms: Option<u64>,
                                                     display_order_key: i64,
                                                     display_order_key_source: &'static str|
                     -> Result<(), String> {
                        if decoded_image_present_sequence_error.is_some() {
                            return Ok(());
                        }
                        pending_decoded_present_frames.push_keep_last(
                            NativeVulkanVulkanaliaPendingDecodedPresentFrame::new(
                                decode_frame_index,
                                sampled_array_layer,
                                source_frame_pts_ms,
                                source_frame_duration_ms,
                                display_order_key,
                                display_order_key_source,
                            ),
                        );
                        Ok(())
                    };
                    let av1_ready_prefix_decode =
                        if let Some((input, bitstream_buffer_size)) = av1_ready_prefix_decode {
                            Some(
                                native_vulkan_vulkanalia_record_av1_ready_prefix_decode_into_image(
                                    &context.device,
                                    context.video_queue,
                                    &memory_properties,
                                    selection.video_queue_family_index,
                                    profile_info,
                                    requested_extent,
                                    queried.capabilities,
                                    session
                                        .as_ref()
                                        .copied()
                                        .expect("Vulkanalia video session is live"),
                                    codec,
                                    resource_image_array_layers,
                                    bitstream_buffer_size,
                                    input,
                                    resource_image
                                        .as_ref()
                                        .expect("Vulkanalia resource image is live"),
                                    Some(&mut enqueue_decoded_frame),
                                )?,
                            )
                        } else {
                            None
                        };
                    let h264_ready_prefix_decode =
                        if let Some((input, bitstream_buffer_size)) = h264_ready_prefix_decode {
                            Some(
                            native_vulkan_vulkanalia_record_h264_ready_prefix_decode_into_image(
                                &context.device,
                                context.video_queue,
                                &memory_properties,
                                selection.video_queue_family_index,
                                profile_info,
                                requested_extent,
                                queried.capabilities,
                                session
                                    .as_ref()
                                    .copied()
                                    .expect("Vulkanalia video session is live"),
                                codec,
                                resource_image_array_layers,
                                bitstream_buffer_size,
                                input,
                                resource_image
                                    .as_ref()
                                    .expect("Vulkanalia resource image is live"),
                                Some(&mut enqueue_decoded_frame),
                            )?,
                        )
                        } else {
                            None
                        };
                    let h265_ready_prefix_decode =
                        if let Some((input, bitstream_buffer_size)) = h265_ready_prefix_decode {
                            Some(
                            native_vulkan_vulkanalia_record_h265_ready_prefix_decode_into_image(
                                &context.device,
                                context.video_queue,
                                &memory_properties,
                                selection.video_queue_family_index,
                                profile_info,
                                requested_extent,
                                queried.capabilities,
                                session
                                    .as_ref()
                                    .copied()
                                    .expect("Vulkanalia video session is live"),
                                codec,
                                resource_image_array_layers,
                                bitstream_buffer_size,
                                input,
                                resource_image
                                    .as_ref()
                                    .expect("Vulkanalia resource image is live"),
                                Some(&mut enqueue_decoded_frame),
                            )?,
                        )
                        } else {
                            None
                        };
                    drop(enqueue_decoded_frame);
                    let decoded_present_frame_count = av1_ready_prefix_decode
                        .as_ref()
                        .map(|decode| decode.submitted_frame_count)
                        .or_else(|| {
                            h264_ready_prefix_decode
                                .as_ref()
                                .map(|decode| decode.submitted_frame_count)
                        })
                        .or_else(|| {
                            h265_ready_prefix_decode
                                .as_ref()
                                .map(|decode| decode.submitted_frame_count)
                        })
                        .unwrap_or(0);
                    let requested_present_frame_count =
                        decoded_present_frame_count.max(requested_present_frame_count);
                    if decoded_image_present_sequence_error.is_none() {
                        let present_result =
                            native_vulkan_vulkanalia_flush_pending_decoded_present_frames(
                                &context.device,
                                context.present_queue,
                                swapchain_handle,
                                swapchain_images,
                                swapchain_format,
                                swapchain_extent,
                                resource_image
                                    .as_ref()
                                    .expect("Vulkanalia resource image is live"),
                                picture_format,
                                decoded_image_present_sampler
                                    .as_mut()
                                    .expect("Vulkanalia decoded image present sampler is live"),
                                decoded_image_present_pipeline
                                    .as_ref()
                                    .expect("Vulkanalia decoded image present pipeline is live"),
                                decoded_image_present_frame_resources.as_ref().ok_or_else(|| {
                                    "decoded image present sequence has no reusable frame resources"
                                        .to_owned()
                                })?,
                                sequence_started_at,
                                &mut first_present_pts_ms,
                                target_max_fps,
                                &mut pending_decoded_present_frames,
                                decoded_image_present_timing,
                                requested_present_frame_count,
                            );
                        match present_result {
                            Ok((draws, handoff_snapshot)) => {
                                decoded_image_present_draws = draws;
                                decoded_image_present_handoff_snapshot = Some(handoff_snapshot);
                            }
                            Err(err) => decoded_image_present_sequence_error = Some(err),
                        }
                    }
                    (
                        av1_ready_prefix_decode,
                        h264_ready_prefix_decode,
                        h265_ready_prefix_decode,
                        requested_present_frame_count,
                    )
                };
                let decoded_image_present_sequence =
                    native_vulkan_vulkanalia_decoded_image_present_sequence_from_draws(
                        requested_present_frame_count,
                        decoded_image_present_draws,
                        decoded_image_present_handoff_snapshot.unwrap_or_else(|| {
                            native_vulkan_vulkanalia_present_handoff_snapshot_for_ready_prefix(
                                requested_present_frame_count,
                                swapchain_images.len(),
                                resource_image_array_layers,
                            )
                        }),
                    );
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
                    decoded_image_present_sequence,
                    decoded_image_present_sequence_error,
                    av1_ready_prefix_decode,
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
                        decoded_image_present_boundary: "retained Vulkanalia runtime owns video session memory, coincident sampled DPB/output image, YCbCr sampler/descriptor resources when supported, and Wayland swapchain until the caller drops the runtime; next step records the dynamic-rendering fullscreen draw into the graphics present pass",
                        ffmpeg_reference: FFMPEG_VULKAN_DECODE_REFERENCE,
                    },
                    h265_ready_prefix_decode,
                })
            })();

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

#[allow(clippy::too_many_arguments)]
fn native_vulkan_vulkanalia_flush_pending_decoded_present_frames(
    device: &Device,
    present_queue: vk::Queue,
    swapchain: vk::SwapchainKHR,
    swapchain_images: &[vk::Image],
    swapchain_format: vk::Format,
    swapchain_extent: vk::Extent2D,
    resource_image: &VulkanaliaVideoSessionResourceImage,
    picture_format: vk::Format,
    decoded_image_present_sampler: &mut VulkanaliaDecodedImagePresentSamplerResources,
    decoded_image_present_pipeline: &VulkanaliaDecodedImagePresentPipelineResources,
    decoded_image_present_frame_resources: &VulkanaliaDecodedImagePresentFrameResources,
    sequence_started_at: Instant,
    first_present_pts_ms: &mut Option<u64>,
    target_max_fps: Option<u32>,
    pending_frames: &mut NativeVulkanVulkanaliaDecodedPresentHandoff,
    present_timing: VulkanaliaDecodedImagePresentTimingConfig,
    requested_present_frame_count: u32,
) -> Result<
    (
        Vec<NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot>,
        NativeVulkanVulkanaliaDecodedPresentHandoffSnapshot,
    ),
    String,
> {
    let (frames, handoff_snapshot) = pending_frames.drain_sorted();
    let frames = native_vulkan_vulkanalia_repeat_ready_prefix_present_frames(
        &frames,
        requested_present_frame_count,
    )?;
    let mut draws = Vec::with_capacity(frames.len());
    for (present_frame_index, frame) in frames.into_iter().enumerate() {
        let present_frame_index = u32::try_from(present_frame_index)
            .map_err(|_| "Vulkanalia present frame index exceeds u32".to_owned())?;
        native_vulkan_vulkanalia_retarget_decoded_image_present_sampler_layer(
            device,
            resource_image,
            picture_format,
            decoded_image_present_sampler,
            frame.sampled_array_layer,
        )?;
        let (pacing_sleep_micros, pacing_clock_model) = native_vulkan_vulkanalia_pace_present_frame(
            sequence_started_at,
            first_present_pts_ms,
            present_frame_index,
            frame.source_frame_pts_ms,
            frame.source_frame_duration_ms,
            target_max_fps,
        );
        draws.push(native_vulkan_vulkanalia_present_decoded_image_frame(
            device,
            present_queue,
            swapchain,
            swapchain_images,
            swapchain_format,
            swapchain_extent,
            resource_image,
            decoded_image_present_sampler,
            decoded_image_present_pipeline,
            decoded_image_present_frame_resources,
            frame.sampled_array_layer,
            present_frame_index,
            frame.source_frame_pts_ms,
            frame.source_frame_duration_ms,
            frame.display_order_key,
            frame.display_order_key_source,
            pacing_sleep_micros,
            pacing_clock_model,
            present_timing,
        )?);
    }

    Ok((draws, handoff_snapshot))
}

fn native_vulkan_vulkanalia_repeat_ready_prefix_present_frames(
    frames: &[NativeVulkanVulkanaliaPendingDecodedPresentFrame],
    requested_present_frame_count: u32,
) -> Result<Vec<NativeVulkanVulkanaliaPendingDecodedPresentFrame>, String> {
    if frames.is_empty() {
        return Ok(Vec::new());
    }
    let requested_present_frame_count = usize::try_from(requested_present_frame_count)
        .map_err(|_| "Vulkanalia requested present frame count exceeds usize".to_owned())?;
    if requested_present_frame_count == 0 {
        return Ok(frames.to_vec());
    }

    let pts_cycle_span_ms = native_vulkan_vulkanalia_ready_prefix_pts_cycle_span_ms(frames);
    let display_key_cycle_span =
        native_vulkan_vulkanalia_ready_prefix_display_key_cycle_span(frames, pts_cycle_span_ms);
    let mut repeated = Vec::with_capacity(requested_present_frame_count);
    for present_index in 0..requested_present_frame_count {
        let source = &frames[present_index % frames.len()];
        let cycle = present_index / frames.len();
        let mut frame = source.clone();
        if cycle > 0 {
            let cycle_u64 = u64::try_from(cycle).unwrap_or(u64::MAX);
            let cycle_i64 = i64::try_from(cycle).unwrap_or(i64::MAX);
            if let (Some(pts_ms), Some(span_ms)) = (frame.source_frame_pts_ms, pts_cycle_span_ms) {
                frame.source_frame_pts_ms =
                    Some(pts_ms.saturating_add(span_ms.saturating_mul(cycle_u64)));
            }
            frame.display_order_key = frame
                .display_order_key
                .saturating_add(display_key_cycle_span.saturating_mul(cycle_i64));
        }
        repeated.push(frame);
    }
    Ok(repeated)
}

fn native_vulkan_vulkanalia_ready_prefix_pts_cycle_span_ms(
    frames: &[NativeVulkanVulkanaliaPendingDecodedPresentFrame],
) -> Option<u64> {
    let first_pts = frames.first()?.source_frame_pts_ms?;
    let last = frames.last()?;
    let last_pts = last.source_frame_pts_ms?;
    let last_duration = last.source_frame_duration_ms.unwrap_or_else(|| {
        frames
            .windows(2)
            .filter_map(|pair| {
                pair[1]
                    .source_frame_pts_ms?
                    .checked_sub(pair[0].source_frame_pts_ms?)
            })
            .find(|delta| *delta > 0)
            .unwrap_or(1)
    });
    Some(
        last_pts
            .saturating_sub(first_pts)
            .saturating_add(last_duration.max(1)),
    )
}

fn native_vulkan_vulkanalia_ready_prefix_display_key_cycle_span(
    frames: &[NativeVulkanVulkanaliaPendingDecodedPresentFrame],
    pts_cycle_span_ms: Option<u64>,
) -> i64 {
    if let Some(pts_cycle_span_ms) = pts_cycle_span_ms {
        return i64::try_from(pts_cycle_span_ms.max(1)).unwrap_or(i64::MAX);
    }
    let first = frames
        .first()
        .map(|frame| frame.display_order_key)
        .unwrap_or(0);
    let last = frames
        .last()
        .map(|frame| frame.display_order_key)
        .unwrap_or(first);
    last.saturating_sub(first).saturating_add(1).max(1)
}

fn native_vulkan_vulkanalia_decoded_image_present_sequence_from_draws(
    requested_present_frame_count: u32,
    draws: Vec<NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot>,
    present_handoff: NativeVulkanVulkanaliaDecodedPresentHandoffSnapshot,
) -> Option<NativeVulkanVulkanaliaDecodedImagePresentSequenceSnapshot> {
    if draws.is_empty() {
        return None;
    }
    let sampled_array_layers = draws
        .iter()
        .map(|draw| draw.sampled_array_layer)
        .collect::<Vec<_>>();
    let submitted_present_frame_count = draws.iter().filter(|draw| draw.submitted).count() as u32;
    let presented_frame_count = draws.iter().filter(|draw| draw.presented).count() as u32;
    let all_zero_copy_presented = draws.iter().all(|draw| draw.zero_copy_presented);
    let source_frame_pts_ms = draws
        .iter()
        .map(|draw| draw.source_frame_pts_ms)
        .collect::<Vec<_>>();
    let source_frame_duration_ms = draws
        .iter()
        .map(|draw| draw.source_frame_duration_ms)
        .collect::<Vec<_>>();
    let display_order_keys = draws
        .iter()
        .map(|draw| draw.display_order_key)
        .collect::<Vec<_>>();
    let display_order_key_sources = draws
        .iter()
        .map(|draw| draw.display_order_key_source)
        .collect::<Vec<_>>();
    let present_ids = draws.iter().map(|draw| draw.present_id).collect::<Vec<_>>();
    let total_pacing_sleep_micros = draws
        .iter()
        .map(|draw| draw.pacing_sleep_micros)
        .sum::<u64>();
    let pts_values = source_frame_pts_ms
        .iter()
        .flatten()
        .copied()
        .collect::<Vec<_>>();
    let pts_monotonic = pts_values.windows(2).all(|pair| pair[0] <= pair[1]);
    let display_order_monotonic = display_order_keys.windows(2).all(|pair| pair[0] <= pair[1]);
    let uses_present_id = draws.iter().any(|draw| draw.uses_present_id);
    let uses_present_id2 = draws.iter().any(|draw| draw.uses_present_id2);
    let present_wait_available = draws.iter().any(|draw| draw.present_wait_available);
    let present_wait2_available = draws.iter().any(|draw| draw.present_wait2_available);
    let present_wait_after_present = draws.iter().any(|draw| draw.present_wait_after_present);
    Some(NativeVulkanVulkanaliaDecodedImagePresentSequenceSnapshot {
        binding: "vulkanalia",
        route: "decoded-image-dynamic-rendering-present-sequence",
        requested_present_frame_count,
        submitted_present_frame_count,
        presented_frame_count,
        sampled_array_layers,
        source_frame_pts_ms,
        source_frame_duration_ms,
        display_order_keys,
        display_order_key_sources,
        present_ids,
        total_pacing_sleep_micros,
        pts_monotonic,
        display_order_monotonic,
        uses_present_id,
        uses_present_id2,
        present_wait_available,
        present_wait2_available,
        present_wait_after_present,
        present_handoff,
        draws,
        frame_order_model: "FFmpeg-style display-key scheduler: decode submissions enqueue into a bounded keep-last handoff, then present drain sorts by PTS/POC/order-hint with decode-index tie-break; ready-prefix windows may be looped as metadata-only sampled-layer references before Vulkanalia dynamic rendering",
        present_resource_reuse_model: "one swapchain image-view set, one command pool, one semaphore pair, one fence set and one bounded decoded-frame handoff reused across decoded-image present frames",
        all_zero_copy_presented,
        uses_dynamic_rendering: true,
        uses_synchronization2: true,
        uses_submit2: true,
        ffmpeg_reference: FFMPEG_VULKAN_DECODE_REFERENCE,
    })
}

fn native_vulkan_vulkanalia_pace_present_frame(
    sequence_started_at: Instant,
    first_present_pts_ms: &mut Option<u64>,
    present_frame_index: u32,
    source_frame_pts_ms: Option<u64>,
    source_frame_duration_ms: Option<u64>,
    target_max_fps: Option<u32>,
) -> (u64, &'static str) {
    let (target_offset, clock_model) = if let Some(pts_ms) = source_frame_pts_ms {
        let base_pts_ms = *first_present_pts_ms.get_or_insert(pts_ms);
        (
            Duration::from_millis(pts_ms.saturating_sub(base_pts_ms)),
            "pts-video-clock-sleep",
        )
    } else if let Some(target_max_fps) = target_max_fps {
        (
            native_vulkan_vulkanalia_frame_index_duration(present_frame_index, target_max_fps),
            "target-fps-video-clock-sleep",
        )
    } else if let Some(duration_ms) = source_frame_duration_ms {
        (
            Duration::from_millis(duration_ms.saturating_mul(u64::from(present_frame_index))),
            "duration-video-clock-sleep",
        )
    } else {
        return (0, "unpaced-no-video-clock");
    };

    let deadline = sequence_started_at + target_offset;
    let now = Instant::now();
    if deadline <= now {
        return (0, clock_model);
    }
    let sleep_duration = deadline.duration_since(now);
    thread::sleep(sleep_duration);
    (
        u64::try_from(sleep_duration.as_micros()).unwrap_or(u64::MAX),
        clock_model,
    )
}

fn native_vulkan_vulkanalia_frame_index_duration(
    frame_index: u32,
    target_max_fps: u32,
) -> Duration {
    let fps = u128::from(target_max_fps.max(1));
    let nanos = u128::from(frame_index).saturating_mul(1_000_000_000u128) / fps;
    Duration::from_nanos(nanos.min(u128::from(u64::MAX)) as u64)
}

fn decoded_image_present_frame_count_hint(
    av1_frame_count: Option<usize>,
    h264_frame_count: Option<usize>,
    h265_frame_count: Option<usize>,
) -> u32 {
    av1_frame_count
        .or(h264_frame_count)
        .or(h265_frame_count)
        .and_then(|count| u32::try_from(count).ok())
        .unwrap_or(0)
}

fn native_vulkan_vulkanalia_ready_prefix_resource_image_array_layers(
    session_max_dpb_slots: u32,
    av1_ready_prefix_decode: Option<&NativeVulkanVulkanaliaAv1ReadyPrefixDecodeInput>,
    h264_ready_prefix_decode: Option<&NativeVulkanVulkanaliaH264ReadyPrefixDecodeInput>,
    h265_ready_prefix_decode: Option<&NativeVulkanVulkanaliaH265ReadyPrefixDecodeInput>,
) -> u32 {
    let required_layers = av1_ready_prefix_decode
        .map(native_vulkan_vulkanalia_av1_ready_prefix_resource_image_array_layers)
        .or_else(|| {
            h264_ready_prefix_decode
                .map(native_vulkan_vulkanalia_h264_ready_prefix_resource_image_array_layers)
        })
        .or_else(|| {
            h265_ready_prefix_decode
                .map(native_vulkan_vulkanalia_h265_ready_prefix_resource_image_array_layers)
        })
        .unwrap_or(1);

    native_vulkan_vulkanalia_clamp_ready_prefix_resource_image_array_layers(
        required_layers,
        session_max_dpb_slots,
    )
}

fn native_vulkan_vulkanalia_av1_ready_prefix_resource_image_array_layers(
    input: &NativeVulkanVulkanaliaAv1ReadyPrefixDecodeInput,
) -> u32 {
    native_vulkan_vulkanalia_av1_reference_plan_resource_image_array_layers(
        input
            .frames
            .iter()
            .take(native_vulkan_vulkanalia_requested_ready_prefix_frame_count(
                input.requested_frame_count,
            ))
            .map(|frame| &frame.entry),
    )
}

fn native_vulkan_vulkanalia_h264_ready_prefix_resource_image_array_layers(
    input: &NativeVulkanVulkanaliaH264ReadyPrefixDecodeInput,
) -> u32 {
    native_vulkan_vulkanalia_h264_reference_plan_resource_image_array_layers(
        input
            .frames
            .iter()
            .take(native_vulkan_vulkanalia_requested_ready_prefix_frame_count(
                input.requested_frame_count,
            ))
            .map(|frame| &frame.entry),
    )
}

fn native_vulkan_vulkanalia_h265_ready_prefix_resource_image_array_layers(
    input: &NativeVulkanVulkanaliaH265ReadyPrefixDecodeInput,
) -> u32 {
    native_vulkan_vulkanalia_h265_reference_plan_resource_image_array_layers(
        input
            .frames
            .iter()
            .take(native_vulkan_vulkanalia_requested_ready_prefix_frame_count(
                input.requested_frame_count,
            ))
            .map(|frame| &frame.entry),
    )
}

fn native_vulkan_vulkanalia_requested_ready_prefix_frame_count(
    requested_frame_count: u32,
) -> usize {
    usize::try_from(requested_frame_count).unwrap_or(usize::MAX)
}

fn native_vulkan_vulkanalia_av1_reference_plan_resource_image_array_layers<'a>(
    entries: impl IntoIterator<Item = &'a NativeVulkanAv1DecodeReferencePlanEntrySnapshot>,
) -> u32 {
    let mut max_slot = None;
    for entry in entries {
        native_vulkan_vulkanalia_track_optional_slot(&mut max_slot, entry.output_slot);
        native_vulkan_vulkanalia_track_optional_slot(&mut max_slot, entry.displayed_slot);
        for slot in &entry.decode_reference_slots {
            native_vulkan_vulkanalia_track_non_negative_slot(&mut max_slot, *slot);
        }
        for slot in &entry.reference_name_slot_indices {
            native_vulkan_vulkanalia_track_non_negative_slot(&mut max_slot, *slot);
        }
        for slot in &entry.map_slot_indices_after {
            native_vulkan_vulkanalia_track_non_negative_slot(&mut max_slot, *slot);
        }
    }

    native_vulkan_vulkanalia_array_layers_from_max_slot(max_slot)
}

fn native_vulkan_vulkanalia_h264_reference_plan_resource_image_array_layers<'a>(
    entries: impl IntoIterator<Item = &'a NativeVulkanH264DecodeReferencePlanEntrySnapshot>,
) -> u32 {
    let mut max_slot = None;
    for entry in entries {
        native_vulkan_vulkanalia_track_slot(&mut max_slot, entry.planned_output_slot);
        if let Some(setup_slot_index) = entry.setup_slot_index {
            native_vulkan_vulkanalia_track_non_negative_slot(&mut max_slot, setup_slot_index);
        }
        for reference in &entry.references {
            native_vulkan_vulkanalia_track_optional_slot(&mut max_slot, reference.dpb_slot);
        }
        for reference in &entry.inferred_non_existing_references {
            native_vulkan_vulkanalia_track_slot(&mut max_slot, reference.dpb_slot);
        }
    }

    native_vulkan_vulkanalia_array_layers_from_max_slot(max_slot)
}

fn native_vulkan_vulkanalia_h265_reference_plan_resource_image_array_layers<'a>(
    entries: impl IntoIterator<Item = &'a NativeVulkanH265DecodeReferencePlanEntrySnapshot>,
) -> u32 {
    let mut max_slot = None;
    for entry in entries {
        native_vulkan_vulkanalia_track_slot(&mut max_slot, entry.planned_output_slot);
        if let Some(setup_slot_index) = entry.setup_slot_index {
            native_vulkan_vulkanalia_track_non_negative_slot(&mut max_slot, setup_slot_index);
        }
        for reference in &entry.references {
            native_vulkan_vulkanalia_track_optional_slot(&mut max_slot, reference.dpb_slot);
        }
    }

    native_vulkan_vulkanalia_array_layers_from_max_slot(max_slot)
}

fn native_vulkan_vulkanalia_clamp_ready_prefix_resource_image_array_layers(
    required_layers: u32,
    session_max_dpb_slots: u32,
) -> u32 {
    let required_layers = required_layers.max(1);
    if session_max_dpb_slots == 0 {
        required_layers
    } else {
        required_layers.min(session_max_dpb_slots).max(1)
    }
}

fn native_vulkan_vulkanalia_array_layers_from_max_slot(max_slot: Option<u32>) -> u32 {
    max_slot
        .map(|slot| slot.saturating_add(1))
        .unwrap_or(1)
        .max(1)
}

fn native_vulkan_vulkanalia_track_optional_slot(max_slot: &mut Option<u32>, slot: Option<u32>) {
    if let Some(slot) = slot {
        native_vulkan_vulkanalia_track_slot(max_slot, slot);
    }
}

fn native_vulkan_vulkanalia_track_non_negative_slot(max_slot: &mut Option<u32>, slot: i32) {
    if let Ok(slot) = u32::try_from(slot) {
        native_vulkan_vulkanalia_track_slot(max_slot, slot);
    }
}

fn native_vulkan_vulkanalia_track_slot(max_slot: &mut Option<u32>, slot: u32) {
    *max_slot = Some(max_slot.map_or(slot, |current| current.max(slot)));
}

fn native_vulkan_vulkanalia_present_handoff_capacity(
    requested_present_frame_count: u32,
    swapchain_image_count: usize,
    resource_image_array_layers: u32,
) -> usize {
    usize::try_from(requested_present_frame_count)
        .unwrap_or(usize::MAX)
        .max(swapchain_image_count)
        .max(usize::try_from(resource_image_array_layers).unwrap_or(usize::MAX))
        .max(1)
}

fn native_vulkan_vulkanalia_present_handoff_snapshot_for_ready_prefix(
    requested_present_frame_count: u32,
    swapchain_image_count: usize,
    resource_image_array_layers: u32,
) -> NativeVulkanVulkanaliaDecodedPresentHandoffSnapshot {
    let capacity_frames = native_vulkan_vulkanalia_present_handoff_capacity(
        requested_present_frame_count,
        swapchain_image_count,
        resource_image_array_layers,
    );
    NativeVulkanVulkanaliaDecodedPresentHandoffSnapshot {
        binding: "vulkanalia",
        route: "decoded-image-present-bounded-keep-last-handoff",
        model: "bounded decoded-frame handoff between Vulkan Video decode completion and Vulkanalia dynamic-rendering present",
        capacity_frames,
        queued_frame_count_before_drain: 0,
        enqueued_frame_count: 0,
        dropped_frame_count: 0,
        drained_frame_count: 0,
        peak_depth: 0,
        keep_last_overwrite_enabled: true,
        drop_policy: "when the handoff is full, drop the oldest display-order frame and keep the newest decoded frame",
        drain_order: "sort by display_order_key, then decode_frame_index",
        zero_copy_scope: "handoff stores decoded image array-layer identities and timing metadata only; frame pixels stay in Vulkan images",
        ffmpeg_reference: FFMPEG_VULKAN_DECODE_REFERENCE,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        NativeVulkanVulkanaliaPendingDecodedPresentFrame,
        VIDEO_PRESENT_SESSION_RETAINED_RESOURCE_ROUTE, decoded_image_present_frame_count_hint,
        native_vulkan_vulkanalia_av1_reference_plan_resource_image_array_layers,
        native_vulkan_vulkanalia_clamp_ready_prefix_resource_image_array_layers,
        native_vulkan_vulkanalia_h264_reference_plan_resource_image_array_layers,
        native_vulkan_vulkanalia_h265_reference_plan_resource_image_array_layers,
        native_vulkan_vulkanalia_present_handoff_capacity,
        native_vulkan_vulkanalia_repeat_ready_prefix_present_frames,
    };
    use crate::renderer::native_vulkan::{
        NativeVulkanAv1DecodeReferencePlanEntrySnapshot,
        NativeVulkanH264DecodeReferencePlanEntrySnapshot, NativeVulkanH264DecodeReferenceSnapshot,
        NativeVulkanH264InferredNonExistingReferenceSnapshot,
        NativeVulkanH265DecodeReferencePlanEntrySnapshot, NativeVulkanH265DecodeReferenceSnapshot,
    };

    #[test]
    fn retained_runtime_snapshot_names_resource_ownership_boundary() {
        assert!(
            VIDEO_PRESENT_SESSION_RETAINED_RESOURCE_ROUTE.contains("retained"),
            "route label should make this gate distinct from immediate probe allocation"
        );
    }

    #[test]
    fn ready_prefix_handoff_capacity_preserves_current_smoke_window() {
        assert_eq!(
            native_vulkan_vulkanalia_present_handoff_capacity(16, 3, 8),
            16
        );
        assert_eq!(
            native_vulkan_vulkanalia_present_handoff_capacity(0, 3, 1),
            3
        );
        assert_eq!(
            native_vulkan_vulkanalia_present_handoff_capacity(0, 0, 0),
            1
        );
    }

    #[test]
    fn h264_resource_image_layers_follow_actual_ready_prefix_slots() {
        let entry = NativeVulkanH264DecodeReferencePlanEntrySnapshot {
            access_unit_index: 2,
            pts_ms: Some(8),
            nal_type_label: Some("non-idr-slice"),
            current_frame_num: Some(2),
            current_pic_order_cnt_val: Some(4),
            current_pic_order_cnt: Some([4, 4]),
            current_long_term_frame_idx: None,
            planned_output_slot: 0,
            setup_slot_index: Some(1),
            evicted_frame_num: None,
            evicted_long_term_frame_idx: None,
            dropped_reference_frame_nums: Vec::new(),
            dropped_long_term_frame_indices: Vec::new(),
            inferred_non_existing_frame_nums: vec![1],
            inferred_non_existing_references: vec![
                NativeVulkanH264InferredNonExistingReferenceSnapshot {
                    frame_num: 1,
                    field_pic_flag: false,
                    bottom_field_flag: false,
                    pic_order_cnt_val: 2,
                    pic_order_cnt: [2, 2],
                    dpb_slot: 2,
                },
            ],
            inferred_dropped_reference_frame_nums: Vec::new(),
            inferred_dropped_long_term_frame_indices: Vec::new(),
            inferred_dropped_reference_slots: Vec::new(),
            long_term_reference_conversions: Vec::new(),
            dropped_reference_slots: Vec::new(),
            requested_reference_count: 1,
            references: vec![NativeVulkanH264DecodeReferenceSnapshot {
                frame_num: 0,
                field_pic_flag: false,
                bottom_field_flag: false,
                used_for_long_term_reference: false,
                long_term_frame_idx: None,
                long_term_pic_num: None,
                non_existing: false,
                pic_order_cnt_val: 0,
                pic_order_cnt: [0, 0],
                available: true,
                source_access_unit_index: Some(0),
                dpb_slot: Some(1),
            }],
            available_reference_count: 1,
            missing_reference_count: 0,
            unsupported_reason: None,
            ready_for_decode_submit: true,
        };

        let required_layers =
            native_vulkan_vulkanalia_h264_reference_plan_resource_image_array_layers(
                std::iter::once(&entry),
            );
        assert_eq!(required_layers, 3);
        assert_eq!(
            native_vulkan_vulkanalia_clamp_ready_prefix_resource_image_array_layers(
                required_layers,
                16,
            ),
            3
        );
    }

    #[test]
    fn h265_resource_image_layers_include_references_and_setup_slot() {
        let entry = NativeVulkanH265DecodeReferencePlanEntrySnapshot {
            access_unit_index: 3,
            pts_ms: Some(12),
            nal_type_label: Some("TRAIL_R"),
            current_poc: Some(6),
            planned_output_slot: 1,
            setup_slot_index: Some(2),
            evicted_poc: None,
            references: vec![NativeVulkanH265DecodeReferenceSnapshot {
                delta_poc: -2,
                poc: 4,
                used_for_long_term_reference: false,
                available: true,
                source_access_unit_index: Some(2),
                dpb_slot: Some(3),
            }],
            available_reference_count: 1,
            missing_reference_count: 0,
            missing_reference_pocs: Vec::new(),
            ready_for_decode_submit: true,
        };

        assert_eq!(
            native_vulkan_vulkanalia_h265_reference_plan_resource_image_array_layers(
                std::iter::once(&entry),
            ),
            4
        );
    }

    #[test]
    fn av1_resource_image_layers_ignore_negative_sentinels() {
        let entry = NativeVulkanAv1DecodeReferencePlanEntrySnapshot {
            temporal_unit_index: 4,
            frame_type_label: "inter",
            show_existing_frame: false,
            frame_to_show_map_idx: None,
            show_frame: true,
            order_hint: Some(6),
            current_frame_id: Some(9),
            expected_frame_ids: vec![0; 8],
            refresh_frame_flags: 0x04,
            output_slot: Some(2),
            displayed_slot: Some(4),
            reference_name_slot_indices: vec![1, -1, 3, -1, -1, -1, -1],
            reference_name_order_hints: vec![None; 8],
            map_order_hints: vec![None; 8],
            ref_frame_indices: vec![0],
            decode_reference_slots: vec![1, -1, -1, -1, -1, -1, -1],
            refreshed_reference_names: vec![2],
            missing_reference_names: Vec::new(),
            missing_reference_count: 0,
            references_resolved: true,
            submit_fields_ready: true,
            ready_for_decode_submit: true,
            ready_for_display_handoff: true,
            unsupported_reason: None,
            map_slot_indices_after: vec![-1, 1, 2, -1, 4, -1, -1, -1],
            map_order_hints_after: vec![None; 8],
        };

        assert_eq!(
            native_vulkan_vulkanalia_av1_reference_plan_resource_image_array_layers(
                std::iter::once(&entry),
            ),
            5
        );
    }

    #[test]
    fn empty_ready_prefix_resource_image_layers_fall_back_to_one() {
        let h264_entries: Vec<NativeVulkanH264DecodeReferencePlanEntrySnapshot> = Vec::new();
        assert_eq!(
            native_vulkan_vulkanalia_h264_reference_plan_resource_image_array_layers(
                h264_entries.iter(),
            ),
            1
        );
        assert_eq!(
            native_vulkan_vulkanalia_clamp_ready_prefix_resource_image_array_layers(1, 16),
            1
        );
    }

    #[test]
    fn decoded_image_present_frame_count_hint_prefers_active_codec() {
        assert_eq!(
            decoded_image_present_frame_count_hint(Some(5), Some(7), Some(9)),
            5
        );
        assert_eq!(
            decoded_image_present_frame_count_hint(None, Some(7), Some(9)),
            7
        );
        assert_eq!(decoded_image_present_frame_count_hint(None, None, None), 0);
    }

    #[test]
    fn repeats_ready_prefix_present_window_to_requested_playback_count() {
        let frames = vec![
            NativeVulkanVulkanaliaPendingDecodedPresentFrame::new(
                0,
                0,
                Some(0),
                Some(4),
                0,
                "pts-ms",
            ),
            NativeVulkanVulkanaliaPendingDecodedPresentFrame::new(
                1,
                1,
                Some(4),
                Some(4),
                4,
                "pts-ms",
            ),
        ];

        let repeated =
            native_vulkan_vulkanalia_repeat_ready_prefix_present_frames(&frames, 5).unwrap();

        assert_eq!(
            repeated
                .iter()
                .map(|frame| (
                    frame.sampled_array_layer,
                    frame.source_frame_pts_ms,
                    frame.display_order_key
                ))
                .collect::<Vec<_>>(),
            vec![
                (0, Some(0), 0),
                (1, Some(4), 4),
                (0, Some(8), 8),
                (1, Some(12), 12),
                (0, Some(16), 16),
            ]
        );
    }
}
