#[cfg(feature = "tty")]
mod dmabuf;
mod protocols;
#[cfg(feature = "xwayland")]
mod xwayland;
#[cfg(feature = "tty")]
use dmabuf::{ExplicitSyncCommit, take_explicit_sync_points};
use wayland_server::{Resource, protocol::wl_surface::WlSurface};

#[cfg(feature = "tty")]
use super::state::take_dnd_icon_surface_delta;
use super::{
    globals::compositor::{get_parent, is_sync_subsurface},
    state::{
        PopupKind, RuntimeState, apply_cursor_surface_delta, destroy_surface_state,
        on_commit_surface_handler, xdg_size_constraints,
    },
};

impl RuntimeState {
    pub(in crate::protocol) fn surface_commit_applied(&mut self, surface: &WlSurface) {
        #[cfg(feature = "tty")]
        let mut explicit_sync = match take_explicit_sync_points(surface) {
            ExplicitSyncCommit::None => None,
            ExplicitSyncCommit::Points(points) => Some(points),
            ExplicitSyncCommit::Rejected => {
                let shm_uploads = on_commit_surface_handler(surface);
                self.upload_shm_buffers(shm_uploads);
                let root = self
                    .owning_view_root(surface)
                    .unwrap_or_else(|| surface_root(surface));
                self.discard_deferred_surface_sync(surface);
                self.unregister_toplevel(&root);
                self.flush_client_releases();
                return;
            }
        };
        #[cfg(feature = "tty")]
        let shm_uploads = on_commit_surface_handler(surface);
        #[cfg(not(feature = "tty"))]
        let _ = on_commit_surface_handler(surface);
        #[cfg(feature = "tty")]
        self.upload_shm_buffers(shm_uploads);
        apply_cursor_surface_delta(surface);
        #[cfg(feature = "tty")]
        let dnd_icon_commit = self.dnd_icon.uses_surface(surface);
        #[cfg(feature = "tty")]
        if dnd_icon_commit && let Some(delta) = take_dnd_icon_surface_delta(surface) {
            self.dnd_icon.apply_delta(delta);
        }
        #[cfg(feature = "tty")]
        let cursor_commit = self.cursor.uses_surface(surface);
        let fifo_activated = self
            .protocol_globals
            .surface_timing
            .take_fifo_activation(surface);
        if let Some(popup) = self.protocol_globals.xdg_shell.popup_for_surface(surface) {
            self.popups.commit(&PopupKind::from(popup));
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
        let input_popup_commit = self
            .protocol_globals
            .input_method
            .popup_parent(&root)
            .is_some();
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
                if let Some(toplevel) = window.toplevel() {
                    self.sync_xdg_dialog_placement(toplevel);
                }
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
        #[cfg(feature = "tty")]
        if cursor_commit {
            self.queue_cursor_surface_memberships(surface);
            self.refresh_cursor_surface_outputs();
            self.queue_cursor_surface_memberships(surface);
            self.flush_queued_redraws();
        }
        #[cfg(feature = "tty")]
        if dnd_icon_commit {
            self.refresh_dnd_icon_outputs();
            self.flush_queued_redraws();
        }
        #[cfg(feature = "tty")]
        if input_popup_commit {
            self.refresh_input_method_popup_outputs();
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

    pub(in crate::protocol) fn surface_destroyed_applied(&mut self, surface: &WlSurface) {
        #[cfg(feature = "tty")]
        let previous_input_popup_root = self.input_method_popup_root();
        #[cfg(feature = "tty")]
        let cursor_source = self
            .cursor
            .source_position_for_surface(surface, self.input_seat.pointer_location());
        #[cfg(feature = "tty")]
        if cursor_source.is_some() {
            self.queue_cursor_surface_memberships(surface);
        }
        #[cfg(feature = "tty")]
        if self.cursor.surface_destroyed(surface) {
            for output in self.space.outputs() {
                output.forget_surface(surface);
            }
            if let Some((source, location)) = cursor_source {
                self.queue_cursor_redraw_between(source, location, location);
            }
            self.flush_queued_redraws();
        }
        self.input_seat.surface_destroyed(surface);
        #[cfg(feature = "xwayland")]
        if self
            .protocol_globals
            .xwayland_keyboard_grab
            .surface_destroyed(surface)
        {
            self.sync_keyboard_wire_focus(crate::protocol::serial::next_serial());
        }
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
        #[cfg(feature = "tty")]
        self.refresh_input_method_popups(previous_input_popup_root);
        let notify_destroyed_client = !released.is_empty();
        self.release_surface_barriers(released);
        if notify_destroyed_client && let Some(client) = surface.client() {
            self.compositor_blocker_cleared(&client);
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
