mod popup;

use smithay::{
    desktop::Window,
    reexports::wayland_server::{Resource, protocol::wl_surface::WlSurface},
    utils::{Logical, Rectangle},
    xwayland::{X11Surface, X11Wm, xwm::XwmId},
};
use tracing::warn;

use crate::{ecs::ViewId, layout::SizeConstraints};
use tensor_util::Rect;

use super::{DEFAULT_WORKSPACE, RuntimeState, xdg_size_constraints};

pub(super) use popup::XWaylandPopupRegistry;

/// The two independent signals needed before an X11 window can become a
/// rootless Wayland view. Smithay can report them in either order.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct XWaylandWindowLifecycle {
    map_requested: bool,
    surface_associated: bool,
    registered: bool,
}

impl XWaylandWindowLifecycle {
    fn map_requested(&mut self) {
        self.map_requested = true;
    }

    fn surface_associated(&mut self) {
        self.surface_associated = true;
    }

    fn should_register(self) -> bool {
        self.map_requested && self.surface_associated && !self.registered
    }

    fn mark_registered(&mut self) {
        self.registered = true;
    }

    fn take_registered(&mut self) -> bool {
        let registered = self.registered;
        *self = Self::default();
        registered
    }
}

impl RuntimeState {
    pub(crate) fn install_xwm(&mut self, xwm: X11Wm) {
        if self.xwm.replace(xwm).is_some() {
            warn!("replacing a live XWayland XWM after a second ready event");
        }
    }

    pub(crate) fn xwm_state(&mut self, xwm_id: XwmId) -> &mut X11Wm {
        let xwm = self
            .xwm
            .as_mut()
            .expect("XWayland event arrived without XWM state");
        assert_eq!(
            xwm.id(),
            xwm_id,
            "XWayland event arrived for an unknown XWM"
        );
        xwm
    }

    /// Attach an associated, mapped rootless X11 window to the ordinary
    /// Wayland `Window`/ECS/scene pipeline. X11's global coordinates are never
    /// used as a layout authority.
    pub(crate) fn register_x11_window(&mut self, x11: X11Surface) -> Option<ViewId> {
        let surface = x11.wl_surface()?;
        if let Some(view_id) = self.view_for_surface(&surface) {
            if self.update_x11_constraints(&surface, &x11) {
                self.reflow_default_workspace();
            }
            return Some(view_id);
        }

        #[cfg(feature = "tty")]
        if self
            .surface_buffers
            .register_view_root(surface.id())
            .is_none()
        {
            warn!("surface identity space is exhausted; rejecting rootless XWayland window");
            return None;
        }

        let view_id = self.allocate_view_id();
        self.world
            .spawn_view(view_id, DEFAULT_WORKSPACE)
            .expect("monotonic view IDs must be unique");
        self.surface_views.insert(surface.id(), view_id);
        self.space
            .map_element(Window::new_x11_window(x11.clone()), (0, 0), false);
        self.update_x11_constraints(&surface, &x11);
        self.reflow_default_workspace();
        self.reconcile_x11_popups();
        Some(view_id)
    }

    pub(crate) fn x11_map_requested(&mut self, x11: X11Surface) -> Option<ViewId> {
        self.update_x11_lifecycle(x11, XWaylandWindowLifecycle::map_requested)
    }

    pub(crate) fn x11_surface_associated(&mut self, x11: X11Surface) -> Option<ViewId> {
        self.update_x11_lifecycle(x11, XWaylandWindowLifecycle::surface_associated)
    }

    pub(crate) fn x11_window_gone(&mut self, x11: &X11Surface) -> Option<ViewId> {
        let window_id: u32 = x11.window_id();
        let registered = self
            .xwayland_windows
            .remove(&window_id)
            .is_some_and(|mut lifecycle| lifecycle.take_registered());
        registered
            .then(|| self.unregister_x11_window(x11))
            .flatten()
    }

    fn update_x11_lifecycle(
        &mut self,
        x11: X11Surface,
        update: impl FnOnce(&mut XWaylandWindowLifecycle),
    ) -> Option<ViewId> {
        let window_id: u32 = x11.window_id();
        let should_register = {
            let lifecycle = self.xwayland_windows.entry(window_id).or_default();
            update(lifecycle);
            lifecycle.should_register()
        };
        if !should_register {
            return None;
        }

        let view_id = self.register_x11_window(x11);
        if view_id.is_some()
            && let Some(lifecycle) = self.xwayland_windows.get_mut(&window_id)
        {
            lifecycle.mark_registered();
        }
        view_id
    }

    fn unregister_x11_window(&mut self, x11: &X11Surface) -> Option<ViewId> {
        let surface = x11.wl_surface()?;
        self.unregister_toplevel(&surface)
    }

    fn update_x11_constraints(&mut self, surface: &WlSurface, x11: &X11Surface) -> bool {
        self.update_toplevel_constraints(surface, x11_size_constraints(x11))
    }
}

pub(super) fn configure_x11_window(x11: &X11Surface, geometry: Rect) {
    if let Err(error) = x11.configure(x11_configure_rect(geometry)) {
        warn!(%error, window = x11.window_id(), "failed to configure rootless XWayland window");
    }
}

fn x11_size_constraints(x11: &X11Surface) -> SizeConstraints {
    let min = x11.min_size().unwrap_or((0, 0).into());
    let max = x11.max_size().unwrap_or((0, 0).into());
    xdg_size_constraints(min, max)
}

fn x11_configure_rect(geometry: Rect) -> Rectangle<i32, Logical> {
    Rectangle::new(
        (geometry.x, geometry.y).into(),
        (
            i32::try_from(geometry.width).unwrap_or(i32::MAX),
            i32::try_from(geometry.height).unwrap_or(i32::MAX),
        )
            .into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tensor_util::{OutputScale, Size};

    #[test]
    fn x11_configure_reuses_layout_logical_coordinates() {
        let rect = x11_configure_rect(Rect::new(-12, 8, 1536, 864));

        assert_eq!(rect.loc, (-12, 8).into());
        assert_eq!(rect.size, (1536, 864).into());
    }

    #[test]
    fn x11_configure_saturates_only_unrepresentable_layout_extents() {
        let rect = x11_configure_rect(Rect::new(0, 0, u32::MAX, u32::MAX));

        assert_eq!(rect.size, (i32::MAX, i32::MAX).into());
    }

    #[test]
    fn fractional_output_layout_is_not_scaled_again_for_x11() {
        let scale = OutputScale::from_f64(1.25).unwrap();
        let logical = scale.logical_size_ceil(Size::new(1920, 1080));
        let rect = x11_configure_rect(Rect::new(0, 0, logical.width, logical.height));

        assert_eq!(rect.size, (1536, 864).into());
    }

    #[test]
    fn mapping_waits_for_both_x11_and_wayland_lifecycle_signals() {
        let mut lifecycle = XWaylandWindowLifecycle::default();

        lifecycle.surface_associated();
        assert!(!lifecycle.should_register());
        lifecycle.map_requested();
        assert!(lifecycle.should_register());
        lifecycle.mark_registered();
        assert!(!lifecycle.should_register());
        assert!(lifecycle.take_registered());
        assert_eq!(lifecycle, XWaylandWindowLifecycle::default());
    }

    #[test]
    fn unmap_without_a_registered_view_is_a_noop() {
        let mut lifecycle = XWaylandWindowLifecycle::default();
        lifecycle.map_requested();

        assert!(!lifecycle.take_registered());
        assert_eq!(lifecycle, XWaylandWindowLifecycle::default());
    }
}
