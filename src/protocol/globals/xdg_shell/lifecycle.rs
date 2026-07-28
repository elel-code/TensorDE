//! Runtime integration for Tensor-owned xdg roles.

use tracing::warn;
use wayland_server::protocol::wl_seat;

use crate::protocol::serial::Serial;
use crate::protocol::state::{PopupKind, RuntimeState, find_popup_root_surface};

use super::{Popup, Toplevel};

impl RuntimeState {
    pub(in crate::protocol) fn register_xdg_popup(&mut self, popup: Popup) {
        let popup = PopupKind::from(popup);
        let surface = popup.wl_surface().clone();
        self.unconstrain_popup(&popup);
        match self.popups.track_popup(popup) {
            Ok(()) => self.update_surface_scale(&surface),
            Err(error) => warn!(%error, "failed to track xdg popup"),
        }
    }

    pub(in crate::protocol) fn xdg_popup_destroyed(&mut self) {
        self.popups.cleanup();
    }

    pub(in crate::protocol) fn xdg_toplevel_destroyed(&mut self, toplevel: Toplevel) {
        self.protocol_globals
            .xdg_toplevel_destroyed(toplevel.xdg_toplevel());
        self.unregister_toplevel(toplevel.wl_surface());
    }

    pub(in crate::protocol) fn xdg_toplevel_unmapped(&mut self, toplevel: &Toplevel) {
        self.update_foreign_toplevel(toplevel.wl_surface(), None, None);
    }

    pub(in crate::protocol) fn handle_xdg_popup_grab(
        &mut self,
        popup: Popup,
        seat: wl_seat::WlSeat,
        serial: u32,
    ) {
        if !self.protocol_globals.seat.owns(&seat) {
            return;
        }
        let popup = PopupKind::from(popup);
        let Ok(root) = find_popup_root_surface(&popup) else {
            return;
        };
        let is_view = self.view_for_surface(&root).is_some();
        #[cfg(feature = "tty")]
        let is_layer = self.is_layer_root(&root);
        #[cfg(not(feature = "tty"))]
        let is_layer = false;
        if !is_view && !is_layer {
            let _ = self.popups.dismiss_popup(&root, &popup);
            return;
        }
        #[cfg(feature = "tty")]
        if is_view && self.layer_blocks_window_popup_grabs() {
            let _ = self.popups.dismiss_popup(&root, &popup);
            return;
        }

        let serial = Serial::from(serial);
        let nested_serial = self
            .popup_grab
            .as_ref()
            .is_some_and(|grab| grab.serial() == serial || grab.previous_serial() == Some(serial));
        if !nested_serial
            && !self.input_seat.pointer_has_serial(serial)
            && !self.input_seat.keyboard_has_serial(serial)
        {
            return;
        }
        let Ok(grab) = self.popups.grab_popup(root, popup, serial) else {
            return;
        };
        let focus = grab.current_grab();
        self.popup_grab = Some(grab);
        self.set_keyboard_focus(Some(focus), serial);
    }
}
