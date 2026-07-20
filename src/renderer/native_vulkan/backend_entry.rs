use serde::Serialize;
use std::fmt;

use crate::renderer::native_wayland::{NativeWaylandError, NativeWaylandHostOptions};
use vulkanalia::vk;

mod audio;
mod effect_debug;
mod interop;
mod labels;
mod pipeline;
mod present;
mod scene;
mod video;
mod vulkan;

include!("backend_contract.rs");

#[cfg(feature = "native-vulkan-video")]
use video::ffmpeg_hw;

#[cfg(feature = "native-vulkan-video")]
pub use video::event_source::NativeVulkanMediaEventRuntimeSnapshot;

use audio::policy as audio_policy;
use present::clear_runtime as clear_present_runtime;
use present::render_item;
use present::static_image_runtime as static_image_present_runtime;
use video::codec as video_codec;
use video::flow as video_flow;
use video::route as video_route;

pub use audio_policy::{NativeVulkanAudioOutputMode, NativeVulkanAudioOutputPolicy};
pub use clear_present_runtime::run_clear;
#[cfg(feature = "native-vulkan-video")]
pub use ffmpeg_hw::{
    NativeVulkanFfmpegHwDecodeBackendContract, NativeVulkanFfmpegHwDecodeCodecContract,
    NativeVulkanFfmpegHwDecodeDevicePolicy, NativeVulkanFfmpegVulkanHwDecoderSnapshot,
    NativeVulkanFfmpegVulkanHwDeviceBorrowSnapshot, NativeVulkanFfmpegVulkanHwFrameContract,
    native_vulkan_ffmpeg_hw_decode_backend_contract, native_vulkan_ffmpeg_hw_decode_codec_contracts,
    native_vulkan_ffmpeg_vulkan_hw_frame_contract,
};
pub use interop::{NativeVulkanVideoInteropContract, NativeVulkanWebInteropContract};
use interop::{video_interop_contract, web_interop_contract};
pub use render_item::{NativeVulkanRenderItem, render_items_from_sync_plan};
pub use scene::{
    BuiltinSceneParameterLayout, BuiltinSceneShader,
    NATIVE_VULKAN_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES, NativeVulkanSceneBackendPlan,
    NativeVulkanSceneDescriptorHeapPlan, NativeVulkanSceneHeapStoragePlan,
    NativeVulkanSceneMeshBufferPlan, NativeVulkanSceneMeshUploadPlan,
    NativeVulkanScenePipelineCacheEntry, NativeVulkanScenePipelineCachePlan,
    NativeVulkanSceneRenderGraphCommand, NativeVulkanSceneRenderGraphCommandKind,
    NativeVulkanSceneRenderGraphExecutorPlan, NativeVulkanSceneResourceStoragePlan,
    NativeVulkanSceneRunOptions, NativeVulkanSceneRuntimeSnapshot,
    NativeVulkanSceneShaderHeapSlice, native_vulkan_scene_backend_plan,
    native_vulkan_scene_backend_plan_from_render_item, native_vulkan_scene_pipeline_cache_plan,
    native_vulkan_scene_render_graph_executor_plan, native_vulkan_scene_resource_storage_plan,
    native_vulkan_scene_shader_catalog, native_vulkan_scene_shader_for_key, run_scene,
    run_scene_with_options,
};
pub use static_image_present_runtime::{run_static_image, run_static_image_vulkanalia};
pub use video_codec::NativeVulkanVideoSessionCodec;
pub use video_route::{
    native_vulkan_video_duration_playback_frames, native_vulkan_video_playback_frame_count,
};
pub use vulkan::*;

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
