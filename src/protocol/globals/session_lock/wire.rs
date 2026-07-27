use smithay::wayland::compositor::{self, BufferAssignment, SurfaceAttributes, with_states};
use tracing::info;
use wayland_protocols::ext::session_lock::v1::server::{
    ext_session_lock_manager_v1::{self, ExtSessionLockManagerV1},
    ext_session_lock_surface_v1::{self, ExtSessionLockSurfaceV1},
    ext_session_lock_v1::{self, ExtSessionLockV1},
};
use wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource, Weak, backend::ClientId,
    protocol::wl_surface::WlSurface,
};

use super::*;
use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    globals::output::Output,
    state::{RuntimeState, pending_buffer_logical_size, surface_has_buffer},
};

const LOCK_SURFACE_ROLE: &str = "ext_session_lock_surface_v1";

#[derive(Debug)]
pub(in crate::protocol) struct SessionLockGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct SessionLockManagerData;

#[derive(Debug)]
pub(in crate::protocol) struct SessionLockData {
    token: LockToken,
}

#[derive(Debug)]
pub(in crate::protocol) struct SessionLockSurfaceData {
    token: LockSurfaceToken,
    surface: Weak<WlSurface>,
}

impl GlobalDispatchDelegate<ExtSessionLockManagerV1, RuntimeState> for SessionLockGlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<ExtSessionLockManagerV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, SessionLockManagerData);
    }

    fn can_view(&self, client: &Client) -> bool {
        client
            .get_data::<crate::protocol::state::WaylandClientState>()
            .is_none_or(|data| data.security_context.is_none())
    }
}

impl DispatchDelegate<ExtSessionLockManagerV1, RuntimeState> for SessionLockManagerData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        _manager: &ExtSessionLockManagerV1,
        request: ext_session_lock_manager_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            ext_session_lock_manager_v1::Request::Destroy => {}
            ext_session_lock_manager_v1::Request::Lock { id } => {
                let token = state.protocol_globals.session_lock.allocate_lock();
                let lock = data_init.init(id, SessionLockData { token });
                state.begin_session_lock(token, &lock);
            }
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<ExtSessionLockV1, RuntimeState> for SessionLockData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        lock: &ExtSessionLockV1,
        request: ext_session_lock_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            ext_session_lock_v1::Request::Destroy => {
                match state
                    .protocol_globals
                    .session_lock
                    .request_destroy(self.token)
                {
                    DestroyLock::Allowed => {}
                    DestroyLock::Cancelled => state.session_lock_cancelled(),
                    DestroyLock::Invalid => lock.post_error(
                        ext_session_lock_v1::Error::InvalidDestroy,
                        "cannot destroy a confirmed session lock",
                    ),
                }
            }
            ext_session_lock_v1::Request::UnlockAndDestroy => {
                if state
                    .protocol_globals
                    .session_lock
                    .request_unlock(self.token)
                {
                    state.session_lock_unlocked();
                } else {
                    lock.post_error(
                        ext_session_lock_v1::Error::InvalidUnlock,
                        "session lock was not confirmed",
                    );
                }
            }
            ext_session_lock_v1::Request::GetLockSurface {
                id,
                surface,
                output,
            } => {
                state.create_session_lock_surface(self.token, lock, id, surface, output, data_init)
            }
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, state: &mut RuntimeState, _client: ClientId, _resource: &ExtSessionLockV1) {
        match state
            .protocol_globals
            .session_lock
            .drop_lock_resource(self.token)
        {
            DropLock::None => {}
            DropLock::Cancelled => state.session_lock_cancelled(),
            DropLock::Orphaned => state.session_lock_orphaned(),
        }
    }
}

