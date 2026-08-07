//! `ext-session-lock-v1` event dispatch.

use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::ext::session_lock::v1::client::{
    ext_session_lock_manager_v1, ext_session_lock_surface_v1, ext_session_lock_v1,
};

use super::types::{NativeShellEvent, NativeShellState};
use crate::SessionLockState;

impl Dispatch<ext_session_lock_manager_v1::ExtSessionLockManagerV1, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &ext_session_lock_manager_v1::ExtSessionLockManagerV1,
        _: ext_session_lock_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ext_session_lock_v1::ExtSessionLockV1, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        lock: &ext_session_lock_v1::ExtSessionLockV1,
        event: ext_session_lock_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(record) = state
            .session_lock
            .as_mut()
            .filter(|record| record.lock == *lock)
        else {
            return;
        };
        match event {
            ext_session_lock_v1::Event::Locked => {
                record.state = SessionLockState::Locked;
                record.was_locked = true;
                state.push(NativeShellEvent::SessionLocked);
            }
            ext_session_lock_v1::Event::Finished => {
                let was_locked = record.was_locked;
                record.state = SessionLockState::Finished;
                state.push(NativeShellEvent::SessionLockFinished { was_locked });
            }
            _ => {}
        }
    }
}

impl Dispatch<ext_session_lock_surface_v1::ExtSessionLockSurfaceV1, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        role: &ext_session_lock_surface_v1::ExtSessionLockSurfaceV1,
        event: ext_session_lock_surface_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(id) = state
            .session_lock_surface_objects
            .get(&role.id().protocol_id())
            .copied()
        else {
            return;
        };
        if let ext_session_lock_surface_v1::Event::Configure {
            serial,
            width,
            height,
        } = event
        {
            role.ack_configure(serial);
            let Some(record) = state.session_lock_surfaces.get_mut(&id) else {
                return;
            };
            record.configured = true;
            record.logical_w = width;
            record.logical_h = height;
            if width <= i32::MAX as u32
                && height <= i32::MAX as u32
                && let Some(viewport) = record.viewport.as_ref()
            {
                viewport.set_destination(width as i32, height as i32);
            }
            let output = record.output;
            state.push(NativeShellEvent::SessionLockConfigure {
                surface: id,
                output,
                width,
                height,
                serial,
            });
        }
    }
}
