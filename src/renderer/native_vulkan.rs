//! Hand-rolled Vulkan renderer spike.
//!
//! This module is intentionally separate from the existing wgpu path. The first
//! step is a concrete backend contract: native Wayland layer-shell ownership,
//! Vulkan surface/swapchain ownership, and direct video texture interop are
//! represented here before any default renderer switch is attempted.

#![allow(unsafe_code)]

use serde::Serialize;
#[cfg(feature = "native-vulkan-gst-video")]
use std::ffi::c_void;
use std::ffi::{CStr, CString};
use std::fmt;
#[cfg(feature = "native-vulkan-gst-video")]
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd};
#[cfg(feature = "native-vulkan-gst-video")]
use std::os::raw::c_char;
#[cfg(feature = "native-vulkan-gst-video")]
use std::path::Path;
use std::path::PathBuf;
use std::ptr;
use std::thread;
use std::time::{Duration, Instant};

use crate::config::VideoDecoderPolicy;
use crate::core::{FitMode, Transition};
use crate::renderer::native_wayland::{
    NativeWaylandError, NativeWaylandHost, NativeWaylandHostOptions, NativeWaylandSurfaceHandles,
};
#[cfg(feature = "native-vulkan-gst-video")]
use crate::renderer::video::{
    actual_decoder_reports, apply_decoder_rank_policy, decoder_policy_status, video_caps_reports,
};
use crate::renderer::{
    SceneLiteDisplayPlan, SceneLiteWallpaperPlan, SlideshowWallpaperPlan, StaticRenderSyncPlan,
    StaticWallpaperPlan, VideoWallpaperPlan,
};
use ash::vk;
#[cfg(feature = "native-vulkan-gst-video")]
use gst::prelude::*;
#[cfg(feature = "native-vulkan-gst-video")]
use gstreamer as gst;
#[cfg(feature = "native-vulkan-gst-video")]
use gstreamer_video as gst_video;

const NATIVE_VULKAN_VIDEO_CODEC_OPERATION_DECODE_VP9: u32 = 0x0000_0008;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanCapabilities {
    pub built: bool,
    pub experimental: bool,
    pub default_enabled: bool,
    pub reuses_native_wayland_host: bool,
    pub owns_layer_shell_surface_now: bool,
    pub owns_vulkan_instance_now: bool,
    pub owns_vulkan_device_now: bool,
    pub owns_wayland_vulkan_surface_now: bool,
    pub owns_swapchain_now: bool,
    pub renders_frames_now: bool,
    pub consumes_render_sync: bool,
    pub direct_video_memory_status: &'static str,
    pub unsafe_policy: &'static str,
}

pub fn capabilities() -> NativeVulkanCapabilities {
    NativeVulkanCapabilities {
        built: true,
        experimental: true,
        default_enabled: false,
        reuses_native_wayland_host: true,
        owns_layer_shell_surface_now: true,
        owns_vulkan_instance_now: true,
        owns_vulkan_device_now: true,
        owns_wayland_vulkan_surface_now: true,
        owns_swapchain_now: true,
        renders_frames_now: true,
        consumes_render_sync: false,
        direct_video_memory_status: "contract-only: target is importable DMABuf/EGLImage/Vulkan image sampling",
        unsafe_policy: "unsafe is allowed inside audited Vulkan/Wayland/DMABuf FFI boundaries only",
    }
}

#[derive(Debug)]
pub enum NativeVulkanError {
    Wayland(NativeWaylandError),
    Loading(String),
    Vulkan {
        operation: &'static str,
        result: vk::Result,
    },
    MissingDeviceExtension(&'static str),
    MissingPresentQueue,
    MissingSurfaceFormat,
    UnsupportedSwapchainUsage(&'static str),
    InvalidSwapchainExtent,
    StaticImage(String),
    Video(String),
    MissingMemoryType(&'static str),
}

impl fmt::Display for NativeVulkanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wayland(err) => write!(f, "{err}"),
            Self::Loading(err) => write!(f, "load Vulkan entry: {err}"),
            Self::Vulkan { operation, result } => write!(f, "{operation}: {result:?}"),
            Self::MissingDeviceExtension(extension) => {
                write!(f, "selected Vulkan device is missing {extension}")
            }
            Self::MissingPresentQueue => {
                write!(f, "no Vulkan graphics queue can present to Wayland surface")
            }
            Self::MissingSurfaceFormat => write!(f, "Wayland Vulkan surface has no formats"),
            Self::UnsupportedSwapchainUsage(usage) => {
                write!(
                    f,
                    "Wayland Vulkan surface does not support {usage} swapchain usage"
                )
            }
            Self::InvalidSwapchainExtent => write!(f, "invalid Vulkan swapchain extent"),
            Self::StaticImage(err) => write!(f, "static image error: {err}"),
            Self::Video(err) => write!(f, "video error: {err}"),
            Self::MissingMemoryType(label) => write!(f, "missing Vulkan memory type for {label}"),
        }
    }
}

impl std::error::Error for NativeVulkanError {}

impl From<NativeWaylandError> for NativeVulkanError {
    fn from(err: NativeWaylandError) -> Self {
        Self::Wayland(err)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeVulkanSurfaceProbeOptions {
    pub host: NativeWaylandHostOptions,
    pub wait_configure_roundtrips: usize,
}

impl Default for NativeVulkanSurfaceProbeOptions {
    fn default() -> Self {
        let mut host = NativeWaylandHostOptions::default();
        host.namespace = "gilder-native-vulkan".to_owned();
        Self {
            host,
            wait_configure_roundtrips: 8,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSurfaceProbeSnapshot {
    pub wayland_surface_logical_size: (u32, u32),
    pub wayland_surface_buffer_size: (u32, u32),
    pub dmabuf_main_device: Option<u64>,
    pub physical_device_count: usize,
    pub present_queue_family_count: usize,
    pub selected_physical_device_index: Option<usize>,
    pub selected_physical_device_name: Option<String>,
    pub selected_physical_device_type: Option<&'static str>,
    pub selected_queue_family_index: Option<u32>,
    pub selected_queue_supports_graphics: bool,
    pub surface_capabilities: Option<NativeVulkanSurfaceCapabilitiesSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSurfaceCapabilitiesSnapshot {
    pub min_image_count: u32,
    pub max_image_count: u32,
    pub current_extent: Option<(u32, u32)>,
    pub min_image_extent: (u32, u32),
    pub max_image_extent: (u32, u32),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoDecodeProbeSnapshot {
    pub physical_device_count: usize,
    pub devices: Vec<NativeVulkanVideoDecodeDeviceSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoDecodeDeviceSnapshot {
    pub physical_device_index: usize,
    pub physical_device_name: String,
    pub physical_device_type: &'static str,
    pub vendor_id: u32,
    pub device_id: u32,
    pub api_version: String,
    pub driver_version: u32,
    pub has_video_queue_extension: bool,
    pub has_video_decode_queue_extension: bool,
    pub decode_codec_extensions: Vec<String>,
    pub has_video_decode_queue_family: bool,
    pub video_decode_ready: bool,
    pub h264_direct_decode_ready: bool,
    pub h264_zero_copy_sampled_candidate: bool,
    pub h264_profiles: Vec<NativeVulkanVideoDecodeH264ProfileSnapshot>,
    pub h265_direct_decode_ready: bool,
    pub h265_zero_copy_sampled_candidate: bool,
    pub h265_profiles: Vec<NativeVulkanVideoDecodeH265ProfileSnapshot>,
    pub av1_direct_decode_ready: bool,
    pub av1_zero_copy_sampled_candidate: bool,
    pub av1_profiles: Vec<NativeVulkanVideoDecodeAv1ProfileSnapshot>,
    pub queue_families: Vec<NativeVulkanVideoDecodeQueueFamilySnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoDecodeQueueFamilySnapshot {
    pub queue_family_index: u32,
    pub queue_count: u32,
    pub queue_flags: Vec<&'static str>,
    pub video_codec_operation_bits: u32,
    pub video_codec_operations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoDecodeH264ProfileSnapshot {
    pub profile: &'static str,
    pub std_profile_idc: u32,
    pub picture_layout: &'static str,
    pub chroma_subsampling: Vec<&'static str>,
    pub luma_bit_depth: Vec<&'static str>,
    pub chroma_bit_depth: Vec<&'static str>,
    pub supported: bool,
    pub max_level_idc: Option<u32>,
    pub max_level: Option<&'static str>,
    pub capability_flags: Vec<&'static str>,
    pub decode_capability_flags: Vec<&'static str>,
    pub min_bitstream_buffer_offset_alignment: Option<u64>,
    pub min_bitstream_buffer_size_alignment: Option<u64>,
    pub picture_access_granularity: Option<(u32, u32)>,
    pub min_coded_extent: Option<(u32, u32)>,
    pub max_coded_extent: Option<(u32, u32)>,
    pub max_dpb_slots: Option<u32>,
    pub max_active_reference_pictures: Option<u32>,
    pub field_offset_granularity: Option<(i32, i32)>,
    pub dpb_formats: Vec<NativeVulkanVideoFormatPropertiesSnapshot>,
    pub output_formats: Vec<NativeVulkanVideoFormatPropertiesSnapshot>,
    pub sampled_output_formats: Vec<NativeVulkanVideoFormatPropertiesSnapshot>,
    pub nv12_dpb_supported: bool,
    pub nv12_output_supported: bool,
    pub nv12_sampled_output_supported: bool,
    pub query_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoDecodeH265ProfileSnapshot {
    pub profile: &'static str,
    pub std_profile_idc: u32,
    pub chroma_subsampling: Vec<&'static str>,
    pub luma_bit_depth: Vec<&'static str>,
    pub chroma_bit_depth: Vec<&'static str>,
    pub supported: bool,
    pub max_level_idc: Option<u32>,
    pub max_level: Option<&'static str>,
    pub capability_flags: Vec<&'static str>,
    pub decode_capability_flags: Vec<&'static str>,
    pub min_bitstream_buffer_offset_alignment: Option<u64>,
    pub min_bitstream_buffer_size_alignment: Option<u64>,
    pub picture_access_granularity: Option<(u32, u32)>,
    pub min_coded_extent: Option<(u32, u32)>,
    pub max_coded_extent: Option<(u32, u32)>,
    pub max_dpb_slots: Option<u32>,
    pub max_active_reference_pictures: Option<u32>,
    pub dpb_formats: Vec<NativeVulkanVideoFormatPropertiesSnapshot>,
    pub output_formats: Vec<NativeVulkanVideoFormatPropertiesSnapshot>,
    pub sampled_output_formats: Vec<NativeVulkanVideoFormatPropertiesSnapshot>,
    pub nv12_dpb_supported: bool,
    pub nv12_output_supported: bool,
    pub nv12_sampled_output_supported: bool,
    pub query_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoDecodeAv1ProfileSnapshot {
    pub profile: &'static str,
    pub std_profile: u32,
    pub film_grain_support: bool,
    pub chroma_subsampling: Vec<&'static str>,
    pub luma_bit_depth: Vec<&'static str>,
    pub chroma_bit_depth: Vec<&'static str>,
    pub supported: bool,
    pub max_level: Option<&'static str>,
    pub max_level_raw: Option<u32>,
    pub capability_flags: Vec<&'static str>,
    pub decode_capability_flags: Vec<&'static str>,
    pub min_bitstream_buffer_offset_alignment: Option<u64>,
    pub min_bitstream_buffer_size_alignment: Option<u64>,
    pub picture_access_granularity: Option<(u32, u32)>,
    pub min_coded_extent: Option<(u32, u32)>,
    pub max_coded_extent: Option<(u32, u32)>,
    pub max_dpb_slots: Option<u32>,
    pub max_active_reference_pictures: Option<u32>,
    pub dpb_formats: Vec<NativeVulkanVideoFormatPropertiesSnapshot>,
    pub output_formats: Vec<NativeVulkanVideoFormatPropertiesSnapshot>,
    pub sampled_output_formats: Vec<NativeVulkanVideoFormatPropertiesSnapshot>,
    pub nv12_dpb_supported: bool,
    pub nv12_output_supported: bool,
    pub nv12_sampled_output_supported: bool,
    pub query_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoFormatPropertiesSnapshot {
    pub format: &'static str,
    pub format_raw: i32,
    pub image_type: &'static str,
    pub image_tiling: &'static str,
    pub image_usage_flags: Vec<&'static str>,
    pub image_create_flags: Vec<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeVulkanVideoSessionCodec {
    #[serde(rename = "h265-main-8")]
    H265Main8,
    #[serde(rename = "av1-main-8")]
    Av1Main8,
}

impl NativeVulkanVideoSessionCodec {
    fn label(self) -> &'static str {
        match self {
            Self::H265Main8 => "h265-main-8",
            Self::Av1Main8 => "av1-main-8",
        }
    }

    fn profile_label(self) -> &'static str {
        match self {
            Self::H265Main8 | Self::Av1Main8 => "main-8",
        }
    }

    fn codec_extension_name(self) -> &'static CStr {
        match self {
            Self::H265Main8 => vk::KHR_VIDEO_DECODE_H265_NAME,
            Self::Av1Main8 => vk::KHR_VIDEO_DECODE_AV1_NAME,
        }
    }

    fn codec_operation(self) -> vk::VideoCodecOperationFlagsKHR {
        match self {
            Self::H265Main8 => vk::VideoCodecOperationFlagsKHR::DECODE_H265,
            Self::Av1Main8 => vk::VideoCodecOperationFlagsKHR::DECODE_AV1,
        }
    }
}

impl std::str::FromStr for NativeVulkanVideoSessionCodec {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "h265" | "hevc" | "h265-main-8" | "hevc-main-8" => Ok(Self::H265Main8),
            "av1" | "av1-main-8" => Ok(Self::Av1Main8),
            other => Err(format!("unsupported Vulkan Video session codec: {other}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeVulkanVideoSessionSmokeOptions {
    pub codec: NativeVulkanVideoSessionCodec,
    pub width: u32,
    pub height: u32,
    pub allocate_video_images: bool,
    pub allocate_bitstream_buffer: bool,
    pub bitstream_buffer_size: u64,
    pub extract_bitstream: bool,
    pub decode_first_frame: bool,
    pub sample_decoded_first_frame: bool,
    pub bitstream_source: Option<PathBuf>,
    pub bitstream_extract_max_samples: u32,
}

impl Default for NativeVulkanVideoSessionSmokeOptions {
    fn default() -> Self {
        Self {
            codec: NativeVulkanVideoSessionCodec::H265Main8,
            width: 3840,
            height: 2160,
            allocate_video_images: false,
            allocate_bitstream_buffer: false,
            bitstream_buffer_size: 8 * 1024 * 1024,
            extract_bitstream: false,
            decode_first_frame: false,
            sample_decoded_first_frame: false,
            bitstream_source: None,
            bitstream_extract_max_samples: 8,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoSessionSmokeSnapshot {
    pub result: &'static str,
    pub requested_codec: NativeVulkanVideoSessionCodec,
    pub requested_extent: (u32, u32),
    pub selected_physical_device_index: usize,
    pub selected_physical_device_name: String,
    pub selected_physical_device_type: &'static str,
    pub vendor_id: u32,
    pub device_id: u32,
    pub api_version: String,
    pub driver_version: u32,
    pub selected_queue_family_index: u32,
    pub selected_queue_count: u32,
    pub selected_queue_flags: Vec<&'static str>,
    pub selected_queue_video_codec_operations: Vec<String>,
    pub enabled_device_extensions: Vec<&'static str>,
    pub video_codec_operation: Vec<String>,
    pub profile: &'static str,
    pub picture_format: &'static str,
    pub reference_picture_format: &'static str,
    pub nv12_dpb_supported: bool,
    pub nv12_output_supported: bool,
    pub nv12_sampled_output_supported: bool,
    pub capability_flags: Vec<&'static str>,
    pub decode_capability_flags: Vec<&'static str>,
    pub min_bitstream_buffer_offset_alignment: u64,
    pub min_bitstream_buffer_size_alignment: u64,
    pub picture_access_granularity: (u32, u32),
    pub min_coded_extent: (u32, u32),
    pub max_coded_extent: (u32, u32),
    pub requested_extent_supported: bool,
    pub driver_max_dpb_slots: u32,
    pub driver_max_active_reference_pictures: u32,
    pub session_max_dpb_slots: u32,
    pub session_max_active_reference_pictures: u32,
    pub codec_max_level: Option<String>,
    pub std_header_version_name: String,
    pub std_header_version_spec_version: u32,
    pub memory_requirement_count: usize,
    pub total_bound_memory_bytes: u64,
    pub memory_requirements: Vec<NativeVulkanVideoSessionMemoryRequirementSnapshot>,
    pub video_images_requested: bool,
    pub video_image_count: usize,
    pub total_video_image_memory_bytes: u64,
    pub video_images: Vec<NativeVulkanVideoSessionResourceImageSnapshot>,
    pub bitstream_buffer_requested: bool,
    pub bitstream_buffer: Option<NativeVulkanVideoSessionBitstreamBufferSnapshot>,
    pub bitstream_extract_requested: bool,
    pub bitstream_extract: Option<NativeVulkanVideoBitstreamExtractSnapshot>,
    pub session_parameters_requested: bool,
    pub session_parameters_created: bool,
    pub session_parameters: Option<NativeVulkanVideoSessionParametersSnapshot>,
    pub first_frame_decode_requested: bool,
    pub first_frame_decode: Option<NativeVulkanVideoFirstFrameDecodeSnapshot>,
    pub session_created: bool,
    pub session_memory_bound: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoSessionMemoryRequirementSnapshot {
    pub memory_bind_index: u32,
    pub size: u64,
    pub alignment: u64,
    pub memory_type_bits: u32,
    pub selected_memory_type_index: u32,
    pub selected_memory_property_flags: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoSessionResourceImageSnapshot {
    pub role: &'static str,
    pub format: &'static str,
    pub image_type: &'static str,
    pub image_tiling: &'static str,
    pub image_usage_flags: Vec<&'static str>,
    pub image_create_flags: Vec<&'static str>,
    pub extent: (u32, u32, u32),
    pub array_layers: u32,
    pub image_view_type: &'static str,
    pub image_view_created: bool,
    pub memory_size: u64,
    pub memory_alignment: u64,
    pub memory_type_bits: u32,
    pub selected_memory_type_index: u32,
    pub selected_memory_property_flags: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoSessionBitstreamBufferSnapshot {
    pub requested_size: u64,
    pub size: u64,
    pub min_size_alignment: u64,
    pub usage_flags: Vec<&'static str>,
    pub memory_size: u64,
    pub memory_alignment: u64,
    pub memory_type_bits: u32,
    pub selected_memory_type_index: u32,
    pub selected_memory_property_flags: Vec<&'static str>,
    pub mapped_write_bytes: u64,
    pub mapped_write_source: &'static str,
    pub mapped_write_hash: Option<u64>,
    pub host_visible: bool,
    pub host_coherent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoSessionParametersSnapshot {
    pub codec: &'static str,
    pub source: &'static str,
    pub max_std_vps_count: u32,
    pub max_std_sps_count: u32,
    pub max_std_pps_count: u32,
    pub std_vps_count: u32,
    pub std_sps_count: u32,
    pub std_pps_count: u32,
    pub vps_id: u8,
    pub sps_id: u32,
    pub pps_id: u32,
    pub profile_idc: u8,
    pub level_idc: u8,
    pub width: u32,
    pub height: u32,
    pub created: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoFirstFrameDecodeSnapshot {
    pub codec: &'static str,
    pub command_pool_created: bool,
    pub command_buffer_allocated: bool,
    pub command_buffer_recorded: bool,
    pub submitted: bool,
    pub completed: bool,
    pub queue_family_index: u32,
    pub source_layout: &'static str,
    pub decode_layout: &'static str,
    pub src_buffer_offset: u64,
    pub src_buffer_range: u64,
    pub dst_base_array_layer: u32,
    pub setup_slot_index: i32,
    pub begin_reference_slot_count: u32,
    pub decode_reference_slot_count: u32,
    pub reset_control_recorded: bool,
    pub slice_segment_count: u32,
    pub slice_segment_offsets: Vec<u32>,
    pub nal_type: u8,
    pub nal_type_label: &'static str,
    pub first_slice_segment_in_pic_flag: bool,
    pub slice_type: u32,
    pub pps_id: u32,
    pub sps_video_parameter_set_id: u8,
    pub pps_seq_parameter_set_id: u8,
    pub pps_pic_parameter_set_id: u8,
    pub pic_order_cnt_val: i32,
    pub idr: bool,
    pub irap: bool,
    pub output_readback: Option<NativeVulkanVideoDecodeOutputReadbackSnapshot>,
    pub output_sampling: Option<NativeVulkanVideoDecodeOutputSamplingSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoDecodeOutputReadbackSnapshot {
    pub format: &'static str,
    pub buffer_created: bool,
    pub copied: bool,
    pub host_visible: bool,
    pub host_coherent: bool,
    pub host_cached: bool,
    pub memory_size: u64,
    pub total_bytes: u64,
    pub y_plane_bytes: u64,
    pub uv_plane_bytes: u64,
    pub y_plane_hash: u64,
    pub uv_plane_hash: u64,
    pub combined_hash: u64,
    pub y_plane_nonzero_bytes: u64,
    pub uv_plane_nonzero_bytes: u64,
    pub y_plane_min: u8,
    pub y_plane_max: u8,
    pub uv_plane_min: u8,
    pub uv_plane_max: u8,
    pub y_plane_unique_values: u32,
    pub uv_plane_unique_values: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoDecodeOutputSamplingSnapshot {
    pub source_format: &'static str,
    pub target_format: &'static str,
    pub source_layout: &'static str,
    pub shader_layout: &'static str,
    pub render_extent: (u32, u32),
    pub y_plane_view_created: bool,
    pub uv_plane_view_created: bool,
    pub color_image_created: bool,
    pub color_image_view_created: bool,
    pub renderer_created: bool,
    pub command_buffer_recorded: bool,
    pub rendered: bool,
    pub copied: bool,
    pub host_visible: bool,
    pub host_coherent: bool,
    pub host_cached: bool,
    pub color_image_memory_size: u64,
    pub readback_memory_size: u64,
    pub total_bytes: u64,
    pub rgba_hash: u64,
    pub rgba_nonzero_bytes: u64,
    pub rgba_min: u8,
    pub rgba_max: u8,
    pub rgba_unique_values: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoBitstreamExtractSnapshot {
    pub source: String,
    pub frontend: &'static str,
    pub requested_max_samples: u32,
    pub samples: u32,
    pub total_bytes: u64,
    pub selected_access_unit_bytes: u64,
    pub selected_access_unit_pts_ms: Option<u64>,
    pub selected_access_unit_duration_ms: Option<u64>,
    pub caps: Option<String>,
    pub stream_format: Option<String>,
    pub alignment: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub framerate: Option<String>,
    pub has_annex_b_start_codes: bool,
    pub h265_vps_count: u32,
    pub h265_sps_count: u32,
    pub h265_pps_count: u32,
    pub h265_idr_count: u32,
    pub h265_slice_count: u32,
    pub h265_parameter_sets_present: bool,
    pub h265_parameter_sets: Option<NativeVulkanH265ParameterSetSnapshot>,
    pub h265_nal_units: Vec<NativeVulkanH265NalUnitSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanH265NalUnitSnapshot {
    pub offset: u64,
    pub size: u64,
    pub nal_type: u8,
    pub nal_type_label: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanH265ParameterSetSnapshot {
    pub parser: &'static str,
    pub vps: NativeVulkanH265VpsSnapshot,
    pub sps: NativeVulkanH265SpsSnapshot,
    pub pps: NativeVulkanH265PpsSnapshot,
    pub requested_profile_compatible: bool,
    pub vulkan_std_session_parameters_ready: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanH265VpsSnapshot {
    pub id: u8,
    pub max_layers_minus1: u8,
    pub max_sub_layers_minus1: u8,
    pub temporal_id_nesting_flag: bool,
    pub sub_layer_ordering_info_present_flag: bool,
    pub profile_idc: u8,
    pub profile_label: &'static str,
    pub tier_flag: bool,
    pub progressive_source_flag: bool,
    pub interlaced_source_flag: bool,
    pub non_packed_constraint_flag: bool,
    pub frame_only_constraint_flag: bool,
    pub level_idc: u8,
    pub level_label: Option<&'static str>,
    pub dec_pic_buf_mgr: NativeVulkanH265DecPicBufMgrSnapshot,
    pub timing_info_present_flag: bool,
    pub poc_proportional_to_timing_flag: bool,
    pub num_units_in_tick: Option<u32>,
    pub time_scale: Option<u32>,
    pub num_ticks_poc_diff_one_minus1: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanH265DecPicBufMgrSnapshot {
    pub max_latency_increase_plus1: [u32; 7],
    pub max_dec_pic_buffering_minus1: [u8; 7],
    pub max_num_reorder_pics: [u8; 7],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanH265SpsSnapshot {
    pub id: u32,
    pub vps_id: u8,
    pub max_sub_layers_minus1: u8,
    pub temporal_id_nesting_flag: bool,
    pub sub_layer_ordering_info_present_flag: bool,
    pub profile_idc: u8,
    pub profile_label: &'static str,
    pub tier_flag: bool,
    pub progressive_source_flag: bool,
    pub interlaced_source_flag: bool,
    pub non_packed_constraint_flag: bool,
    pub frame_only_constraint_flag: bool,
    pub level_idc: u8,
    pub level_label: Option<&'static str>,
    pub dec_pic_buf_mgr: NativeVulkanH265DecPicBufMgrSnapshot,
    pub chroma_format_idc: u32,
    pub chroma_format_label: &'static str,
    pub separate_colour_plane_flag: bool,
    pub width: u32,
    pub height: u32,
    pub conformance_window_flag: bool,
    pub conf_win_left_offset: u32,
    pub conf_win_right_offset: u32,
    pub conf_win_top_offset: u32,
    pub conf_win_bottom_offset: u32,
    pub bit_depth_luma_minus8: u32,
    pub bit_depth_chroma_minus8: u32,
    pub log2_max_pic_order_cnt_lsb_minus4: u32,
    pub log2_min_luma_coding_block_size_minus3: u32,
    pub log2_diff_max_min_luma_coding_block_size: u32,
    pub log2_min_luma_transform_block_size_minus2: u32,
    pub log2_diff_max_min_luma_transform_block_size: u32,
    pub max_transform_hierarchy_depth_inter: u32,
    pub max_transform_hierarchy_depth_intra: u32,
    pub scaling_list_enabled_flag: bool,
    pub sps_scaling_list_data_present_flag: bool,
    pub amp_enabled_flag: bool,
    pub sample_adaptive_offset_enabled_flag: bool,
    pub pcm_enabled_flag: bool,
    pub pcm_loop_filter_disabled_flag: bool,
    pub num_short_term_ref_pic_sets: u32,
    pub long_term_ref_pics_present_flag: bool,
    pub temporal_mvp_enabled_flag: bool,
    pub strong_intra_smoothing_enabled_flag: bool,
    pub vui_parameters_present_flag: bool,
    pub vui: Option<NativeVulkanH265VuiSnapshot>,
    pub sps_extension_present_flag: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanH265VuiSnapshot {
    pub aspect_ratio_info_present_flag: bool,
    pub aspect_ratio_idc: u32,
    pub sar_width: u16,
    pub sar_height: u16,
    pub overscan_info_present_flag: bool,
    pub overscan_appropriate_flag: bool,
    pub video_signal_type_present_flag: bool,
    pub video_format: u8,
    pub video_full_range_flag: bool,
    pub colour_description_present_flag: bool,
    pub colour_primaries: u8,
    pub transfer_characteristics: u8,
    pub matrix_coeffs: u8,
    pub chroma_loc_info_present_flag: bool,
    pub chroma_sample_loc_type_top_field: u8,
    pub chroma_sample_loc_type_bottom_field: u8,
    pub neutral_chroma_indication_flag: bool,
    pub field_seq_flag: bool,
    pub frame_field_info_present_flag: bool,
    pub default_display_window_flag: bool,
    pub def_disp_win_left_offset: u16,
    pub def_disp_win_right_offset: u16,
    pub def_disp_win_top_offset: u16,
    pub def_disp_win_bottom_offset: u16,
    pub vui_timing_info_present_flag: bool,
    pub vui_num_units_in_tick: u32,
    pub vui_time_scale: u32,
    pub vui_poc_proportional_to_timing_flag: bool,
    pub vui_num_ticks_poc_diff_one_minus1: u32,
    pub vui_hrd_parameters_present_flag: bool,
    pub bitstream_restriction_flag: bool,
    pub tiles_fixed_structure_flag: bool,
    pub motion_vectors_over_pic_boundaries_flag: bool,
    pub restricted_ref_pic_lists_flag: bool,
    pub min_spatial_segmentation_idc: u16,
    pub max_bytes_per_pic_denom: u8,
    pub max_bits_per_min_cu_denom: u8,
    pub log2_max_mv_length_horizontal: u8,
    pub log2_max_mv_length_vertical: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanH265PpsSnapshot {
    pub id: u32,
    pub sps_id: u32,
    pub dependent_slice_segments_enabled_flag: bool,
    pub output_flag_present_flag: bool,
    pub num_extra_slice_header_bits: u8,
    pub sign_data_hiding_enabled_flag: bool,
    pub cabac_init_present_flag: bool,
    pub num_ref_idx_l0_default_active_minus1: u32,
    pub num_ref_idx_l1_default_active_minus1: u32,
    pub init_qp_minus26: i32,
    pub constrained_intra_pred_flag: bool,
    pub transform_skip_enabled_flag: bool,
    pub cu_qp_delta_enabled_flag: bool,
    pub diff_cu_qp_delta_depth: Option<u32>,
    pub cb_qp_offset: i32,
    pub cr_qp_offset: i32,
    pub slice_chroma_qp_offsets_present_flag: bool,
    pub weighted_pred_flag: bool,
    pub weighted_bipred_flag: bool,
    pub transquant_bypass_enabled_flag: bool,
    pub tiles_enabled_flag: bool,
    pub entropy_coding_sync_enabled_flag: bool,
    pub uniform_spacing_flag: bool,
    pub num_tile_columns_minus1: u32,
    pub num_tile_rows_minus1: u32,
    pub loop_filter_across_tiles_enabled_flag: Option<bool>,
    pub loop_filter_across_slices_enabled_flag: bool,
    pub deblocking_filter_control_present_flag: bool,
    pub deblocking_filter_override_enabled_flag: Option<bool>,
    pub pps_deblocking_filter_disabled_flag: Option<bool>,
    pub pps_beta_offset_div2: i32,
    pub pps_tc_offset_div2: i32,
    pub pps_scaling_list_data_present_flag: bool,
    pub lists_modification_present_flag: bool,
    pub log2_parallel_merge_level_minus2: u32,
    pub slice_segment_header_extension_present_flag: bool,
    pub pps_extension_present_flag: bool,
}

struct NativeVulkanVideoDecodeFormatProbe {
    dpb_formats: Vec<NativeVulkanVideoFormatPropertiesSnapshot>,
    output_formats: Vec<NativeVulkanVideoFormatPropertiesSnapshot>,
    sampled_output_formats: Vec<NativeVulkanVideoFormatPropertiesSnapshot>,
    nv12_dpb_supported: bool,
    nv12_output_supported: bool,
    nv12_sampled_output_supported: bool,
    query_error: Option<String>,
}

pub type NativeVulkanVideoDecodeProbeResult =
    Result<NativeVulkanVideoDecodeProbeSnapshot, NativeVulkanError>;

pub struct NativeVulkanSurfaceProbe {
    host: NativeWaylandHost,
    _entry: ash::Entry,
    instance: ash::Instance,
    surface_loader: ash::khr::surface::Instance,
    _wayland_surface_loader: ash::khr::wayland_surface::Instance,
    surface: vk::SurfaceKHR,
    snapshot: NativeVulkanSurfaceProbeSnapshot,
}

impl NativeVulkanSurfaceProbe {
    pub fn connect(options: NativeVulkanSurfaceProbeOptions) -> Result<Self, NativeVulkanError> {
        let mut host = NativeWaylandHost::connect(options.host)?;
        host.wait_until_configured(options.wait_configure_roundtrips)?;
        let handles = host.surface_handles()?;

        let (entry, instance) = create_native_vulkan_instance()?;
        let surface_loader = ash::khr::surface::Instance::new(&entry, &instance);
        let wayland_surface_loader = ash::khr::wayland_surface::Instance::new(&entry, &instance);
        let surface_create_info = vk::WaylandSurfaceCreateInfoKHR::default()
            .display(handles.display.as_ptr().cast::<vk::wl_display>())
            .surface(handles.surface.as_ptr().cast::<vk::wl_surface>());
        let surface = match unsafe {
            wayland_surface_loader.create_wayland_surface(&surface_create_info, None)
        } {
            Ok(surface) => surface,
            Err(result) => {
                unsafe {
                    instance.destroy_instance(None);
                }
                return Err(NativeVulkanError::Vulkan {
                    operation: "vkCreateWaylandSurfaceKHR",
                    result,
                });
            }
        };

        let mut probe = Self {
            host,
            _entry: entry,
            instance,
            surface_loader,
            _wayland_surface_loader: wayland_surface_loader,
            surface,
            snapshot: NativeVulkanSurfaceProbeSnapshot::initial(handles),
        };
        probe.snapshot = probe.query_surface_snapshot(handles)?;
        Ok(probe)
    }

    pub fn pump_events(&mut self) -> Result<(), NativeVulkanError> {
        self.host.pump_events().map_err(Into::into)
    }

    pub fn snapshot(&self) -> NativeVulkanSurfaceProbeSnapshot {
        self.snapshot.clone()
    }

    fn query_surface_snapshot(
        &self,
        handles: NativeWaylandSurfaceHandles,
    ) -> Result<NativeVulkanSurfaceProbeSnapshot, NativeVulkanError> {
        let physical_devices =
            unsafe { self.instance.enumerate_physical_devices() }.map_err(|result| {
                NativeVulkanError::Vulkan {
                    operation: "vkEnumeratePhysicalDevices",
                    result,
                }
            })?;
        let mut present_queue_family_count = 0usize;
        let mut selected = None;

        for (physical_device_index, physical_device) in physical_devices.iter().copied().enumerate()
        {
            let properties = unsafe {
                self.instance
                    .get_physical_device_properties(physical_device)
            };
            let queue_families = unsafe {
                self.instance
                    .get_physical_device_queue_family_properties(physical_device)
            };
            for (queue_family_index, queue_family) in queue_families.iter().enumerate() {
                let supports_surface = unsafe {
                    self.surface_loader.get_physical_device_surface_support(
                        physical_device,
                        queue_family_index as u32,
                        self.surface,
                    )
                }
                .map_err(|result| NativeVulkanError::Vulkan {
                    operation: "vkGetPhysicalDeviceSurfaceSupportKHR",
                    result,
                })?;
                if !supports_surface {
                    continue;
                }
                present_queue_family_count += 1;

                let supports_graphics = queue_family.queue_flags.contains(vk::QueueFlags::GRAPHICS);
                if selected.is_none() && supports_graphics {
                    selected = Some(NativeVulkanPresentQueueSelection {
                        physical_device,
                        physical_device_index,
                        physical_device_name: native_vulkan_physical_device_name(properties),
                        physical_device_type: native_vulkan_physical_device_type_label(
                            properties.device_type,
                        ),
                        queue_family_index: queue_family_index as u32,
                    });
                }
            }
        }

        let Some(selected) = selected else {
            return Err(NativeVulkanError::MissingPresentQueue);
        };
        let surface_capabilities = unsafe {
            self.surface_loader
                .get_physical_device_surface_capabilities(selected.physical_device, self.surface)
        }
        .map_err(|result| NativeVulkanError::Vulkan {
            operation: "vkGetPhysicalDeviceSurfaceCapabilitiesKHR",
            result,
        })?;

        Ok(NativeVulkanSurfaceProbeSnapshot {
            wayland_surface_logical_size: handles.logical_size,
            wayland_surface_buffer_size: handles.buffer_size,
            dmabuf_main_device: handles.dmabuf_main_device,
            physical_device_count: physical_devices.len(),
            present_queue_family_count,
            selected_physical_device_index: Some(selected.physical_device_index),
            selected_physical_device_name: Some(selected.physical_device_name),
            selected_physical_device_type: Some(selected.physical_device_type),
            selected_queue_family_index: Some(selected.queue_family_index),
            selected_queue_supports_graphics: true,
            surface_capabilities: Some(surface_capabilities.into()),
        })
    }
}

impl Drop for NativeVulkanSurfaceProbe {
    fn drop(&mut self) {
        unsafe {
            self.surface_loader.destroy_surface(self.surface, None);
            self.instance.destroy_instance(None);
        }
    }
}

impl NativeVulkanSurfaceProbeSnapshot {
    fn initial(handles: NativeWaylandSurfaceHandles) -> Self {
        Self {
            wayland_surface_logical_size: handles.logical_size,
            wayland_surface_buffer_size: handles.buffer_size,
            dmabuf_main_device: handles.dmabuf_main_device,
            physical_device_count: 0,
            present_queue_family_count: 0,
            selected_physical_device_index: None,
            selected_physical_device_name: None,
            selected_physical_device_type: None,
            selected_queue_family_index: None,
            selected_queue_supports_graphics: false,
            surface_capabilities: None,
        }
    }
}

impl From<vk::SurfaceCapabilitiesKHR> for NativeVulkanSurfaceCapabilitiesSnapshot {
    fn from(capabilities: vk::SurfaceCapabilitiesKHR) -> Self {
        Self {
            min_image_count: capabilities.min_image_count,
            max_image_count: capabilities.max_image_count,
            current_extent: native_vulkan_extent(capabilities.current_extent),
            min_image_extent: (
                capabilities.min_image_extent.width,
                capabilities.min_image_extent.height,
            ),
            max_image_extent: (
                capabilities.max_image_extent.width,
                capabilities.max_image_extent.height,
            ),
        }
    }
}

struct NativeVulkanPresentQueueSelection {
    physical_device: vk::PhysicalDevice,
    physical_device_index: usize,
    physical_device_name: String,
    physical_device_type: &'static str,
    queue_family_index: u32,
}

pub fn probe_wayland_surface(
    options: NativeVulkanSurfaceProbeOptions,
) -> Result<NativeVulkanSurfaceProbeSnapshot, NativeVulkanError> {
    let mut probe = NativeVulkanSurfaceProbe::connect(options)?;
    probe.pump_events()?;
    Ok(probe.snapshot())
}

pub fn probe_vulkan_video_decode() -> NativeVulkanVideoDecodeProbeResult {
    let (entry, instance) = create_native_vulkan_instance()?;
    let result = native_vulkan_video_decode_probe_inner(&entry, &instance);
    unsafe {
        instance.destroy_instance(None);
    }
    result
}

pub fn probe_vulkan_video_session(
    options: NativeVulkanVideoSessionSmokeOptions,
) -> Result<NativeVulkanVideoSessionSmokeSnapshot, NativeVulkanError> {
    if options.width == 0 || options.height == 0 {
        return Err(NativeVulkanError::Video(
            "Vulkan Video session extent must be non-zero".to_owned(),
        ));
    }

    let (entry, instance) = create_native_vulkan_instance()?;
    let result = native_vulkan_video_session_smoke_inner(&entry, &instance, options);
    unsafe {
        instance.destroy_instance(None);
    }
    result
}

#[derive(Debug, Clone, PartialEq)]
pub struct NativeVulkanOptions {
    pub host: NativeWaylandHostOptions,
    pub wait_configure_roundtrips: usize,
    pub clear_color: NativeVulkanClearColor,
    pub target_max_fps: Option<u32>,
}

impl Default for NativeVulkanOptions {
    fn default() -> Self {
        let mut host = NativeWaylandHostOptions::default();
        host.namespace = "gilder-native-vulkan".to_owned();
        Self {
            host,
            wait_configure_roundtrips: 8,
            clear_color: NativeVulkanClearColor::default(),
            target_max_fps: Some(240),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct NativeVulkanClearColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Default for NativeVulkanClearColor {
    fn default() -> Self {
        Self {
            r: 0.02,
            g: 0.04,
            b: 0.07,
            a: 1.0,
        }
    }
}

impl From<NativeVulkanClearColor> for vk::ClearColorValue {
    fn from(color: NativeVulkanClearColor) -> Self {
        Self {
            float32: [color.r, color.g, color.b, color.a],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanRuntimeSnapshot {
    pub runtime_elapsed_ms: u64,
    pub frames_rendered: u64,
    pub average_render_fps: f64,
    pub configured: bool,
    pub wayland_surface_logical_size: (u32, u32),
    pub wayland_surface_buffer_size: (u32, u32),
    pub selected_physical_device_name: String,
    pub selected_physical_device_type: &'static str,
    pub selected_queue_family_index: u32,
    pub swapchain_extent: (u32, u32),
    pub swapchain_image_count: usize,
    pub swapchain_format: String,
    pub present_mode: &'static str,
    pub clear_color: NativeVulkanClearColor,
    pub static_upload_bytes: Option<u64>,
    pub video_runtime: Option<NativeVulkanVideoRuntimeSnapshot>,
    pub render_item: NativeVulkanRenderItem,
    pub last_render_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoRuntimeSnapshot {
    pub source: PathBuf,
    pub poster: Option<PathBuf>,
    pub fit: FitMode,
    pub loop_playback: bool,
    pub muted: bool,
    pub manifest_max_fps: Option<u32>,
    pub target_max_fps: Option<u32>,
    pub decoder_policy: VideoDecoderPolicy,
    pub start_offset_ms: u64,
    pub frontend: &'static str,
    pub frontend_status: &'static str,
    pub handoff_status: &'static str,
    pub texture_import_status: &'static str,
    pub audio_status: &'static str,
    pub gst_state: Option<String>,
    pub eos_messages: u64,
    pub segment_done_messages: u64,
    pub frames_received: u64,
    pub frames_imported: u64,
    pub rendered_placeholder_frames: u64,
    pub poster_upload_bytes: Option<u64>,
    pub last_import_size: Option<(u32, u32)>,
    pub last_import_memory_path: Option<String>,
    pub last_import_error: Option<String>,
    pub last_import_elapsed_us: Option<u64>,
    pub max_import_elapsed_us: Option<u64>,
    pub last_sample_caps: Option<String>,
    pub last_sample_format: Option<String>,
    pub last_sample_size: Option<(u32, u32)>,
    pub last_sample_pts_ms: Option<u64>,
    pub last_sample_duration_ms: Option<u64>,
    pub last_sample_pts_delta_ms: Option<u64>,
    pub last_sample_memory_types: Vec<String>,
    pub actual_decoders: Vec<String>,
    pub decoder_policy_status: Option<String>,
    pub caps_report_count: usize,
    pub caps_memory_features: Vec<String>,
    pub caps_reports: Vec<NativeVulkanVideoCapsSnapshot>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoCapsSnapshot {
    pub element: String,
    pub pad: String,
    pub direction: String,
    pub caps: String,
    pub source: String,
    pub memory_features: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeVulkanGstVideoFrontendSnapshot {
    gst_state: Option<String>,
    eos_messages: u64,
    segment_done_messages: u64,
    frames_received: u64,
    last_sample_caps: Option<String>,
    last_sample_format: Option<String>,
    last_sample_size: Option<(u32, u32)>,
    last_sample_pts_ms: Option<u64>,
    last_sample_duration_ms: Option<u64>,
    last_sample_pts_delta_ms: Option<u64>,
    last_sample_memory_types: Vec<String>,
    actual_decoders: Vec<String>,
    decoder_policy_status: Option<String>,
    caps_report_count: usize,
    caps_memory_features: Vec<String>,
    caps_reports: Vec<NativeVulkanVideoCapsSnapshot>,
    last_error: Option<String>,
}

#[cfg(feature = "native-vulkan-gst-video")]
#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeVulkanVideoImportSnapshot {
    texture_import_status: &'static str,
    frames_imported: u64,
    last_import_size: Option<(u32, u32)>,
    last_import_memory_path: Option<String>,
    last_import_error: Option<String>,
    last_import_elapsed_us: Option<u64>,
    max_import_elapsed_us: Option<u64>,
}

#[cfg(not(feature = "native-vulkan-gst-video"))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeVulkanVideoImportSnapshot {
    texture_import_status: &'static str,
    frames_imported: u64,
    last_import_size: Option<(u32, u32)>,
    last_import_memory_path: Option<String>,
    last_import_error: Option<String>,
    last_import_elapsed_us: Option<u64>,
    max_import_elapsed_us: Option<u64>,
}

pub struct NativeVulkanSession {
    host: NativeWaylandHost,
    _entry: ash::Entry,
    instance: ash::Instance,
    surface_loader: ash::khr::surface::Instance,
    _wayland_surface_loader: ash::khr::wayland_surface::Instance,
    surface: vk::SurfaceKHR,
    physical_device: vk::PhysicalDevice,
    selected_physical_device_name: String,
    selected_physical_device_type: &'static str,
    queue_family_index: u32,
    device: ash::Device,
    queue: vk::Queue,
    swapchain_loader: ash::khr::swapchain::Device,
    swapchain: vk::SwapchainKHR,
    swapchain_format: vk::Format,
    present_mode: vk::PresentModeKHR,
    swapchain_extent: vk::Extent2D,
    swapchain_images: Vec<vk::Image>,
    swapchain_image_views: Vec<vk::ImageView>,
    swapchain_image_layouts: Vec<vk::ImageLayout>,
    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,
    image_available: vk::Semaphore,
    render_finished: vk::Semaphore,
    in_flight: vk::Fence,
    static_upload: Option<NativeVulkanStaticImageUpload>,
    #[cfg(feature = "native-vulkan-gst-video")]
    video_frontend: Option<NativeVulkanGstVideoFrontend>,
    #[cfg(feature = "native-vulkan-gst-video")]
    video_renderer: Option<NativeVulkanVideoRenderer>,
    #[cfg(feature = "native-vulkan-gst-video")]
    video_texture: Option<NativeVulkanVideoTexture>,
    #[cfg(feature = "native-vulkan-gst-video")]
    video_import_status: NativeVulkanVideoImportStatus,
    clear_color: NativeVulkanClearColor,
    render_item: NativeVulkanRenderItem,
    started_at: Instant,
    frames_rendered: u64,
    last_render_error: Option<String>,
}

impl NativeVulkanSession {
    pub fn connect(options: NativeVulkanOptions) -> Result<Self, NativeVulkanError> {
        Self::connect_with_render_item(
            options,
            NativeVulkanRenderItem::Clear {
                output_name: "native-vulkan".to_owned(),
            },
        )
    }

    pub fn connect_with_render_item(
        options: NativeVulkanOptions,
        render_item: NativeVulkanRenderItem,
    ) -> Result<Self, NativeVulkanError> {
        let mut host = NativeWaylandHost::connect(options.host)?;
        host.wait_until_configured(options.wait_configure_roundtrips)?;
        let handles = host.surface_handles()?;

        let (entry, instance) = create_native_vulkan_instance()?;
        let surface_loader = ash::khr::surface::Instance::new(&entry, &instance);
        let wayland_surface_loader = ash::khr::wayland_surface::Instance::new(&entry, &instance);
        let surface_create_info = vk::WaylandSurfaceCreateInfoKHR::default()
            .display(handles.display.as_ptr().cast::<vk::wl_display>())
            .surface(handles.surface.as_ptr().cast::<vk::wl_surface>());
        let surface = match unsafe {
            wayland_surface_loader.create_wayland_surface(&surface_create_info, None)
        } {
            Ok(surface) => surface,
            Err(result) => {
                unsafe {
                    instance.destroy_instance(None);
                }
                return Err(NativeVulkanError::Vulkan {
                    operation: "vkCreateWaylandSurfaceKHR",
                    result,
                });
            }
        };

        let selection =
            select_native_vulkan_present_queue(&instance, &surface_loader, surface)?.selection;
        ensure_native_vulkan_device_extension(
            &instance,
            selection.physical_device,
            ash::khr::swapchain::NAME,
        )?;
        #[cfg(feature = "native-vulkan-gst-video")]
        let video_enabled = matches!(&render_item, NativeVulkanRenderItem::Video { .. });
        #[cfg(feature = "native-vulkan-gst-video")]
        if video_enabled {
            ensure_native_vulkan_device_extension(
                &instance,
                selection.physical_device,
                ash::khr::external_memory_fd::NAME,
            )?;
        }
        let priorities = [1.0_f32];
        let queue_create_info = vk::DeviceQueueCreateInfo::default()
            .queue_family_index(selection.queue_family_index)
            .queue_priorities(&priorities);
        let queue_create_infos = [queue_create_info];
        let mut device_extensions = vec![ash::khr::swapchain::NAME.as_ptr()];
        #[cfg(feature = "native-vulkan-gst-video")]
        if video_enabled {
            device_extensions.push(ash::khr::external_memory_fd::NAME.as_ptr());
        }
        let device_create_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_create_infos)
            .enabled_extension_names(&device_extensions);
        let device =
            unsafe { instance.create_device(selection.physical_device, &device_create_info, None) }
                .map_err(|result| NativeVulkanError::Vulkan {
                    operation: "vkCreateDevice",
                    result,
                })?;
        let queue = unsafe { device.get_device_queue(selection.queue_family_index, 0) };
        let swapchain_loader = ash::khr::swapchain::Device::new(&instance, &device);
        let swapchain_plan = create_native_vulkan_swapchain_plan(
            &surface_loader,
            selection.physical_device,
            surface,
            handles.logical_size,
            handles.buffer_size,
        )?;
        let swapchain =
            unsafe { swapchain_loader.create_swapchain(&swapchain_plan.create_info, None) }
                .map_err(|result| NativeVulkanError::Vulkan {
                    operation: "vkCreateSwapchainKHR",
                    result,
                })?;
        let swapchain_images = unsafe { swapchain_loader.get_swapchain_images(swapchain) }
            .map_err(|result| NativeVulkanError::Vulkan {
                operation: "vkGetSwapchainImagesKHR",
                result,
            })?;
        let swapchain_image_views = create_native_vulkan_swapchain_image_views(
            &device,
            &swapchain_images,
            swapchain_plan.format.format,
        )?;
        let command_pool_create_info = vk::CommandPoolCreateInfo::default()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(selection.queue_family_index);
        let command_pool = unsafe { device.create_command_pool(&command_pool_create_info, None) }
            .map_err(|result| NativeVulkanError::Vulkan {
            operation: "vkCreateCommandPool",
            result,
        })?;
        let command_buffer_allocate_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(swapchain_images.len() as u32);
        let command_buffers =
            unsafe { device.allocate_command_buffers(&command_buffer_allocate_info) }.map_err(
                |result| NativeVulkanError::Vulkan {
                    operation: "vkAllocateCommandBuffers",
                    result,
                },
            )?;
        let semaphore_create_info = vk::SemaphoreCreateInfo::default();
        let image_available = unsafe { device.create_semaphore(&semaphore_create_info, None) }
            .map_err(|result| NativeVulkanError::Vulkan {
                operation: "vkCreateSemaphore(image_available)",
                result,
            })?;
        let render_finished = unsafe { device.create_semaphore(&semaphore_create_info, None) }
            .map_err(|result| NativeVulkanError::Vulkan {
                operation: "vkCreateSemaphore(render_finished)",
                result,
            })?;
        let fence_create_info =
            vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
        let in_flight =
            unsafe { device.create_fence(&fence_create_info, None) }.map_err(|result| {
                NativeVulkanError::Vulkan {
                    operation: "vkCreateFence",
                    result,
                }
            })?;
        let static_upload = match &render_item {
            NativeVulkanRenderItem::StaticImage {
                source,
                fit,
                background,
                ..
            } => Some(NativeVulkanStaticImageUpload::new(
                &instance,
                selection.physical_device,
                &device,
                source,
                *fit,
                background.as_deref(),
                swapchain_plan.format.format,
                swapchain_plan.extent,
            )?),
            NativeVulkanRenderItem::Video {
                poster: Some(poster),
                fit,
                ..
            } => Some(NativeVulkanStaticImageUpload::new(
                &instance,
                selection.physical_device,
                &device,
                poster,
                *fit,
                None,
                swapchain_plan.format.format,
                swapchain_plan.extent,
            )?),
            _ => None,
        };
        #[cfg(feature = "native-vulkan-gst-video")]
        let video_frontend = match &render_item {
            NativeVulkanRenderItem::Video { .. } => {
                Some(NativeVulkanGstVideoFrontend::new(&render_item)?)
            }
            _ => None,
        };
        #[cfg(feature = "native-vulkan-gst-video")]
        let video_renderer = match &render_item {
            NativeVulkanRenderItem::Video { .. } => Some(NativeVulkanVideoRenderer::new(
                &device,
                swapchain_plan.format.format,
                swapchain_plan.extent,
                &swapchain_image_views,
            )?),
            _ => None,
        };

        Ok(Self {
            host,
            _entry: entry,
            instance,
            surface_loader,
            _wayland_surface_loader: wayland_surface_loader,
            surface,
            physical_device: selection.physical_device,
            selected_physical_device_name: selection.physical_device_name,
            selected_physical_device_type: selection.physical_device_type,
            queue_family_index: selection.queue_family_index,
            device,
            queue,
            swapchain_loader,
            swapchain,
            swapchain_format: swapchain_plan.format.format,
            present_mode: swapchain_plan.present_mode,
            swapchain_extent: swapchain_plan.extent,
            swapchain_image_layouts: vec![vk::ImageLayout::UNDEFINED; swapchain_images.len()],
            swapchain_image_views,
            swapchain_images,
            command_pool,
            command_buffers,
            image_available,
            render_finished,
            in_flight,
            static_upload,
            #[cfg(feature = "native-vulkan-gst-video")]
            video_frontend,
            #[cfg(feature = "native-vulkan-gst-video")]
            video_renderer,
            #[cfg(feature = "native-vulkan-gst-video")]
            video_texture: None,
            #[cfg(feature = "native-vulkan-gst-video")]
            video_import_status: NativeVulkanVideoImportStatus::default(),
            clear_color: options.clear_color,
            render_item,
            started_at: Instant::now(),
            frames_rendered: 0,
            last_render_error: None,
        })
    }

    pub fn run_for(
        &mut self,
        duration: Duration,
        target_max_fps: Option<u32>,
    ) -> Result<NativeVulkanRuntimeSnapshot, NativeVulkanError> {
        let deadline = Instant::now() + duration;
        let frame_interval = target_max_fps
            .filter(|fps| *fps > 0)
            .map(|fps| Duration::from_secs_f64(1.0 / fps as f64));
        let mut next_frame = Instant::now();

        while Instant::now() < deadline && !self.host.is_closed() {
            self.host.pump_events()?;
            self.wait_for_in_flight()?;
            self.poll_video_frontend()?;
            match self.render_frame() {
                Ok(()) => {}
                Err(err) => {
                    self.last_render_error = Some(err.to_string());
                    return Err(err);
                }
            }
            self.trim_allocator_after_frame();

            if let Some(interval) = frame_interval {
                next_frame += interval;
                let now = Instant::now();
                if next_frame > now {
                    thread::sleep(next_frame - now);
                } else {
                    next_frame = now;
                }
            }
        }

        Ok(self.snapshot())
    }

    pub fn snapshot(&self) -> NativeVulkanRuntimeSnapshot {
        let elapsed = self.started_at.elapsed();
        NativeVulkanRuntimeSnapshot {
            runtime_elapsed_ms: elapsed.as_millis().min(u64::MAX as u128) as u64,
            frames_rendered: self.frames_rendered,
            average_render_fps: if elapsed.is_zero() {
                0.0
            } else {
                self.frames_rendered as f64 / elapsed.as_secs_f64()
            },
            configured: self.host.snapshot().configured,
            wayland_surface_logical_size: self
                .host
                .logical_size()
                .unwrap_or((self.swapchain_extent.width, self.swapchain_extent.height)),
            wayland_surface_buffer_size: (
                self.swapchain_extent.width,
                self.swapchain_extent.height,
            ),
            selected_physical_device_name: self.selected_physical_device_name.clone(),
            selected_physical_device_type: self.selected_physical_device_type,
            selected_queue_family_index: self.queue_family_index,
            swapchain_extent: (self.swapchain_extent.width, self.swapchain_extent.height),
            swapchain_image_count: self.swapchain_images.len(),
            swapchain_format: format!("{:?}", self.swapchain_format),
            present_mode: native_vulkan_present_mode_label(self.present_mode),
            clear_color: self.clear_color,
            static_upload_bytes: self
                .static_upload
                .as_ref()
                .map(|upload| upload.size_bytes.min(u64::MAX as vk::DeviceSize) as u64),
            video_runtime: native_vulkan_video_runtime_snapshot(
                &self.render_item,
                self.video_frontend_snapshot(),
                self.video_import_snapshot(),
                self.frames_rendered,
                self.static_upload
                    .as_ref()
                    .map(|upload| upload.size_bytes.min(u64::MAX as vk::DeviceSize) as u64),
            ),
            render_item: self.render_item.clone(),
            last_render_error: self.last_render_error.clone(),
        }
    }

    fn render_frame(&mut self) -> Result<(), NativeVulkanError> {
        self.wait_for_in_flight()?;
        let fences = [self.in_flight];
        unsafe {
            self.device
                .reset_fences(&fences)
                .map_err(|result| NativeVulkanError::Vulkan {
                    operation: "vkResetFences",
                    result,
                })?;
        }

        let (image_index, _) = unsafe {
            self.swapchain_loader.acquire_next_image(
                self.swapchain,
                u64::MAX,
                self.image_available,
                vk::Fence::null(),
            )
        }
        .map_err(|result| NativeVulkanError::Vulkan {
            operation: "vkAcquireNextImageKHR",
            result,
        })?;
        let image_index = image_index as usize;
        let command_buffer = self.command_buffers[image_index];
        self.record_frame_command(command_buffer, image_index)?;

        let wait_semaphores = [self.image_available];
        let wait_stages = [self.current_render_wait_stage()];
        let command_buffers = [command_buffer];
        let signal_semaphores = [self.render_finished];
        let submit_info = vk::SubmitInfo::default()
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&wait_stages)
            .command_buffers(&command_buffers)
            .signal_semaphores(&signal_semaphores);
        let submit_infos = [submit_info];
        unsafe {
            self.device
                .queue_submit(self.queue, &submit_infos, self.in_flight)
                .map_err(|result| NativeVulkanError::Vulkan {
                    operation: "vkQueueSubmit",
                    result,
                })?;
        }

        let swapchains = [self.swapchain];
        let image_indices = [image_index as u32];
        let present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(&signal_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);
        unsafe {
            self.swapchain_loader
                .queue_present(self.queue, &present_info)
                .map_err(|result| NativeVulkanError::Vulkan {
                    operation: "vkQueuePresentKHR",
                    result,
                })?;
        }
        self.frames_rendered += 1;
        Ok(())
    }

    fn wait_for_in_flight(&self) -> Result<(), NativeVulkanError> {
        let fences = [self.in_flight];
        unsafe {
            self.device
                .wait_for_fences(&fences, true, u64::MAX)
                .map_err(|result| NativeVulkanError::Vulkan {
                    operation: "vkWaitForFences",
                    result,
                })
        }
    }

    fn trim_allocator_after_frame(&self) {
        #[cfg(feature = "native-vulkan-gst-video")]
        if matches!(self.render_item, NativeVulkanRenderItem::Video { .. })
            && self.frames_rendered > 0
            && self.frames_rendered % 240 == 0
        {
            native_vulkan_trim_process_heap();
        }
    }

    fn record_frame_command(
        &mut self,
        command_buffer: vk::CommandBuffer,
        image_index: usize,
    ) -> Result<(), NativeVulkanError> {
        #[cfg(feature = "native-vulkan-gst-video")]
        if self.video_texture.is_some() && self.video_renderer.is_some() {
            return self.record_video_frame_command(command_buffer, image_index);
        }
        unsafe {
            self.device
                .reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty())
                .map_err(|result| NativeVulkanError::Vulkan {
                    operation: "vkResetCommandBuffer",
                    result,
                })?;
            let begin_info = vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            self.device
                .begin_command_buffer(command_buffer, &begin_info)
                .map_err(|result| NativeVulkanError::Vulkan {
                    operation: "vkBeginCommandBuffer",
                    result,
                })?;

            let image = self.swapchain_images[image_index];
            let old_layout = self.swapchain_image_layouts[image_index];
            let range = native_vulkan_color_subresource_range();
            let to_transfer = vk::ImageMemoryBarrier::default()
                .old_layout(old_layout)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(range)
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);
            self.device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[to_transfer],
            );

            if let Some(static_upload) = &self.static_upload {
                let copy = static_upload.buffer_image_copy;
                self.device.cmd_copy_buffer_to_image(
                    command_buffer,
                    static_upload.buffer,
                    image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[copy],
                );
            } else {
                let clear_color = vk::ClearColorValue::from(self.clear_color);
                self.device.cmd_clear_color_image(
                    command_buffer,
                    image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &clear_color,
                    &[range],
                );
            }

            let to_present = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(range)
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::empty());
            self.device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[to_present],
            );

            self.device
                .end_command_buffer(command_buffer)
                .map_err(|result| NativeVulkanError::Vulkan {
                    operation: "vkEndCommandBuffer",
                    result,
                })?;
            self.swapchain_image_layouts[image_index] = vk::ImageLayout::PRESENT_SRC_KHR;
        }
        Ok(())
    }

    fn current_render_wait_stage(&self) -> vk::PipelineStageFlags {
        #[cfg(feature = "native-vulkan-gst-video")]
        if self.video_texture.is_some() && self.video_renderer.is_some() {
            return vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT;
        }
        vk::PipelineStageFlags::TRANSFER
    }

    #[cfg(feature = "native-vulkan-gst-video")]
    fn record_video_frame_command(
        &mut self,
        command_buffer: vk::CommandBuffer,
        image_index: usize,
    ) -> Result<(), NativeVulkanError> {
        let texture = self
            .video_texture
            .as_ref()
            .ok_or_else(|| NativeVulkanError::Video("video texture is not ready".to_owned()))?;
        let renderer = self
            .video_renderer
            .as_ref()
            .ok_or_else(|| NativeVulkanError::Video("video renderer is not ready".to_owned()))?;
        let fit = match &self.render_item {
            NativeVulkanRenderItem::Video { fit, .. } => *fit,
            _ => FitMode::Cover,
        };
        renderer.record_frame(
            &self.device,
            command_buffer,
            image_index,
            self.swapchain_images[image_index],
            self.swapchain_image_layouts[image_index],
            texture,
            fit,
        )?;
        self.swapchain_image_layouts[image_index] = renderer.target_final_layout();
        Ok(())
    }

    fn poll_video_frontend(&mut self) -> Result<(), NativeVulkanError> {
        #[cfg(feature = "native-vulkan-gst-video")]
        if let Some(frontend) = self.video_frontend.as_mut() {
            frontend.poll()?;
            if let Some(sample) = frontend.take_latest_sample() {
                self.import_video_sample(sample);
            }
        }
        Ok(())
    }

    #[cfg(feature = "native-vulkan-gst-video")]
    fn video_frontend_snapshot(&self) -> Option<NativeVulkanGstVideoFrontendSnapshot> {
        self.video_frontend
            .as_ref()
            .map(NativeVulkanGstVideoFrontend::snapshot)
    }

    #[cfg(feature = "native-vulkan-gst-video")]
    fn video_import_snapshot(&self) -> Option<NativeVulkanVideoImportSnapshot> {
        matches!(self.render_item, NativeVulkanRenderItem::Video { .. })
            .then(|| self.video_import_status.snapshot())
    }

    #[cfg(feature = "native-vulkan-gst-video")]
    fn import_video_sample(&mut self, sample: gst::Sample) {
        let started_at = Instant::now();
        let import_result = self.import_video_sample_inner(&sample);
        match import_result {
            Ok(mut report) => {
                report.elapsed_us = native_vulkan_elapsed_us(started_at.elapsed());
                self.video_import_status.record_import(report);
            }
            Err(err) => self.video_import_status.record_error(err.to_string()),
        }
    }

    #[cfg(feature = "native-vulkan-gst-video")]
    fn import_video_sample_inner(
        &mut self,
        sample: &gst::Sample,
    ) -> Result<NativeVulkanVideoImportReport, NativeVulkanError> {
        self.video_renderer.as_ref().ok_or_else(|| {
            NativeVulkanError::Video("native Vulkan video renderer is not initialized".to_owned())
        })?;
        let buffer = sample
            .buffer()
            .ok_or_else(|| NativeVulkanError::Video("appsink sample has no buffer".to_owned()))?;
        let meta = native_vulkan_gst_system_nv12_meta(sample, buffer)?;
        if native_vulkan_gst_buffer_has_dmabuf_memory(buffer) {
            let frame = native_vulkan_gst_dmabuf_frame(sample, buffer, &meta)?;
            return self.import_dmabuf_video_frame(
                &frame,
                "GstDmaBufMemory->Vulkan external DRM modifier image planes",
            );
        }
        if native_vulkan_gst_buffer_has_va_memory(buffer) {
            let frame = native_vulkan_gst_va_dmabuf_frame(buffer, &meta)?;
            return self.import_dmabuf_video_frame(
                &frame,
                "GstVAMemory->vaExportSurfaceHandle(DRM PRIME)->Vulkan external DRM modifier image planes",
            );
        }
        if !native_vulkan_gst_buffer_has_cuda_memory(buffer) {
            return Err(NativeVulkanError::Video(format!(
                "native Vulkan import expected DMABuf, VAMemory, or CUDAMemory, got {}",
                native_vulkan_gst_memory_types(buffer).join("|")
            )));
        }
        let cuda_context = native_vulkan_gst_cuda_context_from_buffer(buffer)?;
        let recreate = match self.video_texture.as_ref() {
            Some(NativeVulkanVideoTexture::Cuda(texture)) => {
                !texture.matches(cuda_context, meta.width, meta.height)
            }
            _ => true,
        };
        if recreate {
            let texture = NativeVulkanCudaVideoTexture::new(
                &self.instance,
                self.physical_device,
                self.queue,
                self.command_pool,
                &self.device,
                self.queue_family_index,
                cuda_context,
                meta.width,
                meta.height,
            )?;
            if let Some(old_texture) = self.video_texture.take() {
                old_texture.destroy(&self.device);
            }
            self.video_texture = Some(NativeVulkanVideoTexture::Cuda(texture));
            let renderer = self.video_renderer.as_mut().ok_or_else(|| {
                NativeVulkanError::Video(
                    "native Vulkan video renderer is not initialized".to_owned(),
                )
            })?;
            renderer.update_descriptors(
                &self.device,
                self.video_texture
                    .as_ref()
                    .expect("video texture must exist after create"),
            );
        }
        let Some(NativeVulkanVideoTexture::Cuda(texture)) = self.video_texture.as_mut() else {
            return Err(NativeVulkanError::Video(
                "native Vulkan CUDA texture was not initialized".to_owned(),
            ));
        };
        texture.copy_sample(buffer, &meta)?;
        Ok(NativeVulkanVideoImportReport {
            width: meta.width,
            height: meta.height,
            memory_path: "CUDAMemory->CUDA->Vulkan external image planes".to_owned(),
            elapsed_us: 0,
        })
    }

    #[cfg(feature = "native-vulkan-gst-video")]
    fn import_dmabuf_video_frame(
        &mut self,
        frame: &NativeVulkanDmabufVideoFrame,
        memory_path: &'static str,
    ) -> Result<NativeVulkanVideoImportReport, NativeVulkanError> {
        let texture = NativeVulkanDmabufVideoTexture::new(
            &self.instance,
            self.physical_device,
            self.queue,
            self.command_pool,
            &self.device,
            self.queue_family_index,
            frame,
        )?;
        if let Some(old_texture) = self.video_texture.take() {
            old_texture.destroy(&self.device);
        }
        self.video_texture = Some(NativeVulkanVideoTexture::Dmabuf(texture));
        let renderer = self.video_renderer.as_mut().ok_or_else(|| {
            NativeVulkanError::Video("native Vulkan video renderer is not initialized".to_owned())
        })?;
        renderer.update_descriptors(
            &self.device,
            self.video_texture
                .as_ref()
                .expect("video texture must exist after DMABuf import"),
        );
        Ok(NativeVulkanVideoImportReport {
            width: frame.width,
            height: frame.height,
            memory_path: memory_path.to_owned(),
            elapsed_us: 0,
        })
    }

    #[cfg(not(feature = "native-vulkan-gst-video"))]
    fn video_frontend_snapshot(&self) -> Option<NativeVulkanGstVideoFrontendSnapshot> {
        None
    }

    #[cfg(not(feature = "native-vulkan-gst-video"))]
    fn video_import_snapshot(&self) -> Option<NativeVulkanVideoImportSnapshot> {
        None
    }
}

impl Drop for NativeVulkanSession {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            #[cfg(feature = "native-vulkan-gst-video")]
            if let Some(texture) = self.video_texture.take() {
                texture.destroy(&self.device);
            }
            #[cfg(feature = "native-vulkan-gst-video")]
            if let Some(renderer) = self.video_renderer.take() {
                renderer.destroy(&self.device);
            }
            if let Some(static_upload) = self.static_upload.take() {
                static_upload.destroy(&self.device);
            }
            self.device.destroy_fence(self.in_flight, None);
            self.device.destroy_semaphore(self.render_finished, None);
            self.device.destroy_semaphore(self.image_available, None);
            self.device.destroy_command_pool(self.command_pool, None);
            for view in self.swapchain_image_views.drain(..) {
                self.device.destroy_image_view(view, None);
            }
            self.swapchain_loader
                .destroy_swapchain(self.swapchain, None);
            self.device.destroy_device(None);
            self.surface_loader.destroy_surface(self.surface, None);
            self.instance.destroy_instance(None);
        }
    }
}

pub fn run_clear(
    options: NativeVulkanOptions,
    duration: Duration,
) -> Result<NativeVulkanRuntimeSnapshot, NativeVulkanError> {
    let target_max_fps = options.target_max_fps;
    let mut session = NativeVulkanSession::connect(options)?;
    session.run_for(duration, target_max_fps)
}

pub fn run_static_image(
    options: NativeVulkanOptions,
    duration: Duration,
    plan: StaticWallpaperPlan,
) -> Result<NativeVulkanRuntimeSnapshot, NativeVulkanError> {
    let target_max_fps = options.target_max_fps;
    let item = native_vulkan_static_item(&plan);
    let mut session = NativeVulkanSession::connect_with_render_item(options, item)?;
    session.run_for(duration, target_max_fps)
}

pub fn run_video(
    options: NativeVulkanOptions,
    duration: Duration,
    plan: VideoWallpaperPlan,
) -> Result<NativeVulkanRuntimeSnapshot, NativeVulkanError> {
    let target_max_fps = options.target_max_fps;
    let item = native_vulkan_video_item(&plan);
    let mut session = NativeVulkanSession::connect_with_render_item(options, item)?;
    session.run_for(duration, target_max_fps)
}

#[cfg(feature = "native-vulkan-gst-video")]
struct NativeVulkanGstVideoFrontend {
    pipeline: gst::Element,
    sink: gst::Element,
    bus: gst::Bus,
    loop_playback: bool,
    decoder_policy: VideoDecoderPolicy,
    eos_messages: u64,
    segment_done_messages: u64,
    frames_received: u64,
    last_sample_caps: Option<String>,
    last_sample_format: Option<String>,
    last_sample_size: Option<(u32, u32)>,
    last_sample_pts_ms: Option<u64>,
    last_sample_duration_ms: Option<u64>,
    last_sample_pts_delta_ms: Option<u64>,
    last_sample_memory_types: Vec<String>,
    latest_sample: Option<gst::Sample>,
    last_error: Option<String>,
}

#[cfg(feature = "native-vulkan-gst-video")]
impl NativeVulkanGstVideoFrontend {
    fn new(item: &NativeVulkanRenderItem) -> Result<Self, NativeVulkanError> {
        let NativeVulkanRenderItem::Video {
            source,
            loop_playback,
            decoder_policy,
            start_offset_ms,
            ..
        } = item
        else {
            return Err(NativeVulkanError::Video(
                "GStreamer frontend requires a video render item".to_owned(),
            ));
        };

        gst::init().map_err(|err| NativeVulkanError::Video(err.to_string()))?;
        apply_decoder_rank_policy(*decoder_policy);
        native_vulkan_apply_memory_path_decoder_policy();
        let pipeline = native_vulkan_gst_video_pipeline(source)?;
        let sink = pipeline
            .by_name("gilder-native-vulkan-video-appsink")
            .ok_or_else(|| NativeVulkanError::Video("video appsink not found".to_owned()))?;
        let bus = pipeline
            .bus()
            .ok_or_else(|| NativeVulkanError::Video("video pipeline has no bus".to_owned()))?;
        pipeline
            .set_state(gst::State::Paused)
            .map_err(|err| NativeVulkanError::Video(err.to_string()))?;
        let _ = pipeline.state(gst::ClockTime::from_seconds(5));
        if *loop_playback {
            native_vulkan_gst_seek_loop_segment(pipeline.upcast_ref(), *start_offset_ms)?;
        } else if *start_offset_ms > 0 {
            native_vulkan_gst_seek_once(pipeline.upcast_ref(), *start_offset_ms)?;
        }
        pipeline
            .set_state(gst::State::Playing)
            .map_err(|err| NativeVulkanError::Video(err.to_string()))?;

        Ok(Self {
            pipeline: pipeline.upcast::<gst::Element>(),
            sink,
            bus,
            loop_playback: *loop_playback,
            decoder_policy: *decoder_policy,
            eos_messages: 0,
            segment_done_messages: 0,
            frames_received: 0,
            last_sample_caps: None,
            last_sample_format: None,
            last_sample_size: None,
            last_sample_pts_ms: None,
            last_sample_duration_ms: None,
            last_sample_pts_delta_ms: None,
            last_sample_memory_types: Vec::new(),
            latest_sample: None,
            last_error: None,
        })
    }

    fn poll(&mut self) -> Result<(), NativeVulkanError> {
        self.poll_bus()?;
        self.pull_available_samples();
        Ok(())
    }

    fn poll_bus(&mut self) -> Result<(), NativeVulkanError> {
        while let Some(message) = self.bus.pop() {
            match message.view() {
                gst::MessageView::Eos(_) => {
                    self.eos_messages = self.eos_messages.saturating_add(1);
                    if self.loop_playback {
                        native_vulkan_gst_seek_loop_segment(&self.pipeline, 0)?;
                    } else {
                        self.pipeline
                            .set_state(gst::State::Paused)
                            .map_err(|err| NativeVulkanError::Video(err.to_string()))?;
                    }
                }
                gst::MessageView::SegmentDone(_) => {
                    self.segment_done_messages = self.segment_done_messages.saturating_add(1);
                    if self.loop_playback {
                        native_vulkan_gst_seek_loop_segment(&self.pipeline, 0)?;
                    }
                }
                gst::MessageView::Error(err) => {
                    let mut message = format!(
                        "{}: {}",
                        err.src()
                            .map(|src| src.path_string())
                            .unwrap_or_else(|| "gstreamer".into()),
                        err.error()
                    );
                    if let Some(debug) = err.debug() {
                        message.push_str(": ");
                        message.push_str(&debug);
                    }
                    self.last_error = Some(message.clone());
                    return Err(NativeVulkanError::Video(message));
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn pull_available_samples(&mut self) {
        let sample = self
            .sink
            .emit_by_name::<Option<gst::Sample>>("try-pull-sample", &[&0u64]);
        let Some(sample) = sample else {
            return;
        };
        self.record_sample(&sample);
        self.latest_sample = Some(sample);
    }

    fn record_sample(&mut self, sample: &gst::Sample) {
        self.frames_received = self.frames_received.saturating_add(1);
        self.last_sample_caps = sample.caps().map(|caps| caps.to_string());
        if let Some(caps) = sample.caps()
            && let Some(structure) = caps.structure(0)
        {
            self.last_sample_format = structure.get::<String>("format").ok();
            let width = structure.get::<i32>("width").ok();
            let height = structure.get::<i32>("height").ok();
            self.last_sample_size = width.zip(height).and_then(|(width, height)| {
                Some((u32::try_from(width).ok()?, u32::try_from(height).ok()?))
            });
        }
        self.last_sample_memory_types = sample
            .buffer()
            .map(|buffer| {
                let pts_ms = native_vulkan_clock_time_ms(buffer.pts());
                self.last_sample_pts_delta_ms = self
                    .last_sample_pts_ms
                    .zip(pts_ms)
                    .and_then(|(previous, current)| current.checked_sub(previous));
                self.last_sample_pts_ms = pts_ms;
                self.last_sample_duration_ms = native_vulkan_clock_time_ms(buffer.duration());
                native_vulkan_gst_memory_types(buffer)
            })
            .unwrap_or_else(|| {
                self.last_sample_pts_ms = None;
                self.last_sample_duration_ms = None;
                self.last_sample_pts_delta_ms = None;
                Vec::new()
            });
        self.last_error = None;
    }

    fn take_latest_sample(&mut self) -> Option<gst::Sample> {
        self.latest_sample.take()
    }

    fn snapshot(&self) -> NativeVulkanGstVideoFrontendSnapshot {
        let gst_state = Some(
            self.pipeline
                .state(gst::ClockTime::ZERO)
                .1
                .name()
                .to_string(),
        );
        let decoder_reports = actual_decoder_reports(&self.pipeline);
        let actual_decoders = decoder_reports
            .iter()
            .map(|report| report.element.clone())
            .collect::<Vec<_>>();
        let decoder_policy_status = Some(format!(
            "{:?}",
            decoder_policy_status(self.decoder_policy, &decoder_reports)
        ));
        let caps_reports = video_caps_reports(&self.pipeline);
        let mut caps_memory_features = caps_reports
            .iter()
            .flat_map(|report| report.memory_features.iter().cloned())
            .collect::<Vec<_>>();
        caps_memory_features.sort();
        caps_memory_features.dedup();
        let caps_report_count = caps_reports.len();
        let caps_reports = caps_reports
            .into_iter()
            .map(|report| NativeVulkanVideoCapsSnapshot {
                element: report.element,
                pad: report.pad,
                direction: report.direction,
                caps: report.caps,
                source: report.source,
                memory_features: report.memory_features,
            })
            .collect();

        NativeVulkanGstVideoFrontendSnapshot {
            gst_state,
            eos_messages: self.eos_messages,
            segment_done_messages: self.segment_done_messages,
            frames_received: self.frames_received,
            last_sample_caps: self.last_sample_caps.clone(),
            last_sample_format: self.last_sample_format.clone(),
            last_sample_size: self.last_sample_size,
            last_sample_pts_ms: self.last_sample_pts_ms,
            last_sample_duration_ms: self.last_sample_duration_ms,
            last_sample_pts_delta_ms: self.last_sample_pts_delta_ms,
            last_sample_memory_types: self.last_sample_memory_types.clone(),
            actual_decoders,
            decoder_policy_status,
            caps_report_count,
            caps_memory_features,
            caps_reports,
            last_error: self.last_error.clone(),
        }
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
impl Drop for NativeVulkanGstVideoFrontend {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_video_pipeline(source: &PathBuf) -> Result<gst::Pipeline, NativeVulkanError> {
    let pipeline = gst::Pipeline::new();
    let filesrc = native_vulkan_gst_element("filesrc")?;
    filesrc.set_property("location", source.to_string_lossy().as_ref());
    let decodebin = native_vulkan_gst_element("decodebin")?;
    if let Ok(decodebin_bin) = decodebin.clone().dynamic_cast::<gst::Bin>() {
        decodebin_bin.connect_element_added(|_, element| {
            native_vulkan_configure_decoder_low_memory(element);
        });
    }
    let queue = native_vulkan_gst_element("queue")?;
    native_vulkan_configure_queue(&queue);
    let sink = native_vulkan_gst_element("appsink")?;
    sink.set_property("name", "gilder-native-vulkan-video-appsink");
    native_vulkan_configure_appsink(&sink);

    pipeline
        .add_many([&filesrc, &decodebin, &queue, &sink])
        .map_err(|err| NativeVulkanError::Video(err.to_string()))?;
    filesrc
        .link(&decodebin)
        .map_err(|err| NativeVulkanError::Video(err.to_string()))?;
    queue
        .link(&sink)
        .map_err(|err| NativeVulkanError::Video(err.to_string()))?;

    let queue_sink = queue
        .static_pad("sink")
        .ok_or_else(|| NativeVulkanError::Video("queue has no sink pad".to_owned()))?;
    decodebin.connect_pad_added(move |_, pad| {
        if queue_sink.is_linked() {
            return;
        }
        let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
        let is_video = caps
            .structure(0)
            .map(|structure| structure.name().starts_with("video/"))
            .unwrap_or(false);
        if is_video {
            let _ = pad.link(&queue_sink);
        }
    });

    Ok(pipeline)
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_element(name: &str) -> Result<gst::Element, NativeVulkanError> {
    gst::ElementFactory::make(name)
        .build()
        .map_err(|err| NativeVulkanError::Video(format!("create {name}: {err}")))
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_seek_once(
    pipeline: &gst::Element,
    start_offset_ms: u64,
) -> Result<(), NativeVulkanError> {
    pipeline
        .seek_simple(
            gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
            gst::ClockTime::from_mseconds(start_offset_ms),
        )
        .map_err(|err| NativeVulkanError::Video(err.to_string()))
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_seek_loop_segment(
    pipeline: &gst::Element,
    start_offset_ms: u64,
) -> Result<(), NativeVulkanError> {
    pipeline
        .seek(
            1.0,
            gst::SeekFlags::FLUSH | gst::SeekFlags::SEGMENT | gst::SeekFlags::KEY_UNIT,
            gst::SeekType::Set,
            gst::ClockTime::from_mseconds(start_offset_ms),
            gst::SeekType::None,
            gst::ClockTime::NONE,
        )
        .map_err(|err| NativeVulkanError::Video(err.to_string()))
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_configure_decoder_low_memory(decoder: &gst::Element) {
    if decoder.find_property("qos").is_some() {
        decoder.set_property("qos", false);
    }
    if decoder.find_property("max-display-delay").is_some() {
        decoder.set_property("max-display-delay", 0i32);
    }
    if decoder.find_property("num-output-surfaces").is_some() {
        decoder.set_property(
            "num-output-surfaces",
            native_vulkan_gst_nvdec_output_surfaces(),
        );
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_configure_queue(queue: &gst::Element) {
    if queue.find_property("max-size-buffers").is_some() {
        queue.set_property("max-size-buffers", native_vulkan_gst_video_queue_frames());
    }
    if queue.find_property("max-size-bytes").is_some() {
        queue.set_property("max-size-bytes", 0u32);
    }
    if queue.find_property("max-size-time").is_some() {
        queue.set_property("max-size-time", 0u64);
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_nvdec_output_surfaces() -> u32 {
    std::env::var("GILDER_VULKAN_GST_NVDEC_OUTPUT_SURFACES")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .map(|value| value.clamp(1, 64))
        .unwrap_or(1)
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_configure_appsink(sink: &gst::Element) {
    if let Some(caps) = native_vulkan_gst_forced_sink_caps() {
        sink.set_property("caps", &caps);
    }
    if sink.find_property("sync").is_some() {
        sink.set_property("sync", true);
    }
    if sink.find_property("async").is_some() {
        sink.set_property("async", false);
    }
    if sink.find_property("emit-signals").is_some() {
        sink.set_property("emit-signals", false);
    }
    if sink.find_property("enable-last-sample").is_some() {
        sink.set_property("enable-last-sample", false);
    }
    if sink.find_property("wait-on-eos").is_some() {
        sink.set_property("wait-on-eos", false);
    }
    if sink.find_property("max-buffers").is_some() {
        sink.set_property("max-buffers", native_vulkan_gst_video_queue_frames());
    }
    if sink.find_property("drop").is_some() {
        sink.set_property("drop", false);
    }
    if sink.find_property("qos").is_some() {
        sink.set_property("qos", false);
    }
    if sink.find_property("max-lateness").is_some() {
        sink.set_property("max-lateness", -1i64);
    }
    if sink.find_property("processing-deadline").is_some() {
        sink.set_property("processing-deadline", 0u64);
    }
    if sink.find_property("render-delay").is_some() {
        sink.set_property("render-delay", 0u64);
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_video_queue_frames() -> u32 {
    1
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_forced_sink_caps() -> Option<gst::Caps> {
    if !native_vulkan_gst_prefers_dmabuf() {
        return None;
    }
    Some(
        gst::Caps::builder_full()
            .structure_with_features(
                gst::Structure::builder("video/x-raw")
                    .field("format", "NV12")
                    .build(),
                gst::CapsFeatures::new(["memory:VAMemory"]),
            )
            .structure_with_features(
                gst::Structure::builder("video/x-raw")
                    .field("format", "DMA_DRM")
                    .build(),
                gst::CapsFeatures::new(["memory:DMABuf"]),
            )
            .build(),
    )
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_apply_memory_path_decoder_policy() {
    if !native_vulkan_gst_prefers_dmabuf() {
        return;
    }
    for element in [
        "vah264dec",
        "vah265dec",
        "vavp8dec",
        "vavp9dec",
        "vaav1dec",
        "nvh264dec",
        "nvh265dec",
        "nvvp8dec",
        "nvvp9dec",
        "nvav1dec",
        "avdec_h264",
        "openh264dec",
        "vp9dec",
        "avdec_vp9",
        "dav1ddec",
        "avdec_av1",
        "av1dec",
    ] {
        let Some(factory) = gst::ElementFactory::find(element) else {
            continue;
        };
        if element.starts_with("va") {
            factory.set_rank(gst::Rank::PRIMARY + 2048);
        } else {
            factory.set_rank(gst::Rank::NONE);
        }
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_prefers_dmabuf() -> bool {
    std::env::var("GILDER_VULKAN_GST_MEMORY_PATH")
        .map(|memory_path| {
            matches!(
                memory_path.as_str(),
                "dmabuf" | "DMABuf" | "gst-dmabuf" | "direct-dmabuf"
            )
        })
        .unwrap_or(false)
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_trim_process_heap() {
    #[cfg(target_os = "linux")]
    unsafe {
        libc::malloc_trim(0);
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_clock_time_ms(value: Option<gst::ClockTime>) -> Option<u64> {
    value.map(|value| value.mseconds())
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_elapsed_us(value: Duration) -> u64 {
    value.as_micros().min(u128::from(u64::MAX)) as u64
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_memory_types(buffer: &gst::BufferRef) -> Vec<String> {
    (0..buffer.n_memory())
        .map(|index| native_vulkan_gst_memory_type(buffer.peek_memory(index)))
        .collect()
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_memory_type(memory: &gst::MemoryRef) -> String {
    for memory_type in [
        "CUDAMemory",
        "GLMemory",
        "DMABuf",
        "VAMemory",
        "SystemMemory",
    ] {
        if memory.is_type(memory_type) {
            return memory_type.to_owned();
        }
    }
    let Some(memory_type) = memory
        .allocator()
        .map(|allocator| allocator.memory_type().to_string())
    else {
        return "unknown".to_owned();
    };
    let lower = memory_type.to_ascii_lowercase();
    if lower.contains("cuda") {
        "CUDAMemory".to_owned()
    } else if lower.contains("gl") {
        "GLMemory".to_owned()
    } else if lower.contains("dmabuf") || lower.contains("dma-buf") {
        "DMABuf".to_owned()
    } else if lower.contains("va") {
        "VAMemory".to_owned()
    } else if lower.contains("system") {
        "SystemMemory".to_owned()
    } else {
        memory_type
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_dmabuf_frame(
    sample: &gst::Sample,
    buffer: &gst::BufferRef,
    meta: &NativeVulkanGstSystemNv12Meta,
) -> Result<NativeVulkanDmabufVideoFrame, NativeVulkanError> {
    let (fourcc, modifier) = native_vulkan_gst_sample_drm_format(sample)?;
    if fourcc != DRM_FORMAT_NV12 {
        return Err(NativeVulkanError::Video(format!(
            "native Vulkan DMABuf importer only supports NV12 for now, got fourcc=0x{fourcc:08x}"
        )));
    }
    let y = native_vulkan_gst_dmabuf_plane(buffer, meta.y)?;
    let uv = native_vulkan_gst_dmabuf_plane(buffer, meta.uv)?;
    if y.fd != uv.fd {
        return Err(NativeVulkanError::Video(
            "native Vulkan DMABuf importer currently requires y/uv planes in one fd".to_owned(),
        ));
    }
    Ok(NativeVulkanDmabufVideoFrame {
        width: meta.width,
        height: meta.height,
        fd: y.fd,
        modifier,
        y,
        uv,
        _owned_fds: Vec::new(),
    })
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_sample_drm_format(
    sample: &gst::Sample,
) -> Result<(u32, u64), NativeVulkanError> {
    let caps = sample
        .caps()
        .ok_or_else(|| NativeVulkanError::Video("DMABuf sample has no caps".to_owned()))?;
    let structure = caps.structure(0).ok_or_else(|| {
        NativeVulkanError::Video("DMABuf sample caps has no structure".to_owned())
    })?;
    if let Ok(drm_format) = structure.get::<String>("drm-format") {
        let (fourcc, modifier) = native_vulkan_drm_fourcc_modifier_from_caps_format(&drm_format)
            .ok_or_else(|| {
                NativeVulkanError::Video(format!(
                    "native Vulkan DMABuf could not parse drm-format={drm_format}"
                ))
            })?;
        let modifier = modifier.ok_or_else(|| {
            NativeVulkanError::Video(format!(
                "native Vulkan DMABuf drm-format={drm_format} has no explicit modifier"
            ))
        })?;
        return Ok((fourcc, modifier));
    }

    let format = structure
        .get::<String>("format")
        .unwrap_or_else(|_| "unknown".to_owned());
    if format == "NV12" {
        return Ok((DRM_FORMAT_NV12, DRM_FORMAT_MOD_LINEAR));
    }
    Err(NativeVulkanError::Video(format!(
        "native Vulkan DMABuf expected drm-format or NV12 caps, got format={format}"
    )))
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_drm_fourcc_modifier_from_caps_format(format: &str) -> Option<(u32, Option<u64>)> {
    let format = CString::new(format).ok()?;
    let mut modifier = 0u64;
    let fourcc = unsafe { gst_video_dma_drm_fourcc_from_string(format.as_ptr(), &mut modifier) };
    (fourcc != 0).then_some((
        fourcc,
        (modifier != DRM_FORMAT_MOD_INVALID).then_some(modifier),
    ))
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_dmabuf_plane(
    buffer: &gst::BufferRef,
    plane: NativeVulkanGstSystemNv12Plane,
) -> Result<NativeVulkanDmabufVideoPlane, NativeVulkanError> {
    let plane_end = plane.offset.checked_add(1).ok_or_else(|| {
        NativeVulkanError::Video("native Vulkan DMABuf plane offset overflow".to_owned())
    })?;
    let (memory_range, memory_skip) = buffer
        .find_memory(plane.offset..plane_end)
        .ok_or_else(|| NativeVulkanError::Video("DMABuf plane has no memory".to_owned()))?;
    let memory_index = memory_range.start;
    if memory_index >= buffer.n_memory() {
        return Err(NativeVulkanError::Video(
            "native Vulkan DMABuf memory index out of range".to_owned(),
        ));
    }
    let memory = buffer.peek_memory(memory_index);
    let fd = native_vulkan_dmabuf_memory_fd(memory).ok_or_else(|| {
        NativeVulkanError::Video(format!(
            "native Vulkan DMABuf plane memory is not GstDmaBufMemory: {}",
            native_vulkan_gst_memory_type(memory)
        ))
    })?;
    let (_, memory_offset, _) = memory.sizes();
    let offset = memory_offset
        .checked_add(memory_skip)
        .and_then(|offset| u64::try_from(offset).ok())
        .ok_or_else(|| {
            NativeVulkanError::Video("native Vulkan DMABuf plane offset too large".to_owned())
        })?;
    Ok(NativeVulkanDmabufVideoPlane {
        fd,
        offset,
        stride: plane.stride,
        height: plane.height,
    })
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_buffer_has_dmabuf_memory(buffer: &gst::BufferRef) -> bool {
    (0..buffer.n_memory()).any(|index| native_vulkan_is_dmabuf_memory(buffer.peek_memory(index)))
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_is_dmabuf_memory(memory: &gst::MemoryRef) -> bool {
    let is_dmabuf =
        unsafe { gst_is_dmabuf_memory(memory.as_ptr().cast_mut()) } != gst::glib::ffi::GFALSE;
    is_dmabuf || memory.is_type("DMABuf")
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_dmabuf_memory_fd(memory: &gst::MemoryRef) -> Option<i32> {
    if !native_vulkan_is_dmabuf_memory(memory) {
        return None;
    }
    let fd = unsafe { gst_dmabuf_memory_get_fd(memory.as_ptr().cast_mut()) };
    (fd >= 0).then_some(fd)
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_buffer_has_va_memory(buffer: &gst::BufferRef) -> bool {
    (0..buffer.n_memory()).any(|index| native_vulkan_is_va_memory(buffer.peek_memory(index)))
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_is_va_memory(memory: &gst::MemoryRef) -> bool {
    memory.is_type("VAMemory") || native_vulkan_gst_memory_type(memory) == "VAMemory"
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_va_dmabuf_frame(
    buffer: &gst::BufferRef,
    meta: &NativeVulkanGstSystemNv12Meta,
) -> Result<NativeVulkanDmabufVideoFrame, NativeVulkanError> {
    let va_surface = native_vulkan_gst_va_surface(buffer)?;
    native_vulkan_va_check(
        unsafe { vaSyncSurface(va_surface.display, va_surface.surface) },
        "vaSyncSurface(video VAMemory)",
    )?;
    let exported = native_vulkan_va_export_prime_surface(va_surface)?;
    native_vulkan_va_prime_surface_to_dmabuf_frame(exported, meta)
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_va_surface(
    buffer: &gst::BufferRef,
) -> Result<NativeVulkanVaSurface, NativeVulkanError> {
    let display = unsafe { gst_va_buffer_peek_display(buffer.as_mut_ptr()) };
    let surface = unsafe { gst_va_buffer_get_surface(buffer.as_mut_ptr()) };
    if !display.is_null() && surface != VA_INVALID_SURFACE {
        let va_display = unsafe { gst_va_display_get_va_dpy(display) };
        if !va_display.is_null() {
            return Ok(NativeVulkanVaSurface {
                display: va_display,
                surface,
            });
        }
    }

    for index in 0..buffer.n_memory() {
        let memory = buffer.peek_memory(index);
        if !native_vulkan_is_va_memory(memory) {
            continue;
        }
        let display = unsafe { gst_va_memory_peek_display(memory.as_ptr().cast_mut()) };
        let surface = unsafe { gst_va_memory_get_surface(memory.as_ptr().cast_mut()) };
        if display.is_null() || surface == VA_INVALID_SURFACE {
            continue;
        }
        let va_display = unsafe { gst_va_display_get_va_dpy(display) };
        if !va_display.is_null() {
            return Ok(NativeVulkanVaSurface {
                display: va_display,
                surface,
            });
        }
    }

    Err(NativeVulkanError::Video(
        "native Vulkan VAMemory importer could not find a VA display/surface".to_owned(),
    ))
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_va_export_prime_surface(
    surface: NativeVulkanVaSurface,
) -> Result<NativeVulkanVaExportedPrimeSurface, NativeVulkanError> {
    let separate_flags = VA_EXPORT_SURFACE_READ_ONLY | VA_EXPORT_SURFACE_SEPARATE_LAYERS;
    match native_vulkan_va_export_prime_surface_with_flags(surface, separate_flags) {
        Ok(exported) => Ok(exported),
        Err(separate_err) => {
            let composed_flags = VA_EXPORT_SURFACE_READ_ONLY | VA_EXPORT_SURFACE_COMPOSED_LAYERS;
            native_vulkan_va_export_prime_surface_with_flags(surface, composed_flags).map_err(
                |composed_err| {
                    NativeVulkanError::Video(format!(
                        "{separate_err}; composed VA DRM PRIME export also failed: {composed_err}"
                    ))
                },
            )
        }
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_va_export_prime_surface_with_flags(
    surface: NativeVulkanVaSurface,
    flags: u32,
) -> Result<NativeVulkanVaExportedPrimeSurface, NativeVulkanError> {
    let mut descriptor = NativeVulkanVaDrmPrimeSurfaceDescriptor::default();
    native_vulkan_va_check(
        unsafe {
            vaExportSurfaceHandle(
                surface.display,
                surface.surface,
                VA_SURFACE_ATTRIB_MEM_TYPE_DRM_PRIME_2,
                flags,
                (&mut descriptor as *mut NativeVulkanVaDrmPrimeSurfaceDescriptor).cast(),
            )
        },
        "vaExportSurfaceHandle(video VAMemory DRM PRIME)",
    )?;
    NativeVulkanVaExportedPrimeSurface::new(descriptor)
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_va_prime_surface_to_dmabuf_frame(
    exported: NativeVulkanVaExportedPrimeSurface,
    meta: &NativeVulkanGstSystemNv12Meta,
) -> Result<NativeVulkanDmabufVideoFrame, NativeVulkanError> {
    let descriptor = exported.descriptor;
    native_vulkan_validate_va_prime_descriptor(&descriptor, meta)?;
    let (y_object, y_offset, y_pitch, uv_object, uv_offset, uv_pitch) =
        native_vulkan_va_nv12_plane_layouts(&descriptor)?;
    if y_object != uv_object {
        return Err(NativeVulkanError::Video(format!(
            "native Vulkan VA DMABuf importer currently requires a single DRM object, got y_object={y_object} uv_object={uv_object}"
        )));
    }
    let object = descriptor.objects.get(y_object).ok_or_else(|| {
        NativeVulkanError::Video("VA DRM PRIME object index out of range".to_owned())
    })?;
    if object.drm_format_modifier == DRM_FORMAT_MOD_INVALID {
        return Err(NativeVulkanError::Video(
            "VA DRM PRIME export returned an invalid DRM modifier".to_owned(),
        ));
    }

    Ok(NativeVulkanDmabufVideoFrame {
        width: meta.width,
        height: meta.height,
        fd: exported
            .owned_fds
            .get(y_object)
            .ok_or_else(|| {
                NativeVulkanError::Video("VA DRM PRIME fd index out of range".to_owned())
            })?
            .as_raw_fd(),
        modifier: object.drm_format_modifier,
        y: NativeVulkanDmabufVideoPlane {
            fd: exported
                .owned_fds
                .get(y_object)
                .expect("VA DRM PRIME fd checked above")
                .as_raw_fd(),
            offset: u64::from(y_offset),
            stride: y_pitch,
            height: meta.height,
        },
        uv: NativeVulkanDmabufVideoPlane {
            fd: exported
                .owned_fds
                .get(uv_object)
                .ok_or_else(|| {
                    NativeVulkanError::Video("VA DRM PRIME uv fd index out of range".to_owned())
                })?
                .as_raw_fd(),
            offset: u64::from(uv_offset),
            stride: uv_pitch,
            height: meta.height / 2,
        },
        _owned_fds: exported.owned_fds,
    })
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_validate_va_prime_descriptor(
    descriptor: &NativeVulkanVaDrmPrimeSurfaceDescriptor,
    meta: &NativeVulkanGstSystemNv12Meta,
) -> Result<(), NativeVulkanError> {
    if descriptor.fourcc != VA_FOURCC_NV12 && descriptor.fourcc != DRM_FORMAT_NV12 {
        return Err(NativeVulkanError::Video(format!(
            "native Vulkan VA DMABuf importer only supports NV12, got fourcc=0x{:08x}",
            descriptor.fourcc
        )));
    }
    if descriptor.width != meta.width || descriptor.height != meta.height {
        return Err(NativeVulkanError::Video(format!(
            "VA DRM PRIME descriptor size {}x{} does not match sample {}x{}",
            descriptor.width, descriptor.height, meta.width, meta.height
        )));
    }
    if descriptor.num_objects == 0 || descriptor.num_objects > 4 {
        return Err(NativeVulkanError::Video(format!(
            "VA DRM PRIME descriptor has invalid object count {}",
            descriptor.num_objects
        )));
    }
    if descriptor.num_layers == 0 || descriptor.num_layers > 4 {
        return Err(NativeVulkanError::Video(format!(
            "VA DRM PRIME descriptor has invalid layer count {}",
            descriptor.num_layers
        )));
    }
    Ok(())
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_va_nv12_plane_layouts(
    descriptor: &NativeVulkanVaDrmPrimeSurfaceDescriptor,
) -> Result<(usize, u32, u32, usize, u32, u32), NativeVulkanError> {
    let layer_count = descriptor.num_layers as usize;
    for layer in descriptor.layers[..layer_count].iter() {
        if layer.drm_format == DRM_FORMAT_NV12 && layer.num_planes >= 2 {
            let y_object = native_vulkan_va_layer_object_index(layer, 0, descriptor)?;
            let uv_object = native_vulkan_va_layer_object_index(layer, 1, descriptor)?;
            return Ok((
                y_object,
                layer.offset[0],
                layer.pitch[0],
                uv_object,
                layer.offset[1],
                layer.pitch[1],
            ));
        }
    }

    let y_layer = descriptor.layers[..layer_count]
        .iter()
        .find(|layer| layer.drm_format == DRM_FORMAT_R8 && layer.num_planes >= 1)
        .ok_or_else(|| {
            NativeVulkanError::Video(
                "VA DRM PRIME separate-layer export has no DRM_FORMAT_R8 luma layer".to_owned(),
            )
        })?;
    let uv_layer = descriptor.layers[..layer_count]
        .iter()
        .find(|layer| layer.drm_format == DRM_FORMAT_GR88 && layer.num_planes >= 1)
        .ok_or_else(|| {
            NativeVulkanError::Video(
                "VA DRM PRIME separate-layer export has no DRM_FORMAT_GR88 chroma layer".to_owned(),
            )
        })?;
    let y_object = native_vulkan_va_layer_object_index(y_layer, 0, descriptor)?;
    let uv_object = native_vulkan_va_layer_object_index(uv_layer, 0, descriptor)?;
    Ok((
        y_object,
        y_layer.offset[0],
        y_layer.pitch[0],
        uv_object,
        uv_layer.offset[0],
        uv_layer.pitch[0],
    ))
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_va_layer_object_index(
    layer: &NativeVulkanVaDrmPrimeLayer,
    plane: usize,
    descriptor: &NativeVulkanVaDrmPrimeSurfaceDescriptor,
) -> Result<usize, NativeVulkanError> {
    let object_index = layer.object_index[plane] as usize;
    if object_index >= descriptor.num_objects as usize {
        return Err(NativeVulkanError::Video(format!(
            "VA DRM PRIME layer object index {object_index} is out of range"
        )));
    }
    Ok(object_index)
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_va_check(
    status: NativeVulkanVaStatus,
    operation: &'static str,
) -> Result<(), NativeVulkanError> {
    if status == VA_STATUS_SUCCESS {
        return Ok(());
    }
    let message = unsafe {
        let error = vaErrorStr(status);
        if error.is_null() {
            format!("{operation} failed with VAStatus {status}")
        } else {
            format!(
                "{operation} failed with VAStatus {status}: {}",
                CStr::from_ptr(error).to_string_lossy()
            )
        }
    };
    Err(NativeVulkanError::Video(message))
}

#[cfg(feature = "native-vulkan-gst-video")]
#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeVulkanVideoImportReport {
    width: u32,
    height: u32,
    memory_path: String,
    elapsed_us: u64,
}

#[cfg(feature = "native-vulkan-gst-video")]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct NativeVulkanVideoImportStatus {
    frames_imported: u64,
    last_import_size: Option<(u32, u32)>,
    last_import_memory_path: Option<String>,
    last_import_error: Option<String>,
    last_import_elapsed_us: Option<u64>,
    max_import_elapsed_us: Option<u64>,
}

#[cfg(feature = "native-vulkan-gst-video")]
impl NativeVulkanVideoImportStatus {
    fn record_import(&mut self, report: NativeVulkanVideoImportReport) {
        self.frames_imported = self.frames_imported.saturating_add(1);
        self.last_import_size = Some((report.width, report.height));
        self.last_import_memory_path = Some(report.memory_path);
        self.last_import_error = None;
        self.last_import_elapsed_us = Some(report.elapsed_us);
        self.max_import_elapsed_us = Some(
            self.max_import_elapsed_us
                .map(|current| current.max(report.elapsed_us))
                .unwrap_or(report.elapsed_us),
        );
    }

    fn record_error(&mut self, error: String) {
        self.last_import_error = Some(error);
    }

    fn snapshot(&self) -> NativeVulkanVideoImportSnapshot {
        let texture_import_status = if self.frames_imported > 0 {
            match self.last_import_memory_path.as_deref() {
                Some(path) if path.contains("GstDmaBufMemory") => "importing-dmabuf-vulkan-image",
                _ => "importing-cuda-vulkan-image-planes",
            }
        } else if self.last_import_error.is_some() {
            "waiting-for-supported-importer"
        } else {
            "waiting-for-importable-sample"
        };
        NativeVulkanVideoImportSnapshot {
            texture_import_status,
            frames_imported: self.frames_imported,
            last_import_size: self.last_import_size,
            last_import_memory_path: self.last_import_memory_path.clone(),
            last_import_error: self.last_import_error.clone(),
            last_import_elapsed_us: self.last_import_elapsed_us,
            max_import_elapsed_us: self.max_import_elapsed_us,
        }
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
struct NativeVulkanVideoRenderer {
    render_pass: vk::RenderPass,
    framebuffers: Vec<vk::Framebuffer>,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    descriptor_set: vk::DescriptorSet,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    sampler: vk::Sampler,
    extent: vk::Extent2D,
    target_final_layout: vk::ImageLayout,
}

#[cfg(feature = "native-vulkan-gst-video")]
impl NativeVulkanVideoRenderer {
    fn new(
        device: &ash::Device,
        swapchain_format: vk::Format,
        extent: vk::Extent2D,
        swapchain_image_views: &[vk::ImageView],
    ) -> Result<Self, NativeVulkanError> {
        Self::new_with_target_final_layout(
            device,
            swapchain_format,
            extent,
            swapchain_image_views,
            vk::ImageLayout::PRESENT_SRC_KHR,
        )
    }

    fn new_with_target_final_layout(
        device: &ash::Device,
        target_format: vk::Format,
        extent: vk::Extent2D,
        target_image_views: &[vk::ImageView],
        target_final_layout: vk::ImageLayout,
    ) -> Result<Self, NativeVulkanError> {
        let render_pass =
            native_vulkan_create_video_render_pass(device, target_format, target_final_layout)?;
        let framebuffers = native_vulkan_create_video_framebuffers(
            device,
            render_pass,
            extent,
            target_image_views,
        )?;
        let bindings = [
            native_vulkan_video_sampler_binding(0),
            native_vulkan_video_sampler_binding(1),
        ];
        let descriptor_set_layout_info =
            vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
        let descriptor_set_layout =
            unsafe { device.create_descriptor_set_layout(&descriptor_set_layout_info, None) }
                .map_err(|result| NativeVulkanError::Vulkan {
                    operation: "vkCreateDescriptorSetLayout(video)",
                    result,
                })?;
        let pool_sizes = [vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: 2,
        }];
        let descriptor_pool_info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(1)
            .pool_sizes(&pool_sizes);
        let descriptor_pool =
            match unsafe { device.create_descriptor_pool(&descriptor_pool_info, None) } {
                Ok(pool) => pool,
                Err(result) => {
                    unsafe {
                        device.destroy_descriptor_set_layout(descriptor_set_layout, None);
                        for framebuffer in framebuffers {
                            device.destroy_framebuffer(framebuffer, None);
                        }
                        device.destroy_render_pass(render_pass, None);
                    }
                    return Err(NativeVulkanError::Vulkan {
                        operation: "vkCreateDescriptorPool(video)",
                        result,
                    });
                }
            };
        let set_layouts = [descriptor_set_layout];
        let descriptor_set_allocate_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&set_layouts);
        let descriptor_set =
            match unsafe { device.allocate_descriptor_sets(&descriptor_set_allocate_info) } {
                Ok(sets) => sets[0],
                Err(result) => {
                    unsafe {
                        device.destroy_descriptor_pool(descriptor_pool, None);
                        device.destroy_descriptor_set_layout(descriptor_set_layout, None);
                        for framebuffer in framebuffers {
                            device.destroy_framebuffer(framebuffer, None);
                        }
                        device.destroy_render_pass(render_pass, None);
                    }
                    return Err(NativeVulkanError::Vulkan {
                        operation: "vkAllocateDescriptorSets(video)",
                        result,
                    });
                }
            };
        let sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .max_lod(1.0);
        let sampler = match unsafe { device.create_sampler(&sampler_info, None) } {
            Ok(sampler) => sampler,
            Err(result) => {
                unsafe {
                    device.destroy_descriptor_pool(descriptor_pool, None);
                    device.destroy_descriptor_set_layout(descriptor_set_layout, None);
                    for framebuffer in framebuffers {
                        device.destroy_framebuffer(framebuffer, None);
                    }
                    device.destroy_render_pass(render_pass, None);
                }
                return Err(NativeVulkanError::Vulkan {
                    operation: "vkCreateSampler(video)",
                    result,
                });
            }
        };
        let push_constant_ranges = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX)
            .offset(0)
            .size(16)];
        let pipeline_set_layouts = [descriptor_set_layout];
        let pipeline_layout_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&pipeline_set_layouts)
            .push_constant_ranges(&push_constant_ranges);
        let pipeline_layout =
            match unsafe { device.create_pipeline_layout(&pipeline_layout_info, None) } {
                Ok(layout) => layout,
                Err(result) => {
                    unsafe {
                        device.destroy_sampler(sampler, None);
                        device.destroy_descriptor_pool(descriptor_pool, None);
                        device.destroy_descriptor_set_layout(descriptor_set_layout, None);
                        for framebuffer in framebuffers {
                            device.destroy_framebuffer(framebuffer, None);
                        }
                        device.destroy_render_pass(render_pass, None);
                    }
                    return Err(NativeVulkanError::Vulkan {
                        operation: "vkCreatePipelineLayout(video)",
                        result,
                    });
                }
            };
        let pipeline =
            match native_vulkan_create_video_pipeline(device, render_pass, pipeline_layout, extent)
            {
                Ok(pipeline) => pipeline,
                Err(err) => {
                    unsafe {
                        device.destroy_pipeline_layout(pipeline_layout, None);
                        device.destroy_sampler(sampler, None);
                        device.destroy_descriptor_pool(descriptor_pool, None);
                        device.destroy_descriptor_set_layout(descriptor_set_layout, None);
                        for framebuffer in framebuffers {
                            device.destroy_framebuffer(framebuffer, None);
                        }
                        device.destroy_render_pass(render_pass, None);
                    }
                    return Err(err);
                }
            };

        Ok(Self {
            render_pass,
            framebuffers,
            descriptor_set_layout,
            descriptor_pool,
            descriptor_set,
            pipeline_layout,
            pipeline,
            sampler,
            extent,
            target_final_layout,
        })
    }

    fn target_final_layout(&self) -> vk::ImageLayout {
        self.target_final_layout
    }

    fn update_descriptors(&mut self, device: &ash::Device, texture: &NativeVulkanVideoTexture) {
        let image_infos = [
            vk::DescriptorImageInfo::default()
                .sampler(self.sampler)
                .image_view(texture.y_view())
                .image_layout(texture.image_layout()),
            vk::DescriptorImageInfo::default()
                .sampler(self.sampler)
                .image_view(texture.uv_view())
                .image_layout(texture.image_layout()),
        ];
        let writes = [
            vk::WriteDescriptorSet::default()
                .dst_set(self.descriptor_set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&image_infos[0..1]),
            vk::WriteDescriptorSet::default()
                .dst_set(self.descriptor_set)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&image_infos[1..2]),
        ];
        unsafe {
            device.update_descriptor_sets(&writes, &[]);
        }
    }

    fn record_frame(
        &self,
        device: &ash::Device,
        command_buffer: vk::CommandBuffer,
        image_index: usize,
        swapchain_image: vk::Image,
        swapchain_old_layout: vk::ImageLayout,
        texture: &NativeVulkanVideoTexture,
        fit: FitMode,
    ) -> Result<(), NativeVulkanError> {
        unsafe {
            device
                .reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty())
                .map_err(|result| NativeVulkanError::Vulkan {
                    operation: "vkResetCommandBuffer(video)",
                    result,
                })?;
            let begin_info = vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            device
                .begin_command_buffer(command_buffer, &begin_info)
                .map_err(|result| NativeVulkanError::Vulkan {
                    operation: "vkBeginCommandBuffer(video)",
                    result,
                })?;

            let swapchain_to_attachment = vk::ImageMemoryBarrier::default()
                .old_layout(swapchain_old_layout)
                .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(swapchain_image)
                .subresource_range(native_vulkan_color_subresource_range())
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE);
            let texture_barriers = texture.shader_read_barriers();
            let barriers = [
                swapchain_to_attachment,
                texture_barriers[0],
                texture_barriers[1],
            ];
            device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::TOP_OF_PIPE | vk::PipelineStageFlags::ALL_COMMANDS,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                    | vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &barriers,
            );

            let clear_values = [vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 1.0],
                },
            }];
            let render_area = vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: self.extent,
            };
            let render_pass_begin = vk::RenderPassBeginInfo::default()
                .render_pass(self.render_pass)
                .framebuffer(self.framebuffers[image_index])
                .render_area(render_area)
                .clear_values(&clear_values);
            device.cmd_begin_render_pass(
                command_buffer,
                &render_pass_begin,
                vk::SubpassContents::INLINE,
            );
            device.cmd_bind_pipeline(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline,
            );
            device.cmd_bind_descriptor_sets(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout,
                0,
                &[self.descriptor_set],
                &[],
            );
            let push = native_vulkan_video_fit_push_constants(
                fit,
                (texture.width(), texture.height()),
                (self.extent.width, self.extent.height),
            );
            let push_bytes = std::slice::from_raw_parts(
                push.as_ptr().cast::<u8>(),
                std::mem::size_of_val(&push),
            );
            device.cmd_push_constants(
                command_buffer,
                self.pipeline_layout,
                vk::ShaderStageFlags::VERTEX,
                0,
                push_bytes,
            );
            device.cmd_draw(command_buffer, 3, 1, 0, 0);
            device.cmd_end_render_pass(command_buffer);

            device
                .end_command_buffer(command_buffer)
                .map_err(|result| NativeVulkanError::Vulkan {
                    operation: "vkEndCommandBuffer(video)",
                    result,
                })?;
        }
        Ok(())
    }

    fn destroy(self, device: &ash::Device) {
        unsafe {
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_pipeline_layout(self.pipeline_layout, None);
            device.destroy_sampler(self.sampler, None);
            device.destroy_descriptor_pool(self.descriptor_pool, None);
            device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            for framebuffer in self.framebuffers {
                device.destroy_framebuffer(framebuffer, None);
            }
            device.destroy_render_pass(self.render_pass, None);
        }
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_video_sampler_binding(binding: u32) -> vk::DescriptorSetLayoutBinding<'static> {
    vk::DescriptorSetLayoutBinding::default()
        .binding(binding)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .descriptor_count(1)
        .stage_flags(vk::ShaderStageFlags::FRAGMENT)
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_create_video_render_pass(
    device: &ash::Device,
    swapchain_format: vk::Format,
    final_layout: vk::ImageLayout,
) -> Result<vk::RenderPass, NativeVulkanError> {
    let color_attachment = vk::AttachmentDescription::default()
        .format(swapchain_format)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
        .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
        .initial_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .final_layout(final_layout);
    let color_attachment_ref = vk::AttachmentReference::default()
        .attachment(0)
        .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
    let color_attachment_refs = [color_attachment_ref];
    let subpass = vk::SubpassDescription::default()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(&color_attachment_refs);
    let dependency = vk::SubpassDependency::default()
        .src_subpass(vk::SUBPASS_EXTERNAL)
        .dst_subpass(0)
        .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE);
    let attachments = [color_attachment];
    let subpasses = [subpass];
    let dependencies = [dependency];
    let render_pass_info = vk::RenderPassCreateInfo::default()
        .attachments(&attachments)
        .subpasses(&subpasses)
        .dependencies(&dependencies);
    unsafe { device.create_render_pass(&render_pass_info, None) }.map_err(|result| {
        NativeVulkanError::Vulkan {
            operation: "vkCreateRenderPass(video)",
            result,
        }
    })
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_create_video_framebuffers(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    extent: vk::Extent2D,
    swapchain_image_views: &[vk::ImageView],
) -> Result<Vec<vk::Framebuffer>, NativeVulkanError> {
    let mut framebuffers = Vec::with_capacity(swapchain_image_views.len());
    for view in swapchain_image_views {
        let attachments = [*view];
        let info = vk::FramebufferCreateInfo::default()
            .render_pass(render_pass)
            .attachments(&attachments)
            .width(extent.width)
            .height(extent.height)
            .layers(1);
        let framebuffer = match unsafe { device.create_framebuffer(&info, None) } {
            Ok(framebuffer) => framebuffer,
            Err(result) => {
                for framebuffer in framebuffers {
                    unsafe {
                        device.destroy_framebuffer(framebuffer, None);
                    }
                }
                return Err(NativeVulkanError::Vulkan {
                    operation: "vkCreateFramebuffer(video)",
                    result,
                });
            }
        };
        framebuffers.push(framebuffer);
    }
    Ok(framebuffers)
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_create_video_pipeline(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    pipeline_layout: vk::PipelineLayout,
    extent: vk::Extent2D,
) -> Result<vk::Pipeline, NativeVulkanError> {
    let vertex_module = native_vulkan_create_shader_module(
        device,
        &NATIVE_VULKAN_VIDEO_VERTEX_SPIRV,
        "video vertex",
    )?;
    let fragment_module = match native_vulkan_create_shader_module(
        device,
        &NATIVE_VULKAN_VIDEO_FRAGMENT_SPIRV,
        "video fragment",
    ) {
        Ok(module) => module,
        Err(err) => {
            unsafe {
                device.destroy_shader_module(vertex_module, None);
            }
            return Err(err);
        }
    };
    let entry = c"main";
    let stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vertex_module)
            .name(entry),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(fragment_module)
            .name(entry),
    ];
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let viewport = vk::Viewport {
        x: 0.0,
        y: 0.0,
        width: extent.width as f32,
        height: extent.height as f32,
        min_depth: 0.0,
        max_depth: 1.0,
    };
    let scissor = vk::Rect2D {
        offset: vk::Offset2D { x: 0, y: 0 },
        extent,
    };
    let viewports = [viewport];
    let scissors = [scissor];
    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewports(&viewports)
        .scissors(&scissors);
    let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0);
    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let color_attachment = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(
            vk::ColorComponentFlags::R
                | vk::ColorComponentFlags::G
                | vk::ColorComponentFlags::B
                | vk::ColorComponentFlags::A,
        )
        .blend_enable(false);
    let color_attachments = [color_attachment];
    let color_blend =
        vk::PipelineColorBlendStateCreateInfo::default().attachments(&color_attachments);
    let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterization)
        .multisample_state(&multisample)
        .color_blend_state(&color_blend)
        .layout(pipeline_layout)
        .render_pass(render_pass)
        .subpass(0);
    let pipelines = unsafe {
        device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
    };
    unsafe {
        device.destroy_shader_module(fragment_module, None);
        device.destroy_shader_module(vertex_module, None);
    }
    pipelines
        .map(|pipelines| pipelines[0])
        .map_err(|(_, result)| NativeVulkanError::Vulkan {
            operation: "vkCreateGraphicsPipelines(video)",
            result,
        })
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_create_shader_module(
    device: &ash::Device,
    code: &[u32],
    label: &'static str,
) -> Result<vk::ShaderModule, NativeVulkanError> {
    let info = vk::ShaderModuleCreateInfo::default().code(code);
    unsafe { device.create_shader_module(&info, None) }.map_err(|result| {
        NativeVulkanError::Vulkan {
            operation: match label {
                "video vertex" => "vkCreateShaderModule(video vertex)",
                "video fragment" => "vkCreateShaderModule(video fragment)",
                _ => "vkCreateShaderModule(video)",
            },
            result,
        }
    })
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_video_fit_push_constants(
    fit: FitMode,
    source_size: (u32, u32),
    surface_size: (u32, u32),
) -> [f32; 4] {
    let (offset, scale) = native_vulkan_video_uv_transform(fit, source_size, surface_size);
    [offset[0], offset[1], scale[0], scale[1]]
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_video_uv_transform(
    fit: FitMode,
    source_size: (u32, u32),
    surface_size: (u32, u32),
) -> ([f32; 2], [f32; 2]) {
    if matches!(fit, FitMode::Stretch | FitMode::Contain | FitMode::Center) {
        return ([0.0, 0.0], [1.0, 1.0]);
    }
    let source_aspect = source_size.0.max(1) as f32 / source_size.1.max(1) as f32;
    let surface_aspect = surface_size.0.max(1) as f32 / surface_size.1.max(1) as f32;
    if source_aspect > surface_aspect {
        let width = (surface_aspect / source_aspect).clamp(0.0, 1.0);
        ([(1.0 - width) * 0.5, 0.0], [width, 1.0])
    } else {
        let height = (source_aspect / surface_aspect).clamp(0.0, 1.0);
        ([0.0, (1.0 - height) * 0.5], [1.0, height])
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
enum NativeVulkanVideoTexture {
    Cuda(NativeVulkanCudaVideoTexture),
    Dmabuf(NativeVulkanDmabufVideoTexture),
    Decoded(NativeVulkanDecodedVideoTexture),
}

#[cfg(feature = "native-vulkan-gst-video")]
impl NativeVulkanVideoTexture {
    fn width(&self) -> u32 {
        match self {
            Self::Cuda(texture) => texture.width,
            Self::Dmabuf(texture) => texture.width,
            Self::Decoded(texture) => texture.width,
        }
    }

    fn height(&self) -> u32 {
        match self {
            Self::Cuda(texture) => texture.height,
            Self::Dmabuf(texture) => texture.height,
            Self::Decoded(texture) => texture.height,
        }
    }

    fn y_view(&self) -> vk::ImageView {
        match self {
            Self::Cuda(texture) => texture.y.view,
            Self::Dmabuf(texture) => texture.y_view,
            Self::Decoded(texture) => texture.y_view,
        }
    }

    fn uv_view(&self) -> vk::ImageView {
        match self {
            Self::Cuda(texture) => texture.uv.view,
            Self::Dmabuf(texture) => texture.uv_view,
            Self::Decoded(texture) => texture.uv_view,
        }
    }

    fn image_layout(&self) -> vk::ImageLayout {
        match self {
            Self::Cuda(_) | Self::Dmabuf(_) => vk::ImageLayout::GENERAL,
            Self::Decoded(texture) => texture.shader_layout,
        }
    }

    fn shader_read_barriers(&self) -> [vk::ImageMemoryBarrier<'static>; 2] {
        match self {
            Self::Cuda(texture) => texture.shader_read_barriers(),
            Self::Dmabuf(texture) => texture.shader_read_barriers(),
            Self::Decoded(texture) => texture.shader_read_barriers(),
        }
    }

    fn destroy(self, device: &ash::Device) {
        match self {
            Self::Cuda(texture) => texture.destroy(device),
            Self::Dmabuf(texture) => texture.destroy(device),
            Self::Decoded(texture) => texture.destroy(device),
        }
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
struct NativeVulkanDecodedVideoTexture {
    image: vk::Image,
    width: u32,
    height: u32,
    y_view: vk::ImageView,
    uv_view: vk::ImageView,
    source_layout: vk::ImageLayout,
    shader_layout: vk::ImageLayout,
    source_access: vk::AccessFlags,
}

#[cfg(feature = "native-vulkan-gst-video")]
impl NativeVulkanDecodedVideoTexture {
    fn new(
        device: &ash::Device,
        image: vk::Image,
        width: u32,
        height: u32,
        source_layout: vk::ImageLayout,
        source_access: vk::AccessFlags,
    ) -> Result<Self, NativeVulkanError> {
        if width == 0 || height == 0 || !width.is_multiple_of(2) || !height.is_multiple_of(2) {
            return Err(NativeVulkanError::Video(format!(
                "decoded NV12 texture requires non-zero even dimensions, got {width}x{height}"
            )));
        }
        let y_view = native_vulkan_create_decoded_video_plane_view(
            device,
            image,
            vk::ImageAspectFlags::PLANE_0,
            vk::Format::R8_UNORM,
            "y",
        )?;
        let uv_view = match native_vulkan_create_decoded_video_plane_view(
            device,
            image,
            vk::ImageAspectFlags::PLANE_1,
            vk::Format::R8G8_UNORM,
            "uv",
        ) {
            Ok(view) => view,
            Err(err) => {
                unsafe {
                    device.destroy_image_view(y_view, None);
                }
                return Err(err);
            }
        };
        Ok(Self {
            image,
            width,
            height,
            y_view,
            uv_view,
            source_layout,
            shader_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            source_access,
        })
    }

    fn shader_read_barriers(&self) -> [vk::ImageMemoryBarrier<'static>; 2] {
        [
            self.shader_read_barrier(vk::ImageAspectFlags::PLANE_0),
            self.shader_read_barrier(vk::ImageAspectFlags::PLANE_1),
        ]
    }

    fn shader_read_barrier(
        &self,
        aspect_mask: vk::ImageAspectFlags,
    ) -> vk::ImageMemoryBarrier<'static> {
        vk::ImageMemoryBarrier::default()
            .old_layout(self.source_layout)
            .new_layout(self.shader_layout)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(self.image)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            })
            .src_access_mask(self.source_access)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
    }

    fn destroy(self, device: &ash::Device) {
        unsafe {
            device.destroy_image_view(self.uv_view, None);
            device.destroy_image_view(self.y_view, None);
        }
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_create_decoded_video_plane_view(
    device: &ash::Device,
    image: vk::Image,
    aspect_mask: vk::ImageAspectFlags,
    format: vk::Format,
    label: &'static str,
) -> Result<vk::ImageView, NativeVulkanError> {
    let mut view_usage_info =
        vk::ImageViewUsageCreateInfo::default().usage(vk::ImageUsageFlags::SAMPLED);
    let view_info = vk::ImageViewCreateInfo::default()
        .image(image)
        .view_type(vk::ImageViewType::TYPE_2D)
        .format(format)
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        })
        .push_next(&mut view_usage_info);
    unsafe { device.create_image_view(&view_info, None) }.map_err(|result| {
        NativeVulkanError::Vulkan {
            operation: match label {
                "y" => "vkCreateImageView(decoded video y plane)",
                "uv" => "vkCreateImageView(decoded video uv plane)",
                _ => "vkCreateImageView(decoded video plane)",
            },
            result,
        }
    })
}

#[cfg(feature = "native-vulkan-gst-video")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeVulkanDmabufVideoPlane {
    fd: i32,
    offset: u64,
    stride: u32,
    height: u32,
}

#[cfg(feature = "native-vulkan-gst-video")]
#[derive(Debug)]
struct NativeVulkanDmabufVideoFrame {
    width: u32,
    height: u32,
    fd: i32,
    modifier: u64,
    y: NativeVulkanDmabufVideoPlane,
    uv: NativeVulkanDmabufVideoPlane,
    _owned_fds: Vec<OwnedFd>,
}

#[cfg(feature = "native-vulkan-gst-video")]
struct NativeVulkanDmabufVideoTexture {
    width: u32,
    height: u32,
    image: vk::Image,
    memory: vk::DeviceMemory,
    y_view: vk::ImageView,
    uv_view: vk::ImageView,
}

#[cfg(feature = "native-vulkan-gst-video")]
impl NativeVulkanDmabufVideoTexture {
    #[allow(clippy::too_many_arguments)]
    fn new(
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        _queue: vk::Queue,
        _command_pool: vk::CommandPool,
        device: &ash::Device,
        _queue_family_index: u32,
        frame: &NativeVulkanDmabufVideoFrame,
    ) -> Result<Self, NativeVulkanError> {
        if frame.width == 0 || frame.height == 0 {
            return Err(NativeVulkanError::Video(
                "native Vulkan DMABuf video frame has zero dimension".to_owned(),
            ));
        }
        if frame.width % 2 != 0 || frame.height % 2 != 0 {
            return Err(NativeVulkanError::Video(format!(
                "native Vulkan DMABuf video dimensions must be even, got {}x{}",
                frame.width, frame.height
            )));
        }
        if frame.fd < 0 || frame.y.fd != frame.fd || frame.uv.fd != frame.fd {
            return Err(NativeVulkanError::Video(
                "native Vulkan DMABuf importer currently requires a single fd NV12 frame"
                    .to_owned(),
            ));
        }

        let plane_layouts = [
            native_vulkan_dmabuf_plane_layout(frame.y),
            native_vulkan_dmabuf_plane_layout(frame.uv),
        ];
        let handle_type = vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT;
        let mut external_image_info =
            vk::ExternalMemoryImageCreateInfo::default().handle_types(handle_type);
        let mut drm_modifier_info = vk::ImageDrmFormatModifierExplicitCreateInfoEXT::default()
            .drm_format_modifier(frame.modifier)
            .plane_layouts(&plane_layouts);
        let image_info = vk::ImageCreateInfo::default()
            .flags(vk::ImageCreateFlags::MUTABLE_FORMAT)
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::G8_B8R8_2PLANE_420_UNORM)
            .extent(vk::Extent3D {
                width: frame.width,
                height: frame.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
            .usage(vk::ImageUsageFlags::SAMPLED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .push_next(&mut external_image_info)
            .push_next(&mut drm_modifier_info);
        let image = unsafe { device.create_image(&image_info, None) }.map_err(|result| {
            NativeVulkanError::Vulkan {
                operation: "vkCreateImage(video DMABuf image)",
                result,
            }
        })?;

        let requirements = unsafe { device.get_image_memory_requirements(image) };
        let external_memory_fd = ash::khr::external_memory_fd::Device::new(instance, device);
        let mut fd_properties = vk::MemoryFdPropertiesKHR::default();
        unsafe {
            external_memory_fd
                .get_memory_fd_properties(handle_type, frame.fd, &mut fd_properties)
                .map_err(|result| {
                    device.destroy_image(image, None);
                    NativeVulkanError::Vulkan {
                        operation: "vkGetMemoryFdPropertiesKHR(video DMABuf)",
                        result,
                    }
                })?;
        }
        let memory_type_bits = requirements.memory_type_bits & fd_properties.memory_type_bits;
        if memory_type_bits == 0 {
            unsafe {
                device.destroy_image(image, None);
            }
            return Err(NativeVulkanError::Video(
                "native Vulkan DMABuf import has zero compatible memory_type_bits".to_owned(),
            ));
        }
        let memory_properties =
            unsafe { instance.get_physical_device_memory_properties(physical_device) };
        let memory_type_index = native_vulkan_memory_type_index_prefer(
            &memory_properties,
            memory_type_bits,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
            vk::MemoryPropertyFlags::empty(),
        )
        .ok_or_else(|| {
            unsafe {
                device.destroy_image(image, None);
            }
            NativeVulkanError::MissingMemoryType("video DMABuf image")
        })?;

        let duplicated_fd = native_vulkan_dup_fd(frame.fd).map_err(|err| {
            unsafe {
                device.destroy_image(image, None);
            }
            err
        })?;
        let mut import_info = vk::ImportMemoryFdInfoKHR::default()
            .handle_type(handle_type)
            .fd(duplicated_fd.as_raw_fd());
        let mut dedicated_info = vk::MemoryDedicatedAllocateInfo::default().image(image);
        let allocate_info = vk::MemoryAllocateInfo::default()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type_index)
            .push_next(&mut dedicated_info)
            .push_next(&mut import_info);
        let memory = match unsafe { device.allocate_memory(&allocate_info, None) } {
            Ok(memory) => {
                let _ = duplicated_fd.into_raw_fd();
                memory
            }
            Err(result) => {
                unsafe {
                    device.destroy_image(image, None);
                }
                return Err(NativeVulkanError::Vulkan {
                    operation: "vkAllocateMemory(video DMABuf image)",
                    result,
                });
            }
        };
        if let Err(result) = unsafe { device.bind_image_memory(image, memory, 0) } {
            unsafe {
                device.free_memory(memory, None);
                device.destroy_image(image, None);
            }
            return Err(NativeVulkanError::Vulkan {
                operation: "vkBindImageMemory(video DMABuf image)",
                result,
            });
        }

        let y_view = match native_vulkan_create_dmabuf_plane_view(
            device,
            image,
            vk::ImageAspectFlags::PLANE_0,
            vk::Format::R8_UNORM,
            "y",
        ) {
            Ok(view) => view,
            Err(err) => {
                unsafe {
                    device.free_memory(memory, None);
                    device.destroy_image(image, None);
                }
                return Err(err);
            }
        };
        let uv_view = match native_vulkan_create_dmabuf_plane_view(
            device,
            image,
            vk::ImageAspectFlags::PLANE_1,
            vk::Format::R8G8_UNORM,
            "uv",
        ) {
            Ok(view) => view,
            Err(err) => {
                unsafe {
                    device.destroy_image_view(y_view, None);
                    device.free_memory(memory, None);
                    device.destroy_image(image, None);
                }
                return Err(err);
            }
        };

        Ok(Self {
            width: frame.width,
            height: frame.height,
            image,
            memory,
            y_view,
            uv_view,
        })
    }

    fn shader_read_barriers(&self) -> [vk::ImageMemoryBarrier<'static>; 2] {
        [
            native_vulkan_dmabuf_shader_read_barrier(self.image, vk::ImageAspectFlags::PLANE_0),
            native_vulkan_dmabuf_shader_read_barrier(self.image, vk::ImageAspectFlags::PLANE_1),
        ]
    }

    fn destroy(self, device: &ash::Device) {
        unsafe {
            device.destroy_image_view(self.uv_view, None);
            device.destroy_image_view(self.y_view, None);
            device.free_memory(self.memory, None);
            device.destroy_image(self.image, None);
        }
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_dmabuf_plane_layout(plane: NativeVulkanDmabufVideoPlane) -> vk::SubresourceLayout {
    vk::SubresourceLayout {
        offset: plane.offset,
        size: u64::from(plane.stride) * u64::from(plane.height),
        row_pitch: u64::from(plane.stride),
        array_pitch: 0,
        depth_pitch: 0,
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_dmabuf_shader_read_barrier(
    image: vk::Image,
    aspect_mask: vk::ImageAspectFlags,
) -> vk::ImageMemoryBarrier<'static> {
    vk::ImageMemoryBarrier::default()
        .old_layout(vk::ImageLayout::GENERAL)
        .new_layout(vk::ImageLayout::GENERAL)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        })
        .src_access_mask(vk::AccessFlags::MEMORY_WRITE)
        .dst_access_mask(vk::AccessFlags::SHADER_READ)
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_create_dmabuf_plane_view(
    device: &ash::Device,
    image: vk::Image,
    aspect_mask: vk::ImageAspectFlags,
    format: vk::Format,
    label: &'static str,
) -> Result<vk::ImageView, NativeVulkanError> {
    let view_info = vk::ImageViewCreateInfo::default()
        .image(image)
        .view_type(vk::ImageViewType::TYPE_2D)
        .format(format)
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        });
    unsafe { device.create_image_view(&view_info, None) }.map_err(|result| {
        NativeVulkanError::Vulkan {
            operation: match label {
                "y" => "vkCreateImageView(video DMABuf y plane)",
                "uv" => "vkCreateImageView(video DMABuf uv plane)",
                _ => "vkCreateImageView(video DMABuf plane)",
            },
            result,
        }
    })
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_dup_fd(fd: i32) -> Result<OwnedFd, NativeVulkanError> {
    let duplicated = unsafe { libc::dup(fd) };
    if duplicated < 0 {
        return Err(NativeVulkanError::Video(format!(
            "native Vulkan DMABuf dup fd failed: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(unsafe { OwnedFd::from_raw_fd(duplicated) })
}

#[cfg(feature = "native-vulkan-gst-video")]
struct NativeVulkanCudaVideoTexture {
    cuda_context: *mut NativeVulkanGstCudaContext,
    width: u32,
    height: u32,
    cuda_stream: NativeVulkanCudaStream,
    y: NativeVulkanCudaVideoPlane,
    uv: NativeVulkanCudaVideoPlane,
}

#[cfg(feature = "native-vulkan-gst-video")]
impl NativeVulkanCudaVideoTexture {
    fn new(
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        device: &ash::Device,
        queue_family_index: u32,
        cuda_context: *mut NativeVulkanGstCudaContext,
        width: u32,
        height: u32,
    ) -> Result<Self, NativeVulkanError> {
        if width == 0 || height == 0 {
            return Err(NativeVulkanError::Video(
                "native Vulkan CUDA video frame has zero dimension".to_owned(),
            ));
        }
        if width % 2 != 0 || height % 2 != 0 {
            return Err(NativeVulkanError::Video(format!(
                "native Vulkan CUDA video dimensions must be even, got {width}x{height}"
            )));
        }
        if cuda_context.is_null() {
            return Err(NativeVulkanError::Video(
                "native Vulkan CUDA video sample has null GstCudaContext".to_owned(),
            ));
        }
        let _guard = NativeVulkanGstCudaContextPushGuard::new(cuda_context)?;
        let cuda_stream = NativeVulkanCudaStream::new()?;
        let y = NativeVulkanCudaVideoPlane::new(
            instance,
            physical_device,
            queue,
            command_pool,
            device,
            queue_family_index,
            width,
            height,
            vk::Format::R8_UNORM,
            1,
            "y",
        )?;
        let uv = match NativeVulkanCudaVideoPlane::new(
            instance,
            physical_device,
            queue,
            command_pool,
            device,
            queue_family_index,
            width / 2,
            height / 2,
            vk::Format::R8G8_UNORM,
            2,
            "uv",
        ) {
            Ok(plane) => plane,
            Err(err) => {
                y.destroy(device);
                return Err(err);
            }
        };
        Ok(Self {
            cuda_context,
            width,
            height,
            cuda_stream,
            y,
            uv,
        })
    }

    fn matches(
        &self,
        cuda_context: *mut NativeVulkanGstCudaContext,
        width: u32,
        height: u32,
    ) -> bool {
        self.cuda_context == cuda_context && self.width == width && self.height == height
    }

    fn copy_sample(
        &mut self,
        buffer: &gst::BufferRef,
        meta: &NativeVulkanGstSystemNv12Meta,
    ) -> Result<(), NativeVulkanError> {
        let _guard = NativeVulkanGstCudaContextPushGuard::new(self.cuda_context)?;
        let y_map = native_vulkan_copy_gst_cuda_plane_to_vulkan_image(
            buffer,
            0,
            meta.y.offset,
            meta.y.stride,
            meta.y.row_bytes,
            meta.y.height,
            self.cuda_context,
            self.cuda_stream.handle,
            &self.y,
            "y",
        )?;
        let uv_map = match native_vulkan_copy_gst_cuda_plane_to_vulkan_image(
            buffer,
            1,
            meta.uv.offset,
            meta.uv.stride,
            meta.uv.row_bytes,
            meta.uv.height,
            self.cuda_context,
            self.cuda_stream.handle,
            &self.uv,
            "uv",
        ) {
            Ok(map) => map,
            Err(err) => {
                let sync_result = native_vulkan_cuda_result(
                    unsafe { CuStreamSynchronize(self.cuda_stream.handle) },
                    "native Vulkan CUDA synchronize after failed uv copy",
                );
                drop(y_map);
                sync_result?;
                return Err(err);
            }
        };
        let sync_result = native_vulkan_cuda_result(
            unsafe { CuStreamSynchronize(self.cuda_stream.handle) },
            "native Vulkan CUDA synchronize copy stream",
        );
        drop(uv_map);
        drop(y_map);
        sync_result?;
        Ok(())
    }

    fn shader_read_barriers(&self) -> [vk::ImageMemoryBarrier<'static>; 2] {
        [self.y.shader_read_barrier(), self.uv.shader_read_barrier()]
    }

    fn destroy(self, device: &ash::Device) {
        let _ = unsafe { CuStreamSynchronize(self.cuda_stream.handle) };
        self.uv.destroy(device);
        self.y.destroy(device);
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
struct NativeVulkanCudaVideoPlane {
    cuda_external_memory: NativeVulkanCudaExternalImageMemory,
    image: vk::Image,
    memory: vk::DeviceMemory,
    view: vk::ImageView,
    width: u32,
    height: u32,
    channels: u32,
}

#[cfg(feature = "native-vulkan-gst-video")]
impl NativeVulkanCudaVideoPlane {
    #[allow(clippy::too_many_arguments)]
    fn new(
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        device: &ash::Device,
        queue_family_index: u32,
        width: u32,
        height: u32,
        format: vk::Format,
        channels: u32,
        label: &'static str,
    ) -> Result<Self, NativeVulkanError> {
        let handle_type = vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD;
        let mut external_image_info =
            vk::ExternalMemoryImageCreateInfo::default().handle_types(handle_type);
        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::SAMPLED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .push_next(&mut external_image_info);
        let image = unsafe { device.create_image(&image_info, None) }.map_err(|result| {
            NativeVulkanError::Vulkan {
                operation: "vkCreateImage(video CUDA plane)",
                result,
            }
        })?;
        let requirements = unsafe { device.get_image_memory_requirements(image) };
        let memory_properties =
            unsafe { instance.get_physical_device_memory_properties(physical_device) };
        let memory_type_index = native_vulkan_memory_type_index_prefer(
            &memory_properties,
            requirements.memory_type_bits,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
            vk::MemoryPropertyFlags::empty(),
        )
        .ok_or_else(|| {
            unsafe {
                device.destroy_image(image, None);
            }
            NativeVulkanError::MissingMemoryType("video CUDA external image")
        })?;
        let mut export_info = vk::ExportMemoryAllocateInfo::default().handle_types(handle_type);
        let mut dedicated_info = vk::MemoryDedicatedAllocateInfo::default().image(image);
        let allocate_info = vk::MemoryAllocateInfo::default()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type_index)
            .push_next(&mut dedicated_info)
            .push_next(&mut export_info);
        let memory = match unsafe { device.allocate_memory(&allocate_info, None) } {
            Ok(memory) => memory,
            Err(result) => {
                unsafe {
                    device.destroy_image(image, None);
                }
                return Err(NativeVulkanError::Vulkan {
                    operation: "vkAllocateMemory(video CUDA plane)",
                    result,
                });
            }
        };
        if let Err(result) = unsafe { device.bind_image_memory(image, memory, 0) } {
            unsafe {
                device.free_memory(memory, None);
                device.destroy_image(image, None);
            }
            return Err(NativeVulkanError::Vulkan {
                operation: "vkBindImageMemory(video CUDA plane)",
                result,
            });
        }
        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(native_vulkan_color_subresource_range());
        let view = match unsafe { device.create_image_view(&view_info, None) } {
            Ok(view) => view,
            Err(result) => {
                unsafe {
                    device.free_memory(memory, None);
                    device.destroy_image(image, None);
                }
                return Err(NativeVulkanError::Vulkan {
                    operation: "vkCreateImageView(video CUDA plane)",
                    result,
                });
            }
        };
        let external_memory_fd = ash::khr::external_memory_fd::Device::new(instance, device);
        let fd_info = vk::MemoryGetFdInfoKHR::default()
            .memory(memory)
            .handle_type(handle_type);
        let fd = match unsafe { external_memory_fd.get_memory_fd(&fd_info) } {
            Ok(fd) => fd,
            Err(result) => {
                unsafe {
                    device.destroy_image_view(view, None);
                    device.free_memory(memory, None);
                    device.destroy_image(image, None);
                }
                return Err(NativeVulkanError::Vulkan {
                    operation: "vkGetMemoryFdKHR(video CUDA plane)",
                    result,
                });
            }
        };
        let cuda_external_memory = match NativeVulkanCudaExternalImageMemory::import_opaque_fd(
            fd,
            requirements.size,
            width,
            height,
            channels,
            label,
        ) {
            Ok(memory) => memory,
            Err(err) => {
                unsafe {
                    device.destroy_image_view(view, None);
                    device.free_memory(memory, None);
                    device.destroy_image(image, None);
                }
                return Err(err);
            }
        };
        if let Err(err) = native_vulkan_transition_image_to_general(
            device,
            queue,
            command_pool,
            image,
            queue_family_index,
        ) {
            unsafe {
                device.destroy_image_view(view, None);
                device.free_memory(memory, None);
                device.destroy_image(image, None);
            }
            drop(cuda_external_memory);
            return Err(err);
        }

        Ok(Self {
            cuda_external_memory,
            image,
            memory,
            view,
            width,
            height,
            channels,
        })
    }

    fn shader_read_barrier(&self) -> vk::ImageMemoryBarrier<'static> {
        vk::ImageMemoryBarrier::default()
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(self.image)
            .subresource_range(native_vulkan_color_subresource_range())
            .src_access_mask(vk::AccessFlags::MEMORY_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
    }

    fn destroy(self, device: &ash::Device) {
        drop(self.cuda_external_memory);
        unsafe {
            device.destroy_image_view(self.view, None);
            device.free_memory(self.memory, None);
            device.destroy_image(self.image, None);
        }
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_transition_image_to_general(
    device: &ash::Device,
    queue: vk::Queue,
    command_pool: vk::CommandPool,
    image: vk::Image,
    queue_family_index: u32,
) -> Result<(), NativeVulkanError> {
    let allocate_info = vk::CommandBufferAllocateInfo::default()
        .command_pool(command_pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(1);
    let command_buffer =
        unsafe { device.allocate_command_buffers(&allocate_info) }.map_err(|result| {
            NativeVulkanError::Vulkan {
                operation: "vkAllocateCommandBuffers(video image transition)",
                result,
            }
        })?[0];
    let result = unsafe {
        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        device
            .begin_command_buffer(command_buffer, &begin_info)
            .map_err(|result| NativeVulkanError::Vulkan {
                operation: "vkBeginCommandBuffer(video image transition)",
                result,
            })?;
        let barrier = vk::ImageMemoryBarrier::default()
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::GENERAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(native_vulkan_color_subresource_range())
            .src_access_mask(vk::AccessFlags::empty())
            .dst_access_mask(vk::AccessFlags::SHADER_READ);
        device.cmd_pipeline_barrier(
            command_buffer,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[barrier],
        );
        device
            .end_command_buffer(command_buffer)
            .map_err(|result| NativeVulkanError::Vulkan {
                operation: "vkEndCommandBuffer(video image transition)",
                result,
            })?;
        let command_buffers = [command_buffer];
        let submit_info = vk::SubmitInfo::default().command_buffers(&command_buffers);
        device
            .queue_submit(queue, &[submit_info], vk::Fence::null())
            .map_err(|result| NativeVulkanError::Vulkan {
                operation: "vkQueueSubmit(video image transition)",
                result,
            })?;
        device
            .queue_wait_idle(queue)
            .map_err(|result| NativeVulkanError::Vulkan {
                operation: "vkQueueWaitIdle(video image transition)",
                result,
            })
    };
    unsafe {
        device.free_command_buffers(command_pool, &[command_buffer]);
    }
    let _ = queue_family_index;
    result
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_memory_type_index_prefer(
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    memory_type_bits: u32,
    preferred: vk::MemoryPropertyFlags,
    required: vk::MemoryPropertyFlags,
) -> Option<u32> {
    let mut fallback = None;
    for (index, memory_type) in memory_properties.memory_types
        [..memory_properties.memory_type_count as usize]
        .iter()
        .enumerate()
    {
        let supported = (memory_type_bits & (1 << index)) != 0;
        if !supported || !memory_type.property_flags.contains(required) {
            continue;
        }
        let index = index as u32;
        if memory_type.property_flags.contains(preferred) {
            return Some(index);
        }
        fallback.get_or_insert(index);
    }
    fallback
}

#[cfg(feature = "native-vulkan-gst-video")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeVulkanGstSystemNv12Plane {
    offset: usize,
    stride: u32,
    height: u32,
    row_bytes: u32,
}

#[cfg(feature = "native-vulkan-gst-video")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeVulkanGstSystemNv12Meta {
    width: u32,
    height: u32,
    y: NativeVulkanGstSystemNv12Plane,
    uv: NativeVulkanGstSystemNv12Plane,
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_system_nv12_meta(
    sample: &gst::Sample,
    buffer: &gst::BufferRef,
) -> Result<NativeVulkanGstSystemNv12Meta, NativeVulkanError> {
    let meta = match native_vulkan_gst_nv12_meta_from_video_meta(sample.caps(), buffer) {
        Ok(meta) => meta,
        Err(meta_err) => native_vulkan_gst_nv12_meta_from_caps(sample)
            .map_err(|caps_err| NativeVulkanError::Video(format!("{meta_err};{caps_err}")))?,
    };
    Ok(meta)
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_nv12_meta_from_video_meta(
    caps: Option<&gst::CapsRef>,
    buffer: &gst::BufferRef,
) -> Result<NativeVulkanGstSystemNv12Meta, String> {
    let meta = buffer
        .meta::<gst_video::VideoMeta>()
        .ok_or_else(|| "appsink buffer has no GstVideoMeta".to_owned())?;
    let caps_format = caps
        .and_then(|caps| caps.structure(0))
        .and_then(|structure| structure.get::<String>("format").ok())
        .unwrap_or_else(|| meta.format().to_str().to_string());
    if meta.format() != gst_video::VideoFormat::Nv12 && caps_format != "NV12" {
        return Err(format!("expected NV12 appsink frame, got {caps_format}"));
    }
    let width = meta.width();
    let height = meta.height();
    if width == 0 || height == 0 {
        return Err("NV12 frame has zero dimension".to_owned());
    }
    if width % 2 != 0 || height % 2 != 0 {
        return Err(format!(
            "NV12 frame dimensions must be even, got {width}x{height}"
        ));
    }
    if meta.offset().len() < 2 || meta.stride().len() < 2 {
        return Err(format!(
            "NV12 frame needs 2 planes, got offsets={} strides={}",
            meta.offset().len(),
            meta.stride().len()
        ));
    }
    let y_stride = native_vulkan_positive_stride("NV12 y", meta.stride()[0])?;
    let uv_stride = native_vulkan_positive_stride("NV12 uv", meta.stride()[1])?;
    if y_stride < width || uv_stride < width {
        return Err(format!(
            "NV12 stride too small: y={y_stride} uv={uv_stride} width={width}"
        ));
    }
    Ok(NativeVulkanGstSystemNv12Meta {
        width,
        height,
        y: NativeVulkanGstSystemNv12Plane {
            offset: meta.offset()[0],
            stride: y_stride,
            height,
            row_bytes: width,
        },
        uv: NativeVulkanGstSystemNv12Plane {
            offset: meta.offset()[1],
            stride: uv_stride,
            height: height / 2,
            row_bytes: width,
        },
    })
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_nv12_meta_from_caps(
    sample: &gst::Sample,
) -> Result<NativeVulkanGstSystemNv12Meta, String> {
    let caps = sample
        .caps()
        .ok_or_else(|| "appsink sample has no caps".to_owned())?;
    let structure = caps
        .structure(0)
        .ok_or_else(|| "appsink caps has no structure".to_owned())?;
    let format = structure
        .get::<String>("format")
        .unwrap_or_else(|_| "unknown".to_owned());
    if format != "NV12" {
        return Err(format!("caps fallback expected NV12, got {format}"));
    }
    let width = structure
        .get::<i32>("width")
        .map_err(|_| "appsink caps missing width".to_owned())
        .and_then(|width| {
            u32::try_from(width)
                .ok()
                .filter(|width| *width > 0)
                .ok_or_else(|| "invalid appsink frame width".to_owned())
        })?;
    let height = structure
        .get::<i32>("height")
        .map_err(|_| "appsink caps missing height".to_owned())
        .and_then(|height| {
            u32::try_from(height)
                .ok()
                .filter(|height| *height > 0)
                .ok_or_else(|| "invalid appsink frame height".to_owned())
        })?;
    if width % 2 != 0 || height % 2 != 0 {
        return Err(format!(
            "NV12 frame dimensions must be even, got {width}x{height}"
        ));
    }
    let y_size = usize::try_from(u64::from(width) * u64::from(height))
        .map_err(|_| "NV12 plane offset overflow".to_owned())?;
    Ok(NativeVulkanGstSystemNv12Meta {
        width,
        height,
        y: NativeVulkanGstSystemNv12Plane {
            offset: 0,
            stride: width,
            height,
            row_bytes: width,
        },
        uv: NativeVulkanGstSystemNv12Plane {
            offset: y_size,
            stride: width,
            height: height / 2,
            row_bytes: width,
        },
    })
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_positive_stride(label: &str, stride: i32) -> Result<u32, String> {
    u32::try_from(stride)
        .ok()
        .filter(|stride| *stride > 0)
        .ok_or_else(|| format!("{label} stride must be positive, got {stride}"))
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_buffer_has_cuda_memory(buffer: &gst::BufferRef) -> bool {
    (0..buffer.n_memory()).any(|index| native_vulkan_is_cuda_memory(buffer.peek_memory(index)))
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_cuda_context_from_buffer(
    buffer: &gst::BufferRef,
) -> Result<*mut NativeVulkanGstCudaContext, NativeVulkanError> {
    for memory_index in 0..buffer.n_memory() {
        let memory = buffer.peek_memory(memory_index);
        if !native_vulkan_is_cuda_memory(memory) {
            continue;
        }
        let cuda_memory = memory
            .as_ptr()
            .cast_mut()
            .cast::<NativeVulkanGstCudaMemory>();
        let context = unsafe { (*cuda_memory).context };
        if !context.is_null() {
            return Ok(context);
        }
    }
    Err(NativeVulkanError::Video(
        "native Vulkan CUDA buffer has no GstCudaContext".to_owned(),
    ))
}

#[cfg(feature = "native-vulkan-gst-video")]
#[allow(clippy::too_many_arguments)]
fn native_vulkan_copy_gst_cuda_plane_to_vulkan_image(
    buffer: &gst::BufferRef,
    plane_index: usize,
    plane_offset: usize,
    source_stride: u32,
    row_bytes: u32,
    height: u32,
    expected_context: *mut NativeVulkanGstCudaContext,
    stream: NativeVulkanCudaStreamHandle,
    image: &NativeVulkanCudaVideoPlane,
    label: &str,
) -> Result<NativeVulkanCudaMemoryMap, NativeVulkanError> {
    let expected_row_bytes = image.width.checked_mul(image.channels).ok_or_else(|| {
        NativeVulkanError::Video(format!("native Vulkan CUDA {label} row byte overflow"))
    })?;
    if row_bytes != expected_row_bytes || height != image.height {
        return Err(NativeVulkanError::Video(format!(
            "native Vulkan CUDA {label} plane shape mismatch: row_bytes={row_bytes} height={height} image={}x{} channels={}",
            image.width, image.height, image.channels
        )));
    }
    let plane_end = plane_offset.checked_add(1).ok_or_else(|| {
        NativeVulkanError::Video(format!("native Vulkan CUDA {label} offset overflow"))
    })?;
    let (memory_range, memory_skip) =
        buffer.find_memory(plane_offset..plane_end).ok_or_else(|| {
            NativeVulkanError::Video(format!("native Vulkan CUDA {label} plane has no memory"))
        })?;
    let memory_index = memory_range.start;
    if memory_index >= buffer.n_memory() {
        return Err(NativeVulkanError::Video(format!(
            "native Vulkan CUDA {label} memory index out of range"
        )));
    }
    let memory = buffer.peek_memory(memory_index);
    if !native_vulkan_is_cuda_memory(memory) {
        return Err(NativeVulkanError::Video(format!(
            "native Vulkan CUDA {label} plane memory is not CUDAMemory: {}",
            native_vulkan_gst_memory_type(memory)
        )));
    }
    let cuda_memory = memory
        .as_ptr()
        .cast_mut()
        .cast::<NativeVulkanGstCudaMemory>();
    let context = unsafe { (*cuda_memory).context };
    if context != expected_context {
        return Err(NativeVulkanError::Video(format!(
            "native Vulkan CUDA {label} plane context changed"
        )));
    }
    unsafe {
        gst_cuda_memory_sync(cuda_memory);
    }
    let map = NativeVulkanCudaMemoryMap::new(memory).map_err(|err| {
        NativeVulkanError::Video(format!("native Vulkan CUDA {label} map failed: {err}"))
    })?;
    let source = map
        .device_ptr()
        .checked_add(u64::try_from(memory_skip).map_err(|_| {
            NativeVulkanError::Video(format!("native Vulkan CUDA {label} memory skip too large"))
        })?)
        .ok_or_else(|| {
            NativeVulkanError::Video(format!("native Vulkan CUDA {label} source overflow"))
        })?;
    let copy = NativeVulkanCudaMemcpy2D {
        src_x_in_bytes: 0,
        src_y: 0,
        src_memory_type: CUDA_MEMORYTYPE_DEVICE,
        src_host: ptr::null(),
        src_device: source,
        src_array: ptr::null_mut(),
        src_pitch: usize::try_from(source_stride).map_err(|_| {
            NativeVulkanError::Video(format!(
                "native Vulkan CUDA {label} source stride too large"
            ))
        })?,
        dst_x_in_bytes: 0,
        dst_y: 0,
        dst_memory_type: CUDA_MEMORYTYPE_ARRAY,
        dst_host: ptr::null_mut(),
        dst_device: 0,
        dst_array: image.cuda_external_memory.array,
        dst_pitch: 0,
        width_in_bytes: usize::try_from(row_bytes).map_err(|_| {
            NativeVulkanError::Video(format!("native Vulkan CUDA {label} row bytes too large"))
        })?,
        height: usize::try_from(height).map_err(|_| {
            NativeVulkanError::Video(format!("native Vulkan CUDA {label} height too large"))
        })?,
    };
    native_vulkan_cuda_result(
        unsafe { CuMemcpy2DAsync(&copy, stream) },
        &format!("native Vulkan CUDA copy {label} plane {plane_index}"),
    )?;
    Ok(map)
}

#[cfg(feature = "native-vulkan-gst-video")]
struct NativeVulkanCudaMemoryMap {
    memory: *mut gst::ffi::GstMemory,
    info: gst::ffi::GstMapInfo,
}

#[cfg(feature = "native-vulkan-gst-video")]
impl NativeVulkanCudaMemoryMap {
    fn new(memory: &gst::MemoryRef) -> Result<Self, String> {
        let memory_ptr = memory.as_ptr().cast_mut();
        let mut info = std::mem::MaybeUninit::<gst::ffi::GstMapInfo>::zeroed();
        let mapped =
            unsafe { gst::ffi::gst_memory_map(memory_ptr, info.as_mut_ptr(), GST_MAP_READ_CUDA) }
                != gst::glib::ffi::GFALSE;
        if !mapped {
            return Err(native_vulkan_gst_memory_type(memory));
        }
        let info = unsafe { info.assume_init() };
        if info.data.is_null() {
            unsafe {
                let mut info = info;
                gst::ffi::gst_memory_unmap(memory_ptr, &mut info);
            }
            return Err("null CUDA map pointer".to_owned());
        }
        Ok(Self {
            memory: memory_ptr,
            info,
        })
    }

    fn device_ptr(&self) -> NativeVulkanCudaDevicePtr {
        self.info.data as usize as NativeVulkanCudaDevicePtr
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
impl Drop for NativeVulkanCudaMemoryMap {
    fn drop(&mut self) {
        unsafe {
            gst::ffi::gst_memory_unmap(self.memory, &mut self.info);
        }
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
struct NativeVulkanCudaExternalImageMemory {
    handle: NativeVulkanCudaExternalMemoryHandle,
    _mipmapped_array: NativeVulkanCudaMipmappedArrayHandle,
    array: NativeVulkanCudaArrayHandle,
}

#[cfg(feature = "native-vulkan-gst-video")]
impl NativeVulkanCudaExternalImageMemory {
    fn import_opaque_fd(
        fd: i32,
        allocation_size: u64,
        width: u32,
        height: u32,
        channels: u32,
        label: &str,
    ) -> Result<Self, NativeVulkanError> {
        let mut external_memory = ptr::null_mut();
        let desc = NativeVulkanCudaExternalMemoryHandleDesc {
            type_: CUDA_EXTERNAL_MEMORY_HANDLE_TYPE_OPAQUE_FD,
            handle: NativeVulkanCudaExternalMemoryHandleUnion { fd },
            size: allocation_size,
            flags: 0,
            reserved: [0; 16],
        };
        native_vulkan_cuda_result(
            unsafe { CuImportExternalMemory(&mut external_memory, &desc) },
            &format!("native Vulkan CUDA import {label} Vulkan image external memory"),
        )?;
        if external_memory.is_null() {
            return Err(NativeVulkanError::Video(format!(
                "native Vulkan CUDA imported {label} external memory is null"
            )));
        }
        let mut mipmapped_array = ptr::null_mut();
        let mipmapped_desc = NativeVulkanCudaExternalMemoryMipmappedArrayDesc {
            offset: 0,
            array_desc: NativeVulkanCudaArray3dDesc {
                width: usize::try_from(width).map_err(|_| {
                    NativeVulkanError::Video(format!("native Vulkan CUDA {label} width too large"))
                })?,
                height: usize::try_from(height).map_err(|_| {
                    NativeVulkanError::Video(format!("native Vulkan CUDA {label} height too large"))
                })?,
                depth: 0,
                format: CUDA_ARRAY_FORMAT_UNSIGNED_INT8,
                num_channels: channels,
                flags: 0,
            },
            num_levels: 1,
            reserved: [0; 16],
        };
        if let Err(err) = native_vulkan_cuda_result(
            unsafe {
                CuExternalMemoryGetMappedMipmappedArray(
                    &mut mipmapped_array,
                    external_memory,
                    &mipmapped_desc,
                )
            },
            &format!("native Vulkan CUDA map {label} Vulkan image mipmapped array"),
        ) {
            let _ = unsafe { CuDestroyExternalMemory(external_memory) };
            return Err(err);
        }
        if mipmapped_array.is_null() {
            let _ = unsafe { CuDestroyExternalMemory(external_memory) };
            return Err(NativeVulkanError::Video(format!(
                "native Vulkan CUDA mapped {label} mipmapped array is null"
            )));
        }
        let mut array = ptr::null_mut();
        if let Err(err) = native_vulkan_cuda_result(
            unsafe { cuMipmappedArrayGetLevel(&mut array, mipmapped_array, 0) },
            &format!("native Vulkan CUDA get {label} mipmapped array level 0"),
        ) {
            let _ = unsafe { CuDestroyExternalMemory(external_memory) };
            return Err(err);
        }
        if array.is_null() {
            let _ = unsafe { CuDestroyExternalMemory(external_memory) };
            return Err(NativeVulkanError::Video(format!(
                "native Vulkan CUDA {label} CUDA array level is null"
            )));
        }
        Ok(Self {
            handle: external_memory,
            _mipmapped_array: mipmapped_array,
            array,
        })
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
impl Drop for NativeVulkanCudaExternalImageMemory {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            let _ = unsafe { CuDestroyExternalMemory(self.handle) };
        }
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
struct NativeVulkanCudaStream {
    handle: NativeVulkanCudaStreamHandle,
}

#[cfg(feature = "native-vulkan-gst-video")]
impl NativeVulkanCudaStream {
    fn new() -> Result<Self, NativeVulkanError> {
        let mut handle = ptr::null_mut();
        native_vulkan_cuda_result(
            unsafe { CuStreamCreate(&mut handle, CUDA_STREAM_NON_BLOCKING) },
            "native Vulkan CUDA create copy stream",
        )?;
        if handle.is_null() {
            return Err(NativeVulkanError::Video(
                "native Vulkan CUDA copy stream is null".to_owned(),
            ));
        }
        Ok(Self { handle })
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
impl Drop for NativeVulkanCudaStream {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            let _ = unsafe { CuStreamDestroy(self.handle) };
        }
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
struct NativeVulkanGstCudaContextPushGuard;

#[cfg(feature = "native-vulkan-gst-video")]
impl NativeVulkanGstCudaContextPushGuard {
    fn new(context: *mut NativeVulkanGstCudaContext) -> Result<Self, NativeVulkanError> {
        if context.is_null() {
            return Err(NativeVulkanError::Video(
                "native Vulkan CUDA cannot push null GstCudaContext".to_owned(),
            ));
        }
        let pushed = unsafe { gst_cuda_context_push(context) } != gst::glib::ffi::GFALSE;
        if !pushed {
            return Err(NativeVulkanError::Video(
                "native Vulkan CUDA failed to push GstCudaContext".to_owned(),
            ));
        }
        Ok(Self)
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
impl Drop for NativeVulkanGstCudaContextPushGuard {
    fn drop(&mut self) {
        let mut context = ptr::null_mut();
        let _ = unsafe { gst_cuda_context_pop(&mut context) };
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_is_cuda_memory(memory: &gst::MemoryRef) -> bool {
    if memory.is_type("CUDAMemory") || memory.is_type("gst.cuda.memory") {
        return true;
    }
    let is_cuda = unsafe { gst_is_cuda_memory(memory.as_ptr().cast_mut()) };
    is_cuda != gst::glib::ffi::GFALSE
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_cuda_result(result: i32, label: &str) -> Result<(), NativeVulkanError> {
    if result == CUDA_SUCCESS {
        return Ok(());
    }
    Err(NativeVulkanError::Video(format!(
        "{label} failed: {}",
        native_vulkan_cuda_error_label(result)
    )))
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_cuda_error_label(result: i32) -> String {
    let mut name = ptr::null();
    let mut description = ptr::null();
    let name_result = unsafe { CuGetErrorName(result, &mut name) };
    let description_result = unsafe { CuGetErrorString(result, &mut description) };
    let name = if name_result == CUDA_SUCCESS && !name.is_null() {
        unsafe { CStr::from_ptr(name) }
            .to_string_lossy()
            .into_owned()
    } else {
        "unknown".to_owned()
    };
    let description = if description_result == CUDA_SUCCESS && !description.is_null() {
        unsafe { CStr::from_ptr(description) }
            .to_string_lossy()
            .into_owned()
    } else {
        "no description".to_owned()
    };
    format!("{result}:{name}:{description}")
}

#[cfg(feature = "native-vulkan-gst-video")]
#[repr(C)]
struct NativeVulkanGstCudaMemory {
    mem: gst::ffi::GstMemory,
    context: *mut NativeVulkanGstCudaContext,
    info: gst_video::ffi::GstVideoInfo,
}

#[cfg(feature = "native-vulkan-gst-video")]
#[repr(C)]
struct NativeVulkanGstCudaContext {
    _private: [u8; 0],
}

#[cfg(feature = "native-vulkan-gst-video")]
#[repr(C)]
struct NativeVulkanGstVaDisplay {
    _private: [u8; 0],
}

#[cfg(feature = "native-vulkan-gst-video")]
#[derive(Debug, Clone, Copy)]
struct NativeVulkanVaSurface {
    display: NativeVulkanVaDisplay,
    surface: NativeVulkanVaSurfaceId,
}

#[cfg(feature = "native-vulkan-gst-video")]
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct NativeVulkanVaDrmPrimeObject {
    fd: i32,
    size: u32,
    drm_format_modifier: u64,
}

#[cfg(feature = "native-vulkan-gst-video")]
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct NativeVulkanVaDrmPrimeLayer {
    drm_format: u32,
    num_planes: u32,
    object_index: [u32; 4],
    offset: [u32; 4],
    pitch: [u32; 4],
}

#[cfg(feature = "native-vulkan-gst-video")]
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct NativeVulkanVaDrmPrimeSurfaceDescriptor {
    fourcc: u32,
    width: u32,
    height: u32,
    num_objects: u32,
    objects: [NativeVulkanVaDrmPrimeObject; 4],
    num_layers: u32,
    layers: [NativeVulkanVaDrmPrimeLayer; 4],
}

#[cfg(feature = "native-vulkan-gst-video")]
#[derive(Debug)]
struct NativeVulkanVaExportedPrimeSurface {
    descriptor: NativeVulkanVaDrmPrimeSurfaceDescriptor,
    owned_fds: Vec<OwnedFd>,
}

#[cfg(feature = "native-vulkan-gst-video")]
impl NativeVulkanVaExportedPrimeSurface {
    fn new(descriptor: NativeVulkanVaDrmPrimeSurfaceDescriptor) -> Result<Self, NativeVulkanError> {
        if descriptor.num_objects > 4 {
            return Err(NativeVulkanError::Video(format!(
                "VA DRM PRIME descriptor has invalid object count {}",
                descriptor.num_objects
            )));
        }
        let mut owned_fds = Vec::with_capacity(descriptor.num_objects as usize);
        for object in descriptor.objects[..descriptor.num_objects as usize].iter() {
            if object.fd < 0 {
                return Err(NativeVulkanError::Video(
                    "VA DRM PRIME export returned an invalid fd".to_owned(),
                ));
            }
            owned_fds.push(unsafe { OwnedFd::from_raw_fd(object.fd) });
        }
        Ok(Self {
            descriptor,
            owned_fds,
        })
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
const GST_MAP_READ_CUDA: gst::ffi::GstMapFlags =
    gst::ffi::GST_MAP_READ | (gst::ffi::GST_MAP_FLAG_LAST << 1);
#[cfg(feature = "native-vulkan-gst-video")]
const CUDA_SUCCESS: i32 = 0;
#[cfg(feature = "native-vulkan-gst-video")]
const CUDA_MEMORYTYPE_DEVICE: u32 = 2;
#[cfg(feature = "native-vulkan-gst-video")]
const CUDA_MEMORYTYPE_ARRAY: u32 = 3;
#[cfg(feature = "native-vulkan-gst-video")]
const CUDA_EXTERNAL_MEMORY_HANDLE_TYPE_OPAQUE_FD: u32 = 1;
#[cfg(feature = "native-vulkan-gst-video")]
const CUDA_STREAM_NON_BLOCKING: u32 = 1;
#[cfg(feature = "native-vulkan-gst-video")]
const CUDA_ARRAY_FORMAT_UNSIGNED_INT8: u32 = 1;
#[cfg(feature = "native-vulkan-gst-video")]
const DRM_FORMAT_NV12: u32 = 0x3231_564e;
#[cfg(feature = "native-vulkan-gst-video")]
const DRM_FORMAT_R8: u32 = 0x2020_3852;
#[cfg(feature = "native-vulkan-gst-video")]
const DRM_FORMAT_GR88: u32 = 0x3838_5247;
#[cfg(feature = "native-vulkan-gst-video")]
const DRM_FORMAT_MOD_LINEAR: u64 = 0;
#[cfg(feature = "native-vulkan-gst-video")]
const DRM_FORMAT_MOD_INVALID: u64 = 0x00ff_ffff_ffff_ffff;
#[cfg(feature = "native-vulkan-gst-video")]
const VA_STATUS_SUCCESS: NativeVulkanVaStatus = 0;
#[cfg(feature = "native-vulkan-gst-video")]
const VA_INVALID_SURFACE: NativeVulkanVaSurfaceId = u32::MAX;
#[cfg(feature = "native-vulkan-gst-video")]
const VA_FOURCC_NV12: u32 = 0x3231_564e;
#[cfg(feature = "native-vulkan-gst-video")]
const VA_SURFACE_ATTRIB_MEM_TYPE_DRM_PRIME_2: u32 = 0x4000_0000;
#[cfg(feature = "native-vulkan-gst-video")]
const VA_EXPORT_SURFACE_READ_ONLY: u32 = 0x0001;
#[cfg(feature = "native-vulkan-gst-video")]
const VA_EXPORT_SURFACE_SEPARATE_LAYERS: u32 = 0x0004;
#[cfg(feature = "native-vulkan-gst-video")]
const VA_EXPORT_SURFACE_COMPOSED_LAYERS: u32 = 0x0008;

#[cfg(feature = "native-vulkan-gst-video")]
type NativeVulkanCudaDevicePtr = u64;
#[cfg(feature = "native-vulkan-gst-video")]
type NativeVulkanCudaExternalMemoryHandle = *mut c_void;
#[cfg(feature = "native-vulkan-gst-video")]
type NativeVulkanCudaArrayHandle = *mut c_void;
#[cfg(feature = "native-vulkan-gst-video")]
type NativeVulkanCudaMipmappedArrayHandle = *mut c_void;
#[cfg(feature = "native-vulkan-gst-video")]
type NativeVulkanCudaStreamHandle = *mut c_void;
#[cfg(feature = "native-vulkan-gst-video")]
type NativeVulkanVaDisplay = *mut c_void;
#[cfg(feature = "native-vulkan-gst-video")]
type NativeVulkanVaSurfaceId = u32;
#[cfg(feature = "native-vulkan-gst-video")]
type NativeVulkanVaStatus = i32;

#[cfg(feature = "native-vulkan-gst-video")]
#[repr(C)]
#[derive(Clone, Copy)]
struct NativeVulkanCudaExternalMemoryWin32Handle {
    handle: *mut c_void,
    name: *const c_void,
}

#[cfg(feature = "native-vulkan-gst-video")]
#[repr(C)]
union NativeVulkanCudaExternalMemoryHandleUnion {
    fd: i32,
    win32: NativeVulkanCudaExternalMemoryWin32Handle,
}

#[cfg(feature = "native-vulkan-gst-video")]
#[repr(C)]
struct NativeVulkanCudaExternalMemoryHandleDesc {
    type_: u32,
    handle: NativeVulkanCudaExternalMemoryHandleUnion,
    size: u64,
    flags: u32,
    reserved: [u32; 16],
}

#[cfg(feature = "native-vulkan-gst-video")]
#[repr(C)]
struct NativeVulkanCudaArray3dDesc {
    width: usize,
    height: usize,
    depth: usize,
    format: u32,
    num_channels: u32,
    flags: u32,
}

#[cfg(feature = "native-vulkan-gst-video")]
#[repr(C)]
struct NativeVulkanCudaExternalMemoryMipmappedArrayDesc {
    offset: u64,
    array_desc: NativeVulkanCudaArray3dDesc,
    num_levels: u32,
    reserved: [u32; 16],
}

#[cfg(feature = "native-vulkan-gst-video")]
#[repr(C)]
struct NativeVulkanCudaMemcpy2D {
    src_x_in_bytes: usize,
    src_y: usize,
    src_memory_type: u32,
    src_host: *const c_void,
    src_device: NativeVulkanCudaDevicePtr,
    src_array: NativeVulkanCudaArrayHandle,
    src_pitch: usize,
    dst_x_in_bytes: usize,
    dst_y: usize,
    dst_memory_type: u32,
    dst_host: *mut c_void,
    dst_device: NativeVulkanCudaDevicePtr,
    dst_array: NativeVulkanCudaArrayHandle,
    dst_pitch: usize,
    width_in_bytes: usize,
    height: usize,
}

#[cfg(feature = "native-vulkan-gst-video")]
#[link(name = "gstcuda-1.0")]
#[allow(clashing_extern_declarations)]
unsafe extern "C" {
    fn CuGetErrorName(error: i32, p_str: *mut *const c_char) -> i32;
    fn CuGetErrorString(error: i32, p_str: *mut *const c_char) -> i32;
    fn CuMemcpy2DAsync(
        copy: *const NativeVulkanCudaMemcpy2D,
        stream: NativeVulkanCudaStreamHandle,
    ) -> i32;
    fn CuStreamCreate(stream_out: *mut NativeVulkanCudaStreamHandle, flags: u32) -> i32;
    fn CuStreamDestroy(stream: NativeVulkanCudaStreamHandle) -> i32;
    fn CuStreamSynchronize(stream: NativeVulkanCudaStreamHandle) -> i32;
    fn CuImportExternalMemory(
        ext_mem_out: *mut NativeVulkanCudaExternalMemoryHandle,
        mem_handle_desc: *const NativeVulkanCudaExternalMemoryHandleDesc,
    ) -> i32;
    fn CuExternalMemoryGetMappedMipmappedArray(
        mipmap: *mut NativeVulkanCudaMipmappedArrayHandle,
        ext_mem: NativeVulkanCudaExternalMemoryHandle,
        mipmap_desc: *const NativeVulkanCudaExternalMemoryMipmappedArrayDesc,
    ) -> i32;
    fn CuDestroyExternalMemory(ext_mem: NativeVulkanCudaExternalMemoryHandle) -> i32;
    fn gst_cuda_context_push(ctx: *mut NativeVulkanGstCudaContext) -> gst::glib::ffi::gboolean;
    fn gst_cuda_context_pop(cuda_ctx: *mut *mut c_void) -> gst::glib::ffi::gboolean;
    fn gst_is_cuda_memory(mem: *mut gst::ffi::GstMemory) -> gst::glib::ffi::gboolean;
    fn gst_cuda_memory_sync(mem: *mut NativeVulkanGstCudaMemory);
}

#[cfg(feature = "native-vulkan-gst-video")]
#[link(name = "cuda")]
unsafe extern "C" {
    fn cuMipmappedArrayGetLevel(
        level_array: *mut NativeVulkanCudaArrayHandle,
        mipmapped_array: NativeVulkanCudaMipmappedArrayHandle,
        level: u32,
    ) -> i32;
}

#[cfg(feature = "native-vulkan-gst-video")]
#[link(name = "gstallocators-1.0")]
unsafe extern "C" {
    fn gst_is_dmabuf_memory(mem: *mut gst::ffi::GstMemory) -> gst::glib::ffi::gboolean;
    fn gst_dmabuf_memory_get_fd(mem: *mut gst::ffi::GstMemory) -> i32;
}

#[cfg(feature = "native-vulkan-gst-video")]
#[link(name = "gstvideo-1.0")]
unsafe extern "C" {
    fn gst_video_dma_drm_fourcc_from_string(format_str: *const c_char, modifier: *mut u64) -> u32;
}

#[cfg(feature = "native-vulkan-gst-video")]
#[link(name = "gstva-1.0")]
unsafe extern "C" {
    fn gst_va_memory_get_surface(mem: *mut gst::ffi::GstMemory) -> NativeVulkanVaSurfaceId;
    fn gst_va_memory_peek_display(mem: *mut gst::ffi::GstMemory) -> *mut NativeVulkanGstVaDisplay;
    fn gst_va_buffer_get_surface(buffer: *mut gst::ffi::GstBuffer) -> NativeVulkanVaSurfaceId;
    fn gst_va_buffer_peek_display(
        buffer: *mut gst::ffi::GstBuffer,
    ) -> *mut NativeVulkanGstVaDisplay;
    fn gst_va_display_get_va_dpy(display: *mut NativeVulkanGstVaDisplay) -> NativeVulkanVaDisplay;
}

#[cfg(feature = "native-vulkan-gst-video")]
#[link(name = "va")]
unsafe extern "C" {
    fn vaSyncSurface(
        display: NativeVulkanVaDisplay,
        render_target: NativeVulkanVaSurfaceId,
    ) -> NativeVulkanVaStatus;
    fn vaExportSurfaceHandle(
        display: NativeVulkanVaDisplay,
        surface_id: NativeVulkanVaSurfaceId,
        mem_type: u32,
        flags: u32,
        descriptor: *mut c_void,
    ) -> NativeVulkanVaStatus;
    fn vaErrorStr(error_status: NativeVulkanVaStatus) -> *const c_char;
}

#[cfg(feature = "native-vulkan-gst-video")]
const NATIVE_VULKAN_VIDEO_VERTEX_SPIRV: [u32; 440] = [
    119734787, 65536, 851979, 63, 0, 131089, 1, 393227, 1, 1280527431, 1685353262, 808793134, 0,
    196622, 0, 1, 524303, 0, 4, 1852399981, 0, 33, 37, 48, 196611, 2, 450, 655364, 1197427783,
    1279741775, 1885560645, 1953718128, 1600482425, 1701734764, 1919509599, 1769235301, 25974,
    524292, 1197427783, 1279741775, 1852399429, 1685417059, 1768185701, 1952671090, 6649449,
    262149, 4, 1852399981, 0, 327685, 12, 1769172848, 1852795252, 115, 196613, 21, 7566965, 393221,
    31, 1348430951, 1700164197, 2019914866, 0, 393222, 31, 0, 1348430951, 1953067887, 7237481,
    458758, 31, 1, 1348430951, 1953393007, 1702521171, 0, 458758, 31, 2, 1130327143, 1148217708,
    1635021673, 6644590, 458758, 31, 3, 1130327143, 1147956341, 1635021673, 6644590, 196613, 33, 0,
    393221, 37, 1449094247, 1702130277, 1684949368, 30821, 262149, 48, 1987403638, 0, 196613, 49,
    7629126, 327686, 49, 0, 1936090735, 29797, 327686, 49, 1, 1818321779, 101, 196613, 51, 7629158,
    196679, 31, 2, 327752, 31, 0, 11, 0, 327752, 31, 1, 11, 1, 327752, 31, 2, 11, 3, 327752, 31, 3,
    11, 4, 262215, 37, 11, 42, 262215, 48, 30, 0, 196679, 49, 2, 327752, 49, 0, 35, 0, 327752, 49,
    1, 35, 8, 131091, 2, 196641, 3, 2, 196630, 6, 32, 262167, 7, 6, 2, 262165, 8, 32, 0, 262187, 8,
    9, 3, 262172, 10, 7, 9, 262176, 11, 7, 10, 262187, 6, 13, 3212836864, 262187, 6, 14,
    3225419776, 327724, 7, 15, 13, 14, 262187, 6, 16, 1077936128, 262187, 6, 17, 1065353216,
    327724, 7, 18, 16, 17, 327724, 7, 19, 13, 17, 393260, 10, 20, 15, 18, 19, 262187, 6, 22, 0,
    262187, 6, 23, 1073741824, 327724, 7, 24, 22, 23, 327724, 7, 25, 23, 22, 327724, 7, 26, 22, 22,
    393260, 10, 27, 24, 25, 26, 262167, 28, 6, 4, 262187, 8, 29, 1, 262172, 30, 6, 29, 393246, 31,
    28, 6, 30, 30, 262176, 32, 3, 31, 262203, 32, 33, 3, 262165, 34, 32, 1, 262187, 34, 35, 0,
    262176, 36, 1, 34, 262203, 36, 37, 1, 262176, 39, 7, 7, 262176, 45, 3, 28, 262176, 47, 3, 7,
    262203, 47, 48, 3, 262174, 49, 7, 7, 262176, 50, 9, 49, 262203, 50, 51, 9, 262176, 52, 9, 7,
    262187, 34, 58, 1, 327734, 2, 4, 0, 3, 131320, 5, 262203, 11, 12, 7, 262203, 11, 21, 7, 196670,
    12, 20, 196670, 21, 27, 262205, 34, 38, 37, 327745, 39, 40, 12, 38, 262205, 7, 41, 40, 327761,
    6, 42, 41, 0, 327761, 6, 43, 41, 1, 458832, 28, 44, 42, 43, 22, 17, 327745, 45, 46, 33, 35,
    196670, 46, 44, 327745, 52, 53, 51, 35, 262205, 7, 54, 53, 262205, 34, 55, 37, 327745, 39, 56,
    21, 55, 262205, 7, 57, 56, 327745, 52, 59, 51, 58, 262205, 7, 60, 59, 327813, 7, 61, 57, 60,
    327809, 7, 62, 54, 61, 196670, 48, 62, 65789, 65592,
];

#[cfg(feature = "native-vulkan-gst-video")]
const NATIVE_VULKAN_VIDEO_FRAGMENT_SPIRV: [u32; 554] = [
    119734787, 65536, 851979, 90, 0, 131089, 1, 393227, 1, 1280527431, 1685353262, 808793134, 0,
    196622, 0, 1, 458767, 4, 4, 1852399981, 0, 16, 82, 196624, 4, 7, 196611, 2, 450, 655364,
    1197427783, 1279741775, 1885560645, 1953718128, 1600482425, 1701734764, 1919509599, 1769235301,
    25974, 524292, 1197427783, 1279741775, 1852399429, 1685417059, 1768185701, 1952671090, 6649449,
    262149, 4, 1852399981, 0, 196613, 8, 121, 327685, 12, 1702125433, 1920300152, 101, 262149, 16,
    1987403638, 0, 196613, 24, 30325, 327685, 25, 1952413301, 1970567269, 25970, 196613, 30, 117,
    196613, 33, 118, 196613, 54, 114, 196613, 62, 103, 196613, 74, 98, 327685, 82, 1601467759,
    1869377379, 114, 262215, 12, 33, 0, 262215, 12, 34, 0, 262215, 16, 30, 0, 262215, 25, 33, 1,
    262215, 25, 34, 0, 262215, 82, 30, 0, 131091, 2, 196641, 3, 2, 196630, 6, 32, 262176, 7, 7, 6,
    589849, 9, 6, 1, 0, 0, 0, 1, 0, 196635, 10, 9, 262176, 11, 0, 10, 262203, 11, 12, 0, 262167,
    14, 6, 2, 262176, 15, 1, 14, 262203, 15, 16, 1, 262167, 18, 6, 4, 262165, 20, 32, 0, 262187,
    20, 21, 0, 262176, 23, 7, 14, 262203, 11, 25, 0, 262187, 20, 34, 1, 262187, 6, 38, 1031831681,
    262187, 6, 40, 1062984668, 262187, 6, 42, 0, 262187, 6, 43, 1065353216, 262187, 6, 47,
    1063313633, 262187, 6, 56, 1070174988, 262187, 6, 58, 1056964608, 262187, 6, 64, 1044368274,
    262187, 6, 69, 1055894222, 262187, 6, 76, 1072530509, 262176, 81, 3, 18, 262203, 81, 82, 3,
    327734, 2, 4, 0, 3, 131320, 5, 262203, 7, 8, 7, 262203, 23, 24, 7, 262203, 7, 30, 7, 262203, 7,
    33, 7, 262203, 7, 54, 7, 262203, 7, 62, 7, 262203, 7, 74, 7, 262205, 10, 13, 12, 262205, 14,
    17, 16, 327767, 18, 19, 13, 17, 327761, 6, 22, 19, 0, 196670, 8, 22, 262205, 10, 26, 25,
    262205, 14, 27, 16, 327767, 18, 28, 26, 27, 458831, 14, 29, 28, 28, 0, 1, 196670, 24, 29,
    327745, 7, 31, 24, 21, 262205, 6, 32, 31, 196670, 30, 32, 327745, 7, 35, 24, 34, 262205, 6, 36,
    35, 196670, 33, 36, 262205, 6, 37, 8, 327811, 6, 39, 37, 38, 327816, 6, 41, 39, 40, 524300, 6,
    44, 1, 43, 41, 42, 43, 196670, 8, 44, 262205, 6, 45, 30, 327811, 6, 46, 45, 38, 327816, 6, 48,
    46, 47, 524300, 6, 49, 1, 43, 48, 42, 43, 196670, 30, 49, 262205, 6, 50, 33, 327811, 6, 51, 50,
    38, 327816, 6, 52, 51, 47, 524300, 6, 53, 1, 43, 52, 42, 43, 196670, 33, 53, 262205, 6, 55, 8,
    262205, 6, 57, 33, 327811, 6, 59, 57, 58, 327813, 6, 60, 56, 59, 327809, 6, 61, 55, 60, 196670,
    54, 61, 262205, 6, 63, 8, 262205, 6, 65, 30, 327811, 6, 66, 65, 58, 327813, 6, 67, 64, 66,
    327811, 6, 68, 63, 67, 262205, 6, 70, 33, 327811, 6, 71, 70, 58, 327813, 6, 72, 69, 71, 327811,
    6, 73, 68, 72, 196670, 62, 73, 262205, 6, 75, 8, 262205, 6, 77, 30, 327811, 6, 78, 77, 58,
    327813, 6, 79, 76, 78, 327809, 6, 80, 75, 79, 196670, 74, 80, 262205, 6, 83, 54, 524300, 6, 84,
    1, 43, 83, 42, 43, 262205, 6, 85, 62, 524300, 6, 86, 1, 43, 85, 42, 43, 262205, 6, 87, 74,
    524300, 6, 88, 1, 43, 87, 42, 43, 458832, 18, 89, 84, 86, 88, 43, 196670, 82, 89, 65789, 65592,
];

struct NativeVulkanStaticImageUpload {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    buffer_image_copy: vk::BufferImageCopy,
    size_bytes: vk::DeviceSize,
}

impl NativeVulkanStaticImageUpload {
    fn new(
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        device: &ash::Device,
        source: &PathBuf,
        fit: FitMode,
        background: Option<&str>,
        swapchain_format: vk::Format,
        extent: vk::Extent2D,
    ) -> Result<Self, NativeVulkanError> {
        let pixels = native_vulkan_static_image_pixels(
            source,
            fit,
            background,
            swapchain_format,
            (extent.width, extent.height),
        )?;
        let size_bytes = pixels.len() as vk::DeviceSize;
        let buffer_create_info = vk::BufferCreateInfo::default()
            .size(size_bytes)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let buffer =
            unsafe { device.create_buffer(&buffer_create_info, None) }.map_err(|result| {
                NativeVulkanError::Vulkan {
                    operation: "vkCreateBuffer(static_image)",
                    result,
                }
            })?;
        let requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
        let memory_properties =
            unsafe { instance.get_physical_device_memory_properties(physical_device) };
        let memory_type_index = native_vulkan_memory_type_index(
            &memory_properties,
            requirements.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )
        .ok_or(NativeVulkanError::MissingMemoryType(
            "static image staging buffer",
        ))?;
        let allocate_info = vk::MemoryAllocateInfo::default()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type_index);
        let memory = unsafe { device.allocate_memory(&allocate_info, None) }.map_err(|result| {
            unsafe {
                device.destroy_buffer(buffer, None);
            }
            NativeVulkanError::Vulkan {
                operation: "vkAllocateMemory(static_image)",
                result,
            }
        })?;
        if let Err(err) = unsafe { device.bind_buffer_memory(buffer, memory, 0) } {
            unsafe {
                device.free_memory(memory, None);
                device.destroy_buffer(buffer, None);
            }
            return Err(NativeVulkanError::Vulkan {
                operation: "vkBindBufferMemory(static_image)",
                result: err,
            });
        }
        let map = unsafe { device.map_memory(memory, 0, size_bytes, vk::MemoryMapFlags::empty()) }
            .map_err(|result| {
                unsafe {
                    device.free_memory(memory, None);
                    device.destroy_buffer(buffer, None);
                }
                NativeVulkanError::Vulkan {
                    operation: "vkMapMemory(static_image)",
                    result,
                }
            })?;
        unsafe {
            ptr::copy_nonoverlapping(pixels.as_ptr(), map.cast::<u8>(), pixels.len());
            device.unmap_memory(memory);
        }

        Ok(Self {
            buffer,
            memory,
            buffer_image_copy: vk::BufferImageCopy {
                buffer_offset: 0,
                buffer_row_length: 0,
                buffer_image_height: 0,
                image_subresource: vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                },
                image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
                image_extent: vk::Extent3D {
                    width: extent.width,
                    height: extent.height,
                    depth: 1,
                },
            },
            size_bytes,
        })
    }

    fn destroy(self, device: &ash::Device) {
        unsafe {
            device.free_memory(self.memory, None);
            device.destroy_buffer(self.buffer, None);
        }
    }
}

fn native_vulkan_static_image_pixels(
    source: &PathBuf,
    fit: FitMode,
    background: Option<&str>,
    format: vk::Format,
    target_size: (u32, u32),
) -> Result<Vec<u8>, NativeVulkanError> {
    if target_size.0 == 0 || target_size.1 == 0 {
        return Err(NativeVulkanError::StaticImage(
            "target image size is zero".to_owned(),
        ));
    }
    let image = image::ImageReader::open(source)
        .map_err(|err| NativeVulkanError::StaticImage(format!("open {}: {err}", source.display())))?
        .with_guessed_format()
        .map_err(|err| {
            NativeVulkanError::StaticImage(format!("guess format {}: {err}", source.display()))
        })?
        .decode()
        .map_err(|err| {
            NativeVulkanError::StaticImage(format!("decode {}: {err}", source.display()))
        })?
        .to_rgba8();
    let mut canvas = image::RgbaImage::from_pixel(
        target_size.0,
        target_size.1,
        native_vulkan_parse_background(background),
    );
    native_vulkan_blit_fit(&image, &mut canvas, fit);
    Ok(native_vulkan_encode_swapchain_pixels(&canvas, format))
}

fn native_vulkan_parse_background(background: Option<&str>) -> image::Rgba<u8> {
    let Some(value) = background else {
        return image::Rgba([0, 0, 0, 255]);
    };
    let Some(hex) = value.trim().strip_prefix('#') else {
        return image::Rgba([0, 0, 0, 255]);
    };
    if hex.len() != 6 {
        return image::Rgba([0, 0, 0, 255]);
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
    image::Rgba([r, g, b, 255])
}

fn native_vulkan_blit_fit(source: &image::RgbaImage, canvas: &mut image::RgbaImage, fit: FitMode) {
    let source_width = source.width().max(1);
    let source_height = source.height().max(1);
    let target_width = canvas.width().max(1);
    let target_height = canvas.height().max(1);
    match fit {
        FitMode::Stretch => {
            let resized = image::imageops::resize(
                source,
                target_width,
                target_height,
                image::imageops::FilterType::Triangle,
            );
            image::imageops::replace(canvas, &resized, 0, 0);
        }
        FitMode::Center => {
            let x = (target_width as i64 - source_width as i64) / 2;
            let y = (target_height as i64 - source_height as i64) / 2;
            image::imageops::overlay(canvas, source, x, y);
        }
        FitMode::Tile => {
            let mut y = 0;
            while y < target_height {
                let mut x = 0;
                while x < target_width {
                    image::imageops::overlay(canvas, source, x as i64, y as i64);
                    x = x.saturating_add(source_width);
                }
                y = y.saturating_add(source_height);
            }
        }
        FitMode::Contain | FitMode::Cover => {
            let scale_x = target_width as f64 / source_width as f64;
            let scale_y = target_height as f64 / source_height as f64;
            let scale = if fit == FitMode::Cover {
                scale_x.max(scale_y)
            } else {
                scale_x.min(scale_y)
            };
            let scaled_width = ((source_width as f64 * scale).round() as u32).max(1);
            let scaled_height = ((source_height as f64 * scale).round() as u32).max(1);
            let resized = image::imageops::resize(
                source,
                scaled_width,
                scaled_height,
                image::imageops::FilterType::Triangle,
            );
            let x = (target_width as i64 - scaled_width as i64) / 2;
            let y = (target_height as i64 - scaled_height as i64) / 2;
            image::imageops::overlay(canvas, &resized, x, y);
        }
    }
}

fn native_vulkan_encode_swapchain_pixels(image: &image::RgbaImage, format: vk::Format) -> Vec<u8> {
    let mut pixels = image.as_raw().clone();
    if matches!(
        format,
        vk::Format::B8G8R8A8_UNORM | vk::Format::B8G8R8A8_SRGB
    ) {
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
    }
    pixels
}

fn native_vulkan_memory_type_index(
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    memory_type_bits: u32,
    flags: vk::MemoryPropertyFlags,
) -> Option<u32> {
    memory_properties.memory_types[..memory_properties.memory_type_count as usize]
        .iter()
        .enumerate()
        .find_map(|(index, memory_type)| {
            let supported = (memory_type_bits & (1 << index)) != 0;
            (supported && memory_type.property_flags.contains(flags)).then_some(index as u32)
        })
}

struct NativeVulkanVideoDecodeQueueSelection {
    physical_device: vk::PhysicalDevice,
    physical_device_index: usize,
    physical_device_name: String,
    physical_device_type: &'static str,
    properties: vk::PhysicalDeviceProperties,
    queue_family_index: u32,
    queue_count: u32,
    queue_flags: vk::QueueFlags,
    video_codec_operations: vk::VideoCodecOperationFlagsKHR,
}

struct NativeVulkanVideoSessionCapabilityQuery {
    capability_flags: vk::VideoCapabilityFlagsKHR,
    min_bitstream_buffer_offset_alignment: u64,
    min_bitstream_buffer_size_alignment: u64,
    picture_access_granularity: vk::Extent2D,
    min_coded_extent: vk::Extent2D,
    max_coded_extent: vk::Extent2D,
    max_dpb_slots: u32,
    max_active_reference_pictures: u32,
    decode_capability_flags: vk::VideoDecodeCapabilityFlagsKHR,
    codec_max_level: Option<String>,
    std_header_version: vk::ExtensionProperties,
}

struct NativeVulkanVideoResourceImage {
    image: vk::Image,
    memory: vk::DeviceMemory,
    view: vk::ImageView,
    snapshot: NativeVulkanVideoSessionResourceImageSnapshot,
}

struct NativeVulkanVideoBitstreamBuffer {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    snapshot: NativeVulkanVideoSessionBitstreamBufferSnapshot,
}

#[cfg(feature = "native-vulkan-gst-video")]
struct NativeVulkanVideoDecodeReadbackBuffer {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    memory_size: u64,
    size: u64,
    y_plane_bytes: u64,
    uv_plane_bytes: u64,
    memory_property_flags: vk::MemoryPropertyFlags,
}

#[cfg(feature = "native-vulkan-gst-video")]
struct NativeVulkanDecodedSamplingTarget {
    image: vk::Image,
    memory: vk::DeviceMemory,
    view: vk::ImageView,
    readback_buffer: vk::Buffer,
    readback_memory: vk::DeviceMemory,
    extent: vk::Extent2D,
    format: vk::Format,
    total_bytes: u64,
    color_memory_size: u64,
    readback_memory_size: u64,
    readback_memory_property_flags: vk::MemoryPropertyFlags,
}

struct NativeVulkanVideoSessionParameters {
    parameters: vk::VideoSessionParametersKHR,
    snapshot: NativeVulkanVideoSessionParametersSnapshot,
}

struct NativeVulkanVideoBitstreamExtract {
    selected_access_unit: Vec<u8>,
    snapshot: NativeVulkanVideoBitstreamExtractSnapshot,
}

fn native_vulkan_video_session_smoke_inner(
    entry: &ash::Entry,
    instance: &ash::Instance,
    options: NativeVulkanVideoSessionSmokeOptions,
) -> Result<NativeVulkanVideoSessionSmokeSnapshot, NativeVulkanError> {
    let video_queue_loader = ash::khr::video_queue::Instance::new(entry, instance);
    let selection = select_native_vulkan_video_decode_queue(instance, options.codec)?;

    match options.codec {
        NativeVulkanVideoSessionCodec::H265Main8 => {
            let mut h265_profile_info = vk::VideoDecodeH265ProfileInfoKHR::default()
                .std_profile_idc(
                    vk::native::StdVideoH265ProfileIdc_STD_VIDEO_H265_PROFILE_IDC_MAIN,
                );
            let profile_info = vk::VideoProfileInfoKHR::default()
                .video_codec_operation(options.codec.codec_operation())
                .chroma_subsampling(vk::VideoChromaSubsamplingFlagsKHR::TYPE_420)
                .luma_bit_depth(vk::VideoComponentBitDepthFlagsKHR::TYPE_8)
                .chroma_bit_depth(vk::VideoComponentBitDepthFlagsKHR::TYPE_8)
                .push_next(&mut h265_profile_info);
            let capabilities = native_vulkan_video_session_h265_capabilities(
                &video_queue_loader,
                selection.physical_device,
                &profile_info,
            )?;
            native_vulkan_video_session_create_and_bind(
                &video_queue_loader,
                instance,
                selection,
                options,
                &profile_info,
                capabilities,
            )
        }
        NativeVulkanVideoSessionCodec::Av1Main8 => {
            let mut av1_profile_info = vk::VideoDecodeAV1ProfileInfoKHR::default()
                .std_profile(vk::native::StdVideoAV1Profile_STD_VIDEO_AV1_PROFILE_MAIN)
                .film_grain_support(false);
            let profile_info = vk::VideoProfileInfoKHR::default()
                .video_codec_operation(options.codec.codec_operation())
                .chroma_subsampling(vk::VideoChromaSubsamplingFlagsKHR::TYPE_420)
                .luma_bit_depth(vk::VideoComponentBitDepthFlagsKHR::TYPE_8)
                .chroma_bit_depth(vk::VideoComponentBitDepthFlagsKHR::TYPE_8)
                .push_next(&mut av1_profile_info);
            let capabilities = native_vulkan_video_session_av1_capabilities(
                &video_queue_loader,
                selection.physical_device,
                &profile_info,
            )?;
            native_vulkan_video_session_create_and_bind(
                &video_queue_loader,
                instance,
                selection,
                options,
                &profile_info,
                capabilities,
            )
        }
    }
}

fn select_native_vulkan_video_decode_queue(
    instance: &ash::Instance,
    codec: NativeVulkanVideoSessionCodec,
) -> Result<NativeVulkanVideoDecodeQueueSelection, NativeVulkanError> {
    let physical_devices = unsafe { instance.enumerate_physical_devices() }.map_err(|result| {
        NativeVulkanError::Vulkan {
            operation: "vkEnumeratePhysicalDevices",
            result,
        }
    })?;
    let required_extensions = native_vulkan_video_session_required_device_extensions(codec);
    let mut selected = None::<NativeVulkanVideoDecodeQueueSelection>;

    for (physical_device_index, physical_device) in physical_devices.iter().copied().enumerate() {
        let extensions = native_vulkan_device_extension_names(instance, physical_device)?;
        if !required_extensions.iter().all(|extension| {
            native_vulkan_extension_available_by_name(&extensions, ash_extension_name(extension))
        }) {
            continue;
        }

        let properties = unsafe { instance.get_physical_device_properties(physical_device) };
        let queue_families =
            native_vulkan_video_decode_queue_family_infos(instance, physical_device);
        for queue_family in queue_families {
            if queue_family.queue_count == 0
                || !queue_family
                    .queue_flags
                    .contains(vk::QueueFlags::VIDEO_DECODE_KHR)
                || !queue_family
                    .video_codec_operations
                    .contains(codec.codec_operation())
            {
                continue;
            }

            let candidate = NativeVulkanVideoDecodeQueueSelection {
                physical_device,
                physical_device_index,
                physical_device_name: native_vulkan_physical_device_name(properties),
                physical_device_type: native_vulkan_physical_device_type_label(
                    properties.device_type,
                ),
                properties,
                queue_family_index: queue_family.queue_family_index,
                queue_count: queue_family.queue_count,
                queue_flags: queue_family.queue_flags,
                video_codec_operations: queue_family.video_codec_operations,
            };
            let prefer_candidate = selected.as_ref().is_none_or(|current| {
                properties.device_type == vk::PhysicalDeviceType::DISCRETE_GPU
                    && current.properties.device_type != vk::PhysicalDeviceType::DISCRETE_GPU
            });
            if prefer_candidate {
                selected = Some(candidate);
            }
        }
    }

    selected.ok_or_else(|| {
        NativeVulkanError::Video(format!(
            "no Vulkan device exposes {} with a matching video decode queue",
            codec.label()
        ))
    })
}

struct NativeVulkanVideoDecodeQueueFamilyInfo {
    queue_family_index: u32,
    queue_count: u32,
    queue_flags: vk::QueueFlags,
    video_codec_operations: vk::VideoCodecOperationFlagsKHR,
}

fn native_vulkan_video_decode_queue_family_infos(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
) -> Vec<NativeVulkanVideoDecodeQueueFamilyInfo> {
    let queue_family_count =
        unsafe { instance.get_physical_device_queue_family_properties2_len(physical_device) };
    let mut queue_properties = vec![vk::QueueFamilyProperties2::default(); queue_family_count];
    let mut video_properties =
        vec![vk::QueueFamilyVideoPropertiesKHR::default(); queue_family_count];
    for (queue, video) in queue_properties.iter_mut().zip(video_properties.iter_mut()) {
        queue.p_next = (video as *mut vk::QueueFamilyVideoPropertiesKHR<'_>).cast();
    }
    unsafe {
        instance
            .get_physical_device_queue_family_properties2(physical_device, &mut queue_properties);
    }

    queue_properties
        .iter()
        .zip(video_properties.iter())
        .enumerate()
        .map(
            |(queue_family_index, (queue, video))| NativeVulkanVideoDecodeQueueFamilyInfo {
                queue_family_index: queue_family_index as u32,
                queue_count: queue.queue_family_properties.queue_count,
                queue_flags: queue.queue_family_properties.queue_flags,
                video_codec_operations: video.video_codec_operations,
            },
        )
        .collect()
}

fn native_vulkan_graphics_queue_family_index(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    preferred_queue_family_index: u32,
) -> Result<u32, NativeVulkanError> {
    let queue_families =
        unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
    if queue_families
        .get(preferred_queue_family_index as usize)
        .is_some_and(|queue_family| {
            queue_family.queue_count > 0
                && queue_family.queue_flags.contains(vk::QueueFlags::GRAPHICS)
        })
    {
        return Ok(preferred_queue_family_index);
    }
    queue_families
        .iter()
        .enumerate()
        .find_map(|(queue_family_index, queue_family)| {
            (queue_family.queue_count > 0
                && queue_family.queue_flags.contains(vk::QueueFlags::GRAPHICS))
            .then_some(queue_family_index as u32)
        })
        .ok_or_else(|| {
            NativeVulkanError::Video(
                "decoded first-frame sampling requires a graphics-capable queue family".to_owned(),
            )
        })
}

fn native_vulkan_video_session_required_device_extensions(
    codec: NativeVulkanVideoSessionCodec,
) -> Vec<&'static CStr> {
    vec![
        vk::KHR_VIDEO_QUEUE_NAME,
        vk::KHR_VIDEO_DECODE_QUEUE_NAME,
        codec.codec_extension_name(),
    ]
}

fn native_vulkan_video_session_h265_capabilities(
    video_queue_loader: &ash::khr::video_queue::Instance,
    physical_device: vk::PhysicalDevice,
    profile_info: &vk::VideoProfileInfoKHR<'_>,
) -> Result<NativeVulkanVideoSessionCapabilityQuery, NativeVulkanError> {
    let mut h265_capabilities = vk::VideoDecodeH265CapabilitiesKHR::default();
    let mut decode_capabilities = vk::VideoDecodeCapabilitiesKHR::default();
    let mut capabilities = vk::VideoCapabilitiesKHR::default()
        .push_next(&mut h265_capabilities)
        .push_next(&mut decode_capabilities);

    unsafe {
        (video_queue_loader
            .fp()
            .get_physical_device_video_capabilities_khr)(
            physical_device,
            profile_info,
            &mut capabilities,
        )
    }
    .result()
    .map_err(|result| NativeVulkanError::Vulkan {
        operation: "vkGetPhysicalDeviceVideoCapabilitiesKHR(h265 session)",
        result,
    })?;

    let capability_flags = capabilities.flags;
    let min_bitstream_buffer_offset_alignment = capabilities.min_bitstream_buffer_offset_alignment;
    let min_bitstream_buffer_size_alignment = capabilities.min_bitstream_buffer_size_alignment;
    let picture_access_granularity = capabilities.picture_access_granularity;
    let min_coded_extent = capabilities.min_coded_extent;
    let max_coded_extent = capabilities.max_coded_extent;
    let max_dpb_slots = capabilities.max_dpb_slots;
    let max_active_reference_pictures = capabilities.max_active_reference_pictures;
    let std_header_version = capabilities.std_header_version;
    let decode_capability_flags = decode_capabilities.flags;
    let codec_max_level =
        native_vulkan_h265_level_label(h265_capabilities.max_level_idc).map(str::to_owned);

    Ok(NativeVulkanVideoSessionCapabilityQuery {
        capability_flags,
        min_bitstream_buffer_offset_alignment,
        min_bitstream_buffer_size_alignment,
        picture_access_granularity,
        min_coded_extent,
        max_coded_extent,
        max_dpb_slots,
        max_active_reference_pictures,
        decode_capability_flags,
        codec_max_level,
        std_header_version,
    })
}

fn native_vulkan_video_session_av1_capabilities(
    video_queue_loader: &ash::khr::video_queue::Instance,
    physical_device: vk::PhysicalDevice,
    profile_info: &vk::VideoProfileInfoKHR<'_>,
) -> Result<NativeVulkanVideoSessionCapabilityQuery, NativeVulkanError> {
    let mut av1_capabilities = vk::VideoDecodeAV1CapabilitiesKHR::default();
    let mut decode_capabilities = vk::VideoDecodeCapabilitiesKHR::default();
    let mut capabilities = vk::VideoCapabilitiesKHR::default()
        .push_next(&mut av1_capabilities)
        .push_next(&mut decode_capabilities);

    unsafe {
        (video_queue_loader
            .fp()
            .get_physical_device_video_capabilities_khr)(
            physical_device,
            profile_info,
            &mut capabilities,
        )
    }
    .result()
    .map_err(|result| NativeVulkanError::Vulkan {
        operation: "vkGetPhysicalDeviceVideoCapabilitiesKHR(av1 session)",
        result,
    })?;

    let capability_flags = capabilities.flags;
    let min_bitstream_buffer_offset_alignment = capabilities.min_bitstream_buffer_offset_alignment;
    let min_bitstream_buffer_size_alignment = capabilities.min_bitstream_buffer_size_alignment;
    let picture_access_granularity = capabilities.picture_access_granularity;
    let min_coded_extent = capabilities.min_coded_extent;
    let max_coded_extent = capabilities.max_coded_extent;
    let max_dpb_slots = capabilities.max_dpb_slots;
    let max_active_reference_pictures = capabilities.max_active_reference_pictures;
    let std_header_version = capabilities.std_header_version;
    let decode_capability_flags = decode_capabilities.flags;
    let codec_max_level =
        native_vulkan_av1_level_label(av1_capabilities.max_level).map(str::to_owned);

    Ok(NativeVulkanVideoSessionCapabilityQuery {
        capability_flags,
        min_bitstream_buffer_offset_alignment,
        min_bitstream_buffer_size_alignment,
        picture_access_granularity,
        min_coded_extent,
        max_coded_extent,
        max_dpb_slots,
        max_active_reference_pictures,
        decode_capability_flags,
        codec_max_level,
        std_header_version,
    })
}

fn native_vulkan_video_session_create_and_bind(
    video_queue_loader: &ash::khr::video_queue::Instance,
    instance: &ash::Instance,
    selection: NativeVulkanVideoDecodeQueueSelection,
    options: NativeVulkanVideoSessionSmokeOptions,
    profile_info: &vk::VideoProfileInfoKHR<'_>,
    capabilities: NativeVulkanVideoSessionCapabilityQuery,
) -> Result<NativeVulkanVideoSessionSmokeSnapshot, NativeVulkanError> {
    let requested_extent = vk::Extent2D {
        width: options.width,
        height: options.height,
    };
    if !native_vulkan_video_session_extent_supported(requested_extent, &capabilities) {
        return Err(NativeVulkanError::Video(format!(
            "requested Vulkan Video extent {}x{} is outside {:?}..{:?} or is not aligned to {:?}",
            requested_extent.width,
            requested_extent.height,
            (
                capabilities.min_coded_extent.width,
                capabilities.min_coded_extent.height
            ),
            (
                capabilities.max_coded_extent.width,
                capabilities.max_coded_extent.height
            ),
            (
                capabilities.picture_access_granularity.width,
                capabilities.picture_access_granularity.height
            )
        )));
    }

    let format_probe = native_vulkan_video_decode_format_probe(
        video_queue_loader,
        selection.physical_device,
        profile_info,
        capabilities.decode_capability_flags,
    );
    if !format_probe.nv12_dpb_supported
        || !format_probe.nv12_output_supported
        || !format_probe.nv12_sampled_output_supported
    {
        return Err(NativeVulkanError::Video(format!(
            "{} lacks NV12 decode+sampled format support for direct Vulkan composition{}",
            options.codec.label(),
            format_probe
                .query_error
                .as_ref()
                .map(|err| format!(": {err}"))
                .unwrap_or_default()
        )));
    }

    let graphics_queue_family_index = if options.sample_decoded_first_frame {
        Some(native_vulkan_graphics_queue_family_index(
            instance,
            selection.physical_device,
            selection.queue_family_index,
        )?)
    } else {
        None
    };
    let mut device_queue_family_indices = vec![selection.queue_family_index];
    if let Some(graphics_queue_family_index) = graphics_queue_family_index
        && !device_queue_family_indices.contains(&graphics_queue_family_index)
    {
        device_queue_family_indices.push(graphics_queue_family_index);
    }
    let priorities = [1.0_f32];
    let queue_create_infos = device_queue_family_indices
        .iter()
        .copied()
        .map(|queue_family_index| {
            vk::DeviceQueueCreateInfo::default()
                .queue_family_index(queue_family_index)
                .queue_priorities(&priorities)
        })
        .collect::<Vec<_>>();
    let enabled_extensions = native_vulkan_video_session_required_device_extensions(options.codec);
    let enabled_extension_names = enabled_extensions
        .iter()
        .map(|extension| extension.as_ptr())
        .collect::<Vec<_>>();
    let mut synchronization2_features =
        vk::PhysicalDeviceSynchronization2Features::default().synchronization2(true);
    let mut device_create_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queue_create_infos)
        .enabled_extension_names(&enabled_extension_names);
    if options.decode_first_frame {
        device_create_info = device_create_info.push_next(&mut synchronization2_features);
    }
    let device =
        unsafe { instance.create_device(selection.physical_device, &device_create_info, None) }
            .map_err(|result| NativeVulkanError::Vulkan {
                operation: "vkCreateDevice(vulkan video session)",
                result,
            })?;
    let video_queue_device = ash::khr::video_queue::Device::new(instance, &device);
    let video_decode_queue_device = ash::khr::video_decode_queue::Device::new(instance, &device);
    let memory_properties =
        unsafe { instance.get_physical_device_memory_properties(selection.physical_device) };
    let mut session = vk::VideoSessionKHR::null();
    let mut video_session_parameters = vk::VideoSessionParametersKHR::null();
    let mut allocated_memories = Vec::<vk::DeviceMemory>::new();
    let mut resource_images = Vec::<NativeVulkanVideoResourceImage>::new();
    let mut bitstream_buffer = None::<NativeVulkanVideoBitstreamBuffer>;
    let mut bitstream_extract = None::<NativeVulkanVideoBitstreamExtract>;

    let result = (|| -> Result<NativeVulkanVideoSessionSmokeSnapshot, NativeVulkanError> {
        if options.extract_bitstream {
            bitstream_extract = Some(native_vulkan_extract_video_bitstream(&options)?);
        }

        let session_max_dpb_slots =
            native_vulkan_video_session_max_dpb_slots(capabilities.max_dpb_slots);
        let session_max_active_reference_pictures =
            native_vulkan_video_session_max_active_reference_pictures(
                capabilities.max_active_reference_pictures,
                session_max_dpb_slots,
            );
        let picture_format = vk::Format::G8_B8R8_2PLANE_420_UNORM;
        let create_info = vk::VideoSessionCreateInfoKHR::default()
            .queue_family_index(selection.queue_family_index)
            .video_profile(profile_info)
            .picture_format(picture_format)
            .reference_picture_format(picture_format)
            .max_coded_extent(requested_extent)
            .max_dpb_slots(session_max_dpb_slots)
            .max_active_reference_pictures(session_max_active_reference_pictures)
            .std_header_version(&capabilities.std_header_version);
        session = native_vulkan_create_video_session(&video_queue_device, &create_info)?;

        let memory_requirements =
            native_vulkan_video_session_memory_requirements(&video_queue_device, session)?;
        let mut bind_infos = Vec::with_capacity(memory_requirements.len());
        let mut memory_snapshots = Vec::with_capacity(memory_requirements.len());
        let mut total_bound_memory_bytes = 0u64;
        for requirement in memory_requirements.iter() {
            if requirement.memory_requirements.size == 0 {
                return Err(NativeVulkanError::Video(format!(
                    "video session memory bind {} reported zero size",
                    requirement.memory_bind_index
                )));
            }
            let memory_type_index = native_vulkan_memory_type_index(
                &memory_properties,
                requirement.memory_requirements.memory_type_bits,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
            )
            .or_else(|| {
                native_vulkan_memory_type_index(
                    &memory_properties,
                    requirement.memory_requirements.memory_type_bits,
                    vk::MemoryPropertyFlags::empty(),
                )
            })
            .ok_or(NativeVulkanError::MissingMemoryType("video session memory"))?;
            let allocation_info = vk::MemoryAllocateInfo::default()
                .allocation_size(requirement.memory_requirements.size)
                .memory_type_index(memory_type_index);
            let memory =
                unsafe { device.allocate_memory(&allocation_info, None) }.map_err(|result| {
                    NativeVulkanError::Vulkan {
                        operation: "vkAllocateMemory(video session)",
                        result,
                    }
                })?;
            allocated_memories.push(memory);
            bind_infos.push(
                vk::BindVideoSessionMemoryInfoKHR::default()
                    .memory_bind_index(requirement.memory_bind_index)
                    .memory(memory)
                    .memory_offset(0)
                    .memory_size(requirement.memory_requirements.size),
            );
            let memory_type = memory_properties.memory_types[memory_type_index as usize];
            memory_snapshots.push(NativeVulkanVideoSessionMemoryRequirementSnapshot {
                memory_bind_index: requirement.memory_bind_index,
                size: requirement.memory_requirements.size,
                alignment: requirement.memory_requirements.alignment,
                memory_type_bits: requirement.memory_requirements.memory_type_bits,
                selected_memory_type_index: memory_type_index,
                selected_memory_property_flags: native_vulkan_memory_property_flag_labels(
                    memory_type.property_flags,
                ),
            });
            total_bound_memory_bytes =
                total_bound_memory_bytes.saturating_add(requirement.memory_requirements.size);
        }

        native_vulkan_bind_video_session_memory(&video_queue_device, session, &bind_infos)?;

        if options.allocate_video_images {
            let image = native_vulkan_create_video_session_resource_image(
                video_queue_loader,
                &device,
                &memory_properties,
                selection.physical_device,
                profile_info,
                requested_extent,
                session_max_dpb_slots.max(1),
                capabilities.decode_capability_flags,
                if options.decode_first_frame {
                    vk::ImageUsageFlags::TRANSFER_SRC
                } else {
                    vk::ImageUsageFlags::empty()
                },
                &device_queue_family_indices,
            )?;
            resource_images.push(image);
        }
        if options.allocate_bitstream_buffer {
            let bitstream_payload = bitstream_extract
                .as_ref()
                .map(|extract| extract.selected_access_unit.as_slice());
            let bitstream_buffer_size = bitstream_payload
                .map(|payload| options.bitstream_buffer_size.max(payload.len() as u64))
                .unwrap_or(options.bitstream_buffer_size);
            bitstream_buffer = Some(native_vulkan_create_video_session_bitstream_buffer(
                &device,
                &memory_properties,
                profile_info,
                bitstream_buffer_size,
                capabilities.min_bitstream_buffer_size_alignment,
                bitstream_payload,
            )?);
        }
        let session_parameters =
            if matches!(options.codec, NativeVulkanVideoSessionCodec::H265Main8) {
                bitstream_extract
                    .as_ref()
                    .and_then(|extract| extract.snapshot.h265_parameter_sets.as_ref())
                    .map(|parameter_sets| {
                        native_vulkan_create_h265_video_session_parameters(
                            &video_queue_device,
                            session,
                            parameter_sets,
                        )
                    })
                    .transpose()?
            } else {
                None
            };
        let session_parameters_snapshot = session_parameters
            .as_ref()
            .map(|parameters| parameters.snapshot.clone());
        if let Some(parameters) = session_parameters.as_ref() {
            video_session_parameters = parameters.parameters;
        }
        let first_frame_decode = if options.decode_first_frame {
            let parameters = session_parameters.as_ref().ok_or_else(|| {
                NativeVulkanError::Video(
                    "--decode-first-frame requires H.265 session parameters".to_owned(),
                )
            })?;
            let image = resource_images.first().ok_or_else(|| {
                NativeVulkanError::Video(
                    "--decode-first-frame requires --allocate-video-images".to_owned(),
                )
            })?;
            let buffer = bitstream_buffer.as_ref().ok_or_else(|| {
                NativeVulkanError::Video(
                    "--decode-first-frame requires --allocate-bitstream-buffer".to_owned(),
                )
            })?;
            let extract = bitstream_extract.as_ref().ok_or_else(|| {
                NativeVulkanError::Video(
                    "--decode-first-frame requires --extract-bitstream".to_owned(),
                )
            })?;
            let parameter_sets =
                extract
                    .snapshot
                    .h265_parameter_sets
                    .as_ref()
                    .ok_or_else(|| {
                        NativeVulkanError::Video(
                            "--decode-first-frame requires parsed H.265 parameter sets".to_owned(),
                        )
                    })?;
            Some(native_vulkan_decode_h265_first_frame_smoke(
                &device,
                &video_queue_device,
                &video_decode_queue_device,
                selection.queue_family_index,
                selection.queue_flags,
                graphics_queue_family_index,
                session,
                parameters.parameters,
                requested_extent,
                capabilities.min_bitstream_buffer_size_alignment,
                &memory_properties,
                image,
                buffer,
                extract,
                parameter_sets,
                options.sample_decoded_first_frame,
            )?)
        } else {
            None
        };
        let video_image_snapshots = resource_images
            .iter()
            .map(|image| image.snapshot.clone())
            .collect::<Vec<_>>();
        let total_video_image_memory_bytes = video_image_snapshots
            .iter()
            .map(|image| image.memory_size)
            .sum::<u64>();
        let bitstream_buffer_snapshot = bitstream_buffer
            .as_ref()
            .map(|buffer| buffer.snapshot.clone());
        let bitstream_extract_snapshot = bitstream_extract
            .as_ref()
            .map(|extract| extract.snapshot.clone());

        Ok(NativeVulkanVideoSessionSmokeSnapshot {
            result: native_vulkan_video_session_smoke_result(&options),
            requested_codec: options.codec,
            requested_extent: (requested_extent.width, requested_extent.height),
            selected_physical_device_index: selection.physical_device_index,
            selected_physical_device_name: selection.physical_device_name.clone(),
            selected_physical_device_type: selection.physical_device_type,
            vendor_id: selection.properties.vendor_id,
            device_id: selection.properties.device_id,
            api_version: native_vulkan_api_version_label(selection.properties.api_version),
            driver_version: selection.properties.driver_version,
            selected_queue_family_index: selection.queue_family_index,
            selected_queue_count: selection.queue_count,
            selected_queue_flags: native_vulkan_queue_flag_labels(selection.queue_flags),
            selected_queue_video_codec_operations: native_vulkan_video_codec_operation_labels(
                selection.video_codec_operations,
            ),
            enabled_device_extensions: enabled_extensions
                .iter()
                .map(|extension| ash_extension_name(extension))
                .collect(),
            video_codec_operation: native_vulkan_video_codec_operation_labels(
                options.codec.codec_operation(),
            ),
            profile: options.codec.profile_label(),
            picture_format: native_vulkan_format_label(picture_format),
            reference_picture_format: native_vulkan_format_label(picture_format),
            nv12_dpb_supported: format_probe.nv12_dpb_supported,
            nv12_output_supported: format_probe.nv12_output_supported,
            nv12_sampled_output_supported: format_probe.nv12_sampled_output_supported,
            capability_flags: native_vulkan_video_capability_flag_labels(
                capabilities.capability_flags,
            ),
            decode_capability_flags: native_vulkan_video_decode_capability_flag_labels(
                capabilities.decode_capability_flags,
            ),
            min_bitstream_buffer_offset_alignment: capabilities
                .min_bitstream_buffer_offset_alignment,
            min_bitstream_buffer_size_alignment: capabilities.min_bitstream_buffer_size_alignment,
            picture_access_granularity: (
                capabilities.picture_access_granularity.width,
                capabilities.picture_access_granularity.height,
            ),
            min_coded_extent: (
                capabilities.min_coded_extent.width,
                capabilities.min_coded_extent.height,
            ),
            max_coded_extent: (
                capabilities.max_coded_extent.width,
                capabilities.max_coded_extent.height,
            ),
            requested_extent_supported: true,
            driver_max_dpb_slots: capabilities.max_dpb_slots,
            driver_max_active_reference_pictures: capabilities.max_active_reference_pictures,
            session_max_dpb_slots,
            session_max_active_reference_pictures,
            codec_max_level: capabilities.codec_max_level.clone(),
            std_header_version_name: native_vulkan_extension_properties_name(
                &capabilities.std_header_version,
            ),
            std_header_version_spec_version: capabilities.std_header_version.spec_version,
            memory_requirement_count: memory_snapshots.len(),
            total_bound_memory_bytes,
            memory_requirements: memory_snapshots,
            video_images_requested: options.allocate_video_images,
            video_image_count: video_image_snapshots.len(),
            total_video_image_memory_bytes,
            video_images: video_image_snapshots,
            bitstream_buffer_requested: options.allocate_bitstream_buffer,
            bitstream_buffer: bitstream_buffer_snapshot,
            bitstream_extract_requested: options.extract_bitstream,
            bitstream_extract: bitstream_extract_snapshot,
            session_parameters_requested: matches!(
                options.codec,
                NativeVulkanVideoSessionCodec::H265Main8
            ) && bitstream_extract
                .as_ref()
                .is_some_and(|extract| extract.snapshot.h265_parameter_sets.is_some()),
            session_parameters_created: session_parameters_snapshot.is_some(),
            session_parameters: session_parameters_snapshot,
            first_frame_decode_requested: options.decode_first_frame,
            first_frame_decode,
            session_created: true,
            session_memory_bound: true,
        })
    })();

    unsafe {
        if video_session_parameters != vk::VideoSessionParametersKHR::null() {
            (video_queue_device.fp().destroy_video_session_parameters_khr)(
                video_queue_device.device(),
                video_session_parameters,
                ptr::null(),
            );
        }
        if let Some(buffer) = bitstream_buffer.as_ref() {
            device.destroy_buffer(buffer.buffer, None);
            device.free_memory(buffer.memory, None);
        }
        for image in resource_images.iter().rev() {
            device.destroy_image_view(image.view, None);
            device.destroy_image(image.image, None);
            device.free_memory(image.memory, None);
        }
        if session != vk::VideoSessionKHR::null() {
            (video_queue_device.fp().destroy_video_session_khr)(
                video_queue_device.device(),
                session,
                ptr::null(),
            );
        }
        for memory in allocated_memories.iter().copied() {
            device.free_memory(memory, None);
        }
        device.destroy_device(None);
    }

    result
}

fn native_vulkan_create_video_session_resource_image(
    video_queue_loader: &ash::khr::video_queue::Instance,
    device: &ash::Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    physical_device: vk::PhysicalDevice,
    profile_info: &vk::VideoProfileInfoKHR<'_>,
    extent: vk::Extent2D,
    array_layers: u32,
    decode_capability_flags: vk::VideoDecodeCapabilityFlagsKHR,
    additional_usage: vk::ImageUsageFlags,
    queue_family_indices: &[u32],
) -> Result<NativeVulkanVideoResourceImage, NativeVulkanError> {
    if !decode_capability_flags.contains(vk::VideoDecodeCapabilityFlagsKHR::DPB_AND_OUTPUT_COINCIDE)
    {
        return Err(NativeVulkanError::Video(
            "video resource smoke currently requires DPB/output coincide".to_owned(),
        ));
    }
    let image_usage = vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR
        | vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR
        | vk::ImageUsageFlags::SAMPLED
        | additional_usage;
    let format = native_vulkan_video_format_properties_raw(
        video_queue_loader,
        physical_device,
        profile_info,
        image_usage,
    )?
    .into_iter()
    .find(|format| {
        format.format == vk::Format::G8_B8R8_2PLANE_420_UNORM
            && format.image_usage_flags.contains(image_usage)
    })
    .ok_or_else(|| {
        NativeVulkanError::Video(
            "NV12 video decode dst+dpb+sampled image format is unavailable".to_owned(),
        )
    })?;

    let mut profile_list_info =
        vk::VideoProfileListInfoKHR::default().profiles(std::slice::from_ref(profile_info));
    let image_extent = vk::Extent3D {
        width: extent.width,
        height: extent.height,
        depth: 1,
    };
    let image_create_info = vk::ImageCreateInfo::default()
        .flags(format.image_create_flags)
        .image_type(format.image_type)
        .format(format.format)
        .extent(image_extent)
        .mip_levels(1)
        .array_layers(array_layers)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(format.image_tiling)
        .usage(image_usage)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .push_next(&mut profile_list_info);
    let image_create_info = if queue_family_indices.len() > 1 {
        image_create_info
            .sharing_mode(vk::SharingMode::CONCURRENT)
            .queue_family_indices(queue_family_indices)
    } else {
        image_create_info.sharing_mode(vk::SharingMode::EXCLUSIVE)
    };
    let image = unsafe { device.create_image(&image_create_info, None) }.map_err(|result| {
        NativeVulkanError::Vulkan {
            operation: "vkCreateImage(video session resource)",
            result,
        }
    })?;

    let mut image_destroyed = false;
    let result = (|| -> Result<NativeVulkanVideoResourceImage, NativeVulkanError> {
        let memory_requirements = unsafe { device.get_image_memory_requirements(image) };
        let memory_type_index = native_vulkan_memory_type_index(
            memory_properties,
            memory_requirements.memory_type_bits,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )
        .or_else(|| {
            native_vulkan_memory_type_index(
                memory_properties,
                memory_requirements.memory_type_bits,
                vk::MemoryPropertyFlags::empty(),
            )
        })
        .ok_or(NativeVulkanError::MissingMemoryType(
            "video session resource image",
        ))?;
        let allocation_info = vk::MemoryAllocateInfo::default()
            .allocation_size(memory_requirements.size)
            .memory_type_index(memory_type_index);
        let memory =
            unsafe { device.allocate_memory(&allocation_info, None) }.map_err(|result| {
                NativeVulkanError::Vulkan {
                    operation: "vkAllocateMemory(video session resource image)",
                    result,
                }
            })?;

        let bind_result = unsafe { device.bind_image_memory(image, memory, 0) };
        if let Err(result) = bind_result {
            unsafe {
                device.free_memory(memory, None);
            }
            return Err(NativeVulkanError::Vulkan {
                operation: "vkBindImageMemory(video session resource)",
                result,
            });
        }

        let view = match native_vulkan_create_video_session_resource_image_view(
            device,
            image,
            format.format,
            image_usage,
        ) {
            Ok(view) => view,
            Err(err) => {
                unsafe {
                    device.destroy_image(image, None);
                    image_destroyed = true;
                    device.free_memory(memory, None);
                }
                return Err(err);
            }
        };

        let memory_type = memory_properties.memory_types[memory_type_index as usize];
        Ok(NativeVulkanVideoResourceImage {
            image,
            memory,
            view,
            snapshot: NativeVulkanVideoSessionResourceImageSnapshot {
                role: "coincident-dpb-output-sampled-nv12",
                format: native_vulkan_format_label(format.format),
                image_type: native_vulkan_image_type_label(format.image_type),
                image_tiling: native_vulkan_image_tiling_label(format.image_tiling),
                image_usage_flags: native_vulkan_image_usage_flag_labels(image_usage),
                image_create_flags: native_vulkan_image_create_flag_labels(
                    format.image_create_flags,
                ),
                extent: (image_extent.width, image_extent.height, image_extent.depth),
                array_layers,
                image_view_type: "2d-array",
                image_view_created: true,
                memory_size: memory_requirements.size,
                memory_alignment: memory_requirements.alignment,
                memory_type_bits: memory_requirements.memory_type_bits,
                selected_memory_type_index: memory_type_index,
                selected_memory_property_flags: native_vulkan_memory_property_flag_labels(
                    memory_type.property_flags,
                ),
            },
        })
    })();

    if result.is_err() && !image_destroyed {
        unsafe {
            device.destroy_image(image, None);
        }
    }
    result
}

fn native_vulkan_create_video_session_resource_image_view(
    device: &ash::Device,
    image: vk::Image,
    format: vk::Format,
    image_usage: vk::ImageUsageFlags,
) -> Result<vk::ImageView, NativeVulkanError> {
    let mut view_usage_info = vk::ImageViewUsageCreateInfo::default().usage(image_usage);
    let subresource_range = vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: vk::REMAINING_ARRAY_LAYERS,
    };
    let create_info = vk::ImageViewCreateInfo::default()
        .image(image)
        .view_type(vk::ImageViewType::TYPE_2D_ARRAY)
        .format(format)
        .subresource_range(subresource_range)
        .push_next(&mut view_usage_info);
    unsafe { device.create_image_view(&create_info, None) }.map_err(|result| {
        NativeVulkanError::Vulkan {
            operation: "vkCreateImageView(video session resource)",
            result,
        }
    })
}

fn native_vulkan_create_video_session_bitstream_buffer(
    device: &ash::Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    profile_info: &vk::VideoProfileInfoKHR<'_>,
    requested_size: u64,
    min_size_alignment: u64,
    write_payload: Option<&[u8]>,
) -> Result<NativeVulkanVideoBitstreamBuffer, NativeVulkanError> {
    let size = native_vulkan_align_up(requested_size.max(1), min_size_alignment.max(1));
    let usage = vk::BufferUsageFlags::VIDEO_DECODE_SRC_KHR;
    let mut profile_list_info =
        vk::VideoProfileListInfoKHR::default().profiles(std::slice::from_ref(profile_info));
    let buffer_create_info = vk::BufferCreateInfo::default()
        .size(size)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .push_next(&mut profile_list_info);
    let buffer = unsafe { device.create_buffer(&buffer_create_info, None) }.map_err(|result| {
        NativeVulkanError::Vulkan {
            operation: "vkCreateBuffer(video bitstream)",
            result,
        }
    })?;

    let mut buffer_destroyed = false;
    let result = (|| -> Result<NativeVulkanVideoBitstreamBuffer, NativeVulkanError> {
        let memory_requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
        let memory_type_index = native_vulkan_memory_type_index(
            memory_properties,
            memory_requirements.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )
        .or_else(|| {
            native_vulkan_memory_type_index(
                memory_properties,
                memory_requirements.memory_type_bits,
                vk::MemoryPropertyFlags::HOST_VISIBLE,
            )
        })
        .ok_or(NativeVulkanError::MissingMemoryType(
            "video bitstream host-visible buffer",
        ))?;
        let memory_type = memory_properties.memory_types[memory_type_index as usize];
        let allocation_info = vk::MemoryAllocateInfo::default()
            .allocation_size(memory_requirements.size)
            .memory_type_index(memory_type_index);
        let memory =
            unsafe { device.allocate_memory(&allocation_info, None) }.map_err(|result| {
                NativeVulkanError::Vulkan {
                    operation: "vkAllocateMemory(video bitstream)",
                    result,
                }
            })?;

        if let Err(result) = unsafe { device.bind_buffer_memory(buffer, memory, 0) } {
            unsafe {
                device.destroy_buffer(buffer, None);
                buffer_destroyed = true;
                device.free_memory(memory, None);
            }
            return Err(NativeVulkanError::Vulkan {
                operation: "vkBindBufferMemory(video bitstream)",
                result,
            });
        }

        let mapped_write_bytes = write_payload
            .map(|payload| payload.len() as u64)
            .unwrap_or_else(|| size.min(256));
        let map_result = unsafe {
            device.map_memory(memory, 0, mapped_write_bytes, vk::MemoryMapFlags::empty())
        };
        let map = match map_result {
            Ok(map) => map,
            Err(result) => {
                unsafe {
                    device.destroy_buffer(buffer, None);
                    buffer_destroyed = true;
                    device.free_memory(memory, None);
                }
                return Err(NativeVulkanError::Vulkan {
                    operation: "vkMapMemory(video bitstream)",
                    result,
                });
            }
        };
        if let Some(payload) = write_payload {
            unsafe {
                ptr::copy_nonoverlapping(payload.as_ptr(), map.cast::<u8>(), payload.len());
            }
        } else {
            unsafe {
                ptr::write_bytes(map.cast::<u8>(), 0, mapped_write_bytes as usize);
            }
        }
        if !memory_type
            .property_flags
            .contains(vk::MemoryPropertyFlags::HOST_COHERENT)
        {
            let range = vk::MappedMemoryRange::default()
                .memory(memory)
                .offset(0)
                .size(mapped_write_bytes);
            if let Err(result) = unsafe { device.flush_mapped_memory_ranges(&[range]) } {
                unsafe {
                    device.unmap_memory(memory);
                    device.destroy_buffer(buffer, None);
                    buffer_destroyed = true;
                    device.free_memory(memory, None);
                }
                return Err(NativeVulkanError::Vulkan {
                    operation: "vkFlushMappedMemoryRanges(video bitstream)",
                    result,
                });
            }
        }
        unsafe {
            device.unmap_memory(memory);
        }

        Ok(NativeVulkanVideoBitstreamBuffer {
            buffer,
            memory,
            snapshot: NativeVulkanVideoSessionBitstreamBufferSnapshot {
                requested_size,
                size,
                min_size_alignment,
                usage_flags: native_vulkan_buffer_usage_flag_labels(usage),
                memory_size: memory_requirements.size,
                memory_alignment: memory_requirements.alignment,
                memory_type_bits: memory_requirements.memory_type_bits,
                selected_memory_type_index: memory_type_index,
                selected_memory_property_flags: native_vulkan_memory_property_flag_labels(
                    memory_type.property_flags,
                ),
                mapped_write_bytes,
                mapped_write_source: if write_payload.is_some() {
                    "extracted-h265-access-unit"
                } else {
                    "zero-fill-smoke-pattern"
                },
                mapped_write_hash: write_payload.map(native_vulkan_stable_byte_hash),
                host_visible: memory_type
                    .property_flags
                    .contains(vk::MemoryPropertyFlags::HOST_VISIBLE),
                host_coherent: memory_type
                    .property_flags
                    .contains(vk::MemoryPropertyFlags::HOST_COHERENT),
            },
        })
    })();

    if result.is_err() && !buffer_destroyed {
        unsafe {
            device.destroy_buffer(buffer, None);
        }
    }
    result
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_create_video_decode_readback_buffer(
    device: &ash::Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    extent: vk::Extent2D,
) -> Result<NativeVulkanVideoDecodeReadbackBuffer, NativeVulkanError> {
    if extent.width == 0
        || extent.height == 0
        || !extent.width.is_multiple_of(2)
        || !extent.height.is_multiple_of(2)
    {
        return Err(NativeVulkanError::Video(format!(
            "NV12 readback requires non-zero even extent, got {}x{}",
            extent.width, extent.height
        )));
    }
    let y_plane_bytes = u64::from(extent.width) * u64::from(extent.height);
    let uv_plane_bytes = y_plane_bytes / 2;
    let size = y_plane_bytes
        .checked_add(uv_plane_bytes)
        .ok_or_else(|| NativeVulkanError::Video("NV12 readback size overflow".to_owned()))?;
    let buffer_create_info = vk::BufferCreateInfo::default()
        .size(size)
        .usage(vk::BufferUsageFlags::TRANSFER_DST)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    let buffer = unsafe { device.create_buffer(&buffer_create_info, None) }.map_err(|result| {
        NativeVulkanError::Vulkan {
            operation: "vkCreateBuffer(h265 decode readback)",
            result,
        }
    })?;

    let mut buffer_destroyed = false;
    let result = (|| -> Result<NativeVulkanVideoDecodeReadbackBuffer, NativeVulkanError> {
        let memory_requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
        let memory_type_index = native_vulkan_memory_type_index_prefer(
            memory_properties,
            memory_requirements.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_CACHED,
            vk::MemoryPropertyFlags::HOST_VISIBLE,
        )
        .ok_or(NativeVulkanError::MissingMemoryType(
            "h265 decode readback buffer",
        ))?;
        let memory_type = memory_properties.memory_types[memory_type_index as usize];
        let allocation_info = vk::MemoryAllocateInfo::default()
            .allocation_size(memory_requirements.size)
            .memory_type_index(memory_type_index);
        let memory =
            unsafe { device.allocate_memory(&allocation_info, None) }.map_err(|result| {
                NativeVulkanError::Vulkan {
                    operation: "vkAllocateMemory(h265 decode readback)",
                    result,
                }
            })?;

        if let Err(result) = unsafe { device.bind_buffer_memory(buffer, memory, 0) } {
            unsafe {
                device.destroy_buffer(buffer, None);
                buffer_destroyed = true;
                device.free_memory(memory, None);
            }
            return Err(NativeVulkanError::Vulkan {
                operation: "vkBindBufferMemory(h265 decode readback)",
                result,
            });
        }

        Ok(NativeVulkanVideoDecodeReadbackBuffer {
            buffer,
            memory,
            memory_size: memory_requirements.size,
            size,
            y_plane_bytes,
            uv_plane_bytes,
            memory_property_flags: memory_type.property_flags,
        })
    })();

    if result.is_err() && !buffer_destroyed {
        unsafe {
            device.destroy_buffer(buffer, None);
        }
    }
    result
}

#[cfg(feature = "native-vulkan-gst-video")]
impl NativeVulkanDecodedSamplingTarget {
    fn new(
        device: &ash::Device,
        memory_properties: &vk::PhysicalDeviceMemoryProperties,
        extent: vk::Extent2D,
    ) -> Result<Self, NativeVulkanError> {
        if extent.width == 0 || extent.height == 0 {
            return Err(NativeVulkanError::Video(format!(
                "decoded sampling target requires non-zero extent, got {}x{}",
                extent.width, extent.height
            )));
        }
        let format = vk::Format::R8G8B8A8_UNORM;
        let total_bytes = u64::from(extent.width)
            .checked_mul(u64::from(extent.height))
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| {
                NativeVulkanError::Video("RGBA sampling readback size overflow".to_owned())
            })?;
        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width: extent.width,
                height: extent.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        let image = unsafe { device.create_image(&image_info, None) }.map_err(|result| {
            NativeVulkanError::Vulkan {
                operation: "vkCreateImage(decoded sampling color target)",
                result,
            }
        })?;

        let result = (|| -> Result<Self, NativeVulkanError> {
            let image_requirements = unsafe { device.get_image_memory_requirements(image) };
            let image_memory_type_index = native_vulkan_memory_type_index_prefer(
                memory_properties,
                image_requirements.memory_type_bits,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
                vk::MemoryPropertyFlags::empty(),
            )
            .ok_or(NativeVulkanError::MissingMemoryType(
                "decoded sampling color target",
            ))?;
            let image_allocate_info = vk::MemoryAllocateInfo::default()
                .allocation_size(image_requirements.size)
                .memory_type_index(image_memory_type_index);
            let memory = unsafe { device.allocate_memory(&image_allocate_info, None) }.map_err(
                |result| NativeVulkanError::Vulkan {
                    operation: "vkAllocateMemory(decoded sampling color target)",
                    result,
                },
            )?;
            if let Err(result) = unsafe { device.bind_image_memory(image, memory, 0) } {
                unsafe {
                    device.free_memory(memory, None);
                }
                return Err(NativeVulkanError::Vulkan {
                    operation: "vkBindImageMemory(decoded sampling color target)",
                    result,
                });
            }

            let view_info = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format)
                .subresource_range(native_vulkan_color_subresource_range());
            let view = match unsafe { device.create_image_view(&view_info, None) } {
                Ok(view) => view,
                Err(result) => {
                    unsafe {
                        device.free_memory(memory, None);
                    }
                    return Err(NativeVulkanError::Vulkan {
                        operation: "vkCreateImageView(decoded sampling color target)",
                        result,
                    });
                }
            };

            let readback_info = vk::BufferCreateInfo::default()
                .size(total_bytes)
                .usage(vk::BufferUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE);
            let readback_buffer = match unsafe { device.create_buffer(&readback_info, None) } {
                Ok(buffer) => buffer,
                Err(result) => {
                    unsafe {
                        device.destroy_image_view(view, None);
                        device.free_memory(memory, None);
                    }
                    return Err(NativeVulkanError::Vulkan {
                        operation: "vkCreateBuffer(decoded sampling readback)",
                        result,
                    });
                }
            };

            let readback_requirements =
                unsafe { device.get_buffer_memory_requirements(readback_buffer) };
            let readback_memory_type_index = match native_vulkan_memory_type_index_prefer(
                memory_properties,
                readback_requirements.memory_type_bits,
                vk::MemoryPropertyFlags::HOST_CACHED,
                vk::MemoryPropertyFlags::HOST_VISIBLE,
            ) {
                Some(index) => index,
                None => {
                    unsafe {
                        device.destroy_buffer(readback_buffer, None);
                        device.destroy_image_view(view, None);
                        device.free_memory(memory, None);
                    }
                    return Err(NativeVulkanError::MissingMemoryType(
                        "decoded sampling readback buffer",
                    ));
                }
            };
            let readback_memory_type =
                memory_properties.memory_types[readback_memory_type_index as usize];
            let readback_allocate_info = vk::MemoryAllocateInfo::default()
                .allocation_size(readback_requirements.size)
                .memory_type_index(readback_memory_type_index);
            let readback_memory =
                match unsafe { device.allocate_memory(&readback_allocate_info, None) } {
                    Ok(memory) => memory,
                    Err(result) => {
                        unsafe {
                            device.destroy_buffer(readback_buffer, None);
                            device.destroy_image_view(view, None);
                            device.free_memory(memory, None);
                        }
                        return Err(NativeVulkanError::Vulkan {
                            operation: "vkAllocateMemory(decoded sampling readback)",
                            result,
                        });
                    }
                };
            if let Err(result) =
                unsafe { device.bind_buffer_memory(readback_buffer, readback_memory, 0) }
            {
                unsafe {
                    device.free_memory(readback_memory, None);
                    device.destroy_buffer(readback_buffer, None);
                    device.destroy_image_view(view, None);
                    device.free_memory(memory, None);
                }
                return Err(NativeVulkanError::Vulkan {
                    operation: "vkBindBufferMemory(decoded sampling readback)",
                    result,
                });
            }

            Ok(Self {
                image,
                memory,
                view,
                readback_buffer,
                readback_memory,
                extent,
                format,
                total_bytes,
                color_memory_size: image_requirements.size,
                readback_memory_size: readback_requirements.size,
                readback_memory_property_flags: readback_memory_type.property_flags,
            })
        })();

        if result.is_err() {
            unsafe {
                device.destroy_image(image, None);
            }
        }
        result
    }

    fn destroy(self, device: &ash::Device) {
        unsafe {
            device.destroy_buffer(self.readback_buffer, None);
            device.free_memory(self.readback_memory, None);
            device.destroy_image_view(self.view, None);
            device.destroy_image(self.image, None);
            device.free_memory(self.memory, None);
        }
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_sample_decoded_video_output(
    device: &ash::Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    queue: vk::Queue,
    command_pool: vk::CommandPool,
    extent: vk::Extent2D,
    decoded_image: vk::Image,
    wait_semaphore: Option<vk::Semaphore>,
) -> Result<NativeVulkanVideoDecodeOutputSamplingSnapshot, NativeVulkanError> {
    let command_buffer_info = vk::CommandBufferAllocateInfo::default()
        .command_pool(command_pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(1);
    let command_buffer =
        unsafe { device.allocate_command_buffers(&command_buffer_info) }.map_err(|result| {
            NativeVulkanError::Vulkan {
                operation: "vkAllocateCommandBuffers(decoded sampling)",
                result,
            }
        })?[0];

    let mut texture = None::<NativeVulkanVideoTexture>;
    let mut target = None::<NativeVulkanDecodedSamplingTarget>;
    let mut renderer = None::<NativeVulkanVideoRenderer>;
    let result =
        (|| -> Result<NativeVulkanVideoDecodeOutputSamplingSnapshot, NativeVulkanError> {
            texture = Some(NativeVulkanVideoTexture::Decoded(
                NativeVulkanDecodedVideoTexture::new(
                    device,
                    decoded_image,
                    extent.width,
                    extent.height,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    vk::AccessFlags::TRANSFER_READ,
                )?,
            ));
            target = Some(NativeVulkanDecodedSamplingTarget::new(
                device,
                memory_properties,
                extent,
            )?);
            let target_ref = target
                .as_ref()
                .expect("decoded sampling target must exist after create");
            renderer = Some(NativeVulkanVideoRenderer::new_with_target_final_layout(
                device,
                target_ref.format,
                extent,
                &[target_ref.view],
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            )?);
            let texture_ref = texture
                .as_ref()
                .expect("decoded sampling texture must exist after create");
            let renderer_ref = renderer
                .as_mut()
                .expect("decoded sampling renderer must exist after create");
            renderer_ref.update_descriptors(device, texture_ref);
            renderer_ref.record_frame(
                device,
                command_buffer,
                0,
                target_ref.image,
                vk::ImageLayout::UNDEFINED,
                texture_ref,
                FitMode::Stretch,
            )?;
            native_vulkan_submit_command_buffer_and_wait(
                device,
                queue,
                command_buffer,
                "decoded sampling render",
                wait_semaphore,
            )?;
            native_vulkan_record_decoded_sampling_readback_commands(
                device,
                command_buffer,
                target_ref,
            )?;
            native_vulkan_submit_command_buffer_and_wait(
                device,
                queue,
                command_buffer,
                "decoded sampling readback",
                None,
            )?;
            native_vulkan_read_decoded_sampling_snapshot(device, target_ref)
        })();

    unsafe {
        if let Some(renderer) = renderer.take() {
            renderer.destroy(device);
        }
        if let Some(texture) = texture.take() {
            texture.destroy(device);
        }
        if let Some(target) = target.take() {
            target.destroy(device);
        }
        device.free_command_buffers(command_pool, &[command_buffer]);
    }
    result
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_submit_command_buffer_and_wait(
    device: &ash::Device,
    queue: vk::Queue,
    command_buffer: vk::CommandBuffer,
    label: &'static str,
    wait_semaphore: Option<vk::Semaphore>,
) -> Result<(), NativeVulkanError> {
    let command_buffers = [command_buffer];
    let wait_semaphores = wait_semaphore
        .iter()
        .copied()
        .collect::<Vec<vk::Semaphore>>();
    let wait_stages = wait_semaphore
        .map(|_| vk::PipelineStageFlags::ALL_COMMANDS)
        .into_iter()
        .collect::<Vec<_>>();
    let submit_info = vk::SubmitInfo::default()
        .wait_semaphores(&wait_semaphores)
        .wait_dst_stage_mask(&wait_stages)
        .command_buffers(&command_buffers);
    unsafe {
        device
            .queue_submit(queue, &[submit_info], vk::Fence::null())
            .map_err(|result| NativeVulkanError::Vulkan {
                operation: match label {
                    "decoded sampling render" => "vkQueueSubmit(decoded sampling render)",
                    "decoded sampling readback" => "vkQueueSubmit(decoded sampling readback)",
                    _ => "vkQueueSubmit(decoded sampling)",
                },
                result,
            })?;
        device
            .queue_wait_idle(queue)
            .map_err(|result| NativeVulkanError::Vulkan {
                operation: match label {
                    "decoded sampling render" => "vkQueueWaitIdle(decoded sampling render)",
                    "decoded sampling readback" => "vkQueueWaitIdle(decoded sampling readback)",
                    _ => "vkQueueWaitIdle(decoded sampling)",
                },
                result,
            })
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_record_decoded_sampling_readback_commands(
    device: &ash::Device,
    command_buffer: vk::CommandBuffer,
    target: &NativeVulkanDecodedSamplingTarget,
) -> Result<(), NativeVulkanError> {
    unsafe {
        device
            .reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty())
            .map_err(|result| NativeVulkanError::Vulkan {
                operation: "vkResetCommandBuffer(decoded sampling readback)",
                result,
            })?;
        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        device
            .begin_command_buffer(command_buffer, &begin_info)
            .map_err(|result| NativeVulkanError::Vulkan {
                operation: "vkBeginCommandBuffer(decoded sampling readback)",
                result,
            })?;
        let copy = vk::BufferImageCopy::default()
            .buffer_offset(0)
            .buffer_row_length(0)
            .buffer_image_height(0)
            .image_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
            .image_extent(vk::Extent3D {
                width: target.extent.width,
                height: target.extent.height,
                depth: 1,
            });
        device.cmd_copy_image_to_buffer(
            command_buffer,
            target.image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            target.readback_buffer,
            &[copy],
        );
        let buffer_barrier = vk::BufferMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
            .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::HOST)
            .dst_access_mask(vk::AccessFlags2::HOST_READ)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(target.readback_buffer)
            .offset(0)
            .size(vk::WHOLE_SIZE);
        let buffer_barriers = [buffer_barrier];
        let dependency_info =
            vk::DependencyInfo::default().buffer_memory_barriers(&buffer_barriers);
        device.cmd_pipeline_barrier2(command_buffer, &dependency_info);
        device
            .end_command_buffer(command_buffer)
            .map_err(|result| NativeVulkanError::Vulkan {
                operation: "vkEndCommandBuffer(decoded sampling readback)",
                result,
            })?;
    }
    Ok(())
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_read_decoded_sampling_snapshot(
    device: &ash::Device,
    target: &NativeVulkanDecodedSamplingTarget,
) -> Result<NativeVulkanVideoDecodeOutputSamplingSnapshot, NativeVulkanError> {
    let map = unsafe {
        device.map_memory(
            target.readback_memory,
            0,
            target.total_bytes,
            vk::MemoryMapFlags::empty(),
        )
    }
    .map_err(|result| NativeVulkanError::Vulkan {
        operation: "vkMapMemory(decoded sampling readback)",
        result,
    })?;

    let result =
        (|| -> Result<NativeVulkanVideoDecodeOutputSamplingSnapshot, NativeVulkanError> {
            if !target
                .readback_memory_property_flags
                .contains(vk::MemoryPropertyFlags::HOST_COHERENT)
            {
                let range = vk::MappedMemoryRange::default()
                    .memory(target.readback_memory)
                    .offset(0)
                    .size(vk::WHOLE_SIZE);
                unsafe { device.invalidate_mapped_memory_ranges(&[range]) }.map_err(|result| {
                    NativeVulkanError::Vulkan {
                        operation: "vkInvalidateMappedMemoryRanges(decoded sampling readback)",
                        result,
                    }
                })?;
            }
            let bytes = unsafe {
                std::slice::from_raw_parts(map.cast::<u8>(), target.total_bytes as usize)
            };
            let summary = native_vulkan_byte_summary(bytes);
            Ok(NativeVulkanVideoDecodeOutputSamplingSnapshot {
                source_format: "G8_B8R8_2PLANE_420_UNORM",
                target_format: native_vulkan_format_label(target.format),
                source_layout: "transfer-src-optimal",
                shader_layout: "shader-read-only-optimal",
                render_extent: (target.extent.width, target.extent.height),
                y_plane_view_created: true,
                uv_plane_view_created: true,
                color_image_created: true,
                color_image_view_created: true,
                renderer_created: true,
                command_buffer_recorded: true,
                rendered: true,
                copied: true,
                host_visible: target
                    .readback_memory_property_flags
                    .contains(vk::MemoryPropertyFlags::HOST_VISIBLE),
                host_coherent: target
                    .readback_memory_property_flags
                    .contains(vk::MemoryPropertyFlags::HOST_COHERENT),
                host_cached: target
                    .readback_memory_property_flags
                    .contains(vk::MemoryPropertyFlags::HOST_CACHED),
                color_image_memory_size: target.color_memory_size,
                readback_memory_size: target.readback_memory_size,
                total_bytes: target.total_bytes,
                rgba_hash: summary.hash,
                rgba_nonzero_bytes: summary.nonzero_bytes,
                rgba_min: summary.min,
                rgba_max: summary.max,
                rgba_unique_values: summary.unique_values,
            })
        })();

    unsafe {
        device.unmap_memory(target.readback_memory);
    }
    result
}

#[cfg(feature = "native-vulkan-gst-video")]
#[allow(clippy::too_many_arguments)]
fn native_vulkan_decode_h265_first_frame_smoke(
    device: &ash::Device,
    video_queue_device: &ash::khr::video_queue::Device,
    video_decode_queue_device: &ash::khr::video_decode_queue::Device,
    queue_family_index: u32,
    _queue_flags: vk::QueueFlags,
    graphics_queue_family_index: Option<u32>,
    session: vk::VideoSessionKHR,
    session_parameters: vk::VideoSessionParametersKHR,
    extent: vk::Extent2D,
    min_bitstream_buffer_size_alignment: u64,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    image: &NativeVulkanVideoResourceImage,
    buffer: &NativeVulkanVideoBitstreamBuffer,
    extract: &NativeVulkanVideoBitstreamExtract,
    parameter_sets: &NativeVulkanH265ParameterSetSnapshot,
    sample_decoded_output: bool,
) -> Result<NativeVulkanVideoFirstFrameDecodeSnapshot, NativeVulkanError> {
    if session_parameters == vk::VideoSessionParametersKHR::null() {
        return Err(NativeVulkanError::Video(
            "H.265 first-frame decode requires VkVideoSessionParametersKHR".to_owned(),
        ));
    }
    if image.snapshot.array_layers == 0 {
        return Err(NativeVulkanError::Video(
            "H.265 first-frame decode requires at least one DPB/output image layer".to_owned(),
        ));
    }
    let first_slice =
        native_vulkan_h265_first_slice_decode_info(&extract.selected_access_unit, parameter_sets)
            .map_err(NativeVulkanError::Video)?;
    if !first_slice.idr {
        return Err(NativeVulkanError::Video(format!(
            "H.265 first-frame decode currently supports IDR only, got {}",
            first_slice.nal_type_label
        )));
    }

    let src_buffer_range = native_vulkan_align_up(
        extract.selected_access_unit.len() as u64,
        min_bitstream_buffer_size_alignment.max(1),
    );
    if src_buffer_range > buffer.snapshot.size {
        return Err(NativeVulkanError::Video(format!(
            "H.265 first-frame decode needs {src_buffer_range} bytes but bitstream buffer has {} bytes",
            buffer.snapshot.size
        )));
    }

    let queue = unsafe { device.get_device_queue(queue_family_index, 0) };
    let command_pool_info = vk::CommandPoolCreateInfo::default()
        .flags(
            vk::CommandPoolCreateFlags::TRANSIENT
                | vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER,
        )
        .queue_family_index(queue_family_index);
    let command_pool =
        unsafe { device.create_command_pool(&command_pool_info, None) }.map_err(|result| {
            NativeVulkanError::Vulkan {
                operation: "vkCreateCommandPool(h265 first-frame decode)",
                result,
            }
        })?;
    let mut readback_buffer = None::<NativeVulkanVideoDecodeReadbackBuffer>;
    let mut sampling_command_pool = None::<vk::CommandPool>;
    let mut sampling_ready = None::<vk::Semaphore>;

    let result = (|| -> Result<NativeVulkanVideoFirstFrameDecodeSnapshot, NativeVulkanError> {
        let sampling_queue_family_index = if sample_decoded_output {
            Some(graphics_queue_family_index.ok_or_else(|| {
                NativeVulkanError::Video(
                    "decoded first-frame sampling requires a graphics queue family".to_owned(),
                )
            })?)
        } else {
            None
        };
        if let Some(sampling_queue_family_index) = sampling_queue_family_index {
            let pool_info = vk::CommandPoolCreateInfo::default()
                .flags(
                    vk::CommandPoolCreateFlags::TRANSIENT
                        | vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER,
                )
                .queue_family_index(sampling_queue_family_index);
            sampling_command_pool = Some(
                unsafe { device.create_command_pool(&pool_info, None) }.map_err(|result| {
                    NativeVulkanError::Vulkan {
                        operation: "vkCreateCommandPool(decoded sampling)",
                        result,
                    }
                })?,
            );
            sampling_ready = Some(
                unsafe { device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None) }
                    .map_err(|result| NativeVulkanError::Vulkan {
                        operation: "vkCreateSemaphore(decoded sampling ready)",
                        result,
                    })?,
            );
        }
        readback_buffer = Some(native_vulkan_create_video_decode_readback_buffer(
            device,
            memory_properties,
            extent,
        )?);
        let readback = readback_buffer
            .as_ref()
            .expect("readback buffer was just created");
        let command_buffer_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        let command_buffer = unsafe { device.allocate_command_buffers(&command_buffer_info) }
            .map_err(|result| NativeVulkanError::Vulkan {
                operation: "vkAllocateCommandBuffers(h265 first-frame decode)",
                result,
            })?[0];

        let begin_resources = (0..image.snapshot.array_layers)
            .map(|layer| native_vulkan_video_picture_resource_info(image.view, extent, layer))
            .collect::<Vec<_>>();
        let begin_reference_slots = begin_resources
            .iter()
            .map(|resource| {
                vk::VideoReferenceSlotInfoKHR::default()
                    .picture_resource(resource)
                    .slot_index(-1)
            })
            .collect::<Vec<_>>();
        let dst_picture_resource = native_vulkan_video_picture_resource_info(image.view, extent, 0);
        let setup_picture_resource = dst_picture_resource;
        let std_reference_info = vk::native::StdVideoDecodeH265ReferenceInfo {
            flags: vk::native::StdVideoDecodeH265ReferenceInfoFlags {
                _bitfield_align_1: [],
                _bitfield_1: vk::native::StdVideoDecodeH265ReferenceInfoFlags::new_bitfield_1(0, 0),
                __bindgen_padding_0: [0; 3],
            },
            PicOrderCntVal: first_slice.pic_order_cnt_val,
        };
        let mut setup_h265_slot_info =
            vk::VideoDecodeH265DpbSlotInfoKHR::default().std_reference_info(&std_reference_info);
        let setup_reference_slot = vk::VideoReferenceSlotInfoKHR::default()
            .picture_resource(&setup_picture_resource)
            .slot_index(0)
            .push_next(&mut setup_h265_slot_info);
        let std_picture_info = vk::native::StdVideoDecodeH265PictureInfo {
            flags: vk::native::StdVideoDecodeH265PictureInfoFlags {
                _bitfield_align_1: [],
                _bitfield_1: vk::native::StdVideoDecodeH265PictureInfoFlags::new_bitfield_1(
                    native_vulkan_bool_u32(first_slice.irap),
                    native_vulkan_bool_u32(first_slice.idr),
                    1,
                    0,
                ),
                __bindgen_padding_0: [0; 3],
            },
            sps_video_parameter_set_id: parameter_sets.sps.vps_id,
            pps_seq_parameter_set_id: native_vulkan_h265_u8(
                parameter_sets.pps.sps_id,
                "pps_seq_parameter_set_id",
            )
            .map_err(NativeVulkanError::Video)?,
            pps_pic_parameter_set_id: native_vulkan_h265_u8(
                parameter_sets.pps.id,
                "pps_pic_parameter_set_id",
            )
            .map_err(NativeVulkanError::Video)?,
            NumDeltaPocsOfRefRpsIdx: 0,
            PicOrderCntVal: first_slice.pic_order_cnt_val,
            NumBitsForSTRefPicSetInSlice: 0,
            reserved: 0,
            RefPicSetStCurrBefore: [0xff; 8],
            RefPicSetStCurrAfter: [0xff; 8],
            RefPicSetLtCurr: [0xff; 8],
        };
        let slice_segment_offsets = vec![first_slice.slice_segment_offset];
        let mut h265_picture_info = vk::VideoDecodeH265PictureInfoKHR::default()
            .std_picture_info(&std_picture_info)
            .slice_segment_offsets(&slice_segment_offsets);
        let begin_info = vk::VideoBeginCodingInfoKHR::default()
            .video_session(session)
            .video_session_parameters(session_parameters)
            .reference_slots(&begin_reference_slots);
        let control_info =
            vk::VideoCodingControlInfoKHR::default().flags(vk::VideoCodingControlFlagsKHR::RESET);
        let decode_info = vk::VideoDecodeInfoKHR::default()
            .src_buffer(buffer.buffer)
            .src_buffer_offset(0)
            .src_buffer_range(src_buffer_range)
            .dst_picture_resource(dst_picture_resource)
            .setup_reference_slot(&setup_reference_slot)
            .push_next(&mut h265_picture_info);

        unsafe {
            let command_begin_info = vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            device
                .begin_command_buffer(command_buffer, &command_begin_info)
                .map_err(|result| NativeVulkanError::Vulkan {
                    operation: "vkBeginCommandBuffer(h265 first-frame decode)",
                    result,
                })?;
            native_vulkan_video_first_frame_decode_barriers(
                device,
                command_buffer,
                image.image,
                buffer.buffer,
                src_buffer_range,
            );
            (video_queue_device.fp().cmd_begin_video_coding_khr)(command_buffer, &begin_info);
            (video_queue_device.fp().cmd_control_video_coding_khr)(command_buffer, &control_info);
            (video_decode_queue_device.fp().cmd_decode_video_khr)(command_buffer, &decode_info);
            (video_queue_device.fp().cmd_end_video_coding_khr)(
                command_buffer,
                &vk::VideoEndCodingInfoKHR::default(),
            );
            native_vulkan_video_first_frame_readback_commands(
                device,
                command_buffer,
                image.image,
                readback.buffer,
                extent,
                readback.y_plane_bytes,
            );
            device
                .end_command_buffer(command_buffer)
                .map_err(|result| NativeVulkanError::Vulkan {
                    operation: "vkEndCommandBuffer(h265 first-frame decode)",
                    result,
                })?;
            let command_buffers = [command_buffer];
            let signal_semaphores = sampling_ready.iter().copied().collect::<Vec<_>>();
            let submit_info = vk::SubmitInfo::default()
                .command_buffers(&command_buffers)
                .signal_semaphores(&signal_semaphores);
            device
                .queue_submit(queue, &[submit_info], vk::Fence::null())
                .map_err(|result| NativeVulkanError::Vulkan {
                    operation: "vkQueueSubmit(h265 first-frame decode)",
                    result,
                })?;
            device
                .queue_wait_idle(queue)
                .map_err(|result| NativeVulkanError::Vulkan {
                    operation: "vkQueueWaitIdle(h265 first-frame decode)",
                    result,
                })?;
        }
        let output_readback = native_vulkan_read_video_decode_output_snapshot(device, readback)?;
        let output_sampling = if sample_decoded_output {
            let sampling_queue_family_index = sampling_queue_family_index
                .expect("sampling queue family index must exist when sampling is requested");
            let sampling_queue = unsafe { device.get_device_queue(sampling_queue_family_index, 0) };
            let sampling_command_pool = sampling_command_pool
                .expect("sampling command pool must exist when sampling is requested");
            Some(native_vulkan_sample_decoded_video_output(
                device,
                memory_properties,
                sampling_queue,
                sampling_command_pool,
                extent,
                image.image,
                sampling_ready,
            )?)
        } else {
            None
        };

        Ok(NativeVulkanVideoFirstFrameDecodeSnapshot {
            codec: "h265-main-8",
            command_pool_created: true,
            command_buffer_allocated: true,
            command_buffer_recorded: true,
            submitted: true,
            completed: true,
            queue_family_index,
            source_layout: "undefined",
            decode_layout: "video-decode-dpb",
            src_buffer_offset: 0,
            src_buffer_range,
            dst_base_array_layer: 0,
            setup_slot_index: 0,
            begin_reference_slot_count: begin_reference_slots.len() as u32,
            decode_reference_slot_count: 0,
            reset_control_recorded: true,
            slice_segment_count: slice_segment_offsets.len() as u32,
            slice_segment_offsets,
            nal_type: first_slice.nal_type,
            nal_type_label: first_slice.nal_type_label,
            first_slice_segment_in_pic_flag: first_slice.first_slice_segment_in_pic_flag,
            slice_type: first_slice.slice_type,
            pps_id: first_slice.pps_id,
            sps_video_parameter_set_id: parameter_sets.sps.vps_id,
            pps_seq_parameter_set_id: std_picture_info.pps_seq_parameter_set_id,
            pps_pic_parameter_set_id: std_picture_info.pps_pic_parameter_set_id,
            pic_order_cnt_val: first_slice.pic_order_cnt_val,
            idr: first_slice.idr,
            irap: first_slice.irap,
            output_readback: Some(output_readback),
            output_sampling,
        })
    })();

    unsafe {
        if let Some(readback) = readback_buffer.as_ref() {
            device.destroy_buffer(readback.buffer, None);
            device.free_memory(readback.memory, None);
        }
        if let Some(semaphore) = sampling_ready {
            device.destroy_semaphore(semaphore, None);
        }
        if let Some(command_pool) = sampling_command_pool {
            device.destroy_command_pool(command_pool, None);
        }
        device.destroy_command_pool(command_pool, None);
    }

    result
}

#[cfg(not(feature = "native-vulkan-gst-video"))]
#[allow(clippy::too_many_arguments)]
fn native_vulkan_decode_h265_first_frame_smoke(
    _device: &ash::Device,
    _video_queue_device: &ash::khr::video_queue::Device,
    _video_decode_queue_device: &ash::khr::video_decode_queue::Device,
    _queue_family_index: u32,
    _queue_flags: vk::QueueFlags,
    _graphics_queue_family_index: Option<u32>,
    _session: vk::VideoSessionKHR,
    _session_parameters: vk::VideoSessionParametersKHR,
    _extent: vk::Extent2D,
    _min_bitstream_buffer_size_alignment: u64,
    _memory_properties: &vk::PhysicalDeviceMemoryProperties,
    _image: &NativeVulkanVideoResourceImage,
    _buffer: &NativeVulkanVideoBitstreamBuffer,
    _extract: &NativeVulkanVideoBitstreamExtract,
    _parameter_sets: &NativeVulkanH265ParameterSetSnapshot,
    _sample_decoded_output: bool,
) -> Result<NativeVulkanVideoFirstFrameDecodeSnapshot, NativeVulkanError> {
    Err(NativeVulkanError::Video(
        "--decode-first-frame requires the native-vulkan-gst-video feature".to_owned(),
    ))
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_video_picture_resource_info(
    image_view: vk::ImageView,
    extent: vk::Extent2D,
    base_array_layer: u32,
) -> vk::VideoPictureResourceInfoKHR<'static> {
    vk::VideoPictureResourceInfoKHR::default()
        .coded_offset(vk::Offset2D { x: 0, y: 0 })
        .coded_extent(extent)
        .base_array_layer(base_array_layer)
        .image_view_binding(image_view)
}

#[cfg(feature = "native-vulkan-gst-video")]
unsafe fn native_vulkan_video_first_frame_decode_barriers(
    device: &ash::Device,
    command_buffer: vk::CommandBuffer,
    image: vk::Image,
    buffer: vk::Buffer,
    buffer_size: u64,
) {
    let buffer_barrier = vk::BufferMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::HOST)
        .src_access_mask(vk::AccessFlags2::HOST_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::VIDEO_DECODE_KHR)
        .dst_access_mask(vk::AccessFlags2::VIDEO_DECODE_READ_KHR)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .buffer(buffer)
        .offset(0)
        .size(buffer_size);
    let image_barrier = vk::ImageMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::TOP_OF_PIPE)
        .src_access_mask(vk::AccessFlags2::empty())
        .dst_stage_mask(vk::PipelineStageFlags2::VIDEO_DECODE_KHR)
        .dst_access_mask(
            vk::AccessFlags2::VIDEO_DECODE_READ_KHR | vk::AccessFlags2::VIDEO_DECODE_WRITE_KHR,
        )
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(vk::ImageLayout::VIDEO_DECODE_DPB_KHR)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(native_vulkan_color_subresource_range());
    let buffer_barriers = [buffer_barrier];
    let image_barriers = [image_barrier];
    let dependency_info = vk::DependencyInfo::default()
        .buffer_memory_barriers(&buffer_barriers)
        .image_memory_barriers(&image_barriers);
    unsafe {
        device.cmd_pipeline_barrier2(command_buffer, &dependency_info);
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
unsafe fn native_vulkan_video_first_frame_readback_commands(
    device: &ash::Device,
    command_buffer: vk::CommandBuffer,
    image: vk::Image,
    buffer: vk::Buffer,
    extent: vk::Extent2D,
    y_plane_bytes: u64,
) {
    let image_barrier = vk::ImageMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::VIDEO_DECODE_KHR)
        .src_access_mask(vk::AccessFlags2::VIDEO_DECODE_WRITE_KHR)
        .dst_stage_mask(vk::PipelineStageFlags2::TRANSFER)
        .dst_access_mask(vk::AccessFlags2::TRANSFER_READ)
        .old_layout(vk::ImageLayout::VIDEO_DECODE_DPB_KHR)
        .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(native_vulkan_color_subresource_range());
    let image_barriers = [image_barrier];
    let image_dependency = vk::DependencyInfo::default().image_memory_barriers(&image_barriers);
    unsafe {
        device.cmd_pipeline_barrier2(command_buffer, &image_dependency);
    }

    let regions = [
        vk::BufferImageCopy::default()
            .buffer_offset(0)
            .buffer_row_length(0)
            .buffer_image_height(0)
            .image_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::PLANE_0,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
            .image_extent(vk::Extent3D {
                width: extent.width,
                height: extent.height,
                depth: 1,
            }),
        vk::BufferImageCopy::default()
            .buffer_offset(y_plane_bytes)
            .buffer_row_length(0)
            .buffer_image_height(0)
            .image_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::PLANE_1,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
            .image_extent(vk::Extent3D {
                width: extent.width / 2,
                height: extent.height / 2,
                depth: 1,
            }),
    ];
    unsafe {
        device.cmd_copy_image_to_buffer(
            command_buffer,
            image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            buffer,
            &regions,
        );
    }

    let buffer_barrier = vk::BufferMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
        .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::HOST)
        .dst_access_mask(vk::AccessFlags2::HOST_READ)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .buffer(buffer)
        .offset(0)
        .size(vk::WHOLE_SIZE);
    let buffer_barriers = [buffer_barrier];
    let buffer_dependency = vk::DependencyInfo::default().buffer_memory_barriers(&buffer_barriers);
    unsafe {
        device.cmd_pipeline_barrier2(command_buffer, &buffer_dependency);
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_read_video_decode_output_snapshot(
    device: &ash::Device,
    readback: &NativeVulkanVideoDecodeReadbackBuffer,
) -> Result<NativeVulkanVideoDecodeOutputReadbackSnapshot, NativeVulkanError> {
    let map = unsafe {
        device.map_memory(
            readback.memory,
            0,
            readback.size,
            vk::MemoryMapFlags::empty(),
        )
    }
    .map_err(|result| NativeVulkanError::Vulkan {
        operation: "vkMapMemory(h265 decode readback)",
        result,
    })?;

    let result =
        (|| -> Result<NativeVulkanVideoDecodeOutputReadbackSnapshot, NativeVulkanError> {
            if !readback
                .memory_property_flags
                .contains(vk::MemoryPropertyFlags::HOST_COHERENT)
            {
                let range = vk::MappedMemoryRange::default()
                    .memory(readback.memory)
                    .offset(0)
                    .size(vk::WHOLE_SIZE);
                unsafe { device.invalidate_mapped_memory_ranges(&[range]) }.map_err(|result| {
                    NativeVulkanError::Vulkan {
                        operation: "vkInvalidateMappedMemoryRanges(h265 decode readback)",
                        result,
                    }
                })?;
            }

            let bytes =
                unsafe { std::slice::from_raw_parts(map.cast::<u8>(), readback.size as usize) };
            let y_len = readback.y_plane_bytes as usize;
            let uv_len = readback.uv_plane_bytes as usize;
            let y_bytes = bytes.get(..y_len).ok_or_else(|| {
                NativeVulkanError::Video("H.265 decode readback missing Y plane".to_owned())
            })?;
            let uv_bytes = bytes.get(y_len..y_len + uv_len).ok_or_else(|| {
                NativeVulkanError::Video("H.265 decode readback missing UV plane".to_owned())
            })?;
            let y_summary = native_vulkan_byte_summary(y_bytes);
            let uv_summary = native_vulkan_byte_summary(uv_bytes);
            Ok(NativeVulkanVideoDecodeOutputReadbackSnapshot {
                format: "G8_B8R8_2PLANE_420_UNORM",
                buffer_created: true,
                copied: true,
                host_visible: readback
                    .memory_property_flags
                    .contains(vk::MemoryPropertyFlags::HOST_VISIBLE),
                host_coherent: readback
                    .memory_property_flags
                    .contains(vk::MemoryPropertyFlags::HOST_COHERENT),
                host_cached: readback
                    .memory_property_flags
                    .contains(vk::MemoryPropertyFlags::HOST_CACHED),
                memory_size: readback.memory_size,
                total_bytes: readback.size,
                y_plane_bytes: readback.y_plane_bytes,
                uv_plane_bytes: readback.uv_plane_bytes,
                y_plane_hash: y_summary.hash,
                uv_plane_hash: uv_summary.hash,
                combined_hash: native_vulkan_stable_byte_hash(bytes),
                y_plane_nonzero_bytes: y_summary.nonzero_bytes,
                uv_plane_nonzero_bytes: uv_summary.nonzero_bytes,
                y_plane_min: y_summary.min,
                y_plane_max: y_summary.max,
                uv_plane_min: uv_summary.min,
                uv_plane_max: uv_summary.max,
                y_plane_unique_values: y_summary.unique_values,
                uv_plane_unique_values: uv_summary.unique_values,
            })
        })();

    unsafe {
        device.unmap_memory(readback.memory);
    }
    result
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
struct NativeVulkanByteSummary {
    hash: u64,
    nonzero_bytes: u64,
    min: u8,
    max: u8,
    unique_values: u32,
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
fn native_vulkan_byte_summary(bytes: &[u8]) -> NativeVulkanByteSummary {
    let mut seen = [false; 256];
    let mut nonzero_bytes = 0u64;
    let mut min = u8::MAX;
    let mut max = u8::MIN;
    for byte in bytes.iter().copied() {
        seen[byte as usize] = true;
        if byte != 0 {
            nonzero_bytes = nonzero_bytes.saturating_add(1);
        }
        min = min.min(byte);
        max = max.max(byte);
    }
    NativeVulkanByteSummary {
        hash: native_vulkan_stable_byte_hash(bytes),
        nonzero_bytes,
        min: if bytes.is_empty() { 0 } else { min },
        max: if bytes.is_empty() { 0 } else { max },
        unique_values: seen.into_iter().filter(|value| *value).count() as u32,
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_extract_video_bitstream(
    options: &NativeVulkanVideoSessionSmokeOptions,
) -> Result<NativeVulkanVideoBitstreamExtract, NativeVulkanError> {
    let source = options.bitstream_source.as_deref().ok_or_else(|| {
        NativeVulkanError::Video("--extract-bitstream requires --source".to_owned())
    })?;
    if !source.is_file() {
        return Err(NativeVulkanError::Video(format!(
            "bitstream source does not exist: {}",
            source.display()
        )));
    }
    match options.codec {
        NativeVulkanVideoSessionCodec::H265Main8 => native_vulkan_extract_h265_bitstream(
            source,
            options.bitstream_extract_max_samples.max(1),
        ),
        NativeVulkanVideoSessionCodec::Av1Main8 => Err(NativeVulkanError::Video(
            "AV1 bitstream extraction is not implemented yet; use H.265 for the first Vulkan Video decode path".to_owned(),
        )),
    }
}

#[cfg(not(feature = "native-vulkan-gst-video"))]
fn native_vulkan_extract_video_bitstream(
    _options: &NativeVulkanVideoSessionSmokeOptions,
) -> Result<NativeVulkanVideoBitstreamExtract, NativeVulkanError> {
    Err(NativeVulkanError::Video(
        "--extract-bitstream requires the native-vulkan-gst-video feature".to_owned(),
    ))
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_extract_h265_bitstream(
    source: &Path,
    max_samples: u32,
) -> Result<NativeVulkanVideoBitstreamExtract, NativeVulkanError> {
    gst::init().map_err(|err| NativeVulkanError::Video(err.to_string()))?;
    let pipeline = native_vulkan_h265_bitstream_pipeline(source)?;
    let sink = pipeline
        .by_name("gilder-native-vulkan-h265-bitstream-appsink")
        .ok_or_else(|| NativeVulkanError::Video("H.265 bitstream appsink not found".to_owned()))?;
    let bus = pipeline.bus().ok_or_else(|| {
        NativeVulkanError::Video("H.265 bitstream pipeline has no bus".to_owned())
    })?;

    let result = (|| -> Result<NativeVulkanVideoBitstreamExtract, NativeVulkanError> {
        pipeline
            .set_state(gst::State::Playing)
            .map_err(|err| NativeVulkanError::Video(err.to_string()))?;
        native_vulkan_collect_h265_bitstream_samples(source, &sink, &bus, max_samples)
    })();

    let _ = pipeline.set_state(gst::State::Null);
    let _ = pipeline.state(gst::ClockTime::from_mseconds(500));
    result
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_h265_bitstream_pipeline(
    source: &Path,
) -> Result<gst::Pipeline, NativeVulkanError> {
    let pipeline = gst::Pipeline::new();
    let filesrc = native_vulkan_gst_element("filesrc")?;
    filesrc.set_property("location", source.to_string_lossy().as_ref());
    let demux = native_vulkan_gst_element("qtdemux")?;
    let queue = native_vulkan_gst_element("queue")?;
    native_vulkan_configure_queue(&queue);
    let parser = native_vulkan_gst_element("h265parse")?;
    if parser.find_property("config-interval").is_some() {
        parser.set_property("config-interval", -1i32);
    }
    if parser.find_property("disable-passthrough").is_some() {
        parser.set_property("disable-passthrough", true);
    }
    let capsfilter = native_vulkan_gst_element("capsfilter")?;
    let caps = "video/x-h265,stream-format=byte-stream,alignment=au"
        .parse::<gst::Caps>()
        .map_err(|err| NativeVulkanError::Video(err.to_string()))?;
    capsfilter.set_property("caps", &caps);
    let sink = native_vulkan_gst_element("appsink")?;
    sink.set_property("name", "gilder-native-vulkan-h265-bitstream-appsink");
    native_vulkan_configure_bitstream_appsink(&sink);

    pipeline
        .add_many([&filesrc, &demux, &queue, &parser, &capsfilter, &sink])
        .map_err(|err| NativeVulkanError::Video(err.to_string()))?;
    filesrc
        .link(&demux)
        .map_err(|err| NativeVulkanError::Video(err.to_string()))?;
    gst::Element::link_many([&queue, &parser, &capsfilter, &sink])
        .map_err(|err| NativeVulkanError::Video(err.to_string()))?;

    let queue_sink = queue.static_pad("sink").ok_or_else(|| {
        NativeVulkanError::Video("H.265 bitstream queue has no sink pad".to_owned())
    })?;
    demux.connect_pad_added(move |_, pad| {
        if queue_sink.is_linked() || !native_vulkan_gst_pad_is_h265(pad) {
            return;
        }
        let _ = pad.link(&queue_sink);
    });

    Ok(pipeline)
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_configure_bitstream_appsink(sink: &gst::Element) {
    if sink.find_property("sync").is_some() {
        sink.set_property("sync", false);
    }
    if sink.find_property("async").is_some() {
        sink.set_property("async", false);
    }
    if sink.find_property("emit-signals").is_some() {
        sink.set_property("emit-signals", false);
    }
    if sink.find_property("enable-last-sample").is_some() {
        sink.set_property("enable-last-sample", false);
    }
    if sink.find_property("wait-on-eos").is_some() {
        sink.set_property("wait-on-eos", false);
    }
    if sink.find_property("max-buffers").is_some() {
        sink.set_property("max-buffers", 1u32);
    }
    if sink.find_property("drop").is_some() {
        sink.set_property("drop", false);
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_pad_is_h265(pad: &gst::Pad) -> bool {
    pad.current_caps()
        .or_else(|| Some(pad.query_caps(None)))
        .and_then(|caps| {
            caps.structure(0)
                .map(|structure| structure.name() == "video/x-h265")
        })
        .unwrap_or(false)
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_collect_h265_bitstream_samples(
    source: &Path,
    sink: &gst::Element,
    bus: &gst::Bus,
    max_samples: u32,
) -> Result<NativeVulkanVideoBitstreamExtract, NativeVulkanError> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut sample_count = 0u32;
    let mut total_bytes = 0u64;
    let mut selected = None::<NativeVulkanH265AccessUnitExtract>;
    let mut last_error = None::<String>;

    while sample_count < max_samples && Instant::now() < deadline {
        while let Some(message) = bus.pop() {
            match message.view() {
                gst::MessageView::Error(err) => {
                    let mut message = format!(
                        "{}: {}",
                        err.src()
                            .map(|src| src.path_string())
                            .unwrap_or_else(|| "gstreamer".into()),
                        err.error()
                    );
                    if let Some(debug) = err.debug() {
                        message.push_str(": ");
                        message.push_str(&debug);
                    }
                    return Err(NativeVulkanError::Video(message));
                }
                gst::MessageView::Eos(_) => {
                    last_error = Some("H.265 bitstream pipeline reached EOS".to_owned());
                    break;
                }
                _ => {}
            }
        }

        let timeout_ns = 50_000_000u64;
        let sample = sink.emit_by_name::<Option<gst::Sample>>("try-pull-sample", &[&timeout_ns]);
        let Some(sample) = sample else {
            continue;
        };
        sample_count = sample_count.saturating_add(1);
        let access_unit = native_vulkan_h265_access_unit_from_sample(&sample)?;
        total_bytes = total_bytes.saturating_add(access_unit.bytes.len() as u64);
        if selected
            .as_ref()
            .map(|current| {
                !current.stats.parameter_sets_present()
                    && access_unit.stats.parameter_sets_present()
            })
            .unwrap_or(true)
        {
            selected = Some(access_unit);
        }
        if selected
            .as_ref()
            .is_some_and(|access_unit| access_unit.stats.parameter_sets_present())
        {
            break;
        }
    }

    let selected = selected.ok_or_else(|| {
        NativeVulkanError::Video(
            last_error.unwrap_or_else(|| "H.265 bitstream probe produced no samples".to_owned()),
        )
    })?;
    if !selected.stats.parameter_sets_present() {
        return Err(NativeVulkanError::Video(format!(
            "H.265 bitstream probe did not find VPS/SPS/PPS in {sample_count} samples"
        )));
    }
    let parameter_sets = native_vulkan_parse_h265_parameter_sets(&selected.bytes)
        .map_err(NativeVulkanError::Video)?;

    Ok(NativeVulkanVideoBitstreamExtract {
        selected_access_unit: selected.bytes,
        snapshot: NativeVulkanVideoBitstreamExtractSnapshot {
            source: source.display().to_string(),
            frontend: "gstreamer-qtdemux-h265parse-appsink",
            requested_max_samples: max_samples,
            samples: sample_count,
            total_bytes,
            selected_access_unit_bytes: selected.stats.bytes,
            selected_access_unit_pts_ms: selected.pts_ms,
            selected_access_unit_duration_ms: selected.duration_ms,
            caps: selected.caps,
            stream_format: selected.stream_format,
            alignment: selected.alignment,
            width: selected.width,
            height: selected.height,
            framerate: selected.framerate,
            has_annex_b_start_codes: selected.stats.has_annex_b_start_codes,
            h265_vps_count: selected.stats.vps_count,
            h265_sps_count: selected.stats.sps_count,
            h265_pps_count: selected.stats.pps_count,
            h265_idr_count: selected.stats.idr_count,
            h265_slice_count: selected.stats.slice_count,
            h265_parameter_sets_present: selected.stats.parameter_sets_present(),
            h265_parameter_sets: Some(parameter_sets),
            h265_nal_units: selected.stats.nal_units,
        },
    })
}

#[cfg(feature = "native-vulkan-gst-video")]
struct NativeVulkanH265AccessUnitExtract {
    bytes: Vec<u8>,
    pts_ms: Option<u64>,
    duration_ms: Option<u64>,
    caps: Option<String>,
    stream_format: Option<String>,
    alignment: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    framerate: Option<String>,
    stats: NativeVulkanH265NalStats,
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_h265_access_unit_from_sample(
    sample: &gst::Sample,
) -> Result<NativeVulkanH265AccessUnitExtract, NativeVulkanError> {
    let buffer = sample.buffer().ok_or_else(|| {
        NativeVulkanError::Video("H.265 bitstream sample has no buffer".to_owned())
    })?;
    let map = buffer.map_readable().map_err(|_| {
        NativeVulkanError::Video("H.265 bitstream buffer map_readable failed".to_owned())
    })?;
    let bytes = map.as_slice().to_vec();
    if bytes.is_empty() {
        return Err(NativeVulkanError::Video(
            "H.265 bitstream sample is empty".to_owned(),
        ));
    }
    let stats = native_vulkan_h265_nal_stats(&bytes);
    let mut stream_format = None;
    let mut alignment = None;
    let mut width = None;
    let mut height = None;
    let mut framerate = None;
    let caps = sample.caps().map(|caps| {
        if let Some(structure) = caps.structure(0) {
            stream_format = structure.get::<String>("stream-format").ok();
            alignment = structure.get::<String>("alignment").ok();
            width = structure
                .get::<i32>("width")
                .ok()
                .and_then(|width| u32::try_from(width).ok());
            height = structure
                .get::<i32>("height")
                .ok()
                .and_then(|height| u32::try_from(height).ok());
            framerate = structure
                .get::<gst::Fraction>("framerate")
                .ok()
                .map(|value| value.to_string());
        }
        caps.to_string()
    });

    Ok(NativeVulkanH265AccessUnitExtract {
        bytes,
        pts_ms: native_vulkan_clock_time_ms(buffer.pts()),
        duration_ms: native_vulkan_clock_time_ms(buffer.duration()),
        caps,
        stream_format,
        alignment,
        width,
        height,
        framerate,
        stats,
    })
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
fn native_vulkan_parse_h265_parameter_sets(
    access_unit: &[u8],
) -> Result<NativeVulkanH265ParameterSetSnapshot, String> {
    let nal_units = native_vulkan_h265_nal_payloads(access_unit);
    let vps_payload = nal_units
        .iter()
        .find(|unit| unit.nal_type == 32)
        .ok_or_else(|| "H.265 access unit has no VPS NAL".to_owned())?;
    let sps_payload = nal_units
        .iter()
        .find(|unit| unit.nal_type == 33)
        .ok_or_else(|| "H.265 access unit has no SPS NAL".to_owned())?;
    let pps_payload = nal_units
        .iter()
        .find(|unit| unit.nal_type == 34)
        .ok_or_else(|| "H.265 access unit has no PPS NAL".to_owned())?;

    let vps = native_vulkan_parse_h265_vps(vps_payload.payload)?;
    let sps = native_vulkan_parse_h265_sps(sps_payload.payload)?;
    let pps = native_vulkan_parse_h265_pps(pps_payload.payload)?;
    let requested_profile_compatible = sps.chroma_format_idc == 1
        && !sps.separate_colour_plane_flag
        && sps.bit_depth_luma_minus8 == 0
        && sps.bit_depth_chroma_minus8 == 0
        && sps.profile.main_compatible();
    let vulkan_std_session_parameters_ready = requested_profile_compatible
        && vps.id == sps.vps_id
        && sps.id == pps.sps_id
        && sps.num_short_term_ref_pic_sets == 0
        && !sps.long_term_ref_pics_present_flag
        && !sps.scaling_list_enabled_flag
        && !sps.sps_scaling_list_data_present_flag
        && !sps.pcm_enabled_flag
        && !sps.sps_extension_present_flag
        && !sps
            .vui
            .as_ref()
            .is_some_and(|vui| vui.vui_hrd_parameters_present_flag)
        && !pps.tiles_enabled_flag
        && !pps.pps_scaling_list_data_present_flag
        && !pps.pps_extension_present_flag;

    Ok(NativeVulkanH265ParameterSetSnapshot {
        parser: "native-rust-h265-vps-sps-pps",
        vps: NativeVulkanH265VpsSnapshot {
            id: vps.id,
            max_layers_minus1: vps.max_layers_minus1,
            max_sub_layers_minus1: vps.max_sub_layers_minus1,
            temporal_id_nesting_flag: vps.temporal_id_nesting_flag,
            sub_layer_ordering_info_present_flag: vps
                .dec_pic_buf_mgr
                .sub_layer_ordering_info_present_flag,
            profile_idc: vps.profile.profile_idc,
            profile_label: native_vulkan_h265_profile_idc_label(vps.profile.profile_idc),
            tier_flag: vps.profile.tier_flag,
            progressive_source_flag: vps.profile.progressive_source_flag,
            interlaced_source_flag: vps.profile.interlaced_source_flag,
            non_packed_constraint_flag: vps.profile.non_packed_constraint_flag,
            frame_only_constraint_flag: vps.profile.frame_only_constraint_flag,
            level_idc: vps.profile.level_idc,
            level_label: native_vulkan_h265_level_idc_byte_label(vps.profile.level_idc),
            dec_pic_buf_mgr: native_vulkan_h265_dec_pic_buf_mgr_snapshot(&vps.dec_pic_buf_mgr),
            timing_info_present_flag: vps.timing_info_present_flag,
            poc_proportional_to_timing_flag: vps.poc_proportional_to_timing_flag,
            num_units_in_tick: vps.num_units_in_tick,
            time_scale: vps.time_scale,
            num_ticks_poc_diff_one_minus1: vps.num_ticks_poc_diff_one_minus1,
        },
        sps: NativeVulkanH265SpsSnapshot {
            id: sps.id,
            vps_id: sps.vps_id,
            max_sub_layers_minus1: sps.max_sub_layers_minus1,
            temporal_id_nesting_flag: sps.temporal_id_nesting_flag,
            sub_layer_ordering_info_present_flag: sps
                .dec_pic_buf_mgr
                .sub_layer_ordering_info_present_flag,
            profile_idc: sps.profile.profile_idc,
            profile_label: native_vulkan_h265_profile_idc_label(sps.profile.profile_idc),
            tier_flag: sps.profile.tier_flag,
            progressive_source_flag: sps.profile.progressive_source_flag,
            interlaced_source_flag: sps.profile.interlaced_source_flag,
            non_packed_constraint_flag: sps.profile.non_packed_constraint_flag,
            frame_only_constraint_flag: sps.profile.frame_only_constraint_flag,
            level_idc: sps.profile.level_idc,
            level_label: native_vulkan_h265_level_idc_byte_label(sps.profile.level_idc),
            dec_pic_buf_mgr: native_vulkan_h265_dec_pic_buf_mgr_snapshot(&sps.dec_pic_buf_mgr),
            chroma_format_idc: sps.chroma_format_idc,
            chroma_format_label: native_vulkan_h265_chroma_format_label(sps.chroma_format_idc),
            separate_colour_plane_flag: sps.separate_colour_plane_flag,
            width: sps.width,
            height: sps.height,
            conformance_window_flag: sps.conformance_window_flag,
            conf_win_left_offset: sps.conf_win_left_offset,
            conf_win_right_offset: sps.conf_win_right_offset,
            conf_win_top_offset: sps.conf_win_top_offset,
            conf_win_bottom_offset: sps.conf_win_bottom_offset,
            bit_depth_luma_minus8: sps.bit_depth_luma_minus8,
            bit_depth_chroma_minus8: sps.bit_depth_chroma_minus8,
            log2_max_pic_order_cnt_lsb_minus4: sps.log2_max_pic_order_cnt_lsb_minus4,
            log2_min_luma_coding_block_size_minus3: sps.log2_min_luma_coding_block_size_minus3,
            log2_diff_max_min_luma_coding_block_size: sps.log2_diff_max_min_luma_coding_block_size,
            log2_min_luma_transform_block_size_minus2: sps
                .log2_min_luma_transform_block_size_minus2,
            log2_diff_max_min_luma_transform_block_size: sps
                .log2_diff_max_min_luma_transform_block_size,
            max_transform_hierarchy_depth_inter: sps.max_transform_hierarchy_depth_inter,
            max_transform_hierarchy_depth_intra: sps.max_transform_hierarchy_depth_intra,
            scaling_list_enabled_flag: sps.scaling_list_enabled_flag,
            sps_scaling_list_data_present_flag: sps.sps_scaling_list_data_present_flag,
            amp_enabled_flag: sps.amp_enabled_flag,
            sample_adaptive_offset_enabled_flag: sps.sample_adaptive_offset_enabled_flag,
            pcm_enabled_flag: sps.pcm_enabled_flag,
            pcm_loop_filter_disabled_flag: sps.pcm_loop_filter_disabled_flag,
            num_short_term_ref_pic_sets: sps.num_short_term_ref_pic_sets,
            long_term_ref_pics_present_flag: sps.long_term_ref_pics_present_flag,
            temporal_mvp_enabled_flag: sps.temporal_mvp_enabled_flag,
            strong_intra_smoothing_enabled_flag: sps.strong_intra_smoothing_enabled_flag,
            vui_parameters_present_flag: sps.vui_parameters_present_flag,
            vui: sps.vui.as_ref().map(native_vulkan_h265_vui_snapshot),
            sps_extension_present_flag: sps.sps_extension_present_flag,
        },
        pps: NativeVulkanH265PpsSnapshot {
            id: pps.id,
            sps_id: pps.sps_id,
            dependent_slice_segments_enabled_flag: pps.dependent_slice_segments_enabled_flag,
            output_flag_present_flag: pps.output_flag_present_flag,
            num_extra_slice_header_bits: pps.num_extra_slice_header_bits,
            sign_data_hiding_enabled_flag: pps.sign_data_hiding_enabled_flag,
            cabac_init_present_flag: pps.cabac_init_present_flag,
            num_ref_idx_l0_default_active_minus1: pps.num_ref_idx_l0_default_active_minus1,
            num_ref_idx_l1_default_active_minus1: pps.num_ref_idx_l1_default_active_minus1,
            init_qp_minus26: pps.init_qp_minus26,
            constrained_intra_pred_flag: pps.constrained_intra_pred_flag,
            transform_skip_enabled_flag: pps.transform_skip_enabled_flag,
            cu_qp_delta_enabled_flag: pps.cu_qp_delta_enabled_flag,
            diff_cu_qp_delta_depth: pps.diff_cu_qp_delta_depth,
            cb_qp_offset: pps.cb_qp_offset,
            cr_qp_offset: pps.cr_qp_offset,
            slice_chroma_qp_offsets_present_flag: pps.slice_chroma_qp_offsets_present_flag,
            weighted_pred_flag: pps.weighted_pred_flag,
            weighted_bipred_flag: pps.weighted_bipred_flag,
            transquant_bypass_enabled_flag: pps.transquant_bypass_enabled_flag,
            tiles_enabled_flag: pps.tiles_enabled_flag,
            entropy_coding_sync_enabled_flag: pps.entropy_coding_sync_enabled_flag,
            uniform_spacing_flag: pps.uniform_spacing_flag,
            num_tile_columns_minus1: pps.num_tile_columns_minus1,
            num_tile_rows_minus1: pps.num_tile_rows_minus1,
            loop_filter_across_tiles_enabled_flag: pps.loop_filter_across_tiles_enabled_flag,
            loop_filter_across_slices_enabled_flag: pps.loop_filter_across_slices_enabled_flag,
            deblocking_filter_control_present_flag: pps.deblocking_filter_control_present_flag,
            deblocking_filter_override_enabled_flag: pps.deblocking_filter_override_enabled_flag,
            pps_deblocking_filter_disabled_flag: pps.pps_deblocking_filter_disabled_flag,
            pps_beta_offset_div2: pps.pps_beta_offset_div2,
            pps_tc_offset_div2: pps.pps_tc_offset_div2,
            pps_scaling_list_data_present_flag: pps.pps_scaling_list_data_present_flag,
            lists_modification_present_flag: pps.lists_modification_present_flag,
            log2_parallel_merge_level_minus2: pps.log2_parallel_merge_level_minus2,
            slice_segment_header_extension_present_flag: pps
                .slice_segment_header_extension_present_flag,
            pps_extension_present_flag: pps.pps_extension_present_flag,
        },
        requested_profile_compatible,
        vulkan_std_session_parameters_ready,
    })
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
fn native_vulkan_h265_dec_pic_buf_mgr_snapshot(
    dec_pic_buf_mgr: &NativeVulkanH265ParsedDecPicBufMgr,
) -> NativeVulkanH265DecPicBufMgrSnapshot {
    NativeVulkanH265DecPicBufMgrSnapshot {
        max_latency_increase_plus1: dec_pic_buf_mgr.max_latency_increase_plus1,
        max_dec_pic_buffering_minus1: dec_pic_buf_mgr.max_dec_pic_buffering_minus1,
        max_num_reorder_pics: dec_pic_buf_mgr.max_num_reorder_pics,
    }
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
fn native_vulkan_h265_vui_snapshot(vui: &NativeVulkanH265ParsedVui) -> NativeVulkanH265VuiSnapshot {
    NativeVulkanH265VuiSnapshot {
        aspect_ratio_info_present_flag: vui.aspect_ratio_info_present_flag,
        aspect_ratio_idc: vui.aspect_ratio_idc,
        sar_width: vui.sar_width,
        sar_height: vui.sar_height,
        overscan_info_present_flag: vui.overscan_info_present_flag,
        overscan_appropriate_flag: vui.overscan_appropriate_flag,
        video_signal_type_present_flag: vui.video_signal_type_present_flag,
        video_format: vui.video_format,
        video_full_range_flag: vui.video_full_range_flag,
        colour_description_present_flag: vui.colour_description_present_flag,
        colour_primaries: vui.colour_primaries,
        transfer_characteristics: vui.transfer_characteristics,
        matrix_coeffs: vui.matrix_coeffs,
        chroma_loc_info_present_flag: vui.chroma_loc_info_present_flag,
        chroma_sample_loc_type_top_field: vui.chroma_sample_loc_type_top_field,
        chroma_sample_loc_type_bottom_field: vui.chroma_sample_loc_type_bottom_field,
        neutral_chroma_indication_flag: vui.neutral_chroma_indication_flag,
        field_seq_flag: vui.field_seq_flag,
        frame_field_info_present_flag: vui.frame_field_info_present_flag,
        default_display_window_flag: vui.default_display_window_flag,
        def_disp_win_left_offset: vui.def_disp_win_left_offset,
        def_disp_win_right_offset: vui.def_disp_win_right_offset,
        def_disp_win_top_offset: vui.def_disp_win_top_offset,
        def_disp_win_bottom_offset: vui.def_disp_win_bottom_offset,
        vui_timing_info_present_flag: vui.vui_timing_info_present_flag,
        vui_num_units_in_tick: vui.vui_num_units_in_tick,
        vui_time_scale: vui.vui_time_scale,
        vui_poc_proportional_to_timing_flag: vui.vui_poc_proportional_to_timing_flag,
        vui_num_ticks_poc_diff_one_minus1: vui.vui_num_ticks_poc_diff_one_minus1,
        vui_hrd_parameters_present_flag: vui.vui_hrd_parameters_present_flag,
        bitstream_restriction_flag: vui.bitstream_restriction_flag,
        tiles_fixed_structure_flag: vui.tiles_fixed_structure_flag,
        motion_vectors_over_pic_boundaries_flag: vui.motion_vectors_over_pic_boundaries_flag,
        restricted_ref_pic_lists_flag: vui.restricted_ref_pic_lists_flag,
        min_spatial_segmentation_idc: vui.min_spatial_segmentation_idc,
        max_bytes_per_pic_denom: vui.max_bytes_per_pic_denom,
        max_bits_per_min_cu_denom: vui.max_bits_per_min_cu_denom,
        log2_max_mv_length_horizontal: vui.log2_max_mv_length_horizontal,
        log2_max_mv_length_vertical: vui.log2_max_mv_length_vertical,
    }
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
#[derive(Debug, Clone, Copy)]
struct NativeVulkanH265NalPayload<'a> {
    nal_type: u8,
    start_code_offset: usize,
    payload: &'a [u8],
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
fn native_vulkan_h265_nal_payloads(bytes: &[u8]) -> Vec<NativeVulkanH265NalPayload<'_>> {
    let mut payloads = Vec::new();
    let mut offset = 0usize;
    while let Some((start_code_offset, payload_offset)) =
        native_vulkan_next_annex_b_start_code(bytes, offset)
    {
        let next_start = native_vulkan_next_annex_b_start_code(bytes, payload_offset)
            .map(|(next_start, _)| next_start)
            .unwrap_or(bytes.len());
        if payload_offset < next_start
            && let Some(nal_type) = bytes.get(payload_offset).map(|header| (header >> 1) & 0x3f)
        {
            payloads.push(NativeVulkanH265NalPayload {
                nal_type,
                start_code_offset,
                payload: &bytes[payload_offset..next_start],
            });
        }
        offset = next_start;
    }
    payloads
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeVulkanH265FirstSliceDecodeInfo {
    nal_type: u8,
    nal_type_label: &'static str,
    slice_segment_offset: u32,
    first_slice_segment_in_pic_flag: bool,
    slice_type: u32,
    pps_id: u32,
    pic_order_cnt_val: i32,
    idr: bool,
    irap: bool,
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
fn native_vulkan_h265_first_slice_decode_info(
    access_unit: &[u8],
    parameter_sets: &NativeVulkanH265ParameterSetSnapshot,
) -> Result<NativeVulkanH265FirstSliceDecodeInfo, String> {
    let nal_units = native_vulkan_h265_nal_payloads(access_unit);
    let slice = nal_units
        .iter()
        .find(|unit| unit.nal_type <= 31)
        .ok_or_else(|| "H.265 access unit has no slice NAL".to_owned())?;
    let idr = matches!(slice.nal_type, 19 | 20);
    let irap = (16..=23).contains(&slice.nal_type);
    let rbsp = native_vulkan_h265_rbsp(slice.payload)?;
    if rbsp.len() < 3 {
        return Err("H.265 slice NAL is too short".to_owned());
    }
    let mut bits = NativeVulkanH265BitReader::new(&rbsp);
    bits.skip_bits(16, "h265_nal_unit_header")?;
    let first_slice_segment_in_pic_flag = bits.read_bool("first_slice_segment_in_pic_flag")?;
    if irap {
        bits.read_bool("no_output_of_prior_pics_flag")?;
    }
    let pps_id = bits.read_ue("slice_pic_parameter_set_id")?;
    if pps_id != parameter_sets.pps.id {
        return Err(format!(
            "H.265 first slice PPS id {pps_id} does not match session PPS id {}",
            parameter_sets.pps.id
        ));
    }
    if !first_slice_segment_in_pic_flag {
        return Err(
            "H.265 first-frame decode currently expects first_slice_segment_in_pic_flag".to_owned(),
        );
    }
    for _ in 0..parameter_sets.pps.num_extra_slice_header_bits {
        bits.skip_bits(1, "slice_reserved_flag")?;
    }
    let slice_type = bits.read_ue("slice_type")?;
    if !idr {
        return Err(format!(
            "H.265 first-frame decode currently supports IDR only, got {}",
            native_vulkan_h265_nal_type_label(slice.nal_type)
        ));
    }
    if slice_type != vk::native::StdVideoH265SliceType_STD_VIDEO_H265_SLICE_TYPE_I {
        return Err(format!(
            "H.265 IDR first slice must be I-slice for the first decode subset, got {slice_type}"
        ));
    }

    Ok(NativeVulkanH265FirstSliceDecodeInfo {
        nal_type: slice.nal_type,
        nal_type_label: native_vulkan_h265_nal_type_label(slice.nal_type),
        slice_segment_offset: u32::try_from(slice.start_code_offset)
            .map_err(|_| "H.265 slice offset exceeds u32 range".to_owned())?,
        first_slice_segment_in_pic_flag,
        slice_type,
        pps_id,
        pic_order_cnt_val: 0,
        idr,
        irap,
    })
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeVulkanH265ParsedVps {
    id: u8,
    max_layers_minus1: u8,
    max_sub_layers_minus1: u8,
    temporal_id_nesting_flag: bool,
    dec_pic_buf_mgr: NativeVulkanH265ParsedDecPicBufMgr,
    profile: NativeVulkanH265ParsedProfileTierLevel,
    timing_info_present_flag: bool,
    poc_proportional_to_timing_flag: bool,
    num_units_in_tick: Option<u32>,
    time_scale: Option<u32>,
    num_ticks_poc_diff_one_minus1: Option<u32>,
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeVulkanH265ParsedDecPicBufMgr {
    sub_layer_ordering_info_present_flag: bool,
    max_latency_increase_plus1: [u32; 7],
    max_dec_pic_buffering_minus1: [u8; 7],
    max_num_reorder_pics: [u8; 7],
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeVulkanH265ParsedSps {
    id: u32,
    vps_id: u8,
    max_sub_layers_minus1: u8,
    temporal_id_nesting_flag: bool,
    dec_pic_buf_mgr: NativeVulkanH265ParsedDecPicBufMgr,
    profile: NativeVulkanH265ParsedProfileTierLevel,
    chroma_format_idc: u32,
    separate_colour_plane_flag: bool,
    width: u32,
    height: u32,
    conformance_window_flag: bool,
    conf_win_left_offset: u32,
    conf_win_right_offset: u32,
    conf_win_top_offset: u32,
    conf_win_bottom_offset: u32,
    bit_depth_luma_minus8: u32,
    bit_depth_chroma_minus8: u32,
    log2_max_pic_order_cnt_lsb_minus4: u32,
    log2_min_luma_coding_block_size_minus3: u32,
    log2_diff_max_min_luma_coding_block_size: u32,
    log2_min_luma_transform_block_size_minus2: u32,
    log2_diff_max_min_luma_transform_block_size: u32,
    max_transform_hierarchy_depth_inter: u32,
    max_transform_hierarchy_depth_intra: u32,
    scaling_list_enabled_flag: bool,
    sps_scaling_list_data_present_flag: bool,
    amp_enabled_flag: bool,
    sample_adaptive_offset_enabled_flag: bool,
    pcm_enabled_flag: bool,
    pcm_loop_filter_disabled_flag: bool,
    num_short_term_ref_pic_sets: u32,
    long_term_ref_pics_present_flag: bool,
    temporal_mvp_enabled_flag: bool,
    strong_intra_smoothing_enabled_flag: bool,
    vui_parameters_present_flag: bool,
    vui: Option<NativeVulkanH265ParsedVui>,
    sps_extension_present_flag: bool,
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeVulkanH265ParsedVui {
    aspect_ratio_info_present_flag: bool,
    aspect_ratio_idc: u32,
    sar_width: u16,
    sar_height: u16,
    overscan_info_present_flag: bool,
    overscan_appropriate_flag: bool,
    video_signal_type_present_flag: bool,
    video_format: u8,
    video_full_range_flag: bool,
    colour_description_present_flag: bool,
    colour_primaries: u8,
    transfer_characteristics: u8,
    matrix_coeffs: u8,
    chroma_loc_info_present_flag: bool,
    chroma_sample_loc_type_top_field: u8,
    chroma_sample_loc_type_bottom_field: u8,
    neutral_chroma_indication_flag: bool,
    field_seq_flag: bool,
    frame_field_info_present_flag: bool,
    default_display_window_flag: bool,
    def_disp_win_left_offset: u16,
    def_disp_win_right_offset: u16,
    def_disp_win_top_offset: u16,
    def_disp_win_bottom_offset: u16,
    vui_timing_info_present_flag: bool,
    vui_num_units_in_tick: u32,
    vui_time_scale: u32,
    vui_poc_proportional_to_timing_flag: bool,
    vui_num_ticks_poc_diff_one_minus1: u32,
    vui_hrd_parameters_present_flag: bool,
    bitstream_restriction_flag: bool,
    tiles_fixed_structure_flag: bool,
    motion_vectors_over_pic_boundaries_flag: bool,
    restricted_ref_pic_lists_flag: bool,
    min_spatial_segmentation_idc: u16,
    max_bytes_per_pic_denom: u8,
    max_bits_per_min_cu_denom: u8,
    log2_max_mv_length_horizontal: u8,
    log2_max_mv_length_vertical: u8,
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeVulkanH265ParsedPps {
    id: u32,
    sps_id: u32,
    dependent_slice_segments_enabled_flag: bool,
    output_flag_present_flag: bool,
    num_extra_slice_header_bits: u8,
    sign_data_hiding_enabled_flag: bool,
    cabac_init_present_flag: bool,
    num_ref_idx_l0_default_active_minus1: u32,
    num_ref_idx_l1_default_active_minus1: u32,
    init_qp_minus26: i32,
    constrained_intra_pred_flag: bool,
    transform_skip_enabled_flag: bool,
    cu_qp_delta_enabled_flag: bool,
    diff_cu_qp_delta_depth: Option<u32>,
    cb_qp_offset: i32,
    cr_qp_offset: i32,
    slice_chroma_qp_offsets_present_flag: bool,
    weighted_pred_flag: bool,
    weighted_bipred_flag: bool,
    transquant_bypass_enabled_flag: bool,
    tiles_enabled_flag: bool,
    entropy_coding_sync_enabled_flag: bool,
    uniform_spacing_flag: bool,
    num_tile_columns_minus1: u32,
    num_tile_rows_minus1: u32,
    loop_filter_across_tiles_enabled_flag: Option<bool>,
    loop_filter_across_slices_enabled_flag: bool,
    deblocking_filter_control_present_flag: bool,
    deblocking_filter_override_enabled_flag: Option<bool>,
    pps_deblocking_filter_disabled_flag: Option<bool>,
    pps_beta_offset_div2: i32,
    pps_tc_offset_div2: i32,
    pps_scaling_list_data_present_flag: bool,
    lists_modification_present_flag: bool,
    log2_parallel_merge_level_minus2: u32,
    slice_segment_header_extension_present_flag: bool,
    pps_extension_present_flag: bool,
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeVulkanH265ParsedProfileTierLevel {
    profile_idc: u8,
    tier_flag: bool,
    progressive_source_flag: bool,
    interlaced_source_flag: bool,
    non_packed_constraint_flag: bool,
    frame_only_constraint_flag: bool,
    profile_compatibility_flags: [bool; 32],
    level_idc: u8,
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
impl NativeVulkanH265ParsedProfileTierLevel {
    fn main_compatible(&self) -> bool {
        self.profile_idc == 1 || self.profile_compatibility_flags[1]
    }
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
fn native_vulkan_parse_h265_dec_pic_buf_mgr(
    bits: &mut NativeVulkanH265BitReader<'_>,
    max_sub_layers_minus1: u8,
    label_prefix: &'static str,
) -> Result<NativeVulkanH265ParsedDecPicBufMgr, String> {
    let sub_layer_ordering_info_present_flag =
        bits.read_bool("sub_layer_ordering_info_present_flag")?;
    let ordering_start = if sub_layer_ordering_info_present_flag {
        0
    } else {
        max_sub_layers_minus1
    };
    let mut max_latency_increase_plus1 = [0u32; 7];
    let mut max_dec_pic_buffering_minus1 = [0u8; 7];
    let mut max_num_reorder_pics = [0u8; 7];
    for index in ordering_start..=max_sub_layers_minus1 {
        let max_dec_pic_buffering = bits.read_ue("max_dec_pic_buffering_minus1")?;
        let max_reorder_pics = bits.read_ue("max_num_reorder_pics")?;
        let max_latency_increase = bits.read_ue("max_latency_increase_plus1")?;
        max_dec_pic_buffering_minus1[index as usize] =
            native_vulkan_h265_u8(max_dec_pic_buffering, "max_dec_pic_buffering_minus1")?;
        max_num_reorder_pics[index as usize] =
            native_vulkan_h265_u8(max_reorder_pics, "max_num_reorder_pics")?;
        max_latency_increase_plus1[index as usize] = max_latency_increase;
    }
    if !sub_layer_ordering_info_present_flag {
        let source_index = max_sub_layers_minus1 as usize;
        for index in 0..source_index {
            max_dec_pic_buffering_minus1[index] = max_dec_pic_buffering_minus1[source_index];
            max_num_reorder_pics[index] = max_num_reorder_pics[source_index];
            max_latency_increase_plus1[index] = max_latency_increase_plus1[source_index];
        }
    }
    let _ = label_prefix;

    Ok(NativeVulkanH265ParsedDecPicBufMgr {
        sub_layer_ordering_info_present_flag,
        max_latency_increase_plus1,
        max_dec_pic_buffering_minus1,
        max_num_reorder_pics,
    })
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
fn native_vulkan_parse_h265_vps(payload: &[u8]) -> Result<NativeVulkanH265ParsedVps, String> {
    let rbsp = native_vulkan_h265_rbsp(payload)?;
    let mut bits = NativeVulkanH265BitReader::new(&rbsp);
    bits.skip_bits(16, "vps nal header")?;
    let id = bits.read_bits(4, "vps_video_parameter_set_id")? as u8;
    bits.skip_bits(2, "vps base layer flags")?;
    let max_layers_minus1 = bits.read_bits(6, "vps_max_layers_minus1")? as u8;
    let max_sub_layers_minus1 = bits.read_bits(3, "vps_max_sub_layers_minus1")? as u8;
    if max_sub_layers_minus1 >= 8 {
        return Err(format!(
            "invalid vps_max_sub_layers_minus1={max_sub_layers_minus1}"
        ));
    }
    let temporal_id_nesting_flag = bits.read_bool("vps_temporal_id_nesting_flag")?;
    bits.skip_bits(16, "vps_reserved_0xffff_16bits")?;
    let profile = native_vulkan_parse_h265_profile_tier_level(&mut bits, max_sub_layers_minus1)?;
    let dec_pic_buf_mgr =
        native_vulkan_parse_h265_dec_pic_buf_mgr(&mut bits, max_sub_layers_minus1, "vps")?;
    bits.skip_bits(6, "vps_max_layer_id")?;
    let num_layer_sets_minus1 = bits.read_ue("vps_num_layer_sets_minus1")?;
    for _ in 1..=num_layer_sets_minus1 {
        for _ in 0..=max_layers_minus1 {
            bits.read_bool("layer_id_included_flag")?;
        }
    }
    let timing_info_present_flag = bits.read_bool("vps_timing_info_present_flag")?;
    let mut poc_proportional_to_timing_flag = false;
    let mut num_units_in_tick = None;
    let mut time_scale = None;
    let mut num_ticks_poc_diff_one_minus1 = None;
    if timing_info_present_flag {
        num_units_in_tick = Some(bits.read_bits(32, "vps_num_units_in_tick")?);
        time_scale = Some(bits.read_bits(32, "vps_time_scale")?);
        poc_proportional_to_timing_flag = bits.read_bool("vps_poc_proportional_to_timing_flag")?;
        if poc_proportional_to_timing_flag {
            num_ticks_poc_diff_one_minus1 =
                Some(bits.read_ue("vps_num_ticks_poc_diff_one_minus1")?);
        }
    }

    Ok(NativeVulkanH265ParsedVps {
        id,
        max_layers_minus1,
        max_sub_layers_minus1,
        temporal_id_nesting_flag,
        dec_pic_buf_mgr,
        profile,
        timing_info_present_flag,
        poc_proportional_to_timing_flag,
        num_units_in_tick,
        time_scale,
        num_ticks_poc_diff_one_minus1,
    })
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
fn native_vulkan_parse_h265_sps(payload: &[u8]) -> Result<NativeVulkanH265ParsedSps, String> {
    let rbsp = native_vulkan_h265_rbsp(payload)?;
    let mut bits = NativeVulkanH265BitReader::new(&rbsp);
    bits.skip_bits(16, "sps nal header")?;
    let vps_id = bits.read_bits(4, "sps_video_parameter_set_id")? as u8;
    let max_sub_layers_minus1 = bits.read_bits(3, "sps_max_sub_layers_minus1")? as u8;
    if max_sub_layers_minus1 >= 8 {
        return Err(format!(
            "invalid sps_max_sub_layers_minus1={max_sub_layers_minus1}"
        ));
    }
    let temporal_id_nesting_flag = bits.read_bool("sps_temporal_id_nesting_flag")?;
    let profile = native_vulkan_parse_h265_profile_tier_level(&mut bits, max_sub_layers_minus1)?;
    let id = bits.read_ue("sps_seq_parameter_set_id")?;
    let chroma_format_idc = bits.read_ue("chroma_format_idc")?;
    let separate_colour_plane_flag =
        chroma_format_idc == 3 && bits.read_bool("separate_colour_plane_flag")?;
    let width = bits.read_ue("pic_width_in_luma_samples")?;
    let height = bits.read_ue("pic_height_in_luma_samples")?;
    let conformance_window_flag = bits.read_bool("conformance_window_flag")?;
    let (mut conf_win_left_offset, mut conf_win_right_offset) = (0, 0);
    let (mut conf_win_top_offset, mut conf_win_bottom_offset) = (0, 0);
    if conformance_window_flag {
        conf_win_left_offset = bits.read_ue("conf_win_left_offset")?;
        conf_win_right_offset = bits.read_ue("conf_win_right_offset")?;
        conf_win_top_offset = bits.read_ue("conf_win_top_offset")?;
        conf_win_bottom_offset = bits.read_ue("conf_win_bottom_offset")?;
    }
    let bit_depth_luma_minus8 = bits.read_ue("bit_depth_luma_minus8")?;
    let bit_depth_chroma_minus8 = bits.read_ue("bit_depth_chroma_minus8")?;
    let log2_max_pic_order_cnt_lsb_minus4 = bits.read_ue("log2_max_pic_order_cnt_lsb_minus4")?;
    let dec_pic_buf_mgr =
        native_vulkan_parse_h265_dec_pic_buf_mgr(&mut bits, max_sub_layers_minus1, "sps")?;
    let log2_min_luma_coding_block_size_minus3 =
        bits.read_ue("log2_min_luma_coding_block_size_minus3")?;
    let log2_diff_max_min_luma_coding_block_size =
        bits.read_ue("log2_diff_max_min_luma_coding_block_size")?;
    let log2_min_luma_transform_block_size_minus2 =
        bits.read_ue("log2_min_luma_transform_block_size_minus2")?;
    let log2_diff_max_min_luma_transform_block_size =
        bits.read_ue("log2_diff_max_min_luma_transform_block_size")?;
    let max_transform_hierarchy_depth_inter =
        bits.read_ue("max_transform_hierarchy_depth_inter")?;
    let max_transform_hierarchy_depth_intra =
        bits.read_ue("max_transform_hierarchy_depth_intra")?;
    let scaling_list_enabled_flag = bits.read_bool("scaling_list_enabled_flag")?;
    let sps_scaling_list_data_present_flag =
        scaling_list_enabled_flag && bits.read_bool("sps_scaling_list_data_present_flag")?;
    if sps_scaling_list_data_present_flag {
        native_vulkan_h265_skip_scaling_list_data(&mut bits)?;
    }
    let amp_enabled_flag = bits.read_bool("amp_enabled_flag")?;
    let sample_adaptive_offset_enabled_flag =
        bits.read_bool("sample_adaptive_offset_enabled_flag")?;
    let pcm_enabled_flag = bits.read_bool("pcm_enabled_flag")?;
    let mut pcm_loop_filter_disabled_flag = false;
    if pcm_enabled_flag {
        bits.skip_bits(4, "pcm_sample_bit_depth_luma_minus1")?;
        bits.skip_bits(4, "pcm_sample_bit_depth_chroma_minus1")?;
        bits.read_ue("log2_min_pcm_luma_coding_block_size_minus3")?;
        bits.read_ue("log2_diff_max_min_pcm_luma_coding_block_size")?;
        pcm_loop_filter_disabled_flag = bits.read_bool("pcm_loop_filter_disabled_flag")?;
    }
    let num_short_term_ref_pic_sets = bits.read_ue("num_short_term_ref_pic_sets")?;
    let mut short_term_delta_pocs = Vec::new();
    for st_rps_idx in 0..num_short_term_ref_pic_sets {
        let num_delta_pocs = native_vulkan_h265_skip_short_term_ref_pic_set(
            &mut bits,
            st_rps_idx,
            num_short_term_ref_pic_sets,
            &short_term_delta_pocs,
        )?;
        short_term_delta_pocs.push(num_delta_pocs);
    }
    let long_term_ref_pics_present_flag = bits.read_bool("long_term_ref_pics_present_flag")?;
    if long_term_ref_pics_present_flag {
        let num_long_term_ref_pics_sps = bits.read_ue("num_long_term_ref_pics_sps")?;
        for _ in 0..num_long_term_ref_pics_sps {
            bits.skip_bits(
                log2_max_pic_order_cnt_lsb_minus4 + 4,
                "lt_ref_pic_poc_lsb_sps",
            )?;
            bits.read_bool("used_by_curr_pic_lt_sps_flag")?;
        }
    }
    let temporal_mvp_enabled_flag = bits.read_bool("sps_temporal_mvp_enabled_flag")?;
    let strong_intra_smoothing_enabled_flag =
        bits.read_bool("strong_intra_smoothing_enabled_flag")?;
    let vui_parameters_present_flag = bits.read_bool("vui_parameters_present_flag")?;
    let vui = if vui_parameters_present_flag {
        Some(native_vulkan_parse_h265_vui_parameters(
            &mut bits,
            max_sub_layers_minus1,
        )?)
    } else {
        None
    };
    let sps_extension_present_flag = bits.read_bool("sps_extension_present_flag")?;

    Ok(NativeVulkanH265ParsedSps {
        id,
        vps_id,
        max_sub_layers_minus1,
        temporal_id_nesting_flag,
        dec_pic_buf_mgr,
        profile,
        chroma_format_idc,
        separate_colour_plane_flag,
        width,
        height,
        conformance_window_flag,
        conf_win_left_offset,
        conf_win_right_offset,
        conf_win_top_offset,
        conf_win_bottom_offset,
        bit_depth_luma_minus8,
        bit_depth_chroma_minus8,
        log2_max_pic_order_cnt_lsb_minus4,
        log2_min_luma_coding_block_size_minus3,
        log2_diff_max_min_luma_coding_block_size,
        log2_min_luma_transform_block_size_minus2,
        log2_diff_max_min_luma_transform_block_size,
        max_transform_hierarchy_depth_inter,
        max_transform_hierarchy_depth_intra,
        scaling_list_enabled_flag,
        sps_scaling_list_data_present_flag,
        amp_enabled_flag,
        sample_adaptive_offset_enabled_flag,
        pcm_enabled_flag,
        pcm_loop_filter_disabled_flag,
        num_short_term_ref_pic_sets,
        long_term_ref_pics_present_flag,
        temporal_mvp_enabled_flag,
        strong_intra_smoothing_enabled_flag,
        vui_parameters_present_flag,
        vui,
        sps_extension_present_flag,
    })
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
fn native_vulkan_parse_h265_pps(payload: &[u8]) -> Result<NativeVulkanH265ParsedPps, String> {
    let rbsp = native_vulkan_h265_rbsp(payload)?;
    let mut bits = NativeVulkanH265BitReader::new(&rbsp);
    bits.skip_bits(16, "pps nal header")?;
    let id = bits.read_ue("pps_pic_parameter_set_id")?;
    let sps_id = bits.read_ue("pps_seq_parameter_set_id")?;
    let dependent_slice_segments_enabled_flag =
        bits.read_bool("dependent_slice_segments_enabled_flag")?;
    let output_flag_present_flag = bits.read_bool("output_flag_present_flag")?;
    let num_extra_slice_header_bits = bits.read_bits(3, "num_extra_slice_header_bits")? as u8;
    let sign_data_hiding_enabled_flag = bits.read_bool("sign_data_hiding_enabled_flag")?;
    let cabac_init_present_flag = bits.read_bool("cabac_init_present_flag")?;
    let num_ref_idx_l0_default_active_minus1 =
        bits.read_ue("num_ref_idx_l0_default_active_minus1")?;
    let num_ref_idx_l1_default_active_minus1 =
        bits.read_ue("num_ref_idx_l1_default_active_minus1")?;
    let init_qp_minus26 = bits.read_se("init_qp_minus26")?;
    let constrained_intra_pred_flag = bits.read_bool("constrained_intra_pred_flag")?;
    let transform_skip_enabled_flag = bits.read_bool("transform_skip_enabled_flag")?;
    let cu_qp_delta_enabled_flag = bits.read_bool("cu_qp_delta_enabled_flag")?;
    let diff_cu_qp_delta_depth = if cu_qp_delta_enabled_flag {
        Some(bits.read_ue("diff_cu_qp_delta_depth")?)
    } else {
        None
    };
    let cb_qp_offset = bits.read_se("pps_cb_qp_offset")?;
    let cr_qp_offset = bits.read_se("pps_cr_qp_offset")?;
    let slice_chroma_qp_offsets_present_flag =
        bits.read_bool("pps_slice_chroma_qp_offsets_present_flag")?;
    let weighted_pred_flag = bits.read_bool("weighted_pred_flag")?;
    let weighted_bipred_flag = bits.read_bool("weighted_bipred_flag")?;
    let transquant_bypass_enabled_flag = bits.read_bool("transquant_bypass_enabled_flag")?;
    let tiles_enabled_flag = bits.read_bool("tiles_enabled_flag")?;
    let entropy_coding_sync_enabled_flag = bits.read_bool("entropy_coding_sync_enabled_flag")?;
    let mut num_tile_columns_minus1 = 0;
    let mut num_tile_rows_minus1 = 0;
    let mut loop_filter_across_tiles_enabled_flag = None;
    let mut uniform_spacing_flag = false;
    if tiles_enabled_flag {
        num_tile_columns_minus1 = bits.read_ue("num_tile_columns_minus1")?;
        num_tile_rows_minus1 = bits.read_ue("num_tile_rows_minus1")?;
        uniform_spacing_flag = bits.read_bool("uniform_spacing_flag")?;
        if !uniform_spacing_flag {
            for _ in 0..num_tile_columns_minus1 {
                bits.read_ue("column_width_minus1")?;
            }
            for _ in 0..num_tile_rows_minus1 {
                bits.read_ue("row_height_minus1")?;
            }
        }
        loop_filter_across_tiles_enabled_flag =
            Some(bits.read_bool("loop_filter_across_tiles_enabled_flag")?);
    }
    let loop_filter_across_slices_enabled_flag =
        bits.read_bool("pps_loop_filter_across_slices_enabled_flag")?;
    let deblocking_filter_control_present_flag =
        bits.read_bool("deblocking_filter_control_present_flag")?;
    let mut deblocking_filter_override_enabled_flag = None;
    let mut pps_deblocking_filter_disabled_flag = None;
    let mut pps_beta_offset_div2 = 0;
    let mut pps_tc_offset_div2 = 0;
    if deblocking_filter_control_present_flag {
        deblocking_filter_override_enabled_flag =
            Some(bits.read_bool("deblocking_filter_override_enabled_flag")?);
        let disabled = bits.read_bool("pps_deblocking_filter_disabled_flag")?;
        pps_deblocking_filter_disabled_flag = Some(disabled);
        if !disabled {
            pps_beta_offset_div2 = bits.read_se("pps_beta_offset_div2")?;
            pps_tc_offset_div2 = bits.read_se("pps_tc_offset_div2")?;
        }
    }
    let pps_scaling_list_data_present_flag =
        bits.read_bool("pps_scaling_list_data_present_flag")?;
    if pps_scaling_list_data_present_flag {
        native_vulkan_h265_skip_scaling_list_data(&mut bits)?;
    }
    let lists_modification_present_flag = bits.read_bool("lists_modification_present_flag")?;
    let log2_parallel_merge_level_minus2 = bits.read_ue("log2_parallel_merge_level_minus2")?;
    let slice_segment_header_extension_present_flag =
        bits.read_bool("slice_segment_header_extension_present_flag")?;
    let pps_extension_present_flag = bits.read_bool("pps_extension_present_flag")?;

    Ok(NativeVulkanH265ParsedPps {
        id,
        sps_id,
        dependent_slice_segments_enabled_flag,
        output_flag_present_flag,
        num_extra_slice_header_bits,
        sign_data_hiding_enabled_flag,
        cabac_init_present_flag,
        num_ref_idx_l0_default_active_minus1,
        num_ref_idx_l1_default_active_minus1,
        init_qp_minus26,
        constrained_intra_pred_flag,
        transform_skip_enabled_flag,
        cu_qp_delta_enabled_flag,
        diff_cu_qp_delta_depth,
        cb_qp_offset,
        cr_qp_offset,
        slice_chroma_qp_offsets_present_flag,
        weighted_pred_flag,
        weighted_bipred_flag,
        transquant_bypass_enabled_flag,
        tiles_enabled_flag,
        entropy_coding_sync_enabled_flag,
        uniform_spacing_flag,
        num_tile_columns_minus1,
        num_tile_rows_minus1,
        loop_filter_across_tiles_enabled_flag,
        loop_filter_across_slices_enabled_flag,
        deblocking_filter_control_present_flag,
        deblocking_filter_override_enabled_flag,
        pps_deblocking_filter_disabled_flag,
        pps_beta_offset_div2,
        pps_tc_offset_div2,
        pps_scaling_list_data_present_flag,
        lists_modification_present_flag,
        log2_parallel_merge_level_minus2,
        slice_segment_header_extension_present_flag,
        pps_extension_present_flag,
    })
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
fn native_vulkan_parse_h265_vui_parameters(
    bits: &mut NativeVulkanH265BitReader<'_>,
    max_sub_layers_minus1: u8,
) -> Result<NativeVulkanH265ParsedVui, String> {
    let aspect_ratio_info_present_flag = bits.read_bool("aspect_ratio_info_present_flag")?;
    let mut aspect_ratio_idc = 0u32;
    let mut sar_width = 0u16;
    let mut sar_height = 0u16;
    if aspect_ratio_info_present_flag {
        aspect_ratio_idc = bits.read_bits(8, "aspect_ratio_idc")?;
        if aspect_ratio_idc == 255 {
            sar_width = native_vulkan_h265_u16(bits.read_bits(16, "sar_width")?, "sar_width")?;
            sar_height = native_vulkan_h265_u16(bits.read_bits(16, "sar_height")?, "sar_height")?;
        }
    }

    let overscan_info_present_flag = bits.read_bool("overscan_info_present_flag")?;
    let overscan_appropriate_flag =
        overscan_info_present_flag && bits.read_bool("overscan_appropriate_flag")?;

    let video_signal_type_present_flag = bits.read_bool("video_signal_type_present_flag")?;
    let mut video_format = 5u8;
    let mut video_full_range_flag = false;
    let mut colour_description_present_flag = false;
    let mut colour_primaries = 2u8;
    let mut transfer_characteristics = 2u8;
    let mut matrix_coeffs = 2u8;
    if video_signal_type_present_flag {
        video_format = native_vulkan_h265_u8(bits.read_bits(3, "video_format")?, "video_format")?;
        video_full_range_flag = bits.read_bool("video_full_range_flag")?;
        colour_description_present_flag = bits.read_bool("colour_description_present_flag")?;
        if colour_description_present_flag {
            colour_primaries =
                native_vulkan_h265_u8(bits.read_bits(8, "colour_primaries")?, "colour_primaries")?;
            transfer_characteristics = native_vulkan_h265_u8(
                bits.read_bits(8, "transfer_characteristics")?,
                "transfer_characteristics",
            )?;
            matrix_coeffs =
                native_vulkan_h265_u8(bits.read_bits(8, "matrix_coeffs")?, "matrix_coeffs")?;
        }
    }

    let chroma_loc_info_present_flag = bits.read_bool("chroma_loc_info_present_flag")?;
    let mut chroma_sample_loc_type_top_field = 0u8;
    let mut chroma_sample_loc_type_bottom_field = 0u8;
    if chroma_loc_info_present_flag {
        chroma_sample_loc_type_top_field = native_vulkan_h265_u8(
            bits.read_ue("chroma_sample_loc_type_top_field")?,
            "chroma_sample_loc_type_top_field",
        )?;
        chroma_sample_loc_type_bottom_field = native_vulkan_h265_u8(
            bits.read_ue("chroma_sample_loc_type_bottom_field")?,
            "chroma_sample_loc_type_bottom_field",
        )?;
    }

    let neutral_chroma_indication_flag = bits.read_bool("neutral_chroma_indication_flag")?;
    let field_seq_flag = bits.read_bool("field_seq_flag")?;
    let frame_field_info_present_flag = bits.read_bool("frame_field_info_present_flag")?;
    let default_display_window_flag = bits.read_bool("default_display_window_flag")?;
    let mut def_disp_win_left_offset = 0u16;
    let mut def_disp_win_right_offset = 0u16;
    let mut def_disp_win_top_offset = 0u16;
    let mut def_disp_win_bottom_offset = 0u16;
    if default_display_window_flag {
        def_disp_win_left_offset = native_vulkan_h265_u16(
            bits.read_ue("def_disp_win_left_offset")?,
            "def_disp_win_left_offset",
        )?;
        def_disp_win_right_offset = native_vulkan_h265_u16(
            bits.read_ue("def_disp_win_right_offset")?,
            "def_disp_win_right_offset",
        )?;
        def_disp_win_top_offset = native_vulkan_h265_u16(
            bits.read_ue("def_disp_win_top_offset")?,
            "def_disp_win_top_offset",
        )?;
        def_disp_win_bottom_offset = native_vulkan_h265_u16(
            bits.read_ue("def_disp_win_bottom_offset")?,
            "def_disp_win_bottom_offset",
        )?;
    }

    let vui_timing_info_present_flag = bits.read_bool("vui_timing_info_present_flag")?;
    let mut vui_num_units_in_tick = 0u32;
    let mut vui_time_scale = 0u32;
    let mut vui_poc_proportional_to_timing_flag = false;
    let mut vui_num_ticks_poc_diff_one_minus1 = 0u32;
    let mut vui_hrd_parameters_present_flag = false;
    if vui_timing_info_present_flag {
        vui_num_units_in_tick = bits.read_bits(32, "vui_num_units_in_tick")?;
        vui_time_scale = bits.read_bits(32, "vui_time_scale")?;
        vui_poc_proportional_to_timing_flag =
            bits.read_bool("vui_poc_proportional_to_timing_flag")?;
        if vui_poc_proportional_to_timing_flag {
            vui_num_ticks_poc_diff_one_minus1 =
                bits.read_ue("vui_num_ticks_poc_diff_one_minus1")?;
        }
        vui_hrd_parameters_present_flag = bits.read_bool("vui_hrd_parameters_present_flag")?;
        if vui_hrd_parameters_present_flag {
            native_vulkan_h265_skip_hrd_parameters(bits, true, max_sub_layers_minus1)?;
        }
    }

    let bitstream_restriction_flag = bits.read_bool("bitstream_restriction_flag")?;
    let mut tiles_fixed_structure_flag = false;
    let mut motion_vectors_over_pic_boundaries_flag = false;
    let mut restricted_ref_pic_lists_flag = false;
    let mut min_spatial_segmentation_idc = 0u16;
    let mut max_bytes_per_pic_denom = 0u8;
    let mut max_bits_per_min_cu_denom = 0u8;
    let mut log2_max_mv_length_horizontal = 0u8;
    let mut log2_max_mv_length_vertical = 0u8;
    if bitstream_restriction_flag {
        tiles_fixed_structure_flag = bits.read_bool("tiles_fixed_structure_flag")?;
        motion_vectors_over_pic_boundaries_flag =
            bits.read_bool("motion_vectors_over_pic_boundaries_flag")?;
        restricted_ref_pic_lists_flag = bits.read_bool("restricted_ref_pic_lists_flag")?;
        min_spatial_segmentation_idc = native_vulkan_h265_u16(
            bits.read_ue("min_spatial_segmentation_idc")?,
            "min_spatial_segmentation_idc",
        )?;
        max_bytes_per_pic_denom = native_vulkan_h265_u8(
            bits.read_ue("max_bytes_per_pic_denom")?,
            "max_bytes_per_pic_denom",
        )?;
        max_bits_per_min_cu_denom = native_vulkan_h265_u8(
            bits.read_ue("max_bits_per_min_cu_denom")?,
            "max_bits_per_min_cu_denom",
        )?;
        log2_max_mv_length_horizontal = native_vulkan_h265_u8(
            bits.read_ue("log2_max_mv_length_horizontal")?,
            "log2_max_mv_length_horizontal",
        )?;
        log2_max_mv_length_vertical = native_vulkan_h265_u8(
            bits.read_ue("log2_max_mv_length_vertical")?,
            "log2_max_mv_length_vertical",
        )?;
    }

    Ok(NativeVulkanH265ParsedVui {
        aspect_ratio_info_present_flag,
        aspect_ratio_idc,
        sar_width,
        sar_height,
        overscan_info_present_flag,
        overscan_appropriate_flag,
        video_signal_type_present_flag,
        video_format,
        video_full_range_flag,
        colour_description_present_flag,
        colour_primaries,
        transfer_characteristics,
        matrix_coeffs,
        chroma_loc_info_present_flag,
        chroma_sample_loc_type_top_field,
        chroma_sample_loc_type_bottom_field,
        neutral_chroma_indication_flag,
        field_seq_flag,
        frame_field_info_present_flag,
        default_display_window_flag,
        def_disp_win_left_offset,
        def_disp_win_right_offset,
        def_disp_win_top_offset,
        def_disp_win_bottom_offset,
        vui_timing_info_present_flag,
        vui_num_units_in_tick,
        vui_time_scale,
        vui_poc_proportional_to_timing_flag,
        vui_num_ticks_poc_diff_one_minus1,
        vui_hrd_parameters_present_flag,
        bitstream_restriction_flag,
        tiles_fixed_structure_flag,
        motion_vectors_over_pic_boundaries_flag,
        restricted_ref_pic_lists_flag,
        min_spatial_segmentation_idc,
        max_bytes_per_pic_denom,
        max_bits_per_min_cu_denom,
        log2_max_mv_length_horizontal,
        log2_max_mv_length_vertical,
    })
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
fn native_vulkan_h265_skip_hrd_parameters(
    bits: &mut NativeVulkanH265BitReader<'_>,
    common_inf_present_flag: bool,
    max_sub_layers_minus1: u8,
) -> Result<(), String> {
    let mut nal_hrd_parameters_present_flag = false;
    let mut vcl_hrd_parameters_present_flag = false;
    let mut sub_pic_hrd_params_present_flag = false;
    if common_inf_present_flag {
        nal_hrd_parameters_present_flag = bits.read_bool("nal_hrd_parameters_present_flag")?;
        vcl_hrd_parameters_present_flag = bits.read_bool("vcl_hrd_parameters_present_flag")?;
        if nal_hrd_parameters_present_flag || vcl_hrd_parameters_present_flag {
            sub_pic_hrd_params_present_flag = bits.read_bool("sub_pic_hrd_params_present_flag")?;
            if sub_pic_hrd_params_present_flag {
                bits.skip_bits(8, "tick_divisor_minus2")?;
                bits.skip_bits(5, "du_cpb_removal_delay_increment_length_minus1")?;
                bits.read_bool("sub_pic_cpb_params_in_pic_timing_sei_flag")?;
                bits.skip_bits(5, "dpb_output_delay_du_length_minus1")?;
            }
            bits.skip_bits(4, "bit_rate_scale")?;
            bits.skip_bits(4, "cpb_size_scale")?;
            if sub_pic_hrd_params_present_flag {
                bits.skip_bits(4, "cpb_size_du_scale")?;
            }
            bits.skip_bits(5, "initial_cpb_removal_delay_length_minus1")?;
            bits.skip_bits(5, "au_cpb_removal_delay_length_minus1")?;
            bits.skip_bits(5, "dpb_output_delay_length_minus1")?;
        }
    }

    for _ in 0..=max_sub_layers_minus1 {
        let fixed_pic_rate_general_flag = bits.read_bool("fixed_pic_rate_general_flag")?;
        let fixed_pic_rate_within_cvs_flag = if fixed_pic_rate_general_flag {
            true
        } else {
            bits.read_bool("fixed_pic_rate_within_cvs_flag")?
        };
        let mut low_delay_hrd_flag = false;
        if fixed_pic_rate_within_cvs_flag {
            bits.read_ue("elemental_duration_in_tc_minus1")?;
        } else {
            low_delay_hrd_flag = bits.read_bool("low_delay_hrd_flag")?;
        }
        let cpb_cnt_minus1 = if low_delay_hrd_flag {
            0
        } else {
            bits.read_ue("cpb_cnt_minus1")?
        };
        if nal_hrd_parameters_present_flag {
            native_vulkan_h265_skip_sub_layer_hrd_parameters(
                bits,
                cpb_cnt_minus1,
                sub_pic_hrd_params_present_flag,
            )?;
        }
        if vcl_hrd_parameters_present_flag {
            native_vulkan_h265_skip_sub_layer_hrd_parameters(
                bits,
                cpb_cnt_minus1,
                sub_pic_hrd_params_present_flag,
            )?;
        }
    }
    Ok(())
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
fn native_vulkan_h265_skip_sub_layer_hrd_parameters(
    bits: &mut NativeVulkanH265BitReader<'_>,
    cpb_cnt_minus1: u32,
    sub_pic_hrd_params_present_flag: bool,
) -> Result<(), String> {
    for _ in 0..=cpb_cnt_minus1 {
        bits.read_ue("bit_rate_value_minus1")?;
        bits.read_ue("cpb_size_value_minus1")?;
        if sub_pic_hrd_params_present_flag {
            bits.read_ue("cpb_size_du_value_minus1")?;
            bits.read_ue("bit_rate_du_value_minus1")?;
        }
        bits.read_bool("cbr_flag")?;
    }
    Ok(())
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
fn native_vulkan_parse_h265_profile_tier_level(
    bits: &mut NativeVulkanH265BitReader<'_>,
    max_sub_layers_minus1: u8,
) -> Result<NativeVulkanH265ParsedProfileTierLevel, String> {
    bits.skip_bits(2, "general_profile_space")?;
    let tier_flag = bits.read_bool("general_tier_flag")?;
    let profile_idc = bits.read_bits(5, "general_profile_idc")? as u8;
    let mut profile_compatibility_flags = [false; 32];
    for flag in profile_compatibility_flags.iter_mut() {
        *flag = bits.read_bool("general_profile_compatibility_flag")?;
    }
    let progressive_source_flag = bits.read_bool("general_progressive_source_flag")?;
    let interlaced_source_flag = bits.read_bool("general_interlaced_source_flag")?;
    let non_packed_constraint_flag = bits.read_bool("general_non_packed_constraint_flag")?;
    let frame_only_constraint_flag = bits.read_bool("general_frame_only_constraint_flag")?;
    bits.skip_bits(44, "general_constraint_indicator_flags")?;
    let level_idc = bits.read_bits(8, "general_level_idc")? as u8;
    let mut sub_layer_profile_present_flags = [false; 8];
    let mut sub_layer_level_present_flags = [false; 8];
    for index in 0..usize::from(max_sub_layers_minus1) {
        sub_layer_profile_present_flags[index] =
            bits.read_bool("sub_layer_profile_present_flag")?;
        sub_layer_level_present_flags[index] = bits.read_bool("sub_layer_level_present_flag")?;
    }
    if max_sub_layers_minus1 > 0 {
        for _ in max_sub_layers_minus1..8 {
            bits.skip_bits(2, "reserved_zero_2bits")?;
        }
    }
    for index in 0..usize::from(max_sub_layers_minus1) {
        if sub_layer_profile_present_flags[index] {
            bits.skip_bits(2, "sub_layer_profile_space")?;
            bits.skip_bits(1, "sub_layer_tier_flag")?;
            bits.skip_bits(5, "sub_layer_profile_idc")?;
            bits.skip_bits(32, "sub_layer_profile_compatibility_flags")?;
            bits.skip_bits(4, "sub_layer_source_constraint_flags")?;
            bits.skip_bits(44, "sub_layer_constraint_indicator_flags")?;
        }
        if sub_layer_level_present_flags[index] {
            bits.skip_bits(8, "sub_layer_level_idc")?;
        }
    }

    Ok(NativeVulkanH265ParsedProfileTierLevel {
        profile_idc,
        tier_flag,
        progressive_source_flag,
        interlaced_source_flag,
        non_packed_constraint_flag,
        frame_only_constraint_flag,
        profile_compatibility_flags,
        level_idc,
    })
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
fn native_vulkan_h265_skip_scaling_list_data(
    bits: &mut NativeVulkanH265BitReader<'_>,
) -> Result<(), String> {
    for size_id in 0..4u32 {
        let step = if size_id == 3 { 3 } else { 1 };
        let mut matrix_id = 0u32;
        while matrix_id < 6 {
            let pred_mode_flag = bits.read_bool("scaling_list_pred_mode_flag")?;
            if !pred_mode_flag {
                bits.read_ue("scaling_list_pred_matrix_id_delta")?;
            } else {
                let coef_num = 64u32.min(1u32 << (4 + (size_id << 1)));
                if size_id > 1 {
                    bits.read_se("scaling_list_dc_coef_minus8")?;
                }
                for _ in 0..coef_num {
                    bits.read_se("scaling_list_delta_coef")?;
                }
            }
            matrix_id += step;
        }
    }
    Ok(())
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
fn native_vulkan_h265_skip_short_term_ref_pic_set(
    bits: &mut NativeVulkanH265BitReader<'_>,
    st_rps_idx: u32,
    num_short_term_ref_pic_sets: u32,
    previous_num_delta_pocs: &[u32],
) -> Result<u32, String> {
    let inter_ref_pic_set_prediction_flag =
        st_rps_idx != 0 && bits.read_bool("inter_ref_pic_set_prediction_flag")?;
    if inter_ref_pic_set_prediction_flag {
        let delta_idx_minus1 = if st_rps_idx == num_short_term_ref_pic_sets {
            bits.read_ue("delta_idx_minus1")?
        } else {
            0
        };
        bits.read_bool("delta_rps_sign")?;
        bits.read_ue("abs_delta_rps_minus1")?;
        let ref_idx = st_rps_idx
            .checked_sub(delta_idx_minus1 + 1)
            .ok_or_else(|| "invalid short-term RPS delta_idx_minus1".to_owned())?;
        let ref_num_delta_pocs = previous_num_delta_pocs
            .get(ref_idx as usize)
            .copied()
            .unwrap_or(0);
        for _ in 0..=ref_num_delta_pocs {
            let used_by_curr_pic_flag = bits.read_bool("used_by_curr_pic_flag")?;
            if !used_by_curr_pic_flag {
                bits.read_bool("use_delta_flag")?;
            }
        }
        return Ok(ref_num_delta_pocs);
    }

    let num_negative_pics = bits.read_ue("num_negative_pics")?;
    let num_positive_pics = bits.read_ue("num_positive_pics")?;
    for _ in 0..num_negative_pics {
        bits.read_ue("delta_poc_s0_minus1")?;
        bits.read_bool("used_by_curr_pic_s0_flag")?;
    }
    for _ in 0..num_positive_pics {
        bits.read_ue("delta_poc_s1_minus1")?;
        bits.read_bool("used_by_curr_pic_s1_flag")?;
    }
    Ok(num_negative_pics.saturating_add(num_positive_pics))
}

fn native_vulkan_h265_u8(value: u32, label: &'static str) -> Result<u8, String> {
    u8::try_from(value).map_err(|_| format!("{label}={value} exceeds u8 range"))
}

fn native_vulkan_h265_i8(value: i32, label: &'static str) -> Result<i8, String> {
    i8::try_from(value).map_err(|_| format!("{label}={value} exceeds i8 range"))
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
fn native_vulkan_h265_u16(value: u32, label: &'static str) -> Result<u16, String> {
    u16::try_from(value).map_err(|_| format!("{label}={value} exceeds u16 range"))
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
fn native_vulkan_h265_rbsp(payload: &[u8]) -> Result<Vec<u8>, String> {
    if payload.len() < 2 {
        return Err("H.265 NAL payload is too short".to_owned());
    }
    let mut rbsp = Vec::with_capacity(payload.len());
    let mut zero_count = 0u8;
    for byte in payload.iter().copied() {
        if zero_count == 2 && byte == 0x03 {
            zero_count = 0;
            continue;
        }
        rbsp.push(byte);
        if byte == 0 {
            zero_count = zero_count.saturating_add(1).min(2);
        } else {
            zero_count = 0;
        }
    }
    Ok(rbsp)
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
struct NativeVulkanH265BitReader<'a> {
    bytes: &'a [u8],
    bit_offset: usize,
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
impl<'a> NativeVulkanH265BitReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            bit_offset: 0,
        }
    }

    fn read_bool(&mut self, label: &'static str) -> Result<bool, String> {
        Ok(self.read_bits(1, label)? != 0)
    }

    fn skip_bits(&mut self, count: u32, label: &'static str) -> Result<(), String> {
        let mut remaining = count;
        while remaining > 0 {
            let chunk = remaining.min(32);
            self.read_bits(chunk, label)?;
            remaining -= chunk;
        }
        Ok(())
    }

    fn read_bits(&mut self, count: u32, label: &'static str) -> Result<u32, String> {
        if count > 32 {
            return Err(format!("{label} requested too many bits: {count}"));
        }
        let end = self
            .bit_offset
            .checked_add(count as usize)
            .ok_or_else(|| format!("{label} bit offset overflow"))?;
        if end > self.bytes.len() * 8 {
            return Err(format!("{label} exceeds H.265 RBSP length"));
        }
        let mut value = 0u32;
        for _ in 0..count {
            let byte = self.bytes[self.bit_offset / 8];
            let shift = 7 - (self.bit_offset % 8);
            value = (value << 1) | u32::from((byte >> shift) & 1);
            self.bit_offset += 1;
        }
        Ok(value)
    }

    fn read_ue(&mut self, label: &'static str) -> Result<u32, String> {
        let mut leading_zero_bits = 0u32;
        while !self.read_bool(label)? {
            leading_zero_bits += 1;
            if leading_zero_bits > 31 {
                return Err(format!("{label} Exp-Golomb code is too large"));
            }
        }
        if leading_zero_bits == 0 {
            return Ok(0);
        }
        let suffix = self.read_bits(leading_zero_bits, label)?;
        Ok((1u32 << leading_zero_bits) - 1 + suffix)
    }

    fn read_se(&mut self, label: &'static str) -> Result<i32, String> {
        let value = self.read_ue(label)?;
        let signed = value.div_ceil(2) as i32;
        if value % 2 == 0 {
            Ok(-signed)
        } else {
            Ok(signed)
        }
    }
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
fn native_vulkan_h265_profile_idc_label(profile_idc: u8) -> &'static str {
    match profile_idc {
        1 => "main",
        2 => "main-10",
        3 => "main-still-picture",
        4 => "format-range-extensions",
        5 => "high-throughput",
        6 => "multiview-main",
        7 => "scalable-main",
        8 => "3d-main",
        9 => "screen-content-coding",
        10 => "scalable-format-range-extensions",
        11 => "high-throughput-screen-content-coding",
        _ => "unknown",
    }
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
fn native_vulkan_h265_chroma_format_label(chroma_format_idc: u32) -> &'static str {
    match chroma_format_idc {
        0 => "monochrome",
        1 => "4:2:0",
        2 => "4:2:2",
        3 => "4:4:4",
        _ => "unknown",
    }
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
fn native_vulkan_h265_level_idc_byte_label(level_idc: u8) -> Option<&'static str> {
    match level_idc {
        30 => Some("1.0"),
        60 => Some("2.0"),
        63 => Some("2.1"),
        90 => Some("3.0"),
        93 => Some("3.1"),
        120 => Some("4.0"),
        123 => Some("4.1"),
        150 => Some("5.0"),
        153 => Some("5.1"),
        156 => Some("5.2"),
        180 => Some("6.0"),
        183 => Some("6.1"),
        186 => Some("6.2"),
        _ => None,
    }
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct NativeVulkanH265NalStats {
    bytes: u64,
    has_annex_b_start_codes: bool,
    vps_count: u32,
    sps_count: u32,
    pps_count: u32,
    idr_count: u32,
    slice_count: u32,
    nal_units: Vec<NativeVulkanH265NalUnitSnapshot>,
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
impl NativeVulkanH265NalStats {
    fn parameter_sets_present(&self) -> bool {
        self.vps_count > 0 && self.sps_count > 0 && self.pps_count > 0
    }
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
fn native_vulkan_h265_nal_stats(bytes: &[u8]) -> NativeVulkanH265NalStats {
    let mut stats = NativeVulkanH265NalStats {
        bytes: bytes.len() as u64,
        ..Default::default()
    };
    let mut offset = 0usize;
    while let Some((start_code_offset, payload_offset)) =
        native_vulkan_next_annex_b_start_code(bytes, offset)
    {
        stats.has_annex_b_start_codes = true;
        let next_search_offset = payload_offset;
        let next_start = native_vulkan_next_annex_b_start_code(bytes, next_search_offset)
            .map(|(next_start, _)| next_start)
            .unwrap_or(bytes.len());
        if payload_offset < next_start {
            let nal_size = next_start - payload_offset;
            if let Some(nal_type) = bytes.get(payload_offset).map(|header| (header >> 1) & 0x3f) {
                match nal_type {
                    32 => stats.vps_count = stats.vps_count.saturating_add(1),
                    33 => stats.sps_count = stats.sps_count.saturating_add(1),
                    34 => stats.pps_count = stats.pps_count.saturating_add(1),
                    19 | 20 => {
                        stats.idr_count = stats.idr_count.saturating_add(1);
                        stats.slice_count = stats.slice_count.saturating_add(1);
                    }
                    0..=31 => stats.slice_count = stats.slice_count.saturating_add(1),
                    _ => {}
                }
                if stats.nal_units.len() < 32 {
                    stats.nal_units.push(NativeVulkanH265NalUnitSnapshot {
                        offset: start_code_offset as u64,
                        size: nal_size as u64,
                        nal_type,
                        nal_type_label: native_vulkan_h265_nal_type_label(nal_type),
                    });
                }
            }
        }
        offset = next_start;
    }
    stats
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
fn native_vulkan_next_annex_b_start_code(bytes: &[u8], from: usize) -> Option<(usize, usize)> {
    let mut index = from;
    while index + 3 <= bytes.len() {
        if bytes[index] == 0 && bytes[index + 1] == 0 {
            if bytes[index + 2] == 1 {
                return Some((index, index + 3));
            }
            if index + 4 <= bytes.len() && bytes[index + 2] == 0 && bytes[index + 3] == 1 {
                return Some((index, index + 4));
            }
        }
        index += 1;
    }
    None
}

#[cfg(any(feature = "native-vulkan-gst-video", test))]
fn native_vulkan_h265_nal_type_label(nal_type: u8) -> &'static str {
    match nal_type {
        0 => "trail-n",
        1 => "trail-r",
        16 => "bla-w-lp",
        17 => "bla-w-radl",
        18 => "bla-n-lp",
        19 => "idr-w-radl",
        20 => "idr-n-lp",
        21 => "cra-nut",
        32 => "vps",
        33 => "sps",
        34 => "pps",
        35 => "aud",
        36 => "eos",
        37 => "eob",
        38 => "fd",
        39 => "prefix-sei",
        40 => "suffix-sei",
        41..=47 => "reserved",
        48..=63 => "unspecified",
        _ => "slice-or-extension",
    }
}

fn native_vulkan_stable_byte_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn native_vulkan_video_session_smoke_result(
    options: &NativeVulkanVideoSessionSmokeOptions,
) -> &'static str {
    if options.sample_decoded_first_frame {
        return "h265-first-frame-decode-output-sampled-and-readback-completed";
    }
    if options.decode_first_frame {
        return "h265-first-frame-decode-and-output-readback-completed";
    }
    match (
        options.allocate_video_images,
        options.allocate_bitstream_buffer,
    ) {
        (false, false) => "session-created-and-memory-bound",
        (true, false) => "session-created-memory-bound-and-video-images-bound",
        (false, true) => "session-created-memory-bound-and-bitstream-buffer-bound",
        (true, true) => "session-created-memory-bound-video-images-and-bitstream-buffer-bound",
    }
}

fn native_vulkan_align_up(value: u64, alignment: u64) -> u64 {
    if alignment <= 1 {
        return value;
    }
    let remainder = value % alignment;
    if remainder == 0 {
        value
    } else {
        value.saturating_add(alignment - remainder)
    }
}

fn native_vulkan_video_session_extent_supported(
    extent: vk::Extent2D,
    capabilities: &NativeVulkanVideoSessionCapabilityQuery,
) -> bool {
    extent.width >= capabilities.min_coded_extent.width
        && extent.height >= capabilities.min_coded_extent.height
        && extent.width <= capabilities.max_coded_extent.width
        && extent.height <= capabilities.max_coded_extent.height
        && native_vulkan_video_session_extent_aligned(
            extent.width,
            capabilities.picture_access_granularity.width,
        )
        && native_vulkan_video_session_extent_aligned(
            extent.height,
            capabilities.picture_access_granularity.height,
        )
}

fn native_vulkan_video_session_extent_aligned(value: u32, granularity: u32) -> bool {
    granularity == 0 || value.is_multiple_of(granularity)
}

fn native_vulkan_video_session_max_dpb_slots(driver_max_dpb_slots: u32) -> u32 {
    if driver_max_dpb_slots == 0 {
        0
    } else {
        driver_max_dpb_slots.min(8).max(1)
    }
}

fn native_vulkan_video_session_max_active_reference_pictures(
    driver_max_active_reference_pictures: u32,
    session_max_dpb_slots: u32,
) -> u32 {
    if driver_max_active_reference_pictures == 0 || session_max_dpb_slots == 0 {
        0
    } else {
        driver_max_active_reference_pictures.min(session_max_dpb_slots)
    }
}

fn native_vulkan_create_video_session(
    video_queue_device: &ash::khr::video_queue::Device,
    create_info: &vk::VideoSessionCreateInfoKHR<'_>,
) -> Result<vk::VideoSessionKHR, NativeVulkanError> {
    let mut session = vk::VideoSessionKHR::null();
    unsafe {
        (video_queue_device.fp().create_video_session_khr)(
            video_queue_device.device(),
            create_info,
            ptr::null(),
            &mut session,
        )
    }
    .result()
    .map_err(|result| NativeVulkanError::Vulkan {
        operation: "vkCreateVideoSessionKHR",
        result,
    })?;
    Ok(session)
}

fn native_vulkan_video_session_memory_requirements(
    video_queue_device: &ash::khr::video_queue::Device,
    session: vk::VideoSessionKHR,
) -> Result<Vec<vk::VideoSessionMemoryRequirementsKHR<'static>>, NativeVulkanError> {
    let mut memory_requirement_count = 0u32;
    unsafe {
        (video_queue_device
            .fp()
            .get_video_session_memory_requirements_khr)(
            video_queue_device.device(),
            session,
            &mut memory_requirement_count,
            ptr::null_mut(),
        )
    }
    .result()
    .map_err(|result| NativeVulkanError::Vulkan {
        operation: "vkGetVideoSessionMemoryRequirementsKHR(count)",
        result,
    })?;

    let mut memory_requirements =
        vec![vk::VideoSessionMemoryRequirementsKHR::default(); memory_requirement_count as usize];
    unsafe {
        (video_queue_device
            .fp()
            .get_video_session_memory_requirements_khr)(
            video_queue_device.device(),
            session,
            &mut memory_requirement_count,
            memory_requirements.as_mut_ptr(),
        )
    }
    .result()
    .map_err(|result| NativeVulkanError::Vulkan {
        operation: "vkGetVideoSessionMemoryRequirementsKHR",
        result,
    })?;
    memory_requirements.truncate(memory_requirement_count as usize);
    Ok(memory_requirements)
}

fn native_vulkan_bind_video_session_memory(
    video_queue_device: &ash::khr::video_queue::Device,
    session: vk::VideoSessionKHR,
    bind_infos: &[vk::BindVideoSessionMemoryInfoKHR<'_>],
) -> Result<(), NativeVulkanError> {
    unsafe {
        (video_queue_device.fp().bind_video_session_memory_khr)(
            video_queue_device.device(),
            session,
            bind_infos.len() as u32,
            bind_infos.as_ptr(),
        )
    }
    .result()
    .map_err(|result| NativeVulkanError::Vulkan {
        operation: "vkBindVideoSessionMemoryKHR",
        result,
    })
}

fn native_vulkan_create_h265_video_session_parameters(
    video_queue_device: &ash::khr::video_queue::Device,
    session: vk::VideoSessionKHR,
    parameter_sets: &NativeVulkanH265ParameterSetSnapshot,
) -> Result<NativeVulkanVideoSessionParameters, NativeVulkanError> {
    if !parameter_sets.vulkan_std_session_parameters_ready {
        return Err(NativeVulkanError::Video(
            "H.265 parameter sets are not in the first supported Vulkan STD subset".to_owned(),
        ));
    }

    let vps_profile_tier_level = native_vulkan_h265_std_profile_tier_level(
        parameter_sets.vps.profile_idc,
        parameter_sets.vps.level_idc,
        parameter_sets.vps.tier_flag,
        parameter_sets.vps.progressive_source_flag,
        parameter_sets.vps.interlaced_source_flag,
        parameter_sets.vps.non_packed_constraint_flag,
        parameter_sets.vps.frame_only_constraint_flag,
    )?;
    let sps_profile_tier_level = native_vulkan_h265_std_profile_tier_level(
        parameter_sets.sps.profile_idc,
        parameter_sets.sps.level_idc,
        parameter_sets.sps.tier_flag,
        parameter_sets.sps.progressive_source_flag,
        parameter_sets.sps.interlaced_source_flag,
        parameter_sets.sps.non_packed_constraint_flag,
        parameter_sets.sps.frame_only_constraint_flag,
    )?;
    let vps_dec_pic_buf_mgr =
        native_vulkan_h265_std_dec_pic_buf_mgr(&parameter_sets.vps.dec_pic_buf_mgr);
    let sps_dec_pic_buf_mgr =
        native_vulkan_h265_std_dec_pic_buf_mgr(&parameter_sets.sps.dec_pic_buf_mgr);
    let sps_vui = parameter_sets
        .sps
        .vui
        .as_ref()
        .map(native_vulkan_h265_std_vui)
        .transpose()?;
    let sps_vui_ptr = sps_vui
        .as_ref()
        .map(|vui| vui as *const vk::native::StdVideoH265SequenceParameterSetVui)
        .unwrap_or_else(ptr::null);

    let vps = [vk::native::StdVideoH265VideoParameterSet {
        flags: vk::native::StdVideoH265VpsFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::native::StdVideoH265VpsFlags::new_bitfield_1(
                native_vulkan_bool_u32(parameter_sets.vps.temporal_id_nesting_flag),
                native_vulkan_bool_u32(parameter_sets.vps.sub_layer_ordering_info_present_flag),
                native_vulkan_bool_u32(parameter_sets.vps.timing_info_present_flag),
                native_vulkan_bool_u32(parameter_sets.vps.poc_proportional_to_timing_flag),
            ),
            __bindgen_padding_0: [0; 3],
        },
        vps_video_parameter_set_id: parameter_sets.vps.id,
        vps_max_sub_layers_minus1: parameter_sets.vps.max_sub_layers_minus1,
        reserved1: 0,
        reserved2: 0,
        vps_num_units_in_tick: parameter_sets.vps.num_units_in_tick.unwrap_or(0),
        vps_time_scale: parameter_sets.vps.time_scale.unwrap_or(0),
        vps_num_ticks_poc_diff_one_minus1: parameter_sets
            .vps
            .num_ticks_poc_diff_one_minus1
            .unwrap_or(0),
        reserved3: 0,
        pDecPicBufMgr: &vps_dec_pic_buf_mgr,
        pHrdParameters: ptr::null(),
        pProfileTierLevel: &vps_profile_tier_level,
    }];

    let sps = [vk::native::StdVideoH265SequenceParameterSet {
        flags: vk::native::StdVideoH265SpsFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::native::StdVideoH265SpsFlags::new_bitfield_1(
                native_vulkan_bool_u32(parameter_sets.sps.temporal_id_nesting_flag),
                native_vulkan_bool_u32(parameter_sets.sps.separate_colour_plane_flag),
                native_vulkan_bool_u32(parameter_sets.sps.conformance_window_flag),
                native_vulkan_bool_u32(parameter_sets.sps.sub_layer_ordering_info_present_flag),
                native_vulkan_bool_u32(parameter_sets.sps.scaling_list_enabled_flag),
                native_vulkan_bool_u32(parameter_sets.sps.sps_scaling_list_data_present_flag),
                native_vulkan_bool_u32(parameter_sets.sps.amp_enabled_flag),
                native_vulkan_bool_u32(parameter_sets.sps.sample_adaptive_offset_enabled_flag),
                native_vulkan_bool_u32(parameter_sets.sps.pcm_enabled_flag),
                native_vulkan_bool_u32(parameter_sets.sps.pcm_loop_filter_disabled_flag),
                native_vulkan_bool_u32(parameter_sets.sps.long_term_ref_pics_present_flag),
                native_vulkan_bool_u32(parameter_sets.sps.temporal_mvp_enabled_flag),
                native_vulkan_bool_u32(parameter_sets.sps.strong_intra_smoothing_enabled_flag),
                native_vulkan_bool_u32(parameter_sets.sps.vui_parameters_present_flag),
                native_vulkan_bool_u32(parameter_sets.sps.sps_extension_present_flag),
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
            ),
        },
        chroma_format_idc: native_vulkan_h265_std_chroma_format_idc(
            parameter_sets.sps.chroma_format_idc,
        )?,
        pic_width_in_luma_samples: parameter_sets.sps.width,
        pic_height_in_luma_samples: parameter_sets.sps.height,
        sps_video_parameter_set_id: parameter_sets.sps.vps_id,
        sps_max_sub_layers_minus1: parameter_sets.sps.max_sub_layers_minus1,
        sps_seq_parameter_set_id: native_vulkan_h265_u8(
            parameter_sets.sps.id,
            "sps_seq_parameter_set_id",
        )
        .map_err(NativeVulkanError::Video)?,
        bit_depth_luma_minus8: native_vulkan_h265_u8(
            parameter_sets.sps.bit_depth_luma_minus8,
            "bit_depth_luma_minus8",
        )
        .map_err(NativeVulkanError::Video)?,
        bit_depth_chroma_minus8: native_vulkan_h265_u8(
            parameter_sets.sps.bit_depth_chroma_minus8,
            "bit_depth_chroma_minus8",
        )
        .map_err(NativeVulkanError::Video)?,
        log2_max_pic_order_cnt_lsb_minus4: native_vulkan_h265_u8(
            parameter_sets.sps.log2_max_pic_order_cnt_lsb_minus4,
            "log2_max_pic_order_cnt_lsb_minus4",
        )
        .map_err(NativeVulkanError::Video)?,
        log2_min_luma_coding_block_size_minus3: native_vulkan_h265_u8(
            parameter_sets.sps.log2_min_luma_coding_block_size_minus3,
            "log2_min_luma_coding_block_size_minus3",
        )
        .map_err(NativeVulkanError::Video)?,
        log2_diff_max_min_luma_coding_block_size: native_vulkan_h265_u8(
            parameter_sets.sps.log2_diff_max_min_luma_coding_block_size,
            "log2_diff_max_min_luma_coding_block_size",
        )
        .map_err(NativeVulkanError::Video)?,
        log2_min_luma_transform_block_size_minus2: native_vulkan_h265_u8(
            parameter_sets.sps.log2_min_luma_transform_block_size_minus2,
            "log2_min_luma_transform_block_size_minus2",
        )
        .map_err(NativeVulkanError::Video)?,
        log2_diff_max_min_luma_transform_block_size: native_vulkan_h265_u8(
            parameter_sets
                .sps
                .log2_diff_max_min_luma_transform_block_size,
            "log2_diff_max_min_luma_transform_block_size",
        )
        .map_err(NativeVulkanError::Video)?,
        max_transform_hierarchy_depth_inter: native_vulkan_h265_u8(
            parameter_sets.sps.max_transform_hierarchy_depth_inter,
            "max_transform_hierarchy_depth_inter",
        )
        .map_err(NativeVulkanError::Video)?,
        max_transform_hierarchy_depth_intra: native_vulkan_h265_u8(
            parameter_sets.sps.max_transform_hierarchy_depth_intra,
            "max_transform_hierarchy_depth_intra",
        )
        .map_err(NativeVulkanError::Video)?,
        num_short_term_ref_pic_sets: native_vulkan_h265_u8(
            parameter_sets.sps.num_short_term_ref_pic_sets,
            "num_short_term_ref_pic_sets",
        )
        .map_err(NativeVulkanError::Video)?,
        num_long_term_ref_pics_sps: 0,
        pcm_sample_bit_depth_luma_minus1: 0,
        pcm_sample_bit_depth_chroma_minus1: 0,
        log2_min_pcm_luma_coding_block_size_minus3: 0,
        log2_diff_max_min_pcm_luma_coding_block_size: 0,
        reserved1: 0,
        reserved2: 0,
        palette_max_size: 0,
        delta_palette_max_predictor_size: 0,
        motion_vector_resolution_control_idc: 0,
        sps_num_palette_predictor_initializers_minus1: 0,
        conf_win_left_offset: parameter_sets.sps.conf_win_left_offset,
        conf_win_right_offset: parameter_sets.sps.conf_win_right_offset,
        conf_win_top_offset: parameter_sets.sps.conf_win_top_offset,
        conf_win_bottom_offset: parameter_sets.sps.conf_win_bottom_offset,
        pProfileTierLevel: &sps_profile_tier_level,
        pDecPicBufMgr: &sps_dec_pic_buf_mgr,
        pScalingLists: ptr::null(),
        pShortTermRefPicSet: ptr::null(),
        pLongTermRefPicsSps: ptr::null(),
        pSequenceParameterSetVui: sps_vui_ptr,
        pPredictorPaletteEntries: ptr::null(),
    }];

    let pps = [vk::native::StdVideoH265PictureParameterSet {
        flags: vk::native::StdVideoH265PpsFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::native::StdVideoH265PpsFlags::new_bitfield_1(
                native_vulkan_bool_u32(parameter_sets.pps.dependent_slice_segments_enabled_flag),
                native_vulkan_bool_u32(parameter_sets.pps.output_flag_present_flag),
                native_vulkan_bool_u32(parameter_sets.pps.sign_data_hiding_enabled_flag),
                native_vulkan_bool_u32(parameter_sets.pps.cabac_init_present_flag),
                native_vulkan_bool_u32(parameter_sets.pps.constrained_intra_pred_flag),
                native_vulkan_bool_u32(parameter_sets.pps.transform_skip_enabled_flag),
                native_vulkan_bool_u32(parameter_sets.pps.cu_qp_delta_enabled_flag),
                native_vulkan_bool_u32(parameter_sets.pps.slice_chroma_qp_offsets_present_flag),
                native_vulkan_bool_u32(parameter_sets.pps.weighted_pred_flag),
                native_vulkan_bool_u32(parameter_sets.pps.weighted_bipred_flag),
                native_vulkan_bool_u32(parameter_sets.pps.transquant_bypass_enabled_flag),
                native_vulkan_bool_u32(parameter_sets.pps.tiles_enabled_flag),
                native_vulkan_bool_u32(parameter_sets.pps.entropy_coding_sync_enabled_flag),
                native_vulkan_bool_u32(parameter_sets.pps.uniform_spacing_flag),
                native_vulkan_bool_u32(
                    parameter_sets
                        .pps
                        .loop_filter_across_tiles_enabled_flag
                        .unwrap_or(false),
                ),
                native_vulkan_bool_u32(parameter_sets.pps.loop_filter_across_slices_enabled_flag),
                native_vulkan_bool_u32(parameter_sets.pps.deblocking_filter_control_present_flag),
                native_vulkan_bool_u32(
                    parameter_sets
                        .pps
                        .deblocking_filter_override_enabled_flag
                        .unwrap_or(false),
                ),
                native_vulkan_bool_u32(
                    parameter_sets
                        .pps
                        .pps_deblocking_filter_disabled_flag
                        .unwrap_or(false),
                ),
                native_vulkan_bool_u32(parameter_sets.pps.pps_scaling_list_data_present_flag),
                native_vulkan_bool_u32(parameter_sets.pps.lists_modification_present_flag),
                native_vulkan_bool_u32(
                    parameter_sets
                        .pps
                        .slice_segment_header_extension_present_flag,
                ),
                native_vulkan_bool_u32(parameter_sets.pps.pps_extension_present_flag),
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
            ),
        },
        pps_pic_parameter_set_id: native_vulkan_h265_u8(
            parameter_sets.pps.id,
            "pps_pic_parameter_set_id",
        )
        .map_err(NativeVulkanError::Video)?,
        pps_seq_parameter_set_id: native_vulkan_h265_u8(
            parameter_sets.pps.sps_id,
            "pps_seq_parameter_set_id",
        )
        .map_err(NativeVulkanError::Video)?,
        sps_video_parameter_set_id: parameter_sets.sps.vps_id,
        num_extra_slice_header_bits: parameter_sets.pps.num_extra_slice_header_bits,
        num_ref_idx_l0_default_active_minus1: native_vulkan_h265_u8(
            parameter_sets.pps.num_ref_idx_l0_default_active_minus1,
            "num_ref_idx_l0_default_active_minus1",
        )
        .map_err(NativeVulkanError::Video)?,
        num_ref_idx_l1_default_active_minus1: native_vulkan_h265_u8(
            parameter_sets.pps.num_ref_idx_l1_default_active_minus1,
            "num_ref_idx_l1_default_active_minus1",
        )
        .map_err(NativeVulkanError::Video)?,
        init_qp_minus26: native_vulkan_h265_i8(
            parameter_sets.pps.init_qp_minus26,
            "init_qp_minus26",
        )
        .map_err(NativeVulkanError::Video)?,
        diff_cu_qp_delta_depth: native_vulkan_h265_u8(
            parameter_sets.pps.diff_cu_qp_delta_depth.unwrap_or(0),
            "diff_cu_qp_delta_depth",
        )
        .map_err(NativeVulkanError::Video)?,
        pps_cb_qp_offset: native_vulkan_h265_i8(
            parameter_sets.pps.cb_qp_offset,
            "pps_cb_qp_offset",
        )
        .map_err(NativeVulkanError::Video)?,
        pps_cr_qp_offset: native_vulkan_h265_i8(
            parameter_sets.pps.cr_qp_offset,
            "pps_cr_qp_offset",
        )
        .map_err(NativeVulkanError::Video)?,
        pps_beta_offset_div2: native_vulkan_h265_i8(
            parameter_sets.pps.pps_beta_offset_div2,
            "pps_beta_offset_div2",
        )
        .map_err(NativeVulkanError::Video)?,
        pps_tc_offset_div2: native_vulkan_h265_i8(
            parameter_sets.pps.pps_tc_offset_div2,
            "pps_tc_offset_div2",
        )
        .map_err(NativeVulkanError::Video)?,
        log2_parallel_merge_level_minus2: native_vulkan_h265_u8(
            parameter_sets.pps.log2_parallel_merge_level_minus2,
            "log2_parallel_merge_level_minus2",
        )
        .map_err(NativeVulkanError::Video)?,
        log2_max_transform_skip_block_size_minus2: 0,
        diff_cu_chroma_qp_offset_depth: 0,
        chroma_qp_offset_list_len_minus1: 0,
        cb_qp_offset_list: [0; 6],
        cr_qp_offset_list: [0; 6],
        log2_sao_offset_scale_luma: 0,
        log2_sao_offset_scale_chroma: 0,
        pps_act_y_qp_offset_plus5: 0,
        pps_act_cb_qp_offset_plus5: 0,
        pps_act_cr_qp_offset_plus3: 0,
        pps_num_palette_predictor_initializers: 0,
        luma_bit_depth_entry_minus8: 0,
        chroma_bit_depth_entry_minus8: 0,
        num_tile_columns_minus1: native_vulkan_h265_u8(
            parameter_sets.pps.num_tile_columns_minus1,
            "num_tile_columns_minus1",
        )
        .map_err(NativeVulkanError::Video)?,
        num_tile_rows_minus1: native_vulkan_h265_u8(
            parameter_sets.pps.num_tile_rows_minus1,
            "num_tile_rows_minus1",
        )
        .map_err(NativeVulkanError::Video)?,
        reserved1: 0,
        reserved2: 0,
        column_width_minus1: [0; 19],
        row_height_minus1: [0; 21],
        reserved3: 0,
        pScalingLists: ptr::null(),
        pPredictorPaletteEntries: ptr::null(),
    }];

    let add_info = vk::VideoDecodeH265SessionParametersAddInfoKHR::default()
        .std_vp_ss(&vps)
        .std_sp_ss(&sps)
        .std_pp_ss(&pps);
    let max_std_vps_count = 32;
    let max_std_sps_count = 32;
    let max_std_pps_count = 64;
    let mut h265_create_info = vk::VideoDecodeH265SessionParametersCreateInfoKHR::default()
        .max_std_vps_count(max_std_vps_count)
        .max_std_sps_count(max_std_sps_count)
        .max_std_pps_count(max_std_pps_count)
        .parameters_add_info(&add_info);
    let create_info = vk::VideoSessionParametersCreateInfoKHR::default()
        .video_session(session)
        .push_next(&mut h265_create_info);
    let mut parameters = vk::VideoSessionParametersKHR::null();
    unsafe {
        (video_queue_device.fp().create_video_session_parameters_khr)(
            video_queue_device.device(),
            &create_info,
            ptr::null(),
            &mut parameters,
        )
    }
    .result()
    .map_err(|result| NativeVulkanError::Vulkan {
        operation: "vkCreateVideoSessionParametersKHR(h265)",
        result,
    })?;

    Ok(NativeVulkanVideoSessionParameters {
        parameters,
        snapshot: NativeVulkanVideoSessionParametersSnapshot {
            codec: "h265-main-8",
            source: "native-rust-h265-vps-sps-pps-to-vulkan-std",
            max_std_vps_count,
            max_std_sps_count,
            max_std_pps_count,
            std_vps_count: vps.len() as u32,
            std_sps_count: sps.len() as u32,
            std_pps_count: pps.len() as u32,
            vps_id: parameter_sets.vps.id,
            sps_id: parameter_sets.sps.id,
            pps_id: parameter_sets.pps.id,
            profile_idc: parameter_sets.sps.profile_idc,
            level_idc: parameter_sets.sps.level_idc,
            width: parameter_sets.sps.width,
            height: parameter_sets.sps.height,
            created: true,
        },
    })
}

fn native_vulkan_h265_std_profile_tier_level(
    profile_idc: u8,
    level_idc: u8,
    tier_flag: bool,
    progressive_source_flag: bool,
    interlaced_source_flag: bool,
    non_packed_constraint_flag: bool,
    frame_only_constraint_flag: bool,
) -> Result<vk::native::StdVideoH265ProfileTierLevel, NativeVulkanError> {
    Ok(vk::native::StdVideoH265ProfileTierLevel {
        flags: vk::native::StdVideoH265ProfileTierLevelFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::native::StdVideoH265ProfileTierLevelFlags::new_bitfield_1(
                native_vulkan_bool_u32(tier_flag),
                native_vulkan_bool_u32(progressive_source_flag),
                native_vulkan_bool_u32(interlaced_source_flag),
                native_vulkan_bool_u32(non_packed_constraint_flag),
                native_vulkan_bool_u32(frame_only_constraint_flag),
            ),
            __bindgen_padding_0: [0; 3],
        },
        general_profile_idc: native_vulkan_h265_std_profile_idc(profile_idc)?,
        general_level_idc: native_vulkan_h265_std_level_idc(level_idc)?,
    })
}

fn native_vulkan_h265_std_dec_pic_buf_mgr(
    snapshot: &NativeVulkanH265DecPicBufMgrSnapshot,
) -> vk::native::StdVideoH265DecPicBufMgr {
    vk::native::StdVideoH265DecPicBufMgr {
        max_latency_increase_plus1: snapshot.max_latency_increase_plus1,
        max_dec_pic_buffering_minus1: snapshot.max_dec_pic_buffering_minus1,
        max_num_reorder_pics: snapshot.max_num_reorder_pics,
    }
}

fn native_vulkan_h265_std_vui(
    vui: &NativeVulkanH265VuiSnapshot,
) -> Result<vk::native::StdVideoH265SequenceParameterSetVui, NativeVulkanError> {
    if vui.vui_hrd_parameters_present_flag {
        return Err(NativeVulkanError::Video(
            "H.265 VUI HRD parameters are not converted to Vulkan STD yet".to_owned(),
        ));
    }
    Ok(vk::native::StdVideoH265SequenceParameterSetVui {
        flags: vk::native::StdVideoH265SpsVuiFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::native::StdVideoH265SpsVuiFlags::new_bitfield_1(
                native_vulkan_bool_u32(vui.aspect_ratio_info_present_flag),
                native_vulkan_bool_u32(vui.overscan_info_present_flag),
                native_vulkan_bool_u32(vui.overscan_appropriate_flag),
                native_vulkan_bool_u32(vui.video_signal_type_present_flag),
                native_vulkan_bool_u32(vui.video_full_range_flag),
                native_vulkan_bool_u32(vui.colour_description_present_flag),
                native_vulkan_bool_u32(vui.chroma_loc_info_present_flag),
                native_vulkan_bool_u32(vui.neutral_chroma_indication_flag),
                native_vulkan_bool_u32(vui.field_seq_flag),
                native_vulkan_bool_u32(vui.frame_field_info_present_flag),
                native_vulkan_bool_u32(vui.default_display_window_flag),
                native_vulkan_bool_u32(vui.vui_timing_info_present_flag),
                native_vulkan_bool_u32(vui.vui_poc_proportional_to_timing_flag),
                native_vulkan_bool_u32(vui.vui_hrd_parameters_present_flag),
                native_vulkan_bool_u32(vui.bitstream_restriction_flag),
                native_vulkan_bool_u32(vui.tiles_fixed_structure_flag),
                native_vulkan_bool_u32(vui.motion_vectors_over_pic_boundaries_flag),
                native_vulkan_bool_u32(vui.restricted_ref_pic_lists_flag),
            ),
            __bindgen_padding_0: 0,
        },
        aspect_ratio_idc: vui.aspect_ratio_idc,
        sar_width: vui.sar_width,
        sar_height: vui.sar_height,
        video_format: vui.video_format,
        colour_primaries: vui.colour_primaries,
        transfer_characteristics: vui.transfer_characteristics,
        matrix_coeffs: vui.matrix_coeffs,
        chroma_sample_loc_type_top_field: vui.chroma_sample_loc_type_top_field,
        chroma_sample_loc_type_bottom_field: vui.chroma_sample_loc_type_bottom_field,
        reserved1: 0,
        reserved2: 0,
        def_disp_win_left_offset: vui.def_disp_win_left_offset,
        def_disp_win_right_offset: vui.def_disp_win_right_offset,
        def_disp_win_top_offset: vui.def_disp_win_top_offset,
        def_disp_win_bottom_offset: vui.def_disp_win_bottom_offset,
        vui_num_units_in_tick: vui.vui_num_units_in_tick,
        vui_time_scale: vui.vui_time_scale,
        vui_num_ticks_poc_diff_one_minus1: vui.vui_num_ticks_poc_diff_one_minus1,
        min_spatial_segmentation_idc: vui.min_spatial_segmentation_idc,
        reserved3: 0,
        max_bytes_per_pic_denom: vui.max_bytes_per_pic_denom,
        max_bits_per_min_cu_denom: vui.max_bits_per_min_cu_denom,
        log2_max_mv_length_horizontal: vui.log2_max_mv_length_horizontal,
        log2_max_mv_length_vertical: vui.log2_max_mv_length_vertical,
        pHrdParameters: ptr::null(),
    })
}

fn native_vulkan_h265_std_chroma_format_idc(
    chroma_format_idc: u32,
) -> Result<vk::native::StdVideoH265ChromaFormatIdc, NativeVulkanError> {
    match chroma_format_idc {
        1 => Ok(vk::native::StdVideoH265ChromaFormatIdc_STD_VIDEO_H265_CHROMA_FORMAT_IDC_420),
        other => Err(NativeVulkanError::Video(format!(
            "unsupported H.265 chroma_format_idc for Vulkan STD session parameters: {other}"
        ))),
    }
}

fn native_vulkan_h265_std_profile_idc(
    profile_idc: u8,
) -> Result<vk::native::StdVideoH265ProfileIdc, NativeVulkanError> {
    match profile_idc {
        1 => Ok(vk::native::StdVideoH265ProfileIdc_STD_VIDEO_H265_PROFILE_IDC_MAIN),
        2 => Ok(vk::native::StdVideoH265ProfileIdc_STD_VIDEO_H265_PROFILE_IDC_MAIN_10),
        other => Err(NativeVulkanError::Video(format!(
            "unsupported H.265 profile_idc for Vulkan STD session parameters: {other}"
        ))),
    }
}

fn native_vulkan_h265_std_level_idc(
    level_idc: u8,
) -> Result<vk::native::StdVideoH265LevelIdc, NativeVulkanError> {
    match level_idc {
        30 => Ok(vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_1_0),
        60 => Ok(vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_2_0),
        63 => Ok(vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_2_1),
        90 => Ok(vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_3_0),
        93 => Ok(vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_3_1),
        120 => Ok(vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_4_0),
        123 => Ok(vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_4_1),
        150 => Ok(vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_5_0),
        153 => Ok(vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_5_1),
        156 => Ok(vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_5_2),
        180 => Ok(vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_6_0),
        183 => Ok(vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_6_1),
        186 => Ok(vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_6_2),
        other => Err(NativeVulkanError::Video(format!(
            "unsupported H.265 level_idc for Vulkan STD session parameters: {other}"
        ))),
    }
}

fn native_vulkan_bool_u32(value: bool) -> u32 {
    value as u32
}

fn native_vulkan_video_decode_probe_inner(
    entry: &ash::Entry,
    instance: &ash::Instance,
) -> Result<NativeVulkanVideoDecodeProbeSnapshot, NativeVulkanError> {
    let physical_devices = unsafe { instance.enumerate_physical_devices() }.map_err(|result| {
        NativeVulkanError::Vulkan {
            operation: "vkEnumeratePhysicalDevices",
            result,
        }
    })?;
    let video_queue_loader = ash::khr::video_queue::Instance::new(entry, instance);
    let mut devices = Vec::with_capacity(physical_devices.len());
    for (physical_device_index, physical_device) in physical_devices.iter().copied().enumerate() {
        let properties = unsafe { instance.get_physical_device_properties(physical_device) };
        let extensions = native_vulkan_device_extension_names(instance, physical_device)?;
        let has_video_queue_extension = native_vulkan_extension_available_by_name(
            &extensions,
            ash_extension_name(vk::KHR_VIDEO_QUEUE_NAME),
        );
        let has_video_decode_queue_extension = native_vulkan_extension_available_by_name(
            &extensions,
            ash_extension_name(vk::KHR_VIDEO_DECODE_QUEUE_NAME),
        );
        let decode_codec_extensions = native_vulkan_video_decode_codec_extensions(&extensions);
        let queue_families = native_vulkan_video_decode_queue_families(instance, physical_device);
        let has_video_decode_queue_family = queue_families
            .iter()
            .any(|family| family.queue_flags.contains(&"video-decode"));
        let video_decode_ready = has_video_queue_extension
            && has_video_decode_queue_extension
            && !decode_codec_extensions.is_empty()
            && has_video_decode_queue_family;
        let h264_profiles = native_vulkan_video_decode_h264_profiles(
            &video_queue_loader,
            physical_device,
            has_video_queue_extension
                && has_video_decode_queue_extension
                && decode_codec_extensions.iter().any(|extension| {
                    extension == ash_extension_name(vk::KHR_VIDEO_DECODE_H264_NAME)
                }),
        );
        let h264_direct_decode_ready = h264_profiles.iter().any(|profile| {
            profile.supported && profile.nv12_dpb_supported && profile.nv12_output_supported
        });
        let h264_zero_copy_sampled_candidate = h264_profiles
            .iter()
            .any(|profile| profile.supported && profile.nv12_sampled_output_supported);
        let h265_profiles = native_vulkan_video_decode_h265_profiles(
            &video_queue_loader,
            physical_device,
            has_video_queue_extension
                && has_video_decode_queue_extension
                && decode_codec_extensions.iter().any(|extension| {
                    extension == ash_extension_name(vk::KHR_VIDEO_DECODE_H265_NAME)
                }),
        );
        let h265_direct_decode_ready = h265_profiles.iter().any(|profile| {
            profile.supported && profile.nv12_dpb_supported && profile.nv12_output_supported
        });
        let h265_zero_copy_sampled_candidate = h265_profiles
            .iter()
            .any(|profile| profile.supported && profile.nv12_sampled_output_supported);
        let av1_profiles = native_vulkan_video_decode_av1_profiles(
            &video_queue_loader,
            physical_device,
            has_video_queue_extension
                && has_video_decode_queue_extension
                && decode_codec_extensions
                    .iter()
                    .any(|extension| extension == "VK_KHR_video_decode_av1"),
        );
        let av1_direct_decode_ready = av1_profiles.iter().any(|profile| {
            profile.supported && profile.nv12_dpb_supported && profile.nv12_output_supported
        });
        let av1_zero_copy_sampled_candidate = av1_profiles
            .iter()
            .any(|profile| profile.supported && profile.nv12_sampled_output_supported);
        devices.push(NativeVulkanVideoDecodeDeviceSnapshot {
            physical_device_index,
            physical_device_name: native_vulkan_physical_device_name(properties),
            physical_device_type: native_vulkan_physical_device_type_label(properties.device_type),
            vendor_id: properties.vendor_id,
            device_id: properties.device_id,
            api_version: native_vulkan_api_version_label(properties.api_version),
            driver_version: properties.driver_version,
            has_video_queue_extension,
            has_video_decode_queue_extension,
            decode_codec_extensions,
            has_video_decode_queue_family,
            video_decode_ready,
            h264_direct_decode_ready,
            h264_zero_copy_sampled_candidate,
            h264_profiles,
            h265_direct_decode_ready,
            h265_zero_copy_sampled_candidate,
            h265_profiles,
            av1_direct_decode_ready,
            av1_zero_copy_sampled_candidate,
            av1_profiles,
            queue_families,
        });
    }
    Ok(NativeVulkanVideoDecodeProbeSnapshot {
        physical_device_count: physical_devices.len(),
        devices,
    })
}

fn native_vulkan_video_decode_h264_profiles(
    video_queue_loader: &ash::khr::video_queue::Instance,
    physical_device: vk::PhysicalDevice,
    query_enabled: bool,
) -> Vec<NativeVulkanVideoDecodeH264ProfileSnapshot> {
    [
        (
            "baseline",
            vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_BASELINE,
        ),
        (
            "main",
            vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_MAIN,
        ),
        (
            "high",
            vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_HIGH,
        ),
    ]
    .into_iter()
    .map(|(profile, std_profile_idc)| {
        if query_enabled {
            native_vulkan_video_decode_h264_profile(
                video_queue_loader,
                physical_device,
                profile,
                std_profile_idc,
            )
        } else {
            native_vulkan_video_decode_h264_profile_error(
                profile,
                std_profile_idc,
                "required Vulkan Video H.264 decode extensions are unavailable".to_owned(),
            )
        }
    })
    .collect()
}

fn native_vulkan_video_decode_h264_profile(
    video_queue_loader: &ash::khr::video_queue::Instance,
    physical_device: vk::PhysicalDevice,
    profile: &'static str,
    std_profile_idc: vk::native::StdVideoH264ProfileIdc,
) -> NativeVulkanVideoDecodeH264ProfileSnapshot {
    let chroma_subsampling = vk::VideoChromaSubsamplingFlagsKHR::TYPE_420;
    let luma_bit_depth = vk::VideoComponentBitDepthFlagsKHR::TYPE_8;
    let chroma_bit_depth = vk::VideoComponentBitDepthFlagsKHR::TYPE_8;
    let picture_layout = vk::VideoDecodeH264PictureLayoutFlagsKHR::PROGRESSIVE;
    let mut h264_profile_info = vk::VideoDecodeH264ProfileInfoKHR::default()
        .std_profile_idc(std_profile_idc)
        .picture_layout(picture_layout);
    let profile_info = vk::VideoProfileInfoKHR::default()
        .video_codec_operation(vk::VideoCodecOperationFlagsKHR::DECODE_H264)
        .chroma_subsampling(chroma_subsampling)
        .luma_bit_depth(luma_bit_depth)
        .chroma_bit_depth(chroma_bit_depth)
        .push_next(&mut h264_profile_info);
    let (
        capability_flags,
        min_bitstream_buffer_offset_alignment,
        min_bitstream_buffer_size_alignment,
        picture_access_granularity,
        min_coded_extent,
        max_coded_extent,
        max_dpb_slots,
        max_active_reference_pictures,
        decode_capability_flags,
        max_level_idc,
        field_offset_granularity,
    ) = {
        let mut h264_capabilities = vk::VideoDecodeH264CapabilitiesKHR::default();
        let mut decode_capabilities = vk::VideoDecodeCapabilitiesKHR::default();
        let mut capabilities = vk::VideoCapabilitiesKHR::default()
            .push_next(&mut h264_capabilities)
            .push_next(&mut decode_capabilities);

        let capabilities_result = unsafe {
            (video_queue_loader
                .fp()
                .get_physical_device_video_capabilities_khr)(
                physical_device,
                &profile_info,
                &mut capabilities,
            )
        }
        .result();
        if let Err(result) = capabilities_result {
            return native_vulkan_video_decode_h264_profile_error(
                profile,
                std_profile_idc,
                format!("vkGetPhysicalDeviceVideoCapabilitiesKHR: {result:?}"),
            );
        }

        (
            capabilities.flags,
            capabilities.min_bitstream_buffer_offset_alignment,
            capabilities.min_bitstream_buffer_size_alignment,
            (
                capabilities.picture_access_granularity.width,
                capabilities.picture_access_granularity.height,
            ),
            (
                capabilities.min_coded_extent.width,
                capabilities.min_coded_extent.height,
            ),
            (
                capabilities.max_coded_extent.width,
                capabilities.max_coded_extent.height,
            ),
            capabilities.max_dpb_slots,
            capabilities.max_active_reference_pictures,
            decode_capabilities.flags,
            h264_capabilities.max_level_idc,
            (
                h264_capabilities.field_offset_granularity.x,
                h264_capabilities.field_offset_granularity.y,
            ),
        )
    };

    let format_probe = native_vulkan_video_decode_format_probe(
        video_queue_loader,
        physical_device,
        &profile_info,
        decode_capability_flags,
    );

    NativeVulkanVideoDecodeH264ProfileSnapshot {
        profile,
        std_profile_idc,
        picture_layout: native_vulkan_h264_picture_layout_label(picture_layout),
        chroma_subsampling: native_vulkan_video_chroma_subsampling_labels(chroma_subsampling),
        luma_bit_depth: native_vulkan_video_component_bit_depth_labels(luma_bit_depth),
        chroma_bit_depth: native_vulkan_video_component_bit_depth_labels(chroma_bit_depth),
        supported: true,
        max_level_idc: Some(max_level_idc),
        max_level: native_vulkan_h264_level_label(max_level_idc),
        capability_flags: native_vulkan_video_capability_flag_labels(capability_flags),
        decode_capability_flags: native_vulkan_video_decode_capability_flag_labels(
            decode_capability_flags,
        ),
        min_bitstream_buffer_offset_alignment: Some(min_bitstream_buffer_offset_alignment),
        min_bitstream_buffer_size_alignment: Some(min_bitstream_buffer_size_alignment),
        picture_access_granularity: Some(picture_access_granularity),
        min_coded_extent: Some(min_coded_extent),
        max_coded_extent: Some(max_coded_extent),
        max_dpb_slots: Some(max_dpb_slots),
        max_active_reference_pictures: Some(max_active_reference_pictures),
        field_offset_granularity: Some(field_offset_granularity),
        dpb_formats: format_probe.dpb_formats,
        output_formats: format_probe.output_formats,
        sampled_output_formats: format_probe.sampled_output_formats,
        nv12_dpb_supported: format_probe.nv12_dpb_supported,
        nv12_output_supported: format_probe.nv12_output_supported,
        nv12_sampled_output_supported: format_probe.nv12_sampled_output_supported,
        query_error: format_probe.query_error,
    }
}

fn native_vulkan_video_decode_h264_profile_error(
    profile: &'static str,
    std_profile_idc: vk::native::StdVideoH264ProfileIdc,
    query_error: String,
) -> NativeVulkanVideoDecodeH264ProfileSnapshot {
    NativeVulkanVideoDecodeH264ProfileSnapshot {
        profile,
        std_profile_idc,
        picture_layout: "progressive",
        chroma_subsampling: vec!["420"],
        luma_bit_depth: vec!["8-bit"],
        chroma_bit_depth: vec!["8-bit"],
        supported: false,
        max_level_idc: None,
        max_level: None,
        capability_flags: Vec::new(),
        decode_capability_flags: Vec::new(),
        min_bitstream_buffer_offset_alignment: None,
        min_bitstream_buffer_size_alignment: None,
        picture_access_granularity: None,
        min_coded_extent: None,
        max_coded_extent: None,
        max_dpb_slots: None,
        max_active_reference_pictures: None,
        field_offset_granularity: None,
        dpb_formats: Vec::new(),
        output_formats: Vec::new(),
        sampled_output_formats: Vec::new(),
        nv12_dpb_supported: false,
        nv12_output_supported: false,
        nv12_sampled_output_supported: false,
        query_error: Some(query_error),
    }
}

fn native_vulkan_video_decode_h265_profiles(
    video_queue_loader: &ash::khr::video_queue::Instance,
    physical_device: vk::PhysicalDevice,
    query_enabled: bool,
) -> Vec<NativeVulkanVideoDecodeH265ProfileSnapshot> {
    [
        (
            "main-8",
            vk::native::StdVideoH265ProfileIdc_STD_VIDEO_H265_PROFILE_IDC_MAIN,
            vk::VideoComponentBitDepthFlagsKHR::TYPE_8,
        ),
        (
            "main-10",
            vk::native::StdVideoH265ProfileIdc_STD_VIDEO_H265_PROFILE_IDC_MAIN_10,
            vk::VideoComponentBitDepthFlagsKHR::TYPE_10,
        ),
    ]
    .into_iter()
    .map(|(profile, std_profile_idc, bit_depth)| {
        if query_enabled {
            native_vulkan_video_decode_h265_profile(
                video_queue_loader,
                physical_device,
                profile,
                std_profile_idc,
                bit_depth,
            )
        } else {
            native_vulkan_video_decode_h265_profile_error(
                profile,
                std_profile_idc,
                bit_depth,
                "required Vulkan Video H.265 decode extensions are unavailable".to_owned(),
            )
        }
    })
    .collect()
}

fn native_vulkan_video_decode_h265_profile(
    video_queue_loader: &ash::khr::video_queue::Instance,
    physical_device: vk::PhysicalDevice,
    profile: &'static str,
    std_profile_idc: vk::native::StdVideoH265ProfileIdc,
    bit_depth: vk::VideoComponentBitDepthFlagsKHR,
) -> NativeVulkanVideoDecodeH265ProfileSnapshot {
    let chroma_subsampling = vk::VideoChromaSubsamplingFlagsKHR::TYPE_420;
    let mut h265_profile_info =
        vk::VideoDecodeH265ProfileInfoKHR::default().std_profile_idc(std_profile_idc);
    let profile_info = vk::VideoProfileInfoKHR::default()
        .video_codec_operation(vk::VideoCodecOperationFlagsKHR::DECODE_H265)
        .chroma_subsampling(chroma_subsampling)
        .luma_bit_depth(bit_depth)
        .chroma_bit_depth(bit_depth)
        .push_next(&mut h265_profile_info);
    let (
        capability_flags,
        min_bitstream_buffer_offset_alignment,
        min_bitstream_buffer_size_alignment,
        picture_access_granularity,
        min_coded_extent,
        max_coded_extent,
        max_dpb_slots,
        max_active_reference_pictures,
        decode_capability_flags,
        max_level_idc,
    ) = {
        let mut h265_capabilities = vk::VideoDecodeH265CapabilitiesKHR::default();
        let mut decode_capabilities = vk::VideoDecodeCapabilitiesKHR::default();
        let mut capabilities = vk::VideoCapabilitiesKHR::default()
            .push_next(&mut h265_capabilities)
            .push_next(&mut decode_capabilities);

        let capabilities_result = unsafe {
            (video_queue_loader
                .fp()
                .get_physical_device_video_capabilities_khr)(
                physical_device,
                &profile_info,
                &mut capabilities,
            )
        }
        .result();
        if let Err(result) = capabilities_result {
            return native_vulkan_video_decode_h265_profile_error(
                profile,
                std_profile_idc,
                bit_depth,
                format!("vkGetPhysicalDeviceVideoCapabilitiesKHR: {result:?}"),
            );
        }

        (
            capabilities.flags,
            capabilities.min_bitstream_buffer_offset_alignment,
            capabilities.min_bitstream_buffer_size_alignment,
            (
                capabilities.picture_access_granularity.width,
                capabilities.picture_access_granularity.height,
            ),
            (
                capabilities.min_coded_extent.width,
                capabilities.min_coded_extent.height,
            ),
            (
                capabilities.max_coded_extent.width,
                capabilities.max_coded_extent.height,
            ),
            capabilities.max_dpb_slots,
            capabilities.max_active_reference_pictures,
            decode_capabilities.flags,
            h265_capabilities.max_level_idc,
        )
    };
    let format_probe = native_vulkan_video_decode_format_probe(
        video_queue_loader,
        physical_device,
        &profile_info,
        decode_capability_flags,
    );

    NativeVulkanVideoDecodeH265ProfileSnapshot {
        profile,
        std_profile_idc,
        chroma_subsampling: native_vulkan_video_chroma_subsampling_labels(chroma_subsampling),
        luma_bit_depth: native_vulkan_video_component_bit_depth_labels(bit_depth),
        chroma_bit_depth: native_vulkan_video_component_bit_depth_labels(bit_depth),
        supported: true,
        max_level_idc: Some(max_level_idc),
        max_level: native_vulkan_h265_level_label(max_level_idc),
        capability_flags: native_vulkan_video_capability_flag_labels(capability_flags),
        decode_capability_flags: native_vulkan_video_decode_capability_flag_labels(
            decode_capability_flags,
        ),
        min_bitstream_buffer_offset_alignment: Some(min_bitstream_buffer_offset_alignment),
        min_bitstream_buffer_size_alignment: Some(min_bitstream_buffer_size_alignment),
        picture_access_granularity: Some(picture_access_granularity),
        min_coded_extent: Some(min_coded_extent),
        max_coded_extent: Some(max_coded_extent),
        max_dpb_slots: Some(max_dpb_slots),
        max_active_reference_pictures: Some(max_active_reference_pictures),
        dpb_formats: format_probe.dpb_formats,
        output_formats: format_probe.output_formats,
        sampled_output_formats: format_probe.sampled_output_formats,
        nv12_dpb_supported: format_probe.nv12_dpb_supported,
        nv12_output_supported: format_probe.nv12_output_supported,
        nv12_sampled_output_supported: format_probe.nv12_sampled_output_supported,
        query_error: format_probe.query_error,
    }
}

fn native_vulkan_video_decode_h265_profile_error(
    profile: &'static str,
    std_profile_idc: vk::native::StdVideoH265ProfileIdc,
    bit_depth: vk::VideoComponentBitDepthFlagsKHR,
    query_error: String,
) -> NativeVulkanVideoDecodeH265ProfileSnapshot {
    NativeVulkanVideoDecodeH265ProfileSnapshot {
        profile,
        std_profile_idc,
        chroma_subsampling: vec!["420"],
        luma_bit_depth: native_vulkan_video_component_bit_depth_labels(bit_depth),
        chroma_bit_depth: native_vulkan_video_component_bit_depth_labels(bit_depth),
        supported: false,
        max_level_idc: None,
        max_level: None,
        capability_flags: Vec::new(),
        decode_capability_flags: Vec::new(),
        min_bitstream_buffer_offset_alignment: None,
        min_bitstream_buffer_size_alignment: None,
        picture_access_granularity: None,
        min_coded_extent: None,
        max_coded_extent: None,
        max_dpb_slots: None,
        max_active_reference_pictures: None,
        dpb_formats: Vec::new(),
        output_formats: Vec::new(),
        sampled_output_formats: Vec::new(),
        nv12_dpb_supported: false,
        nv12_output_supported: false,
        nv12_sampled_output_supported: false,
        query_error: Some(query_error),
    }
}

fn native_vulkan_video_decode_av1_profiles(
    video_queue_loader: &ash::khr::video_queue::Instance,
    physical_device: vk::PhysicalDevice,
    query_enabled: bool,
) -> Vec<NativeVulkanVideoDecodeAv1ProfileSnapshot> {
    [
        (
            "main-8",
            vk::native::StdVideoAV1Profile_STD_VIDEO_AV1_PROFILE_MAIN,
            vk::VideoComponentBitDepthFlagsKHR::TYPE_8,
            false,
        ),
        (
            "main-10",
            vk::native::StdVideoAV1Profile_STD_VIDEO_AV1_PROFILE_MAIN,
            vk::VideoComponentBitDepthFlagsKHR::TYPE_10,
            false,
        ),
    ]
    .into_iter()
    .map(|(profile, std_profile, bit_depth, film_grain_support)| {
        if query_enabled {
            native_vulkan_video_decode_av1_profile(
                video_queue_loader,
                physical_device,
                profile,
                std_profile,
                bit_depth,
                film_grain_support,
            )
        } else {
            native_vulkan_video_decode_av1_profile_error(
                profile,
                std_profile,
                bit_depth,
                film_grain_support,
                "required Vulkan Video AV1 decode extensions are unavailable".to_owned(),
            )
        }
    })
    .collect()
}

fn native_vulkan_video_decode_av1_profile(
    video_queue_loader: &ash::khr::video_queue::Instance,
    physical_device: vk::PhysicalDevice,
    profile: &'static str,
    std_profile: vk::native::StdVideoAV1Profile,
    bit_depth: vk::VideoComponentBitDepthFlagsKHR,
    film_grain_support: bool,
) -> NativeVulkanVideoDecodeAv1ProfileSnapshot {
    let chroma_subsampling = vk::VideoChromaSubsamplingFlagsKHR::TYPE_420;
    let mut av1_profile_info = vk::VideoDecodeAV1ProfileInfoKHR::default()
        .std_profile(std_profile)
        .film_grain_support(film_grain_support);
    let profile_info = vk::VideoProfileInfoKHR::default()
        .video_codec_operation(vk::VideoCodecOperationFlagsKHR::DECODE_AV1)
        .chroma_subsampling(chroma_subsampling)
        .luma_bit_depth(bit_depth)
        .chroma_bit_depth(bit_depth)
        .push_next(&mut av1_profile_info);
    let (
        capability_flags,
        min_bitstream_buffer_offset_alignment,
        min_bitstream_buffer_size_alignment,
        picture_access_granularity,
        min_coded_extent,
        max_coded_extent,
        max_dpb_slots,
        max_active_reference_pictures,
        decode_capability_flags,
        max_level,
    ) = {
        let mut av1_capabilities = vk::VideoDecodeAV1CapabilitiesKHR::default();
        let mut decode_capabilities = vk::VideoDecodeCapabilitiesKHR::default();
        let mut capabilities = vk::VideoCapabilitiesKHR::default()
            .push_next(&mut av1_capabilities)
            .push_next(&mut decode_capabilities);

        let capabilities_result = unsafe {
            (video_queue_loader
                .fp()
                .get_physical_device_video_capabilities_khr)(
                physical_device,
                &profile_info,
                &mut capabilities,
            )
        }
        .result();
        if let Err(result) = capabilities_result {
            return native_vulkan_video_decode_av1_profile_error(
                profile,
                std_profile,
                bit_depth,
                film_grain_support,
                format!("vkGetPhysicalDeviceVideoCapabilitiesKHR: {result:?}"),
            );
        }

        (
            capabilities.flags,
            capabilities.min_bitstream_buffer_offset_alignment,
            capabilities.min_bitstream_buffer_size_alignment,
            (
                capabilities.picture_access_granularity.width,
                capabilities.picture_access_granularity.height,
            ),
            (
                capabilities.min_coded_extent.width,
                capabilities.min_coded_extent.height,
            ),
            (
                capabilities.max_coded_extent.width,
                capabilities.max_coded_extent.height,
            ),
            capabilities.max_dpb_slots,
            capabilities.max_active_reference_pictures,
            decode_capabilities.flags,
            av1_capabilities.max_level,
        )
    };
    let format_probe = native_vulkan_video_decode_format_probe(
        video_queue_loader,
        physical_device,
        &profile_info,
        decode_capability_flags,
    );

    NativeVulkanVideoDecodeAv1ProfileSnapshot {
        profile,
        std_profile,
        film_grain_support,
        chroma_subsampling: native_vulkan_video_chroma_subsampling_labels(chroma_subsampling),
        luma_bit_depth: native_vulkan_video_component_bit_depth_labels(bit_depth),
        chroma_bit_depth: native_vulkan_video_component_bit_depth_labels(bit_depth),
        supported: true,
        max_level: native_vulkan_av1_level_label(max_level),
        max_level_raw: Some(max_level),
        capability_flags: native_vulkan_video_capability_flag_labels(capability_flags),
        decode_capability_flags: native_vulkan_video_decode_capability_flag_labels(
            decode_capability_flags,
        ),
        min_bitstream_buffer_offset_alignment: Some(min_bitstream_buffer_offset_alignment),
        min_bitstream_buffer_size_alignment: Some(min_bitstream_buffer_size_alignment),
        picture_access_granularity: Some(picture_access_granularity),
        min_coded_extent: Some(min_coded_extent),
        max_coded_extent: Some(max_coded_extent),
        max_dpb_slots: Some(max_dpb_slots),
        max_active_reference_pictures: Some(max_active_reference_pictures),
        dpb_formats: format_probe.dpb_formats,
        output_formats: format_probe.output_formats,
        sampled_output_formats: format_probe.sampled_output_formats,
        nv12_dpb_supported: format_probe.nv12_dpb_supported,
        nv12_output_supported: format_probe.nv12_output_supported,
        nv12_sampled_output_supported: format_probe.nv12_sampled_output_supported,
        query_error: format_probe.query_error,
    }
}

fn native_vulkan_video_decode_av1_profile_error(
    profile: &'static str,
    std_profile: vk::native::StdVideoAV1Profile,
    bit_depth: vk::VideoComponentBitDepthFlagsKHR,
    film_grain_support: bool,
    query_error: String,
) -> NativeVulkanVideoDecodeAv1ProfileSnapshot {
    NativeVulkanVideoDecodeAv1ProfileSnapshot {
        profile,
        std_profile,
        film_grain_support,
        chroma_subsampling: vec!["420"],
        luma_bit_depth: native_vulkan_video_component_bit_depth_labels(bit_depth),
        chroma_bit_depth: native_vulkan_video_component_bit_depth_labels(bit_depth),
        supported: false,
        max_level: None,
        max_level_raw: None,
        capability_flags: Vec::new(),
        decode_capability_flags: Vec::new(),
        min_bitstream_buffer_offset_alignment: None,
        min_bitstream_buffer_size_alignment: None,
        picture_access_granularity: None,
        min_coded_extent: None,
        max_coded_extent: None,
        max_dpb_slots: None,
        max_active_reference_pictures: None,
        dpb_formats: Vec::new(),
        output_formats: Vec::new(),
        sampled_output_formats: Vec::new(),
        nv12_dpb_supported: false,
        nv12_output_supported: false,
        nv12_sampled_output_supported: false,
        query_error: Some(query_error),
    }
}

fn native_vulkan_video_decode_format_probe(
    video_queue_loader: &ash::khr::video_queue::Instance,
    physical_device: vk::PhysicalDevice,
    profile_info: &vk::VideoProfileInfoKHR<'_>,
    decode_capability_flags: vk::VideoDecodeCapabilityFlagsKHR,
) -> NativeVulkanVideoDecodeFormatProbe {
    let dpb_and_output_coincide = decode_capability_flags
        .contains(vk::VideoDecodeCapabilityFlagsKHR::DPB_AND_OUTPUT_COINCIDE);
    let dpb_usage = if dpb_and_output_coincide {
        vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR | vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR
    } else {
        vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR
    };
    let output_usage = vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR;
    let sampled_output_usage =
        vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR | vk::ImageUsageFlags::SAMPLED;
    let sampled_output_required_usage: &[&str] = if dpb_and_output_coincide {
        &["video-decode-dst", "video-decode-dpb", "sampled"]
    } else {
        &["video-decode-dst", "sampled"]
    };

    let mut query_errors = Vec::new();
    let dpb_formats = match native_vulkan_video_format_properties(
        video_queue_loader,
        physical_device,
        profile_info,
        dpb_usage,
    ) {
        Ok(formats) => formats,
        Err(err) => {
            query_errors.push(err);
            Vec::new()
        }
    };
    let output_formats = match native_vulkan_video_format_properties(
        video_queue_loader,
        physical_device,
        profile_info,
        output_usage,
    ) {
        Ok(formats) => formats,
        Err(err) => {
            query_errors.push(err);
            Vec::new()
        }
    };
    let sampled_output_formats = match native_vulkan_video_format_properties(
        video_queue_loader,
        physical_device,
        profile_info,
        sampled_output_usage,
    ) {
        Ok(formats) => formats,
        Err(err) => {
            query_errors.push(err);
            Vec::new()
        }
    };

    NativeVulkanVideoDecodeFormatProbe {
        nv12_dpb_supported: native_vulkan_video_formats_include_nv12_with_usage(
            &dpb_formats,
            &["video-decode-dpb"],
        ),
        nv12_output_supported: native_vulkan_video_formats_include_nv12_with_usage(
            &output_formats,
            &["video-decode-dst"],
        ),
        nv12_sampled_output_supported: native_vulkan_video_formats_include_nv12_with_usage(
            &sampled_output_formats,
            sampled_output_required_usage,
        ),
        dpb_formats,
        output_formats,
        sampled_output_formats,
        query_error: (!query_errors.is_empty()).then(|| query_errors.join("; ")),
    }
}

fn native_vulkan_video_format_properties(
    video_queue_loader: &ash::khr::video_queue::Instance,
    physical_device: vk::PhysicalDevice,
    profile_info: &vk::VideoProfileInfoKHR<'_>,
    image_usage: vk::ImageUsageFlags,
) -> Result<Vec<NativeVulkanVideoFormatPropertiesSnapshot>, String> {
    Ok(native_vulkan_video_format_properties_raw(
        video_queue_loader,
        physical_device,
        profile_info,
        image_usage,
    )
    .map_err(|err| err.to_string())?
    .into_iter()
    .map(native_vulkan_video_format_properties_snapshot)
    .collect())
}

fn native_vulkan_video_format_properties_raw(
    video_queue_loader: &ash::khr::video_queue::Instance,
    physical_device: vk::PhysicalDevice,
    profile_info: &vk::VideoProfileInfoKHR<'_>,
    image_usage: vk::ImageUsageFlags,
) -> Result<Vec<vk::VideoFormatPropertiesKHR<'static>>, NativeVulkanError> {
    let mut profile_list_info =
        vk::VideoProfileListInfoKHR::default().profiles(std::slice::from_ref(profile_info));
    let format_info = vk::PhysicalDeviceVideoFormatInfoKHR::default()
        .image_usage(image_usage)
        .push_next(&mut profile_list_info);
    let mut format_count = 0u32;
    unsafe {
        (video_queue_loader
            .fp()
            .get_physical_device_video_format_properties_khr)(
            physical_device,
            &format_info,
            &mut format_count,
            ptr::null_mut(),
        )
    }
    .result()
    .map_err(|result| NativeVulkanError::Vulkan {
        operation: "vkGetPhysicalDeviceVideoFormatPropertiesKHR(count)",
        result,
    })?;

    let mut format_properties =
        vec![vk::VideoFormatPropertiesKHR::default(); format_count as usize];
    unsafe {
        (video_queue_loader
            .fp()
            .get_physical_device_video_format_properties_khr)(
            physical_device,
            &format_info,
            &mut format_count,
            format_properties.as_mut_ptr(),
        )
    }
    .result()
    .map_err(|result| NativeVulkanError::Vulkan {
        operation: "vkGetPhysicalDeviceVideoFormatPropertiesKHR",
        result,
    })?;
    format_properties.truncate(format_count as usize);
    Ok(format_properties)
}

fn native_vulkan_video_format_properties_snapshot(
    format: vk::VideoFormatPropertiesKHR<'_>,
) -> NativeVulkanVideoFormatPropertiesSnapshot {
    NativeVulkanVideoFormatPropertiesSnapshot {
        format: native_vulkan_format_label(format.format),
        format_raw: format.format.as_raw(),
        image_type: native_vulkan_image_type_label(format.image_type),
        image_tiling: native_vulkan_image_tiling_label(format.image_tiling),
        image_usage_flags: native_vulkan_image_usage_flag_labels(format.image_usage_flags),
        image_create_flags: native_vulkan_image_create_flag_labels(format.image_create_flags),
    }
}

fn native_vulkan_video_formats_include_nv12_with_usage(
    formats: &[NativeVulkanVideoFormatPropertiesSnapshot],
    required_usage: &[&str],
) -> bool {
    formats.iter().any(|format| {
        format.format_raw == vk::Format::G8_B8R8_2PLANE_420_UNORM.as_raw()
            && required_usage
                .iter()
                .all(|usage| format.image_usage_flags.contains(usage))
    })
}

fn native_vulkan_device_extension_names(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
) -> Result<Vec<String>, NativeVulkanError> {
    let mut extensions = unsafe { instance.enumerate_device_extension_properties(physical_device) }
        .map_err(|result| NativeVulkanError::Vulkan {
            operation: "vkEnumerateDeviceExtensionProperties",
            result,
        })?
        .iter()
        .filter_map(|property| property.extension_name_as_c_str().ok())
        .map(|name| name.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    extensions.sort();
    Ok(extensions)
}

fn native_vulkan_video_decode_queue_families(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
) -> Vec<NativeVulkanVideoDecodeQueueFamilySnapshot> {
    let queue_family_count =
        unsafe { instance.get_physical_device_queue_family_properties2_len(physical_device) };
    let mut queue_properties = vec![vk::QueueFamilyProperties2::default(); queue_family_count];
    let mut video_properties =
        vec![vk::QueueFamilyVideoPropertiesKHR::default(); queue_family_count];
    for (queue, video) in queue_properties.iter_mut().zip(video_properties.iter_mut()) {
        queue.p_next = (video as *mut vk::QueueFamilyVideoPropertiesKHR<'_>).cast();
    }
    unsafe {
        instance
            .get_physical_device_queue_family_properties2(physical_device, &mut queue_properties);
    }

    queue_properties
        .iter()
        .zip(video_properties.iter())
        .enumerate()
        .map(|(queue_family_index, (queue, video))| {
            let queue_flags =
                native_vulkan_queue_flag_labels(queue.queue_family_properties.queue_flags);
            let video_codec_operation_bits = video.video_codec_operations.as_raw();
            let video_codec_operations =
                native_vulkan_video_codec_operation_labels(video.video_codec_operations);
            NativeVulkanVideoDecodeQueueFamilySnapshot {
                queue_family_index: queue_family_index as u32,
                queue_count: queue.queue_family_properties.queue_count,
                queue_flags,
                video_codec_operation_bits,
                video_codec_operations,
            }
        })
        .collect()
}

fn native_vulkan_video_decode_codec_extensions(extensions: &[String]) -> Vec<String> {
    [
        ash_extension_name(vk::KHR_VIDEO_DECODE_H264_NAME),
        ash_extension_name(vk::KHR_VIDEO_DECODE_H265_NAME),
        "VK_KHR_video_decode_av1",
        "VK_KHR_video_decode_vp9",
    ]
    .into_iter()
    .filter(|extension| native_vulkan_extension_available_by_name(extensions, extension))
    .map(str::to_owned)
    .collect()
}

fn native_vulkan_extension_available_by_name(extensions: &[String], extension: &str) -> bool {
    extensions.iter().any(|available| available == extension)
}

fn native_vulkan_queue_flag_labels(flags: vk::QueueFlags) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if flags.contains(vk::QueueFlags::GRAPHICS) {
        labels.push("graphics");
    }
    if flags.contains(vk::QueueFlags::COMPUTE) {
        labels.push("compute");
    }
    if flags.contains(vk::QueueFlags::TRANSFER) {
        labels.push("transfer");
    }
    if flags.contains(vk::QueueFlags::SPARSE_BINDING) {
        labels.push("sparse-binding");
    }
    if flags.contains(vk::QueueFlags::VIDEO_DECODE_KHR) {
        labels.push("video-decode");
    }
    if flags.contains(vk::QueueFlags::VIDEO_ENCODE_KHR) {
        labels.push("video-encode");
    }
    labels
}

fn native_vulkan_video_codec_operation_labels(
    operations: vk::VideoCodecOperationFlagsKHR,
) -> Vec<String> {
    let raw = operations.as_raw();
    let known = [
        (
            vk::VideoCodecOperationFlagsKHR::DECODE_H264.as_raw(),
            "decode-h264",
        ),
        (
            vk::VideoCodecOperationFlagsKHR::DECODE_H265.as_raw(),
            "decode-h265",
        ),
        (
            vk::VideoCodecOperationFlagsKHR::DECODE_AV1.as_raw(),
            "decode-av1",
        ),
        (NATIVE_VULKAN_VIDEO_CODEC_OPERATION_DECODE_VP9, "decode-vp9"),
    ];
    let known_bits = known.iter().fold(0u32, |bits, (bit, _)| bits | bit);
    let mut labels = known
        .into_iter()
        .filter_map(|(bit, label)| ((raw & bit) != 0).then(|| label.to_owned()))
        .collect::<Vec<_>>();
    let unknown = raw & !known_bits;
    if unknown != 0 {
        labels.push(format!("unknown-0x{unknown:x}"));
    }
    labels
}

fn native_vulkan_h264_picture_layout_label(
    layout: vk::VideoDecodeH264PictureLayoutFlagsKHR,
) -> &'static str {
    if layout.contains(vk::VideoDecodeH264PictureLayoutFlagsKHR::PROGRESSIVE) {
        "progressive"
    } else if layout
        .contains(vk::VideoDecodeH264PictureLayoutFlagsKHR::INTERLACED_INTERLEAVED_LINES)
    {
        "interlaced-interleaved-lines"
    } else if layout.contains(vk::VideoDecodeH264PictureLayoutFlagsKHR::INTERLACED_SEPARATE_PLANES)
    {
        "interlaced-separate-planes"
    } else {
        "unknown"
    }
}

fn native_vulkan_video_chroma_subsampling_labels(
    flags: vk::VideoChromaSubsamplingFlagsKHR,
) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if flags.contains(vk::VideoChromaSubsamplingFlagsKHR::MONOCHROME) {
        labels.push("monochrome");
    }
    if flags.contains(vk::VideoChromaSubsamplingFlagsKHR::TYPE_420) {
        labels.push("420");
    }
    if flags.contains(vk::VideoChromaSubsamplingFlagsKHR::TYPE_422) {
        labels.push("422");
    }
    if flags.contains(vk::VideoChromaSubsamplingFlagsKHR::TYPE_444) {
        labels.push("444");
    }
    labels
}

fn native_vulkan_video_component_bit_depth_labels(
    flags: vk::VideoComponentBitDepthFlagsKHR,
) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if flags.contains(vk::VideoComponentBitDepthFlagsKHR::TYPE_8) {
        labels.push("8-bit");
    }
    if flags.contains(vk::VideoComponentBitDepthFlagsKHR::TYPE_10) {
        labels.push("10-bit");
    }
    if flags.contains(vk::VideoComponentBitDepthFlagsKHR::TYPE_12) {
        labels.push("12-bit");
    }
    labels
}

fn native_vulkan_video_capability_flag_labels(
    flags: vk::VideoCapabilityFlagsKHR,
) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if flags.contains(vk::VideoCapabilityFlagsKHR::PROTECTED_CONTENT) {
        labels.push("protected-content");
    }
    if flags.contains(vk::VideoCapabilityFlagsKHR::SEPARATE_REFERENCE_IMAGES) {
        labels.push("separate-reference-images");
    }
    labels
}

fn native_vulkan_video_decode_capability_flag_labels(
    flags: vk::VideoDecodeCapabilityFlagsKHR,
) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if flags.contains(vk::VideoDecodeCapabilityFlagsKHR::DPB_AND_OUTPUT_COINCIDE) {
        labels.push("dpb-and-output-coincide");
    }
    if flags.contains(vk::VideoDecodeCapabilityFlagsKHR::DPB_AND_OUTPUT_DISTINCT) {
        labels.push("dpb-and-output-distinct");
    }
    labels
}

fn native_vulkan_h264_level_label(level: vk::native::StdVideoH264LevelIdc) -> Option<&'static str> {
    match level {
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_1_0 => Some("1.0"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_1_1 => Some("1.1"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_1_2 => Some("1.2"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_1_3 => Some("1.3"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_2_0 => Some("2.0"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_2_1 => Some("2.1"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_2_2 => Some("2.2"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_3_0 => Some("3.0"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_3_1 => Some("3.1"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_3_2 => Some("3.2"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_4_0 => Some("4.0"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_4_1 => Some("4.1"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_4_2 => Some("4.2"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_5_0 => Some("5.0"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_5_1 => Some("5.1"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_5_2 => Some("5.2"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_6_0 => Some("6.0"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_6_1 => Some("6.1"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_6_2 => Some("6.2"),
        _ => None,
    }
}

fn native_vulkan_h265_level_label(level: vk::native::StdVideoH265LevelIdc) -> Option<&'static str> {
    match level {
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_1_0 => Some("1.0"),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_2_0 => Some("2.0"),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_2_1 => Some("2.1"),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_3_0 => Some("3.0"),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_3_1 => Some("3.1"),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_4_0 => Some("4.0"),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_4_1 => Some("4.1"),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_5_0 => Some("5.0"),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_5_1 => Some("5.1"),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_5_2 => Some("5.2"),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_6_0 => Some("6.0"),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_6_1 => Some("6.1"),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_6_2 => Some("6.2"),
        _ => None,
    }
}

fn native_vulkan_av1_level_label(level: vk::native::StdVideoAV1Level) -> Option<&'static str> {
    match level {
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_2_0 => Some("2.0"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_2_1 => Some("2.1"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_2_2 => Some("2.2"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_2_3 => Some("2.3"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_3_0 => Some("3.0"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_3_1 => Some("3.1"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_3_2 => Some("3.2"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_3_3 => Some("3.3"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_4_0 => Some("4.0"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_4_1 => Some("4.1"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_4_2 => Some("4.2"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_4_3 => Some("4.3"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_5_0 => Some("5.0"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_5_1 => Some("5.1"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_5_2 => Some("5.2"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_5_3 => Some("5.3"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_6_0 => Some("6.0"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_6_1 => Some("6.1"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_6_2 => Some("6.2"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_6_3 => Some("6.3"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_7_0 => Some("7.0"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_7_1 => Some("7.1"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_7_2 => Some("7.2"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_7_3 => Some("7.3"),
        _ => None,
    }
}

fn native_vulkan_format_label(format: vk::Format) -> &'static str {
    match format {
        vk::Format::G8_B8R8_2PLANE_420_UNORM => "G8_B8R8_2PLANE_420_UNORM",
        vk::Format::G8_B8_R8_3PLANE_420_UNORM => "G8_B8_R8_3PLANE_420_UNORM",
        vk::Format::G8_B8R8_2PLANE_422_UNORM => "G8_B8R8_2PLANE_422_UNORM",
        vk::Format::G8_B8_R8_3PLANE_422_UNORM => "G8_B8_R8_3PLANE_422_UNORM",
        vk::Format::G8_B8R8_2PLANE_444_UNORM => "G8_B8R8_2PLANE_444_UNORM",
        vk::Format::G8_B8_R8_3PLANE_444_UNORM => "G8_B8_R8_3PLANE_444_UNORM",
        vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16 => {
            "G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16"
        }
        vk::Format::G10X6_B10X6_R10X6_3PLANE_420_UNORM_3PACK16 => {
            "G10X6_B10X6_R10X6_3PLANE_420_UNORM_3PACK16"
        }
        vk::Format::G16_B16R16_2PLANE_420_UNORM => "G16_B16R16_2PLANE_420_UNORM",
        vk::Format::R8G8B8A8_UNORM => "R8G8B8A8_UNORM",
        vk::Format::B8G8R8A8_UNORM => "B8G8R8A8_UNORM",
        vk::Format::R8_UNORM => "R8_UNORM",
        vk::Format::R8G8_UNORM => "R8G8_UNORM",
        _ => "unknown",
    }
}

fn native_vulkan_image_type_label(image_type: vk::ImageType) -> &'static str {
    match image_type {
        vk::ImageType::TYPE_1D => "1d",
        vk::ImageType::TYPE_2D => "2d",
        vk::ImageType::TYPE_3D => "3d",
        _ => "unknown",
    }
}

fn native_vulkan_image_tiling_label(image_tiling: vk::ImageTiling) -> &'static str {
    match image_tiling {
        vk::ImageTiling::OPTIMAL => "optimal",
        vk::ImageTiling::LINEAR => "linear",
        vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT => "drm-format-modifier",
        _ => "unknown",
    }
}

fn native_vulkan_image_usage_flag_labels(flags: vk::ImageUsageFlags) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if flags.contains(vk::ImageUsageFlags::TRANSFER_SRC) {
        labels.push("transfer-src");
    }
    if flags.contains(vk::ImageUsageFlags::TRANSFER_DST) {
        labels.push("transfer-dst");
    }
    if flags.contains(vk::ImageUsageFlags::SAMPLED) {
        labels.push("sampled");
    }
    if flags.contains(vk::ImageUsageFlags::STORAGE) {
        labels.push("storage");
    }
    if flags.contains(vk::ImageUsageFlags::COLOR_ATTACHMENT) {
        labels.push("color-attachment");
    }
    if flags.contains(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT) {
        labels.push("depth-stencil-attachment");
    }
    if flags.contains(vk::ImageUsageFlags::TRANSIENT_ATTACHMENT) {
        labels.push("transient-attachment");
    }
    if flags.contains(vk::ImageUsageFlags::INPUT_ATTACHMENT) {
        labels.push("input-attachment");
    }
    if flags.contains(vk::ImageUsageFlags::VIDEO_DECODE_SRC_KHR) {
        labels.push("video-decode-src");
    }
    if flags.contains(vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR) {
        labels.push("video-decode-dst");
    }
    if flags.contains(vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR) {
        labels.push("video-decode-dpb");
    }
    labels
}

fn native_vulkan_buffer_usage_flag_labels(flags: vk::BufferUsageFlags) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if flags.contains(vk::BufferUsageFlags::TRANSFER_SRC) {
        labels.push("transfer-src");
    }
    if flags.contains(vk::BufferUsageFlags::TRANSFER_DST) {
        labels.push("transfer-dst");
    }
    if flags.contains(vk::BufferUsageFlags::UNIFORM_TEXEL_BUFFER) {
        labels.push("uniform-texel-buffer");
    }
    if flags.contains(vk::BufferUsageFlags::STORAGE_TEXEL_BUFFER) {
        labels.push("storage-texel-buffer");
    }
    if flags.contains(vk::BufferUsageFlags::UNIFORM_BUFFER) {
        labels.push("uniform-buffer");
    }
    if flags.contains(vk::BufferUsageFlags::STORAGE_BUFFER) {
        labels.push("storage-buffer");
    }
    if flags.contains(vk::BufferUsageFlags::INDEX_BUFFER) {
        labels.push("index-buffer");
    }
    if flags.contains(vk::BufferUsageFlags::VERTEX_BUFFER) {
        labels.push("vertex-buffer");
    }
    if flags.contains(vk::BufferUsageFlags::INDIRECT_BUFFER) {
        labels.push("indirect-buffer");
    }
    if flags.contains(vk::BufferUsageFlags::VIDEO_DECODE_SRC_KHR) {
        labels.push("video-decode-src");
    }
    labels
}

fn native_vulkan_image_create_flag_labels(flags: vk::ImageCreateFlags) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if flags.contains(vk::ImageCreateFlags::SPARSE_BINDING) {
        labels.push("sparse-binding");
    }
    if flags.contains(vk::ImageCreateFlags::SPARSE_RESIDENCY) {
        labels.push("sparse-residency");
    }
    if flags.contains(vk::ImageCreateFlags::SPARSE_ALIASED) {
        labels.push("sparse-aliased");
    }
    if flags.contains(vk::ImageCreateFlags::MUTABLE_FORMAT) {
        labels.push("mutable-format");
    }
    if flags.contains(vk::ImageCreateFlags::CUBE_COMPATIBLE) {
        labels.push("cube-compatible");
    }
    if flags.contains(vk::ImageCreateFlags::DISJOINT) {
        labels.push("disjoint");
    }
    labels
}

fn native_vulkan_memory_property_flag_labels(flags: vk::MemoryPropertyFlags) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if flags.contains(vk::MemoryPropertyFlags::DEVICE_LOCAL) {
        labels.push("device-local");
    }
    if flags.contains(vk::MemoryPropertyFlags::HOST_VISIBLE) {
        labels.push("host-visible");
    }
    if flags.contains(vk::MemoryPropertyFlags::HOST_COHERENT) {
        labels.push("host-coherent");
    }
    if flags.contains(vk::MemoryPropertyFlags::HOST_CACHED) {
        labels.push("host-cached");
    }
    if flags.contains(vk::MemoryPropertyFlags::LAZILY_ALLOCATED) {
        labels.push("lazily-allocated");
    }
    if flags.contains(vk::MemoryPropertyFlags::PROTECTED) {
        labels.push("protected");
    }
    labels
}

fn native_vulkan_extension_properties_name(properties: &vk::ExtensionProperties) -> String {
    unsafe { CStr::from_ptr(properties.extension_name.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

fn native_vulkan_api_version_label(version: u32) -> String {
    format!(
        "{}.{}.{}",
        vk::api_version_major(version),
        vk::api_version_minor(version),
        vk::api_version_patch(version)
    )
}

struct NativeVulkanPresentQueueQuery {
    selection: NativeVulkanPresentQueueSelection,
    #[allow(dead_code)]
    physical_device_count: usize,
    #[allow(dead_code)]
    present_queue_family_count: usize,
}

fn select_native_vulkan_present_queue(
    instance: &ash::Instance,
    surface_loader: &ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
) -> Result<NativeVulkanPresentQueueQuery, NativeVulkanError> {
    let physical_devices = unsafe { instance.enumerate_physical_devices() }.map_err(|result| {
        NativeVulkanError::Vulkan {
            operation: "vkEnumeratePhysicalDevices",
            result,
        }
    })?;
    let mut present_queue_family_count = 0usize;
    let mut selected = None;

    for (physical_device_index, physical_device) in physical_devices.iter().copied().enumerate() {
        let properties = unsafe { instance.get_physical_device_properties(physical_device) };
        let queue_families =
            unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
        for (queue_family_index, queue_family) in queue_families.iter().enumerate() {
            let supports_surface = unsafe {
                surface_loader.get_physical_device_surface_support(
                    physical_device,
                    queue_family_index as u32,
                    surface,
                )
            }
            .map_err(|result| NativeVulkanError::Vulkan {
                operation: "vkGetPhysicalDeviceSurfaceSupportKHR",
                result,
            })?;
            if !supports_surface {
                continue;
            }
            present_queue_family_count += 1;

            let supports_graphics = queue_family.queue_flags.contains(vk::QueueFlags::GRAPHICS);
            if selected.is_none() && supports_graphics {
                selected = Some(NativeVulkanPresentQueueSelection {
                    physical_device,
                    physical_device_index,
                    physical_device_name: native_vulkan_physical_device_name(properties),
                    physical_device_type: native_vulkan_physical_device_type_label(
                        properties.device_type,
                    ),
                    queue_family_index: queue_family_index as u32,
                });
            }
        }
    }

    let Some(selection) = selected else {
        return Err(NativeVulkanError::MissingPresentQueue);
    };
    Ok(NativeVulkanPresentQueueQuery {
        selection,
        physical_device_count: physical_devices.len(),
        present_queue_family_count,
    })
}

fn ensure_native_vulkan_device_extension(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    extension: &'static CStr,
) -> Result<(), NativeVulkanError> {
    let extensions = unsafe { instance.enumerate_device_extension_properties(physical_device) }
        .map_err(|result| NativeVulkanError::Vulkan {
            operation: "vkEnumerateDeviceExtensionProperties",
            result,
        })?;
    if extensions
        .iter()
        .filter_map(|property| property.extension_name_as_c_str().ok())
        .any(|name| name == extension)
    {
        Ok(())
    } else {
        Err(NativeVulkanError::MissingDeviceExtension(
            ash_extension_name(extension),
        ))
    }
}

struct NativeVulkanSwapchainPlan {
    create_info: vk::SwapchainCreateInfoKHR<'static>,
    format: vk::SurfaceFormatKHR,
    present_mode: vk::PresentModeKHR,
    extent: vk::Extent2D,
}

fn create_native_vulkan_swapchain_plan(
    surface_loader: &ash::khr::surface::Instance,
    physical_device: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
    _logical_size: (u32, u32),
    buffer_size: (u32, u32),
) -> Result<NativeVulkanSwapchainPlan, NativeVulkanError> {
    let capabilities = unsafe {
        surface_loader.get_physical_device_surface_capabilities(physical_device, surface)
    }
    .map_err(|result| NativeVulkanError::Vulkan {
        operation: "vkGetPhysicalDeviceSurfaceCapabilitiesKHR",
        result,
    })?;
    if !capabilities
        .supported_usage_flags
        .contains(vk::ImageUsageFlags::TRANSFER_DST)
    {
        return Err(NativeVulkanError::UnsupportedSwapchainUsage("TRANSFER_DST"));
    }
    if !capabilities
        .supported_usage_flags
        .contains(vk::ImageUsageFlags::COLOR_ATTACHMENT)
    {
        return Err(NativeVulkanError::UnsupportedSwapchainUsage(
            "COLOR_ATTACHMENT",
        ));
    }
    let formats =
        unsafe { surface_loader.get_physical_device_surface_formats(physical_device, surface) }
            .map_err(|result| NativeVulkanError::Vulkan {
                operation: "vkGetPhysicalDeviceSurfaceFormatsKHR",
                result,
            })?;
    let format = choose_native_vulkan_surface_format(&formats)?;
    let present_modes = unsafe {
        surface_loader.get_physical_device_surface_present_modes(physical_device, surface)
    }
    .map_err(|result| NativeVulkanError::Vulkan {
        operation: "vkGetPhysicalDeviceSurfacePresentModesKHR",
        result,
    })?;
    let present_mode = choose_native_vulkan_present_mode(&present_modes);
    let extent = choose_native_vulkan_swapchain_extent(&capabilities, buffer_size)?;
    let image_count = native_vulkan_swapchain_image_count(&capabilities);
    let composite_alpha =
        choose_native_vulkan_composite_alpha(capabilities.supported_composite_alpha);
    let create_info = vk::SwapchainCreateInfoKHR::default()
        .surface(surface)
        .min_image_count(image_count)
        .image_format(format.format)
        .image_color_space(format.color_space)
        .image_extent(extent)
        .image_array_layers(1)
        .image_usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::COLOR_ATTACHMENT)
        .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
        .pre_transform(capabilities.current_transform)
        .composite_alpha(composite_alpha)
        .present_mode(present_mode)
        .clipped(true);

    Ok(NativeVulkanSwapchainPlan {
        create_info,
        format,
        present_mode,
        extent,
    })
}

fn create_native_vulkan_swapchain_image_views(
    device: &ash::Device,
    images: &[vk::Image],
    format: vk::Format,
) -> Result<Vec<vk::ImageView>, NativeVulkanError> {
    let mut views = Vec::with_capacity(images.len());
    for image in images {
        let create_info = vk::ImageViewCreateInfo::default()
            .image(*image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(native_vulkan_color_subresource_range());
        let view = match unsafe { device.create_image_view(&create_info, None) } {
            Ok(view) => view,
            Err(result) => {
                for view in views {
                    unsafe {
                        device.destroy_image_view(view, None);
                    }
                }
                return Err(NativeVulkanError::Vulkan {
                    operation: "vkCreateImageView(swapchain)",
                    result,
                });
            }
        };
        views.push(view);
    }
    Ok(views)
}

fn choose_native_vulkan_surface_format(
    formats: &[vk::SurfaceFormatKHR],
) -> Result<vk::SurfaceFormatKHR, NativeVulkanError> {
    if formats.is_empty() {
        return Err(NativeVulkanError::MissingSurfaceFormat);
    }
    formats
        .iter()
        .copied()
        .find(|format| {
            format.format == vk::Format::B8G8R8A8_UNORM
                && format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
        })
        .or_else(|| {
            formats.iter().copied().find(|format| {
                format.format == vk::Format::B8G8R8A8_SRGB
                    && format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
            })
        })
        .or_else(|| formats.first().copied())
        .ok_or(NativeVulkanError::MissingSurfaceFormat)
}

fn choose_native_vulkan_present_mode(present_modes: &[vk::PresentModeKHR]) -> vk::PresentModeKHR {
    if present_modes.contains(&vk::PresentModeKHR::FIFO) {
        vk::PresentModeKHR::FIFO
    } else {
        present_modes
            .first()
            .copied()
            .unwrap_or(vk::PresentModeKHR::FIFO)
    }
}

fn choose_native_vulkan_swapchain_extent(
    capabilities: &vk::SurfaceCapabilitiesKHR,
    logical_size: (u32, u32),
) -> Result<vk::Extent2D, NativeVulkanError> {
    if let Some((width, height)) = native_vulkan_extent(capabilities.current_extent) {
        return Ok(vk::Extent2D { width, height });
    }
    let width = logical_size.0.clamp(
        capabilities.min_image_extent.width,
        capabilities.max_image_extent.width,
    );
    let height = logical_size.1.clamp(
        capabilities.min_image_extent.height,
        capabilities.max_image_extent.height,
    );
    if width == 0 || height == 0 {
        return Err(NativeVulkanError::InvalidSwapchainExtent);
    }
    Ok(vk::Extent2D { width, height })
}

fn native_vulkan_swapchain_image_count(capabilities: &vk::SurfaceCapabilitiesKHR) -> u32 {
    let preferred = capabilities.min_image_count.max(2);
    if capabilities.max_image_count > 0 {
        preferred.min(capabilities.max_image_count)
    } else {
        preferred
    }
}

fn choose_native_vulkan_composite_alpha(
    flags: vk::CompositeAlphaFlagsKHR,
) -> vk::CompositeAlphaFlagsKHR {
    [
        vk::CompositeAlphaFlagsKHR::OPAQUE,
        vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
        vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED,
        vk::CompositeAlphaFlagsKHR::INHERIT,
    ]
    .into_iter()
    .find(|flag| flags.contains(*flag))
    .unwrap_or(vk::CompositeAlphaFlagsKHR::OPAQUE)
}

fn native_vulkan_color_subresource_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1)
}

fn create_native_vulkan_instance() -> Result<(ash::Entry, ash::Instance), NativeVulkanError> {
    let entry =
        unsafe { ash::Entry::load() }.map_err(|err| NativeVulkanError::Loading(err.to_string()))?;
    let app_name = CString::new("gilder-native-vulkan").expect("static app name has no nul");
    let engine_name = CString::new("gilder").expect("static engine name has no nul");
    let app_info = vk::ApplicationInfo::default()
        .application_name(app_name.as_c_str())
        .application_version(1)
        .engine_name(engine_name.as_c_str())
        .engine_version(1)
        .api_version(vk::API_VERSION_1_3);
    let extension_names = [
        ash::khr::surface::NAME.as_ptr(),
        ash::khr::wayland_surface::NAME.as_ptr(),
    ];
    let create_info = vk::InstanceCreateInfo::default()
        .application_info(&app_info)
        .enabled_extension_names(&extension_names);
    let instance = unsafe { entry.create_instance(&create_info, None) }.map_err(|result| {
        NativeVulkanError::Vulkan {
            operation: "vkCreateInstance",
            result,
        }
    })?;

    Ok((entry, instance))
}

fn native_vulkan_physical_device_name(properties: vk::PhysicalDeviceProperties) -> String {
    unsafe { CStr::from_ptr(properties.device_name.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

fn native_vulkan_physical_device_type_label(device_type: vk::PhysicalDeviceType) -> &'static str {
    match device_type {
        vk::PhysicalDeviceType::OTHER => "other",
        vk::PhysicalDeviceType::INTEGRATED_GPU => "integrated-gpu",
        vk::PhysicalDeviceType::DISCRETE_GPU => "discrete-gpu",
        vk::PhysicalDeviceType::VIRTUAL_GPU => "virtual-gpu",
        vk::PhysicalDeviceType::CPU => "cpu",
        _ => "unknown",
    }
}

fn native_vulkan_present_mode_label(present_mode: vk::PresentModeKHR) -> &'static str {
    match present_mode {
        vk::PresentModeKHR::IMMEDIATE => "immediate",
        vk::PresentModeKHR::MAILBOX => "mailbox",
        vk::PresentModeKHR::FIFO => "fifo",
        vk::PresentModeKHR::FIFO_RELAXED => "fifo-relaxed",
        _ => "unknown",
    }
}

fn native_vulkan_extent(extent: vk::Extent2D) -> Option<(u32, u32)> {
    if extent.width == u32::MAX || extent.height == u32::MAX {
        None
    } else {
        Some((extent.width, extent.height))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeVulkanWallpaperType {
    StaticImage,
    Video,
    Web,
    SceneLite,
    Shader,
    Playlist,
}

pub const WALLPAPER_TYPE_CONTRACT: &[NativeVulkanWallpaperType] = &[
    NativeVulkanWallpaperType::StaticImage,
    NativeVulkanWallpaperType::Video,
    NativeVulkanWallpaperType::Web,
    NativeVulkanWallpaperType::SceneLite,
    NativeVulkanWallpaperType::Shader,
    NativeVulkanWallpaperType::Playlist,
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanWallpaperTypeSupport {
    pub wallpaper_type: NativeVulkanWallpaperType,
    pub current_vulkan_item: bool,
    pub current_renderer_status: &'static str,
    pub target_vulkan_path: &'static str,
}

pub fn wallpaper_type_support_matrix() -> Vec<NativeVulkanWallpaperTypeSupport> {
    vec![
        NativeVulkanWallpaperTypeSupport {
            wallpaper_type: NativeVulkanWallpaperType::StaticImage,
            current_vulkan_item: true,
            current_renderer_status: "CPU decode/fit into staging buffer, copied into swapchain image",
            target_vulkan_path: "decode image -> sampled Vulkan image -> fit-aware textured fullscreen pass",
        },
        NativeVulkanWallpaperTypeSupport {
            wallpaper_type: NativeVulkanWallpaperType::Video,
            current_vulkan_item: true,
            current_renderer_status: "video render item runs through native Vulkan lifecycle; GStreamer appsink feeds CUDA importer on NVIDIA; DMABuf/VAAPI importer still pending",
            target_vulkan_path: "GStreamer decode -> importer-specific CUDAMemory/DMABuf/EGLImage/Vulkan image -> Vulkan YUV sampling",
        },
        NativeVulkanWallpaperTypeSupport {
            wallpaper_type: NativeVulkanWallpaperType::Web,
            current_vulkan_item: false,
            current_renderer_status: "helper contract only; current render plan may fall back to static image",
            target_vulkan_path: "Web helper -> DMABuf/EGLImage/shared-frame handoff -> Vulkan composite",
        },
        NativeVulkanWallpaperTypeSupport {
            wallpaper_type: NativeVulkanWallpaperType::SceneLite,
            current_vulkan_item: true,
            current_renderer_status: "render item mapped; scene draw pass not implemented yet",
            target_vulkan_path: "deterministic scene snapshot -> Vulkan shape/image/text passes",
        },
        NativeVulkanWallpaperTypeSupport {
            wallpaper_type: NativeVulkanWallpaperType::Shader,
            current_vulkan_item: false,
            current_renderer_status: "shader contract only; current render plan may fall back to static image",
            target_vulkan_path: "fullscreen triangle -> GLSL/WGSL-derived SPIR-V -> time/property uniforms",
        },
        NativeVulkanWallpaperTypeSupport {
            wallpaper_type: NativeVulkanWallpaperType::Playlist,
            current_vulkan_item: false,
            current_renderer_status: "playlist selection remains in core render sync; selected child maps to Vulkan item",
            target_vulkan_path: "core playlist decision -> selected child item -> same Vulkan runtime path",
        },
    ]
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum NativeVulkanRenderItem {
    Clear {
        output_name: String,
    },
    StaticImage {
        output_name: String,
        source: PathBuf,
        fit: FitMode,
        background: Option<String>,
        renderer_status: &'static str,
    },
    Video {
        output_name: String,
        source: PathBuf,
        poster: Option<PathBuf>,
        fit: FitMode,
        loop_playback: bool,
        muted: bool,
        manifest_max_fps: Option<u32>,
        target_max_fps: Option<u32>,
        decoder_policy: VideoDecoderPolicy,
        start_offset_ms: u64,
        renderer_status: &'static str,
    },
    Slideshow {
        output_name: String,
        sources: Vec<PathBuf>,
        interval_ms: u64,
        transition: Transition,
        fit: FitMode,
        target_max_fps: Option<u32>,
        renderer_status: &'static str,
    },
    SceneLite {
        output_name: String,
        fallback: Option<PathBuf>,
        display_image: Option<PathBuf>,
        layer_count: usize,
        target_max_fps: Option<u32>,
        renderer_status: &'static str,
    },
}

impl NativeVulkanRenderItem {
    pub fn wallpaper_type(&self) -> NativeVulkanWallpaperType {
        match self {
            Self::Clear { .. } => NativeVulkanWallpaperType::StaticImage,
            Self::StaticImage { .. } => NativeVulkanWallpaperType::StaticImage,
            Self::Video { .. } => NativeVulkanWallpaperType::Video,
            Self::Slideshow { .. } => NativeVulkanWallpaperType::Playlist,
            Self::SceneLite { .. } => NativeVulkanWallpaperType::SceneLite,
        }
    }
}

pub fn render_items_from_sync_plan(plan: &StaticRenderSyncPlan) -> Vec<NativeVulkanRenderItem> {
    plan.plans
        .iter()
        .map(native_vulkan_static_item)
        .chain(plan.video_plans.iter().map(native_vulkan_video_item))
        .chain(
            plan.slideshow_plans
                .iter()
                .map(native_vulkan_slideshow_item),
        )
        .chain(
            plan.scene_lite_plans
                .iter()
                .map(native_vulkan_scene_lite_item),
        )
        .collect()
}

fn native_vulkan_static_item(plan: &StaticWallpaperPlan) -> NativeVulkanRenderItem {
    NativeVulkanRenderItem::StaticImage {
        output_name: plan.output_name.clone(),
        source: plan.source.clone(),
        fit: plan.fit,
        background: plan.background.clone(),
        renderer_status: "cpu-fit-staging-copy",
    }
}

fn native_vulkan_video_item(plan: &VideoWallpaperPlan) -> NativeVulkanRenderItem {
    NativeVulkanRenderItem::Video {
        output_name: plan.output_name.clone(),
        source: plan.source.clone(),
        poster: plan.poster.clone(),
        fit: plan.fit,
        loop_playback: plan.loop_playback,
        muted: plan.muted,
        manifest_max_fps: plan.manifest_max_fps,
        target_max_fps: plan.target_max_fps,
        decoder_policy: plan.decoder_policy,
        start_offset_ms: plan.start_offset_ms,
        renderer_status: "vulkan-lifecycle-video-placeholder",
    }
}

fn native_vulkan_video_runtime_snapshot(
    item: &NativeVulkanRenderItem,
    frontend: Option<NativeVulkanGstVideoFrontendSnapshot>,
    import: Option<NativeVulkanVideoImportSnapshot>,
    rendered_frames: u64,
    poster_upload_bytes: Option<u64>,
) -> Option<NativeVulkanVideoRuntimeSnapshot> {
    let NativeVulkanRenderItem::Video {
        source,
        poster,
        fit,
        loop_playback,
        muted,
        manifest_max_fps,
        target_max_fps,
        decoder_policy,
        start_offset_ms,
        ..
    } = item
    else {
        return None;
    };

    let frontend_status = match frontend.as_ref() {
        Some(frontend) if frontend.frames_received > 0 => "appsink-receiving-samples",
        Some(_) => "appsink-started-waiting-for-samples",
        None if poster.is_some() => "not-started-poster-placeholder",
        None => "not-started-clear-placeholder",
    };
    let handoff_status = match frontend.as_ref() {
        Some(frontend) if frontend.frames_received > 0 => "appsink-sample-handoff-active",
        Some(_) => "appsink-started-no-sample-yet",
        None => "pending-appsink-dmabuf-or-gpu-memory-handoff",
    };
    let frames_received = frontend
        .as_ref()
        .map(|frontend| frontend.frames_received)
        .unwrap_or(0);
    let frames_imported = import
        .as_ref()
        .map(|import| import.frames_imported)
        .unwrap_or(0);
    let received_placeholder_frames = rendered_frames.saturating_sub(frames_imported);

    Some(NativeVulkanVideoRuntimeSnapshot {
        source: source.clone(),
        poster: poster.clone(),
        fit: *fit,
        loop_playback: *loop_playback,
        muted: *muted,
        manifest_max_fps: *manifest_max_fps,
        target_max_fps: *target_max_fps,
        decoder_policy: *decoder_policy,
        start_offset_ms: *start_offset_ms,
        frontend: if frontend.is_some() {
            "gstreamer-appsink"
        } else {
            "gstreamer-planned"
        },
        frontend_status,
        handoff_status,
        texture_import_status: import
            .as_ref()
            .map(|import| import.texture_import_status)
            .unwrap_or("not-importing-yet"),
        audio_status: if *muted {
            "muted-no-audio-pipeline"
        } else {
            "planned-separate-audio-pipeline"
        },
        gst_state: frontend
            .as_ref()
            .and_then(|frontend| frontend.gst_state.clone()),
        eos_messages: frontend
            .as_ref()
            .map(|frontend| frontend.eos_messages)
            .unwrap_or(0),
        segment_done_messages: frontend
            .as_ref()
            .map(|frontend| frontend.segment_done_messages)
            .unwrap_or(0),
        frames_received,
        frames_imported,
        rendered_placeholder_frames: received_placeholder_frames,
        poster_upload_bytes,
        last_import_size: import.as_ref().and_then(|import| import.last_import_size),
        last_import_memory_path: import
            .as_ref()
            .and_then(|import| import.last_import_memory_path.clone()),
        last_import_error: import
            .as_ref()
            .and_then(|import| import.last_import_error.clone()),
        last_import_elapsed_us: import
            .as_ref()
            .and_then(|import| import.last_import_elapsed_us),
        max_import_elapsed_us: import
            .as_ref()
            .and_then(|import| import.max_import_elapsed_us),
        last_sample_caps: frontend
            .as_ref()
            .and_then(|frontend| frontend.last_sample_caps.clone()),
        last_sample_format: frontend
            .as_ref()
            .and_then(|frontend| frontend.last_sample_format.clone()),
        last_sample_size: frontend
            .as_ref()
            .and_then(|frontend| frontend.last_sample_size),
        last_sample_pts_ms: frontend
            .as_ref()
            .and_then(|frontend| frontend.last_sample_pts_ms),
        last_sample_duration_ms: frontend
            .as_ref()
            .and_then(|frontend| frontend.last_sample_duration_ms),
        last_sample_pts_delta_ms: frontend
            .as_ref()
            .and_then(|frontend| frontend.last_sample_pts_delta_ms),
        last_sample_memory_types: frontend
            .as_ref()
            .map(|frontend| frontend.last_sample_memory_types.clone())
            .unwrap_or_default(),
        actual_decoders: frontend
            .as_ref()
            .map(|frontend| frontend.actual_decoders.clone())
            .unwrap_or_default(),
        decoder_policy_status: frontend
            .as_ref()
            .and_then(|frontend| frontend.decoder_policy_status.clone()),
        caps_report_count: frontend
            .as_ref()
            .map(|frontend| frontend.caps_report_count)
            .unwrap_or(0),
        caps_memory_features: frontend
            .as_ref()
            .map(|frontend| frontend.caps_memory_features.clone())
            .unwrap_or_default(),
        caps_reports: frontend
            .as_ref()
            .map(|frontend| frontend.caps_reports.clone())
            .unwrap_or_default(),
        last_error: frontend
            .as_ref()
            .and_then(|frontend| frontend.last_error.clone()),
    })
}

fn native_vulkan_slideshow_item(plan: &SlideshowWallpaperPlan) -> NativeVulkanRenderItem {
    NativeVulkanRenderItem::Slideshow {
        output_name: plan.output_name.clone(),
        sources: plan.sources.clone(),
        interval_ms: plan.interval_ms,
        transition: plan.transition,
        fit: plan.fit,
        target_max_fps: plan.target_max_fps,
        renderer_status: "planned-slideshow-static-texture-sequence",
    }
}

fn native_vulkan_scene_lite_item(plan: &SceneLiteWallpaperPlan) -> NativeVulkanRenderItem {
    NativeVulkanRenderItem::SceneLite {
        output_name: plan.output_name.clone(),
        fallback: plan.fallback.clone(),
        display_image: match &plan.display {
            Some(SceneLiteDisplayPlan::Image { source, .. }) => Some(source.clone()),
            Some(SceneLiteDisplayPlan::Color { .. }) | None => None,
        },
        layer_count: plan.layers.len(),
        target_max_fps: plan.target_max_fps,
        renderer_status: "planned-scene-lite-vulkan-passes",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanBackendContract {
    pub backend_name: &'static str,
    pub default_renderer_candidate: bool,
    pub wallpaper_types: &'static [NativeVulkanWallpaperType],
    pub wallpaper_type_support: Vec<NativeVulkanWallpaperTypeSupport>,
    pub layer_shell_host: &'static str,
    pub render_plan_boundary: &'static str,
    pub lifecycle_boundary: &'static str,
    pub resource_telemetry_boundary: &'static str,
    pub required_instance_extensions: Vec<&'static str>,
    pub required_device_extensions: Vec<&'static str>,
    pub video_interop: NativeVulkanVideoInteropContract,
    pub web_interop: NativeVulkanWebInteropContract,
}

pub fn backend_contract() -> NativeVulkanBackendContract {
    NativeVulkanBackendContract {
        backend_name: "native-vulkan",
        default_renderer_candidate: false,
        wallpaper_types: WALLPAPER_TYPE_CONTRACT,
        wallpaper_type_support: wallpaper_type_support_matrix(),
        layer_shell_host: "reuse NativeWaylandHost raw wl_display/wl_surface first, then move ownership here",
        render_plan_boundary: "consume existing renderer plans; do not introduce Vulkan-only manifest semantics",
        lifecycle_boundary: "pause-dynamic, hidden/fullscreen/session release, resize, and output selection stay backend-neutral",
        resource_telemetry_boundary: "report CPU/RSS/PSS/private_dirty/GPU resource counts through stable renderer telemetry",
        required_instance_extensions: required_instance_extensions(),
        required_device_extensions: required_device_extensions(),
        video_interop: video_interop_contract(),
        web_interop: web_interop_contract(),
    }
}

pub fn required_instance_extensions() -> Vec<&'static str> {
    vec![
        ash_extension_name(ash::khr::surface::NAME),
        ash_extension_name(ash::khr::wayland_surface::NAME),
    ]
}

pub fn required_device_extensions() -> Vec<&'static str> {
    vec![
        ash_extension_name(ash::khr::swapchain::NAME),
        ash_extension_name(ash::khr::external_memory_fd::NAME),
        ash_extension_name(ash::khr::external_semaphore_fd::NAME),
        ash_extension_name(ash::khr::timeline_semaphore::NAME),
        ash_extension_name(ash::ext::external_memory_dma_buf::NAME),
        ash_extension_name(ash::ext::image_drm_format_modifier::NAME),
    ]
}

fn ash_extension_name(name: &'static CStr) -> &'static str {
    name.to_str()
        .expect("Vulkan extension names shipped by ash must be UTF-8")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoInteropContract {
    pub target_memory_flow: &'static str,
    pub current_baseline: &'static str,
    pub target_sampling: &'static str,
    pub avoids_default_rgba_upload: bool,
    pub decoder_policy: &'static str,
    pub audio_strategy: &'static str,
    pub known_blockers: &'static [&'static str],
}

pub fn video_interop_contract() -> NativeVulkanVideoInteropContract {
    NativeVulkanVideoInteropContract {
        target_memory_flow: "decoder GPU memory -> importable DMABuf/EGLImage/Vulkan image -> Vulkan YUV sampling",
        current_baseline: "native-wgpu GStreamer CUDAMemory -> CUDA copy -> external Vulkan image planes -> wgpu present",
        target_sampling: "NV12/P010/YUV planes sampled directly in Vulkan before RGB composition",
        avoids_default_rgba_upload: true,
        decoder_policy: "prefer GStreamer for codec/audio coverage; allow Vulkan Video or libavcodec import paths when they win evidence",
        audio_strategy: "keep audio pipeline separate from the video texture path so decoder choice does not block playback support",
        known_blockers: &[
            "direct gst_cuda_memory_export fd import returned zero Vulkan memory_type_bits on NVIDIA",
            "GLMemory DMABuf export may require libnvrtc on nvcodec systems",
            "default switch requires real Wayland evidence beating the current 4K/240 native-wgpu baseline",
        ],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanWebInteropContract {
    pub helper_boundary: &'static str,
    pub accepted_frame_sources: &'static [&'static str],
    pub blocked_designs: &'static [&'static str],
}

pub fn web_interop_contract() -> NativeVulkanWebInteropContract {
    NativeVulkanWebInteropContract {
        helper_boundary: "WebKitGTK or browser code stays in a helper; native Vulkan receives frames or importable textures",
        accepted_frame_sources: &[
            "DMABuf texture handoff",
            "EGLImage/exportable GL texture handoff",
            "shared-memory frame stream only as a fallback",
        ],
        blocked_designs: &[
            "making GTK/WebKitGTK the native Vulkan renderer host",
            "adding Web-specific daemon or manifest branches",
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn reports_vulkan_spike_as_built_but_not_default() {
        let capabilities = capabilities();

        assert!(capabilities.built);
        assert!(capabilities.experimental);
        assert!(!capabilities.default_enabled);
        assert!(capabilities.reuses_native_wayland_host);
        assert!(capabilities.owns_vulkan_instance_now);
        assert!(capabilities.owns_wayland_vulkan_surface_now);
        assert!(capabilities.owns_vulkan_device_now);
        assert!(capabilities.owns_swapchain_now);
        assert!(capabilities.renders_frames_now);
        assert!(!capabilities.consumes_render_sync);
        assert!(capabilities.direct_video_memory_status.contains("DMABuf"));
    }

    #[test]
    fn labels_vulkan_video_decode_codec_operations() {
        let operations = vk::VideoCodecOperationFlagsKHR::from_raw(
            vk::VideoCodecOperationFlagsKHR::DECODE_H264.as_raw()
                | NATIVE_VULKAN_VIDEO_CODEC_OPERATION_DECODE_VP9,
        );

        let labels = native_vulkan_video_codec_operation_labels(operations);

        assert!(labels.contains(&"decode-h264".to_owned()));
        assert!(labels.contains(&"decode-vp9".to_owned()));
    }

    #[test]
    fn labels_h264_probe_format_requirements() {
        assert_eq!(
            native_vulkan_h264_level_label(
                vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_5_2
            ),
            Some("5.2")
        );

        let usage = vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR
            | vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR
            | vk::ImageUsageFlags::SAMPLED;
        let labels = native_vulkan_image_usage_flag_labels(usage);

        assert!(labels.contains(&"video-decode-dst"));
        assert!(labels.contains(&"video-decode-dpb"));
        assert!(labels.contains(&"sampled"));
        assert!(native_vulkan_video_formats_include_nv12_with_usage(
            &[NativeVulkanVideoFormatPropertiesSnapshot {
                format: "G8_B8R8_2PLANE_420_UNORM",
                format_raw: vk::Format::G8_B8R8_2PLANE_420_UNORM.as_raw(),
                image_type: "2d",
                image_tiling: "optimal",
                image_usage_flags: labels,
                image_create_flags: vec!["mutable-format"],
            }],
            &["video-decode-dst", "video-decode-dpb", "sampled"]
        ));
    }

    #[test]
    fn labels_h265_av1_probe_levels_and_formats() {
        assert_eq!(
            native_vulkan_h265_level_label(
                vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_6_2
            ),
            Some("6.2")
        );
        assert_eq!(
            native_vulkan_av1_level_label(vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_6_3),
            Some("6.3")
        );
        assert_eq!(
            native_vulkan_format_label(vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16),
            "G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16"
        );
    }

    #[test]
    fn scans_h265_annex_b_parameter_sets_and_idr() {
        let bytes = [
            0, 0, 0, 1, 0x40, 0x01, 0xaa, 0xbb, // VPS, type 32
            0, 0, 1, 0x42, 0x01, 0xcc, // SPS, type 33
            0, 0, 0, 1, 0x44, 0x01, 0xdd, // PPS, type 34
            0, 0, 1, 0x26, 0x01, 0xee, // IDR_W_RADL, type 19
        ];

        let stats = native_vulkan_h265_nal_stats(&bytes);

        assert!(stats.has_annex_b_start_codes);
        assert!(stats.parameter_sets_present());
        assert_eq!(stats.vps_count, 1);
        assert_eq!(stats.sps_count, 1);
        assert_eq!(stats.pps_count, 1);
        assert_eq!(stats.idr_count, 1);
        assert_eq!(stats.slice_count, 1);
        assert_eq!(
            stats
                .nal_units
                .iter()
                .map(|unit| unit.nal_type_label)
                .collect::<Vec<_>>(),
            vec!["vps", "sps", "pps", "idr-w-radl"]
        );
        let payloads = native_vulkan_h265_nal_payloads(&bytes);
        assert_eq!(payloads[3].nal_type, 19);
        assert_eq!(payloads[3].start_code_offset, 21);
    }

    #[test]
    fn contract_covers_full_wallpaper_type_matrix() {
        let contract = backend_contract();

        assert_eq!(contract.backend_name, "native-vulkan");
        assert_eq!(
            contract.wallpaper_types,
            &[
                NativeVulkanWallpaperType::StaticImage,
                NativeVulkanWallpaperType::Video,
                NativeVulkanWallpaperType::Web,
                NativeVulkanWallpaperType::SceneLite,
                NativeVulkanWallpaperType::Shader,
                NativeVulkanWallpaperType::Playlist,
            ]
        );
        assert!(contract.video_interop.avoids_default_rgba_upload);
        assert_eq!(contract.wallpaper_type_support.len(), 6);
    }

    #[test]
    fn wallpaper_type_support_marks_current_items_and_future_contracts() {
        let support = wallpaper_type_support_matrix();

        assert_eq!(support.len(), WALLPAPER_TYPE_CONTRACT.len());
        assert!(
            support
                .iter()
                .find(|entry| entry.wallpaper_type == NativeVulkanWallpaperType::StaticImage)
                .is_some_and(|entry| entry.current_vulkan_item)
        );
        assert!(
            support
                .iter()
                .find(|entry| entry.wallpaper_type == NativeVulkanWallpaperType::Video)
                .is_some_and(|entry| entry.current_vulkan_item)
        );
        assert!(
            support
                .iter()
                .find(|entry| entry.wallpaper_type == NativeVulkanWallpaperType::Web)
                .is_some_and(|entry| !entry.current_vulkan_item)
        );
        assert!(
            support
                .iter()
                .find(|entry| entry.wallpaper_type == NativeVulkanWallpaperType::Shader)
                .is_some_and(|entry| !entry.current_vulkan_item)
        );
    }

    #[test]
    fn maps_sync_plan_to_vulkan_items() {
        let sync_plan = StaticRenderSyncPlan {
            plans: vec![StaticWallpaperPlan {
                output_name: "HDMI-A-1".to_owned(),
                source: PathBuf::from("/tmp/static.png"),
                fit: FitMode::Cover,
                background: Some("#000000".to_owned()),
            }],
            video_plans: vec![VideoWallpaperPlan {
                output_name: "HDMI-A-1".to_owned(),
                source: PathBuf::from("/tmp/video.mp4"),
                poster: None,
                fit: FitMode::Contain,
                loop_playback: true,
                muted: true,
                manifest_max_fps: Some(240),
                target_max_fps: Some(240),
                decoder_policy: crate::config::VideoDecoderPolicy::HardwarePreferred,
                start_offset_ms: 0,
            }],
            slideshow_plans: Vec::new(),
            scene_lite_plans: Vec::new(),
            removals: Vec::new(),
            errors: Vec::new(),
            decisions: Vec::new(),
            playlist_clock_dependency: Default::default(),
            cache: Default::default(),
        };

        let items = render_items_from_sync_plan(&sync_plan);

        assert_eq!(items.len(), 2);
        assert!(matches!(
            items[0],
            NativeVulkanRenderItem::StaticImage { .. }
        ));
        assert!(matches!(items[1], NativeVulkanRenderItem::Video { .. }));
        assert_eq!(items[1].wallpaper_type(), NativeVulkanWallpaperType::Video);
        let NativeVulkanRenderItem::Video {
            target_max_fps,
            decoder_policy,
            start_offset_ms,
            renderer_status,
            ..
        } = &items[1]
        else {
            unreachable!("item already matched as video");
        };
        assert_eq!(*target_max_fps, Some(240));
        assert_eq!(
            *decoder_policy,
            crate::config::VideoDecoderPolicy::HardwarePreferred
        );
        assert_eq!(*start_offset_ms, 0);
        assert_eq!(*renderer_status, "vulkan-lifecycle-video-placeholder");
    }

    #[test]
    fn video_runtime_snapshot_reports_pending_gstreamer_handoff() {
        let item = NativeVulkanRenderItem::Video {
            output_name: "HDMI-A-1".to_owned(),
            source: PathBuf::from("/tmp/video.mp4"),
            poster: Some(PathBuf::from("/tmp/poster.png")),
            fit: FitMode::Contain,
            loop_playback: true,
            muted: false,
            manifest_max_fps: Some(240),
            target_max_fps: Some(120),
            decoder_policy: crate::config::VideoDecoderPolicy::HardwareRequired,
            start_offset_ms: 1500,
            renderer_status: "vulkan-lifecycle-video-placeholder",
        };

        let snapshot = native_vulkan_video_runtime_snapshot(&item, None, None, 9, Some(1024))
            .expect("video snapshot");

        assert_eq!(snapshot.frontend, "gstreamer-planned");
        assert_eq!(snapshot.frontend_status, "not-started-poster-placeholder");
        assert_eq!(
            snapshot.handoff_status,
            "pending-appsink-dmabuf-or-gpu-memory-handoff"
        );
        assert_eq!(snapshot.audio_status, "planned-separate-audio-pipeline");
        assert_eq!(snapshot.frames_received, 0);
        assert_eq!(snapshot.frames_imported, 0);
        assert_eq!(snapshot.rendered_placeholder_frames, 9);
        assert_eq!(snapshot.poster_upload_bytes, Some(1024));
        assert_eq!(snapshot.texture_import_status, "not-importing-yet");
        assert_eq!(snapshot.last_import_size, None);
        assert_eq!(snapshot.last_import_memory_path, None);
        assert_eq!(snapshot.last_import_error, None);
        assert_eq!(snapshot.last_import_elapsed_us, None);
        assert_eq!(snapshot.max_import_elapsed_us, None);
        assert_eq!(snapshot.start_offset_ms, 1500);
        assert_eq!(snapshot.gst_state, None);
        assert_eq!(snapshot.decoder_policy_status, None);
        assert_eq!(snapshot.caps_report_count, 0);
        assert_eq!(snapshot.segment_done_messages, 0);
    }

    #[test]
    fn video_runtime_snapshot_reports_active_appsink_frontend() {
        let item = NativeVulkanRenderItem::Video {
            output_name: "HDMI-A-1".to_owned(),
            source: PathBuf::from("/tmp/video.mp4"),
            poster: None,
            fit: FitMode::Cover,
            loop_playback: true,
            muted: true,
            manifest_max_fps: None,
            target_max_fps: Some(240),
            decoder_policy: crate::config::VideoDecoderPolicy::HardwarePreferred,
            start_offset_ms: 0,
            renderer_status: "vulkan-lifecycle-video-placeholder",
        };
        let frontend = NativeVulkanGstVideoFrontendSnapshot {
            gst_state: Some("Playing".to_owned()),
            eos_messages: 0,
            segment_done_messages: 1,
            frames_received: 3,
            last_sample_caps: Some("video/x-raw, format=(string)NV12".to_owned()),
            last_sample_format: Some("NV12".to_owned()),
            last_sample_size: Some((3840, 2160)),
            last_sample_pts_ms: Some(8),
            last_sample_duration_ms: Some(4),
            last_sample_pts_delta_ms: Some(4),
            last_sample_memory_types: vec!["CUDAMemory".to_owned()],
            actual_decoders: vec!["nvh264dec".to_owned()],
            decoder_policy_status: Some("Satisfied".to_owned()),
            caps_report_count: 1,
            caps_memory_features: vec!["memory:CUDAMemory".to_owned()],
            caps_reports: vec![NativeVulkanVideoCapsSnapshot {
                element: "appsink0".to_owned(),
                pad: "sink".to_owned(),
                direction: "sink".to_owned(),
                caps: "video/x-raw(memory:CUDAMemory)".to_owned(),
                source: "current".to_owned(),
                memory_features: vec!["memory:CUDAMemory".to_owned()],
            }],
            last_error: None,
        };
        let import = NativeVulkanVideoImportSnapshot {
            texture_import_status: "importing-cuda-vulkan-image-planes",
            frames_imported: 2,
            last_import_size: Some((3840, 2160)),
            last_import_memory_path: Some(
                "CUDAMemory->CUDA->Vulkan external image planes".to_owned(),
            ),
            last_import_error: None,
            last_import_elapsed_us: Some(900),
            max_import_elapsed_us: Some(1200),
        };

        let snapshot =
            native_vulkan_video_runtime_snapshot(&item, Some(frontend), Some(import), 12, None)
                .unwrap();

        assert_eq!(snapshot.frontend, "gstreamer-appsink");
        assert_eq!(snapshot.frontend_status, "appsink-receiving-samples");
        assert_eq!(snapshot.handoff_status, "appsink-sample-handoff-active");
        assert_eq!(snapshot.frames_received, 3);
        assert_eq!(snapshot.frames_imported, 2);
        assert_eq!(snapshot.segment_done_messages, 1);
        assert_eq!(snapshot.rendered_placeholder_frames, 10);
        assert_eq!(
            snapshot.texture_import_status,
            "importing-cuda-vulkan-image-planes"
        );
        assert_eq!(snapshot.last_import_size, Some((3840, 2160)));
        assert_eq!(
            snapshot.last_import_memory_path.as_deref(),
            Some("CUDAMemory->CUDA->Vulkan external image planes")
        );
        assert_eq!(snapshot.last_import_elapsed_us, Some(900));
        assert_eq!(snapshot.max_import_elapsed_us, Some(1200));
        assert_eq!(snapshot.last_sample_format.as_deref(), Some("NV12"));
        assert_eq!(snapshot.last_sample_pts_ms, Some(8));
        assert_eq!(snapshot.last_sample_duration_ms, Some(4));
        assert_eq!(snapshot.last_sample_pts_delta_ms, Some(4));
        assert_eq!(snapshot.last_sample_memory_types, vec!["CUDAMemory"]);
        assert_eq!(snapshot.actual_decoders, vec!["nvh264dec"]);
        assert_eq!(snapshot.decoder_policy_status.as_deref(), Some("Satisfied"));
        assert_eq!(snapshot.caps_memory_features, vec!["memory:CUDAMemory"]);
    }

    #[test]
    fn parses_static_background_hex() {
        assert_eq!(
            native_vulkan_parse_background(Some("#102030")),
            image::Rgba([0x10, 0x20, 0x30, 255])
        );
        assert_eq!(
            native_vulkan_parse_background(Some("bad")),
            image::Rgba([0, 0, 0, 255])
        );
    }

    #[test]
    fn encodes_bgra_swapchain_pixels() {
        let image = image::RgbaImage::from_pixel(1, 1, image::Rgba([1, 2, 3, 4]));

        assert_eq!(
            native_vulkan_encode_swapchain_pixels(&image, vk::Format::B8G8R8A8_UNORM),
            vec![3, 2, 1, 4]
        );
        assert_eq!(
            native_vulkan_encode_swapchain_pixels(&image, vk::Format::R8G8B8A8_UNORM),
            vec![1, 2, 3, 4]
        );
    }

    #[test]
    fn contain_fit_preserves_letterbox_background() {
        let source = image::RgbaImage::from_pixel(2, 1, image::Rgba([255, 0, 0, 255]));
        let mut canvas = image::RgbaImage::from_pixel(4, 4, image::Rgba([0, 0, 0, 255]));

        native_vulkan_blit_fit(&source, &mut canvas, FitMode::Contain);

        assert_eq!(canvas.get_pixel(0, 0), &image::Rgba([0, 0, 0, 255]));
        assert_eq!(canvas.get_pixel(0, 1), &image::Rgba([255, 0, 0, 255]));
        assert_eq!(canvas.get_pixel(3, 2), &image::Rgba([255, 0, 0, 255]));
        assert_eq!(canvas.get_pixel(0, 3), &image::Rgba([0, 0, 0, 255]));
    }

    #[test]
    fn contract_names_required_vulkan_extensions() {
        let contract = backend_contract();

        assert!(
            contract
                .required_instance_extensions
                .contains(&"VK_KHR_wayland_surface")
        );
        assert!(
            contract
                .required_device_extensions
                .contains(&"VK_KHR_swapchain")
        );
        assert!(
            contract
                .required_device_extensions
                .contains(&"VK_EXT_external_memory_dma_buf")
        );
        assert!(
            contract
                .required_device_extensions
                .contains(&"VK_EXT_image_drm_format_modifier")
        );
    }

    #[test]
    fn unknown_surface_extent_is_none() {
        assert_eq!(
            native_vulkan_extent(vk::Extent2D {
                width: u32::MAX,
                height: u32::MAX,
            }),
            None
        );
        assert_eq!(
            native_vulkan_extent(vk::Extent2D {
                width: 3840,
                height: 2160,
            }),
            Some((3840, 2160))
        );
    }
}
