//! Interactive toplevel move / resize / window menu for [`NativeShell`].

use wayland_protocols::xdg::shell::client::xdg_toplevel::ResizeEdge as WireResizeEdge;

use super::api::NativeShell;
use super::types::NativeSurfaceId;
use crate::geometry::LogicalPosition;
use crate::native::connection::NativeError;
use crate::toplevel_interaction::ResizeEdge;

impl NativeShell {
    /// Compositor-driven move using the latest input serial (pointer press preferred).
    pub fn begin_interactive_move(&mut self, id: NativeSurfaceId) -> Result<(), NativeError> {
        let (seat, serial) = self.grab_seat_serial()?;
        let record = self
            .state
            .toplevels
            .get(&id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?;
        record.toplevel._move(&seat, serial);
        self.connection.flush()?;
        Ok(())
    }

    /// Compositor-driven resize from the given edge/corner.
    pub fn begin_interactive_resize(
        &mut self,
        id: NativeSurfaceId,
        edge: ResizeEdge,
    ) -> Result<(), NativeError> {
        let (seat, serial) = self.grab_seat_serial()?;
        let record = self
            .state
            .toplevels
            .get(&id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?;
        record.toplevel.resize(&seat, serial, map_resize_edge(edge));
        self.connection.flush()?;
        Ok(())
    }

    /// Show the compositor window menu at a surface-local position.
    pub fn show_window_menu(
        &mut self,
        id: NativeSurfaceId,
        position: LogicalPosition,
    ) -> Result<(), NativeError> {
        let (seat, serial) = self.grab_seat_serial()?;
        let record = self
            .state
            .toplevels
            .get(&id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?;
        record
            .toplevel
            .show_window_menu(&seat, serial, position.x, position.y);
        self.connection.flush()?;
        Ok(())
    }

    /// Hint the cursor position while the pointer is locked on `id`.
    pub fn set_locked_pointer_position_hint(
        &mut self,
        id: NativeSurfaceId,
        position: (f64, f64),
    ) -> Result<(), NativeError> {
        let Some((surface, proxy)) = self.state.locked_pointer.as_ref() else {
            return Err(NativeError::Protocol("pointer is not locked".into()));
        };
        if *surface != id {
            return Err(NativeError::Protocol(
                "locked pointer is not for this surface".into(),
            ));
        }
        proxy.set_cursor_position_hint(position.0, position.1);
        self.connection.flush()?;
        Ok(())
    }

    pub fn set_maximized(&mut self, id: NativeSurfaceId, maximized: bool) -> Result<(), NativeError> {
        let record = self
            .state
            .toplevels
            .get(&id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?;
        if maximized {
            record.toplevel.set_maximized();
        } else {
            record.toplevel.unset_maximized();
        }
        self.connection.flush()?;
        Ok(())
    }

    pub fn set_fullscreen(
        &mut self,
        id: NativeSurfaceId,
        fullscreen: bool,
    ) -> Result<(), NativeError> {
        let record = self
            .state
            .toplevels
            .get(&id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?;
        if fullscreen {
            record.toplevel.set_fullscreen(None);
        } else {
            record.toplevel.unset_fullscreen();
        }
        self.connection.flush()?;
        Ok(())
    }

    pub fn set_minimized(&mut self, id: NativeSurfaceId) -> Result<(), NativeError> {
        let record = self
            .state
            .toplevels
            .get(&id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?;
        record.toplevel.set_minimized();
        self.connection.flush()?;
        Ok(())
    }

    fn grab_seat_serial(
        &self,
    ) -> Result<(wayland_client::protocol::wl_seat::WlSeat, u32), NativeError> {
        let seat = self
            .state
            .seat
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("no seat".into()))?
            .clone();
        let serial = self.state.last_input_serial.ok_or_else(|| {
            NativeError::Protocol("no input serial for toplevel interaction".into())
        })?;
        Ok((seat, serial))
    }
}

fn map_resize_edge(edge: ResizeEdge) -> WireResizeEdge {
    match edge {
        ResizeEdge::Top => WireResizeEdge::Top,
        ResizeEdge::Bottom => WireResizeEdge::Bottom,
        ResizeEdge::Left => WireResizeEdge::Left,
        ResizeEdge::Right => WireResizeEdge::Right,
        ResizeEdge::TopLeft => WireResizeEdge::TopLeft,
        ResizeEdge::TopRight => WireResizeEdge::TopRight,
        ResizeEdge::BottomLeft => WireResizeEdge::BottomLeft,
        ResizeEdge::BottomRight => WireResizeEdge::BottomRight,
    }
}
