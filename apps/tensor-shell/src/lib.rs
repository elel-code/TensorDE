//! Native desktop shell built on the shared Wayland and Vulkan standards.
//!
//! The shell owns desktop-level surfaces. Application chrome belongs to the
//! applications themselves and is deliberately outside this crate.

mod layout;
mod model;
mod notification;
mod notification_service;
mod present;
mod render;
mod runtime;
mod surfaces;

pub use layout::{ShellLayout, ShellLayoutError};
pub use model::{ShellComponent, ShellModel};
pub use notification::{
    CloseReason, ClosedNotification, Notification, NotificationAction, NotificationGroup,
    NotificationId, NotificationRequest, NotificationStore, NotificationStoreConfig,
    NotificationStoreError, NotificationTimeout, NotificationUrgency,
};
pub use notification_service::NotificationServiceError;
pub use present::ShellPresentError;
pub use render::{
    LocalSceneColorPass, ShellCompositionPath, ShellEffect, ShellFramePlan, ShellFramePlanError,
    ShellRenderNode, ShellRenderScene, compile_frame_plan,
};
pub use runtime::{ShellRuntime, ShellRuntimeError};
pub use surfaces::{SurfaceKey, SurfacePlan, surface_plan};
pub use vulkan_renderer as gpu;
