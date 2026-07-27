//! Compositor-owned Wayland extensions on Tensor's direct dispatch path.
//!
//! Virtual-pointer and gamma-control follow Niri/Hyprland wlr protocol surfaces
//! while every extension delegates directly through `wayland-server`. Smithay
//! protocol state is not part of this boundary.

pub mod ext_workspace;
pub mod gamma_control;
pub mod output_management;
pub mod security_context;
pub mod virtual_pointer;
