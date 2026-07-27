//! Idle-notify and xdg-foreign methods on [`NativeRuntime`].

use crate::native::shell::IdleNotifyKind;
use crate::runtime_common::RuntimeError;
use crate::surface::SurfaceId;

use super::runtime_facade::{map_native_error, NativeRuntime};

impl NativeRuntime {
    pub fn has_idle_notify(&self) -> bool {
        self.shell.has_idle_notify()
    }

    pub fn has_idle_notify_input(&self) -> bool {
        self.shell.has_idle_notify_input()
    }

    /// Create a seat-scoped idle notification (`ext-idle-notify-v1`).
    pub fn create_idle_notification(
        &mut self,
        timeout_ms: u32,
        seat: Option<crate::SeatId>,
        kind: IdleNotifyKind,
    ) -> Result<u64, RuntimeError> {
        if !self.shell.has_idle_notify() {
            return Err(RuntimeError::Unsupported("ext_idle_notifier_v1"));
        }
        self.shell
            .create_idle_notification(timeout_ms, seat, kind)
            .map_err(map_native_error)
    }

    pub fn destroy_idle_notification(&mut self, id: u64) -> Result<(), RuntimeError> {
        self.shell
            .destroy_idle_notification(id)
            .map_err(map_native_error)
    }

    pub fn has_xdg_foreign(&self) -> bool {
        self.shell.has_xdg_foreign()
    }

    /// Export a toplevel; completes with [`crate::Event::Foreign`].
    pub fn export_toplevel(&mut self, surface: SurfaceId) -> Result<(), RuntimeError> {
        if !self.shell.has_xdg_foreign() {
            return Err(RuntimeError::Unsupported("zxdg_exporter_v2"));
        }
        let native = self.native(surface)?;
        self.shell
            .export_toplevel(native)
            .map_err(map_native_error)
    }

    pub fn unexport_toplevel(&mut self, surface: SurfaceId) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        self.shell
            .unexport_toplevel(native)
            .map_err(map_native_error)
    }

    pub fn import_toplevel(&mut self, handle: impl Into<String>) -> Result<u64, RuntimeError> {
        if !self.shell.has_xdg_foreign() {
            return Err(RuntimeError::Unsupported("zxdg_importer_v2"));
        }
        self.shell
            .import_toplevel(handle)
            .map_err(map_native_error)
    }

    pub fn set_foreign_parent_of(
        &mut self,
        import_id: u64,
        child: SurfaceId,
    ) -> Result<(), RuntimeError> {
        let native = self.native(child)?;
        self.shell
            .set_foreign_parent_of(import_id, native)
            .map_err(map_native_error)
    }

    pub fn destroy_foreign_import(&mut self, import_id: u64) -> Result<(), RuntimeError> {
        self.shell
            .destroy_foreign_import(import_id)
            .map_err(map_native_error)
    }
}
