//! Secure session-lock methods on [`NativeRuntime`].

use crate::{OutputId, RuntimeError, SessionLockState, SurfaceId};

use super::runtime_facade::{NativeRuntime, map_native_error};

impl NativeRuntime {
    pub fn session_lock_state(&self) -> SessionLockState {
        self.shell.session_lock_state()
    }

    pub fn begin_session_lock(&mut self) -> Result<Vec<(OutputId, SurfaceId)>, RuntimeError> {
        if !self.capabilities.session_lock_v1 {
            return Err(RuntimeError::Unsupported("ext-session-lock-v1"));
        }
        Ok(self
            .shell
            .begin_session_lock()
            .map_err(map_native_error)?
            .into_iter()
            .map(|(output, native)| {
                let surface = self.surfaces.intern(native);
                self.native_ids.insert(surface, native);
                (OutputId::from_raw(output), surface)
            })
            .collect())
    }

    pub fn finish_session_lock(&mut self) -> Result<Vec<SurfaceId>, RuntimeError> {
        let native = self.shell.finish_session_lock().map_err(map_native_error)?;
        Ok(native
            .into_iter()
            .filter_map(|native| self.surfaces.get(native))
            .collect())
    }
}
