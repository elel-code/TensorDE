mod popup;
mod transient;

use tensor_runtime::{OpaqueFdCompletionRuntime, WorkerTx};
use tracing::warn;
use wayland_server::{Client, Resource, protocol::wl_surface::WlSurface};

use crate::{
    ecs::{ViewId, ViewPlacement},
    layout::SizeConstraints,
    protocol::xwayland::{
        X11PropertyResult, X11PropertyRuntime, X11Surface, X11Wm, XWayland, XWaylandClientData,
    },
};
use tensor_util::{LogicalRect, Rect, Size};

use super::{ProtocolWindow, RuntimeState, xdg_size_constraints};

pub(super) use popup::XWaylandPopupRegistry;
pub(super) use transient::XWaylandTransientRegistry;

pub(super) struct XWaylandProcess {
    instance: XWayland,
    client: Client,
    events: WorkerTx<tensor_runtime::OpaqueFdCompletion>,
    control: WorkerTx<String>,
    property_events: WorkerTx<X11PropertyResult>,
    property_control: WorkerTx<String>,
    xwm_runtime: Option<OpaqueFdCompletionRuntime>,
    property_runtime: Option<X11PropertyRuntime>,
}

/// The two independent signals needed before an X11 window can become a
/// rootless Wayland view. X11 mapping and shell association can arrive in either order.
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
        self.protocol_ready() && !self.registered
    }

    fn protocol_ready(self) -> bool {
        self.map_requested && self.surface_associated
    }

    fn mark_registered(&mut self) {
        self.registered = true;
    }

    fn mark_unregistered(&mut self) {
        self.registered = false;
    }

    fn take_registered(&mut self) -> bool {
        let registered = self.registered;
        *self = Self::default();
        registered
    }
}

impl RuntimeState {
    pub(crate) fn has_xwayland_process(&self) -> bool {
        self.xwayland_process.is_some()
    }

    pub(crate) fn install_xwayland_process(
        &mut self,
        instance: XWayland,
        client: Client,
        events: WorkerTx<tensor_runtime::OpaqueFdCompletion>,
        control: WorkerTx<String>,
        property_events: WorkerTx<X11PropertyResult>,
        property_control: WorkerTx<String>,
    ) {
        assert!(
            self.xwayland_process.is_none(),
            "XWayland process was installed more than once"
        );
        self.xwayland_process = Some(XWaylandProcess {
            instance,
            client,
            events,
            control,
            property_events,
            property_control,
            xwm_runtime: None,
            property_runtime: None,
        });
    }

    /// Consume one completed XWayland displayfd notification on this thread.
    ///
    /// `Ok(None)` means a partial/spurious notification and requires rearming;
    /// `Ok(Some(display))` means the XWM was installed and the watcher can end.
    pub(crate) fn complete_xwayland_startup(&mut self) -> Result<Option<u32>, String> {
        let (socket, display_number, client, events, control, property_events, property_control) = {
            let process = self
                .xwayland_process
                .as_mut()
                .ok_or_else(|| "XWayland completion arrived without a process".to_owned())?;
            let socket = process
                .instance
                .take_socket()
                .map_err(|error| error.to_string())?;
            (
                socket,
                process.instance.display_number(),
                process.client.clone(),
                process.events.clone(),
                process.control.clone(),
                process.property_events.clone(),
                process.property_control.clone(),
            )
        };
        let Some(socket) = socket else {
            return Ok(None);
        };
        let _client_data = client
            .get_data::<XWaylandClientData>()
            .ok_or_else(|| "XWayland client lost its Tensor client data".to_owned())?;

        // wl_output and fractional-scale state remain the only X11 coordinate authority.
        self.compositor_state.set_client_scale(&client, 1.0);
        let (property_runtime, property_requests) =
            X11PropertyRuntime::prepare().map_err(|error| error.to_string())?;
        let xwm = X11Wm::start(socket, property_requests).map_err(|error| error.to_string())?;
        let property_runtime = property_runtime
            .start(display_number, property_events, property_control)
            .map_err(|error| error.to_string())?;
        let runtime = OpaqueFdCompletionRuntime::start(
            "tensor-xwayland-x11-completions",
            xwm.completion_fd(),
            events,
            control,
        )
        .map_err(|error| error.to_string())?;
        self.install_xwm(xwm)?;
        let process = self
            .xwayland_process
            .as_mut()
            .expect("XWayland process disappeared during XWM startup");
        process.xwm_runtime = Some(runtime);
        process.property_runtime = Some(property_runtime);
        Ok(Some(display_number))
    }

    pub(crate) fn install_xwm(&mut self, xwm: X11Wm) -> Result<(), String> {
        if self.xwm.is_some() {
            return Err("XWayland attempted to install a second live XWM".to_owned());
        }
        self.xwm = Some(xwm);
        Ok(())
    }

