#[cfg(feature = "tty")]
mod dmabuf;
mod protocols;
#[cfg(feature = "xwayland")]
mod xwayland;
#[cfg(feature = "tty")]
use super::cursor::CursorImage;
#[cfg(feature = "tty")]
use dmabuf::{ExplicitSyncCommit, take_explicit_sync_points};
use smithay::{
    input::{
        Seat, SeatHandler, SeatState,
        dnd::DndGrabHandler,
        pointer::{CursorImageStatus, Focus},
    },
    utils::Serial,
    wayland::{
        buffer::BufferHandler,
        compositor::{
            CompositorClientState, CompositorHandler, CompositorState, get_parent,
            is_sync_subsurface, with_states,
        },
        seat::WaylandFocus,
        selection::{
            SelectionHandler,
            data_device::{
                DataDeviceHandler, DataDeviceState, WaylandDndGrabHandler, set_data_device_focus,
            },
            primary_selection::{
                PrimarySelectionHandler, PrimarySelectionState, set_primary_focus,
            },
        },
        shell::xdg::{
            PopupSurface, PositionerState, SurfaceCachedState, ToplevelSurface, XdgShellHandler,
            XdgShellState,
        },
    },
};
use tracing::warn;
use wayland_server::{
    Client, Resource,
    protocol::{wl_buffer, wl_seat, wl_surface::WlSurface},
};

#[cfg(feature = "xwayland")]
use smithay::xwayland::XWaylandClientData;

use super::{
    focus::KeyboardFocusTarget,
    state::{
        PopupKind, RuntimeState, WaylandClientState, destroy_surface_state,
        find_popup_root_surface, on_commit_surface_handler,
        popup::{PopupGrabHandler, PopupKeyboardGrab, PopupPointerGrab},
        xdg_size_constraints,
    },
};

impl PopupGrabHandler for RuntimeState {
    fn dismiss_grabbed_popup(
        &mut self,
        root: &WlSurface,
        popup: &PopupKind,
    ) -> Result<(), smithay::utils::DeadResource> {
        self.popups.dismiss_popup(root, popup)
    }
}

