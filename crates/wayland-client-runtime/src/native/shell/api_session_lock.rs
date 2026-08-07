//! Secure `ext-session-lock-v1` lifecycle on [`NativeShell`].

use std::sync::Arc;

use wayland_client::Proxy;

use super::api::NativeShell;
use super::handle::NativeSurfaceLease;
use super::types::{NativeSurfaceId, SessionLockRecord, SessionLockSurfaceRecord};
use crate::SessionLockState;
use crate::native::connection::NativeError;

impl NativeShell {
    pub fn session_lock_state(&self) -> SessionLockState {
        self.state
            .session_lock
            .as_ref()
            .map_or(SessionLockState::Unlocked, |lock| lock.state)
    }

    /// Request a secure session lock and create one role-less GPU surface for
    /// every output currently advertised by the compositor.
    pub fn begin_session_lock(&mut self) -> Result<Vec<(u32, NativeSurfaceId)>, NativeError> {
        if self.state.session_lock.is_some() {
            return Err(NativeError::Protocol(
                "a session-lock request is already active".into(),
            ));
        }
        let manager = self
            .state
            .session_lock_manager
            .as_ref()
            .ok_or_else(|| NativeError::Registry("ext_session_lock_manager_v1".into()))?;
        let qh = self.queue.handle();
        let lock = manager.lock(&qh, ());
        self.state.session_lock = Some(SessionLockRecord {
            lock,
            state: SessionLockState::Pending,
            was_locked: false,
        });

        let mut outputs = self
            .state
            .output_proxies
            .keys()
            .copied()
            .collect::<Vec<_>>();
        outputs.sort_unstable();
        let mut surfaces = Vec::with_capacity(outputs.len());
        for output in outputs {
            match self.create_session_lock_surface(output) {
                Ok(surface) => surfaces.push((output, surface)),
                Err(error) => {
                    if let Ok(partial) = self.finish_session_lock() {
                        for surface in partial {
                            let _ = self.destroy_session_lock_surface(surface);
                        }
                    }
                    return Err(error);
                }
            }
        }
        self.connection.mark_dirty();
        Ok(surfaces)
    }

    /// Add the required lock surface for an output advertised after locking began.
    pub fn create_session_lock_surface(
        &mut self,
        output: u32,
    ) -> Result<NativeSurfaceId, NativeError> {
        let lock = self
            .state
            .session_lock
            .as_ref()
            .filter(|record| record.state != SessionLockState::Finished)
            .map(|record| record.lock.clone())
            .ok_or_else(|| NativeError::Protocol("no usable session-lock request".into()))?;
        if self.state.session_lock_outputs.contains_key(&output) {
            return Err(NativeError::Protocol(format!(
                "output {output} already has a session-lock surface"
            )));
        }
        let output_proxy = self
            .state
            .output_proxies
            .get(&output)
            .cloned()
            .ok_or_else(|| NativeError::Protocol(format!("unknown output {output}")))?;
        let compositor = self
            .state
            .compositor
            .as_ref()
            .ok_or_else(|| NativeError::Registry("wl_compositor".into()))?;
        let qh = self.queue.handle();
        let wl = compositor.create_surface(&qh, ());
        let viewport = self
            .state
            .viewporter
            .as_ref()
            .map(|manager| manager.get_viewport(&wl, &qh, ()));
        let fractional = self
            .state
            .fractional_manager
            .as_ref()
            .map(|manager| manager.get_fractional_scale(&wl, &qh, ()));
        let role = lock.get_lock_surface(&wl, &output_proxy, &qh, ());
        let id = self.state.alloc_id();
        self.state
            .session_lock_surface_objects
            .insert(role.id().protocol_id(), id);
        self.state
            .wl_surface_objects
            .insert(wl.id().protocol_id(), id);
        if let Some(fractional) = fractional.as_ref() {
            self.state
                .fractional_objects
                .insert(fractional.id().protocol_id(), id);
        }
        let surface_lease = Arc::new(NativeSurfaceLease::session_lock(
            self.connection.connection().clone(),
            wl.clone(),
            id,
            role.clone(),
        ));
        self.state.session_lock_outputs.insert(output, id);
        self.state.session_lock_surfaces.insert(
            id,
            SessionLockSurfaceRecord {
                surface_lease,
                wl,
                role,
                output,
                viewport,
                fractional,
                scale_factor: 1.0,
                configured: false,
                logical_w: 0,
                logical_h: 0,
            },
        );
        self.state
            .push(super::types::NativeShellEvent::SessionLockSurfaceAdded {
                surface: id,
                output,
            });
        self.connection.mark_dirty();
        Ok(id)
    }

    pub fn destroy_session_lock_surface(&mut self, id: NativeSurfaceId) -> Result<(), NativeError> {
        self.state.cancel_touch_for_surface(id);
        self.state.clear_surface_protocol_state(id);
        let record = self
            .state
            .session_lock_surfaces
            .remove(&id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown session-lock surface {id:?}")))?;
        self.state.session_lock_outputs.remove(&record.output);
        self.state
            .session_lock_surface_objects
            .remove(&record.role.id().protocol_id());
        self.state
            .wl_surface_objects
            .remove(&record.wl.id().protocol_id());
        if let Some(fractional) = record.fractional {
            self.state
                .fractional_objects
                .remove(&fractional.id().protocol_id());
            fractional.destroy();
        }
        if let Some(viewport) = record.viewport {
            viewport.destroy();
        }
        self.connection.mark_dirty();
        Ok(())
    }

    /// Finish the lock using the only destructor legal for its wire state.
    ///
    /// Returned surfaces stay live so renderers can first destroy their Vulkan
    /// surfaces. Call [`Self::destroy_session_lock_surface`] afterwards.
    pub fn finish_session_lock(&mut self) -> Result<Vec<NativeSurfaceId>, NativeError> {
        let record = self
            .state
            .session_lock
            .take()
            .ok_or_else(|| NativeError::Protocol("no session-lock request is active".into()))?;
        if record.was_locked {
            record.lock.unlock_and_destroy();
        } else {
            record.lock.destroy();
        }
        let mut surfaces = self
            .state
            .session_lock_surfaces
            .keys()
            .copied()
            .collect::<Vec<_>>();
        surfaces.sort_unstable_by_key(|surface| surface.get());
        self.connection.mark_dirty();
        Ok(surfaces)
    }
}
