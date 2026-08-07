//! Native desktop shell built on the shared Wayland and Vulkan standards.
//!
//! The shell owns desktop-level surfaces. Application chrome belongs to the
//! applications themselves and is deliberately outside this crate.

mod config;
mod config_reload;
mod control_center_scene;
mod layout;
mod media;
mod media_osd;
mod media_osd_scene;
mod model;
mod network;
mod notification;
mod notification_scene;
mod notification_service;
mod overview;
mod overview_scene;
mod panel;
mod present;
mod render;
mod runtime;
mod session_lock_service;
mod surfaces;
mod system_status;

pub use config::{
    LauncherEndpoint, MAX_LAUNCHER_COMMAND_ARGS, MAX_LAUNCHER_COMMAND_BYTES,
    MAX_MEDIA_OSD_TIMEOUT_MS, MAX_SHELL_CONFIG_BYTES, MIN_MEDIA_OSD_TIMEOUT_MS, MediaConfig,
    PanelConfig, ShellConfig, ShellConfigError, TensorlandConfigEndpoint,
};
pub use config_reload::ShellConfigReloadError;
pub use layout::{ShellLayout, ShellLayoutError};
pub use media::{MediaActionState, MediaServiceError, MediaServiceSnapshot, media_applet_state};
pub use model::{ShellComponent, ShellModel};
pub use network::{
    NetworkActionState, NetworkServiceError, NetworkServiceSnapshot, network_applet_state,
};
pub use notification::{
    CloseReason, ClosedNotification, Notification, NotificationAction, NotificationGroup,
    NotificationId, NotificationRequest, NotificationStore, NotificationStoreConfig,
    NotificationStoreError, NotificationTimeout, NotificationUrgency,
};
pub use notification_service::NotificationServiceError;
pub use panel::{
    PanelAppletAvailability, PanelAppletEmphasis, PanelAppletState, PanelAppletStore,
    PanelAppletUpdate, PanelScene, PanelWidget, PanelWidgetKind,
};
pub use present::ShellPresentError;
pub use render::{
    LocalSceneColorPass, ShellCompositionPath, ShellEffect, ShellFramePlan, ShellFramePlanError,
    ShellRenderNode, ShellRenderScene, compile_frame_plan,
};
pub use runtime::{ShellRuntime, ShellRuntimeError};
pub use surfaces::{SurfaceKey, SurfacePlan, lock_surface_key, surface_plan};
pub use system_status::{PowerServiceSnapshot, power_applet_state};
pub use vulkan_renderer as gpu;
