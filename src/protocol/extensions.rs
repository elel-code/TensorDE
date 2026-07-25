//! Compositor-owned Wayland extensions not yet on the Dispatch2 path.
//!
//! `virtual_pointer` and `gamma_control` are ported from Niri (same wlr
//! surfaces Hyprland exposes). They currently use wayland-server's legacy
//! `Dispatch`/`GlobalDispatch` API; Tensor's compositor state uses
//! `delegate_dispatch2!`. Wiring them requires a Dispatch2 port (next step).
//!
//! Until then the modules stay compiled for review and incremental migration.

#![allow(dead_code)]

pub mod gamma_control;
pub mod virtual_pointer;
