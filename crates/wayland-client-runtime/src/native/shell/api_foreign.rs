//! `xdg-foreign-unstable-v2` helpers on [`NativeShell`].

use wayland_client::Proxy;

use super::api::NativeShell;
use super::types::NativeSurfaceId;
use crate::native::connection::NativeError;

impl NativeShell {
    pub fn has_xdg_foreign(&self) -> bool {
        self.state.xdg_exporter.is_some() && self.state.xdg_importer.is_some()
    }

    /// Export a toplevel so another client can import it via the handle event.
    ///
    /// Completes with [`super::types::NativeShellEvent::ForeignExported`].
    pub fn export_toplevel(&mut self, id: NativeSurfaceId) -> Result<(), NativeError> {
        let exporter = self
            .state
            .xdg_exporter
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("zxdg_exporter_v2 missing".into()))?
            .clone();
        let wl = self
            .state
            .wl_surface(id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?
            .clone();
        // Only xdg_toplevel (or dialog) content surfaces are valid exports.
        if !self.state.toplevels.contains_key(&id) {
            return Err(NativeError::Protocol(
                "export_toplevel requires an xdg_toplevel surface".into(),
            ));
        }
        if let Some(old) = self.state.foreign_exports.remove(&id) {
            self.state
                .foreign_export_objects
                .remove(&old.id().protocol_id());
            old.destroy();
        }
        let qh = self.queue.handle();
        let exported = exporter.export_toplevel(&wl, &qh, ());
        self.state
            .foreign_export_objects
            .insert(exported.id().protocol_id(), id);
        self.state.foreign_exports.insert(id, exported);
        self.connection.mark_dirty();
        Ok(())
    }

    /// Revoke a previous [`Self::export_toplevel`] for `id`.
    pub fn unexport_toplevel(&mut self, id: NativeSurfaceId) -> Result<(), NativeError> {
        let Some(exported) = self.state.foreign_exports.remove(&id) else {
            return Ok(());
        };
        self.state
            .foreign_export_objects
            .remove(&exported.id().protocol_id());
        exported.destroy();
        self.connection.mark_dirty();
        Ok(())
    }

    /// Import a remote exported handle. Returns a client-local import id.
    ///
    /// Use [`Self::set_foreign_parent_of`] to stack a local surface above it.
    /// Emits [`super::types::NativeShellEvent::ForeignImportedDestroyed`] if
    /// the export is revoked.
    pub fn import_toplevel(&mut self, handle: impl Into<String>) -> Result<u64, NativeError> {
        let importer = self
            .state
            .xdg_importer
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("zxdg_importer_v2 missing".into()))?
            .clone();
        let qh = self.queue.handle();
        let imported = importer.import_toplevel(handle.into(), &qh, ());
        let id = self.state.next_foreign_import_id;
        self.state.next_foreign_import_id = id.saturating_add(1);
        self.state
            .foreign_import_objects
            .insert(imported.id().protocol_id(), id);
        self.state.foreign_imports.insert(id, imported);
        self.connection.mark_dirty();
        Ok(id)
    }

    /// Set `child` as a transient child of the imported foreign surface.
    pub fn set_foreign_parent_of(
        &mut self,
        import_id: u64,
        child: NativeSurfaceId,
    ) -> Result<(), NativeError> {
        let imported = self
            .state
            .foreign_imports
            .get(&import_id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown foreign import {import_id}")))?;
        let wl = self
            .state
            .wl_surface(child)
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {child:?}")))?;
        imported.set_parent_of(wl);
        self.connection.mark_dirty();
        Ok(())
    }

    pub fn destroy_foreign_import(&mut self, import_id: u64) -> Result<(), NativeError> {
        let Some(imported) = self.state.foreign_imports.remove(&import_id) else {
            return Ok(());
        };
        self.state
            .foreign_import_objects
            .remove(&imported.id().protocol_id());
        imported.destroy();
        self.connection.mark_dirty();
        Ok(())
    }
}