impl CompositorHandler for RuntimeState {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        #[cfg(feature = "xwayland")]
        if let Some(state) = client.get_data::<XWaylandClientData>() {
            return &state.compositor_state;
        }
        &client
            .get_data::<WaylandClientState>()
            .expect("all Tensor Wayland clients carry compositor state")
            .compositor_state
    }

    fn commit(&mut self, surface: &WlSurface) {
        #[cfg(feature = "tty")]
        let mut explicit_sync = match take_explicit_sync_points(surface) {
            ExplicitSyncCommit::None => None,
            ExplicitSyncCommit::Points(points) => Some(points),
            ExplicitSyncCommit::Rejected => {
                on_commit_surface_handler(surface);
                let root = self
                    .owning_view_root(surface)
                    .unwrap_or_else(|| surface_root(surface));
                self.discard_deferred_surface_sync(surface);
                self.unregister_toplevel(&root);
                self.flush_client_releases();
                return;
            }
        };
        on_commit_surface_handler(surface);
        let fifo_activated = self
            .protocol_globals
            .surface_timing
            .take_fifo_activation(surface);
        self.popups.commit(surface);

        #[cfg(feature = "tty")]
        if self.handle_session_lock_commit(surface) || self.handle_layer_shell_commit(surface) {
            if fifo_activated {
                self.request_redraw_all();
            }
            if let Some(points) = explicit_sync.take() {
                self.finish_unused_explicit_sync(points);
            }
            self.flush_client_releases();
            return;
        }

        #[cfg(feature = "tty")]
        let mut content_changed = false;
        #[cfg(feature = "tty")]
        let mut reflowed = false;
        let root = surface_root(surface);
        #[cfg(feature = "tty")]
        let root = self.owning_view_root(surface).unwrap_or(root);

        if is_sync_subsurface(surface) {
            #[cfg(feature = "tty")]
            if let Some(view_id) = self.view_for_surface(&root) {
                self.defer_surface_sync(&root, surface, explicit_sync.take());
                self.pending_content_repaints.insert(view_id);
            } else if let Some(points) = explicit_sync.take() {
                self.finish_unused_explicit_sync(points);
            }
        } else {
            let window = self
                .space
                .elements()
                .find(|window| window.wl_surface().as_deref() == Some(&root))
                .cloned();
            if let Some(window) = &window {
                window.on_commit();
            }

            #[cfg(feature = "tty")]
            {
                content_changed = self.update_surface_content(&root);
                // Committed ext-background-effect blur region → ECS EffectStyle.
                content_changed |= self.sync_view_background_effect(&root);
                self.reconcile_surface_sync(surface, explicit_sync.take());
                self.reconcile_deferred_surface_sync(&root);
            }

            if let Some(window) = window
                && window.wl_surface().as_deref() == Some(&root)
                && let Some(toplevel) = window.toplevel().cloned()
            {
                let constraints = with_states(toplevel.wl_surface(), |states| {
                    let mut cached = states.cached_state.get::<SurfaceCachedState>();
                    let current = cached.current();
                    xdg_size_constraints(current.min_size, current.max_size)
                });
                let constraints_changed =
                    self.update_toplevel_constraints(toplevel.wl_surface(), constraints);
                let needs_initial_configure = !toplevel.is_initial_configure_sent();
                if constraints_changed || needs_initial_configure {
                    #[cfg(feature = "tty")]
                    {
                        reflowed = self.reflow_default_workspace();
                    }
                    #[cfg(not(feature = "tty"))]
                    {
                        self.reflow_default_workspace();
                    }
                }
                if needs_initial_configure {
                    if !toplevel.is_initial_configure_sent() {
                        toplevel.send_configure();
                    }
                    // `focus_mapped_window` selects ECS and XDG activation
                    // immediately, but waits for this mandatory initial
                    // configure before emitting `wl_keyboard.enter`.
                    #[cfg(feature = "tty")]
                    self.restore_keyboard_focus();
                }
            }
        }

        #[cfg(feature = "tty")]
        let deferred_repaint = !is_sync_subsurface(surface)
            && self
                .view_for_surface(&root)
                .is_some_and(|view_id| self.pending_content_repaints.remove(&view_id));
        #[cfg(feature = "tty")]
        if content_changed || deferred_repaint || fifo_activated {
            let view = self.view_for_surface(&root);
            self.push_surface_committed(root.id().protocol_id(), view);
            if !reflowed {
                self.request_redraw_workspace();
            }
        }

        if let Some(PopupKind::Xdg(popup)) = self.popups.find_popup(surface)
            && !popup.is_initial_configure_sent()
            && let Err(error) = popup.send_configure()
        {
            warn!(%error, "failed to send initial popup configure");
        }
        self.popups.cleanup();
        #[cfg(feature = "tty")]
        self.flush_client_releases();
        #[cfg(not(feature = "tty"))]
        let _ = fifo_activated;
    }

    fn destroyed(&mut self, surface: &WlSurface) {
        let released = self.protocol_globals.remove_surface(surface);
        let notify_destroyed_client = !released.is_empty();
        self.release_surface_barriers(released);
        if notify_destroyed_client && let Some(client) = surface.client() {
            let display = self.display_handle.clone();
            self.client_compositor_state(&client)
                .blocker_cleared(self, &display);
        }
        destroy_surface_state(surface);
        #[cfg(feature = "tty")]
        {
            self.discard_deferred_surface_sync(surface);
            #[cfg(feature = "xwayland")]
            let x11_popup_owner = self.x11_popup_surface_destroyed(surface);
            if self.view_for_surface(surface).is_some() {
                self.unregister_toplevel(surface);
            } else if let Some(root) = {
                #[cfg(feature = "xwayland")]
                {
                    x11_popup_owner.or_else(|| self.owning_view_root(surface))
                }
                #[cfg(not(feature = "xwayland"))]
                {
                    self.owning_view_root(surface)
                }
            } {
                if let Some(window) = self
                    .space
                    .elements()
                    .find(|window| window.wl_surface().as_deref() == Some(&root))
                {
                    window.on_commit();
                }
                let changed = self.update_surface_content(&root);
                self.reconcile_deferred_surface_sync(&root);
                let deferred_repaint = self
                    .view_for_surface(&root)
                    .is_some_and(|view_id| self.pending_content_repaints.remove(&view_id));
                if changed || deferred_repaint {
                    self.request_redraw_workspace();
                }
            }
            self.popups.cleanup();
            self.flush_client_releases();
        }
        #[cfg(not(feature = "tty"))]
        {
            #[cfg(feature = "xwayland")]
            let _ = self.x11_popup_surface_destroyed(surface);
            self.unregister_toplevel(surface);
        }
    }
}

