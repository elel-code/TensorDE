//! Native Wayland layer-shell host backed by the shared TensorDE event layer.
//!
//! This module owns only the surface lifecycle and maps shared native events
//! into Gilder scene input. Vulkan rendering remains in `native_vulkan`.

#![allow(unsafe_code)]

use std::{ffi::c_void, fmt, ptr::NonNull};

use serde::Serialize;
use wayland_client_runtime::LayerSurfaceLayer;

#[cfg(feature = "native-vulkan-renderer")]
mod event_source;
mod host;
mod snapshot;

#[cfg(test)]
use snapshot::native_scaled_buffer_dimension;

pub use host::NativeWaylandHost;
pub use snapshot::{
    NativeWaylandDmabufFeedbackSnapshot, NativeWaylandDmabufFeedbackSource,
    NativeWaylandDmabufFormatSnapshot, NativeWaylandDmabufSnapshot,
    NativeWaylandDmabufTrancheSnapshot, NativeWaylandFrameCallbackSnapshot,
    NativeWaylandOutputModeSnapshot, NativeWaylandOutputSnapshot, NativeWaylandSurfaceSnapshot,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeWaylandHostOptions {
    pub namespace: String,
    pub layer: NativeWaylandLayer,
    pub output_name: Option<String>,
    pub opaque_region: bool,
    pub input_passthrough: bool,
    pub fractional_scale_rounding: NativeWaylandFractionalScaleRounding,
}

impl Default for NativeWaylandHostOptions {
    fn default() -> Self {
        Self {
            namespace: "gilder-wallpaper-native".to_owned(),
            layer: NativeWaylandLayer::Background,
            output_name: None,
            opaque_region: true,
            input_passthrough: true,
            fractional_scale_rounding: NativeWaylandFractionalScaleRounding::Ceil,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeWaylandLayer {
    Background,
    Bottom,
    Top,
    Overlay,
}

impl NativeWaylandLayer {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::Bottom => "bottom",
            Self::Top => "top",
            Self::Overlay => "overlay",
        }
    }
}

impl std::str::FromStr for NativeWaylandLayer {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "background" => Ok(Self::Background),
            "bottom" => Ok(Self::Bottom),
            "top" => Ok(Self::Top),
            "overlay" => Ok(Self::Overlay),
            other => Err(format!("unsupported native Wayland layer: {other}")),
        }
    }
}

impl From<NativeWaylandLayer> for LayerSurfaceLayer {
    fn from(layer: NativeWaylandLayer) -> Self {
        match layer {
            NativeWaylandLayer::Background => Self::Background,
            NativeWaylandLayer::Bottom => Self::Bottom,
            NativeWaylandLayer::Top => Self::Top,
            NativeWaylandLayer::Overlay => Self::Overlay,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeWaylandFractionalScaleRounding {
    Ceil,
    Nearest,
    Floor,
}

impl std::str::FromStr for NativeWaylandFractionalScaleRounding {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "ceil" => Ok(Self::Ceil),
            "nearest" | "round" => Ok(Self::Nearest),
            "floor" => Ok(Self::Floor),
            other => Err(format!(
                "unsupported native Wayland fractional scale rounding: {other}"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeWaylandCapabilities {
    pub built: bool,
    pub experimental: bool,
    pub owns_wlr_layer_shell_surface: bool,
    pub exports_raw_wayland_handles: bool,
    pub native_video_overlay: bool,
    pub supports_fractional_scale_protocol: bool,
    pub supports_viewporter_protocol: bool,
    pub probes_linux_dmabuf_protocol: bool,
    pub native_dmabuf_buffer_attach: bool,
    pub consumes_render_sync: bool,
    pub unsafe_policy: &'static str,
}

pub fn capabilities() -> NativeWaylandCapabilities {
    NativeWaylandCapabilities {
        built: true,
        experimental: true,
        owns_wlr_layer_shell_surface: true,
        exports_raw_wayland_handles: true,
        native_video_overlay: false,
        supports_fractional_scale_protocol: true,
        supports_viewporter_protocol: true,
        probes_linux_dmabuf_protocol: true,
        native_dmabuf_buffer_attach: false,
        consumes_render_sync: false,
        unsafe_policy: "unsafe is isolated behind shared Wayland handles and audited Vulkan boundaries",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeWaylandSurfaceHandles {
    pub display: NonNull<c_void>,
    pub surface: NonNull<c_void>,
    pub logical_size: (u32, u32),
    pub buffer_size: (u32, u32),
    pub dmabuf_main_device: Option<u64>,
}

impl NativeWaylandSurfaceHandles {
    pub fn window_handle(self) -> usize {
        self.surface.as_ptr() as usize
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeWaylandError {
    Wayland(String),
    MissingRawHandle(&'static str),
    MissingPreferredFractionalScale,
    Timeout(String),
}

impl fmt::Display for NativeWaylandError {
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

impl std::error::Error for NativeWaylandError {}

impl From<wayland_client_runtime::NativeError> for NativeWaylandError {
    fn from(error: wayland_client_runtime::NativeError) -> Self {
        Self::Wayland(error.to_string())
    }
}

#[cfg(test)]
mod tests;
