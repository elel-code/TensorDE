#[cfg(feature = "tty")]
use smithay::{
    backend::allocator::{Buffer, dmabuf::Dmabuf},
    wayland::dmabuf::{DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier},
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

use super::state::{RuntimeState, WaylandClientState, xdg_size_constraints};

impl CompositorHandler for RuntimeState {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client
            .get_data::<WaylandClientState>()
            .expect("all Tensor Wayland clients carry compositor state")
            .compositor_state
    }

    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<Self>(surface);

        #[cfg(feature = "tty")]
        let mut content_changed = false;
        #[cfg(feature = "tty")]
        let mut reflowed = false;
        if !is_sync_subsurface(surface) {
            let mut root = surface.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }
            #[cfg(feature = "tty")]
            {
                content_changed = self.update_surface_content(&root);
            }
            let toplevel = self
                .space
                .elements()
                .find(|window| window.wl_surface().as_deref() == Some(&root))
                .and_then(|window| {
                    window.on_commit();
                    window.toplevel().cloned()
                });
            if let Some(toplevel) = toplevel {
                let constraints = with_states(toplevel.wl_surface(), |states| {
                    let mut cached = states.cached_state.get::<SurfaceCachedState>();
                    let current = cached.current();
                    xdg_size_constraints(current.min_size, current.max_size)
                });
                let constraints_changed =
                    self.update_toplevel_constraints(toplevel.wl_surface(), constraints);
                if constraints_changed || !toplevel.is_initial_configure_sent() {
                    #[cfg(feature = "tty")]
                    {
                        reflowed = self.reflow_default_workspace();
                    }
                    #[cfg(not(feature = "tty"))]
                    {
                        self.reflow_default_workspace();
                    }
                }
                if !toplevel.is_initial_configure_sent() {
                    toplevel.send_configure();
                }
            }
        }

        #[cfg(feature = "tty")]
        if content_changed && !reflowed {
            self.submit_default_workspace_frame();
        }

        self.popups.commit(surface);
        if let Some(PopupKind::Xdg(popup)) = self.popups.find_popup(surface)
            && !popup.is_initial_configure_sent()
            && let Err(error) = popup.send_configure()
        {
            warn!(%error, "failed to send initial popup configure");
        }
        self.popups.cleanup();
    }

    fn destroyed(&mut self, surface: &WlSurface) {
        self.unregister_toplevel(surface);
    }
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
        let Ok(mut grab) = self.popups.grab_popup(root, popup, &seat, serial) else {
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
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seat_state
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
        let client = focused.and_then(|surface| self.display_handle.get_client(surface.id()).ok());
        set_primary_focus(&self.display_handle, seat, client.clone());
        set_data_device_focus(&self.display_handle, seat, client);
    }

    fn cursor_image(&mut self, _seat: &Seat<Self>, _image: CursorImageStatus) {}
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