impl DispatchDelegate<ExtSessionLockSurfaceV1, RuntimeState> for SessionLockSurfaceData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        surface: &ExtSessionLockSurfaceV1,
        request: ext_session_lock_surface_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            ext_session_lock_surface_v1::Request::Destroy => {}
            ext_session_lock_surface_v1::Request::AckConfigure { serial } => {
                if self.surface.upgrade().is_err() {
                    return;
                }
                match state
                    .protocol_globals
                    .session_lock
                    .ack_configure(self.token, serial)
                {
                    Ok(Some((resource, configure))) => send_configure(&resource, configure),
                    Ok(None) => {}
                    Err(()) => surface.post_error(
                        ext_session_lock_surface_v1::Error::InvalidSerial,
                        format!("unknown or consumed configure serial {serial}"),
                    ),
                }
            }
            _ => unreachable!(),
        }
    }

    fn destroyed(
        &self,
        state: &mut RuntimeState,
        _client: ClientId,
        _resource: &ExtSessionLockSurfaceV1,
    ) {
        if let Some(removed) = state
            .protocol_globals
            .session_lock
            .remove_surface_resource(self.token)
        {
            state.session_lock_surface_removed(removed);
        }
    }
}

impl RuntimeState {
    pub(crate) fn session_is_locked(&self) -> bool {
        self.protocol_globals.session_lock.is_locked()
    }

    fn begin_session_lock(&mut self, token: LockToken, lock: &ExtSessionLockV1) {
        #[cfg(feature = "tty")]
        let result = self.protocol_globals.session_lock.register_lock(
            token,
            lock,
            self.space.outputs().map(|output| output.id()),
        );
        #[cfg(not(feature = "tty"))]
        let result =
            self.protocol_globals
                .session_lock
                .register_lock(token, lock, std::iter::empty());
        match result {
            BeginLock::Pending => {
                self.capture_session_lock_seat();
                info!("session lock pending protected-frame completions");
                #[cfg(feature = "tty")]
                self.request_redraw_all();
            }
            BeginLock::Locked => {
                self.capture_session_lock_seat();
                lock.locked();
                info!("session locked without active outputs");
            }
            BeginLock::Finished => lock.finished(),
        }
    }

    fn create_session_lock_surface(
        &mut self,
        lock_token: LockToken,
        lock: &ExtSessionLockV1,
        id: New<ExtSessionLockSurfaceV1>,
        surface: WlSurface,
        output_resource: wayland_server::protocol::wl_output::WlOutput,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        let Some(output) = Output::from_resource_including_inactive(&output_resource) else {
            return;
        };
        let output_id = output.id();
        let output_instance = output.instance_id();
        if !self
            .protocol_globals
            .session_lock
            .can_add_surface(lock_token, output_instance)
        {
            lock.post_error(
                ext_session_lock_v1::Error::DuplicateOutput,
                "this lock already has a surface for the output",
            );
            return;
        }
        if compositor::give_role(&surface, LOCK_SURFACE_ROLE).is_err() {
            lock.post_error(
                ext_session_lock_v1::Error::Role,
                "wl_surface already has a role",
            );
            return;
        }
        if surface_has_buffer_or_pending(&surface) {
            lock.post_error(
                ext_session_lock_v1::Error::AlreadyConstructed,
                "wl_surface already has a buffer attached or committed",
            );
            return;
        }
        let size = output.logical_size();
        let token = self.protocol_globals.session_lock.allocate_surface();
        let lock_surface = data_init.init(
            id,
            SessionLockSurfaceData {
                token,
                surface: surface.downgrade(),
            },
        );
        compositor::add_pre_commit_hook::<RuntimeState, _>(&surface, lock_surface_pre_commit);
        let active = self
            .protocol_globals
            .session_lock
            .insert_surface(LockSurfaceRegistration {
                token,
                lock: lock_token,
                output_instance,
                output: output_id,
                output_is_live: output.is_live(),
                resource: lock_surface.downgrade(),
                surface: surface.clone(),
            });
        if let Some((resource, configure)) = self
            .protocol_globals
            .session_lock
            .configure_surface(token, size)
        {
            send_configure(&resource, configure);
        }
        if active {
            self.focus_session_lock_surface(&surface);
            #[cfg(feature = "tty")]
            self.request_redraw_all();
        }
    }