fn surface_root(surface: &WlSurface) -> WlSurface {
    let mut root = surface.clone();
    while let Some(parent) = get_parent(&root) {
        root = parent;
    }
    root
}

impl BufferHandler for RuntimeState {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {
        #[cfg(feature = "tty")]
        self.buffer_destroyed(&_buffer.id());
    }
}

impl XdgShellHandler for RuntimeState {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let _ = self.register_toplevel(surface);
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        let popup = PopupKind::Xdg(surface);
        self.unconstrain_popup(&popup);
        if let Err(error) = self.popups.track_popup(popup) {
            warn!(%error, "failed to track xdg popup");
        }
    }

    fn popup_destroyed(&mut self, _surface: PopupSurface) {
        self.popups.cleanup();
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        surface.with_pending_state(|state| {
            state.geometry = positioner.get_geometry();
            state.positioner = positioner;
        });
        self.unconstrain_popup(&PopupKind::Xdg(surface.clone()));
        surface.send_repositioned(token);
        if surface.is_initial_configure_sent()
            && let Err(error) = surface.send_configure()
        {
            warn!(%error, "failed to configure repositioned popup");
        }
    }

    fn grab(&mut self, surface: PopupSurface, seat: wl_seat::WlSeat, serial: Serial) {
        let Some(seat) = Seat::from_resource(&seat) else {
            return;
        };
        let popup = PopupKind::Xdg(surface);
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
        let Ok(mut grab) = self.popups.grab_popup(root.into(), popup, &seat, serial) else {
            return;
        };

        if let Some(keyboard) = seat.get_keyboard() {
            if keyboard.is_grabbed()
                && !(keyboard.has_grab(serial)
                    || keyboard.has_grab(grab.previous_serial().unwrap_or(serial)))
            {
                grab.ungrab(self);
                return;
            }
            keyboard.set_focus(self, grab.current_grab(), serial);
            keyboard.set_grab(self, PopupKeyboardGrab::new(&grab), serial);
        }
        if let Some(pointer) = seat.get_pointer() {
            if pointer.is_grabbed()
                && !(pointer.has_grab(serial)
                    || pointer.has_grab(grab.previous_serial().unwrap_or_else(|| grab.serial())))
            {
                grab.ungrab(self);
                return;
            }
            pointer.set_grab(self, PopupPointerGrab::new(&grab), serial, Focus::Keep);
        }
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        self.protocol_globals
            .xdg_toplevel_destroyed(surface.xdg_toplevel());
        self.unregister_toplevel(surface.wl_surface());
    }

    fn title_changed(&mut self, surface: ToplevelSurface) {
        self.refresh_foreign_toplevel_metadata(&surface);
    }

    fn app_id_changed(&mut self, surface: ToplevelSurface) {
        self.refresh_foreign_toplevel_metadata(&surface);
    }
}

impl SeatHandler for RuntimeState {
    type KeyboardFocus = KeyboardFocusTarget;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seat_state
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&KeyboardFocusTarget>) {
        let client = focused
            .and_then(WaylandFocus::wl_surface)
            .and_then(|surface| self.display_handle.get_client(surface.id()).ok());
        set_primary_focus(&self.display_handle, seat, client.clone());
        set_data_device_focus(&self.display_handle, seat, client);
    }

    fn cursor_image(&mut self, _seat: &Seat<Self>, image: CursorImageStatus) {
        #[cfg(feature = "tty")]
        if self.cursor.set_image(match image {
            CursorImageStatus::Hidden => CursorImage::Hidden,
            CursorImageStatus::Named(icon) => CursorImage::Named(icon),
            CursorImageStatus::Surface(surface) => CursorImage::Surface(surface),
        }) {
            if let Some(pointer) = self.seat.get_pointer() {
                self.request_redraw_at(pointer.current_location());
            } else {
                self.request_redraw_workspace();
            }
        }
        #[cfg(not(feature = "tty"))]
        let _ = image;
    }
}

impl SelectionHandler for RuntimeState {
    type SelectionUserData = ();
}

impl PrimarySelectionHandler for RuntimeState {
    fn primary_selection_state(&mut self) -> &mut PrimarySelectionState {
        self.protocol_globals.primary_selection()
    }
}

