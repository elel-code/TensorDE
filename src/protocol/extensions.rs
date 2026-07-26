//! Compositor-owned Wayland extensions on the Dispatch2 path.
//!
//! Virtual-pointer and gamma-control follow Niri/Hyprland wlr protocol surfaces
//! and implement Smithay's modern `Dispatch2`/`GlobalDispatch2` so
//! `delegate_dispatch2!(RuntimeState)` routes them correctly.

pub mod gamma_control;
pub mod virtual_pointer;
