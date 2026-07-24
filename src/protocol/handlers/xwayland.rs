use smithay::{
    reexports::wayland_server::Resource,
    utils::{Logical, Rectangle},
    wayland::xwayland_shell::{XWaylandShellHandler, XWaylandShellState},
    xwayland::{
        X11Surface, X11Wm, XwmHandler,
        xwm::{Reorder, ResizeEdge, XwmId},
    },
};
use tracing::{debug, warn};

use super::RuntimeState;

impl XWaylandShellHandler for RuntimeState {
    fn xwayland_shell_state(&mut self) -> &mut XWaylandShellState {
        &mut self.xwayland_shell_state
    }

    fn surface_associated(
        &mut self,
        _xwm: XwmId,
        wl_surface: smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
        window: X11Surface,
    ) {
        if window.is_override_redirect() {
            debug!(
                window = window.window_id(),
                surface = ?wl_surface.id().protocol_id(),
                "rootless XWayland override-redirect surface remains unmanaged"
            );
            return;
        }
        let view_id = self.x11_surface_associated(window);
        debug!(
            surface = ?wl_surface.id().protocol_id(),
            view_id = ?view_id.map(|id| id.get()),
            "associated rootless XWayland surface with the ordinary scene path"
        );
    }
}

impl XwmHandler for RuntimeState {
    fn xwm_state(&mut self, xwm: XwmId) -> &mut X11Wm {
        RuntimeState::xwm_state(self, xwm)
    }

    fn new_window(&mut self, _xwm: XwmId, window: X11Surface) {
        debug!(window = window.window_id(), "new rootless XWayland window");
    }

    fn new_override_redirect_window(&mut self, _xwm: XwmId, window: X11Surface) {
        debug!(
            window = window.window_id(),
            "new XWayland override-redirect window"
        );
    }

    fn map_window_request(&mut self, _xwm: XwmId, window: X11Surface) {
        if let Err(error) = window.set_mapped(true) {
            warn!(%error, window = window.window_id(), "failed to map rootless XWayland window");
            return;
        }
        let _ = self.x11_map_requested(window);
    }

    fn map_window_notify(&mut self, _xwm: XwmId, window: X11Surface) {
        let _ = self.x11_map_requested(window);
    }

    fn mapped_override_redirect_window(&mut self, _xwm: XwmId, window: X11Surface) {
        debug!(
            window = window.window_id(),
            "XWayland override-redirect window is not promoted to a tiled rootless view"
        );
    }

    fn unmapped_window(&mut self, _xwm: XwmId, window: X11Surface) {
        let _ = self.x11_window_gone(&window);
        if let Err(error) = window.set_mapped(false) {
            warn!(%error, window = window.window_id(), "failed to unmap rootless XWayland window");
        }
    }

    fn destroyed_window(&mut self, _xwm: XwmId, window: X11Surface) {
        let _ = self.x11_window_gone(&window);
    }

    fn configure_request(
        &mut self,
        _xwm: XwmId,
        window: X11Surface,
        _x: Option<i32>,
        _y: Option<i32>,
        _width: Option<u32>,
        _height: Option<u32>,
        _reorder: Option<Reorder>,
    ) {
        // X11 coordinates are client requests, never layout authority. A
        // reflow sends the current ordinary logical geometry back to XWayland.
        if !self.reflow_default_workspace()
            && let Err(error) = window.configure(None)
        {
            warn!(%error, window = window.window_id(), "failed to acknowledge XWayland configure request");
        }
    }

    fn configure_notify(
        &mut self,
        _xwm: XwmId,
        window: X11Surface,
        _geometry: Rectangle<i32, Logical>,
        _above: Option<u32>,
    ) {
        // The compositor owns logical placement. Accepting this notification
        // as a relocation would create the separate X11 coordinate model that
        // fractional scaling deliberately avoids.
        debug!(
            window = window.window_id(),
            "ignored XWayland configure-notify placement"
        );
    }

    fn resize_request(
        &mut self,
        _xwm: XwmId,
        _window: X11Surface,
        _button: u32,
        _edge: ResizeEdge,
    ) {
        let _ = self.reflow_default_workspace();
    }

    fn move_request(&mut self, _xwm: XwmId, _window: X11Surface, _button: u32) {
        let _ = self.reflow_default_workspace();
    }

    fn disconnected(&mut self, _xwm: XwmId) {
        self.xwm = None;
    }
}