impl DataDeviceHandler for RuntimeState {
    fn data_device_state(&mut self) -> &mut DataDeviceState {
        &mut self.data_device_state
    }
}

/// Tokens older than this are rejected (matches Niri's activation window).
const XDG_ACTIVATION_TOKEN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

impl smithay::wayland::xdg_activation::XdgActivationHandler for RuntimeState {
    fn activation_state(&mut self) -> &mut smithay::wayland::xdg_activation::XdgActivationState {
        self.protocol_globals.activation()
    }

    fn request_activation(
        &mut self,
        token: smithay::wayland::xdg_activation::XdgActivationToken,
        token_data: smithay::wayland::xdg_activation::XdgActivationTokenData,
        surface: WlSurface,
    ) {
        if token_data.timestamp.elapsed() >= XDG_ACTIVATION_TOKEN_TIMEOUT {
            self.protocol_globals.activation().remove_token(&token);
            return;
        }
        // Accept activation requests that still have a mapped view. Unmapped
        // surfaces that carry a fresh compositor-issued spawn token are also
        // accepted once they map and call activate with the same token.
        let window = self
            .view_for_surface(&surface)
            .is_some()
            .then(|| {
                self.space
                    .elements()
                    .find(|window| window.wl_surface().as_deref() == Some(&surface))
                    .cloned()
            })
            .flatten();
        if let Some(window) = window {
            #[cfg(feature = "tty")]
            let _ = self.focus_mapped_window(window, smithay::utils::SERIAL_COUNTER.next_serial());
            #[cfg(not(feature = "tty"))]
            let _ = window;
        }
        self.protocol_globals.activation().remove_token(&token);
        #[cfg(feature = "tty")]
        self.request_redraw_workspace();
    }
}

impl RuntimeState {
    /// Mint an external xdg-activation token for compositor-owned launches.
    pub(crate) fn issue_spawn_activation_token(&mut self) -> String {
        self.protocol_globals
            .activation()
            .retain_tokens(|_, data| data.timestamp.elapsed() < XDG_ACTIVATION_TOKEN_TIMEOUT);
        let (token, _) = self
            .protocol_globals
            .activation()
            .create_external_token(None);
        token.as_str().to_owned()
    }
}

impl smithay::input::tablet::TabletSeatHandler for RuntimeState {
    type ToolFocus = WlSurface;
}

impl smithay::wayland::shell::wlr_layer::WlrLayerShellHandler for RuntimeState {
    fn shell_state(&mut self) -> &mut smithay::wayland::shell::wlr_layer::WlrLayerShellState {
        self.protocol_globals.layer_shell()
    }

    fn new_layer_surface(
        &mut self,
        surface: smithay::wayland::shell::wlr_layer::LayerSurface,
        output: Option<wayland_server::protocol::wl_output::WlOutput>,
        _layer: smithay::wayland::shell::wlr_layer::Layer,
        namespace: String,
    ) {
        let output = output
            .as_ref()
            .and_then(|resource| {
                self.space
                    .outputs()
                    .find(|candidate| candidate.owns(resource))
                    .cloned()
            })
            .or_else(|| self.space.outputs().next().cloned());
        let Some(output) = output else {
            warn!(%namespace, "layer surface created without a mapped output");
            surface.send_close();
            return;
        };
        self.map_layer_surface(&output, surface, namespace);
        #[cfg(feature = "tty")]
        self.request_redraw_all();
    }

    fn layer_destroyed(&mut self, surface: smithay::wayland::shell::wlr_layer::LayerSurface) {
        #[cfg(feature = "tty")]
        self.forget_layer_surface(surface.wl_surface());
        self.unmap_layer_surface(&surface);
        #[cfg(feature = "tty")]
        {
            let _ = self.reflow_default_workspace_layout();
            self.request_redraw_all();
        }
    }

    fn new_popup(
        &mut self,
        _parent: smithay::wayland::shell::wlr_layer::LayerSurface,
        popup: PopupSurface,
    ) {
        let kind = PopupKind::Xdg(popup);
        self.unconstrain_popup(&kind);
        if let Err(error) = self.popups.track_popup(kind) {
            warn!(%error, "failed to track layer-shell xdg popup");
        }
    }
}

impl DndGrabHandler for RuntimeState {}
impl WaylandDndGrabHandler for RuntimeState {}
smithay::delegate_dispatch2!(RuntimeState);