    fn capture_session_lock_seat(&mut self) {
        #[cfg(feature = "tty")]
        {
            self.publish_window_activation(None);
            self.cursor
                .set_image(crate::protocol::cursor::CursorImage::default_named());
            let serial = smithay::utils::SERIAL_COUNTER.next_serial();
            if let Some(keyboard) = self.seat.get_keyboard() {
                keyboard.unset_grab(self);
                keyboard.set_focus(self, None, serial);
            }
            if let Some(pointer) = self.seat.get_pointer() {
                pointer.unset_grab(self, serial, 0);
                let location = pointer.current_location();
                pointer.motion(
                    self,
                    None,
                    &smithay::input::pointer::MotionEvent {
                        location,
                        serial,
                        time: 0,
                    },
                );
                pointer.frame(self);
                self.protocol_globals.activation.sync_pointer_focus(None);
                self.protocol_globals
                    .pointer_gestures
                    .focus_changed(None, serial, 0);
                let _ = self
                    .protocol_globals
                    .pointer_constraints
                    .focus_changed(None, location);
            }
        }
    }

    pub(crate) fn focus_session_lock_surface(&mut self, surface: &WlSurface) {
        #[cfg(not(feature = "tty"))]
        let _ = surface;
        #[cfg(feature = "tty")]
        {
            use crate::protocol::focus::KeyboardFocusTarget;

            let Some(keyboard) = self.seat.get_keyboard() else {
                return;
            };
            let has_lock_focus = keyboard.current_focus().is_some_and(|focus| {
                use smithay::wayland::seat::WaylandFocus;
                focus.wl_surface().is_some_and(|focused| {
                    self.protocol_globals
                        .session_lock
                        .contains_active_surface(focused.as_ref())
                })
            });
            if !has_lock_focus {
                let serial = smithay::utils::SERIAL_COUNTER.next_serial();
                keyboard.unset_grab(self);
                keyboard.set_focus(
                    self,
                    Some(KeyboardFocusTarget::from(surface.clone())),
                    serial,
                );
            }
        }
    }

    fn session_lock_cancelled(&mut self) {
        info!("pending session lock cancelled");
        #[cfg(feature = "tty")]
        {
            self.release_session_lock_seat();
            self.restore_keyboard_focus();
            self.request_redraw_all();
        }
    }

    fn session_lock_unlocked(&mut self) {
        info!("session unlocked");
        #[cfg(feature = "tty")]
        {
            self.release_session_lock_seat();
            self.restore_keyboard_focus();
            self.request_redraw_all();
        }
    }

    #[cfg(feature = "tty")]
    fn release_session_lock_seat(&mut self) {
        self.cursor
            .set_image(crate::protocol::cursor::CursorImage::default_named());
        let serial = smithay::utils::SERIAL_COUNTER.next_serial();
        if let Some(keyboard) = self.seat.get_keyboard() {
            keyboard.unset_grab(self);
            keyboard.set_focus(self, None, serial);
        }
        if let Some(pointer) = self.seat.get_pointer() {
            let location = pointer.current_location();
            pointer.unset_grab(self, serial, 0);
            pointer.motion(
                self,
                None,
                &smithay::input::pointer::MotionEvent {
                    location,
                    serial,
                    time: 0,
                },
            );
            pointer.frame(self);
            self.protocol_globals.activation.sync_pointer_focus(None);
            self.protocol_globals
                .pointer_gestures
                .focus_changed(None, serial, 0);
            let _ = self
                .protocol_globals
                .pointer_constraints
                .focus_changed(None, location);
        }
    }

    fn session_lock_orphaned(&mut self) {
        info!("session lock client disconnected; session remains locked");
        #[cfg(feature = "tty")]
        self.request_redraw_all();
    }

    pub(crate) fn session_lock_output_removed(&mut self, output: ConnectorId) {
        let removed = self.protocol_globals.session_lock.output_removed(output);
        if let Some(surface) = removed.surface {
            self.session_lock_surface_removed(RemovedSurface {
                surface,
                active: true,
            });
        }
        if let Some(lock) = removed.confirmed {
            lock.locked();
            info!("session locked after the remaining protected frames completed");
        }
    }

    fn session_lock_surface_removed(&mut self, removed: RemovedSurface) {
        if !removed.active {
            return;
        }
        #[cfg(feature = "tty")]
        {
            self.clear_keyboard_focus_for_surface(&removed.surface);
            self.clear_session_lock_pointer_focus_for_surface(&removed.surface);
            if let Some(replacement) = self.protocol_globals.session_lock.first_active_surface() {
                self.focus_session_lock_surface(&replacement);
            }
            self.request_redraw_all();
        }
    }

