//! Toplevel interaction and window-state methods on [`NativeRuntime`].

use crate::geometry::LogicalPosition;
use crate::native::connection::NativeError;
use crate::runtime_common::RuntimeError;
use crate::surface::SurfaceId;

use super::runtime_facade::{NativeRuntime, map_native_error};

impl NativeRuntime {
    pub fn begin_interactive_move(&mut self, surface: SurfaceId) -> Result<(), RuntimeError> {
        self.begin_interactive_move_on_seat(surface, None)
    }

    /// Start an interactive move using a specific seat's serial (or auto-resolve).
    pub fn begin_interactive_move_on_seat(
        &mut self,
        surface: SurfaceId,
        seat: Option<crate::SeatId>,
    ) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        self.shell
            .begin_interactive_move_on_seat(native, seat)
            .map_err(|e| match e {
                NativeError::Protocol(msg) if msg.contains("serial") => {
                    RuntimeError::InvalidToplevelInteractionSerial
                }
                other => map_native_error(other),
            })
    }

    pub fn begin_interactive_resize(
        &mut self,
        surface: SurfaceId,
        edge: crate::ResizeEdge,
    ) -> Result<(), RuntimeError> {
        self.begin_interactive_resize_on_seat(surface, edge, None)
    }

    /// Start an interactive resize using a specific seat's serial (or auto-resolve).
    pub fn begin_interactive_resize_on_seat(
        &mut self,
        surface: SurfaceId,
        edge: crate::ResizeEdge,
        seat: Option<crate::SeatId>,
    ) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        self.shell
            .begin_interactive_resize_on_seat(native, edge, seat)
            .map_err(|e| match e {
                NativeError::Protocol(msg) if msg.contains("serial") => {
                    RuntimeError::InvalidToplevelInteractionSerial
                }
                other => map_native_error(other),
            })
    }

    pub fn show_window_menu(
        &mut self,
        surface: SurfaceId,
        position: LogicalPosition,
    ) -> Result<(), RuntimeError> {
        self.show_window_menu_on_seat(surface, position, None)
    }

    /// Show the window menu using a specific seat's serial (or auto-resolve).
    pub fn show_window_menu_on_seat(
        &mut self,
        surface: SurfaceId,
        position: LogicalPosition,
        seat: Option<crate::SeatId>,
    ) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        self.shell
            .show_window_menu_on_seat(native, position, seat)
            .map_err(|e| match e {
                NativeError::Protocol(msg) if msg.contains("serial") => {
                    RuntimeError::InvalidToplevelInteractionSerial
                }
                other => map_native_error(other),
            })
    }

    pub fn preferred_toplevel_icon_sizes(&self) -> Vec<u32> {
        self.shell.preferred_icon_sizes().to_vec()
    }

    pub fn set_maximized(
        &mut self,
        surface: SurfaceId,
        maximized: bool,
    ) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        self.shell
            .set_maximized(native, maximized)
            .map_err(map_native_error)
    }

    pub fn set_fullscreen(
        &mut self,
        surface: SurfaceId,
        fullscreen: bool,
    ) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        self.shell
            .set_fullscreen(native, fullscreen)
            .map_err(map_native_error)
    }

    /// Enable/disable idle inhibit for a surface (no-op style error if unsupported).
    pub fn set_idle_inhibit(
        &mut self,
        surface: SurfaceId,
        inhibit: bool,
    ) -> Result<(), RuntimeError> {
        if !self.shell.has_idle_inhibit() {
            return Err(RuntimeError::Unsupported("zwp_idle_inhibit_manager_v1"));
        }
        let native = self.native(surface)?;
        self.shell
            .set_idle_inhibit(native, inhibit)
            .map_err(map_native_error)
    }

    pub fn set_minimized(&mut self, surface: SurfaceId) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        self.shell.set_minimized(native).map_err(map_native_error)
    }
}
