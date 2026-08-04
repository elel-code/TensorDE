//! Wayland/Vulkan renderer.
//!
//! This module integrates typed scene/video plans with renderer-owned Wayland
//! presentation. The presentation contract covers layer-shell ownership,
//! surface/swapchain requirements, and direct video texture interop.

#![allow(unsafe_code)]
#![allow(dead_code)]

use serde::Serialize;
use std::fmt;

use crate::renderer::wayland::{WaylandError, WaylandHostOptions};

mod audio;
mod effect_debug;
mod interop;
mod pipeline;
mod present;
mod scene;
mod scene_present;
pub(in crate::renderer::rendering_device) mod shared_presentation;
mod video;

mod presentation_contract;
pub use presentation_contract::{
    RendererRequirements, WallpaperKindSupport, WallpaperPresentationContract,
    required_device_extensions, required_instance_extensions, wallpaper_kind_support_matrix,
    wallpaper_presentation_contract,
};

#[cfg(feature = "video")]
pub use video::event_source::RenderingDeviceMediaEventRuntimeSnapshot;
#[cfg(feature = "video")]
pub use video::shared_present::{
    RenderingDeviceSharedVideoPresentOptions, RenderingDeviceSharedVideoPresentSnapshot,
    run_rendering_device_shared_video_present,
};

use present::render_item;
use video::codec as video_codec;
use video::route as video_route;

pub use interop::{RenderingDeviceVideoInteropContract, RenderingDeviceWebInteropContract};
pub use render_item::{
    RenderingDeviceRenderItem, RenderingDeviceSceneRenderItem, render_items_from_sync_plan,
};
pub use scene::{
    BuiltinSceneParameterLayout, BuiltinSceneShader,
    RENDERING_DEVICE_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES, SceneExecutionPlan,
    RenderingDeviceSceneDescriptorHeapPlan, RenderingDeviceSceneHeapStoragePlan,
    RenderingDeviceSceneMeshBufferPlan, RenderingDeviceSceneMeshUploadPlan,
    RenderingDeviceScenePipelineCacheEntry, RenderingDeviceScenePipelineCachePlan,
    RenderingDeviceSceneRenderGraphCommand, RenderingDeviceSceneRenderGraphCommandKind,
    RenderingDeviceSceneRenderGraphExecutorPlan, RenderingDeviceSceneResourceStoragePlan,
    RenderingDeviceSceneRunOptions, RenderingDeviceSceneRuntimeSnapshot,
    RenderingDeviceSceneShaderHeapSlice, RenderingDeviceSceneShaderProgramSet,
    RenderingDeviceSceneSpirvProgram, RenderingDeviceSceneVertexProgram,
    RenderingDeviceSceneVideoSource, scene_execution_plan,
    scene_execution_plan_from_render_item,
    scene_execution_plan_from_semantic_frame,
    rendering_device_scene_pipeline_cache_plan, rendering_device_scene_render_graph_executor_plan,
    rendering_device_scene_resource_storage_plan, rendering_device_scene_shader_catalog,
    rendering_device_scene_shader_for_key, run_scene, run_scene_with_options,
};
pub use scene_present::{
    RenderingDeviceSceneOwnedUniformArenaPlanSnapshot,
    RenderingDeviceSceneOwnedUniformSliceSnapshot, RenderingDeviceScenePresentSnapshot,
    rendering_device_scene_owned_uniform_arena_plan,
};
pub(in crate::renderer::rendering_device) use scene_present::{
    RenderingDeviceScenePresentOptions, run_rendering_device_scene_present,
};
pub use video_codec::RenderingDeviceVideoSessionCodec;
pub use video_route::{
    rendering_device_video_duration_playback_frames, rendering_device_video_playback_frame_count,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderingDeviceCapabilities {
    pub built: bool,
    pub experimental: bool,
    pub default_enabled: bool,
    pub reuses_wayland_host: bool,
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

pub fn capabilities() -> RenderingDeviceCapabilities {
    RenderingDeviceCapabilities {
        built: true,
        experimental: true,
        default_enabled: false,
        reuses_wayland_host: true,
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
pub enum RenderingDeviceError {
    Wayland(WaylandError),
    Scene(String),
    Video(String),
}

impl fmt::Display for RenderingDeviceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wayland(err) => write!(f, "{err}"),
            Self::Scene(err) => write!(f, "scene error: {err}"),
            Self::Video(err) => write!(f, "video error: {err}"),
        }
    }
}

impl std::error::Error for RenderingDeviceError {}

impl From<WaylandError> for RenderingDeviceError {
    fn from(err: WaylandError) -> Self {
        Self::Wayland(err)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderingDeviceOptions {
    pub host: WaylandHostOptions,
    pub wait_configure_roundtrips: usize,
    pub clear_color: RenderingDeviceClearColor,
    pub target_max_fps: Option<u32>,
}

impl Default for RenderingDeviceOptions {
    fn default() -> Self {
        Self {
            host: WaylandHostOptions {
                namespace: "tensor-wallpaper".to_owned(),
                ..WaylandHostOptions::default()
            },
            wait_configure_roundtrips: 8,
            clear_color: RenderingDeviceClearColor::default(),
            target_max_fps: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct RenderingDeviceClearColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Default for RenderingDeviceClearColor {
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
pub struct RenderingDeviceDrmDeviceSnapshot {
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
