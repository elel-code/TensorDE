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
mod effect_debug;
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
use video::ffmpeg_hw;

#[cfg(feature = "native-vulkan-video")]
use video::codec_reference;

use audio::policy as audio_policy;
use present::clear_runtime as clear_present_runtime;
use present::render_item;
use present::static_image_runtime as static_image_present_runtime;
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
#[cfg(feature = "native-vulkan-video")]
pub use demux_ffmpeg::native_vulkan_resolve_ffmpeg_video_session_codec;
#[cfg(feature = "native-vulkan-video")]
pub use ffmpeg_hw::{
    NativeVulkanFfmpegHwDecodeBackendContract, NativeVulkanFfmpegHwDecodeCodecContract,
    NativeVulkanFfmpegHwDecodeDevicePolicy, NativeVulkanFfmpegHwDecodeFallbackPolicy,
    NativeVulkanFfmpegVulkanHwDecoderSnapshot, NativeVulkanFfmpegVulkanHwDeviceBorrowSnapshot,
    NativeVulkanFfmpegVulkanHwFrameContract, native_vulkan_ffmpeg_hw_decode_backend_contract,
    native_vulkan_ffmpeg_hw_decode_codec_contracts, native_vulkan_ffmpeg_vulkan_hw_frame_contract,
};
pub use interop::{NativeVulkanVideoInteropContract, NativeVulkanWebInteropContract};
use interop::{video_interop_contract, web_interop_contract};
pub use render_item::{NativeVulkanRenderItem, render_items_from_sync_plan};
pub use scene::{
    BuiltinSceneShader, NativeVulkanSceneBackendPlan, NativeVulkanSceneDescriptorHeapPlan,
    NativeVulkanSceneHeapStoragePlan, NativeVulkanSceneMeshBufferPlan,
    NativeVulkanSceneMeshUploadPlan, NativeVulkanScenePipelineCacheEntry,
    NativeVulkanScenePipelineCachePlan, NativeVulkanSceneRenderGraphCommand,
    NativeVulkanSceneRenderGraphCommandKind, NativeVulkanSceneRenderGraphExecutorPlan,
    NativeVulkanSceneResourceStoragePlan, NativeVulkanSceneRuntimeSnapshot,
    NativeVulkanSceneShaderHeapSlice, native_vulkan_scene_backend_plan,
    native_vulkan_scene_backend_plan_from_render_item, native_vulkan_scene_pipeline_cache_plan,
    native_vulkan_scene_render_graph_executor_plan, native_vulkan_scene_resource_storage_plan,
    native_vulkan_scene_shader_catalog, native_vulkan_scene_shader_for_key, run_scene,
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
