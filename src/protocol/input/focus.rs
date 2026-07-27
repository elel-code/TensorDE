use tracing::warn;

use crate::{
    ecs::ViewId,
    protocol::{
        serial::{Serial, next_serial},
        state::{ProtocolWindow, RuntimeState},
    },
};

impl RuntimeState {
    /// Reapply the ECS-selected root when a keyboard capability becomes
    /// available after its window mapped.
    pub(crate) fn restore_keyboard_focus(&mut self) {
        let Some(view_id) = self.world.focused_view(self.active_workspace()) else {
            return;
        };
        let Some(window) = self.mapped_window_for_view(view_id) else {
            return;
        };
        let _ = self.focus_mapped_window(window, next_serial());
    }

    pub(crate) fn focus_mapped_window(&mut self, window: ProtocolWindow, serial: Serial) -> bool {
        if self.popup_grab.is_some() {
            return false;
        }
        let Some(surface) = window.wl_surface().map(std::borrow::Cow::into_owned) else {
            return false;
        };
        let Some(view_id) = self.view_for_surface(&surface) else {
            return false;
        };
        let focus_changed = !self.world.is_focused(view_id);
        let seat_focus_changed = self.input_seat.keyboard_focus() != Some(&surface);
        if let Err(error) = self.world.focus_view(view_id) {
            warn!(%error, view_id = view_id.get(), "failed to focus mapped view");
            return false;
        }
        self.clear_layer_on_demand_focus();
        self.publish_window_activation(Some(&window));
        if focus_changed {
            self.raise_view_family_in_space(view_id, &window);
            #[cfg(feature = "xwayland")]
            self.raise_x11_popups_for_root(&surface);
            #[cfg(feature = "xwayland")]
            if let Some(x11) = window.x11_surface()
                && let Some(xwm) = self.xwm.as_mut()
                && let Err(error) = xwm.raise_window(x11)
            {
                warn!(%error, window = x11.window_id(), "failed to synchronize XWayland stacking");
            }
        }
        let keyboard_ready = window
            .toplevel()
            .is_none_or(|toplevel| toplevel.initial_configure_sent());
        if self.input_seat.keyboard_enabled() && keyboard_ready && seat_focus_changed {
            self.set_keyboard_focus(Some(surface), serial);
        }
        focus_changed
    }

    /// Keep ECS focus, Tensor seat delivery, and xdg-toplevel activation in
    /// lockstep. Initial configure publication remains in the commit handler.
    pub(crate) fn publish_window_activation(&mut self, focused_window: Option<&ProtocolWindow>) {
        let windows = self.space.elements().cloned().collect::<Vec<_>>();
        for window in windows {
            let active = focused_window.is_some_and(|focused| window == *focused);
            if !window.set_activated(active) {
                continue;
            }
            if let Some(toplevel) = window.toplevel()
                && toplevel.initial_configure_sent()
            {
                toplevel.send_pending_configure();
            }
        }
    }

    fn raise_view_family_in_space(&mut self, focused: ViewId, focused_window: &ProtocolWindow) {
        let Some(root) = self.world.tiled_ancestor(focused) else {
            self.space.raise_element(focused_window, true);
            return;
        };
        let mut family = self.view_attachment_family(root);
        if focused != root {
            let focused_subtree = self.view_attachment_family(focused);
            family.retain(|view_id| !focused_subtree.contains(view_id));
            family.extend(focused_subtree);
        }
        for view_id in family {
            let window = (view_id == focused)
                .then(|| focused_window.clone())
                .or_else(|| self.mapped_window_for_view(view_id));
            if let Some(window) = window {
                self.space.raise_element(&window, view_id == focused);
            }
        }
    }

    fn view_attachment_family(&self, root: ViewId) -> Vec<ViewId> {
        let mut family = vec![root];
        let mut index = 0;
        while let Some(owner) = family.get(index).copied() {
            family.extend(self.world.attached_children(owner));
            index += 1;
        }
        family
    }

    pub(crate) fn mapped_window_for_view(&self, view_id: ViewId) -> Option<ProtocolWindow> {
        self.space
            .elements()
            .find(|window| {
                window
                    .wl_surface()
                    .as_deref()
                    .and_then(|surface| self.view_for_surface(surface))
                    == Some(view_id)
            })
            .cloned()
    }
}
