//! Wayland layer-shell host backed by the shared TensorDE event layer.
//!
//! This module owns only the surface lifecycle and maps shared Wayland events
//! into Tensor Wallpaper scene input. Vulkan rendering remains in `rendering_device`.

#![allow(unsafe_code)]

use std::{ffi::c_void, fmt, ptr::NonNull};

use serde::Serialize;
use wayland_client_runtime::{
    LayerSurfaceLayer, NativeError as WaylandRuntimeError,
    NativeSurfaceHandle as WaylandSurfaceHandle,
};

#[cfg(feature = "rendering-device")]
mod event_source;
mod host;
mod snapshot;

#[cfg(test)]
use snapshot::scaled_buffer_dimension;

pub use host::WaylandHost;
pub use snapshot::{
    WaylandDmabufFeedbackSnapshot, WaylandDmabufFeedbackSource,
    WaylandDmabufFormatSnapshot, WaylandDmabufSnapshot,
    WaylandDmabufTrancheSnapshot, WaylandFrameCallbackSnapshot,
    WaylandOutputModeSnapshot, WaylandOutputSnapshot, WaylandSurfaceSnapshot,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaylandHostOptions {
    pub namespace: String,
    pub layer: WaylandLayer,
    pub output_name: Option<String>,
    pub opaque_region: bool,
    pub input_passthrough: bool,
    pub fractional_scale_rounding: WaylandFractionalScaleRounding,
}

impl Default for WaylandHostOptions {
    fn default() -> Self {
        Self {
            namespace: "tensor-wallpaper-surface".to_owned(),
            layer: WaylandLayer::Background,
            output_name: None,
            opaque_region: true,
            input_passthrough: true,
            fractional_scale_rounding: WaylandFractionalScaleRounding::Ceil,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WaylandLayer {
    Background,
    Bottom,
    Top,
    Overlay,
}

impl WaylandLayer {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::Bottom => "bottom",
            Self::Top => "top",
            Self::Overlay => "overlay",
        }
    }
}

impl std::str::FromStr for WaylandLayer {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "background" => Ok(Self::Background),
            "bottom" => Ok(Self::Bottom),
            "top" => Ok(Self::Top),
            "overlay" => Ok(Self::Overlay),
            other => Err(format!("unsupported Wayland layer: {other}")),
        }
    }
}

impl From<WaylandLayer> for LayerSurfaceLayer {
    fn from(layer: WaylandLayer) -> Self {
        match layer {
            WaylandLayer::Background => Self::Background,
            WaylandLayer::Bottom => Self::Bottom,
            WaylandLayer::Top => Self::Top,
            WaylandLayer::Overlay => Self::Overlay,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WaylandFractionalScaleRounding {
    Ceil,
    Nearest,
    Floor,
}

impl std::str::FromStr for WaylandFractionalScaleRounding {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "ceil" => Ok(Self::Ceil),
            "nearest" | "round" => Ok(Self::Nearest),
            "floor" => Ok(Self::Floor),
            other => Err(format!(
                "unsupported Wayland fractional scale rounding: {other}"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct WaylandCapabilities {
    pub built: bool,
    pub experimental: bool,
    pub owns_wlr_layer_shell_surface: bool,
    pub exports_raw_wayland_handles: bool,
    pub direct_video_overlay: bool,
    pub supports_fractional_scale_protocol: bool,
    pub supports_viewporter_protocol: bool,
    pub probes_linux_dmabuf_protocol: bool,
    pub dmabuf_buffer_attach: bool,
    pub consumes_render_sync: bool,
    pub unsafe_policy: &'static str,
}

pub fn capabilities() -> WaylandCapabilities {
    WaylandCapabilities {
        built: true,
        experimental: true,
        owns_wlr_layer_shell_surface: true,
        exports_raw_wayland_handles: true,
        direct_video_overlay: false,
        supports_fractional_scale_protocol: true,
        supports_viewporter_protocol: true,
        probes_linux_dmabuf_protocol: true,
        dmabuf_buffer_attach: false,
        consumes_render_sync: false,
        unsafe_policy: "unsafe is isolated behind shared Wayland handles and audited Vulkan boundaries",
    }
}

#[derive(Debug, Clone)]
pub struct WaylandSurfaceHandles {
    /// Retained renderer lease. The shared Vulkan surface owns a clone so the
    /// Wayland connection and `wl_surface` outlive every swapchain image.
    pub renderer_handle: WaylandSurfaceHandle,
    pub display: NonNull<c_void>,
    pub surface: NonNull<c_void>,
    pub logical_size: (u32, u32),
    pub buffer_size: (u32, u32),
    pub dmabuf_main_device: Option<u64>,
}

impl WaylandSurfaceHandles {
    pub fn window_handle(&self) -> usize {
        self.surface.as_ptr() as usize
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WaylandError {
    Wayland(String),
    MissingRawHandle(&'static str),
    MissingPreferredFractionalScale,
    Timeout(String),
}

impl fmt::Display for WaylandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wayland(err) => write!(f, "wayland error: {err}"),
            Self::MissingRawHandle(handle) => write!(f, "missing Wayland {handle} handle"),
            Self::MissingPreferredFractionalScale => {
                write!(
                    f,
                    "missing Wayland preferred fractional scale for configured surface"
                )
            }
            Self::Timeout(message) => write!(f, "timeout: {message}"),
        }
    }
}

impl std::error::Error for WaylandError {}

impl From<WaylandRuntimeError> for WaylandError {
    fn from(error: WaylandRuntimeError) -> Self {
        Self::Wayland(error.to_string())
    }
}

#[cfg(test)]
mod tests;
