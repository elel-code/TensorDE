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
    input::{Seat, SeatHandler, SeatState, dnd::DndGrabHandler, pointer::CursorImageStatus},
    wayland::{
        buffer::BufferHandler,
        compositor::{
            CompositorClientState, CompositorHandler, CompositorState, get_parent,
            is_sync_subsurface,
        },
        seat::WaylandFocus,
    },
};
use wayland_server::{
    Client, Resource,
    protocol::{wl_buffer, wl_surface::WlSurface},
};

#[cfg(feature = "xwayland")]
use smithay::xwayland::XWaylandClientData;

use super::{
    focus::{KeyboardFocusTarget, SurfaceFocusTarget},
    state::{
        PopupKind, RuntimeState, WaylandClientState, destroy_surface_state,
        on_commit_surface_handler, popup::PopupGrabHandler, xdg_size_constraints,
    },
};

impl PopupGrabHandler for RuntimeState {
    fn dismiss_grabbed_popup(
        &mut self,
        root: &WlSurface,
        popup: &WlSurface,
    ) -> Result<(), smithay::utils::DeadResource> {
        let Some(popup) = self.popups.find_popup(popup) else {
            return Ok(());
        };
        self.popups.dismiss_popup(root, &popup)
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
        if let Some(popup) = self.protocol_globals.xdg_shell.popup_for_surface(surface) {
            self.popups.commit(&PopupKind::Xdg(popup));
        }

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
                let (min_size, max_size) = toplevel.constraints();
                let constraints = xdg_size_constraints(min_size, max_size);
                let constraints_changed =
                    self.update_toplevel_constraints(toplevel.wl_surface(), constraints);
                let needs_initial_configure = !toplevel.initial_configure_sent();
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
                    if !toplevel.initial_configure_sent() {
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

        if let Some(popup) = self.protocol_globals.xdg_shell.popup_for_surface(surface)
            && !popup.initial_configure_sent()
        {
            popup.send_configure();
        }
        self.popups.cleanup();
        #[cfg(feature = "tty")]
        self.flush_client_releases();
        #[cfg(not(feature = "tty"))]
        let _ = fifo_activated;
    }

    fn destroyed(&mut self, surface: &WlSurface) {
        self.selection_surface_destroyed(surface);
        self.layer_surface_wl_destroyed(surface);
        self.remove_session_lock_surface(surface);
        self.remove_xdg_foreign_surface(surface);
        if let Some(toplevel) = self
            .protocol_globals
            .xdg_shell
            .toplevel_for_surface(surface)
        {
            self.protocol_globals
                .xdg_toplevel_destroyed(toplevel.xdg_toplevel());
        }
        self.protocol_globals.xdg_shell.remove_wl_surface(surface);
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

impl SeatHandler for RuntimeState {
    type KeyboardFocus = KeyboardFocusTarget;
    type PointerFocus = SurfaceFocusTarget;
    type TouchFocus = SurfaceFocusTarget;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seat_state
    }

    fn focus_changed(&mut self, _seat: &Seat<Self>, focused: Option<&KeyboardFocusTarget>) {
        let focused = focused.and_then(WaylandFocus::wl_surface);
        self.protocol_globals
            .activation
            .sync_keyboard_focus(focused.as_deref());
        let client = focused.and_then(|surface| surface.client().map(|client| client.id()));
        self.protocol_globals.selection.set_focus(client);
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

impl smithay::input::tablet::TabletSeatHandler for RuntimeState {
    type ToolFocus = WlSurface;
}

impl DndGrabHandler for RuntimeState {
    fn dropped(
        &mut self,
        _target: Option<smithay::input::dnd::DndTarget<'_, Self>>,
        _validated: bool,
        _seat: Seat<Self>,
        _location: smithay::utils::Point<f64, smithay::utils::Logical>,
    ) {
        self.finish_selection_dnd();
    }

    fn cancelled(
        &mut self,
        _seat: Seat<Self>,
        _location: smithay::utils::Point<f64, smithay::utils::Logical>,
    ) {
        self.finish_selection_dnd();
    }
}
smithay::delegate_dispatch2!(RuntimeState);
