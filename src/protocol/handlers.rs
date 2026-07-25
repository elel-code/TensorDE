#[cfg(feature = "tty")]
use std::cell::RefCell;
#[cfg(feature = "xwayland")]
mod xwayland;
#[cfg(feature = "tty")]
use smithay::wayland::drm_syncobj::{DrmSyncobjCachedState, DrmSyncobjHandler, DrmSyncobjState};
#[cfg(feature = "tty")]
use smithay::reexports::wayland_protocols::wp::linux_drm_syncobj::v1::server::wp_linux_drm_syncobj_surface_v1::{
    self, WpLinuxDrmSyncobjSurfaceV1,
};
#[cfg(feature = "tty")]
use smithay::{
    backend::allocator::{Buffer, dmabuf::Dmabuf},
    wayland::compositor::{BufferAssignment, SurfaceAttributes},
    wayland::dmabuf::{
        DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier, get_dmabuf,
    },
};
use smithay::{
    backend::renderer::utils::on_commit_buffer_handler,
    desktop::{
        PopupKeyboardGrab, PopupKind, PopupPointerGrab, PopupUngrabStrategy,
        find_popup_root_surface,
    },
    input::{
        Seat, SeatHandler, SeatState,
        dnd::DndGrabHandler,
        pointer::{CursorImageStatus, Focus},
    },
    reexports::wayland_server::{
        Client, Resource,
        protocol::{wl_buffer, wl_seat, wl_surface::WlSurface},
    },
    utils::Serial,
    wayland::{
        buffer::BufferHandler,
        compositor::{
            CompositorClientState, CompositorHandler, CompositorState, get_parent,
            is_sync_subsurface, with_states,
        },
        fractional_scale::FractionalScaleHandler,
        output::OutputHandler,
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
            XdgShellState, decoration::XdgDecorationHandler,
        },
        shm::{ShmHandler, ShmState},
    },
};
use tracing::warn;

#[cfg(feature = "xwayland")]
use smithay::xwayland::XWaylandClientData;

