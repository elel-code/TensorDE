//! Seat query and cursor methods on [`NativeRuntime`].

use wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::Shape as CursorShape;

use crate::input::CursorIcon;
use crate::runtime_common::RuntimeError;
use crate::surface::SurfaceId;

use super::runtime_facade::{map_native_error, NativeRuntime};

impl NativeRuntime {
    /// Bound seats (multi-seat compositors may advertise more than one).
    pub fn seats(&self) -> Vec<crate::SeatInfo> {
        self.shell.seats()
    }

    #[inline]
    pub fn seat_count(&self) -> usize {
        self.shell.seat_count()
    }

    pub fn primary_seat_id(&self) -> Option<crate::SeatId> {
        self.shell.primary_seat_id()
    }

    pub fn seat_keyboard_focus(&self, seat: crate::SeatId) -> Option<SurfaceId> {
        let native = self.shell.seat_keyboard_focus(seat)?;
        self.surfaces.get(native)
    }

    pub fn seat_pointer_focus(&self, seat: crate::SeatId) -> Option<SurfaceId> {
        let native = self.shell.seat_pointer_focus(seat)?;
        self.surfaces.get(native)
    }

    pub fn seat_last_input_serial(&self, seat: crate::SeatId) -> Option<u32> {
        self.shell.seat_last_input_serial(seat)
    }

    pub fn seat_input_serial(
        &self,
        seat: crate::SeatId,
        source: crate::InputSerialSource,
    ) -> Option<crate::InputSerial> {
        self.shell.seat_input_serial(seat, source)
    }

    pub fn seat_has_data_device(&self, seat: crate::SeatId) -> bool {
        self.shell.seat_has_data_device(seat)
    }

    pub fn seat_has_primary_device(&self, seat: crate::SeatId) -> bool {
        self.shell.seat_has_primary_device(seat)
    }

    pub fn set_cursor(&mut self, icon: CursorIcon) -> Result<(), RuntimeError> {
        self.set_cursor_on_seat(icon, None)
    }

    /// Set the cursor shape on a specific seat's pointer (or auto-resolve).
    pub fn set_cursor_on_seat(
        &mut self,
        icon: CursorIcon,
        seat: Option<crate::SeatId>,
    ) -> Result<(), RuntimeError> {
        if !self.shell.has_cursor_shape() {
            return Err(RuntimeError::Unsupported("wp_cursor_shape_manager_v1"));
        }
        let shape = match icon {
            CursorIcon::Default => CursorShape::Default,
            CursorIcon::Pointer => CursorShape::Pointer,
            CursorIcon::Text => CursorShape::Text,
            CursorIcon::ColResize => CursorShape::ColResize,
            CursorIcon::NResize => CursorShape::NResize,
            CursorIcon::SResize => CursorShape::SResize,
            CursorIcon::EResize => CursorShape::EResize,
            CursorIcon::WResize => CursorShape::WResize,
            CursorIcon::NeResize => CursorShape::NeResize,
            CursorIcon::NwResize => CursorShape::NwResize,
            CursorIcon::SeResize => CursorShape::SeResize,
            CursorIcon::SwResize => CursorShape::SwResize,
        };
        self.shell
            .set_cursor_shape_on_seat(shape, seat)
            .map_err(map_native_error)
    }
}
