//! Native Wayland/Vulkan renderer.
//!
//! This module owns the native Wayland/Vulkan renderer path. The backend
//! contract covers native Wayland layer-shell ownership, Vulkan
//! surface/swapchain ownership, and direct video texture interop.

#![allow(unsafe_code)]
#![allow(dead_code)]

use serde::Serialize;
#[cfg(any(feature = "native-vulkan-video", test))]
use std::borrow::Cow;
#[cfg(feature = "native-vulkan-video")]
use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

#[cfg(test)]
use crate::core::FitMode;
use crate::renderer::native_wayland::{NativeWaylandError, NativeWaylandHostOptions};
#[cfg(test)]
use crate::renderer::{StaticWallpaperPlan, VideoWallpaperPlan};
use vulkanalia::vk;

#[cfg(all(
    any(
        feature = "native-vulkan-renderer",
        feature = "native-vulkan-video",
        test
    ),
    target_family = "unix"
))]
unsafe extern "C" {
    #[link_name = "memchr"]
    fn native_vulkan_c_memchr(
        s: *const std::ffi::c_void,
        c: std::os::raw::c_int,
        n: usize,
    ) -> *mut std::ffi::c_void;
}

pub enum NativeVulkanEncodedAccessUnitPayload {
    Empty,
    Owned(Vec<u8>),
    #[cfg(feature = "native-vulkan-video")]
    FfmpegPacket(demux_ffmpeg::NativeVulkanFfmpegPacketPayload),
}

impl NativeVulkanEncodedAccessUnitPayload {
    #[cfg(test)]
    pub(crate) fn owned(bytes: Vec<u8>) -> Self {
        Self::Owned(bytes)
    }

    #[cfg(feature = "native-vulkan-video")]
    fn from_ffmpeg_packet(payload: demux_ffmpeg::NativeVulkanFfmpegPacketPayload) -> Self {
        Self::FfmpegPacket(payload)
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        match self {
            Self::Empty => &[],
            Self::Owned(bytes) => bytes,
            #[cfg(feature = "native-vulkan-video")]
            Self::FfmpegPacket(packet) => packet.bytes(),
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.bytes().len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.bytes().is_empty()
    }

    pub(crate) fn clear(&mut self) {
        *self = Self::Empty;
    }
}

impl Default for NativeVulkanEncodedAccessUnitPayload {
    fn default() -> Self {
        Self::Empty
    }
}

impl fmt::Debug for NativeVulkanEncodedAccessUnitPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeVulkanEncodedAccessUnitPayload")
            .field(
                "model",
                &match self {
                    Self::Empty => "empty",
                    Self::Owned(_) => "owned-vec",
                    #[cfg(feature = "native-vulkan-video")]
                    Self::FfmpegPacket(_) => "ffmpeg-avpacket",
                },
            )
            .field("bytes", &self.len())
            .finish()
    }
}

mod audio;
mod interop;
mod labels;
mod pipeline;
mod present;
mod scene;
mod video;
mod vulkan;

#[cfg(feature = "native-vulkan-video")]
use video::direct as video_direct;
#[cfg(feature = "native-vulkan-video")]
use video::vulkan_extract;

#[cfg(feature = "native-vulkan-video")]
use video::demux;
#[cfg(feature = "native-vulkan-video")]
use video::demux_ffmpeg;

#[cfg(feature = "native-vulkan-video")]
use video::codec_reference;

use audio::policy as audio_policy;
use present::clear_runtime as clear_present_runtime;
use present::render_item;
use present::static_image_runtime as static_image_present_runtime;
use scene::present_runtime as scene_present_runtime;
use scene::runtime as scene_runtime;
use video::codec as video_codec;
use video::codec_snapshots;
use video::flow as video_flow;
#[cfg(any(feature = "native-vulkan-video", test))]
use video::h264;
use video::probe_snapshots as video_probe_snapshots;
use video::route as video_route;
#[cfg(feature = "native-vulkan-video")]
use video::session_snapshots as video_session_snapshots;

pub use audio_policy::{NativeVulkanAudioOutputMode, NativeVulkanAudioOutputPolicy};
pub use clear_present_runtime::run_clear;
#[cfg(feature = "native-vulkan-video")]
use codec_reference::*;
pub use codec_snapshots::*;
pub use interop::{NativeVulkanVideoInteropContract, NativeVulkanWebInteropContract};
use interop::{video_interop_contract, web_interop_contract};
pub use render_item::{NativeVulkanRenderItem, render_items_from_sync_plan};
pub use scene_present_runtime::{
    NativeVulkanSceneAudioCueRuntimeSnapshot, NativeVulkanScenePresentSnapshot,
    NativeVulkanSceneVideoBridgeOptions, run_scene,
};
pub use scene_runtime::{
    NativeVulkanSceneDrawOpSnapshot, NativeVulkanSceneQuadRecordingStepSnapshot,
    NativeVulkanSceneQuadVertexSnapshot, NativeVulkanSceneRuntimeSnapshot,
    NativeVulkanSceneUnsupportedLayerSnapshot,
};
pub use static_image_present_runtime::{run_static_image, run_static_image_vulkanalia};
pub use video_codec::NativeVulkanVideoSessionCodec;
#[cfg(feature = "native-vulkan-video")]
pub use video_direct::{
    NativeVulkanVulkanaliaReadyPrefixRuntimeSnapshot, run_vulkanalia_ready_prefix_video,
};
pub use video_probe_snapshots::*;
pub use video_route::{
    NativeVulkanVideoReadyPrefixCounts, NativeVulkanVideoRunRouteDecision,
    NativeVulkanVideoRunRouteKind, native_vulkan_video_duration_playback_frames,
    native_vulkan_video_playback_frame_count, native_vulkan_video_run_route,
};
#[cfg(feature = "native-vulkan-video")]
pub use video_session_snapshots::*;
pub use vulkan::*;
#[cfg(feature = "native-vulkan-video")]
pub use vulkan_extract::{
    native_vulkan_extract_av1_sequence_header_for_vulkanalia,
    native_vulkan_extract_h264_parameter_sets_for_vulkanalia,
    native_vulkan_extract_h265_parameter_sets_for_vulkanalia,
};

#[cfg(feature = "native-vulkan-video")]
use demux::{NativeVulkanStreamingAccessUnit, NativeVulkanStreamingPacketQueue};
#[cfg(feature = "native-vulkan-video")]
use demux_ffmpeg::{
    NativeVulkanFfmpegCodec, NativeVulkanFfmpegPacketMetadata, NativeVulkanFfmpegPacketPayload,
    NativeVulkanFfmpegStreamingAccessUnit,
    native_vulkan_start_ffmpeg_streaming_packet_queue as native_vulkan_start_streaming_packet_queue,
};

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
    Clear(String),
    StaticImage(String),
    Scene(String),
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
            Self::Clear(err) => write!(f, "clear present error: {err}"),
            Self::StaticImage(err) => write!(f, "static image error: {err}"),
            Self::Scene(err) => write!(f, "scene error: {err}"),
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

pub(super) fn native_vulkan_bool_u32(value: bool) -> u32 {
    value as u32
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeVulkanVideoSessionSmokeOptions {
    pub codec: NativeVulkanVideoSessionCodec,
    pub width: u32,
    pub height: u32,
    pub allocate_video_images: bool,
    pub allocate_bitstream_buffer: bool,
    pub extract_bitstream: bool,
    pub decode_h264_ready_prefix_frames: u32,
    pub decode_h265_ready_prefix_frames: u32,
    pub decode_av1_ready_prefix_frames: u32,
    pub bitstream_source: Option<PathBuf>,
    pub bitstream_extract_max_samples: u32,
    pub h264_required_ready_prefix_access_units: u32,
    pub h265_required_ready_prefix_access_units: u32,
    pub av1_required_ready_prefix_temporal_units: u32,
}

impl Default for NativeVulkanVideoSessionSmokeOptions {
    fn default() -> Self {
        Self {
            codec: NativeVulkanVideoSessionCodec::H265Main8,
            width: 3840,
            height: 2160,
            allocate_video_images: false,
            allocate_bitstream_buffer: false,
            extract_bitstream: false,
            decode_h264_ready_prefix_frames: 0,
            decode_h265_ready_prefix_frames: 0,
            decode_av1_ready_prefix_frames: 0,
            bitstream_source: None,
            bitstream_extract_max_samples: 8,
            h264_required_ready_prefix_access_units: 0,
            h265_required_ready_prefix_access_units: 0,
            av1_required_ready_prefix_temporal_units: 0,
        }
    }
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
    Result<NativeVulkanVulkanaliaDeviceProbeSnapshot, NativeVulkanError>;

pub fn probe_wayland_surface(
    options: NativeVulkanSurfaceProbeOptions,
) -> Result<NativeVulkanVulkanaliaSurfaceSwapchainProbeSnapshot, NativeVulkanError> {
    probe_native_vulkan_vulkanalia_surface_swapchain(
        NativeVulkanVulkanaliaSurfaceSwapchainProbeOptions {
            host: options.host,
            wait_configure_roundtrips: options.wait_configure_roundtrips,
        },
    )
    .map_err(NativeVulkanError::Video)
}

pub fn probe_vulkan_video_decode() -> NativeVulkanVideoDecodeProbeResult {
    probe_native_vulkan_vulkanalia_devices().map_err(NativeVulkanError::Video)
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanDrmDeviceSnapshot {
    pub extension_available: bool,
    pub has_primary: bool,
    pub primary_major: Option<i64>,
    pub primary_minor: Option<i64>,
    pub primary_dev_t: Option<u64>,
    pub primary_node: Option<String>,
    pub has_render: bool,
    pub render_major: Option<i64>,
    pub render_minor: Option<i64>,
    pub render_dev_t: Option<u64>,
    pub render_node: Option<String>,
}

#[cfg(feature = "native-vulkan-video")]
struct NativeVulkanAv1FirstFrameDecodeInfo {
    frame_header_obu_offset: u64,
    frame_header_payload_offset: u64,
    header: NativeVulkanAv1ParsedFrameHeader,
    tile_offsets: Vec<u32>,
    tile_sizes: Vec<u32>,
}

#[cfg(feature = "native-vulkan-video")]
struct NativeVulkanH265AccessUnitExtract {
    payload: NativeVulkanEncodedAccessUnitPayload,
    pts_ns: Option<u64>,
    duration_ns: Option<u64>,
    pts_ms: Option<u64>,
    duration_ms: Option<u64>,
    stats: NativeVulkanH265NalStats,
}

#[cfg(feature = "native-vulkan-video")]
struct NativeVulkanH264AccessUnitExtract {
    payload: NativeVulkanEncodedAccessUnitPayload,
    pts_ns: Option<u64>,
    duration_ns: Option<u64>,
    pts_ms: Option<u64>,
    duration_ms: Option<u64>,
    stats: NativeVulkanH264NalStats,
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanStreamingAccessUnit for NativeVulkanH264AccessUnitExtract {
    type ParameterSets = NativeVulkanH264ParameterSetSnapshot;
    type Snapshot = NativeVulkanH264AccessUnitSnapshot;

    const CODEC_LABEL: &'static str = "H.264";
    const PARAMETER_SETS_LABEL: &'static str = "SPS/PPS";

    fn parse_parameter_sets(bytes: &[u8]) -> Result<Self::ParameterSets, String> {
        native_vulkan_parse_h264_parameter_sets(bytes)
    }

    fn snapshot(
        index: u32,
        access_unit: &Self,
        parameter_sets: &Self::ParameterSets,
    ) -> Self::Snapshot {
        native_vulkan_h264_access_unit_snapshot(index, access_unit, parameter_sets)
    }

    fn bytes(&self) -> &[u8] {
        self.payload.bytes()
    }

    fn pts_ms(&self) -> Option<u64> {
        self.pts_ms
    }

    fn duration_ms(&self) -> Option<u64> {
        self.duration_ms
    }

    fn has_parameter_sets(&self) -> bool {
        self.stats.parameter_sets_present()
    }

    fn is_random_access(&self) -> bool {
        self.stats.idr_count > 0
    }
}

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, Copy)]
struct NativeVulkanAv1ActiveDpbReference {
    frame_type: u8,
    order_hint: u8,
    ref_frame_sign_bias: u8,
    saved_order_hints: [u8; 8],
    frame_width: u32,
    frame_height: u32,
    render_width: u32,
    render_height: u32,
    disable_frame_end_update_cdf: bool,
    segmentation_enabled: bool,
    segmentation: NativeVulkanAv1ParsedSegmentation,
    loop_filter_ref_deltas: [i8; 8],
    loop_filter_mode_deltas: [i8; 2],
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_av1_active_dpb_reference_from_decode_info(
    decode_info: &NativeVulkanAv1FirstFrameDecodeInfo,
    ref_frame_sign_bias: u8,
    reference_name_order_hints: [u8; 8],
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
) -> NativeVulkanAv1ActiveDpbReference {
    let order_hint = decode_info.header.order_hint.unwrap_or(0);
    NativeVulkanAv1ActiveDpbReference {
        frame_type: decode_info.header.frame_type,
        order_hint,
        ref_frame_sign_bias,
        // FFmpeg stores the current frame's ref-name order hints in the frame
        // state and later passes them to Vulkan as SavedOrderHints for refs.
        // See references/ffmpeg/libavcodec/av1dec.c:369-379 and
        // references/ffmpeg/libavcodec/vulkan_av1.c:318.
        saved_order_hints: native_vulkan_av1_setup_saved_order_hints(
            reference_name_order_hints,
            decode_info.header.refresh_frame_flags,
            order_hint,
        ),
        frame_width: decode_info
            .header
            .frame_width
            .unwrap_or(sequence_header.max_frame_width),
        frame_height: decode_info
            .header
            .frame_height
            .unwrap_or(sequence_header.max_frame_height),
        render_width: decode_info
            .header
            .render_width
            .unwrap_or(sequence_header.max_frame_width),
        render_height: decode_info
            .header
            .render_height
            .unwrap_or(sequence_header.max_frame_height),
        disable_frame_end_update_cdf: decode_info.header.disable_frame_end_update_cdf,
        segmentation_enabled: decode_info.header.segmentation.enabled,
        segmentation: decode_info.header.segmentation,
        loop_filter_ref_deltas: decode_info.header.loop_filter.ref_deltas,
        loop_filter_mode_deltas: decode_info.header.loop_filter.mode_deltas,
    }
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_av1_active_dpb_slots_after(
    entry: &NativeVulkanAv1DecodeReferencePlanEntrySnapshot,
) -> Vec<u32> {
    let mut active_slots_after = entry
        .map_slot_indices_after
        .iter()
        .filter_map(|slot| u32::try_from(*slot).ok())
        .collect::<Vec<_>>();
    active_slots_after.sort_unstable();
    active_slots_after.dedup();
    active_slots_after
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_av1_update_active_dpb_refs_after_decode(
    active_dpb_refs: &mut [Option<NativeVulkanAv1ActiveDpbReference>],
    entry: &NativeVulkanAv1DecodeReferencePlanEntrySnapshot,
    decode_info: &NativeVulkanAv1FirstFrameDecodeInfo,
    ref_frame_sign_bias: u8,
    reference_name_order_hints: [u8; 8],
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
) {
    let active_slots_after = native_vulkan_av1_active_dpb_slots_after(entry);
    let current_reference = entry.output_slot.and_then(|output_slot| {
        (!entry.refreshed_reference_names.is_empty()).then_some((
            output_slot,
            native_vulkan_av1_active_dpb_reference_from_decode_info(
                decode_info,
                ref_frame_sign_bias,
                reference_name_order_hints,
                sequence_header,
            ),
        ))
    });
    for (slot_index, slot) in active_dpb_refs.iter_mut().enumerate() {
        let slot_index = slot_index as u32;
        if !active_slots_after.contains(&slot_index) {
            *slot = None;
            continue;
        }
        if let Some((output_slot, reference)) = current_reference
            && output_slot == slot_index
        {
            *slot = Some(reference);
        }
    }
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_av1_update_active_dpb_refs_after_display_handoff(
    active_dpb_refs: &mut [Option<NativeVulkanAv1ActiveDpbReference>],
    entry: &NativeVulkanAv1DecodeReferencePlanEntrySnapshot,
) -> Result<(), String> {
    let displayed_slot = entry.displayed_slot.ok_or_else(|| {
        format!(
            "AV1 TU {} show_existing_frame has no displayed DPB slot",
            entry.temporal_unit_index
        )
    })?;
    let displayed_reference = active_dpb_refs
        .get(displayed_slot as usize)
        .and_then(|reference| *reference)
        .ok_or_else(|| {
            format!(
                "AV1 TU {} show_existing_frame references inactive DPB slot {}",
                entry.temporal_unit_index, displayed_slot
            )
        })?;
    let active_slots_after = native_vulkan_av1_active_dpb_slots_after(entry);
    for (slot_index, slot) in active_dpb_refs.iter_mut().enumerate() {
        let slot_index = slot_index as u32;
        if !active_slots_after.contains(&slot_index) {
            *slot = None;
            continue;
        }
        if slot_index == displayed_slot {
            // FFmpeg's show_existing_frame path replaces cur_frame from ref[idx]
            // and then updates the reference list. Key show-existing therefore
            // collapses all ref names onto the displayed frame state.
            // See references/ffmpeg/libavcodec/av1dec.c:1292-1300 and
            // references/ffmpeg/libavcodec/cbs_av1_syntax_template.c:1346-1402.
            *slot = Some(displayed_reference);
        }
    }
    Ok(())
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_av1_temporal_unit_decode_info(
    bytes: &[u8],
    obus: &[NativeVulkanAv1ObuSnapshot],
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
    reference_context: Option<&NativeVulkanAv1FrameHeaderReferenceContext>,
) -> Result<NativeVulkanAv1FirstFrameDecodeInfo, String> {
    if let Some(frame_obu) = obus.iter().find(|obu| obu.obu_type == 6) {
        let payload_offset = frame_obu.payload_offset as usize;
        let payload_end = payload_offset.saturating_add(frame_obu.payload_size as usize);
        let payload = bytes
            .get(payload_offset..payload_end)
            .ok_or_else(|| "AV1 frame OBU payload range exceeds bitstream".to_owned())?;
        let header = native_vulkan_parse_av1_frame_header_for_submit_with_context(
            payload,
            sequence_header,
            reference_context,
        )?;
        let tile_payload_offset = header.frame_header_bytes;
        let tile_payload = payload.get(tile_payload_offset..).unwrap_or_default();
        let (tile_offsets, tile_sizes) = native_vulkan_av1_tile_group_offsets_from_payload(
            frame_obu.payload_offset,
            tile_payload_offset,
            tile_payload,
            &header,
        )?;
        return native_vulkan_av1_validate_temporal_unit_decode_info(
            frame_obu.offset,
            frame_obu.payload_offset,
            header,
            tile_offsets,
            tile_sizes,
            !tile_payload.is_empty(),
        );
    }

    let frame_header_obu = obus
        .iter()
        .find(|obu| obu.obu_type == 3)
        .ok_or_else(|| "AV1 temporal unit decode found no frame or frame-header OBU".to_owned())?;
    let tile_group_obu = obus
        .iter()
        .find(|obu| obu.obu_type == 4)
        .ok_or_else(|| "AV1 temporal unit decode found no tile-group OBU".to_owned())?;
    let header_payload_offset = frame_header_obu.payload_offset as usize;
    let header_payload_end =
        header_payload_offset.saturating_add(frame_header_obu.payload_size as usize);
    let header_payload = bytes
        .get(header_payload_offset..header_payload_end)
        .ok_or_else(|| "AV1 frame-header OBU payload range exceeds bitstream".to_owned())?;
    let header = native_vulkan_parse_av1_frame_header_for_submit_with_context(
        header_payload,
        sequence_header,
        reference_context,
    )?;
    let tile_payload_offset = tile_group_obu.payload_offset as usize;
    let tile_payload_end = tile_payload_offset.saturating_add(tile_group_obu.payload_size as usize);
    let tile_payload = bytes
        .get(tile_payload_offset..tile_payload_end)
        .ok_or_else(|| "AV1 tile-group OBU payload range exceeds bitstream".to_owned())?;
    let (tile_offsets, tile_sizes) = native_vulkan_av1_tile_group_offsets_from_payload(
        tile_group_obu.payload_offset,
        0,
        tile_payload,
        &header,
    )?;
    native_vulkan_av1_validate_temporal_unit_decode_info(
        frame_header_obu.offset,
        frame_header_obu.payload_offset,
        header,
        tile_offsets,
        tile_sizes,
        !tile_payload.is_empty(),
    )
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_av1_validate_temporal_unit_decode_info(
    frame_header_obu_offset: u64,
    frame_header_payload_offset: u64,
    header: NativeVulkanAv1ParsedFrameHeader,
    tile_offsets: Vec<u32>,
    tile_sizes: Vec<u32>,
    found_tile_payload: bool,
) -> Result<NativeVulkanAv1FirstFrameDecodeInfo, String> {
    if header.show_existing_frame {
        return Err(
            "AV1 show_existing_frame is a display handoff and has no decode payload".to_owned(),
        );
    }
    if let Some(reason) = header.unsupported_reason.as_ref() {
        return Err(reason.clone());
    }
    if !found_tile_payload {
        return Err("AV1 temporal unit decode has no tile payload bytes".to_owned());
    }
    if header.tile_count != tile_offsets.len() as u32 || tile_offsets.len() != tile_sizes.len() {
        return Err(format!(
            "AV1 temporal unit decode tile table mismatch: header tile_count={}, offsets={}, sizes={}",
            header.tile_count,
            tile_offsets.len(),
            tile_sizes.len()
        ));
    }
    Ok(NativeVulkanAv1FirstFrameDecodeInfo {
        frame_header_obu_offset,
        frame_header_payload_offset,
        header,
        tile_offsets,
        tile_sizes,
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_primary_ref_none(primary_ref_frame: Option<u8>) -> bool {
    primary_ref_frame.is_none_or(|primary_ref_frame| primary_ref_frame == 7)
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_av1_final_force_integer_mv(frame_type: u8, force_integer_mv: u8) -> bool {
    matches!(frame_type, 0 | 2) || force_integer_mv == 1
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_skip_mode_parse_disabled() -> bool {
    matches!(
        std::env::var("GILDER_VULKAN_AV1_SKIP_MODE").ok().as_deref(),
        Some("off") | Some("false") | Some("0") | Some("disabled")
    )
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_av1_submit_warped_motion_disabled() -> bool {
    matches!(
        std::env::var("GILDER_VULKAN_AV1_SUBMIT_WARPED_MOTION")
            .ok()
            .as_deref(),
        Some("off") | Some("false") | Some("0") | Some("disabled")
    )
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_av1_submit_ref_frame_mvs_disabled() -> bool {
    matches!(
        std::env::var("GILDER_VULKAN_AV1_SUBMIT_REF_FRAME_MVS")
            .ok()
            .as_deref(),
        Some("off") | Some("false") | Some("0") | Some("disabled")
    )
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_av1_frame_header_offset_for_vulkan(
    frame: &NativeVulkanAv1FirstFrameDecodeInfo,
) -> Result<u32, NativeVulkanError> {
    let offset = match std::env::var("GILDER_VULKAN_AV1_FRAME_HEADER_OFFSET")
        .ok()
        .as_deref()
    {
        Some("payload") | Some("payload-header") => frame.frame_header_payload_offset,
        _ => frame.frame_header_obu_offset,
    };
    u32::try_from(offset).map_err(|_| {
        NativeVulkanError::Video(format!(
            "AV1 frame header offset {offset} exceeds u32 range"
        ))
    })
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_av1_bitstream_offsets_use_buffer_base() -> bool {
    matches!(
        std::env::var("GILDER_VULKAN_AV1_OFFSET_BASE")
            .ok()
            .as_deref(),
        Some("buffer") | Some("bitstream-buffer") | Some("absolute")
    )
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_av1_offset_for_vulkan(
    offset: u32,
    src_buffer_offset: u64,
) -> Result<u32, NativeVulkanError> {
    if !native_vulkan_av1_bitstream_offsets_use_buffer_base() {
        return Ok(offset);
    }
    let absolute = src_buffer_offset
        .checked_add(u64::from(offset))
        .ok_or_else(|| NativeVulkanError::Video("AV1 bitstream offset overflow".to_owned()))?;
    u32::try_from(absolute).map_err(|_| {
        NativeVulkanError::Video(format!("AV1 bitstream offset {absolute} exceeds u32 range"))
    })
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_av1_offsets_for_vulkan(
    offsets: &[u32],
    src_buffer_offset: u64,
) -> Result<Vec<u32>, NativeVulkanError> {
    let tile_offset_adjust = std::env::var("GILDER_VULKAN_AV1_TILE_OFFSET_ADJUST")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0);
    offsets
        .iter()
        .copied()
        .map(|offset| {
            let offset = if tile_offset_adjust >= 0 {
                u64::from(offset).checked_add(tile_offset_adjust as u64)
            } else {
                u64::from(offset).checked_sub(tile_offset_adjust.unsigned_abs())
            }
            .ok_or_else(|| {
                NativeVulkanError::Video(format!(
                    "AV1 tile offset adjustment {tile_offset_adjust} overflows offset {offset}"
                ))
            })?;
            let offset = u32::try_from(offset).map_err(|_| {
                NativeVulkanError::Video(format!(
                    "AV1 adjusted tile offset {offset} exceeds u32 range"
                ))
            })?;
            native_vulkan_av1_offset_for_vulkan(offset, src_buffer_offset)
        })
        .collect()
}

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeVulkanAv1BeginReferenceSlotStrategy {
    FullDpbGeneric,
    DecodeRefsAndSetup,
    DecodeRefsAndCurrentInactive,
    ActiveRefs,
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanAv1BeginReferenceSlotStrategy {
    fn from_env() -> Self {
        match std::env::var("GILDER_VULKAN_AV1_BEGIN_REFERENCE_SLOTS")
            .ok()
            .as_deref()
        {
            Some("decode-refs-setup") | Some("decode") | Some("sample") => Self::DecodeRefsAndSetup,
            Some("decode-refs-current-inactive") | Some("ffmpeg") | Some("current-inactive") => {
                Self::DecodeRefsAndCurrentInactive
            }
            Some("active") | Some("active-only") | Some("active-refs") => Self::ActiveRefs,
            Some("full-dpb") | Some("full-dpb-generic") => Self::FullDpbGeneric,
            _ => Self::DecodeRefsAndCurrentInactive,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::FullDpbGeneric => "full-dpb-generic",
            Self::DecodeRefsAndSetup => "decode-refs-and-setup",
            Self::DecodeRefsAndCurrentInactive => "decode-refs-current-inactive",
            Self::ActiveRefs => "active-refs",
        }
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_relative_dist_from_order_hint_bits(
    enable_order_hint: bool,
    order_hint_bits_minus_1: Option<u8>,
    a: u8,
    b: u8,
) -> i32 {
    if !enable_order_hint {
        return 0;
    }
    let bits = (u32::from(order_hint_bits_minus_1.unwrap_or(0)) + 1).clamp(1, 8);
    let mask = (1i32 << bits) - 1;
    let a = i32::from(a) & mask;
    let b = i32::from(b) & mask;
    let diff = a - b;
    let midpoint = 1i32 << (bits - 1);
    (diff & (midpoint - 1)) - (diff & midpoint)
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_relative_dist(
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
    a: u8,
    b: u8,
) -> i32 {
    native_vulkan_av1_relative_dist_from_order_hint_bits(
        sequence_header.enable_order_hint,
        sequence_header.order_hint_bits_minus_1,
        a,
        b,
    )
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_ref_frame_sign_bias_from_order_hints(
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
    current_order_hint: u8,
    order_hints: [u8; 8],
) -> u8 {
    if !sequence_header.enable_order_hint {
        return 0;
    }
    let mut packed = 0u8;
    for ref_name in 1..8 {
        let relative = native_vulkan_av1_relative_dist(
            sequence_header,
            current_order_hint,
            order_hints[ref_name],
        );
        if relative < 0 {
            packed |= 1u8 << ref_name;
        }
    }
    packed
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_current_ref_frame_sign_bias(
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
    frame_type: u8,
    current_order_hint: u8,
    order_hints: [u8; 8],
) -> u8 {
    if matches!(frame_type, 0 | 2) {
        return 0;
    }
    native_vulkan_av1_ref_frame_sign_bias_from_order_hints(
        sequence_header,
        current_order_hint,
        order_hints,
    )
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_dpb_reference_sign_bias(
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
    frame_type: u8,
    current_order_hint: u8,
    order_hints: [u8; 8],
) -> u8 {
    match std::env::var("GILDER_VULKAN_AV1_REFERENCE_SIGN_BIAS")
        .ok()
        .as_deref()
    {
        Some("zero") => 0,
        Some("all") | Some("all-frames") => native_vulkan_av1_ref_frame_sign_bias_from_order_hints(
            sequence_header,
            current_order_hint,
            order_hints,
        ),
        _ => native_vulkan_av1_current_ref_frame_sign_bias(
            sequence_header,
            frame_type,
            current_order_hint,
            order_hints,
        ),
    }
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_av1_setup_saved_order_hints(
    order_hints: [u8; 8],
    _refresh_frame_flags: u8,
    _current_order_hint: u8,
) -> [u8; 8] {
    order_hints
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_av1_current_setup_saved_order_hints(
    _order_hints: [u8; 8],
    _refresh_frame_flags: u8,
    _current_order_hint: u8,
) -> [u8; 8] {
    [0; 8]
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_av1_expected_frame_ids_array(expected_frame_ids: &[u32]) -> [u32; 8] {
    let mut values = [0u32; 8];
    for (index, value) in expected_frame_ids.iter().take(8).copied().enumerate() {
        values[index] = value;
    }
    values
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_av1_order_hint_offset_enabled(_vendor_id: u32) -> bool {
    match std::env::var("GILDER_VULKAN_AV1_ORDER_HINT_OFFSET")
        .ok()
        .as_deref()
    {
        Some("off") | Some("false") | Some("0") | Some("none") | Some("standard") => false,
        Some("on") | Some("true") | Some("1") | Some("ffmpeg") | Some("nvidia")
        | Some("shift-left") => true,
        _ => false,
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_std_order_hints(
    order_hints: [u8; 8],
    order_hint_offset_enabled: bool,
) -> [u8; 8] {
    if !order_hint_offset_enabled {
        return order_hints;
    }
    let mut shifted = [0u8; 8];
    shifted[..7].copy_from_slice(&order_hints[1..8]);
    shifted
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_order_hints_array(hints: &[Option<u8>]) -> [u8; 8] {
    let mut values = [0u8; 8];
    for (index, hint) in hints.iter().take(8).enumerate() {
        values[index] = hint.unwrap_or(0);
    }
    values
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_picture_order_hints_for_submit(
    reference_name_order_hints: [u8; 8],
    order_hint_offset_enabled: bool,
) -> [u8; 8] {
    native_vulkan_av1_std_order_hints(reference_name_order_hints, order_hint_offset_enabled)
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeVulkanAv1ReferenceHistory {
    frame_width: u32,
    frame_height: u32,
    render_width: u32,
    render_height: u32,
    segmentation: NativeVulkanAv1ParsedSegmentation,
    loop_filter_ref_deltas: [i8; 8],
    loop_filter_mode_deltas: [i8; 2],
}

#[cfg(feature = "native-vulkan-video")]
impl From<NativeVulkanAv1ActiveDpbReference> for NativeVulkanAv1ReferenceHistory {
    fn from(reference: NativeVulkanAv1ActiveDpbReference) -> Self {
        Self {
            frame_width: reference.frame_width,
            frame_height: reference.frame_height,
            render_width: reference.render_width,
            render_height: reference.render_height,
            segmentation: reference.segmentation,
            loop_filter_ref_deltas: reference.loop_filter_ref_deltas,
            loop_filter_mode_deltas: reference.loop_filter_mode_deltas,
        }
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeVulkanAv1FrameHeaderReferenceContext {
    reference_name_order_hints: [u8; 8],
    reference_name_slot_indices: [i32; vk::MAX_VIDEO_AV1_REFERENCES_PER_FRAME_KHR],
    reference_histories:
        [Option<NativeVulkanAv1ReferenceHistory>; vk::MAX_VIDEO_AV1_REFERENCES_PER_FRAME_KHR],
}

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, Copy)]
struct NativeVulkanAv1PreparedReferenceContext {
    reference_name_order_hints: [u8; 8],
    reference_name_dpb_slot_indices: [i32; vk::MAX_VIDEO_AV1_REFERENCES_PER_FRAME_KHR],
    reference_context: NativeVulkanAv1FrameHeaderReferenceContext,
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_av1_prepared_reference_context(
    entry: &NativeVulkanAv1DecodeReferencePlanEntrySnapshot,
    active_dpb_refs: &[Option<NativeVulkanAv1ActiveDpbReference>],
) -> NativeVulkanAv1PreparedReferenceContext {
    let reference_name_dpb_slot_indices = native_vulkan_av1_reference_name_slot_indices(entry);
    let reference_name_order_hints =
        native_vulkan_av1_order_hints_array(&entry.reference_name_order_hints);
    let mut reference_histories =
        [None::<NativeVulkanAv1ReferenceHistory>; vk::MAX_VIDEO_AV1_REFERENCES_PER_FRAME_KHR];
    for (reference_index, slot_index) in reference_name_dpb_slot_indices.iter().copied().enumerate()
    {
        let Ok(slot_index) = usize::try_from(slot_index) else {
            continue;
        };
        reference_histories[reference_index] = active_dpb_refs
            .get(slot_index)
            .and_then(|reference| reference.map(NativeVulkanAv1ReferenceHistory::from));
    }
    NativeVulkanAv1PreparedReferenceContext {
        reference_name_order_hints,
        reference_name_dpb_slot_indices,
        reference_context: NativeVulkanAv1FrameHeaderReferenceContext {
            reference_name_order_hints,
            reference_name_slot_indices: reference_name_dpb_slot_indices,
            reference_histories,
        },
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
impl NativeVulkanAv1FrameHeaderReferenceContext {
    fn primary_reference_history(
        &self,
        primary_ref_frame: Option<u8>,
    ) -> Option<NativeVulkanAv1ReferenceHistory> {
        if native_vulkan_av1_primary_ref_none(primary_ref_frame) {
            return None;
        }
        let index = usize::from(primary_ref_frame?);
        self.reference_histories.get(index).copied().flatten()
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_skip_mode_frame_from_order_hints(
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
    frame_type: u8,
    error_resilient_mode: bool,
    reference_select: bool,
    current_order_hint: u8,
    reference_name_order_hints: [u8; 8],
    reference_name_slot_indices: [i32; vk::MAX_VIDEO_AV1_REFERENCES_PER_FRAME_KHR],
) -> Option<[u8; 2]> {
    if !sequence_header.enable_order_hint
        || error_resilient_mode
        || frame_type != 1
        || !reference_select
    {
        return None;
    }

    let mut ref0 = None::<u8>;
    let mut ref1 = None::<u8>;
    let mut ref0_hint = None::<u8>;
    let mut ref1_hint = None::<u8>;

    for ref_name_minus_one in 0..vk::MAX_VIDEO_AV1_REFERENCES_PER_FRAME_KHR {
        if reference_name_slot_indices[ref_name_minus_one] < 0 {
            continue;
        }
        let ref_name = (ref_name_minus_one + 1) as u8;
        let ref_order_hint = reference_name_order_hints[ref_name as usize];
        let relative =
            native_vulkan_av1_relative_dist(sequence_header, ref_order_hint, current_order_hint);
        if relative < 0
            && ref0_hint.is_none_or(|hint| {
                native_vulkan_av1_relative_dist(sequence_header, ref_order_hint, hint) > 0
            })
        {
            ref0 = Some(ref_name);
            ref0_hint = Some(ref_order_hint);
        }
        if relative > 0
            && ref1_hint.is_none_or(|hint| {
                native_vulkan_av1_relative_dist(sequence_header, ref_order_hint, hint) < 0
            })
        {
            ref1 = Some(ref_name);
            ref1_hint = Some(ref_order_hint);
        }
    }

    match (ref0, ref1) {
        (Some(left), Some(right)) => Some([left.min(right), left.max(right)]),
        (Some(left), None) => {
            let first_forward_hint = ref0_hint?;
            let mut second = None::<u8>;
            let mut second_hint = None::<u8>;
            for ref_name_minus_one in 0..vk::MAX_VIDEO_AV1_REFERENCES_PER_FRAME_KHR {
                if reference_name_slot_indices[ref_name_minus_one] < 0 {
                    continue;
                }
                let ref_name = (ref_name_minus_one + 1) as u8;
                let ref_order_hint = reference_name_order_hints[ref_name as usize];
                if native_vulkan_av1_relative_dist(
                    sequence_header,
                    ref_order_hint,
                    first_forward_hint,
                ) < 0
                    && second_hint.is_none_or(|hint| {
                        native_vulkan_av1_relative_dist(sequence_header, ref_order_hint, hint) > 0
                    })
                {
                    second = Some(ref_name);
                    second_hint = Some(ref_order_hint);
                }
            }
            let right = second?;
            Some([left.min(right), left.max(right)])
        }
        _ => None,
    }
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_av1_reference_name_slot_indices(
    entry: &NativeVulkanAv1DecodeReferencePlanEntrySnapshot,
) -> [i32; vk::MAX_VIDEO_AV1_REFERENCES_PER_FRAME_KHR] {
    let mut slots = [-1i32; vk::MAX_VIDEO_AV1_REFERENCES_PER_FRAME_KHR];
    for (index, slot) in entry
        .decode_reference_slots
        .iter()
        .take(vk::MAX_VIDEO_AV1_REFERENCES_PER_FRAME_KHR)
        .enumerate()
    {
        slots[index] = *slot;
    }
    slots
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_av1_reference_name_decode_slot_indices(
    reference_name_dpb_slot_indices: [i32; vk::MAX_VIDEO_AV1_REFERENCES_PER_FRAME_KHR],
    unique_reference_slots: &[u32],
) -> [i32; vk::MAX_VIDEO_AV1_REFERENCES_PER_FRAME_KHR] {
    let mut slots = [-1i32; vk::MAX_VIDEO_AV1_REFERENCES_PER_FRAME_KHR];
    for (index, dpb_slot) in reference_name_dpb_slot_indices.iter().copied().enumerate() {
        let Ok(dpb_slot) = u32::try_from(dpb_slot) else {
            continue;
        };
        if let Some(reference_slot_index) = unique_reference_slots
            .iter()
            .position(|slot| *slot == dpb_slot)
        {
            slots[index] = reference_slot_index as i32;
        }
    }
    slots
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_av1_reference_info_from_active(
    reference: NativeVulkanAv1ActiveDpbReference,
    order_hint_offset_enabled: bool,
) -> vk::video::StdVideoDecodeAV1ReferenceInfo {
    vk::video::StdVideoDecodeAV1ReferenceInfo {
        flags: vk::video::StdVideoDecodeAV1ReferenceInfoFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::video::StdVideoDecodeAV1ReferenceInfoFlags::new_bitfield_1(
                native_vulkan_bool_u32(reference.disable_frame_end_update_cdf),
                native_vulkan_bool_u32(reference.segmentation_enabled),
                0,
            ),
        },
        frame_type: reference.frame_type,
        RefFrameSignBias: reference.ref_frame_sign_bias,
        OrderHint: reference.order_hint,
        SavedOrderHints: native_vulkan_av1_std_order_hints(
            reference.saved_order_hints,
            order_hint_offset_enabled,
        ),
    }
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_av1_reference_info_from_decode_info(
    decode_info: &NativeVulkanAv1FirstFrameDecodeInfo,
    ref_frame_sign_bias: u8,
    saved_order_hints: [u8; 8],
    order_hint_offset_enabled: bool,
) -> vk::video::StdVideoDecodeAV1ReferenceInfo {
    vk::video::StdVideoDecodeAV1ReferenceInfo {
        flags: vk::video::StdVideoDecodeAV1ReferenceInfoFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::video::StdVideoDecodeAV1ReferenceInfoFlags::new_bitfield_1(
                native_vulkan_bool_u32(decode_info.header.disable_frame_end_update_cdf),
                native_vulkan_bool_u32(decode_info.header.segmentation.enabled),
                0,
            ),
        },
        frame_type: decode_info.header.frame_type,
        RefFrameSignBias: ref_frame_sign_bias,
        OrderHint: decode_info.header.order_hint.unwrap_or(0),
        SavedOrderHints: native_vulkan_av1_std_order_hints(
            saved_order_hints,
            order_hint_offset_enabled,
        ),
    }
}

#[cfg(feature = "native-vulkan-video")]
struct NativeVulkanAv1TemporalUnitExtract {
    payload: NativeVulkanEncodedAccessUnitPayload,
    pts_ns: Option<u64>,
    duration_ns: Option<u64>,
    pts_ms: Option<u64>,
    duration_ms: Option<u64>,
    stats: NativeVulkanAv1ObuStats,
}

#[cfg(feature = "native-vulkan-video")]
type NativeVulkanH264StreamingPacketQueue =
    NativeVulkanStreamingPacketQueue<NativeVulkanH264AccessUnitExtract>;

#[cfg(feature = "native-vulkan-video")]
type NativeVulkanH265StreamingPacketQueue =
    NativeVulkanStreamingPacketQueue<NativeVulkanH265AccessUnitExtract>;

#[cfg(feature = "native-vulkan-video")]
#[allow(dead_code)]
type NativeVulkanAv1StreamingPacketQueue =
    NativeVulkanStreamingPacketQueue<NativeVulkanAv1TemporalUnitExtract>;

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanFfmpegStreamingAccessUnit for NativeVulkanH264AccessUnitExtract {
    const FFMPEG_CODEC: NativeVulkanFfmpegCodec = NativeVulkanFfmpegCodec::H264;

    fn from_ffmpeg_packet(
        payload: NativeVulkanFfmpegPacketPayload,
        metadata: NativeVulkanFfmpegPacketMetadata,
    ) -> Result<Self, NativeVulkanError> {
        let payload = NativeVulkanEncodedAccessUnitPayload::from_ffmpeg_packet(payload);
        if payload.is_empty() {
            return Err(NativeVulkanError::Video(
                "H.264 FFmpeg packet is empty".to_owned(),
            ));
        }
        let stats = native_vulkan_h264_nal_stats(payload.bytes());
        Ok(Self {
            payload,
            pts_ns: metadata.pts_ns,
            duration_ns: metadata.duration_ns,
            pts_ms: metadata.pts_ms,
            duration_ms: metadata.duration_ms,
            stats,
        })
    }
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanStreamingAccessUnit for NativeVulkanH265AccessUnitExtract {
    type ParameterSets = NativeVulkanH265ParameterSetSnapshot;
    type Snapshot = NativeVulkanH265AccessUnitSnapshot;

    const CODEC_LABEL: &'static str = "H.265";
    const PARAMETER_SETS_LABEL: &'static str = "VPS/SPS/PPS";

    fn parse_parameter_sets(bytes: &[u8]) -> Result<Self::ParameterSets, String> {
        native_vulkan_parse_h265_parameter_sets(bytes)
    }

    fn snapshot(
        index: u32,
        access_unit: &Self,
        parameter_sets: &Self::ParameterSets,
    ) -> Self::Snapshot {
        native_vulkan_h265_access_unit_snapshot(index, access_unit, parameter_sets)
    }

    fn bytes(&self) -> &[u8] {
        self.payload.bytes()
    }

    fn pts_ms(&self) -> Option<u64> {
        self.pts_ms
    }

    fn duration_ms(&self) -> Option<u64> {
        self.duration_ms
    }

    fn has_parameter_sets(&self) -> bool {
        self.stats.parameter_sets_present()
    }

    fn is_random_access(&self) -> bool {
        self.stats.idr_count > 0
    }
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanFfmpegStreamingAccessUnit for NativeVulkanH265AccessUnitExtract {
    const FFMPEG_CODEC: NativeVulkanFfmpegCodec = NativeVulkanFfmpegCodec::H265;

    fn from_ffmpeg_packet(
        payload: NativeVulkanFfmpegPacketPayload,
        metadata: NativeVulkanFfmpegPacketMetadata,
    ) -> Result<Self, NativeVulkanError> {
        let payload = NativeVulkanEncodedAccessUnitPayload::from_ffmpeg_packet(payload);
        if payload.is_empty() {
            return Err(NativeVulkanError::Video(
                "H.265 FFmpeg packet is empty".to_owned(),
            ));
        }
        let stats = native_vulkan_h265_nal_stats(payload.bytes());
        Ok(Self {
            payload,
            pts_ns: metadata.pts_ns,
            duration_ns: metadata.duration_ns,
            pts_ms: metadata.pts_ms,
            duration_ms: metadata.duration_ms,
            stats,
        })
    }
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanStreamingAccessUnit for NativeVulkanAv1TemporalUnitExtract {
    type ParameterSets = NativeVulkanAv1SequenceHeaderSnapshot;
    type Snapshot = NativeVulkanAv1TemporalUnitSnapshot;

    const CODEC_LABEL: &'static str = "AV1";
    const PARAMETER_SETS_LABEL: &'static str = "sequence header";

    fn parse_parameter_sets(bytes: &[u8]) -> Result<Self::ParameterSets, String> {
        native_vulkan_av1_obu_stats(bytes)?
            .sequence_header
            .ok_or_else(|| "AV1 temporal unit has no sequence header".to_owned())
    }

    fn snapshot(
        index: u32,
        access_unit: &Self,
        parameter_sets: &Self::ParameterSets,
    ) -> Self::Snapshot {
        native_vulkan_av1_temporal_unit_snapshot(index, access_unit, Some(parameter_sets))
    }

    fn bytes(&self) -> &[u8] {
        self.payload.bytes()
    }

    fn pts_ms(&self) -> Option<u64> {
        self.pts_ms
    }

    fn duration_ms(&self) -> Option<u64> {
        self.duration_ms
    }

    fn has_parameter_sets(&self) -> bool {
        self.stats.sequence_header_present()
    }

    fn is_random_access(&self) -> bool {
        self.stats
            .first_frame_submit
            .as_ref()
            .is_some_and(|submit| {
                submit.frame_type == 0 && submit.show_frame && submit.vulkan_submit_candidate
            })
    }

    fn is_random_access_with_parameter_sets(&self, parameter_sets: &Self::ParameterSets) -> bool {
        self.stats
            .first_frame_submit
            .clone()
            .or_else(|| {
                native_vulkan_av1_first_frame_submit_snapshot(
                    self.payload.bytes(),
                    &self.stats.obus,
                    parameter_sets,
                )
            })
            .is_some_and(|submit| {
                submit.frame_type == 0 && submit.show_frame && submit.vulkan_submit_candidate
            })
    }
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanFfmpegStreamingAccessUnit for NativeVulkanAv1TemporalUnitExtract {
    const FFMPEG_CODEC: NativeVulkanFfmpegCodec = NativeVulkanFfmpegCodec::Av1;
    const FFMPEG_PACKET_SPLITS_ACCESS_UNITS: bool = true;

    fn from_ffmpeg_packet(
        payload: NativeVulkanFfmpegPacketPayload,
        metadata: NativeVulkanFfmpegPacketMetadata,
    ) -> Result<Self, NativeVulkanError> {
        let payload = NativeVulkanEncodedAccessUnitPayload::from_ffmpeg_packet(payload);
        if payload.is_empty() {
            return Err(NativeVulkanError::Video(
                "AV1 FFmpeg packet is empty".to_owned(),
            ));
        }
        let stats =
            native_vulkan_av1_obu_stats(payload.bytes()).map_err(NativeVulkanError::Video)?;
        Ok(Self {
            payload,
            pts_ns: metadata.pts_ns,
            duration_ns: metadata.duration_ns,
            pts_ms: metadata.pts_ms,
            duration_ms: metadata.duration_ms,
            stats,
        })
    }

    fn from_ffmpeg_packet_many(
        payload: NativeVulkanFfmpegPacketPayload,
        metadata: NativeVulkanFfmpegPacketMetadata,
    ) -> Result<Vec<Self>, NativeVulkanError> {
        let ranges = native_vulkan_av1_split_ffmpeg_packet_frame_ranges(payload.bytes())
            .map_err(NativeVulkanError::Video)?;
        payload
            .split_into_ranges(ranges, "AV1")?
            .into_iter()
            .map(|unit| {
                let payload = NativeVulkanEncodedAccessUnitPayload::from_ffmpeg_packet(unit);
                if payload.is_empty() {
                    return Err(NativeVulkanError::Video(
                        "AV1 FFmpeg packet frame unit is empty".to_owned(),
                    ));
                }
                let stats = native_vulkan_av1_obu_stats(payload.bytes())
                    .map_err(NativeVulkanError::Video)?;
                Ok(Self {
                    payload,
                    pts_ns: metadata.pts_ns,
                    duration_ns: metadata.duration_ns,
                    pts_ms: metadata.pts_ms,
                    duration_ms: metadata.duration_ms,
                    stats,
                })
            })
            .collect()
    }
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_av1_temporal_unit_snapshot(
    index: u32,
    temporal_unit: &NativeVulkanAv1TemporalUnitExtract,
    active_sequence_header: Option<&NativeVulkanAv1SequenceHeaderSnapshot>,
) -> NativeVulkanAv1TemporalUnitSnapshot {
    let first_frame_submit = temporal_unit.stats.first_frame_submit.clone().or_else(|| {
        let sequence_header = temporal_unit
            .stats
            .sequence_header
            .as_ref()
            .or(active_sequence_header)?;
        native_vulkan_av1_first_frame_submit_snapshot(
            temporal_unit.payload.bytes(),
            &temporal_unit.stats.obus,
            sequence_header,
        )
    });

    NativeVulkanAv1TemporalUnitSnapshot {
        index,
        bytes: temporal_unit.stats.bytes,
        byte_hash: 0,
        pts_ns: temporal_unit.pts_ns,
        duration_ns: temporal_unit.duration_ns,
        pts_ms: temporal_unit.pts_ms,
        duration_ms: temporal_unit.duration_ms,
        obu_count: temporal_unit.stats.obu_count,
        sequence_header_count: temporal_unit.stats.sequence_header_count,
        temporal_delimiter_count: temporal_unit.stats.temporal_delimiter_count,
        frame_header_count: temporal_unit.stats.frame_header_count,
        tile_group_count: temporal_unit.stats.tile_group_count,
        frame_count: temporal_unit.stats.frame_count,
        decode_candidate: temporal_unit.stats.decode_candidate(),
        tile_payload_bytes: temporal_unit.stats.tile_payload_bytes,
        frame_payload_bytes: temporal_unit.stats.frame_payload_bytes,
        first_frame_header_obu_offset: temporal_unit.stats.first_frame_header_obu_offset,
        first_tile_group_obu_offset: temporal_unit.stats.first_tile_group_obu_offset,
        sequence_header_present: temporal_unit.stats.sequence_header_present(),
        sequence_header: temporal_unit.stats.sequence_header.clone(),
        first_frame_submit,
        obus: temporal_unit.stats.obus.clone(),
    }
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_h264_access_unit_snapshot(
    index: u32,
    access_unit: &NativeVulkanH264AccessUnitExtract,
    parameter_sets: &NativeVulkanH264ParameterSetSnapshot,
) -> NativeVulkanH264AccessUnitSnapshot {
    let first_frame = native_vulkan_h264_picture_decode_info_from_stats(
        access_unit.payload.bytes(),
        &access_unit.stats,
        parameter_sets,
    );
    let (first_slice, first_slice_parse_error) = match first_frame {
        Ok(first_frame) => (
            Some(NativeVulkanH264AccessUnitSliceSnapshot {
                nal_type: first_frame.nal_type,
                nal_type_label: first_frame.nal_type_label,
                nal_ref_idc: first_frame.nal_ref_idc,
                first_mb_in_slice: first_frame.first_mb_in_slice,
                first_slice_segment_in_pic_flag: first_frame.first_slice_segment_in_pic_flag,
                slice_type: first_frame.slice_type,
                slice_type_normalized: first_frame.slice_type_normalized,
                pps_id: first_frame.pps_id,
                frame_num: first_frame.frame_num,
                idr_pic_id: first_frame.idr_pic_id,
                num_ref_idx_l0_active_minus1: first_frame.num_ref_idx_l0_active_minus1,
                num_ref_idx_l1_active_minus1: first_frame.num_ref_idx_l1_active_minus1,
                ref_pic_list_modification_l0: first_frame.ref_pic_list_modification_l0,
                ref_pic_list_modifications_l0: first_frame.ref_pic_list_modifications_l0,
                ref_pic_list_modification_l1: first_frame.ref_pic_list_modification_l1,
                ref_pic_list_modifications_l1: first_frame.ref_pic_list_modifications_l1,
                adaptive_ref_pic_marking_mode_flag: first_frame.adaptive_ref_pic_marking_mode_flag,
                memory_management_control_operations: first_frame
                    .memory_management_control_operations,
                field_pic_flag: first_frame.field_pic_flag,
                bottom_field_flag: first_frame.bottom_field_flag,
                is_reference: first_frame.is_reference,
                is_intra: first_frame.is_intra,
                is_p: first_frame.is_p,
                is_b: first_frame.is_b,
                long_term_reference_flag: first_frame.long_term_reference_flag,
                pic_order_cnt: first_frame.pic_order_cnt,
                slice_offsets: first_frame.slice_offsets,
                idr: first_frame.idr,
                irap: first_frame.irap,
            }),
            None,
        ),
        Err(err) => (None, Some(err)),
    };
    let idr_decode_ready = first_slice.as_ref().is_some_and(|slice| {
        slice.idr
            && slice.irap
            && slice.is_intra
            && !slice.field_pic_flag
            && !slice.slice_offsets.is_empty()
    });
    let decode_ready = first_slice.as_ref().is_some_and(|slice| {
        let active_l0_refs = slice
            .num_ref_idx_l0_active_minus1
            .map(|value| value.saturating_add(1))
            .unwrap_or(0);
        !slice.field_pic_flag
            && slice.is_reference
            && !slice.slice_offsets.is_empty()
            && !slice.is_b
            && !slice.long_term_reference_flag
            && native_vulkan_h264_ref_pic_list_modifications_supported(slice)
            && !slice.adaptive_ref_pic_marking_mode_flag
            && (slice.is_intra || (slice.is_p && active_l0_refs > 0))
    });

    NativeVulkanH264AccessUnitSnapshot {
        index,
        bytes: access_unit.stats.bytes,
        byte_hash: 0,
        pts_ns: access_unit.pts_ns,
        duration_ns: access_unit.duration_ns,
        pts_ms: access_unit.pts_ms,
        duration_ms: access_unit.duration_ms,
        has_annex_b_start_codes: access_unit.stats.has_annex_b_start_codes,
        has_parameter_sets: access_unit.stats.parameter_sets_present(),
        h264_sps_count: access_unit.stats.sps_count,
        h264_pps_count: access_unit.stats.pps_count,
        h264_idr_count: access_unit.stats.idr_count,
        h264_slice_count: access_unit.stats.slice_count,
        first_slice,
        first_slice_parse_error,
        idr_decode_ready,
        decode_ready,
    }
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_h265_access_unit_snapshot(
    index: u32,
    access_unit: &NativeVulkanH265AccessUnitExtract,
    parameter_sets: &NativeVulkanH265ParameterSetSnapshot,
) -> NativeVulkanH265AccessUnitSnapshot {
    let first_slice_result = native_vulkan_h265_first_slice_probe_snapshot_from_stats(
        access_unit.payload.bytes(),
        &access_unit.stats,
        parameter_sets,
    );
    let (first_slice, first_slice_parse_error) = match first_slice_result {
        Ok(snapshot) => (Some(snapshot), None),
        Err(err) => (None, Some(err)),
    };
    NativeVulkanH265AccessUnitSnapshot {
        index,
        bytes: access_unit.stats.bytes,
        byte_hash: 0,
        pts_ns: access_unit.pts_ns,
        duration_ns: access_unit.duration_ns,
        pts_ms: access_unit.pts_ms,
        duration_ms: access_unit.duration_ms,
        has_annex_b_start_codes: access_unit.stats.has_annex_b_start_codes,
        has_parameter_sets: access_unit.stats.parameter_sets_present(),
        h265_vps_count: access_unit.stats.vps_count,
        h265_sps_count: access_unit.stats.sps_count,
        h265_pps_count: access_unit.stats.pps_count,
        h265_idr_count: access_unit.stats.idr_count,
        h265_slice_count: access_unit.stats.slice_count,
        first_slice,
        first_slice_parse_error,
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h265_sps_short_term_ref_pic_sets_supported(
    ref_pic_sets: &[NativeVulkanH265ShortTermRefPicSetSnapshot],
) -> bool {
    ref_pic_sets.iter().all(|ref_pic_set| {
        ref_pic_set.num_negative_pics <= 16
            && ref_pic_set.num_positive_pics <= 16
            && ref_pic_set.use_delta_flags.len() <= 16
            && ref_pic_set.used_by_current_flags.len() <= 16
            && ref_pic_set
                .abs_delta_rps_minus1
                .is_none_or(|value| value <= u16::MAX as u32)
            && ref_pic_set
                .negative_delta_pocs
                .iter()
                .chain(ref_pic_set.positive_delta_pocs.iter())
                .all(|delta_poc| delta_poc.unsigned_abs() <= u16::MAX as u32)
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h265_sps_long_term_ref_pics_supported(
    ref_pics: &[NativeVulkanH265LongTermRefPicSpsSnapshot],
) -> bool {
    ref_pics.len() <= 32
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_h264_sps_max_frame_num(sps: &NativeVulkanH264SpsSnapshot) -> u32 {
    1u32.checked_shl(sps.log2_max_frame_num_minus4.saturating_add(4))
        .unwrap_or(u32::MAX)
        .max(1)
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_h265_sps_max_pic_order_cnt_lsb(sps: &NativeVulkanH265SpsSnapshot) -> u32 {
    1u32.checked_shl(sps.log2_max_pic_order_cnt_lsb_minus4.saturating_add(4))
        .unwrap_or(u32::MAX)
        .max(1)
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h265_first_slice_probe_snapshot_from_stats(
    access_unit: &[u8],
    stats: &NativeVulkanH265NalStats,
    parameter_sets: &NativeVulkanH265ParameterSetSnapshot,
) -> Result<NativeVulkanH265AccessUnitSliceSnapshot, String> {
    let first_slice = stats
        .first_slice
        .ok_or_else(|| "H.265 access unit has no slice NAL".to_owned())?;
    if first_slice.payload_start >= first_slice.payload_end
        || first_slice.payload_end > access_unit.len()
    {
        return Err("H.265 first slice payload range exceeds access-unit bounds".to_owned());
    }
    native_vulkan_h265_slice_probe_snapshot_from_summary(
        first_slice,
        &access_unit[first_slice.payload_start..first_slice.payload_end],
        parameter_sets,
    )
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h265_slice_probe_snapshot_from_summary(
    slice: NativeVulkanH265SlicePayloadSummary,
    payload: &[u8],
    parameter_sets: &NativeVulkanH265ParameterSetSnapshot,
) -> Result<NativeVulkanH265AccessUnitSliceSnapshot, String> {
    let idr = matches!(slice.nal_type, 19 | 20);
    let irap = (16..=23).contains(&slice.nal_type);
    let rbsp = native_vulkan_h265_slice_header_rbsp(payload)?;
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
            "H.265 access unit first slice is not the first slice segment in picture".to_owned(),
        );
    }
    for _ in 0..parameter_sets.pps.num_extra_slice_header_bits {
        bits.skip_bits(1, "slice_reserved_flag")?;
    }
    let slice_type = bits.read_ue("slice_type")?;
    if parameter_sets.pps.output_flag_present_flag {
        bits.read_bool("pic_output_flag")?;
    }
    if parameter_sets.sps.separate_colour_plane_flag {
        bits.skip_bits(2, "colour_plane_id")?;
    }
    let mut short_term_ref_pic_set_sps_flag = false;
    let mut short_term_ref_pic_set_idx = None::<u32>;
    let mut num_delta_pocs_of_ref_rps_idx = 0u8;
    let mut num_bits_for_st_ref_pic_set_in_slice = 0u16;
    let (pic_order_cnt_lsb, short_term_reference_delta_pocs, long_term_references) = if idr {
        (None, NativeVulkanH265ReferenceDeltas::new(), Vec::new())
    } else {
        let pic_order_cnt_lsb = bits.read_bits(
            parameter_sets.sps.log2_max_pic_order_cnt_lsb_minus4 + 4,
            "slice_pic_order_cnt_lsb",
        )?;
        short_term_ref_pic_set_sps_flag = bits.read_bool("short_term_ref_pic_set_sps_flag")?;
        let mut short_term_reference_delta_pocs = NativeVulkanH265ReferenceDeltas::new();
        if short_term_ref_pic_set_sps_flag {
            if parameter_sets.sps.num_short_term_ref_pic_sets == 0 {
                return Err("H.265 slice references SPS short-term RPS but SPS has none".to_owned());
            }
            let selected_ref_pic_set_idx = if parameter_sets.sps.num_short_term_ref_pic_sets > 1 {
                let bits_for_idx =
                    32 - (parameter_sets.sps.num_short_term_ref_pic_sets - 1).leading_zeros();
                bits.read_bits(bits_for_idx, "short_term_ref_pic_set_idx")?
            } else {
                0
            };
            short_term_ref_pic_set_idx = Some(selected_ref_pic_set_idx);
            let short_term_ref_pic_set = parameter_sets
                .sps
                .short_term_ref_pic_sets
                .get(selected_ref_pic_set_idx as usize)
                .ok_or_else(|| {
                    format!(
                        "H.265 slice short_term_ref_pic_set_idx {selected_ref_pic_set_idx} exceeds SPS RPS count {}",
                        parameter_sets.sps.short_term_ref_pic_sets.len()
                    )
                })?;
            short_term_reference_delta_pocs.extend_used_ref_pic_set(short_term_ref_pic_set);
        } else {
            let rps_bit_start = bits.bit_offset();
            let short_term_ref_pic_set = native_vulkan_h265_read_short_term_ref_pic_set(
                &mut bits,
                parameter_sets.sps.num_short_term_ref_pic_sets,
                parameter_sets.sps.num_short_term_ref_pic_sets,
                &parameter_sets.sps.short_term_ref_pic_sets,
            )?;
            let rps_bit_count = bits
                .bit_offset()
                .checked_sub(rps_bit_start)
                .ok_or_else(|| "H.265 short-term RPS bit position underflow".to_owned())?;
            num_bits_for_st_ref_pic_set_in_slice = u16::try_from(rps_bit_count)
                .map_err(|_| "H.265 short-term RPS bit count exceeds u16 range".to_owned())?;
            if short_term_ref_pic_set.inter_ref_pic_set_prediction_flag {
                num_delta_pocs_of_ref_rps_idx = native_vulkan_h265_u8(
                    short_term_ref_pic_set.num_delta_pocs_of_ref_rps_idx,
                    "NumDeltaPocsOfRefRpsIdx",
                )?;
            }
            short_term_reference_delta_pocs.extend_used_ref_pic_set(&short_term_ref_pic_set);
        }
        let long_term_references =
            native_vulkan_h265_read_long_term_references(&mut bits, &parameter_sets.sps)?;
        (
            Some(pic_order_cnt_lsb),
            short_term_reference_delta_pocs,
            long_term_references,
        )
    };

    Ok(NativeVulkanH265AccessUnitSliceSnapshot {
        nal_type: slice.nal_type,
        nal_type_label: native_vulkan_h265_nal_type_label(slice.nal_type),
        slice_segment_offset: slice.slice_segment_offset,
        first_slice_segment_in_pic_flag,
        slice_type,
        pps_id,
        pic_order_cnt_lsb,
        short_term_ref_pic_set_sps_flag,
        short_term_ref_pic_set_idx,
        num_delta_pocs_of_ref_rps_idx,
        num_bits_for_st_ref_pic_set_in_slice,
        short_term_reference_delta_pocs,
        long_term_references,
        idr,
        irap,
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h265_read_long_term_references(
    bits: &mut NativeVulkanH265BitReader<'_>,
    sps: &NativeVulkanH265SpsSnapshot,
) -> Result<Vec<NativeVulkanH265LongTermReferenceSnapshot>, String> {
    if !sps.long_term_ref_pics_present_flag {
        return Ok(Vec::new());
    }

    let sps_ref_count = sps.long_term_ref_pics_sps.len() as u32;
    let num_long_term_sps = if sps_ref_count > 0 {
        bits.read_ue("num_long_term_sps")?
    } else {
        0
    };
    let num_long_term_pics = bits.read_ue("num_long_term_pics")?;
    if num_long_term_sps > sps_ref_count {
        return Err(format!(
            "H.265 slice requests {num_long_term_sps} SPS long-term refs but SPS has {sps_ref_count}"
        ));
    }
    let total = num_long_term_sps
        .checked_add(num_long_term_pics)
        .ok_or_else(|| "H.265 long-term reference count overflow".to_owned())?;
    if total > 32 {
        return Err(format!(
            "H.265 slice has {total} long-term refs; maximum supported is 32"
        ));
    }

    let mut references = Vec::with_capacity(total as usize);
    let lt_idx_sps_bits = native_vulkan_h265_ceil_log2(sps_ref_count);
    let poc_lsb_bits = sps
        .log2_max_pic_order_cnt_lsb_minus4
        .checked_add(4)
        .ok_or_else(|| "H.265 long-term POC LSB bit count overflow".to_owned())?;
    let mut previous_delta_poc_msb_cycle_lt = None::<u32>;
    for index in 0..total {
        let (from_sps, lt_idx_sps, poc_lsb, used_by_current) = if index < num_long_term_sps {
            let lt_idx_sps = if sps_ref_count > 1 {
                bits.read_bits(lt_idx_sps_bits, "lt_idx_sps")?
            } else {
                0
            };
            let entry = sps
                .long_term_ref_pics_sps
                .get(lt_idx_sps as usize)
                .ok_or_else(|| {
                    format!(
                        "H.265 slice lt_idx_sps {lt_idx_sps} exceeds SPS long-term ref count {sps_ref_count}"
                    )
                })?;
            (
                true,
                Some(lt_idx_sps),
                entry.lt_ref_pic_poc_lsb_sps,
                entry.used_by_curr_pic_lt_sps_flag,
            )
        } else {
            let poc_lsb = bits.read_bits(poc_lsb_bits, "poc_lsb_lt")?;
            let used_by_current = bits.read_bool("used_by_curr_pic_lt_flag")?;
            (false, None, poc_lsb, used_by_current)
        };
        let delta_poc_msb_present_flag = bits.read_bool("delta_poc_msb_present_flag")?;
        let delta_poc_msb_cycle_lt = if delta_poc_msb_present_flag {
            let value = bits.read_ue("delta_poc_msb_cycle_lt")?;
            let derived = if index == 0 || index == num_long_term_sps {
                value
            } else {
                previous_delta_poc_msb_cycle_lt
                    .unwrap_or(0)
                    .saturating_add(value)
            };
            previous_delta_poc_msb_cycle_lt = Some(derived);
            Some(derived)
        } else {
            None
        };
        references.push(NativeVulkanH265LongTermReferenceSnapshot {
            from_sps,
            lt_idx_sps,
            poc_lsb,
            used_by_current,
            delta_poc_msb_present_flag,
            delta_poc_msb_cycle_lt,
        });
    }

    Ok(references)
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h265_ceil_log2(value: u32) -> u32 {
    if value <= 1 {
        0
    } else {
        u32::BITS - (value - 1).leading_zeros()
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
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
    let h265_main8_compatible = sps.bit_depth_luma_minus8 == 0
        && sps.bit_depth_chroma_minus8 == 0
        && sps.profile.main_compatible();
    let h265_main10_compatible = sps.bit_depth_luma_minus8 == 2
        && sps.bit_depth_chroma_minus8 == 2
        && sps.profile.main10_compatible();
    let requested_profile_compatible = sps.chroma_format_idc == 1
        && !sps.separate_colour_plane_flag
        && (h265_main8_compatible || h265_main10_compatible);
    let vulkan_std_session_parameters_ready = requested_profile_compatible
        && vps.id == sps.vps_id
        && sps.id == pps.sps_id
        && native_vulkan_h265_sps_short_term_ref_pic_sets_supported(&sps.short_term_ref_pic_sets)
        && native_vulkan_h265_sps_long_term_ref_pics_supported(&sps.long_term_ref_pics_sps)
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
            short_term_ref_pic_sets: sps.short_term_ref_pic_sets.clone(),
            long_term_ref_pics_present_flag: sps.long_term_ref_pics_present_flag,
            long_term_ref_pics_sps: sps.long_term_ref_pics_sps.clone(),
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

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h265_dec_pic_buf_mgr_snapshot(
    dec_pic_buf_mgr: &NativeVulkanH265ParsedDecPicBufMgr,
) -> NativeVulkanH265DecPicBufMgrSnapshot {
    NativeVulkanH265DecPicBufMgrSnapshot {
        max_latency_increase_plus1: dec_pic_buf_mgr.max_latency_increase_plus1,
        max_dec_pic_buffering_minus1: dec_pic_buf_mgr.max_dec_pic_buffering_minus1,
        max_num_reorder_pics: dec_pic_buf_mgr.max_num_reorder_pics,
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
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

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_h264_parameter_sets(
    access_unit: &[u8],
) -> Result<NativeVulkanH264ParameterSetSnapshot, String> {
    let nal_units = native_vulkan_h264_nal_payloads(access_unit);
    let sps_payload = nal_units
        .iter()
        .find(|unit| unit.nal_type == 7)
        .ok_or_else(|| "H.264 access unit has no SPS NAL".to_owned())?;
    let pps_payload = nal_units
        .iter()
        .find(|unit| unit.nal_type == 8)
        .ok_or_else(|| "H.264 access unit has no PPS NAL".to_owned())?;

    let sps = native_vulkan_parse_h264_sps(sps_payload.payload)?;
    let pps = native_vulkan_parse_h264_pps(pps_payload.payload, &sps)?;
    let requested_profile_compatible =
        h264::native_vulkan_h264_profile_is_8bit_420_decode_candidate(sps.profile_idc)
            && sps.chroma_format_idc == 1
            && !sps.separate_colour_plane_flag
            && sps.bit_depth_luma_minus8 == 0
            && sps.bit_depth_chroma_minus8 == 0;
    let vulkan_std_session_parameters_ready = requested_profile_compatible
        && sps.id == pps.sps_id
        && pps.num_slice_groups_minus1 == 0
        && sps.pic_order_cnt_type <= 2
        && sps.offset_for_ref_frame.len() <= u8::MAX as usize
        && !sps.seq_scaling_matrix_present_flag
        && !pps.pic_scaling_matrix_present_flag;

    Ok(NativeVulkanH264ParameterSetSnapshot {
        parser: "native-rust-h264-sps-pps",
        sps,
        pps,
        requested_profile_compatible,
        vulkan_std_session_parameters_ready,
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_h264_sps(payload: &[u8]) -> Result<NativeVulkanH264SpsSnapshot, String> {
    let rbsp = native_vulkan_h264_rbsp(payload)?;
    if rbsp.len() < 4 {
        return Err("H.264 SPS NAL is too short".to_owned());
    }
    let mut bits = NativeVulkanH264BitReader::new(&rbsp[1..]);
    let profile_idc = native_vulkan_h264_u8(bits.read_bits(8, "profile_idc")?, "profile_idc")?;
    let constraint_flags = native_vulkan_h264_u8(
        bits.read_bits(8, "constraint_set_flags")?,
        "constraint_set_flags",
    )?;
    let constraint_set0_flag = constraint_flags & 0x80 != 0;
    let constraint_set1_flag = constraint_flags & 0x40 != 0;
    let constraint_set2_flag = constraint_flags & 0x20 != 0;
    let constraint_set3_flag = constraint_flags & 0x10 != 0;
    let constraint_set4_flag = constraint_flags & 0x08 != 0;
    let constraint_set5_flag = constraint_flags & 0x04 != 0;
    let level_idc = native_vulkan_h264_u8(bits.read_bits(8, "level_idc")?, "level_idc")?;
    let id = bits.read_ue("seq_parameter_set_id")?;

    let mut chroma_format_idc = 1;
    let mut separate_colour_plane_flag = false;
    let mut bit_depth_luma_minus8 = 0;
    let mut bit_depth_chroma_minus8 = 0;
    let mut qpprime_y_zero_transform_bypass_flag = false;
    let mut seq_scaling_matrix_present_flag = false;
    if h264::native_vulkan_h264_profile_has_high_syntax(profile_idc) {
        chroma_format_idc = bits.read_ue("chroma_format_idc")?;
        if chroma_format_idc > 3 {
            return Err(format!(
                "H.264 chroma_format_idc {chroma_format_idc} is not supported"
            ));
        }
        if chroma_format_idc == 3 {
            separate_colour_plane_flag = bits.read_bool("separate_colour_plane_flag")?;
        }
        bit_depth_luma_minus8 = bits.read_ue("bit_depth_luma_minus8")?;
        bit_depth_chroma_minus8 = bits.read_ue("bit_depth_chroma_minus8")?;
        qpprime_y_zero_transform_bypass_flag =
            bits.read_bool("qpprime_y_zero_transform_bypass_flag")?;
        seq_scaling_matrix_present_flag = bits.read_bool("seq_scaling_matrix_present_flag")?;
        if seq_scaling_matrix_present_flag {
            let scaling_list_count = if chroma_format_idc != 3 { 8 } else { 12 };
            for index in 0..scaling_list_count {
                if bits.read_bool("seq_scaling_list_present_flag")? {
                    let size = if index < 6 { 16 } else { 64 };
                    native_vulkan_h264_skip_scaling_list(&mut bits, size)?;
                }
            }
        }
    }

    let log2_max_frame_num_minus4 = bits.read_ue("log2_max_frame_num_minus4")?;
    let pic_order_cnt_type = bits.read_ue("pic_order_cnt_type")?;
    let mut log2_max_pic_order_cnt_lsb_minus4 = 0;
    let mut delta_pic_order_always_zero_flag = false;
    let mut offset_for_non_ref_pic = 0;
    let mut offset_for_top_to_bottom_field = 0;
    let mut offset_for_ref_frame = Vec::new();
    match pic_order_cnt_type {
        0 => {
            log2_max_pic_order_cnt_lsb_minus4 =
                bits.read_ue("log2_max_pic_order_cnt_lsb_minus4")?;
        }
        1 => {
            delta_pic_order_always_zero_flag =
                bits.read_bool("delta_pic_order_always_zero_flag")?;
            offset_for_non_ref_pic = bits.read_se("offset_for_non_ref_pic")?;
            offset_for_top_to_bottom_field = bits.read_se("offset_for_top_to_bottom_field")?;
            let num_ref_frames_in_pic_order_cnt_cycle =
                bits.read_ue("num_ref_frames_in_pic_order_cnt_cycle")?;
            if num_ref_frames_in_pic_order_cnt_cycle > u8::MAX as u32 {
                return Err(format!(
                    "H.264 num_ref_frames_in_pic_order_cnt_cycle {num_ref_frames_in_pic_order_cnt_cycle} exceeds u8 range"
                ));
            }
            for _ in 0..num_ref_frames_in_pic_order_cnt_cycle {
                offset_for_ref_frame.push(bits.read_se("offset_for_ref_frame")?);
            }
        }
        2 => {}
        _ => {
            return Err(format!(
                "H.264 pic_order_cnt_type {pic_order_cnt_type} is not supported"
            ));
        }
    }

    let max_num_ref_frames = bits.read_ue("max_num_ref_frames")?;
    let gaps_in_frame_num_value_allowed_flag =
        bits.read_bool("gaps_in_frame_num_value_allowed_flag")?;
    let pic_width_in_mbs_minus1 = bits.read_ue("pic_width_in_mbs_minus1")?;
    let pic_height_in_map_units_minus1 = bits.read_ue("pic_height_in_map_units_minus1")?;
    let frame_mbs_only_flag = bits.read_bool("frame_mbs_only_flag")?;
    let mb_adaptive_frame_field_flag = if frame_mbs_only_flag {
        false
    } else {
        bits.read_bool("mb_adaptive_frame_field_flag")?
    };
    let direct_8x8_inference_flag = bits.read_bool("direct_8x8_inference_flag")?;
    let frame_cropping_flag = bits.read_bool("frame_cropping_flag")?;
    let (
        frame_crop_left_offset,
        frame_crop_right_offset,
        frame_crop_top_offset,
        frame_crop_bottom_offset,
    ) = if frame_cropping_flag {
        (
            bits.read_ue("frame_crop_left_offset")?,
            bits.read_ue("frame_crop_right_offset")?,
            bits.read_ue("frame_crop_top_offset")?,
            bits.read_ue("frame_crop_bottom_offset")?,
        )
    } else {
        (0, 0, 0, 0)
    };
    let vui_parameters_present_flag = bits.read_bool("vui_parameters_present_flag")?;
    let vui = if vui_parameters_present_flag {
        Some(native_vulkan_parse_h264_vui_parameters(
            &mut bits,
            &rbsp[1..],
        )?)
    } else {
        None
    };
    let (width, height) = native_vulkan_h264_sps_dimensions(
        chroma_format_idc,
        separate_colour_plane_flag,
        pic_width_in_mbs_minus1,
        pic_height_in_map_units_minus1,
        frame_mbs_only_flag,
        frame_crop_left_offset,
        frame_crop_right_offset,
        frame_crop_top_offset,
        frame_crop_bottom_offset,
    )?;

    Ok(NativeVulkanH264SpsSnapshot {
        id,
        profile_idc,
        profile_label: h264::native_vulkan_h264_profile_idc_label(profile_idc),
        constraint_set0_flag,
        constraint_set1_flag,
        constraint_set2_flag,
        constraint_set3_flag,
        constraint_set4_flag,
        constraint_set5_flag,
        level_idc,
        level_label: native_vulkan_h264_level_idc_byte_label(level_idc),
        chroma_format_idc,
        chroma_format_label: native_vulkan_h264_chroma_format_label(chroma_format_idc),
        separate_colour_plane_flag,
        bit_depth_luma_minus8,
        bit_depth_chroma_minus8,
        qpprime_y_zero_transform_bypass_flag,
        seq_scaling_matrix_present_flag,
        log2_max_frame_num_minus4,
        pic_order_cnt_type,
        log2_max_pic_order_cnt_lsb_minus4,
        delta_pic_order_always_zero_flag,
        offset_for_non_ref_pic,
        offset_for_top_to_bottom_field,
        offset_for_ref_frame,
        max_num_ref_frames,
        gaps_in_frame_num_value_allowed_flag,
        pic_width_in_mbs_minus1,
        pic_height_in_map_units_minus1,
        frame_mbs_only_flag,
        mb_adaptive_frame_field_flag,
        direct_8x8_inference_flag,
        frame_cropping_flag,
        frame_crop_left_offset,
        frame_crop_right_offset,
        frame_crop_top_offset,
        frame_crop_bottom_offset,
        vui_parameters_present_flag,
        vui,
        width,
        height,
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_h264_pps(
    payload: &[u8],
    sps: &NativeVulkanH264SpsSnapshot,
) -> Result<NativeVulkanH264PpsSnapshot, String> {
    let rbsp = native_vulkan_h264_rbsp(payload)?;
    if rbsp.len() < 2 {
        return Err("H.264 PPS NAL is too short".to_owned());
    }
    let mut bits = NativeVulkanH264BitReader::new(&rbsp[1..]);
    let id = bits.read_ue("pic_parameter_set_id")?;
    let sps_id = bits.read_ue("seq_parameter_set_id")?;
    let entropy_coding_mode_flag = bits.read_bool("entropy_coding_mode_flag")?;
    let bottom_field_pic_order_in_frame_present_flag =
        bits.read_bool("bottom_field_pic_order_in_frame_present_flag")?;
    let num_slice_groups_minus1 = bits.read_ue("num_slice_groups_minus1")?;
    if num_slice_groups_minus1 > 0 {
        return Err(format!(
            "H.264 num_slice_groups_minus1 {num_slice_groups_minus1} is not supported"
        ));
    }
    let num_ref_idx_l0_default_active_minus1 =
        bits.read_ue("num_ref_idx_l0_default_active_minus1")?;
    let num_ref_idx_l1_default_active_minus1 =
        bits.read_ue("num_ref_idx_l1_default_active_minus1")?;
    let weighted_pred_flag = bits.read_bool("weighted_pred_flag")?;
    let weighted_bipred_idc = bits.read_bits(2, "weighted_bipred_idc")?;
    let pic_init_qp_minus26 = bits.read_se("pic_init_qp_minus26")?;
    let pic_init_qs_minus26 = bits.read_se("pic_init_qs_minus26")?;
    let chroma_qp_index_offset = bits.read_se("chroma_qp_index_offset")?;
    let deblocking_filter_control_present_flag =
        bits.read_bool("deblocking_filter_control_present_flag")?;
    let constrained_intra_pred_flag = bits.read_bool("constrained_intra_pred_flag")?;
    let redundant_pic_cnt_present_flag = bits.read_bool("redundant_pic_cnt_present_flag")?;
    let mut transform_8x8_mode_flag = false;
    let mut pic_scaling_matrix_present_flag = false;
    let mut second_chroma_qp_index_offset = chroma_qp_index_offset;
    if native_vulkan_rbsp_more_data(&rbsp[1..], bits.bit_offset()) {
        transform_8x8_mode_flag = bits.read_bool("transform_8x8_mode_flag")?;
        pic_scaling_matrix_present_flag = bits.read_bool("pic_scaling_matrix_present_flag")?;
        if pic_scaling_matrix_present_flag {
            let scaling_list_count = 6 + if transform_8x8_mode_flag {
                if sps.chroma_format_idc != 3 { 2 } else { 6 }
            } else {
                0
            };
            for index in 0..scaling_list_count {
                if bits.read_bool("pic_scaling_list_present_flag")? {
                    let size = if index < 6 { 16 } else { 64 };
                    native_vulkan_h264_skip_scaling_list(&mut bits, size)?;
                }
            }
        }
        if native_vulkan_rbsp_more_data(&rbsp[1..], bits.bit_offset()) {
            second_chroma_qp_index_offset = bits.read_se("second_chroma_qp_index_offset")?;
        }
    }

    Ok(NativeVulkanH264PpsSnapshot {
        id,
        sps_id,
        entropy_coding_mode_flag,
        bottom_field_pic_order_in_frame_present_flag,
        num_slice_groups_minus1,
        num_ref_idx_l0_default_active_minus1,
        num_ref_idx_l1_default_active_minus1,
        weighted_pred_flag,
        weighted_bipred_idc,
        pic_init_qp_minus26,
        pic_init_qs_minus26,
        chroma_qp_index_offset,
        deblocking_filter_control_present_flag,
        constrained_intra_pred_flag,
        redundant_pic_cnt_present_flag,
        transform_8x8_mode_flag,
        pic_scaling_matrix_present_flag,
        second_chroma_qp_index_offset,
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h264_sps_dimensions(
    chroma_format_idc: u32,
    separate_colour_plane_flag: bool,
    pic_width_in_mbs_minus1: u32,
    pic_height_in_map_units_minus1: u32,
    frame_mbs_only_flag: bool,
    frame_crop_left_offset: u32,
    frame_crop_right_offset: u32,
    frame_crop_top_offset: u32,
    frame_crop_bottom_offset: u32,
) -> Result<(u32, u32), String> {
    let chroma_array_type = if separate_colour_plane_flag {
        0
    } else {
        chroma_format_idc
    };
    let (sub_width_c, sub_height_c): (u32, u32) = match chroma_format_idc {
        0 => (1, 1),
        1 => (2, 2),
        2 => (2, 1),
        3 => (1, 1),
        _ => {
            return Err(format!(
                "H.264 chroma_format_idc {chroma_format_idc} is not supported"
            ));
        }
    };
    let frame_height_in_mbs_factor = if frame_mbs_only_flag { 1 } else { 2 };
    let (crop_unit_x, crop_unit_y) = if chroma_array_type == 0 {
        (1, frame_height_in_mbs_factor)
    } else {
        (
            sub_width_c,
            sub_height_c.saturating_mul(frame_height_in_mbs_factor),
        )
    };
    let coded_width = pic_width_in_mbs_minus1
        .checked_add(1)
        .and_then(|mbs| mbs.checked_mul(16))
        .ok_or_else(|| "H.264 SPS width overflow".to_owned())?;
    let coded_height = pic_height_in_map_units_minus1
        .checked_add(1)
        .and_then(|map_units| map_units.checked_mul(frame_height_in_mbs_factor))
        .and_then(|mbs| mbs.checked_mul(16))
        .ok_or_else(|| "H.264 SPS height overflow".to_owned())?;
    let crop_width = frame_crop_left_offset
        .checked_add(frame_crop_right_offset)
        .and_then(|crop| crop.checked_mul(crop_unit_x))
        .ok_or_else(|| "H.264 SPS crop width overflow".to_owned())?;
    let crop_height = frame_crop_top_offset
        .checked_add(frame_crop_bottom_offset)
        .and_then(|crop| crop.checked_mul(crop_unit_y))
        .ok_or_else(|| "H.264 SPS crop height overflow".to_owned())?;
    Ok((
        coded_width.saturating_sub(crop_width),
        coded_height.saturating_sub(crop_height),
    ))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h264_skip_scaling_list(
    bits: &mut NativeVulkanH264BitReader<'_>,
    size: u32,
) -> Result<(), String> {
    let mut last_scale = 8i32;
    let mut next_scale = 8i32;
    for _ in 0..size {
        if next_scale != 0 {
            let delta_scale = bits.read_se("delta_scale")?;
            next_scale = (last_scale + delta_scale + 256) % 256;
        }
        if next_scale != 0 {
            last_scale = next_scale;
        }
    }
    Ok(())
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_h264_vui_parameters(
    bits: &mut NativeVulkanH264BitReader<'_>,
    rbsp_payload: &[u8],
) -> Result<NativeVulkanH264VuiSnapshot, String> {
    let aspect_ratio_info_present_flag = bits.read_bool("aspect_ratio_info_present_flag")?;
    let mut aspect_ratio_idc = 0;
    let mut sar_width = 0;
    let mut sar_height = 0;
    if aspect_ratio_info_present_flag {
        aspect_ratio_idc = bits.read_bits(8, "aspect_ratio_idc")?;
        if aspect_ratio_idc == 255 {
            sar_width = bits.read_bits(16, "sar_width")?;
            sar_height = bits.read_bits(16, "sar_height")?;
        }
    }

    let overscan_info_present_flag = bits.read_bool("overscan_info_present_flag")?;
    let overscan_appropriate_flag = if overscan_info_present_flag {
        bits.read_bool("overscan_appropriate_flag")?
    } else {
        false
    };

    let video_signal_type_present_flag = bits.read_bool("video_signal_type_present_flag")?;
    let mut video_format = 5;
    let mut video_full_range_flag = false;
    let mut colour_description_present_flag = false;
    let mut colour_primaries = 2;
    let mut transfer_characteristics = 2;
    let mut matrix_coeffs = 2;
    if video_signal_type_present_flag {
        video_format = bits.read_bits(3, "video_format")?;
        video_full_range_flag = bits.read_bool("video_full_range_flag")?;
        colour_description_present_flag = bits.read_bool("colour_description_present_flag")?;
        if colour_description_present_flag {
            colour_primaries = bits.read_bits(8, "colour_primaries")?;
            transfer_characteristics = bits.read_bits(8, "transfer_characteristics")?;
            matrix_coeffs = bits.read_bits(8, "matrix_coeffs")?;
        }
    }

    let chroma_loc_info_present_flag = bits.read_bool("chroma_loc_info_present_flag")?;
    let mut chroma_sample_loc_type_top_field = 0;
    let mut chroma_sample_loc_type_bottom_field = 0;
    if chroma_loc_info_present_flag {
        chroma_sample_loc_type_top_field = bits.read_ue("chroma_sample_loc_type_top_field")?;
        chroma_sample_loc_type_bottom_field =
            bits.read_ue("chroma_sample_loc_type_bottom_field")?;
    }

    let timing_info_present_flag = bits.read_bool("timing_info_present_flag")?;
    let mut num_units_in_tick = 0;
    let mut time_scale = 0;
    let mut fixed_frame_rate_flag = false;
    if timing_info_present_flag {
        num_units_in_tick = bits.read_bits(32, "num_units_in_tick")?;
        time_scale = bits.read_bits(32, "time_scale")?;
        fixed_frame_rate_flag = bits.read_bool("fixed_frame_rate_flag")?;
    }

    let nal_hrd_parameters_present_flag = bits.read_bool("nal_hrd_parameters_present_flag")?;
    if nal_hrd_parameters_present_flag {
        native_vulkan_skip_h264_hrd_parameters(bits)?;
    }
    let vcl_hrd_parameters_present_flag = bits.read_bool("vcl_hrd_parameters_present_flag")?;
    if vcl_hrd_parameters_present_flag {
        native_vulkan_skip_h264_hrd_parameters(bits)?;
    }
    let low_delay_hrd_flag = if nal_hrd_parameters_present_flag || vcl_hrd_parameters_present_flag {
        bits.read_bool("low_delay_hrd_flag")?
    } else {
        false
    };
    let pic_struct_present_flag = bits.read_bool("pic_struct_present_flag")?;

    let mut bitstream_restriction_flag = false;
    let mut motion_vectors_over_pic_boundaries_flag = false;
    let mut max_bytes_per_pic_denom = 0;
    let mut max_bits_per_mb_denom = 0;
    let mut log2_max_mv_length_horizontal = 0;
    let mut log2_max_mv_length_vertical = 0;
    let mut num_reorder_frames = 0;
    let mut max_dec_frame_buffering = 0;
    if native_vulkan_rbsp_more_data(rbsp_payload, bits.bit_offset()) {
        bitstream_restriction_flag = bits.read_bool("bitstream_restriction_flag")?;
        if bitstream_restriction_flag {
            motion_vectors_over_pic_boundaries_flag =
                bits.read_bool("motion_vectors_over_pic_boundaries_flag")?;
            max_bytes_per_pic_denom = bits.read_ue("max_bytes_per_pic_denom")?;
            max_bits_per_mb_denom = bits.read_ue("max_bits_per_mb_denom")?;
            log2_max_mv_length_horizontal = bits.read_ue("log2_max_mv_length_horizontal")?;
            log2_max_mv_length_vertical = bits.read_ue("log2_max_mv_length_vertical")?;
            num_reorder_frames = bits.read_ue("num_reorder_frames")?;
            max_dec_frame_buffering = bits.read_ue("max_dec_frame_buffering")?;
            if num_reorder_frames > 16 {
                return Err(format!(
                    "H.264 num_reorder_frames {num_reorder_frames} exceeds Vulkan Video DPB bound"
                ));
            }
        }
    }

    Ok(NativeVulkanH264VuiSnapshot {
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
        timing_info_present_flag,
        num_units_in_tick,
        time_scale,
        fixed_frame_rate_flag,
        nal_hrd_parameters_present_flag,
        vcl_hrd_parameters_present_flag,
        low_delay_hrd_flag,
        pic_struct_present_flag,
        bitstream_restriction_flag,
        motion_vectors_over_pic_boundaries_flag,
        max_bytes_per_pic_denom,
        max_bits_per_mb_denom,
        log2_max_mv_length_horizontal,
        log2_max_mv_length_vertical,
        num_reorder_frames,
        max_dec_frame_buffering,
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_skip_h264_hrd_parameters(
    bits: &mut NativeVulkanH264BitReader<'_>,
) -> Result<(), String> {
    let cpb_cnt_minus1 = bits.read_ue("cpb_cnt_minus1")?;
    if cpb_cnt_minus1 > 31 {
        return Err(format!(
            "H.264 cpb_cnt_minus1 {cpb_cnt_minus1} exceeds HRD bound"
        ));
    }
    bits.read_bits(4, "bit_rate_scale")?;
    bits.read_bits(4, "cpb_size_scale")?;
    for _ in 0..=cpb_cnt_minus1 {
        bits.read_ue("bit_rate_value_minus1")?;
        bits.read_ue("cpb_size_value_minus1")?;
        bits.read_bool("cbr_flag")?;
    }
    bits.read_bits(5, "initial_cpb_removal_delay_length_minus1")?;
    bits.read_bits(5, "cpb_removal_delay_length_minus1")?;
    bits.read_bits(5, "dpb_output_delay_length_minus1")?;
    bits.read_bits(5, "time_offset_length")?;
    Ok(())
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h264_chroma_format_label(chroma_format_idc: u32) -> &'static str {
    match chroma_format_idc {
        0 => "monochrome",
        1 => "4:2:0",
        2 => "4:2:2",
        3 => "4:4:4",
        _ => "unknown",
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h264_level_idc_byte_label(level_idc: u8) -> Option<&'static str> {
    match level_idc {
        10 => Some("1.0"),
        11 => Some("1.1"),
        12 => Some("1.2"),
        13 => Some("1.3"),
        20 => Some("2.0"),
        21 => Some("2.1"),
        22 => Some("2.2"),
        30 => Some("3.0"),
        31 => Some("3.1"),
        32 => Some("3.2"),
        40 => Some("4.0"),
        41 => Some("4.1"),
        42 => Some("4.2"),
        50 => Some("5.0"),
        51 => Some("5.1"),
        52 => Some("5.2"),
        60 => Some("6.0"),
        61 => Some("6.1"),
        62 => Some("6.2"),
        _ => None,
    }
}

pub(super) fn native_vulkan_h264_u8(value: u32, label: &'static str) -> Result<u8, String> {
    u8::try_from(value).map_err(|_| format!("{label}={value} exceeds u8 range"))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h264_u16(value: u32, label: &'static str) -> Result<u16, String> {
    u16::try_from(value).map_err(|_| format!("{label}={value} exceeds u16 range"))
}

pub(super) fn native_vulkan_h264_i8(value: i32, label: &'static str) -> Result<i8, String> {
    i8::try_from(value).map_err(|_| format!("{label}={value} exceeds i8 range"))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h264_rbsp(payload: &[u8]) -> Result<Cow<'_, [u8]>, String> {
    if payload.is_empty() {
        return Err("H.264 NAL payload is empty".to_owned());
    }
    Ok(native_vulkan_rbsp_unescape(payload))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_rbsp_unescape(payload: &[u8]) -> Cow<'_, [u8]> {
    let mut zero_count = 0u8;
    let mut first_escape = None;
    for (index, byte) in payload.iter().copied().enumerate() {
        if zero_count == 2 && byte == 0x03 {
            first_escape = Some(index);
            break;
        }
        if byte == 0 {
            zero_count = zero_count.saturating_add(1).min(2);
        } else {
            zero_count = 0;
        }
    }

    let Some(first_escape) = first_escape else {
        return Cow::Borrowed(payload);
    };

    let mut rbsp = Vec::with_capacity(payload.len());
    rbsp.extend_from_slice(&payload[..first_escape]);
    zero_count = 2;
    for byte in payload[first_escape..].iter().copied() {
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
    Cow::Owned(rbsp)
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_rbsp_more_data(bytes: &[u8], bit_offset: usize) -> bool {
    let total_bits = bytes.len().saturating_mul(8);
    if bit_offset >= total_bits {
        return false;
    }
    let mut last_one_bit = None;
    for bit in bit_offset..total_bits {
        let byte = bytes[bit / 8];
        let shift = 7 - (bit % 8);
        if ((byte >> shift) & 1) != 0 {
            last_one_bit = Some(bit);
        }
    }
    last_one_bit.is_some_and(|last| bit_offset < last)
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy)]
struct NativeVulkanH264NalPayload<'a> {
    nal_type: u8,
    nal_ref_idc: u8,
    payload: &'a [u8],
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h264_nal_payloads(bytes: &[u8]) -> Vec<NativeVulkanH264NalPayload<'_>> {
    let mut payloads = Vec::new();
    let mut offset = 0usize;
    while let Some((_, payload_offset)) = native_vulkan_next_annex_b_start_code(bytes, offset) {
        let next_start = native_vulkan_next_annex_b_start_code(bytes, payload_offset)
            .map(|(next_start, _)| next_start)
            .unwrap_or(bytes.len());
        if payload_offset < next_start
            && let Some(header) = bytes.get(payload_offset).copied()
        {
            payloads.push(NativeVulkanH264NalPayload {
                nal_type: header & 0x1f,
                nal_ref_idc: (header >> 5) & 0x03,
                payload: &bytes[payload_offset..next_start],
            });
        }
        offset = next_start;
    }
    payloads
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h264_annex_b_slice_offset(
    start_code_offset: usize,
    payload_offset: usize,
) -> usize {
    payload_offset
        .checked_sub(3)
        .filter(|offset| *offset >= start_code_offset)
        .unwrap_or(start_code_offset)
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_read_bits_be(
    bytes: &[u8],
    bit_offset: &mut usize,
    count: u32,
    label: &'static str,
    bounds_label: &'static str,
) -> Result<u32, String> {
    if count > 32 {
        return Err(format!("{label} requested too many bits: {count}"));
    }
    if count == 0 {
        return Ok(0);
    }
    let start = *bit_offset;
    let end = start
        .checked_add(count as usize)
        .ok_or_else(|| format!("{label} bit offset overflow"))?;
    if end > bytes.len() * 8 {
        return Err(format!("{label} exceeds {bounds_label}"));
    }

    // FFmpeg's get_bits() reads a cached word and advances the bit index
    // (references/ffmpeg/libavcodec/get_bits.h:337-350). Keep the same shape
    // instead of looping one bit at a time in H.264/H.265 slice parsing.
    let byte_start = start / 8;
    let bit_in_byte = start % 8;
    let byte_count = bit_in_byte
        .checked_add(count as usize)
        .ok_or_else(|| format!("{label} bit window overflow"))?
        .div_ceil(8);
    let mut window = 0u64;
    for byte in &bytes[byte_start..byte_start + byte_count] {
        window = (window << 8) | u64::from(*byte);
    }
    let total_bits = byte_count * 8;
    let shift = total_bits - bit_in_byte - count as usize;
    let mask = if count == 32 {
        u64::from(u32::MAX)
    } else {
        (1u64 << count) - 1
    };
    *bit_offset = end;
    Ok(((window >> shift) & mask) as u32)
}

struct NativeVulkanH264BitReader<'a> {
    bytes: &'a [u8],
    bit_offset: usize,
}

#[cfg(any(feature = "native-vulkan-video", test))]
impl<'a> NativeVulkanH264BitReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            bit_offset: 0,
        }
    }

    fn bit_offset(&self) -> usize {
        self.bit_offset
    }

    fn read_bool(&mut self, label: &'static str) -> Result<bool, String> {
        Ok(self.read_bits(1, label)? != 0)
    }

    fn read_bits(&mut self, count: u32, label: &'static str) -> Result<u32, String> {
        native_vulkan_read_bits_be(
            self.bytes,
            &mut self.bit_offset,
            count,
            label,
            "H.264 RBSP length",
        )
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

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h264_chroma_array_type(sps: &NativeVulkanH264SpsSnapshot) -> u32 {
    if sps.separate_colour_plane_flag {
        0
    } else {
        sps.chroma_format_idc
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h264_skip_pred_weight_table(
    bits: &mut NativeVulkanH264BitReader<'_>,
    parameter_sets: &NativeVulkanH264ParameterSetSnapshot,
    is_p: bool,
    is_b: bool,
    num_ref_idx_l0_active_minus1: Option<u32>,
    num_ref_idx_l1_active_minus1: Option<u32>,
) -> Result<(), String> {
    let weighted_p = parameter_sets.pps.weighted_pred_flag && is_p;
    let explicit_weighted_b = parameter_sets.pps.weighted_bipred_idc == 1 && is_b;
    if !weighted_p && !explicit_weighted_b {
        return Ok(());
    }

    bits.read_ue("luma_log2_weight_denom")?;
    let has_chroma = native_vulkan_h264_chroma_array_type(&parameter_sets.sps) != 0;
    if has_chroma {
        bits.read_ue("chroma_log2_weight_denom")?;
    }
    let l0_count = num_ref_idx_l0_active_minus1
        .ok_or_else(|| "H.264 weighted prediction table is missing L0 ref count".to_owned())?
        .checked_add(1)
        .ok_or_else(|| "H.264 weighted prediction L0 ref count overflow".to_owned())?;
    native_vulkan_h264_skip_pred_weight_table_entries(bits, l0_count, has_chroma)?;
    if explicit_weighted_b {
        let l1_count = num_ref_idx_l1_active_minus1
            .ok_or_else(|| "H.264 weighted prediction table is missing L1 ref count".to_owned())?
            .checked_add(1)
            .ok_or_else(|| "H.264 weighted prediction L1 ref count overflow".to_owned())?;
        native_vulkan_h264_skip_pred_weight_table_entries(bits, l1_count, has_chroma)?;
    }

    Ok(())
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h264_skip_pred_weight_table_entries(
    bits: &mut NativeVulkanH264BitReader<'_>,
    count: u32,
    has_chroma: bool,
) -> Result<(), String> {
    if count > 32 {
        return Err(format!(
            "H.264 weighted prediction ref count {count} exceeds supported parser bound"
        ));
    }
    for _ in 0..count {
        if bits.read_bool("luma_weight_flag")? {
            bits.read_se("luma_weight")?;
            bits.read_se("luma_offset")?;
        }
        if has_chroma && bits.read_bool("chroma_weight_flag")? {
            for _ in 0..2 {
                bits.read_se("chroma_weight")?;
                bits.read_se("chroma_offset")?;
            }
        }
    }

    Ok(())
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h264_read_ref_pic_list_modifications(
    bits: &mut NativeVulkanH264BitReader<'_>,
    flag_label: &'static str,
    list_label: &'static str,
) -> Result<(bool, Vec<NativeVulkanH264RefPicListModificationSnapshot>), String> {
    let modification_flag = bits.read_bool(flag_label)?;
    let mut modifications = Vec::<NativeVulkanH264RefPicListModificationSnapshot>::new();
    if !modification_flag {
        return Ok((false, modifications));
    }

    loop {
        let modification_of_pic_nums_idc = bits.read_ue("modification_of_pic_nums_idc")?;
        if modification_of_pic_nums_idc == 3 {
            break;
        }
        let (abs_diff_pic_num_minus1, long_term_pic_num) = match modification_of_pic_nums_idc {
            0 | 1 => (Some(bits.read_ue("abs_diff_pic_num_minus1")?), None),
            2 => (None, Some(bits.read_ue("long_term_pic_num")?)),
            other => {
                return Err(format!(
                    "H.264 ref_pic_list_modification_{list_label} idc {other} is not supported"
                ));
            }
        };
        modifications.push(NativeVulkanH264RefPicListModificationSnapshot {
            modification_of_pic_nums_idc,
            abs_diff_pic_num_minus1,
            long_term_pic_num,
        });
    }

    Ok((true, modifications))
}

#[derive(Debug, Clone, Copy)]
struct NativeVulkanH265NalPayload<'a> {
    nal_type: u8,
    #[cfg_attr(not(test), allow(dead_code))]
    start_code_offset: usize,
    slice_segment_offset: usize,
    #[cfg_attr(not(test), allow(dead_code))]
    payload_offset: usize,
    #[cfg_attr(not(feature = "native-vulkan-video"), allow(dead_code))]
    payload: &'a [u8],
}

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
                slice_segment_offset: native_vulkan_h265_annex_b_slice_segment_offset(
                    start_code_offset,
                    payload_offset,
                ),
                payload_offset,
                payload: &bytes[payload_offset..next_start],
            });
        }
        offset = next_start;
    }
    payloads
}

fn native_vulkan_h265_annex_b_slice_segment_offset(
    start_code_offset: usize,
    payload_offset: usize,
) -> usize {
    payload_offset
        .checked_sub(3)
        .filter(|offset| *offset >= start_code_offset)
        .unwrap_or(start_code_offset)
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeVulkanH264FirstFrameDecodeInfo {
    nal_type: u8,
    nal_type_label: &'static str,
    nal_ref_idc: u8,
    first_mb_in_slice: u32,
    first_slice_segment_in_pic_flag: bool,
    slice_type: u32,
    slice_type_normalized: u32,
    pps_id: u32,
    frame_num: u16,
    idr_pic_id: u16,
    num_ref_idx_l0_active_minus1: Option<u32>,
    num_ref_idx_l1_active_minus1: Option<u32>,
    ref_pic_list_modification_l0: bool,
    ref_pic_list_modifications_l0: Vec<NativeVulkanH264RefPicListModificationSnapshot>,
    ref_pic_list_modification_l1: bool,
    ref_pic_list_modifications_l1: Vec<NativeVulkanH264RefPicListModificationSnapshot>,
    adaptive_ref_pic_marking_mode_flag: bool,
    memory_management_control_operations:
        Vec<NativeVulkanH264MemoryManagementControlOperationSnapshot>,
    field_pic_flag: bool,
    bottom_field_flag: bool,
    is_reference: bool,
    is_intra: bool,
    is_p: bool,
    is_b: bool,
    long_term_reference_flag: bool,
    pic_order_cnt: [i32; 2],
    slice_offsets: NativeVulkanH264SliceOffsets,
    idr: bool,
    irap: bool,
}

#[cfg(test)]
fn native_vulkan_h264_first_frame_decode_info(
    access_unit: &[u8],
    parameter_sets: &NativeVulkanH264ParameterSetSnapshot,
) -> Result<NativeVulkanH264FirstFrameDecodeInfo, String> {
    let picture = native_vulkan_h264_picture_decode_info(access_unit, parameter_sets, 0)?;
    if !picture.idr {
        return Err(format!(
            "H.264 first-frame decode currently supports IDR only, got {}",
            picture.nal_type_label
        ));
    }
    if !picture.is_intra {
        return Err(format!(
            "H.264 IDR first slice must be I-slice for the first decode subset, got {}",
            picture.slice_type
        ));
    }
    Ok(picture)
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h264_picture_decode_info_from_stats(
    access_unit: &[u8],
    stats: &NativeVulkanH264NalStats,
    parameter_sets: &NativeVulkanH264ParameterSetSnapshot,
) -> Result<NativeVulkanH264FirstFrameDecodeInfo, String> {
    let first_slice = stats
        .first_slice
        .ok_or_else(|| "H.264 access unit has no slice NAL".to_owned())?;
    if stats.slice_offsets.is_empty() {
        return Err("H.264 access unit has no slice offsets".to_owned());
    }
    if first_slice.payload_start >= first_slice.payload_end
        || first_slice.payload_end > access_unit.len()
    {
        return Err("H.264 first slice payload range exceeds access-unit bounds".to_owned());
    }

    let slice = NativeVulkanH264NalPayload {
        nal_type: first_slice.nal_type,
        nal_ref_idc: first_slice.nal_ref_idc,
        payload: &access_unit[first_slice.payload_start..first_slice.payload_end],
    };
    let mut first_slice = native_vulkan_h264_slice_decode_info(&slice, parameter_sets)?;
    first_slice.slice_offsets = stats.slice_offsets.clone();
    Ok(first_slice)
}

#[cfg(test)]
fn native_vulkan_h264_picture_decode_info(
    access_unit: &[u8],
    parameter_sets: &NativeVulkanH264ParameterSetSnapshot,
    _slice_count_hint: usize,
) -> Result<NativeVulkanH264FirstFrameDecodeInfo, String> {
    let mut first_slice = None;
    let mut slice_offsets = NativeVulkanH264SliceOffsets::new();
    let mut offset = 0usize;
    while let Some((start_code_offset, payload_offset)) =
        native_vulkan_next_annex_b_start_code(access_unit, offset)
    {
        let next_start = native_vulkan_next_annex_b_start_code(access_unit, payload_offset)
            .map(|(next_start, _)| next_start)
            .unwrap_or(access_unit.len());
        if payload_offset < next_start
            && let Some(header) = access_unit.get(payload_offset).copied()
        {
            let nal_type = header & 0x1f;
            if matches!(nal_type, 1..=5) {
                let slice = NativeVulkanH264NalPayload {
                    nal_type,
                    nal_ref_idc: (header >> 5) & 0x03,
                    payload: &access_unit[payload_offset..next_start],
                };
                let slice_offset =
                    native_vulkan_h264_annex_b_slice_offset(start_code_offset, payload_offset);
                if first_slice.is_none() {
                    first_slice = Some(native_vulkan_h264_slice_decode_info(
                        &slice,
                        parameter_sets,
                    )?);
                }
                slice_offsets.push(
                    u32::try_from(slice_offset)
                        .map_err(|_| "H.264 slice offset exceeds u32 range".to_owned())?,
                );
            }
        }
        offset = next_start;
    }
    let mut first_slice =
        first_slice.ok_or_else(|| "H.264 access unit has no slice NAL".to_owned())?;
    if slice_offsets.is_empty() {
        return Err("H.264 access unit has no slice offsets".to_owned());
    }
    // FFmpeg's Vulkan H.264 path takes the already-parsed first slice context
    // as picture info and appends every NAL through ff_vk_decode_add_slice(),
    // which only grows the reusable slice-offset array.
    // See references/ffmpeg/libavcodec/vulkan_h264.c:481-495 and
    // references/ffmpeg/libavcodec/vulkan_decode.c:309-340.
    first_slice.slice_offsets = slice_offsets;
    Ok(first_slice)
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h264_slice_decode_info(
    slice: &NativeVulkanH264NalPayload<'_>,
    parameter_sets: &NativeVulkanH264ParameterSetSnapshot,
) -> Result<NativeVulkanH264FirstFrameDecodeInfo, String> {
    let rbsp = native_vulkan_h264_rbsp(slice.payload)?;
    if rbsp.len() < 2 {
        return Err("H.264 slice NAL is too short".to_owned());
    }
    let mut bits = NativeVulkanH264BitReader::new(&rbsp[1..]);
    let first_mb_in_slice = bits.read_ue("first_mb_in_slice")?;
    let slice_type = bits.read_ue("slice_type")?;
    let normalized_slice_type = slice_type % 5;
    let is_intra = normalized_slice_type == vk::video::STD_VIDEO_H264_SLICE_TYPE_I.0 as u32;
    let is_p = normalized_slice_type == vk::video::STD_VIDEO_H264_SLICE_TYPE_P.0 as u32;
    let is_b = normalized_slice_type == vk::video::STD_VIDEO_H264_SLICE_TYPE_B.0 as u32;
    let pps_id = bits.read_ue("pic_parameter_set_id")?;
    if pps_id != parameter_sets.pps.id {
        return Err(format!(
            "H.264 slice PPS id {pps_id} does not match session PPS id {}",
            parameter_sets.pps.id
        ));
    }
    let frame_num_bits = parameter_sets
        .sps
        .log2_max_frame_num_minus4
        .checked_add(4)
        .ok_or_else(|| "H.264 frame_num bit count overflow".to_owned())?;
    let frame_num =
        native_vulkan_h264_u16(bits.read_bits(frame_num_bits, "frame_num")?, "frame_num")?;
    let mut field_pic_flag = false;
    let mut bottom_field_flag = false;
    if !parameter_sets.sps.frame_mbs_only_flag {
        field_pic_flag = bits.read_bool("field_pic_flag")?;
        if field_pic_flag {
            bottom_field_flag = bits.read_bool("bottom_field_flag")?;
        }
    }
    let idr = slice.nal_type == 5;
    let idr_pic_id = if idr {
        native_vulkan_h264_u16(bits.read_ue("idr_pic_id")?, "idr_pic_id")?
    } else {
        0
    };
    let mut delta_pic_order_cnt_bottom = 0;
    let pic_order_cnt = match parameter_sets.sps.pic_order_cnt_type {
        0 => {
            let pic_order_cnt_lsb_bits = parameter_sets
                .sps
                .log2_max_pic_order_cnt_lsb_minus4
                .checked_add(4)
                .ok_or_else(|| "H.264 pic_order_cnt_lsb bit count overflow".to_owned())?;
            let pic_order_cnt_lsb =
                bits.read_bits(pic_order_cnt_lsb_bits, "pic_order_cnt_lsb")? as i32;
            if parameter_sets
                .pps
                .bottom_field_pic_order_in_frame_present_flag
                && !field_pic_flag
            {
                delta_pic_order_cnt_bottom = bits.read_se("delta_pic_order_cnt_bottom")?;
            }
            [
                pic_order_cnt_lsb,
                pic_order_cnt_lsb + delta_pic_order_cnt_bottom,
            ]
        }
        1 if !parameter_sets.sps.delta_pic_order_always_zero_flag => {
            let delta_pic_order_cnt_0 = bits.read_se("delta_pic_order_cnt[0]")?;
            if parameter_sets
                .pps
                .bottom_field_pic_order_in_frame_present_flag
                && !field_pic_flag
            {
                let _delta_pic_order_cnt_top = bits.read_se("delta_pic_order_cnt[1]")?;
            }
            if idr {
                [0, 0]
            } else {
                [i32::from(frame_num).saturating_add(delta_pic_order_cnt_0); 2]
            }
        }
        1 | 2 => {
            if idr {
                [0, 0]
            } else {
                [i32::from(frame_num); 2]
            }
        }
        other => {
            return Err(format!("H.264 pic_order_cnt_type {other} is not supported"));
        }
    };
    if parameter_sets.pps.redundant_pic_cnt_present_flag {
        bits.read_ue("redundant_pic_cnt")?;
    }
    if is_b {
        bits.read_bool("direct_spatial_mv_pred_flag")?;
    }
    let mut num_ref_idx_l0_active_minus1 = None::<u32>;
    let mut num_ref_idx_l1_active_minus1 = None::<u32>;
    if is_p || is_b {
        if bits.read_bool("num_ref_idx_active_override_flag")? {
            num_ref_idx_l0_active_minus1 = Some(bits.read_ue("num_ref_idx_l0_active_minus1")?);
            if is_b {
                num_ref_idx_l1_active_minus1 = Some(bits.read_ue("num_ref_idx_l1_active_minus1")?);
            }
        } else {
            num_ref_idx_l0_active_minus1 =
                Some(parameter_sets.pps.num_ref_idx_l0_default_active_minus1);
            if is_b {
                num_ref_idx_l1_active_minus1 =
                    Some(parameter_sets.pps.num_ref_idx_l1_default_active_minus1);
            }
        }
    }
    let (ref_pic_list_modification_l0, ref_pic_list_modifications_l0) = if is_p || is_b {
        native_vulkan_h264_read_ref_pic_list_modifications(
            &mut bits,
            "ref_pic_list_modification_flag_l0",
            "l0",
        )?
    } else {
        (false, Vec::new())
    };
    let (ref_pic_list_modification_l1, ref_pic_list_modifications_l1) = if is_b {
        native_vulkan_h264_read_ref_pic_list_modifications(
            &mut bits,
            "ref_pic_list_modification_flag_l1",
            "l1",
        )?
    } else {
        (false, Vec::new())
    };
    native_vulkan_h264_skip_pred_weight_table(
        &mut bits,
        parameter_sets,
        is_p,
        is_b,
        num_ref_idx_l0_active_minus1,
        num_ref_idx_l1_active_minus1,
    )?;
    let mut long_term_reference_flag = false;
    let mut adaptive_ref_pic_marking_mode_flag = false;
    let mut memory_management_control_operations =
        Vec::<NativeVulkanH264MemoryManagementControlOperationSnapshot>::new();
    if slice.nal_ref_idc != 0 && idr {
        bits.read_bool("no_output_of_prior_pics_flag")?;
        long_term_reference_flag = bits.read_bool("long_term_reference_flag")?;
    } else if slice.nal_ref_idc != 0 {
        adaptive_ref_pic_marking_mode_flag =
            bits.read_bool("adaptive_ref_pic_marking_mode_flag")?;
        if adaptive_ref_pic_marking_mode_flag {
            loop {
                let memory_management_control_operation =
                    bits.read_ue("memory_management_control_operation")?;
                if memory_management_control_operation == 0 {
                    break;
                }
                let mut difference_of_pic_nums_minus1 = None;
                let mut long_term_pic_num = None;
                let mut long_term_frame_idx = None;
                let mut max_long_term_frame_idx_plus1 = None;
                match memory_management_control_operation {
                    1 => {
                        difference_of_pic_nums_minus1 =
                            Some(bits.read_ue("difference_of_pic_nums_minus1")?);
                    }
                    2 => {
                        long_term_pic_num = Some(bits.read_ue("long_term_pic_num")?);
                    }
                    3 => {
                        difference_of_pic_nums_minus1 =
                            Some(bits.read_ue("difference_of_pic_nums_minus1")?);
                        long_term_frame_idx = Some(bits.read_ue("long_term_frame_idx")?);
                    }
                    4 => {
                        max_long_term_frame_idx_plus1 =
                            Some(bits.read_ue("max_long_term_frame_idx_plus1")?);
                    }
                    5 => {}
                    6 => {
                        long_term_frame_idx = Some(bits.read_ue("long_term_frame_idx")?);
                    }
                    other => {
                        return Err(format!(
                            "H.264 memory_management_control_operation {other} is not supported"
                        ));
                    }
                }
                memory_management_control_operations.push(
                    NativeVulkanH264MemoryManagementControlOperationSnapshot {
                        memory_management_control_operation,
                        difference_of_pic_nums_minus1,
                        long_term_pic_num,
                        long_term_frame_idx,
                        max_long_term_frame_idx_plus1,
                    },
                );
            }
        }
    }

    Ok(NativeVulkanH264FirstFrameDecodeInfo {
        nal_type: slice.nal_type,
        nal_type_label: native_vulkan_h264_nal_type_label(slice.nal_type),
        nal_ref_idc: slice.nal_ref_idc,
        first_mb_in_slice,
        first_slice_segment_in_pic_flag: first_mb_in_slice == 0,
        slice_type,
        slice_type_normalized: normalized_slice_type,
        pps_id,
        frame_num,
        idr_pic_id,
        num_ref_idx_l0_active_minus1,
        num_ref_idx_l1_active_minus1,
        ref_pic_list_modification_l0,
        ref_pic_list_modifications_l0,
        ref_pic_list_modification_l1,
        ref_pic_list_modifications_l1,
        adaptive_ref_pic_marking_mode_flag,
        memory_management_control_operations,
        field_pic_flag,
        bottom_field_flag,
        is_reference: slice.nal_ref_idc != 0,
        is_intra,
        is_p,
        is_b,
        long_term_reference_flag,
        pic_order_cnt,
        slice_offsets: NativeVulkanH264SliceOffsets::new(),
        idr,
        irap: idr,
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
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

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeVulkanH265ParsedDecPicBufMgr {
    sub_layer_ordering_info_present_flag: bool,
    max_latency_increase_plus1: [u32; 7],
    max_dec_pic_buffering_minus1: [u8; 7],
    max_num_reorder_pics: [u8; 7],
}

#[cfg(any(feature = "native-vulkan-video", test))]
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
    short_term_ref_pic_sets: Vec<NativeVulkanH265ShortTermRefPicSetSnapshot>,
    long_term_ref_pics_present_flag: bool,
    long_term_ref_pics_sps: Vec<NativeVulkanH265LongTermRefPicSpsSnapshot>,
    temporal_mvp_enabled_flag: bool,
    strong_intra_smoothing_enabled_flag: bool,
    vui_parameters_present_flag: bool,
    vui: Option<NativeVulkanH265ParsedVui>,
    sps_extension_present_flag: bool,
}

#[cfg(any(feature = "native-vulkan-video", test))]
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

#[cfg(any(feature = "native-vulkan-video", test))]
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

#[cfg(any(feature = "native-vulkan-video", test))]
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

#[cfg(any(feature = "native-vulkan-video", test))]
impl NativeVulkanH265ParsedProfileTierLevel {
    fn main_compatible(&self) -> bool {
        self.profile_idc == 1 || self.profile_compatibility_flags[1]
    }

    fn main10_compatible(&self) -> bool {
        self.profile_idc == 2 || self.profile_compatibility_flags[2]
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
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

#[cfg(any(feature = "native-vulkan-video", test))]
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

#[cfg(any(feature = "native-vulkan-video", test))]
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
    let mut short_term_ref_pic_sets = Vec::with_capacity(num_short_term_ref_pic_sets as usize);
    for st_rps_idx in 0..num_short_term_ref_pic_sets {
        let short_term_ref_pic_set = native_vulkan_h265_read_short_term_ref_pic_set(
            &mut bits,
            st_rps_idx,
            num_short_term_ref_pic_sets,
            &short_term_ref_pic_sets,
        )?;
        short_term_ref_pic_sets.push(short_term_ref_pic_set);
    }
    let long_term_ref_pics_present_flag = bits.read_bool("long_term_ref_pics_present_flag")?;
    let mut long_term_ref_pics_sps = Vec::new();
    if long_term_ref_pics_present_flag {
        let num_long_term_ref_pics_sps = bits.read_ue("num_long_term_ref_pics_sps")?;
        if num_long_term_ref_pics_sps > 32 {
            return Err(format!(
                "H.265 SPS has {num_long_term_ref_pics_sps} long-term refs; maximum supported is 32"
            ));
        }
        long_term_ref_pics_sps.reserve(num_long_term_ref_pics_sps as usize);
        for _ in 0..num_long_term_ref_pics_sps {
            let lt_ref_pic_poc_lsb_sps = bits.read_bits(
                log2_max_pic_order_cnt_lsb_minus4 + 4,
                "lt_ref_pic_poc_lsb_sps",
            )?;
            let used_by_curr_pic_lt_sps_flag = bits.read_bool("used_by_curr_pic_lt_sps_flag")?;
            long_term_ref_pics_sps.push(NativeVulkanH265LongTermRefPicSpsSnapshot {
                lt_ref_pic_poc_lsb_sps,
                used_by_curr_pic_lt_sps_flag,
            });
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
        short_term_ref_pic_sets,
        long_term_ref_pics_present_flag,
        long_term_ref_pics_sps,
        temporal_mvp_enabled_flag,
        strong_intra_smoothing_enabled_flag,
        vui_parameters_present_flag,
        vui,
        sps_extension_present_flag,
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
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

#[cfg(any(feature = "native-vulkan-video", test))]
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

#[cfg(any(feature = "native-vulkan-video", test))]
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

#[cfg(any(feature = "native-vulkan-video", test))]
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

#[cfg(any(feature = "native-vulkan-video", test))]
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

#[cfg(any(feature = "native-vulkan-video", test))]
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

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h265_read_short_term_ref_pic_set(
    bits: &mut NativeVulkanH265BitReader<'_>,
    st_rps_idx: u32,
    num_short_term_ref_pic_sets: u32,
    previous_ref_pic_sets: &[NativeVulkanH265ShortTermRefPicSetSnapshot],
) -> Result<NativeVulkanH265ShortTermRefPicSetSnapshot, String> {
    let inter_ref_pic_set_prediction_flag =
        st_rps_idx != 0 && bits.read_bool("inter_ref_pic_set_prediction_flag")?;
    if inter_ref_pic_set_prediction_flag {
        let delta_idx_minus1 = if st_rps_idx == num_short_term_ref_pic_sets {
            bits.read_ue("delta_idx_minus1")?
        } else {
            0
        };
        let ref_rps_idx = st_rps_idx
            .checked_sub(delta_idx_minus1.saturating_add(1))
            .ok_or_else(|| {
                format!(
                    "H.265 predicted RPS delta_idx_minus1 {delta_idx_minus1} underflows stRpsIdx {st_rps_idx}"
                )
            })?;
        let ref_pic_set = previous_ref_pic_sets
            .get(ref_rps_idx as usize)
            .ok_or_else(|| {
                format!(
                    "H.265 predicted RPS RefRpsIdx {ref_rps_idx} exceeds previous RPS count {}",
                    previous_ref_pic_sets.len()
                )
            })?;
        let ref_num_delta_pocs = ref_pic_set
            .num_negative_pics
            .checked_add(ref_pic_set.num_positive_pics)
            .ok_or_else(|| "H.265 predicted RPS reference delta POC count overflow".to_owned())?;
        let delta_rps_sign = bits.read_bool("delta_rps_sign")?;
        let abs_delta_rps_minus1 = bits.read_ue("abs_delta_rps_minus1")?;
        let delta_rps_magnitude = i32::try_from(abs_delta_rps_minus1.saturating_add(1))
            .map_err(|_| "H.265 predicted RPS abs_delta_rps_minus1 exceeds i32 range".to_owned())?;
        let delta_rps = if delta_rps_sign {
            -delta_rps_magnitude
        } else {
            delta_rps_magnitude
        };
        let flag_count = ref_num_delta_pocs.saturating_add(1);
        if flag_count > 16 {
            return Err(format!(
                "H.265 predicted RPS has {flag_count} use-delta flags; maximum supported is 16"
            ));
        }
        let mut used_by_current_flags = Vec::with_capacity(flag_count as usize);
        let mut use_delta_flags = Vec::with_capacity(flag_count as usize);
        for flag_index in 0..flag_count {
            let used_by_current = bits.read_bool("used_by_curr_pic_flag")?;
            let use_delta = if used_by_current {
                true
            } else {
                bits.read_bool("use_delta_flag")?
            };
            used_by_current_flags.push(used_by_current);
            use_delta_flags.push(use_delta);
            if flag_index == ref_num_delta_pocs && !use_delta {
                continue;
            }
        }

        let mut negative_entries = Vec::<(i32, bool)>::new();
        let mut positive_entries = Vec::<(i32, bool)>::new();
        let ref_negative_count = ref_pic_set.negative_delta_pocs.len();
        let ref_positive_count = ref_pic_set.positive_delta_pocs.len();
        for index in (0..ref_positive_count).rev() {
            let flag_index = ref_negative_count + index;
            let delta_poc = ref_pic_set.positive_delta_pocs[index]
                .checked_add(delta_rps)
                .ok_or_else(|| "H.265 predicted positive RPS delta overflow".to_owned())?;
            if delta_poc < 0 && use_delta_flags.get(flag_index).copied().unwrap_or(false) {
                negative_entries.push((
                    delta_poc,
                    used_by_current_flags
                        .get(flag_index)
                        .copied()
                        .unwrap_or(false),
                ));
            }
        }
        let delta_rps_flag_index = ref_num_delta_pocs as usize;
        if delta_rps < 0
            && use_delta_flags
                .get(delta_rps_flag_index)
                .copied()
                .unwrap_or(false)
        {
            negative_entries.push((
                delta_rps,
                used_by_current_flags
                    .get(delta_rps_flag_index)
                    .copied()
                    .unwrap_or(false),
            ));
        }
        for index in 0..ref_negative_count {
            let delta_poc = ref_pic_set.negative_delta_pocs[index]
                .checked_add(delta_rps)
                .ok_or_else(|| "H.265 predicted negative RPS delta overflow".to_owned())?;
            if delta_poc < 0 && use_delta_flags.get(index).copied().unwrap_or(false) {
                negative_entries.push((
                    delta_poc,
                    used_by_current_flags.get(index).copied().unwrap_or(false),
                ));
            }
        }

        for index in (0..ref_negative_count).rev() {
            let delta_poc = ref_pic_set.negative_delta_pocs[index]
                .checked_add(delta_rps)
                .ok_or_else(|| "H.265 predicted negative RPS delta overflow".to_owned())?;
            if delta_poc > 0 && use_delta_flags.get(index).copied().unwrap_or(false) {
                positive_entries.push((
                    delta_poc,
                    used_by_current_flags.get(index).copied().unwrap_or(false),
                ));
            }
        }
        if delta_rps > 0
            && use_delta_flags
                .get(delta_rps_flag_index)
                .copied()
                .unwrap_or(false)
        {
            positive_entries.push((
                delta_rps,
                used_by_current_flags
                    .get(delta_rps_flag_index)
                    .copied()
                    .unwrap_or(false),
            ));
        }
        for index in 0..ref_positive_count {
            let flag_index = ref_negative_count + index;
            let delta_poc = ref_pic_set.positive_delta_pocs[index]
                .checked_add(delta_rps)
                .ok_or_else(|| "H.265 predicted positive RPS delta overflow".to_owned())?;
            if delta_poc > 0 && use_delta_flags.get(flag_index).copied().unwrap_or(false) {
                positive_entries.push((
                    delta_poc,
                    used_by_current_flags
                        .get(flag_index)
                        .copied()
                        .unwrap_or(false),
                ));
            }
        }

        let negative_delta_pocs = negative_entries
            .iter()
            .map(|(delta_poc, _)| *delta_poc)
            .collect::<Vec<_>>();
        let negative_used_by_curr_pic = negative_entries
            .iter()
            .map(|(_, used)| *used)
            .collect::<Vec<_>>();
        let positive_delta_pocs = positive_entries
            .iter()
            .map(|(delta_poc, _)| *delta_poc)
            .collect::<Vec<_>>();
        let positive_used_by_curr_pic = positive_entries
            .iter()
            .map(|(_, used)| *used)
            .collect::<Vec<_>>();
        return Ok(native_vulkan_h265_short_term_ref_pic_set_snapshot(
            true,
            Some(delta_idx_minus1),
            Some(delta_rps_sign),
            Some(abs_delta_rps_minus1),
            ref_num_delta_pocs,
            use_delta_flags,
            used_by_current_flags,
            negative_delta_pocs,
            negative_used_by_curr_pic,
            positive_delta_pocs,
            positive_used_by_curr_pic,
        ));
    }

    let num_negative_pics = bits.read_ue("num_negative_pics")?;
    let num_positive_pics = bits.read_ue("num_positive_pics")?;
    let mut negative_delta_pocs = Vec::with_capacity(num_negative_pics as usize);
    let mut negative_used_by_curr_pic = Vec::with_capacity(num_negative_pics as usize);
    let mut previous_delta_poc = 0i32;
    for _ in 0..num_negative_pics {
        let delta_poc_s0_minus1 = bits.read_ue("delta_poc_s0_minus1")?;
        let delta_poc = previous_delta_poc
            .checked_sub(
                i32::try_from(delta_poc_s0_minus1)
                    .map_err(|_| "delta_poc_s0_minus1 exceeds i32 range".to_owned())?
                    + 1,
            )
            .ok_or_else(|| "negative short-term delta POC underflow".to_owned())?;
        previous_delta_poc = delta_poc;
        negative_delta_pocs.push(delta_poc);
        negative_used_by_curr_pic.push(bits.read_bool("used_by_curr_pic_s0_flag")?);
    }

    let mut positive_delta_pocs = Vec::with_capacity(num_positive_pics as usize);
    let mut positive_used_by_curr_pic = Vec::with_capacity(num_positive_pics as usize);
    let mut previous_delta_poc = 0i32;
    for _ in 0..num_positive_pics {
        let delta_poc_s1_minus1 = bits.read_ue("delta_poc_s1_minus1")?;
        let delta_poc = previous_delta_poc
            .checked_add(
                i32::try_from(delta_poc_s1_minus1)
                    .map_err(|_| "delta_poc_s1_minus1 exceeds i32 range".to_owned())?
                    + 1,
            )
            .ok_or_else(|| "positive short-term delta POC overflow".to_owned())?;
        previous_delta_poc = delta_poc;
        positive_delta_pocs.push(delta_poc);
        positive_used_by_curr_pic.push(bits.read_bool("used_by_curr_pic_s1_flag")?);
    }
    Ok(native_vulkan_h265_short_term_ref_pic_set_snapshot(
        false,
        None,
        None,
        None,
        0,
        Vec::new(),
        Vec::new(),
        negative_delta_pocs,
        negative_used_by_curr_pic,
        positive_delta_pocs,
        positive_used_by_curr_pic,
    ))
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[allow(clippy::too_many_arguments)]
fn native_vulkan_h265_short_term_ref_pic_set_snapshot(
    inter_ref_pic_set_prediction_flag: bool,
    delta_idx_minus1: Option<u32>,
    delta_rps_sign: Option<bool>,
    abs_delta_rps_minus1: Option<u32>,
    num_delta_pocs_of_ref_rps_idx: u32,
    use_delta_flags: Vec<bool>,
    used_by_current_flags: Vec<bool>,
    negative_delta_pocs: Vec<i32>,
    negative_used_by_curr_pic: Vec<bool>,
    positive_delta_pocs: Vec<i32>,
    positive_used_by_curr_pic: Vec<bool>,
) -> NativeVulkanH265ShortTermRefPicSetSnapshot {
    let used_by_current_count = negative_used_by_curr_pic
        .iter()
        .chain(positive_used_by_curr_pic.iter())
        .filter(|used| **used)
        .count() as u32;
    let used_negative_delta_pocs = negative_delta_pocs
        .iter()
        .copied()
        .zip(negative_used_by_curr_pic.iter().copied())
        .filter_map(|(delta_poc, used)| used.then_some(delta_poc))
        .collect::<Vec<_>>();
    let used_positive_delta_pocs = positive_delta_pocs
        .iter()
        .copied()
        .zip(positive_used_by_curr_pic.iter().copied())
        .filter_map(|(delta_poc, used)| used.then_some(delta_poc))
        .collect::<Vec<_>>();
    NativeVulkanH265ShortTermRefPicSetSnapshot {
        inter_ref_pic_set_prediction_flag,
        delta_idx_minus1,
        delta_rps_sign,
        abs_delta_rps_minus1,
        num_delta_pocs_of_ref_rps_idx,
        use_delta_flags,
        used_by_current_flags,
        num_negative_pics: negative_delta_pocs.len() as u32,
        num_positive_pics: positive_delta_pocs.len() as u32,
        negative_delta_pocs,
        negative_used_by_curr_pic,
        used_negative_delta_pocs,
        positive_delta_pocs,
        positive_used_by_curr_pic,
        used_positive_delta_pocs,
        used_by_current_count,
    }
}

pub(super) fn native_vulkan_h265_u8(value: u32, label: &'static str) -> Result<u8, String> {
    u8::try_from(value).map_err(|_| format!("{label}={value} exceeds u8 range"))
}

pub(super) fn native_vulkan_h265_i8(value: i32, label: &'static str) -> Result<i8, String> {
    i8::try_from(value).map_err(|_| format!("{label}={value} exceeds i8 range"))
}

pub(super) fn native_vulkan_h265_u16(value: u32, label: &'static str) -> Result<u16, String> {
    u16::try_from(value).map_err(|_| format!("{label}={value} exceeds u16 range"))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h265_rbsp(payload: &[u8]) -> Result<Cow<'_, [u8]>, String> {
    if payload.len() < 2 {
        return Err("H.265 NAL payload is too short".to_owned());
    }
    Ok(native_vulkan_rbsp_unescape(payload))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h265_slice_header_rbsp(payload: &[u8]) -> Result<Cow<'_, [u8]>, String> {
    const H265_SLICE_HEADER_PROBE_BYTES: usize = 4096;
    if payload.len() < 2 {
        return Err("H.265 NAL payload is too short".to_owned());
    }
    let probe_len = payload.len().min(H265_SLICE_HEADER_PROBE_BYTES);
    Ok(native_vulkan_rbsp_unescape(&payload[..probe_len]))
}

#[cfg(any(feature = "native-vulkan-video", test))]
struct NativeVulkanH265BitReader<'a> {
    bytes: &'a [u8],
    bit_offset: usize,
}

#[cfg(any(feature = "native-vulkan-video", test))]
impl<'a> NativeVulkanH265BitReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            bit_offset: 0,
        }
    }

    fn bit_offset(&self) -> usize {
        self.bit_offset
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
        native_vulkan_read_bits_be(
            self.bytes,
            &mut self.bit_offset,
            count,
            label,
            "H.265 RBSP length",
        )
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

#[cfg(any(feature = "native-vulkan-video", test))]
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

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h265_chroma_format_label(chroma_format_idc: u32) -> &'static str {
    match chroma_format_idc {
        0 => "monochrome",
        1 => "4:2:0",
        2 => "4:2:2",
        3 => "4:4:4",
        _ => "unknown",
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
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

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct NativeVulkanH265NalStats {
    bytes: u64,
    has_annex_b_start_codes: bool,
    vps_count: u32,
    sps_count: u32,
    pps_count: u32,
    idr_count: u32,
    slice_count: u32,
    first_slice: Option<NativeVulkanH265SlicePayloadSummary>,
}

#[cfg(any(feature = "native-vulkan-video", test))]
impl NativeVulkanH265NalStats {
    fn parameter_sets_present(&self) -> bool {
        self.vps_count > 0 && self.sps_count > 0 && self.pps_count > 0
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeVulkanH265SlicePayloadSummary {
    nal_type: u8,
    slice_segment_offset: u32,
    payload_start: usize,
    payload_end: usize,
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct NativeVulkanH264NalStats {
    bytes: u64,
    has_annex_b_start_codes: bool,
    sps_count: u32,
    pps_count: u32,
    idr_count: u32,
    slice_count: u32,
    first_slice: Option<NativeVulkanH264SlicePayloadSummary>,
    slice_offsets: NativeVulkanH264SliceOffsets,
}

#[cfg(any(feature = "native-vulkan-video", test))]
impl NativeVulkanH264NalStats {
    fn parameter_sets_present(&self) -> bool {
        self.sps_count > 0 && self.pps_count > 0
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeVulkanH264SlicePayloadSummary {
    nal_type: u8,
    nal_ref_idc: u8,
    payload_start: usize,
    payload_end: usize,
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct NativeVulkanAv1ObuStats {
    bytes: u64,
    obu_count: u32,
    sequence_header_count: u32,
    temporal_delimiter_count: u32,
    frame_header_count: u32,
    tile_group_count: u32,
    frame_count: u32,
    tile_payload_bytes: u64,
    frame_payload_bytes: u64,
    first_frame_header_obu_offset: Option<u64>,
    first_tile_group_obu_offset: Option<u64>,
    sequence_header: Option<NativeVulkanAv1SequenceHeaderSnapshot>,
    first_frame_submit: Option<NativeVulkanAv1FrameSubmitSnapshot>,
    obus: Vec<NativeVulkanAv1ObuSnapshot>,
}

#[cfg(any(feature = "native-vulkan-video", test))]
impl NativeVulkanAv1ObuStats {
    fn sequence_header_present(&self) -> bool {
        self.sequence_header_count > 0
    }

    fn decode_candidate(&self) -> bool {
        self.sequence_header_present()
            && (self.frame_count > 0 || (self.frame_header_count > 0 && self.tile_group_count > 0))
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_obu_stats(bytes: &[u8]) -> Result<NativeVulkanAv1ObuStats, String> {
    let mut stats = NativeVulkanAv1ObuStats {
        bytes: bytes.len() as u64,
        ..Default::default()
    };
    let mut offset = 0usize;
    while offset < bytes.len() {
        let header_offset = offset;
        let header = bytes[offset];
        if header & 0x80 != 0 {
            return Err(format!(
                "AV1 OBU forbidden bit set at byte offset {header_offset}"
            ));
        }
        let obu_type = (header >> 3) & 0x0f;
        let has_extension = header & 0x04 != 0;
        let has_size_field = header & 0x02 != 0;
        if header & 0x01 != 0 {
            return Err(format!(
                "AV1 OBU reserved bit set at byte offset {header_offset}"
            ));
        }
        offset += 1;
        if has_extension {
            if offset >= bytes.len() {
                return Err("AV1 OBU extension flag set without extension byte".to_owned());
            }
            offset += 1;
        }
        if !has_size_field {
            return Err(format!(
                "AV1 OBU at byte offset {header_offset} has no size field; annexb AV1 extraction is not supported yet"
            ));
        }
        let (payload_size, leb_size) = native_vulkan_av1_read_leb128(&bytes[offset..])?;
        offset = offset
            .checked_add(leb_size)
            .ok_or_else(|| "AV1 OBU offset overflow after LEB128".to_owned())?;
        let payload_offset = offset;
        let payload_size_usize = usize::try_from(payload_size)
            .map_err(|_| format!("AV1 OBU payload size {payload_size} exceeds usize"))?;
        let payload_end = payload_offset
            .checked_add(payload_size_usize)
            .ok_or_else(|| "AV1 OBU payload end overflow".to_owned())?;
        if payload_end > bytes.len() {
            return Err(format!(
                "AV1 OBU payload at byte offset {payload_offset} extends past sample end"
            ));
        }

        stats.obu_count = stats.obu_count.saturating_add(1);
        match obu_type {
            1 => {
                stats.sequence_header_count = stats.sequence_header_count.saturating_add(1);
                if stats.sequence_header.is_none() {
                    stats.sequence_header = Some(native_vulkan_parse_av1_sequence_header(
                        &bytes[payload_offset..payload_end],
                    )?);
                }
            }
            2 => stats.temporal_delimiter_count = stats.temporal_delimiter_count.saturating_add(1),
            3 => {
                stats.frame_header_count = stats.frame_header_count.saturating_add(1);
                stats
                    .first_frame_header_obu_offset
                    .get_or_insert(header_offset as u64);
            }
            4 => {
                stats.tile_group_count = stats.tile_group_count.saturating_add(1);
                stats.tile_payload_bytes = stats.tile_payload_bytes.saturating_add(payload_size);
                stats
                    .first_tile_group_obu_offset
                    .get_or_insert(header_offset as u64);
            }
            6 => {
                stats.frame_count = stats.frame_count.saturating_add(1);
                stats.frame_payload_bytes = stats.frame_payload_bytes.saturating_add(payload_size);
                stats
                    .first_frame_header_obu_offset
                    .get_or_insert(header_offset as u64);
            }
            _ => {}
        }
        if stats.obus.len() < 32 {
            stats.obus.push(NativeVulkanAv1ObuSnapshot {
                offset: header_offset as u64,
                header_size: (payload_offset - header_offset) as u64,
                payload_offset: payload_offset as u64,
                payload_size,
                obu_type,
                obu_type_label: native_vulkan_av1_obu_type_label(obu_type),
                has_extension,
                has_size_field,
            });
        }
        offset = payload_end;
    }
    if let Some(sequence_header) = stats.sequence_header.as_ref() {
        stats.first_frame_submit =
            native_vulkan_av1_first_frame_submit_snapshot(bytes, &stats.obus, sequence_header);
    }
    Ok(stats)
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeVulkanAv1ObuRange {
    offset: usize,
    end: usize,
    obu_type: u8,
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_obu_ranges(bytes: &[u8]) -> Result<Vec<NativeVulkanAv1ObuRange>, String> {
    let mut ranges = Vec::new();
    let mut offset = 0usize;
    while offset < bytes.len() {
        let header_offset = offset;
        let header = bytes[offset];
        if header & 0x80 != 0 {
            return Err(format!(
                "AV1 OBU forbidden bit set at byte offset {header_offset}"
            ));
        }
        let obu_type = (header >> 3) & 0x0f;
        let has_extension = header & 0x04 != 0;
        let has_size_field = header & 0x02 != 0;
        if header & 0x01 != 0 {
            return Err(format!(
                "AV1 OBU reserved bit set at byte offset {header_offset}"
            ));
        }
        offset += 1;
        if has_extension {
            if offset >= bytes.len() {
                return Err("AV1 OBU extension flag set without extension byte".to_owned());
            }
            offset += 1;
        }
        if !has_size_field {
            return Err(format!(
                "AV1 OBU at byte offset {header_offset} has no size field; annexb AV1 extraction is not supported yet"
            ));
        }
        let (payload_size, leb_size) = native_vulkan_av1_read_leb128(&bytes[offset..])?;
        offset = offset
            .checked_add(leb_size)
            .ok_or_else(|| "AV1 OBU offset overflow after LEB128".to_owned())?;
        let payload_size_usize = usize::try_from(payload_size)
            .map_err(|_| format!("AV1 OBU payload size {payload_size} exceeds usize"))?;
        let payload_end = offset
            .checked_add(payload_size_usize)
            .ok_or_else(|| "AV1 OBU payload end overflow".to_owned())?;
        if payload_end > bytes.len() {
            return Err(format!(
                "AV1 OBU payload at byte offset {offset} extends past sample end"
            ));
        }
        ranges.push(NativeVulkanAv1ObuRange {
            offset: header_offset,
            end: payload_end,
            obu_type,
        });
        offset = payload_end;
    }
    Ok(ranges)
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_split_ffmpeg_packet_frame_ranges(
    bytes: &[u8],
) -> Result<Vec<std::ops::Range<usize>>, String> {
    let ranges = native_vulkan_av1_obu_ranges(bytes)?;
    let mut units = Vec::<std::ops::Range<usize>>::new();
    let mut pending_prefix = None::<std::ops::Range<usize>>;
    let mut current_frame = None::<std::ops::Range<usize>>;

    for range in ranges {
        match range.obu_type {
            1 | 2 => {
                if let Some(unit) = current_frame.take() {
                    units.push(unit);
                }
                native_vulkan_av1_extend_range(&mut pending_prefix, range.offset, range.end);
            }
            3 => {
                if let Some(unit) = current_frame.take() {
                    units.push(unit);
                }
                current_frame = Some(native_vulkan_av1_take_prefixed_range(
                    &mut pending_prefix,
                    range.offset,
                    range.end,
                ));
            }
            4 => {
                if let Some(unit) = current_frame.as_mut() {
                    unit.end = range.end;
                } else {
                    current_frame = Some(native_vulkan_av1_take_prefixed_range(
                        &mut pending_prefix,
                        range.offset,
                        range.end,
                    ));
                }
            }
            6 => {
                if let Some(unit) = current_frame.take() {
                    units.push(unit);
                }
                units.push(native_vulkan_av1_take_prefixed_range(
                    &mut pending_prefix,
                    range.offset,
                    range.end,
                ));
            }
            _ => {
                if let Some(unit) = current_frame.as_mut() {
                    unit.end = range.end;
                } else {
                    native_vulkan_av1_extend_range(&mut pending_prefix, range.offset, range.end);
                }
            }
        }
    }

    if let Some(unit) = current_frame.take() {
        units.push(unit);
    }
    if units.is_empty() {
        if let Some(prefix) = pending_prefix.take() {
            units.push(prefix);
        }
    }
    Ok(units)
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_extend_range(
    range: &mut Option<std::ops::Range<usize>>,
    offset: usize,
    end: usize,
) {
    match range {
        Some(existing) => existing.end = end,
        None => *range = Some(offset..end),
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_take_prefixed_range(
    prefix: &mut Option<std::ops::Range<usize>>,
    offset: usize,
    end: usize,
) -> std::ops::Range<usize> {
    let start = prefix.take().map(|prefix| prefix.start).unwrap_or(offset);
    start..end
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_first_frame_submit_snapshot(
    bytes: &[u8],
    obus: &[NativeVulkanAv1ObuSnapshot],
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
) -> Option<NativeVulkanAv1FrameSubmitSnapshot> {
    let frame_obu = obus.iter().find(|obu| obu.obu_type == 6);
    if let Some(frame_obu) = frame_obu {
        return Some(native_vulkan_av1_frame_submit_from_frame_obu(
            bytes,
            frame_obu,
            sequence_header,
        ));
    }

    let frame_header_obu = obus.iter().find(|obu| obu.obu_type == 3)?;
    let tile_group_obu = obus.iter().find(|obu| obu.obu_type == 4);
    Some(native_vulkan_av1_frame_submit_from_split_obus(
        bytes,
        frame_header_obu,
        tile_group_obu,
        sequence_header,
    ))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_frame_submit_from_frame_obu(
    bytes: &[u8],
    frame_obu: &NativeVulkanAv1ObuSnapshot,
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
) -> NativeVulkanAv1FrameSubmitSnapshot {
    let payload_offset = frame_obu.payload_offset as usize;
    let payload_end = payload_offset.saturating_add(frame_obu.payload_size as usize);
    let payload = bytes.get(payload_offset..payload_end).unwrap_or_default();
    match native_vulkan_parse_av1_frame_header_for_submit(payload, sequence_header) {
        Ok(header) => {
            let tile_payload_offset = header.frame_header_bytes;
            let tile_payload = payload.get(tile_payload_offset..).unwrap_or_default();
            match native_vulkan_av1_tile_group_offsets_from_payload(
                frame_obu.payload_offset,
                tile_payload_offset,
                tile_payload,
                &header,
            ) {
                Ok((tile_offsets, tile_sizes)) => {
                    native_vulkan_av1_frame_submit_snapshot_from_header(
                        frame_obu,
                        frame_obu.payload_offset,
                        frame_obu.payload_size,
                        header,
                        tile_offsets,
                        tile_sizes,
                        !tile_payload.is_empty(),
                    )
                }
                Err(reason) => {
                    let mut snapshot = native_vulkan_av1_frame_submit_snapshot_from_header(
                        frame_obu,
                        frame_obu.payload_offset,
                        frame_obu.payload_size,
                        header,
                        Vec::new(),
                        Vec::new(),
                        false,
                    );
                    if snapshot.unsupported_reason.is_none() {
                        snapshot.unsupported_reason = Some(format!(
                            "AV1 frame OBU tile table is not submit-ready: {reason}"
                        ));
                    }
                    snapshot.vulkan_submit_candidate = false;
                    snapshot
                }
            }
        }
        Err(reason) => native_vulkan_av1_unsupported_frame_submit_snapshot(
            frame_obu,
            frame_obu.payload_offset,
            frame_obu.payload_size,
            format!("AV1 frame OBU header is not submit-ready: {reason}"),
        ),
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_frame_submit_from_split_obus(
    bytes: &[u8],
    frame_header_obu: &NativeVulkanAv1ObuSnapshot,
    tile_group_obu: Option<&NativeVulkanAv1ObuSnapshot>,
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
) -> NativeVulkanAv1FrameSubmitSnapshot {
    let payload_offset = frame_header_obu.payload_offset as usize;
    let payload_end = payload_offset.saturating_add(frame_header_obu.payload_size as usize);
    let payload = bytes.get(payload_offset..payload_end).unwrap_or_default();
    match native_vulkan_parse_av1_frame_header_for_submit(payload, sequence_header) {
        Ok(header) => {
            let Some(tile_group_obu) = tile_group_obu else {
                let mut snapshot = native_vulkan_av1_frame_submit_snapshot_from_header(
                    frame_header_obu,
                    frame_header_obu.payload_offset,
                    frame_header_obu.payload_size,
                    header,
                    Vec::new(),
                    Vec::new(),
                    false,
                );
                if !snapshot.show_existing_frame {
                    snapshot.unsupported_reason =
                        Some("AV1 frame-header OBU has no following tile-group OBU".to_owned());
                }
                snapshot.vulkan_submit_candidate = false;
                return snapshot;
            };
            let tile_payload_offset = tile_group_obu.payload_offset as usize;
            let tile_payload_end =
                tile_payload_offset.saturating_add(tile_group_obu.payload_size as usize);
            let tile_payload = bytes
                .get(tile_payload_offset..tile_payload_end)
                .unwrap_or_default();
            match native_vulkan_av1_tile_group_offsets_from_payload(
                tile_group_obu.payload_offset,
                0,
                tile_payload,
                &header,
            ) {
                Ok((tile_offsets, tile_sizes)) => {
                    native_vulkan_av1_frame_submit_snapshot_from_header(
                        frame_header_obu,
                        frame_header_obu.payload_offset,
                        frame_header_obu.payload_size,
                        header,
                        tile_offsets,
                        tile_sizes,
                        !tile_payload.is_empty(),
                    )
                }
                Err(reason) => {
                    let mut snapshot = native_vulkan_av1_frame_submit_snapshot_from_header(
                        frame_header_obu,
                        frame_header_obu.payload_offset,
                        frame_header_obu.payload_size,
                        header,
                        Vec::new(),
                        Vec::new(),
                        false,
                    );
                    if snapshot.unsupported_reason.is_none() {
                        snapshot.unsupported_reason = Some(format!(
                            "AV1 tile-group OBU table is not submit-ready: {reason}"
                        ));
                    }
                    snapshot.vulkan_submit_candidate = false;
                    snapshot
                }
            }
        }
        Err(reason) => native_vulkan_av1_unsupported_frame_submit_snapshot(
            frame_header_obu,
            frame_header_obu.payload_offset,
            frame_header_obu.payload_size,
            format!("AV1 frame-header OBU is not submit-ready: {reason}"),
        ),
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_frame_submit_snapshot_from_header(
    frame_header_obu: &NativeVulkanAv1ObuSnapshot,
    frame_header_payload_offset: u64,
    frame_obu_payload_bytes: u64,
    header: NativeVulkanAv1ParsedFrameHeader,
    tile_offsets: Vec<u32>,
    tile_sizes: Vec<u32>,
    found_tile_payload: bool,
) -> NativeVulkanAv1FrameSubmitSnapshot {
    let found_frame_header = true;
    let tile_payload_total_bytes = tile_sizes.iter().map(|size| u64::from(*size)).sum::<u64>();
    let unsupported_reason = if header.unsupported_reason.is_some() {
        header.unsupported_reason.clone()
    } else if !found_tile_payload {
        Some("AV1 first frame has no tile payload bytes".to_owned())
    } else if header.tile_count != tile_offsets.len() as u32
        || tile_offsets.len() != tile_sizes.len()
    {
        Some(format!(
            "AV1 tile table mismatch: header tile_count={}, offsets={}, sizes={}",
            header.tile_count,
            tile_offsets.len(),
            tile_sizes.len()
        ))
    } else {
        None
    };
    let vulkan_submit_candidate = unsupported_reason.is_none()
        && found_frame_header
        && found_tile_payload
        && header.tile_count > 0
        && !header.show_existing_frame;

    NativeVulkanAv1FrameSubmitSnapshot {
        parser: "native-rust-av1-first-frame-submit",
        frame_header_obu_offset: frame_header_obu.offset,
        frame_header_payload_offset,
        frame_header_payload_size: frame_header_obu.payload_size,
        frame_header_offset_for_vulkan: u32::try_from(frame_header_obu.offset).unwrap_or(0),
        tile_count: header.tile_count,
        tile_columns: header.tile_columns,
        tile_rows: header.tile_rows,
        tile_size_bytes: header.tile_size_bytes,
        tile_offsets,
        tile_sizes,
        tile_payload_total_bytes,
        frame_obu_payload_bytes,
        frame_type: header.frame_type,
        frame_type_label: native_vulkan_av1_frame_type_label(header.frame_type),
        show_existing_frame: header.show_existing_frame,
        frame_to_show_map_idx: header.frame_to_show_map_idx,
        display_frame_id: header.display_frame_id,
        current_frame_id: header.current_frame_id,
        expected_frame_ids: header.expected_frame_ids,
        show_frame: header.show_frame,
        showable_frame: header.showable_frame,
        error_resilient_mode: header.error_resilient_mode,
        disable_cdf_update: header.disable_cdf_update,
        allow_screen_content_tools: header.allow_screen_content_tools,
        force_integer_mv: header.force_integer_mv,
        allow_high_precision_mv: header.allow_high_precision_mv,
        interpolation_filter: header.interpolation_filter.0 as u32,
        interpolation_filter_label: native_vulkan_av1_interpolation_filter_label(
            header.interpolation_filter,
        ),
        is_filter_switchable: header.is_filter_switchable,
        is_motion_mode_switchable: header.is_motion_mode_switchable,
        use_ref_frame_mvs: header.use_ref_frame_mvs,
        reference_select: header.reference_select,
        skip_mode_present: header.skip_mode_present,
        allow_warped_motion: header.allow_warped_motion,
        order_hint: header.order_hint,
        primary_ref_frame: header.primary_ref_frame,
        refresh_frame_flags: header.refresh_frame_flags,
        reference_order_hints: header.reference_order_hints,
        frame_refs_short_signaling: header.frame_refs_short_signaling,
        last_frame_idx: header.last_frame_idx,
        gold_frame_idx: header.gold_frame_idx,
        ref_frame_indices: header.ref_frame_indices,
        render_and_frame_size_different: header.render_and_frame_size_different,
        frame_width: header.frame_width,
        frame_height: header.frame_height,
        render_width: header.render_width,
        render_height: header.render_height,
        found_frame_header,
        found_tile_payload,
        vulkan_submit_candidate,
        unsupported_reason,
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_unsupported_frame_submit_snapshot(
    frame_header_obu: &NativeVulkanAv1ObuSnapshot,
    frame_header_payload_offset: u64,
    frame_obu_payload_bytes: u64,
    reason: String,
) -> NativeVulkanAv1FrameSubmitSnapshot {
    NativeVulkanAv1FrameSubmitSnapshot {
        parser: "native-rust-av1-first-frame-submit",
        frame_header_obu_offset: frame_header_obu.offset,
        frame_header_payload_offset,
        frame_header_payload_size: frame_header_obu.payload_size,
        frame_header_offset_for_vulkan: u32::try_from(frame_header_obu.offset).unwrap_or(0),
        tile_count: 0,
        tile_columns: 0,
        tile_rows: 0,
        tile_size_bytes: 0,
        tile_offsets: Vec::new(),
        tile_sizes: Vec::new(),
        tile_payload_total_bytes: 0,
        frame_obu_payload_bytes,
        frame_type: u8::MAX,
        frame_type_label: "unknown",
        show_existing_frame: false,
        frame_to_show_map_idx: None,
        display_frame_id: None,
        current_frame_id: None,
        expected_frame_ids: Vec::new(),
        show_frame: false,
        showable_frame: false,
        error_resilient_mode: false,
        disable_cdf_update: false,
        allow_screen_content_tools: 0,
        force_integer_mv: 0,
        allow_high_precision_mv: false,
        interpolation_filter: vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_EIGHTTAP.0 as u32,
        interpolation_filter_label: "eighttap",
        is_filter_switchable: false,
        is_motion_mode_switchable: false,
        use_ref_frame_mvs: false,
        reference_select: false,
        skip_mode_present: false,
        allow_warped_motion: false,
        order_hint: None,
        primary_ref_frame: None,
        refresh_frame_flags: 0,
        reference_order_hints: Vec::new(),
        frame_refs_short_signaling: false,
        last_frame_idx: None,
        gold_frame_idx: None,
        ref_frame_indices: Vec::new(),
        render_and_frame_size_different: None,
        frame_width: None,
        frame_height: None,
        render_width: None,
        render_height: None,
        found_frame_header: false,
        found_tile_payload: false,
        vulkan_submit_candidate: false,
        unsupported_reason: Some(reason),
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeVulkanAv1ParsedFrameHeader {
    frame_header_bytes: usize,
    tile_count: u32,
    tile_columns: u32,
    tile_rows: u32,
    tile_size_bytes: u32,
    tile_bits: u32,
    tile_info: NativeVulkanAv1ParsedTileInfo,
    frame_type: u8,
    show_existing_frame: bool,
    frame_to_show_map_idx: Option<u8>,
    display_frame_id: Option<u32>,
    current_frame_id: Option<u32>,
    expected_frame_ids: Vec<u32>,
    show_frame: bool,
    showable_frame: bool,
    error_resilient_mode: bool,
    disable_cdf_update: bool,
    disable_frame_end_update_cdf: bool,
    allow_screen_content_tools: u8,
    force_integer_mv: u8,
    allow_high_precision_mv: bool,
    interpolation_filter: vk::video::StdVideoAV1InterpolationFilter,
    is_filter_switchable: bool,
    is_motion_mode_switchable: bool,
    use_ref_frame_mvs: bool,
    reference_select: bool,
    skip_mode_present: bool,
    allow_warped_motion: bool,
    frame_size_override_flag: bool,
    order_hint: Option<u8>,
    primary_ref_frame: Option<u8>,
    refresh_frame_flags: u8,
    reference_order_hints: Vec<u8>,
    frame_refs_short_signaling: bool,
    last_frame_idx: Option<u8>,
    gold_frame_idx: Option<u8>,
    ref_frame_indices: Vec<i8>,
    use_superres: bool,
    coded_denom: u8,
    render_and_frame_size_different: Option<bool>,
    frame_width: Option<u32>,
    frame_height: Option<u32>,
    render_width: Option<u32>,
    render_height: Option<u32>,
    quantization: NativeVulkanAv1ParsedQuantization,
    segmentation: NativeVulkanAv1ParsedSegmentation,
    delta_q: NativeVulkanAv1ParsedDeltaQ,
    delta_lf: NativeVulkanAv1ParsedDeltaLf,
    loop_filter: NativeVulkanAv1ParsedLoopFilter,
    cdef: NativeVulkanAv1ParsedCdef,
    loop_restoration: NativeVulkanAv1ParsedLoopRestoration,
    global_motion: NativeVulkanAv1ParsedGlobalMotion,
    tx_mode_select: bool,
    reduced_tx_set: bool,
    unsupported_reason: Option<String>,
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy)]
struct NativeVulkanAv1ParsedFrameHeaderPrefix {
    frame_type: u8,
    show_existing_frame: bool,
    frame_to_show_map_idx: Option<u8>,
    display_frame_id: Option<u32>,
    current_frame_id: Option<u32>,
    show_frame: bool,
    showable_frame: bool,
    error_resilient_mode: bool,
    disable_cdf_update: bool,
    disable_frame_end_update_cdf: bool,
    allow_screen_content_tools: u8,
    force_integer_mv: u8,
    allow_high_precision_mv: bool,
    interpolation_filter: vk::video::StdVideoAV1InterpolationFilter,
    is_filter_switchable: bool,
    is_motion_mode_switchable: bool,
    use_ref_frame_mvs: bool,
    reference_select: bool,
    skip_mode_present: bool,
    allow_warped_motion: bool,
    frame_size_override_flag: bool,
    order_hint: Option<u8>,
    primary_ref_frame: Option<u8>,
    refresh_frame_flags: u8,
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_partial_frame_header(
    bits: &NativeVulkanAv1BitReader<'_>,
    prefix: NativeVulkanAv1ParsedFrameHeaderPrefix,
    expected_frame_ids: Vec<u32>,
    reference_order_hints: Vec<u8>,
    frame_refs_short_signaling: bool,
    last_frame_idx: Option<u8>,
    gold_frame_idx: Option<u8>,
    ref_frame_indices: Vec<i8>,
    reason: String,
) -> NativeVulkanAv1ParsedFrameHeader {
    NativeVulkanAv1ParsedFrameHeader {
        frame_header_bytes: bits.byte_offset(),
        tile_count: 0,
        tile_columns: 0,
        tile_rows: 0,
        tile_size_bytes: 0,
        tile_bits: 0,
        tile_info: NativeVulkanAv1ParsedTileInfo {
            tile_count: 0,
            tile_columns: 0,
            tile_rows: 0,
            tile_size_bytes: 0,
            tile_bits: 0,
            uniform_tile_spacing_flag: false,
            context_update_tile_id: 0,
            mi_col_starts: Vec::new(),
            mi_row_starts: Vec::new(),
            width_in_sbs_minus_1: Vec::new(),
            height_in_sbs_minus_1: Vec::new(),
        },
        frame_type: prefix.frame_type,
        show_existing_frame: prefix.show_existing_frame,
        frame_to_show_map_idx: prefix.frame_to_show_map_idx,
        display_frame_id: prefix.display_frame_id,
        current_frame_id: prefix.current_frame_id,
        expected_frame_ids,
        show_frame: prefix.show_frame,
        showable_frame: prefix.showable_frame,
        error_resilient_mode: prefix.error_resilient_mode,
        disable_cdf_update: prefix.disable_cdf_update,
        disable_frame_end_update_cdf: prefix.disable_frame_end_update_cdf,
        allow_screen_content_tools: prefix.allow_screen_content_tools,
        force_integer_mv: prefix.force_integer_mv,
        allow_high_precision_mv: prefix.allow_high_precision_mv,
        interpolation_filter: prefix.interpolation_filter,
        is_filter_switchable: prefix.is_filter_switchable,
        is_motion_mode_switchable: prefix.is_motion_mode_switchable,
        use_ref_frame_mvs: prefix.use_ref_frame_mvs,
        reference_select: prefix.reference_select,
        skip_mode_present: prefix.skip_mode_present,
        allow_warped_motion: prefix.allow_warped_motion,
        frame_size_override_flag: prefix.frame_size_override_flag,
        order_hint: prefix.order_hint,
        primary_ref_frame: prefix.primary_ref_frame,
        refresh_frame_flags: prefix.refresh_frame_flags,
        reference_order_hints,
        frame_refs_short_signaling,
        last_frame_idx,
        gold_frame_idx,
        ref_frame_indices,
        use_superres: false,
        coded_denom: 8,
        render_and_frame_size_different: None,
        frame_width: None,
        frame_height: None,
        render_width: None,
        render_height: None,
        quantization: NativeVulkanAv1ParsedQuantization {
            base_q_idx: 0,
            delta_q_y_dc: 0,
            delta_q_u_dc: 0,
            delta_q_u_ac: 0,
            delta_q_v_dc: 0,
            delta_q_v_ac: 0,
            using_qmatrix: false,
            diff_uv_delta: false,
            qm_y: 0,
            qm_u: 0,
            qm_v: 0,
        },
        segmentation: NativeVulkanAv1ParsedSegmentation {
            enabled: false,
            update_map: false,
            temporal_update: false,
            update_data: false,
            feature_enabled: [0; 8],
            feature_data: [[0; 8]; 8],
        },
        delta_q: NativeVulkanAv1ParsedDeltaQ {
            present: false,
            res: 0,
        },
        delta_lf: NativeVulkanAv1ParsedDeltaLf {
            present: false,
            res: 0,
            multi: false,
        },
        loop_filter: NativeVulkanAv1ParsedLoopFilter {
            level: [0; 4],
            sharpness: 0,
            delta_enabled: false,
            delta_update: false,
            update_ref_delta: 0,
            ref_deltas: [1, 0, 0, 0, -1, 0, -1, -1],
            update_mode_delta: 0,
            mode_deltas: [0, 0],
        },
        cdef: NativeVulkanAv1ParsedCdef {
            damping_minus_3: 0,
            bits: 0,
            y_pri_strength: [0; 8],
            y_sec_strength: [0; 8],
            uv_pri_strength: [0; 8],
            uv_sec_strength: [0; 8],
        },
        loop_restoration: NativeVulkanAv1ParsedLoopRestoration {
            frame_restoration_type: [
                vk::video::STD_VIDEO_AV1_FRAME_RESTORATION_TYPE_NONE.0 as u32,
                vk::video::STD_VIDEO_AV1_FRAME_RESTORATION_TYPE_NONE.0 as u32,
                vk::video::STD_VIDEO_AV1_FRAME_RESTORATION_TYPE_NONE.0 as u32,
            ],
            loop_restoration_size: [0; 3],
            uses_lr: false,
            uses_chroma_lr: false,
        },
        global_motion: native_vulkan_av1_default_global_motion(),
        tx_mode_select: false,
        reduced_tx_set: false,
        unsupported_reason: Some(reason),
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeVulkanAv1ParsedQuantization {
    base_q_idx: u8,
    delta_q_y_dc: i8,
    delta_q_u_dc: i8,
    delta_q_u_ac: i8,
    delta_q_v_dc: i8,
    delta_q_v_ac: i8,
    using_qmatrix: bool,
    diff_uv_delta: bool,
    qm_y: u8,
    qm_u: u8,
    qm_v: u8,
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeVulkanAv1ParsedSegmentation {
    enabled: bool,
    update_map: bool,
    temporal_update: bool,
    update_data: bool,
    feature_enabled: [u8; 8],
    feature_data: [[i16; 8]; 8],
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeVulkanAv1ParsedDeltaQ {
    present: bool,
    res: u8,
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeVulkanAv1ParsedDeltaLf {
    present: bool,
    res: u8,
    multi: bool,
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeVulkanAv1ParsedLoopFilter {
    level: [u8; 4],
    sharpness: u8,
    delta_enabled: bool,
    delta_update: bool,
    update_ref_delta: u8,
    ref_deltas: [i8; 8],
    update_mode_delta: u8,
    mode_deltas: [i8; 2],
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeVulkanAv1ParsedCdef {
    damping_minus_3: u8,
    bits: u8,
    y_pri_strength: [u8; 8],
    y_sec_strength: [u8; 8],
    uv_pri_strength: [u8; 8],
    uv_sec_strength: [u8; 8],
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeVulkanAv1ParsedLoopRestoration {
    frame_restoration_type: [u32; 3],
    loop_restoration_size: [u16; 3],
    uses_lr: bool,
    uses_chroma_lr: bool,
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeVulkanAv1ParsedGlobalMotion {
    gm_type: [u8; 8],
    gm_params: [[i32; 6]; 8],
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_frame_header_for_submit(
    payload: &[u8],
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
) -> Result<NativeVulkanAv1ParsedFrameHeader, String> {
    native_vulkan_parse_av1_frame_header_for_submit_with_context(payload, sequence_header, None)
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_frame_header_for_submit_with_context(
    payload: &[u8],
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
    reference_context: Option<&NativeVulkanAv1FrameHeaderReferenceContext>,
) -> Result<NativeVulkanAv1ParsedFrameHeader, String> {
    if !sequence_header.vulkan_std_session_parameters_ready {
        return Err("AV1 sequence header is not Vulkan STD ready".to_owned());
    }
    if sequence_header.decoder_model_info_present_flag {
        return Err("AV1 decoder model frame header fields are not parsed yet".to_owned());
    }
    if sequence_header.enable_superres {
        return Err("AV1 superres frame headers are not parsed yet".to_owned());
    }
    if sequence_header.film_grain_params_present {
        return Err("AV1 film grain frame headers are not parsed yet".to_owned());
    }

    let mut bits = NativeVulkanAv1BitReader::new(payload);
    let mut show_existing_frame = false;
    let mut frame_to_show_map_idx = None;
    let mut display_frame_id = None;
    if !sequence_header.reduced_still_picture_header {
        show_existing_frame = bits.read_bool("show_existing_frame")?;
        if show_existing_frame {
            frame_to_show_map_idx = Some(native_vulkan_av1_u8(
                bits.read_bits(3, "frame_to_show_map_idx")?,
                "frame_to_show_map_idx",
            )?);
            if sequence_header.frame_id_numbers_present_flag {
                let frame_id_bits = u32::from(
                    sequence_header
                        .additional_frame_id_length_minus_1
                        .unwrap_or(0),
                ) + u32::from(
                    sequence_header.delta_frame_id_length_minus_2.unwrap_or(0),
                ) + 3;
                display_frame_id = Some(bits.read_bits(frame_id_bits, "display_frame_id")?);
            }
            let prefix = NativeVulkanAv1ParsedFrameHeaderPrefix {
                frame_type: u8::MAX,
                show_existing_frame,
                frame_to_show_map_idx,
                display_frame_id,
                current_frame_id: None,
                show_frame: true,
                showable_frame: false,
                error_resilient_mode: false,
                disable_cdf_update: true,
                disable_frame_end_update_cdf: true,
                allow_screen_content_tools: 0,
                force_integer_mv: 2,
                allow_high_precision_mv: false,
                interpolation_filter: vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_EIGHTTAP,
                is_filter_switchable: false,
                is_motion_mode_switchable: false,
                use_ref_frame_mvs: false,
                reference_select: false,
                skip_mode_present: false,
                allow_warped_motion: false,
                frame_size_override_flag: false,
                order_hint: None,
                primary_ref_frame: None,
                refresh_frame_flags: 0,
            };
            return Ok(native_vulkan_av1_partial_frame_header(
                &bits,
                prefix,
                Vec::new(),
                Vec::new(),
                false,
                None,
                None,
                Vec::new(),
                "AV1 show_existing_frame map index parsed; display handoff waits for reference map availability".to_owned(),
            ));
        }
    }

    let (frame_type, show_frame, showable_frame) = if sequence_header.reduced_still_picture_header {
        (0u8, true, false)
    } else {
        let frame_type = native_vulkan_av1_u8(bits.read_bits(2, "frame_type")?, "frame_type")?;
        let show_frame = bits.read_bool("show_frame")?;
        let showable_frame = if show_frame {
            frame_type != 0
        } else {
            bits.read_bool("showable_frame")?
        };
        (frame_type, show_frame, showable_frame)
    };
    let frame_is_intra = matches!(frame_type, 0 | 2);
    let error_resilient_mode = if sequence_header.reduced_still_picture_header
        || frame_type == 3
        || (frame_type == 0 && show_frame)
    {
        true
    } else {
        bits.read_bool("error_resilient_mode")?
    };
    let disable_cdf_update = bits.read_bool("disable_cdf_update")?;

    let allow_screen_content_tools = if sequence_header.seq_force_screen_content_tools == 2 {
        u8::from(bits.read_bool("allow_screen_content_tools")?)
    } else {
        sequence_header.seq_force_screen_content_tools
    };
    let force_integer_mv = if allow_screen_content_tools > 0 {
        if sequence_header.seq_force_integer_mv == 2 {
            u8::from(bits.read_bool("force_integer_mv")?)
        } else {
            sequence_header.seq_force_integer_mv
        }
    } else {
        0
    };

    let current_frame_id = if sequence_header.frame_id_numbers_present_flag {
        let frame_id_bits = u32::from(sequence_header.delta_frame_id_length_minus_2.unwrap_or(0))
            + u32::from(
                sequence_header
                    .additional_frame_id_length_minus_1
                    .unwrap_or(0),
            )
            + 3;
        Some(bits.read_bits(frame_id_bits, "current_frame_id")?)
    } else {
        None
    };

    let frame_size_override_flag =
        if frame_type != 3 && !sequence_header.reduced_still_picture_header {
            bits.read_bool("frame_size_override_flag")?
        } else {
            false
        };
    let order_hint = if sequence_header.enable_order_hint {
        let order_hint_bits = u32::from(sequence_header.order_hint_bits_minus_1.unwrap_or(0)) + 1;
        Some(native_vulkan_av1_u8(
            bits.read_bits(order_hint_bits, "order_hint")?,
            "order_hint",
        )?)
    } else {
        None
    };
    let primary_ref_frame = if !error_resilient_mode && !frame_is_intra {
        Some(native_vulkan_av1_u8(
            bits.read_bits(3, "primary_ref_frame")?,
            "primary_ref_frame",
        )?)
    } else {
        None
    };

    let refresh_frame_flags = if frame_type == 0 && show_frame {
        0xff
    } else if frame_type == 3 {
        0xff
    } else {
        native_vulkan_av1_u8(
            bits.read_bits(8, "refresh_frame_flags")?,
            "refresh_frame_flags",
        )?
    };

    let mut reference_order_hints = Vec::new();
    if !frame_is_intra || refresh_frame_flags != 0xff {
        if error_resilient_mode && sequence_header.enable_order_hint {
            let order_hint_bits =
                u32::from(sequence_header.order_hint_bits_minus_1.unwrap_or(0)) + 1;
            for _ in 0..8 {
                reference_order_hints.push(native_vulkan_av1_u8(
                    bits.read_bits(order_hint_bits, "ref_order_hint")?,
                    "ref_order_hint",
                )?);
            }
        }
    }

    let prefix = NativeVulkanAv1ParsedFrameHeaderPrefix {
        frame_type,
        show_existing_frame,
        frame_to_show_map_idx,
        display_frame_id,
        current_frame_id,
        show_frame,
        showable_frame,
        error_resilient_mode,
        disable_cdf_update,
        disable_frame_end_update_cdf: true,
        allow_screen_content_tools,
        force_integer_mv,
        allow_high_precision_mv: false,
        interpolation_filter: vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_EIGHTTAP,
        is_filter_switchable: false,
        is_motion_mode_switchable: false,
        use_ref_frame_mvs: false,
        reference_select: false,
        skip_mode_present: false,
        allow_warped_motion: false,
        frame_size_override_flag,
        order_hint,
        primary_ref_frame,
        refresh_frame_flags,
    };

    let mut frame_refs_short_signaling = false;
    let mut last_frame_idx = None;
    let mut gold_frame_idx = None;
    let mut ref_frame_indices = Vec::new();
    let mut expected_frame_ids = Vec::new();
    let mut allow_high_precision_mv = false;
    let mut interpolation_filter = vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_EIGHTTAP;
    let mut is_filter_switchable = false;
    let mut is_motion_mode_switchable = false;
    let mut use_ref_frame_mvs = false;
    let mut reference_select = false;
    let mut skip_mode_present = false;
    let mut allow_warped_motion = false;

    let (
        frame_width,
        frame_height,
        render_width,
        render_height,
        render_and_frame_size_different,
        use_superres,
        coded_denom,
        allow_intrabc,
    ) = if frame_is_intra {
        let frame_size = native_vulkan_parse_av1_frame_size(
            &mut bits,
            sequence_header,
            frame_size_override_flag,
        )?;
        let (use_superres, coded_denom) =
            native_vulkan_parse_av1_superres_params(&mut bits, sequence_header)?;
        let render_size = native_vulkan_parse_av1_render_size(&mut bits, frame_size)?;
        let allow_intrabc = allow_screen_content_tools > 0 && bits.read_bool("allow_intrabc")?;
        (
            Some(frame_size.0),
            Some(frame_size.1),
            Some(render_size.1),
            Some(render_size.2),
            Some(render_size.0),
            use_superres,
            coded_denom,
            allow_intrabc,
        )
    } else {
        if sequence_header.enable_order_hint {
            frame_refs_short_signaling = bits.read_bool("frame_refs_short_signaling")?;
            if frame_refs_short_signaling {
                last_frame_idx = Some(native_vulkan_av1_u8(
                    bits.read_bits(3, "last_frame_idx")?,
                    "last_frame_idx",
                )?);
                gold_frame_idx = Some(native_vulkan_av1_u8(
                    bits.read_bits(3, "gold_frame_idx")?,
                    "gold_frame_idx",
                )?);
            }
        }
        if frame_refs_short_signaling {
            return Ok(native_vulkan_av1_partial_frame_header(
                &bits,
                prefix,
                expected_frame_ids,
                reference_order_hints,
                frame_refs_short_signaling,
                last_frame_idx,
                gold_frame_idx,
                ref_frame_indices,
                "AV1 inter frame short reference signaling needs set_frame_refs slot expansion"
                    .to_owned(),
            ));
        }
        ref_frame_indices.reserve(7);
        for _ in 0..7 {
            ref_frame_indices.push(native_vulkan_av1_i8(
                bits.read_bits(3, "ref_frame_idx")?,
                "ref_frame_idx",
            )?);
            if sequence_header.frame_id_numbers_present_flag {
                let delta_frame_id_bits =
                    u32::from(sequence_header.delta_frame_id_length_minus_2.unwrap_or(0)) + 2;
                let delta_frame_id_minus_1 =
                    bits.read_bits(delta_frame_id_bits, "delta_frame_id_minus_1")?;
                let frame_id_bits =
                    u32::from(sequence_header.delta_frame_id_length_minus_2.unwrap_or(0))
                        + u32::from(
                            sequence_header
                                .additional_frame_id_length_minus_1
                                .unwrap_or(0),
                        )
                        + 3;
                let modulus = 1u64.checked_shl(frame_id_bits).unwrap_or(0).max(1);
                let current = u64::from(current_frame_id.unwrap_or(0));
                let delta = u64::from(delta_frame_id_minus_1).saturating_add(1);
                expected_frame_ids.push(((current + modulus - (delta % modulus)) % modulus) as u32);
            }
        }
        let inter_tail_parse = (|| -> Result<
            (
                (u32, u32),
                (bool, u32, u32),
                bool,
                u8,
                bool,
                vk::video::StdVideoAV1InterpolationFilter,
                bool,
                bool,
                bool,
                bool,
            ),
            String,
        > {
            let (frame_size, render_size, use_superres, coded_denom) =
                if frame_size_override_flag && !error_resilient_mode {
                    native_vulkan_parse_av1_frame_size_with_refs(
                        &mut bits,
                        sequence_header,
                        reference_context,
                    )?
                } else {
                let frame_size = native_vulkan_parse_av1_frame_size(
                    &mut bits,
                    sequence_header,
                    frame_size_override_flag,
                )?;
                let (use_superres, coded_denom) =
                    native_vulkan_parse_av1_superres_params(&mut bits, sequence_header)?;
                let render_size = native_vulkan_parse_av1_render_size(&mut bits, frame_size)?;
                (frame_size, render_size, use_superres, coded_denom)
                };

            let allow_high_precision_mv = if force_integer_mv != 1 {
                bits.read_bool("allow_high_precision_mv")?
            } else {
                false
            };
            let (interpolation_filter, is_filter_switchable) =
                native_vulkan_parse_av1_interpolation_filter(&mut bits)?;
            let is_motion_mode_switchable = bits.read_bool("is_motion_mode_switchable")?;
            let use_ref_frame_mvs = if !error_resilient_mode && sequence_header.enable_ref_frame_mvs
            {
                bits.read_bool("use_ref_frame_mvs")?
            } else {
                false
            };
            let allow_warped_motion = false;
            Ok((
                frame_size,
                render_size,
                use_superres,
                coded_denom,
                allow_high_precision_mv,
                interpolation_filter,
                is_filter_switchable,
                is_motion_mode_switchable,
                allow_warped_motion,
                use_ref_frame_mvs,
            ))
        })();
        let (
            frame_size,
            render_size,
            parsed_use_superres,
            parsed_coded_denom,
            parsed_allow_high_precision_mv,
            parsed_interpolation_filter,
            parsed_is_filter_switchable,
            parsed_is_motion_mode_switchable,
            parsed_allow_warped_motion,
            parsed_use_ref_frame_mvs,
        ) = match inter_tail_parse {
            Ok(parsed) => parsed,
            Err(reason) => {
                return Ok(native_vulkan_av1_partial_frame_header(
                    &bits,
                    prefix,
                    expected_frame_ids,
                    reference_order_hints,
                    frame_refs_short_signaling,
                    last_frame_idx,
                    gold_frame_idx,
                    ref_frame_indices,
                    format!(
                        "AV1 inter frame reference indices parsed; inter submit fields are not ready: {reason}"
                    ),
                ));
            }
        };
        allow_high_precision_mv = parsed_allow_high_precision_mv;
        interpolation_filter = parsed_interpolation_filter;
        is_filter_switchable = parsed_is_filter_switchable;
        is_motion_mode_switchable = parsed_is_motion_mode_switchable;
        allow_warped_motion = parsed_allow_warped_motion;
        use_ref_frame_mvs = parsed_use_ref_frame_mvs;
        (
            Some(frame_size.0),
            Some(frame_size.1),
            Some(render_size.1),
            Some(render_size.2),
            Some(render_size.0),
            parsed_use_superres,
            parsed_coded_denom,
            false,
        )
    };
    if allow_intrabc {
        return Err(
            "AV1 intra block copy is not supported by the first direct submit gate".to_owned(),
        );
    }
    let disable_frame_end_update_cdf = if !disable_cdf_update {
        bits.read_bool("disable_frame_end_update_cdf")?
    } else {
        true
    };

    let primary_reference_history =
        reference_context.and_then(|context| context.primary_reference_history(primary_ref_frame));

    let tile_info = native_vulkan_parse_av1_tile_info(
        &mut bits,
        sequence_header,
        frame_width.unwrap_or(sequence_header.max_frame_width),
        frame_height.unwrap_or(sequence_header.max_frame_height),
    )?;
    let quantization = native_vulkan_parse_av1_quantization_params(&mut bits, sequence_header)?;
    let segmentation = native_vulkan_parse_av1_segmentation_params(
        &mut bits,
        primary_ref_frame,
        primary_reference_history,
    )?;
    let delta_q = native_vulkan_parse_av1_delta_q_params(&mut bits)?;
    let delta_lf = native_vulkan_parse_av1_delta_lf_params(&mut bits, delta_q.present)?;
    let loop_filter = native_vulkan_parse_av1_loop_filter_params(
        &mut bits,
        sequence_header,
        primary_reference_history,
    )?;
    let cdef = native_vulkan_parse_av1_cdef_params(&mut bits, sequence_header)?;
    let loop_restoration =
        native_vulkan_parse_av1_loop_restoration_params(&mut bits, sequence_header)?;
    let tx_mode_select = native_vulkan_parse_av1_tx_mode(&mut bits)?;
    let mut global_motion = native_vulkan_av1_default_global_motion();
    let reduced_tx_set;
    if !frame_is_intra {
        reference_select = bits.read_bool("reference_select")?;
        let skip_mode_allowed = reference_context
            .and_then(|context| {
                native_vulkan_av1_skip_mode_frame_from_order_hints(
                    sequence_header,
                    frame_type,
                    error_resilient_mode,
                    reference_select,
                    order_hint.unwrap_or(0),
                    context.reference_name_order_hints,
                    context.reference_name_slot_indices,
                )
            })
            .is_some();
        if !native_vulkan_av1_skip_mode_parse_disabled()
            && (skip_mode_allowed
                || (reference_context.is_none()
                    && native_vulkan_av1_skip_mode_present_field_allowed(
                        sequence_header,
                        error_resilient_mode,
                        frame_type,
                    )))
        {
            skip_mode_present = bits.read_bool("skip_mode_present")?;
        }
        allow_warped_motion = if sequence_header.enable_warped_motion && !error_resilient_mode {
            bits.read_bool("allow_warped_motion")?
        } else {
            false
        };
        reduced_tx_set = bits.read_bool("reduced_tx_set")?;
        global_motion = native_vulkan_parse_av1_global_motion_params(
            &mut bits,
            sequence_header,
            allow_warped_motion,
        )?;
    } else {
        reduced_tx_set = bits.read_bool("reduced_tx_set")?;
    }
    let alignment_reason =
        native_vulkan_av1_zero_align_to_byte_with_reason(&mut bits, "frame_header_byte_alignment")?;

    Ok(NativeVulkanAv1ParsedFrameHeader {
        frame_header_bytes: bits.byte_offset(),
        tile_count: tile_info.tile_count,
        tile_columns: tile_info.tile_columns,
        tile_rows: tile_info.tile_rows,
        tile_size_bytes: tile_info.tile_size_bytes,
        tile_bits: tile_info.tile_bits,
        tile_info,
        frame_type,
        show_existing_frame,
        frame_to_show_map_idx,
        display_frame_id,
        current_frame_id,
        expected_frame_ids,
        show_frame,
        showable_frame,
        error_resilient_mode,
        disable_cdf_update,
        disable_frame_end_update_cdf,
        allow_screen_content_tools,
        force_integer_mv,
        allow_high_precision_mv,
        interpolation_filter,
        is_filter_switchable,
        is_motion_mode_switchable,
        use_ref_frame_mvs,
        reference_select,
        skip_mode_present,
        allow_warped_motion,
        frame_size_override_flag,
        order_hint,
        primary_ref_frame,
        refresh_frame_flags,
        reference_order_hints,
        frame_refs_short_signaling,
        last_frame_idx,
        gold_frame_idx,
        ref_frame_indices,
        use_superres,
        coded_denom,
        render_and_frame_size_different,
        frame_width,
        frame_height,
        render_width,
        render_height,
        quantization,
        segmentation,
        delta_q,
        delta_lf,
        loop_filter,
        cdef,
        loop_restoration,
        global_motion,
        tx_mode_select,
        reduced_tx_set,
        unsupported_reason: alignment_reason,
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_frame_size(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
    frame_size_override_flag: bool,
) -> Result<(u32, u32), String> {
    let (width_minus_1, height_minus_1) = if frame_size_override_flag {
        let width_bits = u32::from(sequence_header.frame_width_bits_minus_1) + 1;
        let height_bits = u32::from(sequence_header.frame_height_bits_minus_1) + 1;
        (
            bits.read_bits(width_bits, "frame_width_minus_1")?,
            bits.read_bits(height_bits, "frame_height_minus_1")?,
        )
    } else {
        (
            sequence_header.max_frame_width_minus_1,
            sequence_header.max_frame_height_minus_1,
        )
    };
    Ok((
        width_minus_1
            .checked_add(1)
            .ok_or_else(|| "AV1 frame width overflow".to_owned())?,
        height_minus_1
            .checked_add(1)
            .ok_or_else(|| "AV1 frame height overflow".to_owned())?,
    ))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_render_size(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    frame_size: (u32, u32),
) -> Result<(bool, u32, u32), String> {
    let render_and_frame_size_different = bits.read_bool("render_and_frame_size_different")?;
    if render_and_frame_size_different {
        Ok((
            true,
            bits.read_bits(16, "render_width_minus_1")?
                .checked_add(1)
                .ok_or_else(|| "AV1 render width overflow".to_owned())?,
            bits.read_bits(16, "render_height_minus_1")?
                .checked_add(1)
                .ok_or_else(|| "AV1 render height overflow".to_owned())?,
        ))
    } else {
        Ok((false, frame_size.0, frame_size.1))
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_frame_size_with_refs(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
    reference_context: Option<&NativeVulkanAv1FrameHeaderReferenceContext>,
) -> Result<((u32, u32), (bool, u32, u32), bool, u8), String> {
    for reference_index in 0..vk::MAX_VIDEO_AV1_REFERENCES_PER_FRAME_KHR {
        if bits.read_bool("found_ref")? {
            let (use_superres, coded_denom) =
                native_vulkan_parse_av1_superres_params(bits, sequence_header)?;
            let history = reference_context
                .and_then(|context| context.reference_histories[reference_index])
                .ok_or_else(|| {
                    format!(
                        "AV1 frame_size_with_refs selected reference {} but no reference size history is available",
                        reference_index + 1
                    )
                })?;
            let frame_size = (history.frame_width, history.frame_height);
            return Ok((
                frame_size,
                (false, history.render_width, history.render_height),
                use_superres,
                coded_denom,
            ));
        }
    }
    let frame_size = native_vulkan_parse_av1_frame_size(bits, sequence_header, true)?;
    let (use_superres, coded_denom) =
        native_vulkan_parse_av1_superres_params(bits, sequence_header)?;
    let render_size = native_vulkan_parse_av1_render_size(bits, frame_size)?;
    Ok((frame_size, render_size, use_superres, coded_denom))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_superres_params(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
) -> Result<(bool, u8), String> {
    const SUPERRES_NUM: u8 = 8;
    const SUPERRES_DENOM_MIN: u8 = 9;
    const SUPERRES_DENOM_BITS: u32 = 3;

    if !sequence_header.enable_superres {
        return Ok((false, SUPERRES_NUM));
    }

    let use_superres = bits.read_bool("use_superres")?;
    if !use_superres {
        return Ok((false, SUPERRES_NUM));
    }

    let denom = native_vulkan_av1_u8(
        bits.read_bits(SUPERRES_DENOM_BITS, "coded_denom")?,
        "coded_denom",
    )?
    .saturating_add(SUPERRES_DENOM_MIN);
    Err(format!(
        "AV1 superres coded_denom {denom} is not supported by the direct Vulkan submit path yet"
    ))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_interpolation_filter(
    bits: &mut NativeVulkanAv1BitReader<'_>,
) -> Result<(vk::video::StdVideoAV1InterpolationFilter, bool), String> {
    let is_filter_switchable = bits.read_bool("is_filter_switchable")?;
    if is_filter_switchable {
        return Ok((
            vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_SWITCHABLE,
            true,
        ));
    }
    let filter = match bits.read_bits(2, "interpolation_filter")? {
        0 => vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_EIGHTTAP,
        1 => vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_EIGHTTAP_SMOOTH,
        2 => vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_EIGHTTAP_SHARP,
        3 => vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_BILINEAR,
        other => return Err(format!("AV1 interpolation_filter {other} is invalid")),
    };
    Ok((filter, false))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_skip_mode_present_field_allowed(
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
    error_resilient_mode: bool,
    frame_type: u8,
) -> bool {
    sequence_header.enable_order_hint && !error_resilient_mode && frame_type == 1
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_global_motion_params(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
    parse_global_motion: bool,
) -> Result<NativeVulkanAv1ParsedGlobalMotion, String> {
    let mut global_motion = native_vulkan_av1_default_global_motion();
    if !parse_global_motion || !sequence_header.enable_warped_motion {
        return Ok(global_motion);
    }
    for reference_index in 1..=7 {
        if bits.read_bool("is_global")? {
            let is_rot_zoom = bits.read_bool("is_rot_zoom")?;
            let gm_type = if is_rot_zoom {
                2
            } else if bits.read_bool("is_translation")? {
                1
            } else {
                3
            };
            global_motion.gm_type[reference_index] = gm_type;
            match gm_type {
                1 => {
                    global_motion.gm_params[reference_index][0] =
                        native_vulkan_av1_read_global_param(bits, gm_type, 0, 0)?;
                    global_motion.gm_params[reference_index][1] =
                        native_vulkan_av1_read_global_param(bits, gm_type, 1, 0)?;
                }
                2 => {
                    let gm2 = native_vulkan_av1_read_global_param(
                        bits,
                        gm_type,
                        2,
                        1 << AV1_WARPEDMODEL_PREC_BITS,
                    )?;
                    let gm3 = native_vulkan_av1_read_global_param(bits, gm_type, 3, 0)?;
                    global_motion.gm_params[reference_index][2] = gm2;
                    global_motion.gm_params[reference_index][3] = gm3;
                    global_motion.gm_params[reference_index][4] = -gm3;
                    global_motion.gm_params[reference_index][5] = gm2;
                    global_motion.gm_params[reference_index][0] =
                        native_vulkan_av1_read_global_param(bits, gm_type, 0, 0)?;
                    global_motion.gm_params[reference_index][1] =
                        native_vulkan_av1_read_global_param(bits, gm_type, 1, 0)?;
                }
                3 => {
                    for param_index in 2..=5 {
                        let default = if param_index == 2 || param_index == 5 {
                            1 << AV1_WARPEDMODEL_PREC_BITS
                        } else {
                            0
                        };
                        global_motion.gm_params[reference_index][param_index] =
                            native_vulkan_av1_read_global_param(
                                bits,
                                gm_type,
                                param_index,
                                default,
                            )?;
                    }
                    global_motion.gm_params[reference_index][0] =
                        native_vulkan_av1_read_global_param(bits, gm_type, 0, 0)?;
                    global_motion.gm_params[reference_index][1] =
                        native_vulkan_av1_read_global_param(bits, gm_type, 1, 0)?;
                }
                _ => return Err(format!("AV1 global motion type {gm_type} is invalid")),
            }
        }
    }
    Ok(global_motion)
}

#[cfg(any(feature = "native-vulkan-video", test))]
const AV1_GM_ABS_TRANS_BITS: u32 = 12;
#[cfg(any(feature = "native-vulkan-video", test))]
const AV1_GM_ABS_TRANS_ONLY_BITS: u32 = 9;
#[cfg(any(feature = "native-vulkan-video", test))]
const AV1_GM_ABS_ALPHA_BITS: u32 = 12;
#[cfg(any(feature = "native-vulkan-video", test))]
const AV1_GM_ALPHA_PREC_BITS: u32 = 15;
#[cfg(any(feature = "native-vulkan-video", test))]
const AV1_GM_TRANS_PREC_BITS: u32 = 6;
#[cfg(any(feature = "native-vulkan-video", test))]
const AV1_GM_TRANS_ONLY_PREC_BITS: u32 = 3;
#[cfg(any(feature = "native-vulkan-video", test))]
const AV1_WARPEDMODEL_PREC_BITS: u32 = 16;
#[cfg(any(feature = "native-vulkan-video", test))]
const AV1_SUBEXP_K: u32 = 3;

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_default_global_motion() -> NativeVulkanAv1ParsedGlobalMotion {
    let mut gm_params = [[0i32; 6]; 8];
    for params in &mut gm_params {
        params[2] = 1 << AV1_WARPEDMODEL_PREC_BITS;
        params[5] = 1 << AV1_WARPEDMODEL_PREC_BITS;
    }
    NativeVulkanAv1ParsedGlobalMotion {
        gm_type: [0; 8],
        gm_params,
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_read_global_param(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    gm_type: u8,
    param_index: usize,
    previous_value: i32,
) -> Result<i32, String> {
    let (abs_bits, prec_bits) = if param_index < 2 {
        if gm_type == 1 {
            (AV1_GM_ABS_TRANS_ONLY_BITS, AV1_GM_TRANS_ONLY_PREC_BITS)
        } else {
            (AV1_GM_ABS_TRANS_BITS, AV1_GM_TRANS_PREC_BITS)
        }
    } else {
        (AV1_GM_ABS_ALPHA_BITS, AV1_GM_ALPHA_PREC_BITS)
    };
    let precision_diff = AV1_WARPEDMODEL_PREC_BITS
        .checked_sub(prec_bits)
        .ok_or_else(|| "AV1 global motion precision underflow".to_owned())?;
    let round = if param_index == 2 || param_index == 5 {
        1 << AV1_WARPEDMODEL_PREC_BITS
    } else {
        0
    };
    let reference = (previous_value - round) >> precision_diff;
    let mx = 1i32
        .checked_shl(abs_bits)
        .ok_or_else(|| "AV1 global motion mx overflow".to_owned())?;
    let value = native_vulkan_av1_decode_signed_subexp_with_ref(
        bits,
        -mx,
        mx + 1,
        AV1_SUBEXP_K,
        reference,
        "global_motion_param",
    )?;
    value
        .checked_shl(precision_diff)
        .and_then(|value| value.checked_add(round))
        .ok_or_else(|| "AV1 global motion parameter overflow".to_owned())
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_decode_signed_subexp_with_ref(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    low: i32,
    high: i32,
    k: u32,
    reference: i32,
    label: &'static str,
) -> Result<i32, String> {
    if high <= low {
        return Err(format!(
            "{label} has invalid signed subexp range {low}..{high}"
        ));
    }
    let range = u32::try_from(high - low).map_err(|_| format!("{label} range exceeds u32"))?;
    let reference = (reference - low).clamp(0, high - low - 1) as u32;
    let value =
        native_vulkan_av1_decode_unsigned_subexp_with_ref(bits, range, k, reference, label)?;
    i32::try_from(value)
        .map(|value| value + low)
        .map_err(|_| format!("{label} value exceeds i32"))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_decode_unsigned_subexp_with_ref(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    mx: u32,
    k: u32,
    reference: u32,
    label: &'static str,
) -> Result<u32, String> {
    let value = native_vulkan_av1_decode_subexp(bits, mx, k, label)?;
    if reference.saturating_mul(2) <= mx {
        native_vulkan_av1_inverse_recenter(reference, value)
    } else {
        let recentered = native_vulkan_av1_inverse_recenter(mx - 1 - reference, value)?;
        Ok(mx - 1 - recentered)
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_decode_subexp(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    num_syms: u32,
    k: u32,
    label: &'static str,
) -> Result<u32, String> {
    let mut index = 0u32;
    let mut mk = 0u32;
    loop {
        let b = if index == 0 { k } else { k + index - 1 };
        let a = 1u32
            .checked_shl(b)
            .ok_or_else(|| format!("{label} subexp shift overflow"))?;
        if num_syms <= mk.saturating_add(3u32.saturating_mul(a)) {
            return Ok(mk + bits.read_quniform(num_syms - mk, label)?);
        }
        if bits.read_bool(label)? {
            index = index.saturating_add(1);
            mk = mk.saturating_add(a);
        } else {
            return Ok(mk + bits.read_bits(b, label)?);
        }
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_inverse_recenter(reference: u32, value: u32) -> Result<u32, String> {
    if value > reference.saturating_mul(2) {
        return Ok(value);
    }
    if value.is_multiple_of(2) {
        reference
            .checked_add(value / 2)
            .ok_or_else(|| "AV1 inverse_recenter overflow".to_owned())
    } else {
        reference
            .checked_sub(value.div_ceil(2))
            .ok_or_else(|| "AV1 inverse_recenter underflow".to_owned())
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeVulkanAv1ParsedTileInfo {
    tile_count: u32,
    tile_columns: u32,
    tile_rows: u32,
    tile_size_bytes: u32,
    tile_bits: u32,
    uniform_tile_spacing_flag: bool,
    context_update_tile_id: u16,
    mi_col_starts: Vec<u16>,
    mi_row_starts: Vec<u16>,
    width_in_sbs_minus_1: Vec<u16>,
    height_in_sbs_minus_1: Vec<u16>,
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_tile_info(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
    frame_width: u32,
    frame_height: u32,
) -> Result<NativeVulkanAv1ParsedTileInfo, String> {
    let sb_size = if sequence_header.use_128x128_superblock {
        128
    } else {
        64
    };
    let sb_cols = frame_width.div_ceil(sb_size);
    let sb_rows = frame_height.div_ceil(sb_size);
    let mi_size_per_sb = if sequence_header.use_128x128_superblock {
        32u32
    } else {
        16u32
    };
    let max_tile_width_sb: u32 = if sequence_header.use_128x128_superblock {
        32
    } else {
        64
    };
    let max_tile_area_sb: u32 = if sequence_header.use_128x128_superblock {
        576
    } else {
        2304
    };
    let uniform_tile_spacing_flag = bits.read_bool("uniform_tile_spacing_flag")?;
    if uniform_tile_spacing_flag {
        let mut min_log2_tile_cols = 0u32;
        while (max_tile_width_sb << min_log2_tile_cols) < sb_cols {
            min_log2_tile_cols = min_log2_tile_cols.saturating_add(1);
        }
        let max_log2_tile_cols = native_vulkan_av1_ceil_log2(sb_cols);
        let mut tile_cols_log2 = min_log2_tile_cols;
        while tile_cols_log2 < max_log2_tile_cols && bits.read_bool("increment_tile_cols_log2")? {
            tile_cols_log2 = tile_cols_log2.saturating_add(1);
        }
        let tile_width_divisor = 1u32
            .checked_shl(tile_cols_log2)
            .ok_or_else(|| "AV1 tile_cols_log2 overflow".to_owned())?;
        let tile_width_sb =
            sb_cols.saturating_add(tile_width_divisor).saturating_sub(1) / tile_width_divisor;
        let tile_columns = sb_cols.div_ceil(tile_width_sb.max(1));

        let mut min_log2_tile_rows = 0u32;
        while max_tile_area_sb
            .checked_shr(tile_cols_log2.saturating_add(min_log2_tile_rows))
            .unwrap_or(0)
            < tile_width_sb.saturating_mul(sb_rows)
        {
            min_log2_tile_rows = min_log2_tile_rows.saturating_add(1);
        }
        let max_log2_tile_rows = native_vulkan_av1_ceil_log2(sb_rows);
        let mut tile_rows_log2 = min_log2_tile_rows.min(max_log2_tile_rows);
        while tile_rows_log2 < max_log2_tile_rows && bits.read_bool("increment_tile_rows_log2")? {
            tile_rows_log2 = tile_rows_log2.saturating_add(1);
        }
        let tile_height_divisor = 1u32
            .checked_shl(tile_rows_log2)
            .ok_or_else(|| "AV1 tile_rows_log2 overflow".to_owned())?;
        let tile_height_sb = sb_rows
            .saturating_add(tile_height_divisor)
            .saturating_sub(1)
            / tile_height_divisor;
        let tile_rows = sb_rows.div_ceil(tile_height_sb.max(1));
        let tile_count = tile_columns.saturating_mul(tile_rows);
        let tile_bits = native_vulkan_av1_ceil_log2(tile_columns)
            .saturating_add(native_vulkan_av1_ceil_log2(tile_rows));
        let (context_update_tile_id, tile_size_bytes) = if tile_count > 1 {
            let context_update_tile_id = native_vulkan_av1_u16(
                bits.read_bits(tile_bits, "context_update_tile_id")?,
                "context_update_tile_id",
            )?;
            let tile_size_bytes = bits
                .read_bits(2, "tile_size_bytes_minus_1")?
                .saturating_add(1);
            (context_update_tile_id, tile_size_bytes)
        } else {
            (0, 0)
        };
        let tile_col_widths = native_vulkan_av1_uniform_tile_sizes(sb_cols, tile_width_sb);
        let tile_row_heights = native_vulkan_av1_uniform_tile_sizes(sb_rows, tile_height_sb);
        let (mi_col_starts, width_in_sbs_minus_1) =
            native_vulkan_av1_tile_axis_layout(&tile_col_widths, mi_size_per_sb)?;
        let (mi_row_starts, height_in_sbs_minus_1) =
            native_vulkan_av1_tile_axis_layout(&tile_row_heights, mi_size_per_sb)?;
        return Ok(NativeVulkanAv1ParsedTileInfo {
            tile_count,
            tile_columns,
            tile_rows,
            tile_size_bytes,
            tile_bits,
            uniform_tile_spacing_flag,
            context_update_tile_id,
            mi_col_starts,
            mi_row_starts,
            width_in_sbs_minus_1,
            height_in_sbs_minus_1,
        });
    }

    let mut tile_col_widths = Vec::new();
    let mut widest_tile_sb = 0u32;
    let mut sofar = 0u32;
    while sofar < sb_cols {
        let max_width = max_tile_width_sb.min(sb_cols - sofar);
        let width = bits
            .read_quniform(max_width, "width_in_sbs_minus_1")?
            .saturating_add(1);
        tile_col_widths.push(width);
        sofar = sofar.saturating_add(width);
        widest_tile_sb = widest_tile_sb.max(width);
    }

    let max_tile_height_sb = (max_tile_area_sb / widest_tile_sb.max(1)).max(1);
    let mut tile_row_heights = Vec::new();
    sofar = 0;
    while sofar < sb_rows {
        let max_height = max_tile_height_sb.min(sb_rows - sofar);
        let height = bits
            .read_quniform(max_height, "height_in_sbs_minus_1")?
            .saturating_add(1);
        tile_row_heights.push(height);
        sofar = sofar.saturating_add(height);
    }

    let tile_columns = tile_col_widths.len() as u32;
    let tile_rows = tile_row_heights.len() as u32;
    let tile_count = tile_columns.saturating_mul(tile_rows);
    let tile_bits = native_vulkan_av1_ceil_log2(tile_columns)
        .saturating_add(native_vulkan_av1_ceil_log2(tile_rows));
    let (context_update_tile_id, tile_size_bytes) = if tile_count > 1 {
        let context_update_tile_id = native_vulkan_av1_u16(
            bits.read_bits(tile_bits, "context_update_tile_id")?,
            "context_update_tile_id",
        )?;
        let tile_size_bytes = bits
            .read_bits(2, "tile_size_bytes_minus_1")?
            .saturating_add(1);
        (context_update_tile_id, tile_size_bytes)
    } else {
        (0, 0)
    };
    let (mi_col_starts, width_in_sbs_minus_1) =
        native_vulkan_av1_tile_axis_layout(&tile_col_widths, mi_size_per_sb)?;
    let (mi_row_starts, height_in_sbs_minus_1) =
        native_vulkan_av1_tile_axis_layout(&tile_row_heights, mi_size_per_sb)?;
    Ok(NativeVulkanAv1ParsedTileInfo {
        tile_count,
        tile_columns,
        tile_rows,
        tile_size_bytes,
        tile_bits,
        uniform_tile_spacing_flag,
        context_update_tile_id,
        mi_col_starts,
        mi_row_starts,
        width_in_sbs_minus_1,
        height_in_sbs_minus_1,
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_uniform_tile_sizes(total_sb: u32, tile_size_sb: u32) -> Vec<u32> {
    let tile_size_sb = tile_size_sb.max(1);
    let mut sizes = Vec::new();
    let mut remaining = total_sb;
    while remaining > 0 {
        let size = remaining.min(tile_size_sb);
        sizes.push(size);
        remaining -= size;
    }
    sizes
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_tile_axis_layout(
    sizes_in_sb: &[u32],
    mi_size_per_sb: u32,
) -> Result<(Vec<u16>, Vec<u16>), String> {
    let mut starts = Vec::with_capacity(sizes_in_sb.len().saturating_add(1));
    let mut sizes_minus_1 = Vec::with_capacity(sizes_in_sb.len());
    let mut cursor = 0u32;
    starts.push(0);
    for size in sizes_in_sb.iter().copied() {
        if size == 0 {
            return Err("AV1 tile axis has a zero-sized tile".to_owned());
        }
        sizes_minus_1.push(u16::try_from(size - 1).map_err(|_| {
            format!(
                "AV1 tile axis size_in_sbs_minus_1 {} exceeds u16 range",
                size - 1
            )
        })?);
        cursor = cursor
            .checked_add(size.saturating_mul(mi_size_per_sb))
            .ok_or_else(|| "AV1 tile axis MI cursor overflow".to_owned())?;
        starts.push(u16::try_from(cursor).map_err(|_| {
            format!("AV1 tile axis MI start {cursor} exceeds Vulkan STD u16 range")
        })?);
    }
    Ok((starts, sizes_minus_1))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_tile_group_offsets_from_payload(
    absolute_payload_base_offset: u64,
    tile_payload_offset: usize,
    tile_payload: &[u8],
    header: &NativeVulkanAv1ParsedFrameHeader,
) -> Result<(Vec<u32>, Vec<u32>), String> {
    if header.tile_count == 0 {
        return Err("AV1 tile_count is zero".to_owned());
    }
    let absolute_payload_base_offset = u32::try_from(absolute_payload_base_offset)
        .map_err(|_| "AV1 tile payload base offset exceeds u32 range".to_owned())?;
    let tile_payload_offset = u32::try_from(tile_payload_offset)
        .map_err(|_| "AV1 tile payload offset exceeds u32 range".to_owned())?;
    let absolute_tile_payload_offset = absolute_payload_base_offset
        .checked_add(tile_payload_offset)
        .ok_or_else(|| "AV1 absolute tile payload offset overflow".to_owned())?;

    if header.tile_count == 1 {
        let leading_padding =
            native_vulkan_av1_single_tile_leading_padding_bytes(header, tile_payload);
        let absolute_tile_offset = absolute_tile_payload_offset
            .checked_add(
                u32::try_from(leading_padding)
                    .map_err(|_| "AV1 single tile padding exceeds u32 range".to_owned())?,
            )
            .ok_or_else(|| "AV1 single tile absolute offset overflow".to_owned())?;
        let size = u32::try_from(tile_payload.len().saturating_sub(leading_padding))
            .map_err(|_| "AV1 single tile payload exceeds u32 range".to_owned())?;
        return Ok((vec![absolute_tile_offset], vec![size]));
    }
    if header.tile_size_bytes == 0 {
        return Err("AV1 multi-tile payload has zero tile_size_bytes".to_owned());
    }

    let mut bits = NativeVulkanAv1BitReader::new(tile_payload);
    let tile_start_and_end_present_flag = bits.read_bool("tile_start_and_end_present_flag")?;
    let (tile_start, tile_end) = if tile_start_and_end_present_flag {
        (
            bits.read_bits(header.tile_bits, "tg_start")?,
            bits.read_bits(header.tile_bits, "tg_end")?,
        )
    } else {
        (0, header.tile_count.saturating_sub(1))
    };
    if tile_start != 0 || tile_end.saturating_add(1) != header.tile_count {
        return Err(format!(
            "AV1 first-frame tile group covers {tile_start}..={tile_end}, expected full 0..={}",
            header.tile_count.saturating_sub(1)
        ));
    }
    bits.zero_align_to_byte("tile_group_header_byte_alignment")?;
    let mut cursor = bits.byte_offset();
    let mut tile_offsets = Vec::with_capacity(header.tile_count as usize);
    let mut tile_sizes = Vec::with_capacity(header.tile_count as usize);
    for tile_index in 0..header.tile_count {
        if cursor > tile_payload.len() {
            return Err("AV1 tile table cursor moved past payload".to_owned());
        }
        let tile_size = if tile_index + 1 == header.tile_count {
            tile_payload.len().saturating_sub(cursor)
        } else {
            let size_bytes = header.tile_size_bytes as usize;
            let size_end = cursor
                .checked_add(size_bytes)
                .ok_or_else(|| "AV1 tile size cursor overflow".to_owned())?;
            let size_field = tile_payload
                .get(cursor..size_end)
                .ok_or_else(|| format!("AV1 tile {tile_index} size field exceeds tile payload"))?;
            cursor = size_end;
            native_vulkan_av1_read_le_uint(size_field)
                .and_then(|value| value.checked_add(1).ok_or(()))
                .map_err(|_| format!("AV1 tile {tile_index} size overflow"))? as usize
        };
        let absolute_offset = absolute_tile_payload_offset
            .checked_add(
                u32::try_from(cursor)
                    .map_err(|_| "AV1 tile offset cursor exceeds u32 range".to_owned())?,
            )
            .ok_or_else(|| "AV1 tile absolute offset overflow".to_owned())?;
        let tile_size_u32 = u32::try_from(tile_size)
            .map_err(|_| format!("AV1 tile {tile_index} size exceeds u32 range"))?;
        tile_offsets.push(absolute_offset);
        tile_sizes.push(tile_size_u32);
        cursor = cursor
            .checked_add(tile_size)
            .ok_or_else(|| "AV1 tile cursor overflow".to_owned())?;
    }
    if cursor != tile_payload.len() {
        return Err(format!(
            "AV1 tile table consumed {cursor} bytes but payload has {} bytes",
            tile_payload.len()
        ));
    }
    Ok((tile_offsets, tile_sizes))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_single_tile_leading_padding_bytes(
    header: &NativeVulkanAv1ParsedFrameHeader,
    tile_payload: &[u8],
) -> usize {
    if header.frame_type == 1
        && header.tile_count == 1
        && header.tile_columns == 1
        && header.tile_rows == 1
        && tile_payload.len() > 1
        && tile_payload.first().copied() == Some(0)
    {
        1
    } else {
        0
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_quantization_params(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
) -> Result<NativeVulkanAv1ParsedQuantization, String> {
    let base_q_idx = native_vulkan_av1_u8(bits.read_bits(8, "base_q_idx")?, "base_q_idx")?;
    let delta_q_y_dc = native_vulkan_av1_read_delta_q(bits, "delta_q_y_dc")?;
    let mut delta_q_u_dc = 0;
    let mut delta_q_u_ac = 0;
    let mut delta_q_v_dc = 0;
    let mut delta_q_v_ac = 0;
    let mut diff_uv_delta = false;
    if sequence_header.color_config.num_planes > 1 {
        diff_uv_delta = if sequence_header.color_config.separate_uv_delta_q {
            bits.read_bool("diff_uv_delta")?
        } else {
            false
        };
        delta_q_u_dc = native_vulkan_av1_read_delta_q(bits, "delta_q_u_dc")?;
        delta_q_u_ac = native_vulkan_av1_read_delta_q(bits, "delta_q_u_ac")?;
        if diff_uv_delta {
            delta_q_v_dc = native_vulkan_av1_read_delta_q(bits, "delta_q_v_dc")?;
            delta_q_v_ac = native_vulkan_av1_read_delta_q(bits, "delta_q_v_ac")?;
        } else {
            delta_q_v_dc = delta_q_u_dc;
            delta_q_v_ac = delta_q_u_ac;
        }
    }
    let using_qmatrix = bits.read_bool("using_qmatrix")?;
    let mut qm_y = 0;
    let mut qm_u = 0;
    let mut qm_v = 0;
    if using_qmatrix {
        qm_y = native_vulkan_av1_u8(bits.read_bits(4, "qm_y")?, "qm_y")?;
        qm_u = native_vulkan_av1_u8(bits.read_bits(4, "qm_u")?, "qm_u")?;
        if sequence_header.color_config.separate_uv_delta_q {
            qm_v = native_vulkan_av1_u8(bits.read_bits(4, "qm_v")?, "qm_v")?;
        } else {
            qm_v = qm_u;
        }
    }
    Ok(NativeVulkanAv1ParsedQuantization {
        base_q_idx,
        delta_q_y_dc,
        delta_q_u_dc,
        delta_q_u_ac,
        delta_q_v_dc,
        delta_q_v_ac,
        using_qmatrix,
        diff_uv_delta,
        qm_y,
        qm_u,
        qm_v,
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_segmentation_params(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    primary_ref_frame: Option<u8>,
    primary_reference_history: Option<NativeVulkanAv1ReferenceHistory>,
) -> Result<NativeVulkanAv1ParsedSegmentation, String> {
    let segmentation_enabled = bits.read_bool("segmentation_enabled")?;
    let mut segmentation = NativeVulkanAv1ParsedSegmentation {
        enabled: segmentation_enabled,
        update_map: false,
        temporal_update: false,
        update_data: false,
        feature_enabled: [0; 8],
        feature_data: [[0; 8]; 8],
    };
    if !segmentation_enabled {
        return Ok(segmentation);
    }

    let primary_ref_none = native_vulkan_av1_primary_ref_none(primary_ref_frame);
    let segmentation_update_map = if primary_ref_none {
        true
    } else {
        bits.read_bool("segmentation_update_map")?
    };
    segmentation.update_map = segmentation_update_map;
    if segmentation_update_map && !primary_ref_none {
        segmentation.temporal_update = bits.read_bool("segmentation_temporal_update")?;
    }
    let segmentation_update_data = if primary_ref_none {
        true
    } else {
        bits.read_bool("segmentation_update_data")?
    };
    segmentation.update_data = segmentation_update_data;
    if segmentation_update_data {
        const AV1_SEGMENT_FEATURE_BITS: [u32; 8] = [8, 6, 6, 6, 6, 3, 0, 0];
        const AV1_SEGMENT_FEATURE_SIGNED: [bool; 8] =
            [true, true, true, true, true, false, false, false];
        for segment_index in 0..8 {
            for feature_index in 0..8 {
                if bits.read_bool("segmentation_feature_enabled")? {
                    segmentation.feature_enabled[segment_index] |= 1u8 << feature_index;
                    let feature_bits = AV1_SEGMENT_FEATURE_BITS[feature_index];
                    let mut feature_value = if feature_bits > 0 {
                        i16::try_from(bits.read_bits(feature_bits, "segmentation_feature_value")?)
                            .map_err(|_| "AV1 segmentation feature value exceeds i16".to_owned())?
                    } else {
                        0
                    };
                    if AV1_SEGMENT_FEATURE_SIGNED[feature_index]
                        && feature_value != 0
                        && bits.read_bool("segmentation_feature_sign")?
                    {
                        feature_value = -feature_value;
                    }
                    segmentation.feature_data[segment_index][feature_index] = feature_value;
                }
            }
        }
    } else if let Some(history) = primary_reference_history {
        segmentation.feature_enabled = history.segmentation.feature_enabled;
        segmentation.feature_data = history.segmentation.feature_data;
    }
    Ok(segmentation)
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_zero_align_to_byte_with_reason(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    label: &'static str,
) -> Result<Option<String>, String> {
    while !bits.bit_offset.is_multiple_of(8) {
        let _ = bits.read_bool(label)?;
    }
    Ok(None)
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_delta_q_params(
    bits: &mut NativeVulkanAv1BitReader<'_>,
) -> Result<NativeVulkanAv1ParsedDeltaQ, String> {
    let delta_q_present = bits.read_bool("delta_q_present")?;
    let mut delta_q_res = 0;
    if delta_q_present {
        delta_q_res = native_vulkan_av1_u8(bits.read_bits(2, "delta_q_res")?, "delta_q_res")?;
    }
    Ok(NativeVulkanAv1ParsedDeltaQ {
        present: delta_q_present,
        res: delta_q_res,
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_delta_lf_params(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    delta_q_present: bool,
) -> Result<NativeVulkanAv1ParsedDeltaLf, String> {
    let mut delta_lf = NativeVulkanAv1ParsedDeltaLf {
        present: false,
        res: 0,
        multi: false,
    };
    if delta_q_present {
        let delta_lf_present = bits.read_bool("delta_lf_present")?;
        delta_lf.present = delta_lf_present;
        if delta_lf_present {
            delta_lf.res =
                native_vulkan_av1_u8(bits.read_bits(2, "delta_lf_res")?, "delta_lf_res")?;
            delta_lf.multi = bits.read_bool("delta_lf_multi")?;
        }
    }
    Ok(delta_lf)
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_loop_filter_params(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
    primary_reference_history: Option<NativeVulkanAv1ReferenceHistory>,
) -> Result<NativeVulkanAv1ParsedLoopFilter, String> {
    let loop_filter_level_0 = native_vulkan_av1_u8(
        bits.read_bits(6, "loop_filter_level_0")?,
        "loop_filter_level_0",
    )?;
    let loop_filter_level_1 = native_vulkan_av1_u8(
        bits.read_bits(6, "loop_filter_level_1")?,
        "loop_filter_level_1",
    )?;
    let mut level = [loop_filter_level_0, loop_filter_level_1, 0, 0];
    if sequence_header.color_config.num_planes > 1
        && (loop_filter_level_0 > 0 || loop_filter_level_1 > 0)
    {
        level[2] = native_vulkan_av1_u8(
            bits.read_bits(6, "loop_filter_level_2")?,
            "loop_filter_level_2",
        )?;
        level[3] = native_vulkan_av1_u8(
            bits.read_bits(6, "loop_filter_level_3")?,
            "loop_filter_level_3",
        )?;
    }
    let sharpness = native_vulkan_av1_u8(
        bits.read_bits(3, "loop_filter_sharpness")?,
        "loop_filter_sharpness",
    )?;
    let loop_filter_delta_enabled = bits.read_bool("loop_filter_delta_enabled")?;
    let inherited_ref_deltas = primary_reference_history
        .map(|history| history.loop_filter_ref_deltas)
        .unwrap_or([1, 0, 0, 0, -1, 0, -1, -1]);
    let inherited_mode_deltas = primary_reference_history
        .map(|history| history.loop_filter_mode_deltas)
        .unwrap_or([0, 0]);
    let mut loop_filter = NativeVulkanAv1ParsedLoopFilter {
        level,
        sharpness,
        delta_enabled: loop_filter_delta_enabled,
        delta_update: false,
        update_ref_delta: 0,
        ref_deltas: inherited_ref_deltas,
        update_mode_delta: 0,
        mode_deltas: inherited_mode_deltas,
    };
    if loop_filter_delta_enabled {
        let loop_filter_delta_update = bits.read_bool("loop_filter_delta_update")?;
        loop_filter.delta_update = loop_filter_delta_update;
        if loop_filter_delta_update {
            for ref_index in 0..8 {
                if bits.read_bool("update_ref_delta")? {
                    loop_filter.update_ref_delta |= 1u8 << ref_index;
                    loop_filter.ref_deltas[ref_index] =
                        native_vulkan_av1_read_signed_literal(bits, 7, "loop_filter_ref_delta")?;
                }
            }
            for mode_index in 0..2 {
                if bits.read_bool("update_mode_delta")? {
                    loop_filter.update_mode_delta |= 1u8 << mode_index;
                    loop_filter.mode_deltas[mode_index] =
                        native_vulkan_av1_read_signed_literal(bits, 7, "loop_filter_mode_delta")?;
                }
            }
        }
    }
    Ok(loop_filter)
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_cdef_params(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
) -> Result<NativeVulkanAv1ParsedCdef, String> {
    let mut cdef = NativeVulkanAv1ParsedCdef {
        damping_minus_3: 0,
        bits: 0,
        y_pri_strength: [0; 8],
        y_sec_strength: [0; 8],
        uv_pri_strength: [0; 8],
        uv_sec_strength: [0; 8],
    };
    if sequence_header.enable_cdef {
        cdef.damping_minus_3 = native_vulkan_av1_u8(
            bits.read_bits(2, "cdef_damping_minus_3")?,
            "cdef_damping_minus_3",
        )?;
        cdef.bits = native_vulkan_av1_u8(bits.read_bits(2, "cdef_bits")?, "cdef_bits")?;
        for index in 0..(1usize << cdef.bits) {
            let y_strength =
                native_vulkan_av1_u8(bits.read_bits(6, "cdef_y_strength")?, "cdef_y_strength")?;
            cdef.y_pri_strength[index] = y_strength >> 2;
            cdef.y_sec_strength[index] = y_strength & 0x03;
            if cdef.y_sec_strength[index] == 3 {
                cdef.y_sec_strength[index] = 4;
            }
            if sequence_header.color_config.num_planes > 1 {
                let uv_strength = native_vulkan_av1_u8(
                    bits.read_bits(6, "cdef_uv_strength")?,
                    "cdef_uv_strength",
                )?;
                cdef.uv_pri_strength[index] = uv_strength >> 2;
                cdef.uv_sec_strength[index] = uv_strength & 0x03;
                if cdef.uv_sec_strength[index] == 3 {
                    cdef.uv_sec_strength[index] = 4;
                }
            }
        }
    }
    Ok(cdef)
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_loop_restoration_params(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
) -> Result<NativeVulkanAv1ParsedLoopRestoration, String> {
    let mut loop_restoration = NativeVulkanAv1ParsedLoopRestoration {
        frame_restoration_type: [
            vk::video::STD_VIDEO_AV1_FRAME_RESTORATION_TYPE_NONE.0 as u32,
            vk::video::STD_VIDEO_AV1_FRAME_RESTORATION_TYPE_NONE.0 as u32,
            vk::video::STD_VIDEO_AV1_FRAME_RESTORATION_TYPE_NONE.0 as u32,
        ],
        loop_restoration_size: [0; 3],
        uses_lr: false,
        uses_chroma_lr: false,
    };
    if sequence_header.enable_restoration {
        let planes = sequence_header.color_config.num_planes.max(1);
        let mut use_lrf = false;
        let mut use_chroma_lrf = false;
        for plane in 0..usize::from(planes) {
            let restoration_type = native_vulkan_av1_std_frame_restoration_type(
                bits.read_bits(2, "frame_restoration_type")?,
            )?;
            loop_restoration.frame_restoration_type[plane] = restoration_type;
            if restoration_type != 0 {
                use_lrf = true;
                if plane > 0 {
                    use_chroma_lrf = true;
                }
            }
        }
        if use_lrf {
            let lr_unit_shift = if sequence_header.use_128x128_superblock {
                true
            } else {
                bits.read_bool("lr_unit_shift")?
            };
            let lr_unit_extra_shift = if lr_unit_shift {
                bits.read_bool("lr_unit_extra_shift")?
            } else {
                false
            };
            let luma_size =
                native_vulkan_av1_loop_restoration_size(lr_unit_shift, lr_unit_extra_shift, false)?;
            loop_restoration.loop_restoration_size[0] = luma_size;
            if planes > 1 {
                loop_restoration.loop_restoration_size[1] = luma_size;
                loop_restoration.loop_restoration_size[2] = luma_size;
            }
            if use_chroma_lrf
                && sequence_header.color_config.subsampling_x
                && sequence_header.color_config.subsampling_y
            {
                let lr_uv_shift = bits.read_bool("lr_uv_shift")?;
                let chroma_size = native_vulkan_av1_loop_restoration_size(
                    lr_unit_shift,
                    lr_unit_extra_shift,
                    lr_uv_shift,
                )?;
                loop_restoration.loop_restoration_size[1] = chroma_size;
                loop_restoration.loop_restoration_size[2] = chroma_size;
            }
        }
        loop_restoration.uses_lr = use_lrf;
        loop_restoration.uses_chroma_lr = use_chroma_lrf;
    }
    Ok(loop_restoration)
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_tx_mode(
    bits: &mut NativeVulkanAv1BitReader<'_>,
) -> Result<bool, String> {
    bits.read_bool("tx_mode_select")
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_read_delta_q(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    label: &'static str,
) -> Result<i8, String> {
    if bits.read_bool(label)? {
        native_vulkan_av1_read_signed_literal(bits, 7, label)
    } else {
        Ok(0)
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_read_signed_literal(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    count: u32,
    label: &'static str,
) -> Result<i8, String> {
    if count == 0 || count > 8 {
        return Err(format!(
            "{label} requested invalid signed literal width {count}"
        ));
    }
    let value = bits.read_bits(count, label)? as i32;
    let sign_bit = 1i32 << (count - 1);
    let signed = if value & sign_bit != 0 {
        value - (sign_bit << 1)
    } else {
        value
    };
    i8::try_from(signed).map_err(|_| format!("{label}={signed} exceeds i8 range"))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_std_frame_restoration_type(value: u32) -> Result<u32, String> {
    match value {
        0 => Ok(vk::video::STD_VIDEO_AV1_FRAME_RESTORATION_TYPE_NONE.0 as u32),
        1 => Ok(vk::video::STD_VIDEO_AV1_FRAME_RESTORATION_TYPE_WIENER.0 as u32),
        2 => Ok(vk::video::STD_VIDEO_AV1_FRAME_RESTORATION_TYPE_SGRPROJ.0 as u32),
        3 => Ok(vk::video::STD_VIDEO_AV1_FRAME_RESTORATION_TYPE_SWITCHABLE.0 as u32),
        other => Err(format!("unsupported AV1 frame_restoration_type {other}")),
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_loop_restoration_size(
    lr_unit_shift: bool,
    lr_unit_extra_shift: bool,
    lr_uv_shift: bool,
) -> Result<u16, String> {
    let mut size = 256u32;
    if lr_unit_shift {
        size >>= 1;
    }
    if lr_unit_extra_shift {
        size >>= 1;
    }
    if lr_uv_shift {
        size >>= 1;
    }
    u16::try_from(size).map_err(|_| format!("AV1 loop restoration size {size} exceeds u16 range"))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_ceil_log2(value: u32) -> u32 {
    if value <= 1 {
        0
    } else {
        u32::BITS - (value - 1).leading_zeros()
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_read_le_uint(bytes: &[u8]) -> Result<u32, ()> {
    if bytes.len() > 4 {
        return Err(());
    }
    let mut value = 0u32;
    for (index, byte) in bytes.iter().copied().enumerate() {
        value |= u32::from(byte) << (index * 8);
    }
    Ok(value)
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_sequence_header(
    payload: &[u8],
) -> Result<NativeVulkanAv1SequenceHeaderSnapshot, String> {
    let mut bits = NativeVulkanAv1BitReader::new(payload);
    let seq_profile = native_vulkan_av1_u8(bits.read_bits(3, "seq_profile")?, "seq_profile")?;
    if seq_profile > 2 {
        return Err(format!("AV1 seq_profile {seq_profile} is reserved"));
    }
    let still_picture = bits.read_bool("still_picture")?;
    let reduced_still_picture_header = bits.read_bool("reduced_still_picture_header")?;

    let timing_info_present_flag;
    let mut timing_info = None;
    let mut decoder_model_info_present_flag = false;
    let mut buffer_delay_length_minus_1 = 0u8;
    let mut frame_presentation_time_length_minus_1 = 0u8;
    let initial_display_delay_present_flag;
    let operating_points_cnt_minus_1;
    let mut operating_points = Vec::new();

    if reduced_still_picture_header {
        timing_info_present_flag = false;
        initial_display_delay_present_flag = false;
        operating_points_cnt_minus_1 = 0;
        let seq_level_idx =
            native_vulkan_av1_u8(bits.read_bits(5, "seq_level_idx")?, "seq_level_idx")?;
        operating_points.push(NativeVulkanAv1OperatingPointSnapshot {
            index: 0,
            idc: 0,
            seq_level_idx,
            seq_level_label: native_vulkan_av1_sequence_level_idx_label(seq_level_idx),
            seq_tier: false,
            decoder_model_present_for_this_op: false,
            initial_display_delay_present_for_this_op: false,
            initial_display_delay_minus_1: None,
        });
    } else {
        timing_info_present_flag = bits.read_bool("timing_info_present_flag")?;
        if timing_info_present_flag {
            let num_units_in_display_tick = bits.read_bits(32, "num_units_in_display_tick")?;
            let time_scale = bits.read_bits(32, "time_scale")?;
            let equal_picture_interval = bits.read_bool("equal_picture_interval")?;
            let num_ticks_per_picture_minus_1 = if equal_picture_interval {
                Some(bits.read_uvlc("num_ticks_per_picture_minus_1")?)
            } else {
                None
            };
            timing_info = Some(NativeVulkanAv1TimingInfoSnapshot {
                num_units_in_display_tick,
                time_scale,
                equal_picture_interval,
                num_ticks_per_picture_minus_1,
            });
            decoder_model_info_present_flag = bits.read_bool("decoder_model_info_present_flag")?;
            if decoder_model_info_present_flag {
                buffer_delay_length_minus_1 = native_vulkan_av1_u8(
                    bits.read_bits(5, "buffer_delay_length_minus_1")?,
                    "buffer_delay_length_minus_1",
                )?;
                bits.skip_bits(32, "num_units_in_decoding_tick")?;
                bits.skip_bits(5, "buffer_removal_time_length_minus_1")?;
                frame_presentation_time_length_minus_1 = native_vulkan_av1_u8(
                    bits.read_bits(5, "frame_presentation_time_length_minus_1")?,
                    "frame_presentation_time_length_minus_1",
                )?;
            }
        }
        initial_display_delay_present_flag =
            bits.read_bool("initial_display_delay_present_flag")?;
        operating_points_cnt_minus_1 = native_vulkan_av1_u8(
            bits.read_bits(5, "operating_points_cnt_minus_1")?,
            "operating_points_cnt_minus_1",
        )?;
        for index in 0..=operating_points_cnt_minus_1 {
            let idc = native_vulkan_av1_u16(
                bits.read_bits(12, "operating_point_idc")?,
                "operating_point_idc",
            )?;
            let seq_level_idx =
                native_vulkan_av1_u8(bits.read_bits(5, "seq_level_idx")?, "seq_level_idx")?;
            let seq_tier = seq_level_idx > 7 && bits.read_bool("seq_tier")?;
            let decoder_model_present_for_this_op = if decoder_model_info_present_flag {
                bits.read_bool("decoder_model_present_for_this_op")?
            } else {
                false
            };
            if decoder_model_present_for_this_op {
                let delay_bits = u32::from(buffer_delay_length_minus_1) + 1;
                bits.skip_bits(delay_bits, "decoder_buffer_delay")?;
                bits.skip_bits(delay_bits, "encoder_buffer_delay")?;
                bits.read_bool("low_delay_mode_flag")?;
            }
            let mut initial_display_delay_present_for_this_op = false;
            let mut initial_display_delay_minus_1 = None;
            if initial_display_delay_present_flag {
                initial_display_delay_present_for_this_op =
                    bits.read_bool("initial_display_delay_present_for_this_op")?;
                if initial_display_delay_present_for_this_op {
                    initial_display_delay_minus_1 = Some(native_vulkan_av1_u8(
                        bits.read_bits(4, "initial_display_delay_minus_1")?,
                        "initial_display_delay_minus_1",
                    )?);
                }
            }
            operating_points.push(NativeVulkanAv1OperatingPointSnapshot {
                index,
                idc,
                seq_level_idx,
                seq_level_label: native_vulkan_av1_sequence_level_idx_label(seq_level_idx),
                seq_tier,
                decoder_model_present_for_this_op,
                initial_display_delay_present_for_this_op,
                initial_display_delay_minus_1,
            });
        }
    }

    let frame_width_bits_minus_1 = native_vulkan_av1_u8(
        bits.read_bits(4, "frame_width_bits_minus_1")?,
        "frame_width_bits_minus_1",
    )?;
    let frame_height_bits_minus_1 = native_vulkan_av1_u8(
        bits.read_bits(4, "frame_height_bits_minus_1")?,
        "frame_height_bits_minus_1",
    )?;
    let frame_width_bits = u32::from(frame_width_bits_minus_1) + 1;
    let frame_height_bits = u32::from(frame_height_bits_minus_1) + 1;
    let max_frame_width_minus_1 = bits.read_bits(frame_width_bits, "max_frame_width_minus_1")?;
    let max_frame_height_minus_1 = bits.read_bits(frame_height_bits, "max_frame_height_minus_1")?;
    let max_frame_width = max_frame_width_minus_1
        .checked_add(1)
        .ok_or_else(|| "AV1 max_frame_width overflow".to_owned())?;
    let max_frame_height = max_frame_height_minus_1
        .checked_add(1)
        .ok_or_else(|| "AV1 max_frame_height overflow".to_owned())?;

    let mut delta_frame_id_length_minus_2 = None;
    let mut additional_frame_id_length_minus_1 = None;
    let frame_id_numbers_present_flag = if reduced_still_picture_header {
        false
    } else {
        let present = bits.read_bool("frame_id_numbers_present_flag")?;
        if present {
            delta_frame_id_length_minus_2 = Some(native_vulkan_av1_u8(
                bits.read_bits(4, "delta_frame_id_length_minus_2")?,
                "delta_frame_id_length_minus_2",
            )?);
            additional_frame_id_length_minus_1 = Some(native_vulkan_av1_u8(
                bits.read_bits(3, "additional_frame_id_length_minus_1")?,
                "additional_frame_id_length_minus_1",
            )?);
        }
        present
    };

    let use_128x128_superblock = bits.read_bool("use_128x128_superblock")?;
    let enable_filter_intra = bits.read_bool("enable_filter_intra")?;
    let enable_intra_edge_filter = bits.read_bool("enable_intra_edge_filter")?;

    let (
        enable_interintra_compound,
        enable_masked_compound,
        enable_warped_motion,
        enable_dual_filter,
        enable_order_hint,
        enable_jnt_comp,
        enable_ref_frame_mvs,
        seq_force_screen_content_tools,
        seq_force_integer_mv,
        order_hint_bits_minus_1,
    ) = if reduced_still_picture_header {
        (false, false, false, false, false, false, false, 2, 2, None)
    } else {
        let enable_interintra_compound = bits.read_bool("enable_interintra_compound")?;
        let enable_masked_compound = bits.read_bool("enable_masked_compound")?;
        let enable_warped_motion = bits.read_bool("enable_warped_motion")?;
        let enable_dual_filter = bits.read_bool("enable_dual_filter")?;
        let enable_order_hint = bits.read_bool("enable_order_hint")?;
        let (enable_jnt_comp, enable_ref_frame_mvs) = if enable_order_hint {
            (
                bits.read_bool("enable_jnt_comp")?,
                bits.read_bool("enable_ref_frame_mvs")?,
            )
        } else {
            (false, false)
        };
        let seq_choose_screen_content_tools = bits.read_bool("seq_choose_screen_content_tools")?;
        let seq_force_screen_content_tools = if seq_choose_screen_content_tools {
            2
        } else {
            native_vulkan_av1_u8(
                bits.read_bits(1, "seq_force_screen_content_tools")?,
                "seq_force_screen_content_tools",
            )?
        };
        let seq_force_integer_mv = if seq_force_screen_content_tools > 0 {
            let seq_choose_integer_mv = bits.read_bool("seq_choose_integer_mv")?;
            if seq_choose_integer_mv {
                2
            } else {
                native_vulkan_av1_u8(
                    bits.read_bits(1, "seq_force_integer_mv")?,
                    "seq_force_integer_mv",
                )?
            }
        } else {
            2
        };
        let order_hint_bits_minus_1 = if enable_order_hint {
            Some(native_vulkan_av1_u8(
                bits.read_bits(3, "order_hint_bits_minus_1")?,
                "order_hint_bits_minus_1",
            )?)
        } else {
            None
        };
        (
            enable_interintra_compound,
            enable_masked_compound,
            enable_warped_motion,
            enable_dual_filter,
            enable_order_hint,
            enable_jnt_comp,
            enable_ref_frame_mvs,
            seq_force_screen_content_tools,
            seq_force_integer_mv,
            order_hint_bits_minus_1,
        )
    };

    let enable_superres = bits.read_bool("enable_superres")?;
    let enable_cdef = bits.read_bool("enable_cdef")?;
    let enable_restoration = bits.read_bool("enable_restoration")?;
    let color_config = native_vulkan_parse_av1_color_config(&mut bits, seq_profile)?;
    let film_grain_params_present = bits.read_bool("film_grain_params_present")?;

    let requested_profile_compatible = seq_profile == 0
        && matches!(color_config.bit_depth, 8 | 10)
        && color_config.num_planes == 3
        && color_config.subsampling_x
        && color_config.subsampling_y;
    let vulkan_std_session_parameters_ready = requested_profile_compatible
        && !film_grain_params_present
        && max_frame_width > 0
        && max_frame_height > 0
        && !operating_points.is_empty();

    Ok(NativeVulkanAv1SequenceHeaderSnapshot {
        parser: "native-rust-av1-sequence-header",
        seq_profile,
        seq_profile_label: native_vulkan_av1_profile_label(seq_profile),
        still_picture,
        reduced_still_picture_header,
        timing_info_present_flag,
        timing_info,
        decoder_model_info_present_flag,
        buffer_delay_length_minus_1,
        frame_presentation_time_length_minus_1,
        initial_display_delay_present_flag,
        operating_points_cnt_minus_1,
        operating_points,
        frame_width_bits_minus_1,
        frame_height_bits_minus_1,
        max_frame_width_minus_1,
        max_frame_height_minus_1,
        max_frame_width,
        max_frame_height,
        frame_id_numbers_present_flag,
        delta_frame_id_length_minus_2,
        additional_frame_id_length_minus_1,
        use_128x128_superblock,
        enable_filter_intra,
        enable_intra_edge_filter,
        enable_interintra_compound,
        enable_masked_compound,
        enable_warped_motion,
        enable_dual_filter,
        enable_order_hint,
        enable_jnt_comp,
        enable_ref_frame_mvs,
        seq_force_screen_content_tools,
        seq_force_integer_mv,
        order_hint_bits_minus_1,
        enable_superres,
        enable_cdef,
        enable_restoration,
        film_grain_params_present,
        color_config,
        requested_profile_compatible,
        vulkan_std_session_parameters_ready,
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_av1_color_config(
    bits: &mut NativeVulkanAv1BitReader<'_>,
    seq_profile: u8,
) -> Result<NativeVulkanAv1ColorConfigSnapshot, String> {
    let high_bitdepth = bits.read_bool("high_bitdepth")?;
    let twelve_bit;
    let bit_depth;
    if seq_profile == 2 && high_bitdepth {
        twelve_bit = bits.read_bool("twelve_bit")?;
        bit_depth = if twelve_bit { 12 } else { 10 };
    } else {
        twelve_bit = false;
        bit_depth = if high_bitdepth { 10 } else { 8 };
    }

    let mono_chrome = if seq_profile == 1 {
        false
    } else {
        bits.read_bool("mono_chrome")?
    };
    let num_planes = if mono_chrome { 1 } else { 3 };
    let color_description_present_flag = bits.read_bool("color_description_present_flag")?;
    let (color_primaries, transfer_characteristics, matrix_coefficients) =
        if color_description_present_flag {
            (
                native_vulkan_av1_u8(bits.read_bits(8, "color_primaries")?, "color_primaries")?,
                native_vulkan_av1_u8(
                    bits.read_bits(8, "transfer_characteristics")?,
                    "transfer_characteristics",
                )?,
                native_vulkan_av1_u8(
                    bits.read_bits(8, "matrix_coefficients")?,
                    "matrix_coefficients",
                )?,
            )
        } else {
            (2, 2, 2)
        };

    if mono_chrome {
        let color_range = bits.read_bool("color_range")?;
        return Ok(NativeVulkanAv1ColorConfigSnapshot {
            high_bitdepth,
            twelve_bit,
            mono_chrome,
            color_description_present_flag,
            color_primaries,
            transfer_characteristics,
            matrix_coefficients,
            color_range,
            subsampling_x: true,
            subsampling_y: true,
            chroma_sample_position: 0,
            separate_uv_delta_q: false,
            bit_depth,
            num_planes,
        });
    }

    let mut chroma_sample_position = 0u8;
    let (color_range, subsampling_x, subsampling_y) =
        if color_primaries == 1 && transfer_characteristics == 13 && matrix_coefficients == 0 {
            (true, false, false)
        } else {
            let color_range = bits.read_bool("color_range")?;
            let (subsampling_x, subsampling_y) = match seq_profile {
                0 => (true, true),
                1 => (false, false),
                2 if bit_depth == 12 => {
                    let subsampling_x = bits.read_bool("subsampling_x")?;
                    let subsampling_y = subsampling_x && bits.read_bool("subsampling_y")?;
                    (subsampling_x, subsampling_y)
                }
                2 => (true, false),
                _ => return Err(format!("AV1 seq_profile {seq_profile} is reserved")),
            };
            if subsampling_x && subsampling_y {
                chroma_sample_position = native_vulkan_av1_u8(
                    bits.read_bits(2, "chroma_sample_position")?,
                    "chroma_sample_position",
                )?;
            }
            (color_range, subsampling_x, subsampling_y)
        };
    let separate_uv_delta_q = bits.read_bool("separate_uv_delta_q")?;

    Ok(NativeVulkanAv1ColorConfigSnapshot {
        high_bitdepth,
        twelve_bit,
        mono_chrome,
        color_description_present_flag,
        color_primaries,
        transfer_characteristics,
        matrix_coefficients,
        color_range,
        subsampling_x,
        subsampling_y,
        chroma_sample_position,
        separate_uv_delta_q,
        bit_depth,
        num_planes,
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
struct NativeVulkanAv1BitReader<'a> {
    bytes: &'a [u8],
    bit_offset: usize,
}

#[cfg(any(feature = "native-vulkan-video", test))]
impl<'a> NativeVulkanAv1BitReader<'a> {
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

    fn byte_offset(&self) -> usize {
        self.bit_offset.div_ceil(8)
    }

    fn zero_align_to_byte(&mut self, label: &'static str) -> Result<(), String> {
        while !self.bit_offset.is_multiple_of(8) {
            if self.read_bool(label)? {
                return Err(format!("{label} expected zero padding bit"));
            }
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
            return Err(format!("{label} exceeds AV1 OBU payload length"));
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

    fn read_uvlc(&mut self, label: &'static str) -> Result<u32, String> {
        let mut leading_zero_bits = 0u32;
        while leading_zero_bits < 32 && !self.read_bool(label)? {
            leading_zero_bits += 1;
        }
        if leading_zero_bits == 32 {
            return Ok(u32::MAX);
        }
        if leading_zero_bits == 0 {
            return Ok(0);
        }
        let suffix = self.read_bits(leading_zero_bits, label)?;
        Ok((1u32 << leading_zero_bits) - 1 + suffix)
    }

    fn read_quniform(&mut self, n: u32, label: &'static str) -> Result<u32, String> {
        if n <= 1 {
            return Ok(0);
        }
        let l = 32 - n.leading_zeros();
        let m = (1u32 << l) - n;
        let value = self.read_bits(l - 1, label)?;
        if value < m {
            Ok(value)
        } else {
            let extra = self.read_bits(1, label)?;
            Ok(m + ((value - m) << 1) + extra)
        }
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_u8(value: u32, label: &'static str) -> Result<u8, String> {
    u8::try_from(value).map_err(|_| format!("{label}={value} exceeds u8 range"))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_i8(value: u32, label: &'static str) -> Result<i8, String> {
    i8::try_from(value).map_err(|_| format!("{label}={value} exceeds i8 range"))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_u16(value: u32, label: &'static str) -> Result<u16, String> {
    u16::try_from(value).map_err(|_| format!("{label}={value} exceeds u16 range"))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_read_leb128(bytes: &[u8]) -> Result<(u64, usize), String> {
    let mut value = 0u64;
    for (index, byte) in bytes.iter().copied().take(8).enumerate() {
        value |= u64::from(byte & 0x7f) << (index * 7);
        if byte & 0x80 == 0 {
            return Ok((value, index + 1));
        }
    }
    Err("AV1 LEB128 size field is missing terminator within 8 bytes".to_owned())
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_obu_type_label(obu_type: u8) -> &'static str {
    match obu_type {
        1 => "sequence-header",
        2 => "temporal-delimiter",
        3 => "frame-header",
        4 => "tile-group",
        5 => "metadata",
        6 => "frame",
        7 => "redundant-frame-header",
        8 => "tile-list",
        15 => "padding",
        _ => "reserved",
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_frame_type_label(frame_type: u8) -> &'static str {
    match frame_type {
        0 => "key",
        1 => "inter",
        2 => "intra-only",
        3 => "switch",
        _ => "unknown",
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_interpolation_filter_label(
    filter: vk::video::StdVideoAV1InterpolationFilter,
) -> &'static str {
    match filter {
        vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_EIGHTTAP => "eighttap",
        vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_EIGHTTAP_SMOOTH => "eighttap-smooth",
        vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_EIGHTTAP_SHARP => "eighttap-sharp",
        vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_BILINEAR => "bilinear",
        vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_SWITCHABLE => "switchable",
        _ => "invalid",
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_profile_label(profile: u8) -> &'static str {
    match profile {
        0 => "main",
        1 => "high",
        2 => "professional",
        _ => "reserved",
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_av1_sequence_level_idx_label(level_idx: u8) -> Option<&'static str> {
    match level_idx {
        0 => Some("2.0"),
        1 => Some("2.1"),
        2 => Some("2.2"),
        3 => Some("2.3"),
        4 => Some("3.0"),
        5 => Some("3.1"),
        6 => Some("3.2"),
        7 => Some("3.3"),
        8 => Some("4.0"),
        9 => Some("4.1"),
        10 => Some("4.2"),
        11 => Some("4.3"),
        12 => Some("5.0"),
        13 => Some("5.1"),
        14 => Some("5.2"),
        15 => Some("5.3"),
        16 => Some("6.0"),
        17 => Some("6.1"),
        18 => Some("6.2"),
        19 => Some("6.3"),
        20 => Some("7.0"),
        21 => Some("7.1"),
        22 => Some("7.2"),
        23 => Some("7.3"),
        _ => None,
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
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
                if nal_type <= 31 && stats.first_slice.is_none() {
                    let slice_segment_offset = native_vulkan_h265_annex_b_slice_segment_offset(
                        start_code_offset,
                        payload_offset,
                    );
                    if let Ok(slice_segment_offset) = u32::try_from(slice_segment_offset) {
                        stats.first_slice = Some(NativeVulkanH265SlicePayloadSummary {
                            nal_type,
                            slice_segment_offset,
                            payload_start: payload_offset,
                            payload_end: next_start,
                        });
                    }
                }
            }
        }
        offset = next_start;
    }
    stats
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h264_nal_stats(bytes: &[u8]) -> NativeVulkanH264NalStats {
    let mut stats = NativeVulkanH264NalStats {
        bytes: bytes.len() as u64,
        ..Default::default()
    };
    let mut offset = 0usize;
    while let Some((start_code_offset, payload_offset)) =
        native_vulkan_next_annex_b_start_code(bytes, offset)
    {
        stats.has_annex_b_start_codes = true;
        let next_start = native_vulkan_next_annex_b_start_code(bytes, payload_offset)
            .map(|(next_start, _)| next_start)
            .unwrap_or(bytes.len());
        if payload_offset < next_start
            && let Some(header) = bytes.get(payload_offset).copied()
        {
            let nal_type = header & 0x1f;
            match nal_type {
                1..=5 => {
                    stats.slice_count = stats.slice_count.saturating_add(1);
                    if nal_type == 5 {
                        stats.idr_count = stats.idr_count.saturating_add(1);
                    }
                    let slice_offset =
                        native_vulkan_h264_annex_b_slice_offset(start_code_offset, payload_offset);
                    if let Ok(slice_offset_u32) = u32::try_from(slice_offset) {
                        stats.slice_offsets.push(slice_offset_u32);
                        if stats.first_slice.is_none() {
                            stats.first_slice = Some(NativeVulkanH264SlicePayloadSummary {
                                nal_type,
                                nal_ref_idc: (header >> 5) & 0x03,
                                payload_start: payload_offset,
                                payload_end: next_start,
                            });
                        }
                    }
                }
                7 => stats.sps_count = stats.sps_count.saturating_add(1),
                8 => stats.pps_count = stats.pps_count.saturating_add(1),
                _ => {}
            }
        }
        offset = next_start;
    }
    stats
}

fn native_vulkan_next_annex_b_start_code(bytes: &[u8], from: usize) -> Option<(usize, usize)> {
    let mut index = from.min(bytes.len());
    while index + 3 <= bytes.len() {
        // Match FFmpeg's H.264/H.265 parser shape: first jump to a zero byte,
        // then check whether it starts a three- or four-byte Annex-B prefix.
        // See references/ffmpeg/libavcodec/h2645_parse.c:37-180.
        let zero_offset = native_vulkan_memchr_zero(&bytes[index..])?;
        index = index.saturating_add(zero_offset);
        if index + 3 > bytes.len() {
            return None;
        }
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

#[cfg(any(
    feature = "native-vulkan-renderer",
    feature = "native-vulkan-video",
    test
))]
fn native_vulkan_memchr_zero(bytes: &[u8]) -> Option<usize> {
    #[cfg(target_family = "unix")]
    {
        let ptr = unsafe {
            native_vulkan_c_memchr(bytes.as_ptr().cast::<std::ffi::c_void>(), 0, bytes.len())
        };
        if ptr.is_null() {
            None
        } else {
            let offset = unsafe { ptr.cast::<u8>().offset_from(bytes.as_ptr()) };
            usize::try_from(offset).ok()
        }
    }
    #[cfg(not(target_family = "unix"))]
    {
        bytes.iter().position(|byte| *byte == 0)
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h264_nal_type_label(nal_type: u8) -> &'static str {
    match nal_type {
        1 => "non-idr",
        2 => "data-partition-a",
        3 => "data-partition-b",
        4 => "data-partition-c",
        5 => "idr",
        6 => "sei",
        7 => "sps",
        8 => "pps",
        9 => "aud",
        10 => "end-of-sequence",
        11 => "end-of-stream",
        12 => "filler",
        13 => "sps-extension",
        14 => "prefix",
        15 => "subset-sps",
        19 => "auxiliary-slice",
        20 => "extension-slice",
        21 => "depth-extension-slice",
        22..=23 => "reserved",
        24..=31 => "unspecified",
        _ => "unknown",
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
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

fn native_vulkan_h264_sps_dpb_slot_count(sps: &NativeVulkanH264SpsSnapshot) -> u32 {
    sps.max_num_ref_frames.saturating_add(1).max(1)
}

fn native_vulkan_h265_sps_dpb_slot_count(sps: &NativeVulkanH265SpsSnapshot) -> u32 {
    let layer_count = usize::from(sps.max_sub_layers_minus1).saturating_add(1);
    sps.dec_pic_buf_mgr
        .max_dec_pic_buffering_minus1
        .iter()
        .take(layer_count.min(sps.dec_pic_buf_mgr.max_dec_pic_buffering_minus1.len()))
        .copied()
        .max()
        .map(|value| u32::from(value).saturating_add(1))
        .unwrap_or(1)
        .max(1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeVulkanWallpaperType {
    StaticImage,
    Video,
    Web,
    Scene,
    Shader,
    Playlist,
}

pub const WALLPAPER_TYPE_CONTRACT: &[NativeVulkanWallpaperType] = &[
    NativeVulkanWallpaperType::StaticImage,
    NativeVulkanWallpaperType::Video,
    NativeVulkanWallpaperType::Web,
    NativeVulkanWallpaperType::Scene,
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
            current_renderer_status: "--run-static lowers static images into a single scene sampled-image layer, then uses Vulkanalia sampled-image dynamic rendering; ash static session and staging-copy runtime are removed",
            target_vulkan_path: "decode image once -> retained sampled Vulkan image -> fit-aware dynamic-rendering pass shared with scene/image layers",
        },
        NativeVulkanWallpaperTypeSupport {
            wallpaper_type: NativeVulkanWallpaperType::Video,
            current_vulkan_item: true,
            current_renderer_status: "--run-video routes H.264/H.265 through Vulkanalia streaming decode/present; AV1 waits for the continuous streaming runtime",
            target_vulkan_path: "container demux/parser -> Vulkan Video bitstream/session parameters -> decoded NV12/P010 image -> Vulkan YUV sampling; importer paths remain fallback/comparison",
        },
        NativeVulkanWallpaperTypeSupport {
            wallpaper_type: NativeVulkanWallpaperType::Web,
            current_vulkan_item: false,
            current_renderer_status: "helper contract only; current render plan may fall back to static image",
            target_vulkan_path: "Web helper -> DMABuf/EGLImage/shared-frame handoff -> Vulkan composite",
        },
        NativeVulkanWallpaperTypeSupport {
            wallpaper_type: NativeVulkanWallpaperType::Scene,
            current_vulkan_item: true,
            current_renderer_status: "deterministic scene snapshot layers carried by Vulkan render item; static images lower into single-image scene layers; native draw-pass plan, fast-clear color path, color/rectangle quads and sampled-image geometry exist, text/path rasterization remains pending",
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
    pub video_pipeline: pipeline::NativeVulkanVideoPipelineContract,
    pub video_flow: video_flow::NativeVulkanVideoFlowContract,
    pub video_interop: NativeVulkanVideoInteropContract,
    pub web_interop: NativeVulkanWebInteropContract,
    pub vulkan_backend: NativeVulkanBackendPlan,
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
        video_pipeline: pipeline::native_vulkan_video_pipeline_contract(),
        video_flow: video_flow::native_vulkan_video_flow_contract(),
        video_interop: video_interop_contract(),
        web_interop: web_interop_contract(),
        vulkan_backend: native_vulkan_backend_plan(),
    }
}

pub fn required_instance_extensions() -> Vec<&'static str> {
    vec!["VK_KHR_surface", "VK_KHR_wayland_surface"]
}

pub fn required_device_extensions() -> Vec<&'static str> {
    vec![
        "VK_KHR_swapchain",
        "VK_KHR_external_memory_fd",
        "VK_KHR_external_semaphore_fd",
        "VK_KHR_timeline_semaphore",
        "VK_EXT_external_memory_dma_buf",
        "VK_EXT_image_drm_format_modifier",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::{
        SceneDisplayPlan, SceneRenderLayer, SceneWallpaperPlan, StaticRenderSyncPlan,
    };
    use std::path::PathBuf;

    #[cfg(feature = "native-vulkan-video")]
    struct NativeVulkanTestDecodeReadbackLayout {
        format: &'static str,
        y_plane_bytes: u64,
        uv_plane_bytes: u64,
        size: u64,
    }

    #[cfg(feature = "native-vulkan-video")]
    struct NativeVulkanTestDecodedPlaneFormats {
        y_view_format: vk::Format,
        uv_view_format: vk::Format,
    }

    #[cfg(feature = "native-vulkan-video")]
    fn native_vulkan_video_decode_readback_layout(
        format: vk::Format,
        extent: vk::Extent2D,
    ) -> Option<NativeVulkanTestDecodeReadbackLayout> {
        let pixels = u64::from(extent.width).checked_mul(u64::from(extent.height))?;
        match format {
            vk::Format::G8_B8R8_2PLANE_420_UNORM => Some(NativeVulkanTestDecodeReadbackLayout {
                format: "G8_B8R8_2PLANE_420_UNORM",
                y_plane_bytes: pixels,
                uv_plane_bytes: pixels / 2,
                size: pixels * 3 / 2,
            }),
            vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16 => {
                Some(NativeVulkanTestDecodeReadbackLayout {
                    format: "G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16",
                    y_plane_bytes: pixels * 2,
                    uv_plane_bytes: pixels,
                    size: pixels * 3,
                })
            }
            _ => None,
        }
    }

    #[cfg(feature = "native-vulkan-video")]
    fn native_vulkan_decoded_video_plane_formats(
        format: vk::Format,
    ) -> Option<NativeVulkanTestDecodedPlaneFormats> {
        match format {
            vk::Format::G8_B8R8_2PLANE_420_UNORM => Some(NativeVulkanTestDecodedPlaneFormats {
                y_view_format: vk::Format::R8_UNORM,
                uv_view_format: vk::Format::R8G8_UNORM,
            }),
            vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16 => {
                Some(NativeVulkanTestDecodedPlaneFormats {
                    y_view_format: vk::Format::R16_UNORM,
                    uv_view_format: vk::Format::R16G16_UNORM,
                })
            }
            _ => None,
        }
    }

    #[cfg(feature = "native-vulkan-video")]
    fn native_vulkan_h264_reference_info_flags(
        field_pic_flag: bool,
        bottom_field_flag: bool,
        used_for_long_term_reference: bool,
        non_existing: bool,
    ) -> vk::video::StdVideoDecodeH264ReferenceInfoFlags {
        let top_field_flag = field_pic_flag && !bottom_field_flag;
        let bottom_field_flag = field_pic_flag && bottom_field_flag;
        vk::video::StdVideoDecodeH264ReferenceInfoFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::video::StdVideoDecodeH264ReferenceInfoFlags::new_bitfield_1(
                native_vulkan_bool_u32(top_field_flag),
                native_vulkan_bool_u32(bottom_field_flag),
                native_vulkan_bool_u32(used_for_long_term_reference),
                native_vulkan_bool_u32(non_existing),
            ),
            __bindgen_padding_0: [0; 3],
        }
    }

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

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn sizes_video_decode_readback_layouts_by_output_format() {
        let extent = vk::Extent2D {
            width: 3840,
            height: 2160,
        };

        let nv12 = native_vulkan_video_decode_readback_layout(
            vk::Format::G8_B8R8_2PLANE_420_UNORM,
            extent,
        )
        .expect("NV12 readback layout should be supported");
        assert_eq!(nv12.format, "G8_B8R8_2PLANE_420_UNORM");
        assert_eq!(nv12.y_plane_bytes, 3840 * 2160);
        assert_eq!(nv12.uv_plane_bytes, 3840 * 2160 / 2);
        assert_eq!(nv12.size, 3840 * 2160 * 3 / 2);

        let p010 = native_vulkan_video_decode_readback_layout(
            vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16,
            extent,
        )
        .expect("P010 readback layout should be supported");
        assert_eq!(p010.format, "G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16");
        assert_eq!(p010.y_plane_bytes, 3840 * 2160 * 2);
        assert_eq!(p010.uv_plane_bytes, 3840 * 2160);
        assert_eq!(p010.size, 3840 * 2160 * 3);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn chooses_decoded_plane_view_formats_by_output_format() {
        let nv12 = native_vulkan_decoded_video_plane_formats(vk::Format::G8_B8R8_2PLANE_420_UNORM)
            .expect("NV12 plane view formats should be supported");
        assert_eq!(nv12.y_view_format, vk::Format::R8_UNORM);
        assert_eq!(nv12.uv_view_format, vk::Format::R8G8_UNORM);

        let p010 = native_vulkan_decoded_video_plane_formats(
            vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16,
        )
        .expect("P010 plane view formats should be supported");
        assert_eq!(p010.y_view_format, vk::Format::R16_UNORM);
        assert_eq!(p010.uv_view_format, vk::Format::R16G16_UNORM);
    }

    #[test]
    fn parses_native_vulkan_video_session_main10_codecs() {
        assert_eq!(
            "h264".parse::<NativeVulkanVideoSessionCodec>(),
            Ok(NativeVulkanVideoSessionCodec::H264High8)
        );
        assert_eq!(
            "h264-high-8".parse::<NativeVulkanVideoSessionCodec>(),
            Ok(NativeVulkanVideoSessionCodec::H264High8)
        );
        assert_eq!(
            "h265-main-10".parse::<NativeVulkanVideoSessionCodec>(),
            Ok(NativeVulkanVideoSessionCodec::H265Main10)
        );
        assert_eq!(
            "hevc-main-10".parse::<NativeVulkanVideoSessionCodec>(),
            Ok(NativeVulkanVideoSessionCodec::H265Main10)
        );
        assert_eq!(
            "av1-main-10".parse::<NativeVulkanVideoSessionCodec>(),
            Ok(NativeVulkanVideoSessionCodec::Av1Main10)
        );
        assert_eq!(
            NativeVulkanVideoSessionCodec::H265Main10.label(),
            "h265-main-10"
        );
        assert_eq!(
            NativeVulkanVideoSessionCodec::H264High8.label(),
            "h264-high-8"
        );
        assert_eq!(
            NativeVulkanVideoSessionCodec::H264High8.profile_label(),
            "high-8"
        );
        assert_eq!(
            NativeVulkanVideoSessionCodec::Av1Main10.profile_label(),
            "main-10"
        );
    }

    #[test]
    fn scans_h264_annex_b_parameter_sets_and_idr() {
        let bytes = [
            0, 0, 0, 1, 0x67, 0x64, 0x00, 0x2a, 0, 0, 1, 0x68, 0xee, 0x3c, 0x80, 0, 0, 1, 0x65,
            0x88, 0x84, 0, 0, 1, 0x41, 0x9a,
        ];

        let stats = native_vulkan_h264_nal_stats(&bytes);

        assert_eq!(stats.bytes, bytes.len() as u64);
        assert!(stats.has_annex_b_start_codes);
        assert_eq!(stats.sps_count, 1);
        assert_eq!(stats.pps_count, 1);
        assert_eq!(stats.idr_count, 1);
        assert_eq!(stats.slice_count, 2);
        assert!(stats.parameter_sets_present());
    }

    #[test]
    fn parses_h264_high_sps_pps_for_vulkan_std_subset() {
        let bytes = [
            0x00, 0x00, 0x00, 0x01, 0x67, 0x64, 0x00, 0x2a, 0xac, 0xb4, 0x02, 0x80, 0x2d, 0xd8,
            0x08, 0x80, 0x00, 0x00, 0x03, 0x00, 0x80, 0x00, 0x00, 0x3c, 0x47, 0x8c, 0x19, 0x50,
            0x00, 0x00, 0x00, 0x01, 0x68, 0xef, 0x0f, 0xcb,
        ];

        let parameter_sets = native_vulkan_parse_h264_parameter_sets(&bytes).unwrap();

        assert_eq!(parameter_sets.parser, "native-rust-h264-sps-pps");
        assert_eq!(parameter_sets.sps.profile_idc, 100);
        assert_eq!(parameter_sets.sps.profile_label, "high");
        assert_eq!(parameter_sets.sps.level_idc, 42);
        assert_eq!(parameter_sets.sps.width, 1280);
        assert_eq!(parameter_sets.sps.height, 720);
        assert_eq!(parameter_sets.sps.chroma_format_idc, 1);
        assert_eq!(parameter_sets.sps.pic_order_cnt_type, 2);
        assert_eq!(parameter_sets.pps.id, 0);
        assert_eq!(parameter_sets.pps.sps_id, 0);
        assert!(parameter_sets.pps.entropy_coding_mode_flag);
        assert!(parameter_sets.pps.transform_8x8_mode_flag);
        assert!(parameter_sets.pps.weighted_pred_flag);
        assert!(parameter_sets.requested_profile_compatible);
        assert!(parameter_sets.vulkan_std_session_parameters_ready);
    }

    #[test]
    fn parses_h264_first_idr_slice_for_direct_decode() {
        fn push_bits(bits: &mut Vec<bool>, value: u32, count: u32) {
            for shift in (0..count).rev() {
                bits.push(((value >> shift) & 1) != 0);
            }
        }
        fn push_ue(bits: &mut Vec<bool>, value: u32) {
            let code_num = value + 1;
            let bit_count = 32 - code_num.leading_zeros();
            for _ in 0..bit_count.saturating_sub(1) {
                bits.push(false);
            }
            push_bits(bits, code_num, bit_count);
        }
        fn pack_rbsp(mut bits: Vec<bool>) -> Vec<u8> {
            bits.push(true);
            while !bits.len().is_multiple_of(8) {
                bits.push(false);
            }
            let mut bytes = vec![0u8; bits.len() / 8];
            for (index, bit) in bits.into_iter().enumerate() {
                if bit {
                    bytes[index / 8] |= 1 << (7 - (index % 8));
                }
            }
            bytes
        }

        let mut access_unit = vec![
            0x00, 0x00, 0x00, 0x01, 0x67, 0x64, 0x00, 0x2a, 0xac, 0xb4, 0x02, 0x80, 0x2d, 0xd8,
            0x08, 0x80, 0x00, 0x00, 0x03, 0x00, 0x80, 0x00, 0x00, 0x3c, 0x47, 0x8c, 0x19, 0x50,
            0x00, 0x00, 0x00, 0x01, 0x68, 0xef, 0x0f, 0xcb,
        ];
        let parameter_sets = native_vulkan_parse_h264_parameter_sets(&access_unit).unwrap();
        let slice_start_code_offset = access_unit.len();
        let mut slice_bits = Vec::new();
        push_ue(&mut slice_bits, 0); // first_mb_in_slice
        push_ue(&mut slice_bits, 2); // I-slice
        push_ue(&mut slice_bits, parameter_sets.pps.id);
        push_bits(
            &mut slice_bits,
            0,
            parameter_sets.sps.log2_max_frame_num_minus4 + 4,
        );
        push_ue(&mut slice_bits, 0); // idr_pic_id
        slice_bits.push(false); // no_output_of_prior_pics_flag
        slice_bits.push(false); // long_term_reference_flag
        access_unit.extend_from_slice(&[0x00, 0x00, 0x00, 0x01, 0x65]);
        access_unit.extend_from_slice(&pack_rbsp(slice_bits));

        let first_frame =
            native_vulkan_h264_first_frame_decode_info(&access_unit, &parameter_sets).unwrap();

        assert_eq!(first_frame.nal_type_label, "idr");
        assert!(first_frame.idr);
        assert!(first_frame.irap);
        assert!(first_frame.is_intra);
        assert!(first_frame.is_reference);
        assert_eq!(first_frame.pps_id, parameter_sets.pps.id);
        assert_eq!(first_frame.frame_num, 0);
        assert_eq!(first_frame.idr_pic_id, 0);
        assert_eq!(first_frame.pic_order_cnt, [0, 0]);
        assert_eq!(
            first_frame.slice_offsets,
            vec![(slice_start_code_offset + 1) as u32]
        );
    }

    #[test]
    fn parses_h264_weighted_p_slice_header_for_vulkanalia_extractor() {
        fn push_bits(bits: &mut Vec<bool>, value: u32, count: u32) {
            for shift in (0..count).rev() {
                bits.push(((value >> shift) & 1) != 0);
            }
        }
        fn push_ue(bits: &mut Vec<bool>, value: u32) {
            let code_num = value + 1;
            let bit_count = 32 - code_num.leading_zeros();
            for _ in 0..bit_count.saturating_sub(1) {
                bits.push(false);
            }
            push_bits(bits, code_num, bit_count);
        }
        fn push_se(bits: &mut Vec<bool>, value: i32) {
            let code_num = if value <= 0 {
                value.unsigned_abs() * 2
            } else {
                value as u32 * 2 - 1
            };
            push_ue(bits, code_num);
        }
        fn pack_rbsp(mut bits: Vec<bool>) -> Vec<u8> {
            bits.push(true);
            while !bits.len().is_multiple_of(8) {
                bits.push(false);
            }
            let mut bytes = vec![0u8; bits.len() / 8];
            for (index, bit) in bits.into_iter().enumerate() {
                if bit {
                    bytes[index / 8] |= 1 << (7 - (index % 8));
                }
            }
            bytes
        }

        let mut access_unit = vec![
            0x00, 0x00, 0x00, 0x01, 0x67, 0x64, 0x00, 0x2a, 0xac, 0xb4, 0x02, 0x80, 0x2d, 0xd8,
            0x08, 0x80, 0x00, 0x00, 0x03, 0x00, 0x80, 0x00, 0x00, 0x3c, 0x47, 0x8c, 0x19, 0x50,
            0x00, 0x00, 0x00, 0x01, 0x68, 0xef, 0x0f, 0xcb,
        ];
        let parameter_sets = native_vulkan_parse_h264_parameter_sets(&access_unit).unwrap();
        assert!(parameter_sets.pps.weighted_pred_flag);
        assert_eq!(parameter_sets.sps.pic_order_cnt_type, 2);

        let mut slice_bits = Vec::new();
        push_ue(&mut slice_bits, 0); // first_mb_in_slice
        push_ue(&mut slice_bits, 0); // P-slice
        push_ue(&mut slice_bits, parameter_sets.pps.id);
        push_bits(
            &mut slice_bits,
            1,
            parameter_sets.sps.log2_max_frame_num_minus4 + 4,
        );
        if parameter_sets.pps.redundant_pic_cnt_present_flag {
            push_ue(&mut slice_bits, 0);
        }
        slice_bits.push(true); // num_ref_idx_active_override_flag
        push_ue(&mut slice_bits, 0); // num_ref_idx_l0_active_minus1
        slice_bits.push(false); // ref_pic_list_modification_flag_l0
        push_ue(&mut slice_bits, 0); // luma_log2_weight_denom
        if native_vulkan_h264_chroma_array_type(&parameter_sets.sps) != 0 {
            push_ue(&mut slice_bits, 0); // chroma_log2_weight_denom
        }
        slice_bits.push(true); // luma_weight_l0_flag
        push_se(&mut slice_bits, 1); // luma_weight_l0[0]
        push_se(&mut slice_bits, 0); // luma_offset_l0[0]
        if native_vulkan_h264_chroma_array_type(&parameter_sets.sps) != 0 {
            slice_bits.push(true); // chroma_weight_l0_flag
            push_se(&mut slice_bits, 1); // chroma_weight_l0[0][0]
            push_se(&mut slice_bits, 0); // chroma_offset_l0[0][0]
            push_se(&mut slice_bits, 1); // chroma_weight_l0[0][1]
            push_se(&mut slice_bits, 0); // chroma_offset_l0[0][1]
        }
        slice_bits.push(false); // adaptive_ref_pic_marking_mode_flag
        access_unit.extend_from_slice(&[0x00, 0x00, 0x00, 0x01, 0x61]);
        access_unit.extend_from_slice(&pack_rbsp(slice_bits));

        let picture = native_vulkan_h264_picture_decode_info(&access_unit, &parameter_sets, 1)
            .expect("weighted P-slice header should parse");

        assert!(picture.is_p);
        assert_eq!(picture.frame_num, 1);
        assert_eq!(picture.num_ref_idx_l0_active_minus1, Some(0));
        assert!(!picture.ref_pic_list_modification_l0);
        assert!(!picture.adaptive_ref_pic_marking_mode_flag);
        assert_eq!(picture.pic_order_cnt, [1, 1]);
    }

    #[test]
    fn parses_h264_b_slice_l1_ref_list_modification_for_vulkanalia_extractor() {
        fn push_bits(bits: &mut Vec<bool>, value: u32, count: u32) {
            for shift in (0..count).rev() {
                bits.push(((value >> shift) & 1) != 0);
            }
        }
        fn push_ue(bits: &mut Vec<bool>, value: u32) {
            let code_num = value + 1;
            let bit_count = 32 - code_num.leading_zeros();
            for _ in 0..bit_count.saturating_sub(1) {
                bits.push(false);
            }
            push_bits(bits, code_num, bit_count);
        }
        fn pack_rbsp(mut bits: Vec<bool>) -> Vec<u8> {
            bits.push(true);
            while !bits.len().is_multiple_of(8) {
                bits.push(false);
            }
            let mut bytes = vec![0u8; bits.len() / 8];
            for (index, bit) in bits.into_iter().enumerate() {
                if bit {
                    bytes[index / 8] |= 1 << (7 - (index % 8));
                }
            }
            bytes
        }

        let mut access_unit = vec![
            0x00, 0x00, 0x00, 0x01, 0x67, 0x64, 0x00, 0x2a, 0xac, 0xb4, 0x02, 0x80, 0x2d, 0xd8,
            0x08, 0x80, 0x00, 0x00, 0x03, 0x00, 0x80, 0x00, 0x00, 0x3c, 0x47, 0x8c, 0x19, 0x50,
            0x00, 0x00, 0x00, 0x01, 0x68, 0xef, 0x0f, 0xcb,
        ];
        let parameter_sets = native_vulkan_parse_h264_parameter_sets(&access_unit).unwrap();
        assert_eq!(parameter_sets.sps.pic_order_cnt_type, 2);

        let mut slice_bits = Vec::new();
        push_ue(&mut slice_bits, 0); // first_mb_in_slice
        push_ue(&mut slice_bits, 1); // B-slice
        push_ue(&mut slice_bits, parameter_sets.pps.id);
        push_bits(
            &mut slice_bits,
            3,
            parameter_sets.sps.log2_max_frame_num_minus4 + 4,
        );
        if parameter_sets.pps.redundant_pic_cnt_present_flag {
            push_ue(&mut slice_bits, 0);
        }
        slice_bits.push(false); // direct_spatial_mv_pred_flag
        slice_bits.push(true); // num_ref_idx_active_override_flag
        push_ue(&mut slice_bits, 0); // num_ref_idx_l0_active_minus1
        push_ue(&mut slice_bits, 0); // num_ref_idx_l1_active_minus1
        slice_bits.push(false); // ref_pic_list_modification_flag_l0
        slice_bits.push(true); // ref_pic_list_modification_flag_l1
        push_ue(&mut slice_bits, 0); // modification_of_pic_nums_idc: short-term subtract
        push_ue(&mut slice_bits, 2); // abs_diff_pic_num_minus1
        push_ue(&mut slice_bits, 3); // end
        if parameter_sets.pps.weighted_bipred_idc == 1 {
            push_ue(&mut slice_bits, 0); // luma_log2_weight_denom
            if native_vulkan_h264_chroma_array_type(&parameter_sets.sps) != 0 {
                push_ue(&mut slice_bits, 0); // chroma_log2_weight_denom
            }
            slice_bits.push(false); // luma_weight_l0_flag
            if native_vulkan_h264_chroma_array_type(&parameter_sets.sps) != 0 {
                slice_bits.push(false); // chroma_weight_l0_flag
            }
            slice_bits.push(false); // luma_weight_l1_flag
            if native_vulkan_h264_chroma_array_type(&parameter_sets.sps) != 0 {
                slice_bits.push(false); // chroma_weight_l1_flag
            }
        }
        slice_bits.push(false); // adaptive_ref_pic_marking_mode_flag
        access_unit.extend_from_slice(&[0x00, 0x00, 0x00, 0x01, 0x61]);
        access_unit.extend_from_slice(&pack_rbsp(slice_bits));

        let picture = native_vulkan_h264_picture_decode_info(&access_unit, &parameter_sets, 1)
            .expect("B-slice L1 modification header should parse");

        assert!(picture.is_b);
        assert_eq!(picture.frame_num, 3);
        assert_eq!(picture.num_ref_idx_l0_active_minus1, Some(0));
        assert_eq!(picture.num_ref_idx_l1_active_minus1, Some(0));
        assert!(!picture.ref_pic_list_modification_l0);
        assert!(picture.ref_pic_list_modification_l1);
        assert_eq!(
            picture.ref_pic_list_modifications_l1,
            vec![NativeVulkanH264RefPicListModificationSnapshot {
                modification_of_pic_nums_idc: 0,
                abs_diff_pic_num_minus1: Some(2),
                long_term_pic_num: None,
            }]
        );
    }

    #[test]
    fn parses_av1_sequence_header_obu_for_vulkan_std_subset() {
        fn push_bits(bits: &mut Vec<bool>, value: u32, count: u32) {
            for shift in (0..count).rev() {
                bits.push(((value >> shift) & 1) != 0);
            }
        }
        fn pack_bits(bits: &[bool]) -> Vec<u8> {
            let mut bytes = vec![0u8; bits.len().div_ceil(8)];
            for (index, bit) in bits.iter().copied().enumerate() {
                if bit {
                    bytes[index / 8] |= 1 << (7 - (index % 8));
                }
            }
            bytes
        }

        let mut bits = Vec::new();
        push_bits(&mut bits, 0, 3); // seq_profile Main
        push_bits(&mut bits, 0, 1); // still_picture
        push_bits(&mut bits, 0, 1); // reduced_still_picture_header
        push_bits(&mut bits, 0, 1); // timing_info_present_flag
        push_bits(&mut bits, 0, 1); // initial_display_delay_present_flag
        push_bits(&mut bits, 0, 5); // operating_points_cnt_minus_1
        push_bits(&mut bits, 0, 12); // operating_point_idc
        push_bits(&mut bits, 4, 5); // seq_level_idx 3.0
        push_bits(&mut bits, 9, 4); // frame_width_bits_minus_1
        push_bits(&mut bits, 8, 4); // frame_height_bits_minus_1
        push_bits(&mut bits, 639, 10); // max_frame_width_minus_1
        push_bits(&mut bits, 367, 9); // max_frame_height_minus_1
        push_bits(&mut bits, 0, 1); // frame_id_numbers_present_flag
        push_bits(&mut bits, 0, 1); // use_128x128_superblock
        push_bits(&mut bits, 1, 1); // enable_filter_intra
        push_bits(&mut bits, 1, 1); // enable_intra_edge_filter
        push_bits(&mut bits, 1, 1); // enable_interintra_compound
        push_bits(&mut bits, 1, 1); // enable_masked_compound
        push_bits(&mut bits, 1, 1); // enable_warped_motion
        push_bits(&mut bits, 1, 1); // enable_dual_filter
        push_bits(&mut bits, 1, 1); // enable_order_hint
        push_bits(&mut bits, 1, 1); // enable_jnt_comp
        push_bits(&mut bits, 1, 1); // enable_ref_frame_mvs
        push_bits(&mut bits, 1, 1); // seq_choose_screen_content_tools
        push_bits(&mut bits, 1, 1); // seq_choose_integer_mv
        push_bits(&mut bits, 6, 3); // order_hint_bits_minus_1
        push_bits(&mut bits, 0, 1); // enable_superres
        push_bits(&mut bits, 1, 1); // enable_cdef
        push_bits(&mut bits, 1, 1); // enable_restoration
        push_bits(&mut bits, 0, 1); // high_bitdepth
        push_bits(&mut bits, 0, 1); // mono_chrome
        push_bits(&mut bits, 0, 1); // color_description_present_flag
        push_bits(&mut bits, 0, 1); // color_range
        push_bits(&mut bits, 0, 2); // chroma_sample_position
        push_bits(&mut bits, 0, 1); // separate_uv_delta_q
        push_bits(&mut bits, 0, 1); // film_grain_params_present

        let payload = pack_bits(&bits);
        let mut obu = Vec::with_capacity(payload.len() + 2);
        obu.push(0x0a); // sequence-header OBU with size field
        obu.push(payload.len() as u8);
        obu.extend_from_slice(&payload);

        let stats = native_vulkan_av1_obu_stats(&obu).unwrap();
        let sequence_header = stats.sequence_header.as_ref().unwrap();

        assert_eq!(stats.sequence_header_count, 1);
        assert_eq!(sequence_header.seq_profile_label, "main");
        assert_eq!(sequence_header.max_frame_width, 640);
        assert_eq!(sequence_header.max_frame_height, 368);
        assert_eq!(sequence_header.color_config.bit_depth, 8);
        assert!(sequence_header.color_config.subsampling_x);
        assert!(sequence_header.color_config.subsampling_y);
        assert!(sequence_header.vulkan_std_session_parameters_ready);
        assert!(!stats.decode_candidate());

        let mut obu_with_frame = obu.clone();
        let frame_obu_offset = obu_with_frame.len() as u64;
        obu_with_frame.push(0x32); // frame OBU with size field
        obu_with_frame.push(1);
        obu_with_frame.push(0);
        let stats_with_frame = native_vulkan_av1_obu_stats(&obu_with_frame).unwrap();
        assert_eq!(stats_with_frame.frame_count, 1);
        assert_eq!(stats_with_frame.frame_payload_bytes, 1);
        assert_eq!(
            stats_with_frame.first_frame_header_obu_offset,
            Some(frame_obu_offset)
        );
        assert!(stats_with_frame.decode_candidate());
        assert!(stats_with_frame.first_frame_submit.is_some());
        assert!(
            !stats_with_frame
                .first_frame_submit
                .as_ref()
                .unwrap()
                .vulkan_submit_candidate
        );
    }

    #[test]
    fn parses_av1_inter_frame_reference_indices_for_planning() {
        fn push_bits(bits: &mut Vec<bool>, value: u32, count: u32) {
            for shift in (0..count).rev() {
                bits.push(((value >> shift) & 1) != 0);
            }
        }
        fn pack_bits(bits: &[bool]) -> Vec<u8> {
            let mut bytes = vec![0u8; bits.len().div_ceil(8)];
            for (index, bit) in bits.iter().copied().enumerate() {
                if bit {
                    bytes[index / 8] |= 1 << (7 - (index % 8));
                }
            }
            bytes
        }
        fn push_obu(bytes: &mut Vec<u8>, obu_type: u8, payload: &[u8]) {
            bytes.push((obu_type << 3) | 0x02);
            bytes.push(payload.len() as u8);
            bytes.extend_from_slice(payload);
        }

        let mut sequence_bits = Vec::new();
        push_bits(&mut sequence_bits, 0, 3); // seq_profile Main
        push_bits(&mut sequence_bits, 0, 1); // still_picture
        push_bits(&mut sequence_bits, 0, 1); // reduced_still_picture_header
        push_bits(&mut sequence_bits, 0, 1); // timing_info_present_flag
        push_bits(&mut sequence_bits, 0, 1); // initial_display_delay_present_flag
        push_bits(&mut sequence_bits, 0, 5); // operating_points_cnt_minus_1
        push_bits(&mut sequence_bits, 0, 12); // operating_point_idc
        push_bits(&mut sequence_bits, 4, 5); // seq_level_idx 3.0
        push_bits(&mut sequence_bits, 9, 4); // frame_width_bits_minus_1
        push_bits(&mut sequence_bits, 8, 4); // frame_height_bits_minus_1
        push_bits(&mut sequence_bits, 639, 10); // max_frame_width_minus_1
        push_bits(&mut sequence_bits, 367, 9); // max_frame_height_minus_1
        push_bits(&mut sequence_bits, 0, 1); // frame_id_numbers_present_flag
        push_bits(&mut sequence_bits, 0, 1); // use_128x128_superblock
        push_bits(&mut sequence_bits, 1, 1); // enable_filter_intra
        push_bits(&mut sequence_bits, 1, 1); // enable_intra_edge_filter
        push_bits(&mut sequence_bits, 0, 1); // enable_interintra_compound
        push_bits(&mut sequence_bits, 0, 1); // enable_masked_compound
        push_bits(&mut sequence_bits, 0, 1); // enable_warped_motion
        push_bits(&mut sequence_bits, 0, 1); // enable_dual_filter
        push_bits(&mut sequence_bits, 1, 1); // enable_order_hint
        push_bits(&mut sequence_bits, 0, 1); // enable_jnt_comp
        push_bits(&mut sequence_bits, 0, 1); // enable_ref_frame_mvs
        push_bits(&mut sequence_bits, 0, 1); // seq_choose_screen_content_tools
        push_bits(&mut sequence_bits, 0, 1); // seq_force_screen_content_tools
        push_bits(&mut sequence_bits, 6, 3); // order_hint_bits_minus_1
        push_bits(&mut sequence_bits, 0, 1); // enable_superres
        push_bits(&mut sequence_bits, 0, 1); // enable_cdef
        push_bits(&mut sequence_bits, 0, 1); // enable_restoration
        push_bits(&mut sequence_bits, 0, 1); // high_bitdepth
        push_bits(&mut sequence_bits, 0, 1); // mono_chrome
        push_bits(&mut sequence_bits, 0, 1); // color_description_present_flag
        push_bits(&mut sequence_bits, 0, 1); // color_range
        push_bits(&mut sequence_bits, 0, 2); // chroma_sample_position
        push_bits(&mut sequence_bits, 0, 1); // separate_uv_delta_q
        push_bits(&mut sequence_bits, 0, 1); // film_grain_params_present

        let mut frame_bits = Vec::new();
        push_bits(&mut frame_bits, 0, 1); // show_existing_frame
        push_bits(&mut frame_bits, 1, 2); // frame_type inter
        push_bits(&mut frame_bits, 1, 1); // show_frame
        push_bits(&mut frame_bits, 1, 1); // error_resilient_mode
        push_bits(&mut frame_bits, 1, 1); // disable_cdf_update
        push_bits(&mut frame_bits, 0, 1); // frame_size_override_flag
        push_bits(&mut frame_bits, 5, 7); // order_hint
        push_bits(&mut frame_bits, 0x01, 8); // refresh_frame_flags
        for value in 0..8 {
            push_bits(&mut frame_bits, value, 7); // ref_order_hint
        }
        push_bits(&mut frame_bits, 0, 1); // frame_refs_short_signaling
        for value in 0..7 {
            push_bits(&mut frame_bits, value, 3); // ref_frame_idx
        }

        let mut obu = Vec::new();
        push_obu(&mut obu, 1, &pack_bits(&sequence_bits));
        push_obu(&mut obu, 6, &pack_bits(&frame_bits));

        let stats = native_vulkan_av1_obu_stats(&obu).unwrap();
        let submit = stats.first_frame_submit.as_ref().unwrap();

        assert_eq!(submit.frame_type_label, "inter");
        assert!(submit.found_frame_header);
        assert!(!submit.vulkan_submit_candidate);
        assert_eq!(submit.order_hint, Some(5));
        assert_eq!(submit.refresh_frame_flags, 0x01);
        assert_eq!(submit.reference_order_hints, vec![0, 1, 2, 3, 4, 5, 6, 7]);
        assert!(!submit.frame_refs_short_signaling);
        assert_eq!(submit.ref_frame_indices, vec![0, 1, 2, 3, 4, 5, 6]);
        assert!(
            submit
                .unsupported_reason
                .as_deref()
                .unwrap_or_default()
                .contains("reference indices parsed")
        );
    }

    #[test]
    fn parses_av1_allow_warped_motion_before_reduced_tx_set() {
        fn push_bits(bits: &mut Vec<bool>, value: u32, count: u32) {
            for shift in (0..count).rev() {
                bits.push(((value >> shift) & 1) != 0);
            }
        }
        fn pack_bits(bits: &[bool]) -> Vec<u8> {
            let mut bytes = vec![0u8; bits.len().div_ceil(8)];
            for (index, bit) in bits.iter().copied().enumerate() {
                if bit {
                    bytes[index / 8] |= 1 << (7 - (index % 8));
                }
            }
            bytes
        }
        fn push_obu(bytes: &mut Vec<u8>, obu_type: u8, payload: &[u8]) {
            bytes.push((obu_type << 3) | 0x02);
            bytes.push(payload.len() as u8);
            bytes.extend_from_slice(payload);
        }

        let mut sequence_bits = Vec::new();
        push_bits(&mut sequence_bits, 0, 3); // seq_profile Main
        push_bits(&mut sequence_bits, 0, 1); // still_picture
        push_bits(&mut sequence_bits, 0, 1); // reduced_still_picture_header
        push_bits(&mut sequence_bits, 0, 1); // timing_info_present_flag
        push_bits(&mut sequence_bits, 0, 1); // initial_display_delay_present_flag
        push_bits(&mut sequence_bits, 0, 5); // operating_points_cnt_minus_1
        push_bits(&mut sequence_bits, 0, 12); // operating_point_idc
        push_bits(&mut sequence_bits, 4, 5); // seq_level_idx 3.0
        push_bits(&mut sequence_bits, 9, 4); // frame_width_bits_minus_1
        push_bits(&mut sequence_bits, 8, 4); // frame_height_bits_minus_1
        push_bits(&mut sequence_bits, 639, 10); // max_frame_width_minus_1
        push_bits(&mut sequence_bits, 367, 9); // max_frame_height_minus_1
        push_bits(&mut sequence_bits, 0, 1); // frame_id_numbers_present_flag
        push_bits(&mut sequence_bits, 0, 1); // use_128x128_superblock
        push_bits(&mut sequence_bits, 1, 1); // enable_filter_intra
        push_bits(&mut sequence_bits, 1, 1); // enable_intra_edge_filter
        push_bits(&mut sequence_bits, 0, 1); // enable_interintra_compound
        push_bits(&mut sequence_bits, 0, 1); // enable_masked_compound
        push_bits(&mut sequence_bits, 1, 1); // enable_warped_motion
        push_bits(&mut sequence_bits, 0, 1); // enable_dual_filter
        push_bits(&mut sequence_bits, 1, 1); // enable_order_hint
        push_bits(&mut sequence_bits, 0, 1); // enable_jnt_comp
        push_bits(&mut sequence_bits, 0, 1); // enable_ref_frame_mvs
        push_bits(&mut sequence_bits, 0, 1); // seq_choose_screen_content_tools
        push_bits(&mut sequence_bits, 0, 1); // seq_force_screen_content_tools
        push_bits(&mut sequence_bits, 6, 3); // order_hint_bits_minus_1
        push_bits(&mut sequence_bits, 0, 1); // enable_superres
        push_bits(&mut sequence_bits, 0, 1); // enable_cdef
        push_bits(&mut sequence_bits, 0, 1); // enable_restoration
        push_bits(&mut sequence_bits, 0, 1); // high_bitdepth
        push_bits(&mut sequence_bits, 0, 1); // mono_chrome
        push_bits(&mut sequence_bits, 0, 1); // color_description_present_flag
        push_bits(&mut sequence_bits, 0, 1); // color_range
        push_bits(&mut sequence_bits, 0, 2); // chroma_sample_position
        push_bits(&mut sequence_bits, 0, 1); // separate_uv_delta_q
        push_bits(&mut sequence_bits, 0, 1); // film_grain_params_present

        let mut frame_bits = Vec::new();
        push_bits(&mut frame_bits, 0, 1); // show_existing_frame
        push_bits(&mut frame_bits, 1, 2); // frame_type inter
        push_bits(&mut frame_bits, 1, 1); // show_frame
        push_bits(&mut frame_bits, 0, 1); // error_resilient_mode
        push_bits(&mut frame_bits, 1, 1); // disable_cdf_update
        push_bits(&mut frame_bits, 0, 1); // frame_size_override_flag
        push_bits(&mut frame_bits, 5, 7); // order_hint
        push_bits(&mut frame_bits, 7, 3); // primary_ref_frame none
        push_bits(&mut frame_bits, 0x01, 8); // refresh_frame_flags
        push_bits(&mut frame_bits, 0, 1); // frame_refs_short_signaling
        for value in 0..7 {
            push_bits(&mut frame_bits, value, 3); // ref_frame_idx
        }
        push_bits(&mut frame_bits, 0, 1); // render_and_frame_size_different
        push_bits(&mut frame_bits, 0, 1); // allow_high_precision_mv
        push_bits(&mut frame_bits, 0, 1); // is_filter_switchable
        push_bits(&mut frame_bits, 0, 2); // interpolation_filter eighttap
        push_bits(&mut frame_bits, 1, 1); // is_motion_mode_switchable
        push_bits(&mut frame_bits, 1, 1); // uniform_tile_spacing_flag
        push_bits(&mut frame_bits, 0, 1); // stop tile column increments
        push_bits(&mut frame_bits, 0, 1); // stop tile row increments
        push_bits(&mut frame_bits, 1, 8); // base_q_idx
        push_bits(&mut frame_bits, 0, 1); // delta_q_y_dc
        push_bits(&mut frame_bits, 0, 1); // delta_q_u_dc
        push_bits(&mut frame_bits, 0, 1); // delta_q_u_ac
        push_bits(&mut frame_bits, 0, 1); // using_qmatrix
        push_bits(&mut frame_bits, 0, 1); // segmentation_enabled
        push_bits(&mut frame_bits, 0, 1); // delta_q_present
        push_bits(&mut frame_bits, 0, 6); // loop_filter_level_0
        push_bits(&mut frame_bits, 0, 6); // loop_filter_level_1
        push_bits(&mut frame_bits, 0, 3); // loop_filter_sharpness
        push_bits(&mut frame_bits, 0, 1); // loop_filter_delta_enabled
        push_bits(&mut frame_bits, 0, 1); // tx_mode_select
        push_bits(&mut frame_bits, 0, 1); // reference_select
        push_bits(&mut frame_bits, 0, 1); // skip_mode_present
        push_bits(&mut frame_bits, 0, 1); // allow_warped_motion
        push_bits(&mut frame_bits, 1, 1); // reduced_tx_set
        while !frame_bits.len().is_multiple_of(8) {
            frame_bits.push(false);
        }

        let sequence_payload = pack_bits(&sequence_bits);
        let frame_payload = pack_bits(&frame_bits);
        let sequence_header = native_vulkan_parse_av1_sequence_header(&sequence_payload).unwrap();
        let header =
            native_vulkan_parse_av1_frame_header_for_submit(&frame_payload, &sequence_header)
                .unwrap();

        assert!(!header.allow_warped_motion);
        assert!(header.reduced_tx_set);

        let mut obu = Vec::new();
        push_obu(&mut obu, 1, &sequence_payload);
        push_obu(&mut obu, 6, &frame_payload);

        let stats = native_vulkan_av1_obu_stats(&obu).unwrap();
        let submit = stats.first_frame_submit.as_ref().unwrap();

        assert_eq!(submit.frame_type_label, "inter");
        assert!(submit.is_motion_mode_switchable);
        assert!(!submit.allow_warped_motion);
        assert!(
            submit
                .unsupported_reason
                .as_deref()
                .unwrap_or_default()
                .contains("AV1 first frame has no tile payload bytes")
        );
    }

    #[test]
    fn parses_av1_show_existing_frame_for_display_planning() {
        fn push_bits(bits: &mut Vec<bool>, value: u32, count: u32) {
            for shift in (0..count).rev() {
                bits.push(((value >> shift) & 1) != 0);
            }
        }
        fn pack_bits(bits: &[bool]) -> Vec<u8> {
            let mut bytes = vec![0u8; bits.len().div_ceil(8)];
            for (index, bit) in bits.iter().copied().enumerate() {
                if bit {
                    bytes[index / 8] |= 1 << (7 - (index % 8));
                }
            }
            bytes
        }
        fn push_obu(bytes: &mut Vec<u8>, obu_type: u8, payload: &[u8]) {
            bytes.push((obu_type << 3) | 0x02);
            bytes.push(payload.len() as u8);
            bytes.extend_from_slice(payload);
        }

        let mut sequence_bits = Vec::new();
        push_bits(&mut sequence_bits, 0, 3); // seq_profile Main
        push_bits(&mut sequence_bits, 0, 1); // still_picture
        push_bits(&mut sequence_bits, 0, 1); // reduced_still_picture_header
        push_bits(&mut sequence_bits, 0, 1); // timing_info_present_flag
        push_bits(&mut sequence_bits, 0, 1); // initial_display_delay_present_flag
        push_bits(&mut sequence_bits, 0, 5); // operating_points_cnt_minus_1
        push_bits(&mut sequence_bits, 0, 12); // operating_point_idc
        push_bits(&mut sequence_bits, 4, 5); // seq_level_idx 3.0
        push_bits(&mut sequence_bits, 9, 4); // frame_width_bits_minus_1
        push_bits(&mut sequence_bits, 8, 4); // frame_height_bits_minus_1
        push_bits(&mut sequence_bits, 639, 10); // max_frame_width_minus_1
        push_bits(&mut sequence_bits, 367, 9); // max_frame_height_minus_1
        push_bits(&mut sequence_bits, 0, 1); // frame_id_numbers_present_flag
        push_bits(&mut sequence_bits, 0, 1); // use_128x128_superblock
        push_bits(&mut sequence_bits, 1, 1); // enable_filter_intra
        push_bits(&mut sequence_bits, 1, 1); // enable_intra_edge_filter
        push_bits(&mut sequence_bits, 0, 1); // enable_interintra_compound
        push_bits(&mut sequence_bits, 0, 1); // enable_masked_compound
        push_bits(&mut sequence_bits, 0, 1); // enable_warped_motion
        push_bits(&mut sequence_bits, 0, 1); // enable_dual_filter
        push_bits(&mut sequence_bits, 1, 1); // enable_order_hint
        push_bits(&mut sequence_bits, 0, 1); // enable_jnt_comp
        push_bits(&mut sequence_bits, 0, 1); // enable_ref_frame_mvs
        push_bits(&mut sequence_bits, 0, 1); // seq_choose_screen_content_tools
        push_bits(&mut sequence_bits, 0, 1); // seq_force_screen_content_tools
        push_bits(&mut sequence_bits, 6, 3); // order_hint_bits_minus_1
        push_bits(&mut sequence_bits, 0, 1); // enable_superres
        push_bits(&mut sequence_bits, 0, 1); // enable_cdef
        push_bits(&mut sequence_bits, 0, 1); // enable_restoration
        push_bits(&mut sequence_bits, 0, 1); // high_bitdepth
        push_bits(&mut sequence_bits, 0, 1); // mono_chrome
        push_bits(&mut sequence_bits, 0, 1); // color_description_present_flag
        push_bits(&mut sequence_bits, 0, 1); // color_range
        push_bits(&mut sequence_bits, 0, 2); // chroma_sample_position
        push_bits(&mut sequence_bits, 0, 1); // separate_uv_delta_q
        push_bits(&mut sequence_bits, 0, 1); // film_grain_params_present

        let mut frame_bits = Vec::new();
        push_bits(&mut frame_bits, 1, 1); // show_existing_frame
        push_bits(&mut frame_bits, 5, 3); // frame_to_show_map_idx

        let mut obu = Vec::new();
        push_obu(&mut obu, 1, &pack_bits(&sequence_bits));
        push_obu(&mut obu, 6, &pack_bits(&frame_bits));

        let stats = native_vulkan_av1_obu_stats(&obu).unwrap();
        let submit = stats.first_frame_submit.as_ref().unwrap();

        assert!(submit.show_existing_frame);
        assert_eq!(submit.frame_to_show_map_idx, Some(5));
        assert_eq!(submit.frame_type_label, "unknown");
        assert!(submit.show_frame);
        assert!(!submit.vulkan_submit_candidate);
        assert!(
            submit
                .unsupported_reason
                .as_deref()
                .unwrap_or_default()
                .contains("show_existing_frame map index parsed")
        );

        let mut split_obu = Vec::new();
        push_obu(&mut split_obu, 1, &pack_bits(&sequence_bits));
        push_obu(&mut split_obu, 3, &pack_bits(&frame_bits));

        let split_stats = native_vulkan_av1_obu_stats(&split_obu).unwrap();
        let split_submit = split_stats.first_frame_submit.as_ref().unwrap();

        assert!(split_submit.show_existing_frame);
        assert_eq!(split_submit.frame_to_show_map_idx, Some(5));
        assert!(!split_submit.vulkan_submit_candidate);
        assert!(
            split_submit
                .unsupported_reason
                .as_deref()
                .unwrap_or_default()
                .contains("show_existing_frame map index parsed")
        );
        assert!(
            !split_submit
                .unsupported_reason
                .as_deref()
                .unwrap_or_default()
                .contains("no following tile-group")
        );
    }

    #[test]
    fn splits_av1_ffmpeg_packet_into_frame_units() {
        fn push_obu(bytes: &mut Vec<u8>, obu_type: u8, payload: &[u8]) {
            bytes.push((obu_type << 3) | 0x02);
            bytes.push(payload.len() as u8);
            bytes.extend_from_slice(payload);
        }

        let mut packet = Vec::new();
        push_obu(&mut packet, 1, &[0xaa]); // sequence header prefixes next frame
        push_obu(&mut packet, 6, &[0x80, 0x01]); // complete frame OBU
        push_obu(&mut packet, 3, &[0xc8]); // show-existing style frame header
        push_obu(&mut packet, 3, &[0x40]); // split frame header
        push_obu(&mut packet, 4, &[0x11, 0x22]); // tile group for split header

        let ranges = native_vulkan_av1_split_ffmpeg_packet_frame_ranges(&packet).unwrap();
        assert_eq!(ranges.len(), 3);

        let unit_obu_types = ranges
            .iter()
            .map(|range| {
                native_vulkan_av1_obu_ranges(&packet[range.clone()])
                    .unwrap()
                    .into_iter()
                    .map(|range| range.obu_type)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(unit_obu_types[0], vec![1, 6]);
        assert_eq!(unit_obu_types[1], vec![3]);
        assert_eq!(unit_obu_types[2], vec![3, 4]);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_av1_reference_map_for_inter_and_show_existing_frames() {
        fn submit(
            frame_type: u8,
            frame_type_label: &'static str,
            show_existing_frame: bool,
            frame_to_show_map_idx: Option<u8>,
            show_frame: bool,
            order_hint: Option<u8>,
            refresh_frame_flags: u8,
            ref_frame_indices: Vec<i8>,
            submit_ready: bool,
        ) -> NativeVulkanAv1FrameSubmitSnapshot {
            NativeVulkanAv1FrameSubmitSnapshot {
                parser: "test",
                frame_header_obu_offset: 0,
                frame_header_payload_offset: 0,
                frame_header_payload_size: 0,
                frame_header_offset_for_vulkan: 0,
                tile_count: u32::from(submit_ready),
                tile_columns: u32::from(submit_ready),
                tile_rows: u32::from(submit_ready),
                tile_size_bytes: 0,
                tile_offsets: if submit_ready { vec![0] } else { Vec::new() },
                tile_sizes: if submit_ready { vec![1] } else { Vec::new() },
                tile_payload_total_bytes: u64::from(submit_ready),
                frame_obu_payload_bytes: u64::from(submit_ready),
                frame_type,
                frame_type_label,
                show_existing_frame,
                frame_to_show_map_idx,
                display_frame_id: None,
                current_frame_id: None,
                expected_frame_ids: Vec::new(),
                show_frame,
                showable_frame: false,
                error_resilient_mode: frame_type == 0,
                disable_cdf_update: true,
                allow_screen_content_tools: 0,
                force_integer_mv: 2,
                allow_high_precision_mv: false,
                interpolation_filter: vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_EIGHTTAP.0
                    as u32,
                interpolation_filter_label: "eighttap",
                is_filter_switchable: false,
                is_motion_mode_switchable: false,
                use_ref_frame_mvs: false,
                reference_select: false,
                skip_mode_present: false,
                allow_warped_motion: false,
                order_hint,
                primary_ref_frame: None,
                refresh_frame_flags,
                reference_order_hints: Vec::new(),
                frame_refs_short_signaling: false,
                last_frame_idx: None,
                gold_frame_idx: None,
                ref_frame_indices,
                render_and_frame_size_different: None,
                frame_width: Some(640),
                frame_height: Some(368),
                render_width: Some(640),
                render_height: Some(368),
                found_frame_header: true,
                found_tile_payload: submit_ready,
                vulkan_submit_candidate: submit_ready,
                unsupported_reason: (!submit_ready && !show_existing_frame)
                    .then(|| "AV1 inter frame reference indices parsed; inter submit fields are not ready".to_owned()),
            }
        }

        fn temporal_unit(
            index: u32,
            first_frame_submit: NativeVulkanAv1FrameSubmitSnapshot,
        ) -> NativeVulkanAv1TemporalUnitSnapshot {
            NativeVulkanAv1TemporalUnitSnapshot {
                index,
                bytes: 0,
                byte_hash: 0,
                pts_ns: None,
                duration_ns: None,
                pts_ms: None,
                duration_ms: None,
                obu_count: 1,
                sequence_header_count: u32::from(index == 0),
                temporal_delimiter_count: 0,
                frame_header_count: 0,
                tile_group_count: 0,
                frame_count: u32::from(!first_frame_submit.show_existing_frame),
                decode_candidate: true,
                tile_payload_bytes: 0,
                frame_payload_bytes: 0,
                first_frame_header_obu_offset: Some(0),
                first_tile_group_obu_offset: None,
                sequence_header_present: index == 0,
                sequence_header: None,
                first_frame_submit: Some(first_frame_submit),
                obus: Vec::new(),
            }
        }

        let units = vec![
            temporal_unit(
                0,
                submit(0, "key", false, None, true, Some(0), 0xff, Vec::new(), true),
            ),
            temporal_unit(
                1,
                submit(
                    1,
                    "inter",
                    false,
                    None,
                    false,
                    Some(7),
                    0x02,
                    vec![0, 0, 0, 0, 0, 0, 0],
                    false,
                ),
            ),
            temporal_unit(
                2,
                submit(
                    1,
                    "inter",
                    false,
                    None,
                    true,
                    Some(2),
                    0x10,
                    vec![3, 0, 0, 0, 2, 0, 1],
                    false,
                ),
            ),
            temporal_unit(
                3,
                submit(
                    u8::MAX,
                    "unknown",
                    true,
                    Some(2),
                    true,
                    None,
                    0,
                    Vec::new(),
                    false,
                ),
            ),
        ];

        let plan = native_vulkan_av1_decode_reference_plan(&units, 8);

        assert_eq!(plan.len(), 4);
        assert!(plan[0].ready_for_decode_submit);
        assert_eq!(plan[0].output_slot, Some(0));
        assert_eq!(plan[0].map_slot_indices_after, vec![0, 0, 0, 0, 0, 0, 0, 0]);

        assert!(plan[1].references_resolved);
        assert!(!plan[1].submit_fields_ready);
        assert!(!plan[1].ready_for_decode_submit);
        assert_eq!(plan[1].output_slot, Some(1));
        assert_eq!(plan[1].decode_reference_slots, vec![0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(
            plan[1].reference_name_order_hints,
            vec![
                None,
                Some(0),
                Some(0),
                Some(0),
                Some(0),
                Some(0),
                Some(0),
                Some(0)
            ]
        );
        assert_eq!(plan[1].refreshed_reference_names, vec![1]);
        assert_eq!(plan[1].map_slot_indices_after, vec![0, 1, 0, 0, 0, 0, 0, 0]);

        assert!(plan[2].references_resolved);
        assert_eq!(plan[2].output_slot, Some(2));
        assert_eq!(plan[2].displayed_slot, Some(2));
        assert_eq!(plan[2].decode_reference_slots, vec![0, 0, 0, 0, 0, 0, 1]);
        assert_eq!(
            plan[2].reference_name_order_hints,
            vec![
                None,
                Some(0),
                Some(0),
                Some(0),
                Some(0),
                Some(0),
                Some(0),
                Some(7)
            ]
        );
        assert_eq!(plan[2].refreshed_reference_names, vec![4]);
        assert_eq!(plan[2].map_slot_indices_after, vec![0, 1, 0, 0, 2, 0, 0, 0]);

        assert!(plan[3].show_existing_frame);
        assert!(plan[3].ready_for_display_handoff);
        assert_eq!(plan[3].frame_to_show_map_idx, Some(2));
        assert_eq!(plan[3].displayed_slot, Some(0));
        assert_eq!(plan[3].missing_reference_count, 0);

        let ready_units = vec![
            temporal_unit(
                0,
                submit(0, "key", false, None, true, Some(0), 0xff, Vec::new(), true),
            ),
            temporal_unit(
                1,
                submit(
                    1,
                    "inter",
                    false,
                    None,
                    true,
                    Some(1),
                    0x02,
                    vec![0, 0, 0, 0, 0, 0, 0],
                    true,
                ),
            ),
        ];
        let one_slot_plan = native_vulkan_av1_decode_reference_plan(&ready_units, 1);
        assert!(!one_slot_plan[1].ready_for_decode_submit);
        assert!(
            one_slot_plan[1]
                .unsupported_reason
                .as_deref()
                .unwrap_or_default()
                .contains("no free DPB output slot")
        );
        let (min_slots, min_plan) = native_vulkan_av1_min_decodable_dpb_plan(&ready_units, 8);
        assert_eq!(min_slots, 2);
        assert!(min_plan[1].ready_for_decode_submit);
        assert_eq!(min_plan[1].output_slot, Some(1));
        assert_eq!(
            min_plan[1].decode_reference_slots,
            vec![0, 0, 0, 0, 0, 0, 0]
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn updates_av1_active_dpb_refs_for_show_existing_key_handoff() {
        let segmentation = NativeVulkanAv1ParsedSegmentation {
            enabled: false,
            update_map: false,
            temporal_update: false,
            update_data: false,
            feature_enabled: [0; 8],
            feature_data: [[0; 8]; 8],
        };
        let displayed = NativeVulkanAv1ActiveDpbReference {
            frame_type: 0,
            order_hint: 11,
            ref_frame_sign_bias: 0,
            saved_order_hints: [0, 7, 8, 9, 10, 0, 0, 0],
            frame_width: 3840,
            frame_height: 2160,
            render_width: 3840,
            render_height: 2160,
            disable_frame_end_update_cdf: true,
            segmentation_enabled: false,
            segmentation,
            loop_filter_ref_deltas: [1, 0, 0, 0, -1, 0, -1, -1],
            loop_filter_mode_deltas: [0, 0],
        };
        let stale = NativeVulkanAv1ActiveDpbReference {
            order_hint: 99,
            frame_type: 1,
            ..displayed
        };
        let mut active_dpb_refs = vec![Some(displayed), Some(stale), Some(stale)];
        let entry = NativeVulkanAv1DecodeReferencePlanEntrySnapshot {
            temporal_unit_index: 6,
            frame_type_label: "key",
            show_existing_frame: true,
            frame_to_show_map_idx: Some(2),
            show_frame: true,
            order_hint: Some(11),
            current_frame_id: None,
            expected_frame_ids: Vec::new(),
            refresh_frame_flags: 0xff,
            output_slot: None,
            displayed_slot: Some(0),
            reference_name_slot_indices: vec![0, 1, 2, -1, -1, -1, -1, -1],
            reference_name_order_hints: vec![None; 8],
            map_order_hints: vec![Some(11), Some(99), Some(99), None, None, None, None, None],
            ref_frame_indices: Vec::new(),
            decode_reference_slots: Vec::new(),
            refreshed_reference_names: (0..8).collect(),
            missing_reference_names: Vec::new(),
            missing_reference_count: 0,
            references_resolved: true,
            submit_fields_ready: false,
            ready_for_decode_submit: false,
            ready_for_display_handoff: true,
            unsupported_reason: None,
            map_slot_indices_after: vec![0; 8],
            map_order_hints_after: vec![Some(11); 8],
        };

        native_vulkan_av1_update_active_dpb_refs_after_display_handoff(
            &mut active_dpb_refs,
            &entry,
        )
        .expect("show-existing handoff updates active refs");

        assert_eq!(
            active_dpb_refs[0].map(|reference| reference.order_hint),
            Some(11)
        );
        assert!(active_dpb_refs[1].is_none());
        assert!(active_dpb_refs[2].is_none());
    }

    #[test]
    fn computes_av1_reference_sign_bias_from_order_hint_distance() {
        assert_eq!(
            native_vulkan_av1_relative_dist_from_order_hint_bits(true, Some(3), 0, 15),
            1
        );
        assert_eq!(
            native_vulkan_av1_relative_dist_from_order_hint_bits(true, Some(3), 15, 0),
            -1
        );
        assert_eq!(
            native_vulkan_av1_relative_dist_from_order_hint_bits(false, Some(3), 15, 0),
            0
        );

        let sequence_header = NativeVulkanAv1SequenceHeaderSnapshot {
            parser: "test",
            seq_profile: 0,
            seq_profile_label: "main",
            still_picture: false,
            reduced_still_picture_header: false,
            timing_info_present_flag: false,
            timing_info: None,
            decoder_model_info_present_flag: false,
            buffer_delay_length_minus_1: 0,
            frame_presentation_time_length_minus_1: 0,
            initial_display_delay_present_flag: false,
            operating_points_cnt_minus_1: 0,
            operating_points: vec![NativeVulkanAv1OperatingPointSnapshot {
                index: 0,
                idc: 0,
                seq_level_idx: 4,
                seq_level_label: Some("3.0"),
                seq_tier: false,
                decoder_model_present_for_this_op: false,
                initial_display_delay_present_for_this_op: false,
                initial_display_delay_minus_1: None,
            }],
            frame_width_bits_minus_1: 9,
            frame_height_bits_minus_1: 8,
            max_frame_width_minus_1: 639,
            max_frame_height_minus_1: 367,
            max_frame_width: 640,
            max_frame_height: 368,
            frame_id_numbers_present_flag: false,
            delta_frame_id_length_minus_2: None,
            additional_frame_id_length_minus_1: None,
            use_128x128_superblock: false,
            enable_filter_intra: false,
            enable_intra_edge_filter: false,
            enable_interintra_compound: false,
            enable_masked_compound: false,
            enable_warped_motion: false,
            enable_dual_filter: false,
            enable_order_hint: true,
            enable_jnt_comp: false,
            enable_ref_frame_mvs: false,
            seq_force_screen_content_tools: 0,
            seq_force_integer_mv: 2,
            order_hint_bits_minus_1: Some(6),
            enable_superres: false,
            enable_cdef: false,
            enable_restoration: false,
            film_grain_params_present: false,
            color_config: NativeVulkanAv1ColorConfigSnapshot {
                high_bitdepth: false,
                twelve_bit: false,
                mono_chrome: false,
                color_description_present_flag: false,
                color_primaries: 2,
                transfer_characteristics: 2,
                matrix_coefficients: 2,
                color_range: false,
                subsampling_x: true,
                subsampling_y: true,
                chroma_sample_position: 0,
                separate_uv_delta_q: false,
                bit_depth: 8,
                num_planes: 3,
            },
            requested_profile_compatible: true,
            vulkan_std_session_parameters_ready: true,
        };
        assert_eq!(
            native_vulkan_av1_ref_frame_sign_bias_from_order_hints(
                &sequence_header,
                8,
                [0, 8, 9, 7, 8, 0, 0, 0],
            ),
            0b0000_0100
        );
    }

    #[test]
    fn trims_av1_single_tile_inter_leading_zero_for_tile_payload_window() {
        fn header(frame_type: u8) -> NativeVulkanAv1ParsedFrameHeader {
            let bits = NativeVulkanAv1BitReader::new(&[]);
            let prefix = NativeVulkanAv1ParsedFrameHeaderPrefix {
                frame_type,
                show_existing_frame: false,
                frame_to_show_map_idx: None,
                display_frame_id: None,
                current_frame_id: None,
                show_frame: true,
                showable_frame: false,
                error_resilient_mode: frame_type == 0,
                disable_cdf_update: true,
                disable_frame_end_update_cdf: true,
                allow_screen_content_tools: 0,
                force_integer_mv: 2,
                allow_high_precision_mv: false,
                interpolation_filter: vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_EIGHTTAP,
                is_filter_switchable: false,
                is_motion_mode_switchable: false,
                use_ref_frame_mvs: false,
                reference_select: false,
                skip_mode_present: false,
                allow_warped_motion: false,
                frame_size_override_flag: false,
                order_hint: Some(0),
                primary_ref_frame: None,
                refresh_frame_flags: 0,
            };
            let mut header = native_vulkan_av1_partial_frame_header(
                &bits,
                prefix,
                Vec::new(),
                Vec::new(),
                false,
                None,
                None,
                Vec::new(),
                "test".to_owned(),
            );
            header.tile_count = 1;
            header.tile_columns = 1;
            header.tile_rows = 1;
            header
        }

        let inter_header = header(1);
        let (offsets, sizes) = native_vulkan_av1_tile_group_offsets_from_payload(
            100,
            20,
            &[0x00, 0xff, 0xaa],
            &inter_header,
        )
        .unwrap();
        assert_eq!(offsets, vec![121]);
        assert_eq!(sizes, vec![2]);

        let (offsets, sizes) = native_vulkan_av1_tile_group_offsets_from_payload(
            100,
            20,
            &[0xff, 0xaa],
            &inter_header,
        )
        .unwrap();
        assert_eq!(offsets, vec![120]);
        assert_eq!(sizes, vec![2]);

        let key_header = header(0);
        let (offsets, sizes) = native_vulkan_av1_tile_group_offsets_from_payload(
            100,
            20,
            &[0x00, 0xff, 0xaa],
            &key_header,
        )
        .unwrap();
        assert_eq!(offsets, vec![120]);
        assert_eq!(sizes, vec![3]);
    }

    #[test]
    fn submits_av1_picture_order_hints_by_reference_name() {
        let reference_name_order_hints = [0, 0, 0, 0, 0, 29, 0, 0];

        assert_eq!(
            native_vulkan_av1_picture_order_hints_for_submit(reference_name_order_hints, false),
            reference_name_order_hints
        );
        assert_eq!(
            native_vulkan_av1_picture_order_hints_for_submit(reference_name_order_hints, true),
            [0, 0, 0, 0, 29, 0, 0, 0]
        );
    }

    #[test]
    fn treats_av1_primary_ref_frame_7_as_none_for_segmentation() {
        fn pack_bits(bits: &[bool]) -> Vec<u8> {
            let mut bytes = vec![0u8; bits.len().div_ceil(8)];
            for (index, bit) in bits.iter().copied().enumerate() {
                if bit {
                    bytes[index / 8] |= 1 << (7 - (index % 8));
                }
            }
            bytes
        }

        assert!(native_vulkan_av1_primary_ref_none(None));
        assert!(native_vulkan_av1_primary_ref_none(Some(7)));
        assert!(!native_vulkan_av1_primary_ref_none(Some(0)));

        let mut bits = vec![true]; // segmentation_enabled
        bits.resize(65, false); // update_data=true feature flags
        let bytes = pack_bits(&bits);
        let mut reader = NativeVulkanAv1BitReader::new(&bytes);
        let segmentation =
            native_vulkan_parse_av1_segmentation_params(&mut reader, Some(7), None).unwrap();
        assert!(segmentation.enabled);
        assert!(segmentation.update_map);
        assert!(!segmentation.temporal_update);
        assert!(segmentation.update_data);
        assert_eq!(reader.bit_offset, 65);
    }

    #[test]
    fn inherits_av1_segmentation_from_primary_reference_when_update_data_is_clear() {
        let mut history_segmentation = NativeVulkanAv1ParsedSegmentation {
            enabled: true,
            update_map: true,
            temporal_update: false,
            update_data: true,
            feature_enabled: [0; 8],
            feature_data: [[0; 8]; 8],
        };
        history_segmentation.feature_enabled[3] = 0b0000_0101;
        history_segmentation.feature_data[3][0] = -7;
        history_segmentation.feature_data[3][2] = 12;
        let history = NativeVulkanAv1ReferenceHistory {
            frame_width: 640,
            frame_height: 368,
            render_width: 640,
            render_height: 368,
            segmentation: history_segmentation,
            loop_filter_ref_deltas: [2, 1, 0, -1, -2, -3, -4, -5],
            loop_filter_mode_deltas: [3, -3],
        };

        let bytes = [0b1000_0000u8]; // enabled=1, update_map=0, update_data=0
        let mut reader = NativeVulkanAv1BitReader::new(&bytes);
        let segmentation =
            native_vulkan_parse_av1_segmentation_params(&mut reader, Some(0), Some(history))
                .unwrap();
        assert!(segmentation.enabled);
        assert!(!segmentation.update_map);
        assert!(!segmentation.update_data);
        assert_eq!(
            segmentation.feature_enabled,
            history_segmentation.feature_enabled
        );
        assert_eq!(segmentation.feature_data, history_segmentation.feature_data);
        assert_eq!(reader.bit_offset, 3);
    }

    #[test]
    fn parses_av1_single_tile_key_frame_submit_candidate() {
        fn push_bits(bits: &mut Vec<bool>, value: u32, count: u32) {
            for shift in (0..count).rev() {
                bits.push(((value >> shift) & 1) != 0);
            }
        }
        fn pack_bits(bits: &[bool]) -> Vec<u8> {
            let mut bytes = vec![0u8; bits.len().div_ceil(8)];
            for (index, bit) in bits.iter().copied().enumerate() {
                if bit {
                    bytes[index / 8] |= 1 << (7 - (index % 8));
                }
            }
            bytes
        }
        fn push_obu(bytes: &mut Vec<u8>, obu_type: u8, payload: &[u8]) {
            bytes.push((obu_type << 3) | 0x02);
            bytes.push(payload.len() as u8);
            bytes.extend_from_slice(payload);
        }

        let mut sequence_bits = Vec::new();
        push_bits(&mut sequence_bits, 0, 3); // seq_profile Main
        push_bits(&mut sequence_bits, 0, 1); // still_picture
        push_bits(&mut sequence_bits, 0, 1); // reduced_still_picture_header
        push_bits(&mut sequence_bits, 0, 1); // timing_info_present_flag
        push_bits(&mut sequence_bits, 0, 1); // initial_display_delay_present_flag
        push_bits(&mut sequence_bits, 0, 5); // operating_points_cnt_minus_1
        push_bits(&mut sequence_bits, 0, 12); // operating_point_idc
        push_bits(&mut sequence_bits, 4, 5); // seq_level_idx 3.0
        push_bits(&mut sequence_bits, 9, 4); // frame_width_bits_minus_1
        push_bits(&mut sequence_bits, 8, 4); // frame_height_bits_minus_1
        push_bits(&mut sequence_bits, 639, 10); // max_frame_width_minus_1
        push_bits(&mut sequence_bits, 367, 9); // max_frame_height_minus_1
        push_bits(&mut sequence_bits, 0, 1); // frame_id_numbers_present_flag
        push_bits(&mut sequence_bits, 0, 1); // use_128x128_superblock
        push_bits(&mut sequence_bits, 1, 1); // enable_filter_intra
        push_bits(&mut sequence_bits, 1, 1); // enable_intra_edge_filter
        push_bits(&mut sequence_bits, 0, 1); // enable_interintra_compound
        push_bits(&mut sequence_bits, 0, 1); // enable_masked_compound
        push_bits(&mut sequence_bits, 0, 1); // enable_warped_motion
        push_bits(&mut sequence_bits, 0, 1); // enable_dual_filter
        push_bits(&mut sequence_bits, 0, 1); // enable_order_hint
        push_bits(&mut sequence_bits, 0, 1); // seq_choose_screen_content_tools
        push_bits(&mut sequence_bits, 0, 1); // seq_force_screen_content_tools
        push_bits(&mut sequence_bits, 0, 1); // enable_superres
        push_bits(&mut sequence_bits, 0, 1); // enable_cdef
        push_bits(&mut sequence_bits, 0, 1); // enable_restoration
        push_bits(&mut sequence_bits, 0, 1); // high_bitdepth
        push_bits(&mut sequence_bits, 0, 1); // mono_chrome
        push_bits(&mut sequence_bits, 0, 1); // color_description_present_flag
        push_bits(&mut sequence_bits, 0, 1); // color_range
        push_bits(&mut sequence_bits, 0, 2); // chroma_sample_position
        push_bits(&mut sequence_bits, 0, 1); // separate_uv_delta_q
        push_bits(&mut sequence_bits, 0, 1); // film_grain_params_present

        let mut frame_bits = Vec::new();
        push_bits(&mut frame_bits, 0, 1); // show_existing_frame
        push_bits(&mut frame_bits, 0, 2); // frame_type key
        push_bits(&mut frame_bits, 1, 1); // show_frame
        push_bits(&mut frame_bits, 0, 1); // disable_cdf_update
        push_bits(&mut frame_bits, 0, 1); // frame_size_override_flag
        push_bits(&mut frame_bits, 0, 1); // render_and_frame_size_different
        push_bits(&mut frame_bits, 0, 1); // disable_frame_end_update_cdf
        push_bits(&mut frame_bits, 1, 1); // uniform_tile_spacing_flag
        push_bits(&mut frame_bits, 0, 1); // stop tile column increments
        push_bits(&mut frame_bits, 0, 1); // stop tile row increments
        push_bits(&mut frame_bits, 1, 8); // base_q_idx
        push_bits(&mut frame_bits, 0, 1); // delta_q_y_dc
        push_bits(&mut frame_bits, 0, 1); // delta_q_u_dc
        push_bits(&mut frame_bits, 0, 1); // delta_q_u_ac
        push_bits(&mut frame_bits, 0, 1); // using_qmatrix
        push_bits(&mut frame_bits, 1, 1); // segmentation_enabled
        push_bits(&mut frame_bits, 1, 1); // segmentation_feature_enabled
        push_bits(&mut frame_bits, 0, 8); // segmentation_feature_value
        for _ in 1..64 {
            push_bits(&mut frame_bits, 0, 1); // segmentation_feature_enabled
        }
        push_bits(&mut frame_bits, 0, 1); // delta_q_present
        push_bits(&mut frame_bits, 0, 6); // loop_filter_level_0
        push_bits(&mut frame_bits, 0, 6); // loop_filter_level_1
        push_bits(&mut frame_bits, 0, 3); // loop_filter_sharpness
        push_bits(&mut frame_bits, 0, 1); // loop_filter_delta_enabled
        push_bits(&mut frame_bits, 0, 1); // tx_mode_select
        push_bits(&mut frame_bits, 0, 1); // reduced_tx_set
        while !frame_bits.len().is_multiple_of(8) {
            frame_bits.push(false);
        }

        let mut frame_payload = pack_bits(&frame_bits);
        let expected_tile_offset_in_payload = frame_payload.len() as u32;
        frame_payload.extend_from_slice(&[0xaa, 0xbb, 0xcc]);

        let mut obu = Vec::new();
        push_obu(&mut obu, 1, &pack_bits(&sequence_bits));
        let frame_obu_payload_offset = obu.len() as u32 + 2;
        push_obu(&mut obu, 6, &frame_payload);

        let stats = native_vulkan_av1_obu_stats(&obu).unwrap();
        let submit = stats.first_frame_submit.as_ref().unwrap();

        assert!(submit.vulkan_submit_candidate, "{submit:?}");
        assert_eq!(submit.frame_type_label, "key");
        assert_eq!(submit.tile_count, 1);
        assert_eq!(
            submit.tile_offsets,
            vec![frame_obu_payload_offset + expected_tile_offset_in_payload]
        );
        assert_eq!(submit.tile_sizes, vec![3]);
        assert_eq!(submit.frame_width, Some(640));
        assert_eq!(submit.frame_height, Some(368));
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn av1_temporal_unit_snapshot_uses_bootstrap_sequence_header_for_frame_only_tu() {
        fn push_bits(bits: &mut Vec<bool>, value: u32, count: u32) {
            for shift in (0..count).rev() {
                bits.push(((value >> shift) & 1) != 0);
            }
        }
        fn pack_bits(bits: &[bool]) -> Vec<u8> {
            let mut bytes = vec![0u8; bits.len().div_ceil(8)];
            for (index, bit) in bits.iter().copied().enumerate() {
                if bit {
                    bytes[index / 8] |= 1 << (7 - (index % 8));
                }
            }
            bytes
        }
        fn push_obu(bytes: &mut Vec<u8>, obu_type: u8, payload: &[u8]) {
            bytes.push((obu_type << 3) | 0x02);
            bytes.push(payload.len() as u8);
            bytes.extend_from_slice(payload);
        }

        let sequence_header = NativeVulkanAv1SequenceHeaderSnapshot {
            parser: "test",
            seq_profile: 0,
            seq_profile_label: "main",
            still_picture: false,
            reduced_still_picture_header: false,
            timing_info_present_flag: false,
            timing_info: None,
            decoder_model_info_present_flag: false,
            buffer_delay_length_minus_1: 0,
            frame_presentation_time_length_minus_1: 0,
            initial_display_delay_present_flag: false,
            operating_points_cnt_minus_1: 0,
            operating_points: vec![NativeVulkanAv1OperatingPointSnapshot {
                index: 0,
                idc: 0,
                seq_level_idx: 4,
                seq_level_label: Some("3.0"),
                seq_tier: false,
                decoder_model_present_for_this_op: false,
                initial_display_delay_present_for_this_op: false,
                initial_display_delay_minus_1: None,
            }],
            frame_width_bits_minus_1: 9,
            frame_height_bits_minus_1: 8,
            max_frame_width_minus_1: 639,
            max_frame_height_minus_1: 367,
            max_frame_width: 640,
            max_frame_height: 368,
            frame_id_numbers_present_flag: false,
            delta_frame_id_length_minus_2: None,
            additional_frame_id_length_minus_1: None,
            use_128x128_superblock: false,
            enable_filter_intra: true,
            enable_intra_edge_filter: true,
            enable_interintra_compound: false,
            enable_masked_compound: false,
            enable_warped_motion: false,
            enable_dual_filter: false,
            enable_order_hint: false,
            enable_jnt_comp: false,
            enable_ref_frame_mvs: false,
            seq_force_screen_content_tools: 0,
            seq_force_integer_mv: 2,
            order_hint_bits_minus_1: None,
            enable_superres: false,
            enable_cdef: false,
            enable_restoration: false,
            film_grain_params_present: false,
            color_config: NativeVulkanAv1ColorConfigSnapshot {
                high_bitdepth: false,
                twelve_bit: false,
                mono_chrome: false,
                color_description_present_flag: false,
                color_primaries: 2,
                transfer_characteristics: 2,
                matrix_coefficients: 2,
                color_range: false,
                subsampling_x: true,
                subsampling_y: true,
                chroma_sample_position: 0,
                separate_uv_delta_q: false,
                bit_depth: 8,
                num_planes: 3,
            },
            requested_profile_compatible: true,
            vulkan_std_session_parameters_ready: true,
        };

        let mut frame_bits = Vec::new();
        push_bits(&mut frame_bits, 0, 1); // show_existing_frame
        push_bits(&mut frame_bits, 0, 2); // frame_type key
        push_bits(&mut frame_bits, 1, 1); // show_frame
        push_bits(&mut frame_bits, 0, 1); // disable_cdf_update
        push_bits(&mut frame_bits, 0, 1); // frame_size_override_flag
        push_bits(&mut frame_bits, 0, 1); // render_and_frame_size_different
        push_bits(&mut frame_bits, 0, 1); // disable_frame_end_update_cdf
        push_bits(&mut frame_bits, 1, 1); // uniform_tile_spacing_flag
        push_bits(&mut frame_bits, 0, 1); // stop tile column increments
        push_bits(&mut frame_bits, 0, 1); // stop tile row increments
        push_bits(&mut frame_bits, 1, 8); // base_q_idx
        push_bits(&mut frame_bits, 0, 1); // delta_q_y_dc
        push_bits(&mut frame_bits, 0, 1); // delta_q_u_dc
        push_bits(&mut frame_bits, 0, 1); // delta_q_u_ac
        push_bits(&mut frame_bits, 0, 1); // using_qmatrix
        push_bits(&mut frame_bits, 0, 1); // segmentation_enabled
        push_bits(&mut frame_bits, 0, 1); // delta_q_present
        push_bits(&mut frame_bits, 0, 6); // loop_filter_level_0
        push_bits(&mut frame_bits, 0, 6); // loop_filter_level_1
        push_bits(&mut frame_bits, 0, 3); // loop_filter_sharpness
        push_bits(&mut frame_bits, 0, 1); // loop_filter_delta_enabled
        push_bits(&mut frame_bits, 0, 1); // tx_mode_select
        push_bits(&mut frame_bits, 0, 1); // reduced_tx_set
        while !frame_bits.len().is_multiple_of(8) {
            frame_bits.push(false);
        }

        let mut frame_payload = pack_bits(&frame_bits);
        let expected_tile_offset_in_payload = frame_payload.len() as u32;
        frame_payload.extend_from_slice(&[0xaa, 0xbb, 0xcc]);

        let mut frame_only_obu = Vec::new();
        let frame_obu_payload_offset = 2u32;
        push_obu(&mut frame_only_obu, 6, &frame_payload);
        let frame_only_stats = native_vulkan_av1_obu_stats(&frame_only_obu).unwrap();
        assert_eq!(frame_only_stats.sequence_header_count, 0);
        assert!(frame_only_stats.first_frame_submit.is_none());

        let temporal_unit = NativeVulkanAv1TemporalUnitExtract {
            payload: NativeVulkanEncodedAccessUnitPayload::owned(frame_only_obu),
            pts_ns: Some(4_000_000),
            duration_ns: Some(4_000_000),
            pts_ms: Some(4),
            duration_ms: Some(4),
            stats: frame_only_stats,
        };

        let snapshot =
            native_vulkan_av1_temporal_unit_snapshot(1, &temporal_unit, Some(&sequence_header));
        let submit = snapshot.first_frame_submit.as_ref().unwrap();

        assert!(!snapshot.sequence_header_present);
        assert!(submit.vulkan_submit_candidate, "{submit:?}");
        assert_eq!(snapshot.pts_ms, Some(4));
        assert_eq!(snapshot.duration_ms, Some(4));
        assert_eq!(
            submit.tile_offsets,
            vec![frame_obu_payload_offset + expected_tile_offset_in_payload]
        );
        assert_eq!(submit.tile_sizes, vec![3]);
    }

    #[test]
    fn parses_av1_uniform_multi_tile_key_frame_submit_candidate() {
        fn push_bits(bits: &mut Vec<bool>, value: u32, count: u32) {
            for shift in (0..count).rev() {
                bits.push(((value >> shift) & 1) != 0);
            }
        }
        fn pack_bits(bits: &[bool]) -> Vec<u8> {
            let mut bytes = vec![0u8; bits.len().div_ceil(8)];
            for (index, bit) in bits.iter().copied().enumerate() {
                if bit {
                    bytes[index / 8] |= 1 << (7 - (index % 8));
                }
            }
            bytes
        }
        fn push_obu(bytes: &mut Vec<u8>, obu_type: u8, payload: &[u8]) {
            bytes.push((obu_type << 3) | 0x02);
            bytes.push(payload.len() as u8);
            bytes.extend_from_slice(payload);
        }

        let mut sequence_bits = Vec::new();
        push_bits(&mut sequence_bits, 0, 3); // seq_profile Main
        push_bits(&mut sequence_bits, 0, 1); // still_picture
        push_bits(&mut sequence_bits, 0, 1); // reduced_still_picture_header
        push_bits(&mut sequence_bits, 0, 1); // timing_info_present_flag
        push_bits(&mut sequence_bits, 0, 1); // initial_display_delay_present_flag
        push_bits(&mut sequence_bits, 0, 5); // operating_points_cnt_minus_1
        push_bits(&mut sequence_bits, 0, 12); // operating_point_idc
        push_bits(&mut sequence_bits, 4, 5); // seq_level_idx 3.0
        push_bits(&mut sequence_bits, 9, 4); // frame_width_bits_minus_1
        push_bits(&mut sequence_bits, 8, 4); // frame_height_bits_minus_1
        push_bits(&mut sequence_bits, 639, 10); // max_frame_width_minus_1
        push_bits(&mut sequence_bits, 367, 9); // max_frame_height_minus_1
        push_bits(&mut sequence_bits, 0, 1); // frame_id_numbers_present_flag
        push_bits(&mut sequence_bits, 0, 1); // use_128x128_superblock
        push_bits(&mut sequence_bits, 1, 1); // enable_filter_intra
        push_bits(&mut sequence_bits, 1, 1); // enable_intra_edge_filter
        push_bits(&mut sequence_bits, 0, 1); // enable_interintra_compound
        push_bits(&mut sequence_bits, 0, 1); // enable_masked_compound
        push_bits(&mut sequence_bits, 0, 1); // enable_warped_motion
        push_bits(&mut sequence_bits, 0, 1); // enable_dual_filter
        push_bits(&mut sequence_bits, 0, 1); // enable_order_hint
        push_bits(&mut sequence_bits, 0, 1); // seq_choose_screen_content_tools
        push_bits(&mut sequence_bits, 0, 1); // seq_force_screen_content_tools
        push_bits(&mut sequence_bits, 0, 1); // enable_superres
        push_bits(&mut sequence_bits, 0, 1); // enable_cdef
        push_bits(&mut sequence_bits, 0, 1); // enable_restoration
        push_bits(&mut sequence_bits, 0, 1); // high_bitdepth
        push_bits(&mut sequence_bits, 0, 1); // mono_chrome
        push_bits(&mut sequence_bits, 0, 1); // color_description_present_flag
        push_bits(&mut sequence_bits, 0, 1); // color_range
        push_bits(&mut sequence_bits, 0, 2); // chroma_sample_position
        push_bits(&mut sequence_bits, 0, 1); // separate_uv_delta_q
        push_bits(&mut sequence_bits, 0, 1); // film_grain_params_present

        let mut frame_bits = Vec::new();
        push_bits(&mut frame_bits, 0, 1); // show_existing_frame
        push_bits(&mut frame_bits, 0, 2); // frame_type key
        push_bits(&mut frame_bits, 1, 1); // show_frame
        push_bits(&mut frame_bits, 1, 1); // disable_cdf_update
        push_bits(&mut frame_bits, 0, 1); // frame_size_override_flag
        push_bits(&mut frame_bits, 0, 1); // render_and_frame_size_different
        push_bits(&mut frame_bits, 1, 1); // uniform_tile_spacing_flag
        push_bits(&mut frame_bits, 1, 1); // increment_tile_cols_log2 -> 1
        push_bits(&mut frame_bits, 0, 1); // stop tile column increments
        push_bits(&mut frame_bits, 1, 1); // increment_tile_rows_log2 -> 1
        push_bits(&mut frame_bits, 0, 1); // stop tile row increments
        push_bits(&mut frame_bits, 0, 2); // context_update_tile_id
        push_bits(&mut frame_bits, 0, 2); // tile_size_bytes_minus_1
        push_bits(&mut frame_bits, 1, 8); // base_q_idx
        push_bits(&mut frame_bits, 0, 1); // delta_q_y_dc
        push_bits(&mut frame_bits, 0, 1); // delta_q_u_dc
        push_bits(&mut frame_bits, 0, 1); // delta_q_u_ac
        push_bits(&mut frame_bits, 0, 1); // using_qmatrix
        push_bits(&mut frame_bits, 0, 1); // segmentation_enabled
        push_bits(&mut frame_bits, 0, 1); // delta_q_present
        push_bits(&mut frame_bits, 0, 6); // loop_filter_level_0
        push_bits(&mut frame_bits, 0, 6); // loop_filter_level_1
        push_bits(&mut frame_bits, 0, 3); // loop_filter_sharpness
        push_bits(&mut frame_bits, 0, 1); // loop_filter_delta_enabled
        push_bits(&mut frame_bits, 0, 1); // tx_mode_select
        push_bits(&mut frame_bits, 0, 1); // reduced_tx_set
        while !frame_bits.len().is_multiple_of(8) {
            frame_bits.push(false);
        }

        let mut frame_payload = pack_bits(&frame_bits);
        let frame_header_len = frame_payload.len() as u32;
        frame_payload.push(0); // tile_start_and_end_present_flag + alignment padding
        frame_payload.extend_from_slice(&[1, 0xaa, 0xab]);
        frame_payload.extend_from_slice(&[2, 0xba, 0xbb, 0xbc]);
        frame_payload.extend_from_slice(&[0, 0xca]);
        frame_payload.extend_from_slice(&[0xda, 0xdb, 0xdc, 0xdd]);

        let mut obu = Vec::new();
        push_obu(&mut obu, 1, &pack_bits(&sequence_bits));
        let frame_obu_payload_offset = obu.len() as u32 + 2;
        push_obu(&mut obu, 6, &frame_payload);

        let stats = native_vulkan_av1_obu_stats(&obu).unwrap();
        let submit = stats.first_frame_submit.as_ref().unwrap();

        assert!(submit.vulkan_submit_candidate, "{submit:?}");
        assert_eq!(submit.tile_count, 4);
        assert_eq!(submit.tile_columns, 2);
        assert_eq!(submit.tile_rows, 2);
        assert_eq!(submit.tile_size_bytes, 1);
        assert_eq!(
            submit.tile_offsets,
            vec![
                frame_obu_payload_offset + frame_header_len + 2,
                frame_obu_payload_offset + frame_header_len + 5,
                frame_obu_payload_offset + frame_header_len + 9,
                frame_obu_payload_offset + frame_header_len + 10,
            ]
        );
        assert_eq!(submit.tile_sizes, vec![2, 3, 1, 4]);
        assert_eq!(submit.tile_payload_total_bytes, 10);
    }

    #[test]
    fn parses_av1_main10_sequence_header_obu_for_vulkan_std_subset() {
        fn push_bits(bits: &mut Vec<bool>, value: u32, count: u32) {
            for shift in (0..count).rev() {
                bits.push(((value >> shift) & 1) != 0);
            }
        }
        fn pack_bits(bits: &[bool]) -> Vec<u8> {
            let mut bytes = vec![0u8; bits.len().div_ceil(8)];
            for (index, bit) in bits.iter().copied().enumerate() {
                if bit {
                    bytes[index / 8] |= 1 << (7 - (index % 8));
                }
            }
            bytes
        }

        let mut bits = Vec::new();
        push_bits(&mut bits, 0, 3); // seq_profile Main
        push_bits(&mut bits, 0, 1); // still_picture
        push_bits(&mut bits, 0, 1); // reduced_still_picture_header
        push_bits(&mut bits, 0, 1); // timing_info_present_flag
        push_bits(&mut bits, 0, 1); // initial_display_delay_present_flag
        push_bits(&mut bits, 0, 5); // operating_points_cnt_minus_1
        push_bits(&mut bits, 0, 12); // operating_point_idc
        push_bits(&mut bits, 4, 5); // seq_level_idx 3.0
        push_bits(&mut bits, 9, 4); // frame_width_bits_minus_1
        push_bits(&mut bits, 8, 4); // frame_height_bits_minus_1
        push_bits(&mut bits, 639, 10); // max_frame_width_minus_1
        push_bits(&mut bits, 367, 9); // max_frame_height_minus_1
        push_bits(&mut bits, 0, 1); // frame_id_numbers_present_flag
        push_bits(&mut bits, 0, 1); // use_128x128_superblock
        push_bits(&mut bits, 1, 1); // enable_filter_intra
        push_bits(&mut bits, 1, 1); // enable_intra_edge_filter
        push_bits(&mut bits, 1, 1); // enable_interintra_compound
        push_bits(&mut bits, 1, 1); // enable_masked_compound
        push_bits(&mut bits, 1, 1); // enable_warped_motion
        push_bits(&mut bits, 1, 1); // enable_dual_filter
        push_bits(&mut bits, 1, 1); // enable_order_hint
        push_bits(&mut bits, 1, 1); // enable_jnt_comp
        push_bits(&mut bits, 1, 1); // enable_ref_frame_mvs
        push_bits(&mut bits, 1, 1); // seq_choose_screen_content_tools
        push_bits(&mut bits, 1, 1); // seq_choose_integer_mv
        push_bits(&mut bits, 6, 3); // order_hint_bits_minus_1
        push_bits(&mut bits, 0, 1); // enable_superres
        push_bits(&mut bits, 1, 1); // enable_cdef
        push_bits(&mut bits, 1, 1); // enable_restoration
        push_bits(&mut bits, 1, 1); // high_bitdepth
        push_bits(&mut bits, 0, 1); // mono_chrome
        push_bits(&mut bits, 0, 1); // color_description_present_flag
        push_bits(&mut bits, 0, 1); // color_range
        push_bits(&mut bits, 0, 2); // chroma_sample_position
        push_bits(&mut bits, 0, 1); // separate_uv_delta_q
        push_bits(&mut bits, 0, 1); // film_grain_params_present

        let payload = pack_bits(&bits);
        let mut obu = Vec::with_capacity(payload.len() + 2);
        obu.push(0x0a);
        obu.push(payload.len() as u8);
        obu.extend_from_slice(&payload);

        let stats = native_vulkan_av1_obu_stats(&obu).unwrap();
        let sequence_header = stats.sequence_header.as_ref().unwrap();

        assert_eq!(stats.sequence_header_count, 1);
        assert_eq!(sequence_header.seq_profile_label, "main");
        assert_eq!(sequence_header.max_frame_width, 640);
        assert_eq!(sequence_header.max_frame_height, 368);
        assert_eq!(sequence_header.color_config.bit_depth, 10);
        assert!(sequence_header.color_config.subsampling_x);
        assert!(sequence_header.color_config.subsampling_y);
        assert!(sequence_header.vulkan_std_session_parameters_ready);
    }

    #[cfg(feature = "native-vulkan-video")]
    fn h264_test_access_unit(
        index: u32,
        frame_num: u16,
        idr: bool,
    ) -> NativeVulkanH264AccessUnitSnapshot {
        let is_p = !idr;
        NativeVulkanH264AccessUnitSnapshot {
            index,
            bytes: 0,
            byte_hash: 0,
            pts_ns: Some(u64::from(index) * 4_166_667),
            duration_ns: Some(4_166_667),
            pts_ms: Some(u64::from(index) * 4),
            duration_ms: Some(4),
            has_annex_b_start_codes: true,
            has_parameter_sets: idr,
            h264_sps_count: u32::from(idr),
            h264_pps_count: u32::from(idr),
            h264_idr_count: u32::from(idr),
            h264_slice_count: 1,
            first_slice: Some(NativeVulkanH264AccessUnitSliceSnapshot {
                nal_type: if idr { 5 } else { 1 },
                nal_type_label: if idr { "idr" } else { "non-idr-slice" },
                nal_ref_idc: 3,
                first_mb_in_slice: 0,
                first_slice_segment_in_pic_flag: true,
                slice_type: if idr { 7 } else { 5 },
                slice_type_normalized: if idr { 2 } else { 0 },
                pps_id: 0,
                frame_num,
                idr_pic_id: if idr { 0 } else { 0 },
                num_ref_idx_l0_active_minus1: is_p.then_some(0),
                num_ref_idx_l1_active_minus1: None,
                ref_pic_list_modification_l0: false,
                ref_pic_list_modifications_l0: Vec::new(),
                ref_pic_list_modification_l1: false,
                ref_pic_list_modifications_l1: Vec::new(),
                adaptive_ref_pic_marking_mode_flag: false,
                memory_management_control_operations: Vec::new(),
                field_pic_flag: false,
                bottom_field_flag: false,
                is_reference: true,
                is_intra: idr,
                is_p,
                is_b: false,
                long_term_reference_flag: false,
                pic_order_cnt: [i32::from(frame_num); 2],
                slice_offsets: NativeVulkanH264SliceOffsets::single(0),
                idr,
                irap: idr,
            }),
            first_slice_parse_error: None,
            idr_decode_ready: idr,
            decode_ready: true,
        }
    }

    #[cfg(feature = "native-vulkan-video")]
    fn h264_test_sps(frame_mbs_only_flag: bool) -> NativeVulkanH264SpsSnapshot {
        NativeVulkanH264SpsSnapshot {
            id: 0,
            profile_idc: 100,
            profile_label: "high",
            constraint_set0_flag: false,
            constraint_set1_flag: false,
            constraint_set2_flag: false,
            constraint_set3_flag: false,
            constraint_set4_flag: false,
            constraint_set5_flag: false,
            level_idc: 52,
            level_label: Some("5.2"),
            chroma_format_idc: 1,
            chroma_format_label: "4:2:0",
            separate_colour_plane_flag: false,
            bit_depth_luma_minus8: 0,
            bit_depth_chroma_minus8: 0,
            qpprime_y_zero_transform_bypass_flag: false,
            seq_scaling_matrix_present_flag: false,
            log2_max_frame_num_minus4: 0,
            pic_order_cnt_type: 0,
            log2_max_pic_order_cnt_lsb_minus4: 0,
            delta_pic_order_always_zero_flag: false,
            offset_for_non_ref_pic: 0,
            offset_for_top_to_bottom_field: 0,
            offset_for_ref_frame: Vec::new(),
            max_num_ref_frames: 2,
            gaps_in_frame_num_value_allowed_flag: false,
            pic_width_in_mbs_minus1: 119,
            pic_height_in_map_units_minus1: 67,
            frame_mbs_only_flag,
            mb_adaptive_frame_field_flag: !frame_mbs_only_flag,
            direct_8x8_inference_flag: true,
            frame_cropping_flag: false,
            frame_crop_left_offset: 0,
            frame_crop_right_offset: 0,
            frame_crop_top_offset: 0,
            frame_crop_bottom_offset: 0,
            vui_parameters_present_flag: false,
            vui: None,
            width: 1920,
            height: 1080,
        }
    }

    #[cfg(feature = "native-vulkan-video")]
    fn h264_test_mmco(
        memory_management_control_operation: u32,
        difference_of_pic_nums_minus1: Option<u32>,
        long_term_pic_num: Option<u32>,
        long_term_frame_idx: Option<u32>,
        max_long_term_frame_idx_plus1: Option<u32>,
    ) -> NativeVulkanH264MemoryManagementControlOperationSnapshot {
        NativeVulkanH264MemoryManagementControlOperationSnapshot {
            memory_management_control_operation,
            difference_of_pic_nums_minus1,
            long_term_pic_num,
            long_term_frame_idx,
            max_long_term_frame_idx_plus1,
        }
    }

    #[cfg(feature = "native-vulkan-video")]
    fn h264_test_long_term_l0_modification(
        long_term_pic_num: u32,
    ) -> NativeVulkanH264RefPicListModificationSnapshot {
        NativeVulkanH264RefPicListModificationSnapshot {
            modification_of_pic_nums_idc: 2,
            abs_diff_pic_num_minus1: None,
            long_term_pic_num: Some(long_term_pic_num),
        }
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn keys_h264_short_term_field_pictures_by_frame_num_and_field_kind() {
        let top_key = NativeVulkanH264ShortTermPictureKey {
            frame_num: 7,
            field_kind: NativeVulkanH264PictureFieldKind::TopField,
        };
        let bottom_key = NativeVulkanH264ShortTermPictureKey {
            frame_num: 7,
            field_kind: NativeVulkanH264PictureFieldKind::BottomField,
        };
        let mut references = BTreeMap::new();
        references.insert(
            top_key,
            NativeVulkanH264DpbReferenceState {
                source_access_unit_index: Some(10),
                dpb_slot: 0,
                pic_order_cnt_val: 14,
                pic_order_cnt: [14, 15],
                frame_num: 7,
                field_kind: top_key.field_kind,
                non_existing: false,
            },
        );
        references.insert(
            bottom_key,
            NativeVulkanH264DpbReferenceState {
                source_access_unit_index: Some(11),
                dpb_slot: 1,
                pic_order_cnt_val: 15,
                pic_order_cnt: [14, 15],
                frame_num: 7,
                field_kind: bottom_key.field_kind,
                non_existing: false,
            },
        );

        let keys = references
            .keys()
            .filter(|key| key.frame_num == 7)
            .copied()
            .collect::<Vec<_>>();

        assert_eq!(references.len(), 2);
        assert_eq!(keys, vec![top_key, bottom_key]);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn sets_h264_reference_info_field_flags() {
        let frame = native_vulkan_h264_reference_info_flags(false, false, false, false);
        let top = native_vulkan_h264_reference_info_flags(true, false, false, false);
        let bottom = native_vulkan_h264_reference_info_flags(true, true, true, true);

        assert_eq!(frame.top_field_flag(), 0);
        assert_eq!(frame.bottom_field_flag(), 0);
        assert_eq!(top.top_field_flag(), 1);
        assert_eq!(top.bottom_field_flag(), 0);
        assert_eq!(bottom.top_field_flag(), 0);
        assert_eq!(bottom.bottom_field_flag(), 1);
        assert_eq!(bottom.used_for_long_term_reference(), 1);
        assert_eq!(bottom.is_non_existing(), 1);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_complementary_field_pair_without_frame_num_gap() {
        let mut access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
            h264_test_access_unit(2, 1, false),
            h264_test_access_unit(3, 2, false),
        ];
        let top_field = access_units[1].first_slice.as_mut().unwrap();
        top_field.field_pic_flag = true;
        top_field.bottom_field_flag = false;
        top_field.pic_order_cnt = [2, 0];
        let bottom_field = access_units[2].first_slice.as_mut().unwrap();
        bottom_field.field_pic_flag = true;
        bottom_field.bottom_field_flag = true;
        bottom_field.pic_order_cnt = [2, 3];
        let next_frame = access_units[3].first_slice.as_mut().unwrap();
        next_frame.num_ref_idx_l0_active_minus1 = Some(1);
        next_frame.pic_order_cnt = [4, 4];

        let plan =
            native_vulkan_h264_decode_reference_plan_with_gaps(&access_units, 4, 3, 16, false);

        assert!(
            plan.iter().all(|entry| entry.ready_for_decode_submit),
            "{plan:#?}"
        );
        assert_eq!(plan[1].current_pic_order_cnt_val, Some(2));
        assert_eq!(plan[2].current_pic_order_cnt_val, Some(3));
        assert_eq!(plan[2].references[0].frame_num, 1);
        assert!(plan[2].references[0].field_pic_flag);
        assert!(!plan[2].references[0].bottom_field_flag);
        assert_eq!(
            plan[3]
                .references
                .iter()
                .map(|reference| (
                    reference.frame_num,
                    reference.field_pic_flag,
                    reference.bottom_field_flag
                ))
                .collect::<Vec<_>>(),
            vec![(1, true, true), (1, true, false)]
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn chooses_h264_picture_layout_candidates_from_sps_and_field_window() {
        assert_eq!(
            native_vulkan_h264_picture_layout_candidates(&h264_test_sps(true), false),
            vec![vk::VideoDecodeH264PictureLayoutFlagsKHR::PROGRESSIVE]
        );
        assert_eq!(
            native_vulkan_h264_picture_layout_candidates(&h264_test_sps(false), false),
            vec![
                vk::VideoDecodeH264PictureLayoutFlagsKHR::INTERLACED_INTERLEAVED_LINES,
                vk::VideoDecodeH264PictureLayoutFlagsKHR::INTERLACED_SEPARATE_PLANES,
                vk::VideoDecodeH264PictureLayoutFlagsKHR::PROGRESSIVE,
            ]
        );
        assert_eq!(
            native_vulkan_h264_picture_layout_candidates(&h264_test_sps(false), true),
            vec![
                vk::VideoDecodeH264PictureLayoutFlagsKHR::INTERLACED_INTERLEAVED_LINES,
                vk::VideoDecodeH264PictureLayoutFlagsKHR::INTERLACED_SEPARATE_PLANES,
            ]
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn finds_h264_recovery_offset_after_non_idr_prefix() {
        let access_units = vec![
            h264_test_access_unit(0, 21, false),
            h264_test_access_unit(1, 22, false),
            h264_test_access_unit(2, 0, true),
        ];

        assert!(!native_vulkan_h264_access_unit_starts_recovery(
            &access_units[0]
        ));
        assert!(native_vulkan_h264_access_unit_starts_recovery(
            &access_units[2]
        ));
        assert_eq!(
            native_vulkan_h264_first_recovery_access_unit_offset(&access_units),
            Some(2)
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn chooses_minimum_h264_dpb_slots_for_ippp_ready_prefix() {
        let access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
            h264_test_access_unit(2, 2, false),
        ];

        let one_slot_plan = native_vulkan_h264_decode_reference_plan(&access_units, 1, 1, 16);
        assert!(one_slot_plan[0].ready_for_decode_submit);
        assert!(!one_slot_plan[1].ready_for_decode_submit);
        assert_eq!(one_slot_plan[1].missing_reference_count, 1);

        let (dpb_slots, plan) = native_vulkan_h264_min_decodable_dpb_plan(&access_units, 2, 1, 16);

        assert_eq!(dpb_slots, 2);
        assert!(
            plan.iter().all(|entry| entry.ready_for_decode_submit),
            "{plan:#?}"
        );
        assert_eq!(
            plan.iter()
                .map(|entry| entry.planned_output_slot)
                .collect::<Vec<_>>(),
            vec![0, 1, 0]
        );
        assert_eq!(
            plan.iter()
                .map(|entry| entry.available_reference_count)
                .collect::<Vec<_>>(),
            vec![0, 1, 1]
        );
        assert_eq!(plan[1].references[0].source_access_unit_index, Some(0));
        assert_eq!(plan[2].references[0].source_access_unit_index, Some(1));
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_multi_reference_ippp_ready_prefix() {
        let mut access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
            h264_test_access_unit(2, 2, false),
            h264_test_access_unit(3, 3, false),
        ];
        access_units[2]
            .first_slice
            .as_mut()
            .unwrap()
            .num_ref_idx_l0_active_minus1 = Some(1);
        access_units[3]
            .first_slice
            .as_mut()
            .unwrap()
            .num_ref_idx_l0_active_minus1 = Some(1);

        let (dpb_slots, plan) = native_vulkan_h264_min_decodable_dpb_plan(&access_units, 3, 2, 16);

        assert_eq!(dpb_slots, 3);
        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(
            plan.iter()
                .map(|entry| entry.requested_reference_count)
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 2]
        );
        assert_eq!(
            plan.iter()
                .map(|entry| entry.available_reference_count)
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 2]
        );
        assert_eq!(
            plan[2]
                .references
                .iter()
                .map(|reference| reference.source_access_unit_index)
                .collect::<Vec<_>>(),
            vec![Some(1), Some(0)]
        );
        assert_eq!(
            plan[3]
                .references
                .iter()
                .map(|reference| reference.source_access_unit_index)
                .collect::<Vec<_>>(),
            vec![Some(2), Some(1)]
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_references_with_full_pic_order_count_pair() {
        let mut access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
        ];
        access_units[0].first_slice.as_mut().unwrap().pic_order_cnt = [0, 2];

        let plan = native_vulkan_h264_decode_reference_plan(&access_units, 2, 1, 16);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[0].current_pic_order_cnt, Some([0, 2]));
        assert_eq!(plan[1].references[0].pic_order_cnt_val, 0);
        assert_eq!(plan[1].references[0].pic_order_cnt, [0, 2]);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_short_term_ref_list_modification_p_slice() {
        let mut access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
            h264_test_access_unit(2, 2, false),
        ];
        let p_slice = access_units[2].first_slice.as_mut().unwrap();
        p_slice.ref_pic_list_modification_l0 = true;
        p_slice.ref_pic_list_modifications_l0 =
            vec![NativeVulkanH264RefPicListModificationSnapshot {
                modification_of_pic_nums_idc: 0,
                abs_diff_pic_num_minus1: Some(1),
                long_term_pic_num: None,
            }];

        let (dpb_slots, plan) = native_vulkan_h264_min_decodable_dpb_plan(&access_units, 3, 2, 16);

        assert_eq!(dpb_slots, 3);
        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[2].references[0].frame_num, 0);
        assert_eq!(plan[2].references[0].source_access_unit_index, Some(0));
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn computes_h264_short_term_pic_num_across_frame_num_wrap() {
        assert_eq!(native_vulkan_h264_short_term_pic_num(15, 0, 16), -1);
        assert_eq!(native_vulkan_h264_short_term_pic_num(14, 0, 16), -2);
        assert_eq!(native_vulkan_h264_short_term_pic_num(0, 1, 16), 0);
        assert_eq!(native_vulkan_h264_short_term_pic_num(1, 1, 16), 1);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn computes_h264_field_pic_num_for_same_and_opposite_fields() {
        let top_key = NativeVulkanH264ShortTermPictureKey {
            frame_num: 7,
            field_kind: NativeVulkanH264PictureFieldKind::TopField,
        };
        let bottom_key = NativeVulkanH264ShortTermPictureKey {
            frame_num: 7,
            field_kind: NativeVulkanH264PictureFieldKind::BottomField,
        };

        assert_eq!(
            native_vulkan_h264_current_pic_num(8, NativeVulkanH264PictureFieldKind::TopField),
            17
        );
        assert_eq!(
            native_vulkan_h264_short_term_pic_num_for_key(
                top_key,
                8,
                NativeVulkanH264PictureFieldKind::TopField,
                16,
            ),
            15
        );
        assert_eq!(
            native_vulkan_h264_short_term_pic_num_for_key(
                bottom_key,
                8,
                NativeVulkanH264PictureFieldKind::TopField,
                16,
            ),
            14
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn computes_h264_field_long_term_pic_num_for_same_and_opposite_fields() {
        let top_key = NativeVulkanH264LongTermPictureKey {
            frame_idx: 3,
            field_kind: NativeVulkanH264PictureFieldKind::TopField,
        };
        let bottom_key = NativeVulkanH264LongTermPictureKey {
            frame_idx: 3,
            field_kind: NativeVulkanH264PictureFieldKind::BottomField,
        };

        assert_eq!(
            native_vulkan_h264_long_term_pic_num_for_key(
                top_key,
                NativeVulkanH264PictureFieldKind::TopField,
            ),
            7
        );
        assert_eq!(
            native_vulkan_h264_long_term_pic_num_for_key(
                bottom_key,
                NativeVulkanH264PictureFieldKind::TopField,
            ),
            6
        );
        assert_eq!(
            native_vulkan_h264_long_term_key_from_pic_num(
                7,
                NativeVulkanH264PictureFieldKind::TopField,
            )
            .unwrap(),
            top_key
        );
        assert_eq!(
            native_vulkan_h264_long_term_key_from_pic_num(
                6,
                NativeVulkanH264PictureFieldKind::TopField,
            )
            .unwrap(),
            bottom_key
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_short_term_default_list_by_pic_num_across_wrap() {
        let mut access_units = vec![
            h264_test_access_unit(0, 14, true),
            h264_test_access_unit(1, 15, false),
            h264_test_access_unit(2, 0, false),
        ];
        access_units[2]
            .first_slice
            .as_mut()
            .unwrap()
            .num_ref_idx_l0_active_minus1 = Some(1);

        let plan = native_vulkan_h264_decode_reference_plan(&access_units, 3, 2, 16);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(
            plan[2]
                .references
                .iter()
                .map(|reference| reference.frame_num)
                .collect::<Vec<_>>(),
            vec![15, 14]
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_short_term_ref_list_modification_by_pic_num_across_wrap() {
        let mut access_units = vec![
            h264_test_access_unit(0, 14, true),
            h264_test_access_unit(1, 15, false),
            h264_test_access_unit(2, 0, false),
        ];
        let p_slice = access_units[2].first_slice.as_mut().unwrap();
        p_slice.ref_pic_list_modification_l0 = true;
        p_slice.ref_pic_list_modifications_l0 =
            vec![NativeVulkanH264RefPicListModificationSnapshot {
                modification_of_pic_nums_idc: 0,
                abs_diff_pic_num_minus1: Some(0),
                long_term_pic_num: None,
            }];

        let plan = native_vulkan_h264_decode_reference_plan(&access_units, 3, 2, 16);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[2].references[0].frame_num, 15);
        assert_eq!(plan[2].references[0].source_access_unit_index, Some(1));
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_short_term_ref_list_increment_modification_by_pic_num_across_wrap() {
        let mut access_units = vec![
            h264_test_access_unit(0, 15, true),
            h264_test_access_unit(1, 0, false),
            h264_test_access_unit(2, 15, false),
        ];
        let p_slice = access_units[2].first_slice.as_mut().unwrap();
        p_slice.ref_pic_list_modification_l0 = true;
        p_slice.ref_pic_list_modifications_l0 =
            vec![NativeVulkanH264RefPicListModificationSnapshot {
                modification_of_pic_nums_idc: 1,
                abs_diff_pic_num_minus1: Some(0),
                long_term_pic_num: None,
            }];

        let plan = native_vulkan_h264_decode_reference_plan(&access_units, 17, 16, 16);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[2].references[0].frame_num, 0);
        assert_eq!(plan[2].references[0].source_access_unit_index, Some(1));
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn rejects_h264_frame_num_gap_when_sps_disallows_gaps() {
        let access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 2, false),
        ];

        let plan =
            native_vulkan_h264_decode_reference_plan_with_gaps(&access_units, 3, 1, 16, false);

        assert!(plan[0].ready_for_decode_submit);
        assert!(!plan[1].ready_for_decode_submit);
        assert!(
            plan[1]
                .unsupported_reason
                .as_deref()
                .unwrap_or_default()
                .contains("gaps_in_frame_num_value_allowed_flag is false")
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn infers_h264_non_existing_short_term_reference_for_allowed_gap() {
        let access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 2, false),
        ];

        let plan =
            native_vulkan_h264_decode_reference_plan_with_gaps(&access_units, 3, 1, 16, true);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[1].inferred_non_existing_frame_nums, vec![1]);
        assert_eq!(plan[1].inferred_non_existing_references.len(), 1);
        assert_eq!(plan[1].inferred_non_existing_references[0].frame_num, 1);
        assert_eq!(plan[1].references[0].frame_num, 1);
        assert!(plan[1].references[0].non_existing);
        assert_eq!(plan[1].references[0].source_access_unit_index, None);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn infers_h264_non_existing_short_term_reference_across_u16_frame_num_wrap() {
        let access_units = vec![
            h264_test_access_unit(0, u16::MAX - 1, true),
            h264_test_access_unit(1, 0, false),
        ];

        let plan =
            native_vulkan_h264_decode_reference_plan_with_gaps(&access_units, 3, 1, 65_536, true);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[1].inferred_non_existing_frame_nums, vec![u16::MAX]);
        assert_eq!(plan[1].references[0].frame_num, u16::MAX);
        assert!(plan[1].references[0].non_existing);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn slides_h264_inferred_non_existing_references_through_short_term_window() {
        let access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 3, false),
        ];

        let plan =
            native_vulkan_h264_decode_reference_plan_with_gaps(&access_units, 4, 2, 16, true);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[1].inferred_non_existing_frame_nums, vec![1, 2]);
        assert_eq!(plan[1].inferred_dropped_reference_frame_nums, vec![0]);
        assert_eq!(
            plan[1]
                .inferred_non_existing_references
                .iter()
                .map(|reference| reference.frame_num)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(plan[1].references[0].frame_num, 2);
        assert!(plan[1].references[0].non_existing);
        assert_eq!(plan[1].dropped_reference_frame_nums, vec![1]);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_adaptive_marking_short_term_unused_for_reference() {
        let mut access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
            h264_test_access_unit(2, 2, false),
            h264_test_access_unit(3, 3, false),
        ];
        let mmco_slice = access_units[2].first_slice.as_mut().unwrap();
        mmco_slice.adaptive_ref_pic_marking_mode_flag = true;
        mmco_slice.memory_management_control_operations =
            vec![NativeVulkanH264MemoryManagementControlOperationSnapshot {
                memory_management_control_operation: 1,
                difference_of_pic_nums_minus1: Some(1),
                long_term_pic_num: None,
                long_term_frame_idx: None,
                max_long_term_frame_idx_plus1: None,
            }];
        access_units[3]
            .first_slice
            .as_mut()
            .unwrap()
            .num_ref_idx_l0_active_minus1 = Some(1);

        let (dpb_slots, plan) = native_vulkan_h264_min_decodable_dpb_plan(&access_units, 3, 2, 16);

        assert_eq!(dpb_slots, 3);
        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[2].dropped_reference_frame_nums, vec![0]);
        assert_eq!(plan[2].dropped_reference_slots, vec![0]);
        assert_eq!(
            plan[3]
                .references
                .iter()
                .map(|reference| reference.frame_num)
                .collect::<Vec<_>>(),
            vec![2, 1]
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_mmco1_short_term_unused_across_frame_num_wrap() {
        let mut access_units = vec![
            h264_test_access_unit(0, 11, true),
            h264_test_access_unit(1, 12, false),
            h264_test_access_unit(2, 13, false),
            h264_test_access_unit(3, 14, false),
            h264_test_access_unit(4, 15, false),
            h264_test_access_unit(5, 0, false),
        ];
        let mmco_slice = access_units[5].first_slice.as_mut().unwrap();
        mmco_slice.adaptive_ref_pic_marking_mode_flag = true;
        mmco_slice.memory_management_control_operations =
            vec![h264_test_mmco(1, Some(4), None, None, None)];

        let plan = native_vulkan_h264_decode_reference_plan(&access_units, 8, 8, 16);

        assert!(
            plan.iter().all(|entry| entry.ready_for_decode_submit),
            "{plan:#?}"
        );
        assert_eq!(plan[5].dropped_reference_frame_nums, vec![11]);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_field_mmco1_drops_only_target_field() {
        let mut access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
            h264_test_access_unit(2, 1, false),
            h264_test_access_unit(3, 2, false),
        ];
        let top_field = access_units[1].first_slice.as_mut().unwrap();
        top_field.field_pic_flag = true;
        top_field.bottom_field_flag = false;
        top_field.pic_order_cnt = [2, 0];
        let bottom_field = access_units[2].first_slice.as_mut().unwrap();
        bottom_field.field_pic_flag = true;
        bottom_field.bottom_field_flag = true;
        bottom_field.pic_order_cnt = [2, 3];
        bottom_field.adaptive_ref_pic_marking_mode_flag = true;
        bottom_field.memory_management_control_operations =
            vec![h264_test_mmco(1, Some(0), None, None, None)];
        let next_frame = access_units[3].first_slice.as_mut().unwrap();
        next_frame.num_ref_idx_l0_active_minus1 = Some(1);
        next_frame.pic_order_cnt = [4, 4];

        let plan =
            native_vulkan_h264_decode_reference_plan_with_gaps(&access_units, 4, 3, 16, false);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[2].dropped_reference_frame_nums, vec![1]);
        assert_eq!(
            plan[3]
                .references
                .iter()
                .map(|reference| (
                    reference.frame_num,
                    reference.field_pic_flag,
                    reference.bottom_field_flag,
                ))
                .collect::<Vec<_>>(),
            vec![(1, true, true), (0, false, false)]
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_long_term_reference_marking_and_ref_list_modification() {
        let mut access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
            h264_test_access_unit(2, 2, false),
            h264_test_access_unit(3, 2, false),
        ];
        let mark_long_term = access_units[1].first_slice.as_mut().unwrap();
        mark_long_term.adaptive_ref_pic_marking_mode_flag = true;
        mark_long_term.memory_management_control_operations =
            vec![NativeVulkanH264MemoryManagementControlOperationSnapshot {
                memory_management_control_operation: 3,
                difference_of_pic_nums_minus1: Some(0),
                long_term_pic_num: None,
                long_term_frame_idx: Some(0),
                max_long_term_frame_idx_plus1: None,
            }];
        let long_term_reference = access_units[2].first_slice.as_mut().unwrap();
        long_term_reference.ref_pic_list_modification_l0 = true;
        long_term_reference.ref_pic_list_modifications_l0 =
            vec![NativeVulkanH264RefPicListModificationSnapshot {
                modification_of_pic_nums_idc: 2,
                abs_diff_pic_num_minus1: None,
                long_term_pic_num: Some(0),
            }];
        let drop_long_term = access_units[3].first_slice.as_mut().unwrap();
        drop_long_term.adaptive_ref_pic_marking_mode_flag = true;
        drop_long_term.memory_management_control_operations =
            vec![NativeVulkanH264MemoryManagementControlOperationSnapshot {
                memory_management_control_operation: 2,
                difference_of_pic_nums_minus1: None,
                long_term_pic_num: Some(0),
                long_term_frame_idx: None,
                max_long_term_frame_idx_plus1: None,
            }];

        let plan = native_vulkan_h264_decode_reference_plan(&access_units, 4, 2, 16);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert!(plan[1].dropped_reference_frame_nums.is_empty());
        assert_eq!(plan[2].references[0].frame_num, 0);
        assert!(plan[2].references[0].used_for_long_term_reference);
        assert_eq!(plan[2].references[0].long_term_frame_idx, Some(0));
        assert_eq!(plan[2].references[0].source_access_unit_index, Some(0));
        assert_eq!(plan[3].dropped_long_term_frame_indices, vec![0]);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_field_long_term_ref_list_and_mmco2_by_long_term_pic_num() {
        let mut access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
            h264_test_access_unit(2, 1, false),
            h264_test_access_unit(3, 2, false),
            h264_test_access_unit(4, 3, false),
        ];
        let top_field = access_units[1].first_slice.as_mut().unwrap();
        top_field.field_pic_flag = true;
        top_field.bottom_field_flag = false;
        top_field.pic_order_cnt = [2, 0];
        top_field.adaptive_ref_pic_marking_mode_flag = true;
        top_field.memory_management_control_operations =
            vec![h264_test_mmco(6, None, None, Some(2), None)];

        let bottom_field = access_units[2].first_slice.as_mut().unwrap();
        bottom_field.field_pic_flag = true;
        bottom_field.bottom_field_flag = true;
        bottom_field.pic_order_cnt = [2, 3];

        let ref_top_from_bottom = access_units[3].first_slice.as_mut().unwrap();
        ref_top_from_bottom.field_pic_flag = true;
        ref_top_from_bottom.bottom_field_flag = true;
        ref_top_from_bottom.pic_order_cnt = [4, 5];
        ref_top_from_bottom.ref_pic_list_modification_l0 = true;
        ref_top_from_bottom.ref_pic_list_modifications_l0 =
            vec![h264_test_long_term_l0_modification(4)];

        let drop_top_from_bottom = access_units[4].first_slice.as_mut().unwrap();
        drop_top_from_bottom.field_pic_flag = true;
        drop_top_from_bottom.bottom_field_flag = true;
        drop_top_from_bottom.pic_order_cnt = [6, 7];
        drop_top_from_bottom.adaptive_ref_pic_marking_mode_flag = true;
        drop_top_from_bottom.memory_management_control_operations =
            vec![h264_test_mmco(2, None, Some(4), None, None)];

        let plan =
            native_vulkan_h264_decode_reference_plan_with_gaps(&access_units, 6, 4, 16, false);

        assert!(
            plan.iter().all(|entry| entry.ready_for_decode_submit),
            "{plan:#?}"
        );
        assert_eq!(plan[3].references[0].source_access_unit_index, Some(1));
        assert!(plan[3].references[0].used_for_long_term_reference);
        assert_eq!(plan[3].references[0].long_term_frame_idx, Some(2));
        assert_eq!(plan[3].references[0].long_term_pic_num, Some(4));
        assert!(plan[3].references[0].field_pic_flag);
        assert!(!plan[3].references[0].bottom_field_flag);
        assert_eq!(plan[4].dropped_long_term_frame_indices, vec![2]);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_idr_long_term_reference_flag() {
        let mut access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
        ];
        access_units[0]
            .first_slice
            .as_mut()
            .unwrap()
            .long_term_reference_flag = true;
        let p_slice = access_units[1].first_slice.as_mut().unwrap();
        p_slice.ref_pic_list_modification_l0 = true;
        p_slice.ref_pic_list_modifications_l0 = vec![h264_test_long_term_l0_modification(0)];

        let plan = native_vulkan_h264_decode_reference_plan(&access_units, 2, 1, 16);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[0].current_long_term_frame_idx, Some(0));
        assert_eq!(plan[1].references[0].frame_num, 0);
        assert!(plan[1].references[0].used_for_long_term_reference);
        assert_eq!(plan[1].references[0].long_term_frame_idx, Some(0));
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn slides_h264_short_term_window_with_existing_long_term_reference() {
        let mut access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
            h264_test_access_unit(2, 2, false),
            h264_test_access_unit(3, 3, false),
        ];
        access_units[0]
            .first_slice
            .as_mut()
            .unwrap()
            .long_term_reference_flag = true;
        access_units[3]
            .first_slice
            .as_mut()
            .unwrap()
            .num_ref_idx_l0_active_minus1 = Some(1);

        let plan = native_vulkan_h264_decode_reference_plan(&access_units, 3, 2, 16);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[2].dropped_reference_frame_nums, vec![1]);
        assert_eq!(
            plan[3]
                .references
                .iter()
                .map(|reference| {
                    (
                        reference.frame_num,
                        reference.used_for_long_term_reference,
                        reference.source_access_unit_index,
                    )
                })
                .collect::<Vec<_>>(),
            vec![(2, false, Some(2)), (0, true, Some(0))]
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_mmco6_current_picture_as_long_term_reference() {
        let mut access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
            h264_test_access_unit(2, 2, false),
        ];
        let mark_current_long_term = access_units[1].first_slice.as_mut().unwrap();
        mark_current_long_term.adaptive_ref_pic_marking_mode_flag = true;
        mark_current_long_term.memory_management_control_operations =
            vec![h264_test_mmco(6, None, None, Some(1), None)];
        let p_slice = access_units[2].first_slice.as_mut().unwrap();
        p_slice.ref_pic_list_modification_l0 = true;
        p_slice.ref_pic_list_modifications_l0 = vec![h264_test_long_term_l0_modification(1)];

        let plan = native_vulkan_h264_decode_reference_plan(&access_units, 3, 2, 16);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[1].current_long_term_frame_idx, Some(1));
        assert_eq!(plan[2].references[0].frame_num, 1);
        assert!(plan[2].references[0].used_for_long_term_reference);
        assert_eq!(plan[2].references[0].long_term_frame_idx, Some(1));
        assert_eq!(plan[2].references[0].source_access_unit_index, Some(1));
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_mmco4_drops_long_term_references_above_limit() {
        let mut access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
            h264_test_access_unit(2, 2, false),
            h264_test_access_unit(3, 2, false),
        ];
        let convert_idr_to_long_term = access_units[1].first_slice.as_mut().unwrap();
        convert_idr_to_long_term.adaptive_ref_pic_marking_mode_flag = true;
        convert_idr_to_long_term.memory_management_control_operations =
            vec![h264_test_mmco(3, Some(0), None, Some(0), None)];
        let convert_previous_to_long_term = access_units[2].first_slice.as_mut().unwrap();
        convert_previous_to_long_term.adaptive_ref_pic_marking_mode_flag = true;
        convert_previous_to_long_term.memory_management_control_operations =
            vec![h264_test_mmco(3, Some(0), None, Some(2), None)];
        let trim_long_terms = access_units[3].first_slice.as_mut().unwrap();
        trim_long_terms.adaptive_ref_pic_marking_mode_flag = true;
        trim_long_terms.memory_management_control_operations =
            vec![h264_test_mmco(4, None, None, None, Some(1))];

        let plan = native_vulkan_h264_decode_reference_plan(&access_units, 4, 2, 16);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[1].long_term_reference_conversions[0].frame_num, 0);
        assert_eq!(
            plan[2].long_term_reference_conversions[0].long_term_frame_idx,
            2
        );
        assert_eq!(plan[3].dropped_long_term_frame_indices, vec![2]);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_mmco5_clears_all_existing_references_before_current_picture() {
        let mut access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
            h264_test_access_unit(2, 2, false),
            h264_test_access_unit(3, 3, false),
        ];
        let clear_refs = access_units[2].first_slice.as_mut().unwrap();
        clear_refs.adaptive_ref_pic_marking_mode_flag = true;
        clear_refs.memory_management_control_operations =
            vec![h264_test_mmco(5, None, None, None, None)];

        let plan = native_vulkan_h264_decode_reference_plan(&access_units, 4, 2, 16);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[2].dropped_reference_frame_nums, vec![0, 1]);
        assert_eq!(plan[3].references.len(), 1);
        assert_eq!(plan[3].references[0].frame_num, 2);
        assert_eq!(plan[3].references[0].source_access_unit_index, Some(2));
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_long_term_index_replacement() {
        let mut access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
            h264_test_access_unit(2, 2, false),
            h264_test_access_unit(3, 3, false),
        ];
        let convert_idr_to_long_term = access_units[1].first_slice.as_mut().unwrap();
        convert_idr_to_long_term.adaptive_ref_pic_marking_mode_flag = true;
        convert_idr_to_long_term.memory_management_control_operations =
            vec![h264_test_mmco(3, Some(0), None, Some(0), None)];
        let replace_long_term = access_units[2].first_slice.as_mut().unwrap();
        replace_long_term.adaptive_ref_pic_marking_mode_flag = true;
        replace_long_term.memory_management_control_operations =
            vec![h264_test_mmco(3, Some(0), None, Some(0), None)];
        let p_slice = access_units[3].first_slice.as_mut().unwrap();
        p_slice.ref_pic_list_modification_l0 = true;
        p_slice.ref_pic_list_modifications_l0 = vec![h264_test_long_term_l0_modification(0)];

        let plan = native_vulkan_h264_decode_reference_plan(&access_units, 4, 2, 16);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[2].dropped_long_term_frame_indices, vec![0]);
        assert_eq!(plan[2].long_term_reference_conversions[0].frame_num, 1);
        assert_eq!(plan[3].references[0].frame_num, 1);
        assert!(plan[3].references[0].used_for_long_term_reference);
        assert_eq!(plan[3].references[0].source_access_unit_index, Some(1));
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_non_reference_pictures_as_scratch_outputs() {
        let mut access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
            h264_test_access_unit(2, 2, false),
            h264_test_access_unit(3, 2, false),
        ];
        let non_reference = access_units[2].first_slice.as_mut().unwrap();
        non_reference.nal_ref_idc = 0;
        non_reference.is_reference = false;

        let (dpb_slots, plan) = native_vulkan_h264_min_decodable_dpb_plan(&access_units, 2, 1, 16);

        assert_eq!(dpb_slots, 2);
        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(
            plan.iter()
                .map(|entry| entry.planned_output_slot)
                .collect::<Vec<_>>(),
            vec![0, 1, 0, 0]
        );
        assert_eq!(
            plan.iter()
                .map(|entry| entry.setup_slot_index)
                .collect::<Vec<_>>(),
            vec![Some(0), Some(1), None, Some(0)]
        );
        assert_eq!(plan[2].references[0].frame_num, 1);
        assert_eq!(plan[3].references[0].frame_num, 1);
        assert_eq!(plan[3].references[0].source_access_unit_index, Some(1));
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_default_b_slice_short_term_references() {
        let mut access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
            h264_test_access_unit(2, 2, false),
            h264_test_access_unit(3, 2, false),
        ];
        let future_ref = access_units[1].first_slice.as_mut().unwrap();
        future_ref.pic_order_cnt = [2, 2];
        let b_slice = access_units[2].first_slice.as_mut().unwrap();
        b_slice.nal_ref_idc = 0;
        b_slice.slice_type = 6;
        b_slice.slice_type_normalized = vk::video::STD_VIDEO_H264_SLICE_TYPE_B.0 as u32;
        b_slice.num_ref_idx_l0_active_minus1 = Some(0);
        b_slice.num_ref_idx_l1_active_minus1 = Some(0);
        b_slice.is_reference = false;
        b_slice.is_p = false;
        b_slice.is_b = true;
        b_slice.pic_order_cnt = [1, 1];
        access_units[3].first_slice.as_mut().unwrap().pic_order_cnt = [3, 3];

        assert_eq!(
            native_vulkan_h264_access_units_max_active_references(&access_units),
            2
        );
        let (dpb_slots, plan) = native_vulkan_h264_min_decodable_dpb_plan(&access_units, 3, 2, 16);

        assert_eq!(dpb_slots, 3);
        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(
            plan.iter()
                .map(|entry| entry.planned_output_slot)
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 2]
        );
        assert_eq!(plan[2].setup_slot_index, None);
        assert_eq!(
            plan[2]
                .references
                .iter()
                .map(|reference| reference.frame_num)
                .collect::<Vec<_>>(),
            vec![0, 1]
        );
        assert_eq!(plan[3].references[0].frame_num, 1);
        assert_eq!(plan[3].references[0].source_access_unit_index, Some(1));
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_b_slice_l1_short_term_ref_list_modification() {
        let mut access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
            h264_test_access_unit(2, 2, false),
            h264_test_access_unit(3, 3, false),
        ];
        access_units[1].first_slice.as_mut().unwrap().pic_order_cnt = [4, 4];
        access_units[2].first_slice.as_mut().unwrap().pic_order_cnt = [2, 2];
        let b_slice = access_units[3].first_slice.as_mut().unwrap();
        b_slice.nal_ref_idc = 0;
        b_slice.slice_type = 6;
        b_slice.slice_type_normalized = vk::video::STD_VIDEO_H264_SLICE_TYPE_B.0 as u32;
        b_slice.num_ref_idx_l0_active_minus1 = Some(0);
        b_slice.num_ref_idx_l1_active_minus1 = Some(0);
        b_slice.ref_pic_list_modification_l1 = true;
        b_slice.ref_pic_list_modifications_l1 =
            vec![NativeVulkanH264RefPicListModificationSnapshot {
                modification_of_pic_nums_idc: 0,
                abs_diff_pic_num_minus1: Some(2),
                long_term_pic_num: None,
            }];
        b_slice.is_reference = false;
        b_slice.is_p = false;
        b_slice.is_b = true;
        b_slice.pic_order_cnt = [3, 3];

        let (dpb_slots, plan) = native_vulkan_h264_min_decodable_dpb_plan(&access_units, 4, 3, 16);

        assert_eq!(dpb_slots, 4);
        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(
            plan[3]
                .references
                .iter()
                .map(|reference| reference.frame_num)
                .collect::<Vec<_>>(),
            vec![2, 0]
        );
        assert_eq!(plan[3].references[0].source_access_unit_index, Some(2));
        assert_eq!(plan[3].references[1].source_access_unit_index, Some(0));
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
        let first_slice = stats.first_slice.expect("first slice summary");
        assert_eq!(
            native_vulkan_h265_nal_type_label(first_slice.nal_type),
            "idr-w-radl"
        );
        assert_eq!(first_slice.slice_segment_offset, 21);
        assert_eq!(first_slice.payload_start, 24);
        assert_eq!(first_slice.payload_end, bytes.len());
        let payloads = native_vulkan_h265_nal_payloads(&bytes);
        assert_eq!(payloads[3].nal_type, 19);
        assert_eq!(payloads[3].start_code_offset, 21);
        assert_eq!(payloads[3].slice_segment_offset, 21);
        assert_eq!(payloads[3].payload_offset, 24);
    }

    #[test]
    fn uses_three_byte_h265_start_code_for_slice_segment_offset() {
        let bytes = [
            0, 0, 0, 1, 0x02, 0x01, 0xaa, // TRAIL_R with four-byte Annex-B prefix
            0, 0, 1, 0x26, 0x01, 0xbb, // IDR_W_RADL with three-byte Annex-B prefix
        ];

        let payloads = native_vulkan_h265_nal_payloads(&bytes);

        assert_eq!(payloads[0].start_code_offset, 0);
        assert_eq!(payloads[0].slice_segment_offset, 1);
        assert_eq!(payloads[0].payload_offset, 4);
        assert_eq!(payloads[1].start_code_offset, 7);
        assert_eq!(payloads[1].slice_segment_offset, 7);
        assert_eq!(payloads[1].payload_offset, 10);
    }

    #[cfg(feature = "native-vulkan-video")]
    fn h265_test_access_unit(
        index: u32,
        poc: u32,
        idr: bool,
        used_delta_pocs: &[i32],
    ) -> NativeVulkanH265AccessUnitSnapshot {
        let mut short_term_reference_delta_pocs = NativeVulkanH265ReferenceDeltas::new();
        if !idr {
            for delta_poc in used_delta_pocs {
                short_term_reference_delta_pocs.push(*delta_poc);
            }
        }

        NativeVulkanH265AccessUnitSnapshot {
            index,
            bytes: 0,
            byte_hash: 0,
            pts_ns: Some(u64::from(index) * 4_166_667),
            duration_ns: Some(4_166_667),
            pts_ms: Some(u64::from(index) * 4),
            duration_ms: Some(4),
            has_annex_b_start_codes: true,
            has_parameter_sets: idr,
            h265_vps_count: u32::from(idr),
            h265_sps_count: u32::from(idr),
            h265_pps_count: u32::from(idr),
            h265_idr_count: u32::from(idr),
            h265_slice_count: 1,
            first_slice: Some(NativeVulkanH265AccessUnitSliceSnapshot {
                nal_type: if idr { 19 } else { 1 },
                nal_type_label: if idr { "idr-w-radl" } else { "trail-r" },
                slice_segment_offset: 0,
                first_slice_segment_in_pic_flag: true,
                slice_type: if idr { 2 } else { 1 },
                pps_id: 0,
                pic_order_cnt_lsb: (!idr).then_some(poc),
                short_term_ref_pic_set_sps_flag: false,
                short_term_ref_pic_set_idx: None,
                num_delta_pocs_of_ref_rps_idx: 0,
                num_bits_for_st_ref_pic_set_in_slice: 0,
                short_term_reference_delta_pocs,
                long_term_references: Vec::new(),
                idr,
                irap: idr,
            }),
            first_slice_parse_error: None,
        }
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn maps_h265_sps_long_term_refs_to_vulkan_std() {
        let std_refs = vulkan::native_vulkan_vulkanalia_h265_std_long_term_ref_pics_sps(&[
            NativeVulkanH265LongTermRefPicSpsSnapshot {
                lt_ref_pic_poc_lsb_sps: 4,
                used_by_curr_pic_lt_sps_flag: true,
            },
            NativeVulkanH265LongTermRefPicSpsSnapshot {
                lt_ref_pic_poc_lsb_sps: 9,
                used_by_curr_pic_lt_sps_flag: false,
            },
        ])
        .expect("H.265 SPS long-term refs should map")
        .expect("non-empty refs should produce STD payload");

        assert_eq!(std_refs.used_by_curr_pic_lt_sps_flag, 0b01);
        assert_eq!(std_refs.lt_ref_pic_poc_lsb_sps[0], 4);
        assert_eq!(std_refs.lt_ref_pic_poc_lsb_sps[1], 9);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h265_long_term_reference_by_poc_lsb() {
        let mut access_units = vec![
            h265_test_access_unit(0, 0, true, &[]),
            h265_test_access_unit(1, 4, false, &[]),
            h265_test_access_unit(2, 8, false, &[]),
        ];
        access_units[2]
            .first_slice
            .as_mut()
            .unwrap()
            .long_term_references = vec![NativeVulkanH265LongTermReferenceSnapshot {
            from_sps: false,
            lt_idx_sps: None,
            poc_lsb: 4,
            used_by_current: true,
            delta_poc_msb_present_flag: false,
            delta_poc_msb_cycle_lt: None,
        }];

        let plan = native_vulkan_h265_decode_reference_plan(&access_units, 3, 16);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[2].references.len(), 1);
        assert_eq!(plan[2].references[0].poc, 4);
        assert_eq!(plan[2].references[0].delta_poc, -4);
        assert!(plan[2].references[0].used_for_long_term_reference);
        assert_eq!(plan[2].references[0].source_access_unit_index, Some(1));
        assert_eq!(plan[2].references[0].dpb_slot, Some(1));

        let available_references = plan[2].references.iter().collect::<Vec<_>>();
        let st_before = native_vulkan_h265_ref_pic_set_st_curr_before(2, &available_references)
            .expect("short-term before refs should map");
        let lt_curr = native_vulkan_h265_ref_pic_set_lt_curr(2, &available_references)
            .expect("long-term refs should map");
        assert_eq!(st_before, [0xff; 8]);
        assert_eq!(lt_curr[0], 1);
        assert_eq!(&lt_curr[1..], &[0xff; 7]);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn counts_h265_mixed_short_and_long_term_active_references() {
        let mut access_units = vec![
            h265_test_access_unit(0, 0, true, &[]),
            h265_test_access_unit(1, 4, false, &[]),
            h265_test_access_unit(2, 8, false, &[-4]),
        ];
        access_units[2]
            .first_slice
            .as_mut()
            .unwrap()
            .long_term_references = vec![NativeVulkanH265LongTermReferenceSnapshot {
            from_sps: false,
            lt_idx_sps: None,
            poc_lsb: 0,
            used_by_current: true,
            delta_poc_msb_present_flag: false,
            delta_poc_msb_cycle_lt: None,
        }];

        assert_eq!(
            native_vulkan_h265_access_units_max_active_references(&access_units),
            2
        );

        let plan = native_vulkan_h265_decode_reference_plan(&access_units, 3, 16);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(
            plan[2]
                .references
                .iter()
                .map(|reference| (reference.poc, reference.used_for_long_term_reference))
                .collect::<Vec<_>>(),
            vec![(4, false), (0, true)]
        );
        assert_eq!(plan[2].available_reference_count, 2);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn h265_begin_slots_preserve_current_long_term_reference_flags() {
        let active_refs = vec![
            None,
            Some(NativeVulkanH265ActiveDpbReference {
                poc: 4,
                used_for_long_term_reference: false,
            }),
        ];
        let references = vec![NativeVulkanH265DecodeReferenceSnapshot {
            delta_poc: -4,
            poc: 4,
            used_for_long_term_reference: true,
            available: true,
            source_access_unit_index: Some(1),
            dpb_slot: Some(1),
        }];
        let policy = NativeVulkanH265BeginSlotPolicy::default();

        let begin_refs =
            native_vulkan_h265_begin_slot_refs(&active_refs, &references, false, policy);
        let slot_1 = begin_refs
            .iter()
            .find(|(slot, _)| *slot == 1)
            .expect("active reference slot should be emitted");

        assert_eq!(
            slot_1.1,
            Some(NativeVulkanH265ActiveDpbReference {
                poc: 4,
                used_for_long_term_reference: true,
            })
        );

        let reset_begin_refs =
            native_vulkan_h265_begin_slot_refs(&active_refs, &references, true, policy);
        let reset_slot_1 = reset_begin_refs
            .iter()
            .find(|(slot, _)| *slot == 1)
            .expect("pre-reset active slot should remain visible as inactive");
        assert_eq!(reset_slot_1.1, None);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn parses_predicted_h265_short_term_ref_pic_set() {
        fn push_bits(bits: &mut Vec<bool>, value: u32, count: u32) {
            for shift in (0..count).rev() {
                bits.push(((value >> shift) & 1) != 0);
            }
        }
        fn push_ue(bits: &mut Vec<bool>, value: u32) {
            let code_num = value + 1;
            let bit_count = 32 - code_num.leading_zeros();
            for _ in 0..bit_count.saturating_sub(1) {
                bits.push(false);
            }
            push_bits(bits, code_num, bit_count);
        }
        fn pack_bits(mut bits: Vec<bool>) -> Vec<u8> {
            bits.push(true);
            while !bits.len().is_multiple_of(8) {
                bits.push(false);
            }
            let mut bytes = vec![0u8; bits.len() / 8];
            for (index, bit) in bits.into_iter().enumerate() {
                if bit {
                    bytes[index / 8] |= 1 << (7 - (index % 8));
                }
            }
            bytes
        }

        let reference_rps = native_vulkan_h265_short_term_ref_pic_set_snapshot(
            false,
            None,
            None,
            None,
            0,
            Vec::new(),
            Vec::new(),
            vec![-1, -3],
            vec![true, false],
            vec![2],
            vec![true],
        );
        let mut bits = Vec::new();
        bits.push(true); // inter_ref_pic_set_prediction_flag
        push_ue(&mut bits, 0); // delta_idx_minus1: RefRpsIdx = 0
        bits.push(true); // delta_rps_sign: negative DeltaRps
        push_ue(&mut bits, 0); // abs_delta_rps_minus1: |DeltaRps| = 1
        bits.push(true); // ref negative -1 is used by current and included
        bits.push(false); // ref negative -3 is not used by current
        bits.push(true); // ref negative -3 is still included
        bits.push(true); // ref positive +2 is used by current and included
        bits.push(true); // DeltaRps -1 is used by current and included
        let bytes = pack_bits(bits);
        let mut reader = NativeVulkanH265BitReader::new(&bytes);

        let rps =
            native_vulkan_h265_read_short_term_ref_pic_set(&mut reader, 1, 1, &[reference_rps])
                .expect("predicted RPS should parse");

        assert!(rps.inter_ref_pic_set_prediction_flag);
        assert_eq!(rps.delta_idx_minus1, Some(0));
        assert_eq!(rps.delta_rps_sign, Some(true));
        assert_eq!(rps.abs_delta_rps_minus1, Some(0));
        assert_eq!(rps.num_delta_pocs_of_ref_rps_idx, 3);
        assert_eq!(rps.use_delta_flags, vec![true, true, true, true]);
        assert_eq!(rps.used_by_current_flags, vec![true, false, true, true]);
        assert_eq!(rps.negative_delta_pocs, vec![-1, -2, -4]);
        assert_eq!(rps.negative_used_by_curr_pic, vec![true, true, false]);
        assert_eq!(rps.used_negative_delta_pocs, vec![-1, -2]);
        assert_eq!(rps.positive_delta_pocs, vec![1]);
        assert_eq!(rps.positive_used_by_curr_pic, vec![true]);
        assert_eq!(rps.used_positive_delta_pocs, vec![1]);
        assert_eq!(rps.used_by_current_count, 3);

        let std_rps = vulkan::native_vulkan_vulkanalia_h265_std_short_term_ref_pic_set(&rps)
            .expect("predicted RPS should map to Vulkan STD fields");
        assert_eq!(std_rps.flags.inter_ref_pic_set_prediction_flag(), 1);
        assert_eq!(std_rps.flags.delta_rps_sign(), 1);
        assert_eq!(std_rps.delta_idx_minus1, 0);
        assert_eq!(std_rps.abs_delta_rps_minus1, 0);
        assert_eq!(std_rps.use_delta_flag, 0b1111);
        assert_eq!(std_rps.used_by_curr_pic_flag, 0b1101);
        assert_eq!(std_rps.num_negative_pics, 3);
        assert_eq!(std_rps.num_positive_pics, 1);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn marks_h265_self_evicted_reference_unready() {
        let access_units = vec![
            h265_test_access_unit(0, 0, true, &[]),
            h265_test_access_unit(1, 1, false, &[-1]),
        ];

        let plan = native_vulkan_h265_decode_reference_plan(&access_units, 1, 16);

        assert!(plan[0].ready_for_decode_submit);
        assert!(!plan[1].ready_for_decode_submit);
        assert_eq!(plan[1].planned_output_slot, 0);
        assert_eq!(plan[1].evicted_poc, Some(0));
        assert_eq!(plan[1].missing_reference_pocs, vec![0]);
        assert_eq!(plan[1].references[0].dpb_slot, Some(0));
        assert!(!plan[1].references[0].available);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn keeps_h265_b_frame_references_when_reusing_full_dpb() {
        let access_units = vec![
            h265_test_access_unit(0, 0, true, &[]),
            h265_test_access_unit(1, 3, false, &[-3]),
            h265_test_access_unit(2, 2, false, &[-2, 1]),
            h265_test_access_unit(3, 1, false, &[-1, 1, 2]),
            h265_test_access_unit(4, 6, false, &[-3, -4, -6]),
            h265_test_access_unit(5, 5, false, &[-2, -3, -5, 1]),
        ];

        let plan = native_vulkan_h265_decode_reference_plan(&access_units, 5, 16);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[5].current_poc, Some(5));
        assert_eq!(plan[5].planned_output_slot, 3);
        assert_eq!(plan[5].evicted_poc, Some(1));
        assert_eq!(plan[5].missing_reference_pocs, Vec::<i32>::new());
        assert_eq!(
            plan[5]
                .references
                .iter()
                .map(|reference| reference.poc)
                .collect::<Vec<_>>(),
            vec![3, 2, 0, 6]
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn chooses_minimum_h265_dpb_slots_by_reference_distance() {
        let access_units = vec![
            h265_test_access_unit(0, 0, true, &[]),
            h265_test_access_unit(1, 1, false, &[-1]),
            h265_test_access_unit(2, 2, false, &[-2]),
        ];

        assert_eq!(
            native_vulkan_h265_access_units_max_active_references(&access_units),
            1
        );
        let (dpb_slots, plan) = native_vulkan_h265_min_decodable_dpb_plan(&access_units, 3, 16);

        assert_eq!(dpb_slots, 2);
        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(
            plan.iter()
                .map(|entry| entry.planned_output_slot)
                .collect::<Vec<_>>(),
            vec![0, 1, 1]
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn unwraps_h265_poc_lsb_across_continuous_stream() {
        let mut access_units = Vec::new();
        access_units.push(h265_test_access_unit(0, 0, true, &[]));
        for index in 1..=15 {
            access_units.push(h265_test_access_unit(index, index, false, &[]));
        }
        access_units.push(h265_test_access_unit(16, 0, false, &[-1]));
        access_units.push(h265_test_access_unit(17, 1, false, &[-1]));

        let plan = native_vulkan_h265_decode_reference_plan(&access_units, 18, 16);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(
            plan.iter()
                .map(|entry| entry.current_poc)
                .collect::<Vec<_>>(),
            (0..=17).map(Some).collect::<Vec<_>>()
        );
        assert_eq!(plan[16].references[0].poc, 15);
        assert_eq!(plan[17].references[0].poc, 16);
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
                NativeVulkanWallpaperType::Scene,
                NativeVulkanWallpaperType::Shader,
                NativeVulkanWallpaperType::Playlist,
            ]
        );
        assert!(contract.video_interop.avoids_default_rgba_upload);
        assert_eq!(
            contract.video_pipeline.reference,
            "FFmpeg packet/frame/clock model"
        );
        assert_eq!(contract.video_pipeline.stages.len(), 10);
        assert!(
            contract
                .video_pipeline
                .stages
                .iter()
                .any(|stage| stage.owner == "native-vulkan-demux-boundary")
        );
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
            scene_plans: vec![SceneWallpaperPlan {
                output_name: "HDMI-A-1".to_owned(),
                source: Some(PathBuf::from("/tmp/scene.json")),
                manifest_max_fps: Some(60),
                target_max_fps: Some(30),
                snapshot_time_ms: 1234,
                scene_size: None,
                scene_fit: FitMode::Cover,
                scene_systems: Default::default(),
                audio_cue_count: 0,
                bound_properties: vec!["scene_opacity".to_owned()],
                timeline_animation_count: 2,
                timeline_animated_layer_count: 1,
                property_binding_count: 1,
                cursor_parallax_input_ready: true,
                display: Some(SceneDisplayPlan::Color {
                    color: "#102030".to_owned(),
                }),
                layers: vec![SceneRenderLayer {
                    id: "panel".to_owned(),
                    kind: crate::core::SceneNodeKind::Rectangle,
                    source: None,
                    texture_region: None,
                    audio: Vec::new(),
                    color: Some("#102030".to_owned()),
                    stroke_color: Some("#ffffff".to_owned()),
                    stroke_width: Some(2.0),
                    corner_radius: Some(8.0),
                    width: Some(320.0),
                    height: Some(180.0),
                    text: None,
                    font_size: None,
                    font_family: None,
                    font_weight: None,
                    text_align: None,
                    path_data: None,
                    fit: FitMode::Cover,
                    opacity: 0.75,
                    transform: crate::core::SceneTransform {
                        x: 12.0,
                        y: 24.0,
                        ..Default::default()
                    },
                }],
            }],
            removals: Vec::new(),
            errors: Vec::new(),
            decisions: Vec::new(),
            playlist_clock_dependency: Default::default(),
            cache: Default::default(),
        };

        let items = render_items_from_sync_plan(&sync_plan);

        assert_eq!(items.len(), 3);
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
        assert_eq!(items[2].wallpaper_type(), NativeVulkanWallpaperType::Scene);
        let NativeVulkanRenderItem::Scene {
            scene_source,
            display,
            display_image,
            display_color,
            manifest_max_fps,
            layer_count,
            layers,
            bound_properties,
            timeline_animation_count,
            timeline_animated_layer_count,
            property_binding_count,
            cursor_parallax_input_ready,
            snapshot_time_ms,
            target_max_fps,
            renderer_status,
            ..
        } = &items[2]
        else {
            unreachable!("item already matched as scene");
        };
        assert_eq!(scene_source, &Some(PathBuf::from("/tmp/scene.json")));
        assert_eq!(display_image, &None);
        assert_eq!(display_color.as_deref(), Some("#102030"));
        assert!(matches!(
            display,
            Some(SceneDisplayPlan::Color { color }) if color == "#102030"
        ));
        assert_eq!(*manifest_max_fps, Some(60));
        assert_eq!(*target_max_fps, Some(30));
        assert_eq!(*layer_count, 1);
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].id, "panel");
        assert_eq!(layers[0].kind, crate::core::SceneNodeKind::Rectangle);
        assert_eq!(layers[0].opacity, 0.75);
        assert_eq!(layers[0].transform.x, 12.0);
        assert_eq!(layers[0].transform.y, 24.0);
        assert_eq!(bound_properties, &vec!["scene_opacity".to_owned()]);
        assert_eq!(*timeline_animation_count, 2);
        assert_eq!(*timeline_animated_layer_count, 1);
        assert_eq!(*property_binding_count, 1);
        assert!(*cursor_parallax_input_ready);
        assert_eq!(*snapshot_time_ms, 1234);
        assert_eq!(
            *renderer_status,
            "deterministic-scene-snapshot-ready-for-vulkan-passes"
        );
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
        assert!(
            contract
                .video_interop
                .vulkan_binding_policy
                .contains("vulkanalia")
        );
        assert!(
            contract
                .video_interop
                .vulkanalia_primary_policy
                .contains("vulkanalia owns")
        );
        assert!(
            contract
                .video_interop
                .vulkanalia_primary_policy
                .contains("Vulkan Video submit helpers")
        );
        assert!(
            contract
                .video_interop
                .vulkan_1_4_value
                .contains("dynamic-rendering-local-read")
        );
        assert!(
            contract
                .video_interop
                .vulkan_binding_policy
                .contains("zero-copy evidence")
        );
        assert!(
            contract
                .video_interop
                .removed_ash_baseline
                .contains("Vulkan Video")
        );
        assert!(
            contract
                .video_interop
                .removed_ash_baseline
                .contains("external-memory")
        );
        assert_eq!(contract.vulkan_backend.binding, "vulkanalia");
        assert!(contract.vulkan_backend.api_baseline.contains("Vulkan 1.4"));
    }
}
