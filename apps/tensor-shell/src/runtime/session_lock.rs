use std::collections::BTreeSet;

use wayland_client_runtime::{SessionLockEvent, SessionLockState, SurfaceId};

use super::{ShellRuntime, ShellRuntimeError};
use crate::lock_surface_key;
use crate::session_lock_service::SessionLockServiceStatus;

impl ShellRuntime {
    pub(super) fn reconcile_session_lock_service(&mut self) -> Result<(), ShellRuntimeError> {
        let (revision, snapshot) = self.session_lock.read();
        if revision == self.session_lock_revision {
            return Ok(());
        }
        self.session_lock_revision = revision;
        match snapshot.status {
            SessionLockServiceStatus::Pending => Ok(()),
            SessionLockServiceStatus::Failed => {
                if self.wayland.session_lock_state() == SessionLockState::Unlocked {
                    Err(ShellRuntimeError::SessionLockMonitorStopped)
                } else {
                    Ok(())
                }
            }
            SessionLockServiceStatus::Ready if snapshot.desired_locked => {
                match self.wayland.session_lock_state() {
                    SessionLockState::Unlocked => self.begin_secure_lock(),
                    SessionLockState::Pending | SessionLockState::Locked => Ok(()),
                    SessionLockState::Finished => {
                        Err(ShellRuntimeError::SessionLockFinished { was_locked: false })
                    }
                }
            }
            SessionLockServiceStatus::Ready => match self.wayland.session_lock_state() {
                SessionLockState::Unlocked => Ok(()),
                SessionLockState::Pending
                | SessionLockState::Locked
                | SessionLockState::Finished => self.finish_secure_lock(),
            },
        }
    }

    pub(super) fn handle_session_lock_event(
        &mut self,
        event: &SessionLockEvent,
    ) -> Result<(), ShellRuntimeError> {
        match *event {
            SessionLockEvent::SurfaceAdded { surface, output } => {
                self.register_lock_surface(output, surface)
            }
            SessionLockEvent::Configure {
                surface, output, ..
            } => {
                self.register_lock_surface(output, surface)?;
                self.configured_surfaces.insert(surface);
                self.present_surface(surface)
            }
            SessionLockEvent::SurfaceRemoved { surface, output } => {
                if self.lock_surfaces.get(&output) != Some(&surface) {
                    return Err(ShellRuntimeError::ConflictingLockSurface { output });
                }
                self.lock_surfaces.remove(&output);
                self.surface_keys.remove(&surface);
                self.configured_surfaces.remove(&surface);
                self.remove_surface_state(surface);
                self.remove_presented_surface(surface)?;
                self.wayland.destroy_surface(surface)?;
                Ok(())
            }
            SessionLockEvent::Locked => {
                self.session_lock.set_locked_hint(true)?;
                Ok(())
            }
            SessionLockEvent::Finished { was_locked } => {
                self.finish_secure_lock()?;
                Err(ShellRuntimeError::SessionLockFinished { was_locked })
            }
        }
    }

    fn begin_secure_lock(&mut self) -> Result<(), ShellRuntimeError> {
        for (output, surface) in self.wayland.begin_session_lock()? {
            self.register_lock_surface(output, surface)?;
        }
        self.wayland.flush()?;
        Ok(())
    }

    fn register_lock_surface(
        &mut self,
        output: wayland_client_runtime::OutputId,
        surface: SurfaceId,
    ) -> Result<(), ShellRuntimeError> {
        if let Some(existing) = self.lock_surfaces.get(&output) {
            return if *existing == surface {
                Ok(())
            } else {
                Err(ShellRuntimeError::ConflictingLockSurface { output })
            };
        }
        if self.surface_keys.contains_key(&surface) {
            return Err(ShellRuntimeError::ConflictingLockSurface { output });
        }
        self.lock_surfaces.insert(output, surface);
        self.surface_keys.insert(surface, lock_surface_key(output));
        Ok(())
    }

    fn finish_secure_lock(&mut self) -> Result<(), ShellRuntimeError> {
        let mut surfaces = self
            .lock_surfaces
            .values()
            .copied()
            .collect::<BTreeSet<_>>();
        for surface in &surfaces {
            self.remove_presented_surface(*surface)?;
        }
        surfaces.extend(self.wayland.finish_session_lock()?);
        for surface in surfaces {
            self.surface_keys.remove(&surface);
            self.configured_surfaces.remove(&surface);
            self.remove_surface_state(surface);
            self.remove_presented_surface(surface)?;
            self.wayland.destroy_surface(surface)?;
        }
        self.lock_surfaces.clear();
        self.wayland.flush()?;
        self.session_lock.set_locked_hint(false)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ShellComponent;

    #[test]
    fn secure_lock_render_identity_is_not_a_layer_surface_plan() {
        let key = lock_surface_key(wayland_client_runtime::OutputId::from_raw(4));
        assert_eq!(key.component, ShellComponent::LockScreen);
        assert!(
            crate::surface_plan(key.component, key.output, crate::ShellLayout::default()).is_none()
        );
    }
}