#[cfg(feature = "tty")]
use super::state::ExplicitSyncPoints;
use super::{
    focus::KeyboardFocusTarget,
    state::{RuntimeState, WaylandClientState, xdg_size_constraints},
};

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
                on_commit_buffer_handler::<Self>(surface);
                let root = self
                    .owning_view_root(surface)
                    .unwrap_or_else(|| surface_root(surface));
                self.discard_deferred_surface_sync(surface);
                self.unregister_toplevel(&root);
                self.flush_client_releases();
                return;
            }
        };
        on_commit_buffer_handler::<Self>(surface);
        self.popups.commit(surface);

        #[cfg(feature = "tty")]
        if self.handle_layer_shell_commit(surface) {
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
        if (content_changed || deferred_repaint) && !reflowed {
            self.request_redraw_workspace();
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
    }

    fn destroyed(&mut self, surface: &WlSurface) {
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

impl ShmHandler for RuntimeState {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
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
        if let Err(error) = self.popups.track_popup(PopupKind::Xdg(surface)) {
            warn!(%error, "failed to track xdg popup");
        }
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
        if self.view_for_surface(&root).is_none() {
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
                grab.ungrab(PopupUngrabStrategy::All);
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
                grab.ungrab(PopupUngrabStrategy::All);
                return;
            }
            pointer.set_grab(self, PopupPointerGrab::new(&grab), serial, Focus::Keep);
        }
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        self.unregister_toplevel(surface.wl_surface());
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
        if self.cursor.set_image(image) {
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

impl FractionalScaleHandler for RuntimeState {
    fn new_fractional_scale(&mut self, surface: WlSurface) {
        self.update_surface_scale(&surface);
    }
}

impl XdgDecorationHandler for RuntimeState {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        set_client_side_decoration(&toplevel);
    }

    fn request_mode(
        &mut self,
        toplevel: ToplevelSurface,
        _mode: smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode,
    ) {
        set_client_side_decoration(&toplevel);
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        set_client_side_decoration(&toplevel);
    }
}

fn set_client_side_decoration(toplevel: &ToplevelSurface) {
    use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;

    toplevel.with_pending_state(|state| {
        state.decoration_mode = Some(Mode::ClientSide);
    });
    if toplevel.is_initial_configure_sent() {
        toplevel.send_pending_configure();
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
            let _ = self.focus_mapped_window(window, smithay::utils::SERIAL_COUNTER.next_serial());
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

// wp_cursor_shape_v1 can bind tablet tools; Smithay requires the handler even
// when Tensor does not yet advertise a tablet seat global.
impl smithay::wayland::tablet_manager::TabletSeatHandler for RuntimeState {}

impl smithay::wayland::idle_notify::IdleNotifierHandler for RuntimeState {
    fn idle_notifier_state(
        &mut self,
    ) -> &mut smithay::wayland::idle_notify::IdleNotifierState<Self> {
        self.protocol_globals.idle_notifier()
    }
}

impl smithay::wayland::shell::wlr_layer::WlrLayerShellHandler for RuntimeState {
    fn shell_state(&mut self) -> &mut smithay::wayland::shell::wlr_layer::WlrLayerShellState {
        self.protocol_globals.layer_shell()
    }

    fn new_layer_surface(
        &mut self,
        surface: smithay::wayland::shell::wlr_layer::LayerSurface,
        output: Option<smithay::reexports::wayland_server::protocol::wl_output::WlOutput>,
        _layer: smithay::wayland::shell::wlr_layer::Layer,
        namespace: String,
    ) {
        use smithay::desktop::{LayerSurface as DesktopLayerSurface, layer_map_for_output};

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
        {
            let mut map = layer_map_for_output(&output);
            if let Err(error) = map.map_layer(&DesktopLayerSurface::new(surface, namespace)) {
                warn!(%error, "failed to map layer surface");
            } else {
                map.arrange();
            }
        }
        #[cfg(feature = "tty")]
        self.request_redraw_all();
    }

    fn layer_destroyed(&mut self, surface: smithay::wayland::shell::wlr_layer::LayerSurface) {
        use smithay::desktop::layer_map_for_output;

        #[cfg(feature = "tty")]
        self.forget_layer_surface(surface.wl_surface());
        for output in self.space.outputs().cloned().collect::<Vec<_>>() {
            let mut map = layer_map_for_output(&output);
            let layer = map
                .layers()
                .find(|layer| layer.layer_surface() == &surface)
                .cloned();
            if let Some(layer) = layer {
                map.unmap_layer(&layer);
                map.arrange();
            }
        }
        #[cfg(feature = "tty")]
        {
            let _ = self.reflow_default_workspace_layout();
            self.request_redraw_all();
        }
    }
}

#[cfg(feature = "tty")]
impl DrmSyncobjHandler for RuntimeState {
    fn drm_syncobj_state(&mut self) -> Option<&mut DrmSyncobjState> {
        self.protocol_globals.drm_syncobj_state()
    }
}

#[cfg(feature = "tty")]
enum ExplicitSyncCommit {
    None,
    Points(ExplicitSyncPoints),
    Rejected,
}

#[cfg(feature = "tty")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExplicitSyncShape {
    None,
    Points,
    MissingPoints,
    Rejected,
}

#[cfg(feature = "tty")]
fn explicit_sync_shape(
    has_surface: bool,
    has_buffer: bool,
    has_acquire: bool,
    has_release: bool,
) -> ExplicitSyncShape {
    match (has_surface, has_buffer, has_acquire, has_release) {
        (false, _, false, false) | (true, false, false, false) => ExplicitSyncShape::None,
        (true, true, true, true) => ExplicitSyncShape::Points,
        (true, true, false, false) => ExplicitSyncShape::MissingPoints,
        _ => ExplicitSyncShape::Rejected,
    }
}

#[cfg(feature = "tty")]
fn take_explicit_sync_points(surface: &WlSurface) -> ExplicitSyncCommit {
    with_states(surface, |states| {
        let syncobj_surface = states
            .data_map
            .get::<RefCell<Option<WpLinuxDrmSyncobjSurfaceV1>>>()
            .and_then(|surface| surface.borrow().clone());
        let new_buffer = {
            let mut cached = states.cached_state.get::<SurfaceAttributes>();
            cached
                .current()
                .buffer
                .as_ref()
                .and_then(|assignment| match assignment {
                    BufferAssignment::NewBuffer(buffer) => Some(buffer.clone()),
                    _ => None,
                })
        };
        let mut cached = states.cached_state.get::<DrmSyncobjCachedState>();
        let current = cached.current();
        let acquire = current.acquire_point.take();
        let release = current.release_point.take();

        match explicit_sync_shape(
            syncobj_surface.is_some(),
            new_buffer.is_some(),
            acquire.is_some(),
            release.is_some(),
        ) {
            ExplicitSyncShape::None => ExplicitSyncCommit::None,
            ExplicitSyncShape::Points => {
                let buffer = new_buffer.expect("shape checked a new buffer");
                let acquire = acquire.expect("shape checked an acquire point");
                let release = release.expect("shape checked a release point");
                let conflicting =
                    acquire.timeline() == release.timeline() && release.point() <= acquire.point();
                if conflicting || get_dmabuf(&buffer).is_err() {
                    ExplicitSyncCommit::Rejected
                } else {
                    ExplicitSyncCommit::Points(ExplicitSyncPoints { acquire, release })
                }
            }
            ExplicitSyncShape::MissingPoints => {
                syncobj_surface
                    .expect("shape checked a syncobj surface")
                    .post_error(
                        wp_linux_drm_syncobj_surface_v1::Error::NoAcquirePoint,
                        "buffer commit did not provide explicit acquire/release points".to_owned(),
                    );
                ExplicitSyncCommit::Rejected
            }
            ExplicitSyncShape::Rejected => ExplicitSyncCommit::Rejected,
        }
    })
}

#[cfg(all(test, feature = "tty"))]
mod explicit_sync_tests {
    use super::*;

    #[test]
    fn syncobj_surface_requires_both_points_for_every_buffer_attach() {
        assert_eq!(
            explicit_sync_shape(true, true, false, false),
            ExplicitSyncShape::MissingPoints
        );
        assert_eq!(
            explicit_sync_shape(true, true, true, false),
            ExplicitSyncShape::Rejected
        );
        assert_eq!(
            explicit_sync_shape(true, true, false, true),
            ExplicitSyncShape::Rejected
        );
        assert_eq!(
            explicit_sync_shape(true, true, true, true),
            ExplicitSyncShape::Points
        );
    }

    #[test]
    fn damage_only_commit_does_not_require_new_points() {
        assert_eq!(
            explicit_sync_shape(true, false, false, false),
            ExplicitSyncShape::None
        );
        assert_eq!(
            explicit_sync_shape(true, false, true, true),
            ExplicitSyncShape::Rejected
        );
    }
}

#[cfg(feature = "tty")]
impl DmabufHandler for RuntimeState {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        self.protocol_globals.dmabuf_state()
    }

    fn dmabuf_imported(
        &mut self,
        _global: &DmabufGlobal,
        dmabuf: Dmabuf,
        notifier: ImportNotifier,
    ) {
        let Some(size) = dmabuf_size(&dmabuf) else {
            notifier.failed();
            return;
        };
        let Some(_) = self.renderer.as_ref() else {
            notifier.failed();
            return;
        };
        let Some(buffer_id) = self.allocate_client_buffer_id() else {
            warn!("client buffer identity space is exhausted; rejecting linux-dmabuf import");
            notifier.failed();
            return;
        };
        let import_result = self
            .renderer
            .as_mut()
            .expect("renderer existence was checked above")
            .import_client_dmabuf(buffer_id, &dmabuf);
        match import_result {
            Ok(()) => match notifier.successful::<RuntimeState>() {
                Ok(buffer) => {
                    if !self.register_imported_client_buffer(buffer.id(), buffer_id, size) {
                        self.release_client_buffers([buffer_id]);
                        warn!("linux-dmabuf buffer identity was already occupied; released import");
                    }
                }
                Err(error) => {
                    self.release_client_buffers([buffer_id]);
                    warn!(%error, "client disappeared while completing linux-dmabuf import");
                }
            },
            Err(error) => {
                warn!(%error, "client linux-dmabuf import failed");
                notifier.failed();
            }
        }
    }
}

#[cfg(feature = "tty")]
fn dmabuf_size(dmabuf: &Dmabuf) -> Option<tensor_util::Size> {
    let size = dmabuf.size();
    Some(tensor_util::Size::new(
        u32::try_from(size.w).ok()?,
        u32::try_from(size.h).ok()?,
    ))
    .filter(|size| size.width > 0 && size.height > 0)
}

impl DndGrabHandler for RuntimeState {}
impl WaylandDndGrabHandler for RuntimeState {}
impl OutputHandler for RuntimeState {}

smithay::delegate_dispatch2!(RuntimeState);
