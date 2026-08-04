use tracing::{debug, warn};
use wayland_server::{Resource, protocol::wl_surface::WlSurface};

use crate::protocol::{
    state::RuntimeState,
    xwayland::{WmWindowProperty, XwmEvent},
};

impl RuntimeState {
    pub(crate) fn associate_x11_surface(&mut self, window: u32, surface: WlSurface) {
        let Some(x11) = self.xwm.as_ref().and_then(|xwm| xwm.window(window)) else {
            warn!(
                window,
                surface = surface.id().protocol_id(),
                "xwayland-shell referenced an unknown X11 window"
            );
            return;
        };
        x11.set_wl_surface(Some(surface.clone()));
        if x11.is_override_redirect() {
            self.x11_popup_surface_associated(x11, surface.clone());
            debug!(
                window,
                surface = surface.id().protocol_id(),
                "associated rootless XWayland popup surface"
            );
        } else {
            let view = self.x11_surface_associated(x11);
            debug!(
                window,
                surface = surface.id().protocol_id(),
                view = ?view.map(|view| view.get()),
                "associated rootless XWayland surface"
            );
        }
    }

    pub(crate) fn handle_xwm_event(&mut self, event: XwmEvent) {
        match event {
            XwmEvent::NewWindow(window) => {
                if window.is_override_redirect() {
                    self.x11_popup_new(window.clone());
                } else {
                    self.x11_window_new(window.clone());
                }
                debug!(window = window.window_id(), "created rootless X11 window");
            }
            XwmEvent::MapRequested(window) => {
                if window.is_override_redirect() {
                    self.x11_popup_mapped(window);
                    return;
                }
                if let Err(error) = window.set_mapped(true) {
                    window.cancel_map_request();
                    warn!(%error, window = window.window_id(), "failed to map rootless X11 window");
                    return;
                }
                let _ = self.x11_map_requested(window);
            }
            XwmEvent::Mapped(window) => {
                if window.is_override_redirect() {
                    self.x11_popup_mapped(window);
                } else {
                    let _ = self.x11_map_requested(window);
                }
            }
            XwmEvent::Unmapped(window) => {
                if window.is_override_redirect() {
                    self.x11_popup_unmapped(&window);
                } else {
                    let _ = self.x11_window_gone(&window);
                }
                window.set_wl_surface(None);
            }
            XwmEvent::Destroyed(window) => {
                if window.is_override_redirect() {
                    self.x11_popup_destroyed(&window);
                } else {
                    let _ = self.x11_window_gone(&window);
                }
            }
            XwmEvent::ConfigureRequested {
                window,
                width,
                height,
            } => {
                if window.is_override_redirect()
                    || self.x11_transient_configure_requested(&window, width, height)
                {
                    return;
                }
                if !self.reflow_default_workspace()
                    && let Err(error) = window.configure(None)
                {
                    warn!(%error, window = window.window_id(), "failed to acknowledge X11 configure request");
                }
            }
            XwmEvent::Configured { window, above } => {
                if window.is_override_redirect() {
                    self.x11_popup_configured(window, above);
                }
            }
            XwmEvent::PropertyChanged(window, property) => match property {
                WmWindowProperty::TransientFor if window.is_override_redirect() => {
                    self.x11_popup_transient_for_changed(window);
                }
                WmWindowProperty::TransientFor => self.x11_transient_for_changed(window),
                WmWindowProperty::NormalHints if !window.is_override_redirect() => {
                    self.x11_normal_hints_changed(window);
                }
                WmWindowProperty::NormalHints => {}
                WmWindowProperty::Title => {
                    if let Some(surface) = window.wl_surface() {
                        let title = window.title();
                        self.update_foreign_toplevel(&surface, Some(&title), None);
                    }
                }
                WmWindowProperty::Class => {
                    if let Some(surface) = window.wl_surface() {
                        let app_id = window.app_id();
                        self.update_foreign_toplevel(&surface, None, Some(&app_id));
                    }
                }
            },
            XwmEvent::SurfaceSerial { window, serial } => {
                self.xwayland_window_serial_received(window, serial);
            }
            XwmEvent::FocusRequested(window) => {
                #[cfg(feature = "tty")]
                let protocol_window = self
                    .space
                    .elements()
                    .find(|candidate| candidate.x11_surface() == Some(&window))
                    .cloned();
                if let Some(protocol_window) = protocol_window {
                    let _ = self.focus_mapped_window(
                        protocol_window,
                        crate::protocol::serial::next_serial(),
                    );
                }
                #[cfg(not(feature = "tty"))]
                let _ = window;
            }
            XwmEvent::CloseRequested(window) => {
                if let Err(error) = window.request_close() {
                    warn!(%error, window = window.window_id(), "failed to close rootless X11 window");
                }
            }
            XwmEvent::ReflowRequested => {
                let _ = self.reflow_default_workspace();
            }
        }
    }
}
