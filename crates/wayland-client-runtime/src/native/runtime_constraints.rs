//! Pointer constraint / relative-pointer methods on [`NativeRuntime`].

use crate::native::connection::NativeError;
use crate::runtime_common::RuntimeError;
use crate::surface::SurfaceId;

use super::runtime_facade::{NativeRuntime, map_native_error};

impl NativeRuntime {
    pub fn pointer_gestures_enabled(&self, _surface: SurfaceId) -> Result<bool, RuntimeError> {
        Ok(self.capabilities.pointer_gestures_v1)
    }

    pub fn set_pointer_gestures_enabled(
        &mut self,
        _surface: SurfaceId,
        _enabled: bool,
    ) -> Result<(), RuntimeError> {
        if self.capabilities.pointer_gestures_v1 {
            Ok(())
        } else {
            Err(RuntimeError::Unsupported("zwp_pointer_gestures_v1"))
        }
    }

    pub fn set_pointer_capture_state(
        &mut self,
        surface: SurfaceId,
        state: crate::PointerCaptureState,
    ) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        self.shell
            .set_pointer_capture_state(native, state)
            .map_err(|e| match e {
                NativeError::Protocol(msg) if msg.contains("pointer_constraints") => {
                    RuntimeError::Unsupported("zwp-pointer-constraints-v1")
                }
                NativeError::Protocol(msg) if msg.contains("relative_pointer") => {
                    RuntimeError::Unsupported("zwp-relative-pointer-v1")
                }
                other => map_native_error(other),
            })
    }

    pub fn set_pointer_constraint(
        &mut self,
        surface: SurfaceId,
        constraint: crate::PointerConstraint,
    ) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        self.shell
            .set_pointer_constraint(native, constraint)
            .map_err(|e| match e {
                NativeError::Protocol(msg) if msg.contains("pointer_constraints") => {
                    RuntimeError::Unsupported("zwp-pointer-constraints-v1")
                }
                other => map_native_error(other),
            })
    }

    pub fn set_relative_pointer_enabled(
        &mut self,
        surface: SurfaceId,
        enabled: bool,
    ) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        self.shell
            .set_relative_pointer_enabled(native, enabled)
            .map_err(|e| match e {
                NativeError::Protocol(msg) if msg.contains("relative_pointer") => {
                    RuntimeError::Unsupported("zwp-relative-pointer-v1")
                }
                other => map_native_error(other),
            })
    }

    pub fn set_pointer_constraint_region(
        &mut self,
        surface: SurfaceId,
        region: crate::PointerConstraintRegion,
    ) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        let mut capture = self
            .shell
            .state
            .toplevels
            .get(&native)
            .map(|r| r.pointer_capture.clone())
            .ok_or(RuntimeError::SurfaceNotFound(surface))?;
        capture.region = region;
        self.shell
            .set_pointer_capture_state(native, capture)
            .map_err(map_native_error)
    }

    pub fn set_locked_pointer_position_hint(
        &mut self,
        surface: SurfaceId,
        position: (f64, f64),
    ) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        self.shell
            .set_locked_pointer_position_hint(native, position)
            .map_err(|e| match e {
                NativeError::Protocol(msg) if msg.contains("not locked") => {
                    RuntimeError::PointerNotLocked(surface)
                }
                other => map_native_error(other),
            })
    }
}
