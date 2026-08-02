use serde::Serialize;
use std::fmt;

use crate::renderer::native_wayland::{NativeWaylandError, NativeWaylandHostOptions};

mod audio;
mod effect_debug;
mod interop;
mod pipeline;
mod present;
mod scene;
pub(in crate::renderer::native_vulkan) mod shared_presentation;
mod video;
mod vulkan;

include!("backend_contract.rs");

#[cfg(feature = "native-vulkan-video")]
pub use video::event_source::NativeVulkanMediaEventRuntimeSnapshot;
#[cfg(feature = "native-vulkan-video")]
pub use video::shared_present::{
    NativeVulkanSharedVideoPresentOptions, NativeVulkanSharedVideoPresentSnapshot,
    run_native_vulkan_shared_video_present,
};

use present::render_item;
use video::codec as video_codec;
use video::flow as video_flow;
use video::route as video_route;

pub use interop::{NativeVulkanVideoInteropContract, NativeVulkanWebInteropContract};
use interop::{video_interop_contract, web_interop_contract};
pub use render_item::{
    NativeVulkanRenderItem, NativeVulkanSceneRenderItem, render_items_from_sync_plan,
};
pub use scene::{
    BuiltinSceneParameterLayout, BuiltinSceneShader,
    NATIVE_VULKAN_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES, NativeVulkanSceneBackendPlan,
    NativeVulkanSceneDescriptorHeapPlan, NativeVulkanSceneHeapStoragePlan,
    NativeVulkanSceneMeshBufferPlan, NativeVulkanSceneMeshUploadPlan,
    NativeVulkanScenePipelineCacheEntry, NativeVulkanScenePipelineCachePlan,
    NativeVulkanSceneRenderGraphCommand, NativeVulkanSceneRenderGraphCommandKind,
    NativeVulkanSceneRenderGraphExecutorPlan, NativeVulkanSceneResourceStoragePlan,
    NativeVulkanSceneRunOptions, NativeVulkanSceneRuntimeSnapshot, NativeVulkanSceneVideoSource,
    NativeVulkanSceneShaderHeapSlice, NativeVulkanSceneShaderProgramSet,
    NativeVulkanSceneSpirvProgram, NativeVulkanSceneVertexProgram,
    native_vulkan_scene_backend_plan,
    native_vulkan_scene_backend_plan_from_render_item,
    native_vulkan_scene_backend_plan_from_semantic_frame, native_vulkan_scene_pipeline_cache_plan,
    native_vulkan_scene_render_graph_executor_plan, native_vulkan_scene_resource_storage_plan,
    native_vulkan_scene_shader_catalog, native_vulkan_scene_shader_for_key, run_scene,
    run_scene_with_options,
};
pub use video_codec::NativeVulkanVideoSessionCodec;
pub use video_route::{
    native_vulkan_video_duration_playback_frames, native_vulkan_video_playback_frame_count,
};
pub use vulkan::{
    NativeVulkanSceneOwnedUniformArenaPlanSnapshot, NativeVulkanSceneOwnedUniformSliceSnapshot,
    NativeVulkanScenePresentSnapshot, native_vulkan_scene_owned_uniform_arena_plan,
};
pub(in crate::renderer::native_vulkan) use vulkan::{
    NativeVulkanScenePresentOptions, run_native_vulkan_scene_present,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanCapabilities {
    pub built: bool,
    pub experimental: bool,
    pub default_enabled: bool,
    pub reuses_native_wayland_host: bool,
    pub owns_layer_shell_surface_now: bool,
    pub renderer_owns_vulkan_instance: bool,
    pub renderer_owns_vulkan_device: bool,
    pub renderer_owns_wayland_vulkan_surface: bool,
    pub renderer_owns_swapchain: bool,
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
        renderer_owns_vulkan_instance: true,
        renderer_owns_vulkan_device: true,
        renderer_owns_wayland_vulkan_surface: true,
        renderer_owns_swapchain: true,
        renders_frames_now: true,
        consumes_render_sync: false,
        direct_video_memory_status: "contract-only: target is importable DMABuf/EGLImage/Vulkan image sampling",
        unsafe_policy: "Vulkan unsafe is renderer-owned; product unsafe is limited to audited Wayland/DMABuf FFI boundaries",
    }
}
#[derive(Debug)]
pub enum NativeVulkanError {
    Wayland(NativeWaylandError),
    Scene(String),
    Video(String),
}

impl fmt::Display for NativeVulkanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wayland(err) => write!(f, "{err}"),
            Self::Scene(err) => write!(f, "scene error: {err}"),
            Self::Video(err) => write!(f, "video error: {err}"),
        }
    }
}

impl std::error::Error for NativeVulkanError {}

impl From<NativeWaylandError> for NativeVulkanError {
    fn from(err: NativeWaylandError) -> Self {
        Self::Wayland(err)
    }
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
        Self {
            host: NativeWaylandHostOptions {
                namespace: "gilder-native-vulkan".to_owned(),
                ..NativeWaylandHostOptions::default()
            },
            wait_configure_roundtrips: 8,
            clear_color: NativeVulkanClearColor::default(),
            target_max_fps: None,
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