    pub(crate) fn drain_xwm_events(&mut self) -> Result<(), String> {
        self.xwm
            .as_mut()
            .ok_or_else(|| "X11 completion arrived without XWM state".to_owned())?
            .drain_events()
            .map_err(|error| error.to_string())?;
        while let Some(event) = self.xwm.as_mut().and_then(X11Wm::next_event) {
            self.handle_xwm_event(event);
        }
        Ok(())
    }

    pub(crate) fn apply_x11_property_result(
        &mut self,
        result: X11PropertyResult,
    ) -> Result<(), String> {
        self.xwm
            .as_mut()
            .ok_or_else(|| "X11 property completion arrived without XWM state".to_owned())?
            .apply_property_result(result)
            .map_err(|error| error.to_string())?;
        while let Some(event) = self.xwm.as_mut().and_then(X11Wm::next_event) {
            self.handle_xwm_event(event);
        }
        Ok(())
    }

    pub(crate) fn stop_xwayland(&mut self) {
        self.xwm = None;
        self.xwayland_process = None;
    }

    /// Attach an associated, mapped rootless X11 window to the ordinary
    /// `ProtocolWindow`/ECS/scene pipeline. X11's global coordinates are never
    /// used as a layout authority.
    pub(crate) fn register_x11_window(&mut self, x11: X11Surface) -> Option<ViewId> {
        self.register_x11_window_with_placement(x11, None)
    }

    /// Register an X11 view with its final placement before its first
    /// configure. This prevents transient dialogs from briefly entering the
    /// tiled layout while XWayland lifecycle events are reconciled.
    pub(super) fn register_x11_window_with_placement(
        &mut self,
        x11: X11Surface,
        placement: Option<ViewPlacement>,
    ) -> Option<ViewId> {
        let surface = x11.wl_surface()?;
        if let Some(view_id) = self.view_for_surface(&surface) {
            let placement_changed = if let Some(placement) = placement {
                match self.world.set_view_placement(view_id, placement) {
                    Ok(changed) => changed,
                    Err(error) => {
                        warn!(%error, window = x11.window_id(), "failed to update XWayland view placement");
                        return None;
                    }
                }
            } else {
                false
            };
            if self.update_x11_constraints(&surface, &x11) || placement_changed {
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
            .spawn_view(view_id, self.active_workspace())
            .expect("monotonic view IDs must be unique");
        if let Some(placement) = placement
            && let Err(error) = self.world.set_view_placement(view_id, placement)
        {
            warn!(%error, window = x11.window_id(), "failed to place rootless XWayland view");
            let _ = self.world.remove_view(view_id);
            return None;
        }
        self.surface_views.insert(surface.id(), view_id);
        let window = ProtocolWindow::new_x11(x11.clone());
        self.space.map_element(window.clone(), (0, 0), false);
        self.update_x11_constraints(&surface, &x11);
        #[cfg(feature = "tty")]
        let _ = self.focus_mapped_window(window, crate::protocol::serial::next_serial());
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
        self.xwayland_transients.remove(window_id);
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
        let protocol_ready = {
            let lifecycle = self.xwayland_windows.entry(window_id).or_default();
            update(lifecycle);
            lifecycle.protocol_ready()
        };
        if !protocol_ready {
            return None;
        }

        if x11.is_transient_for().is_some() {
            self.xwayland_transients.observe(x11.clone());
            self.reconcile_x11_transients();
        } else {
            self.restore_x11_tiled_window(x11.clone());
            self.reconcile_x11_transients();
        }
        x11.wl_surface()
            .and_then(|surface| self.view_for_surface(&surface))
    }

    fn unregister_x11_window(&mut self, x11: &X11Surface) -> Option<ViewId> {
        let surface = x11.wl_surface()?;
        self.unregister_toplevel(&surface)
    }

    fn update_x11_constraints(&mut self, surface: &WlSurface, x11: &X11Surface) -> bool {
        self.update_toplevel_constraints(surface, x11_size_constraints(x11))
    }

    pub(super) fn managed_x11_window(&self, window_id: u32) -> Option<X11Surface> {
        self.space
            .elements()
            .filter_map(ProtocolWindow::x11_surface)
            .find(|surface| surface.window_id() == window_id)
            .cloned()
    }
}

pub(super) fn configure_x11_window(x11: &X11Surface, geometry: Rect) {
    if let Err(error) = x11.configure(Some(x11_configure_rect(geometry))) {
        warn!(%error, window = x11.window_id(), "failed to configure rootless XWayland window");
    }
}

fn x11_size_constraints(x11: &X11Surface) -> SizeConstraints {
    let min = x11.min_size().unwrap_or((0, 0).into());
    let max = x11.max_size().unwrap_or((0, 0).into());
    xdg_size_constraints(
        Size::new(
            u32::try_from(min.w).unwrap_or(0),
            u32::try_from(min.h).unwrap_or(0),
        ),
        Size::new(
            u32::try_from(max.w).unwrap_or(0),
            u32::try_from(max.h).unwrap_or(0),
        ),
    )
}

fn x11_configure_rect(geometry: Rect) -> LogicalRect<i32> {
    LogicalRect::new(
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