    #[cfg(feature = "tty")]
    fn clear_session_lock_pointer_focus_for_surface(&mut self, surface: &WlSurface) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let Some(mut focused) = pointer.current_focus() else {
            return;
        };
        while let Some(parent) = compositor::get_parent(&focused) {
            focused = parent;
        }
        if focused != *surface {
            return;
        }
        self.cursor
            .set_image(crate::protocol::cursor::CursorImage::default_named());
        let serial = smithay::utils::SERIAL_COUNTER.next_serial();
        let location = pointer.current_location();
        pointer.unset_grab(self, serial, 0);
        pointer.motion(
            self,
            None,
            &smithay::input::pointer::MotionEvent {
                location,
                serial,
                time: 0,
            },
        );
        pointer.frame(self);
        self.protocol_globals.activation.sync_pointer_focus(None);
        self.protocol_globals
            .pointer_gestures
            .focus_changed(None, serial, 0);
        let _ = self
            .protocol_globals
            .pointer_constraints
            .focus_changed(None, location);
    }

    pub(in crate::protocol) fn remove_session_lock_surface(&mut self, surface: &WlSurface) {
        if let Some(removed) = self
            .protocol_globals
            .session_lock
            .remove_wl_surface(surface)
        {
            self.session_lock_surface_removed(removed);
        }
    }

    fn validate_session_lock_commit(&mut self, surface: &WlSurface) {
        let change = with_states(surface, |states| {
            let mut attributes = states.cached_state.get::<SurfaceAttributes>();
            match attributes.pending().buffer.as_ref() {
                Some(BufferAssignment::NewBuffer(buffer)) => {
                    PendingBufferChange::New(pending_buffer_logical_size(states, buffer))
                }
                Some(BufferAssignment::Removed) => PendingBufferChange::Removed,
                None => PendingBufferChange::None,
            }
        });
        let Err(failure) = self
            .protocol_globals
            .session_lock
            .validate_commit(surface, change)
        else {
            return;
        };
        match failure.error {
            CommitError::BeforeFirstAck => failure.resource.post_error(
                ext_session_lock_surface_v1::Error::CommitBeforeFirstAck,
                "wl_surface committed before the first configure was acknowledged",
            ),
            CommitError::NullBuffer => failure.resource.post_error(
                ext_session_lock_surface_v1::Error::NullBuffer,
                "lock surfaces cannot attach a null buffer",
            ),
            CommitError::DimensionsMismatch => failure.resource.post_error(
                ext_session_lock_surface_v1::Error::DimensionsMismatch,
                "buffer dimensions do not match the acknowledged configure",
            ),
        }
    }
}

fn surface_has_buffer_or_pending(surface: &WlSurface) -> bool {
    if surface_has_buffer(surface) {
        return true;
    }
    with_states(surface, |states| {
        let mut attributes = states.cached_state.get::<SurfaceAttributes>();
        matches!(
            attributes.pending().buffer,
            Some(BufferAssignment::NewBuffer(_))
        ) || matches!(
            attributes.current().buffer,
            Some(BufferAssignment::NewBuffer(_))
        )
    })
}

fn send_configure(surface: &ExtSessionLockSurfaceV1, configure: LockConfigure) {
    surface.configure(configure.serial, configure.size.0, configure.size.1);
}

fn lock_surface_pre_commit(
    state: &mut RuntimeState,
    _display: &DisplayHandle,
    surface: &WlSurface,
) {
    state.validate_session_lock_commit(surface);
}

delegate_global_dispatch!(RuntimeState, ExtSessionLockManagerV1, SessionLockGlobalData);
delegate_dispatch!(
    RuntimeState,
    ExtSessionLockManagerV1,
    SessionLockManagerData
);
delegate_dispatch!(RuntimeState, ExtSessionLockV1, SessionLockData);
delegate_dispatch!(
    RuntimeState,
    ExtSessionLockSurfaceV1,
    SessionLockSurfaceData
);
