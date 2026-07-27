//! xdg-popup positioning against window and layer-shell parents.

use smithay::utils::{Logical, Rectangle};
use wayland_server::protocol::wl_surface::WlSurface;

#[cfg(feature = "tty")]
use crate::protocol::state::layer::WlrLayer;

use super::registry::{PopupKind, find_popup_root_surface, get_popup_toplevel_coords};
use crate::protocol::state::RuntimeState;

impl RuntimeState {
    /// Reposition an xdg popup against its window or layer-shell parent.
    pub(crate) fn unconstrain_popup(&self, popup: &PopupKind) {
        let Ok(root) = find_popup_root_surface(popup) else {
            return;
        };
        if self.view_for_surface(&root).is_some() {
            self.unconstrain_window_popup(popup, &root);
            return;
        }
        #[cfg(feature = "tty")]
        self.unconstrain_layer_popup(popup, &root);
    }

    fn unconstrain_window_popup(&self, popup: &PopupKind, root: &WlSurface) {
        let Some(window) = self
            .space
            .elements()
            .find(|window| window.wl_surface().as_deref() == Some(root))
        else {
            return;
        };
        let Some(output) = self.space.outputs_for_element(window).next() else {
            return;
        };
        let Some(output_geo) = self.space.output_geometry(output) else {
            return;
        };
        let Some(window_geo) = self.space.element_geometry(window) else {
            return;
        };
        let mut target = Rectangle::new(
            (
                output_geo.loc.x.saturating_sub(window_geo.loc.x),
                output_geo.loc.y.saturating_sub(window_geo.loc.y),
            )
                .into(),
            output_geo.size,
        );
        target.loc -= get_popup_toplevel_coords(popup);
        position_xdg_popup(popup, target);
    }

    #[cfg(feature = "tty")]
    fn unconstrain_layer_popup(&self, popup: &PopupKind, root: &WlSurface) {
        let Some(context) = self.layer_popup_context(root) else {
            return;
        };
        let Some(output_geo) = self.space.output_geometry(&context.output) else {
            return;
        };
        let mut target = match context.layer {
            WlrLayer::Background | WlrLayer::Bottom => context.non_exclusive_zone,
            WlrLayer::Top | WlrLayer::Overlay => Rectangle::from_size(output_geo.size),
        };
        target.loc -= context.geometry.loc;
        target.loc -= get_popup_toplevel_coords(popup);
        position_xdg_popup(popup, target);
    }
}

fn position_xdg_popup(popup: &PopupKind, target: Rectangle<i32, Logical>) {
    let PopupKind::Xdg(surface) = popup else {
        return;
    };
    surface.with_pending_state(|state| {
        state.geometry = state.positioner.get_unconstrained_geometry(target);
    });
}

#[cfg(test)]
mod tests {
    use super::position_xdg_popup;
    use crate::protocol::state::popup::PopupKind;
    use smithay::utils::{Logical, Rectangle};

    #[test]
    fn position_helper_accepts_tensor_popup_kinds() {
        let _ = position_xdg_popup as fn(&PopupKind, Rectangle<i32, Logical>);
    }
}
