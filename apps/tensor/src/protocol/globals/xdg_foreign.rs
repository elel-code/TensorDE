//! Tensor-owned xdg-foreign-v2 handles and parent relationships.

use std::{collections::HashMap, io, sync::Arc};

use tracing::warn;
use wayland_protocols::xdg::foreign::zv2::server::{
    zxdg_exported_v2::{self, ZxdgExportedV2},
    zxdg_exporter_v2::{self, ZxdgExporterV2},
    zxdg_imported_v2::{self, ZxdgImportedV2},
    zxdg_importer_v2::{self, ZxdgImporterV2},
};
use wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource, Weak,
    backend::{ClientId, GlobalId, ObjectId},
    protocol::wl_surface::WlSurface,
};

use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    state::RuntimeState,
};

const HANDLE_BYTES: usize = 32;
const MAX_EXPORTS: usize = 4_096;
const MAX_IMPORTS: usize = 8_192;

pub(crate) struct XdgForeignProtocol {
    _exporter_global: GlobalId,
    _importer_global: GlobalId,
    exports: HashMap<Arc<str>, Export>,
    exports_by_surface: HashMap<ObjectId, Vec<Arc<str>>>,
    relations: HashMap<ObjectId, Relation>,
    relation_owners: HashMap<ObjectId, ObjectId>,
    import_count: usize,
}

struct Export {
    surface: Weak<WlSurface>,
    importers: HashMap<ObjectId, Weak<ZxdgImportedV2>>,
}

struct Relation {
    child: Weak<WlSurface>,
    parent: ObjectId,
}

impl XdgForeignProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        Self {
            _exporter_global: display
                .create_global::<RuntimeState, ZxdgExporterV2, _>(1, ExporterGlobalData),
            _importer_global: display
                .create_global::<RuntimeState, ZxdgImporterV2, _>(1, ImporterGlobalData),
            exports: HashMap::new(),
            exports_by_surface: HashMap::new(),
            relations: HashMap::new(),
            relation_owners: HashMap::new(),
            import_count: 0,
        }
    }

    fn register_export(&mut self, surface: &WlSurface) -> io::Result<Arc<str>> {
        if self.exports.len() >= MAX_EXPORTS {
            return Err(io::Error::other("xdg-foreign export table is full"));
        }
        let handle = self.mint_handle()?;
        self.exports.insert(
            handle.clone(),
            Export {
                surface: surface.downgrade(),
                importers: HashMap::new(),
            },
        );
        self.exports_by_surface
            .entry(surface.id())
            .or_default()
            .push(handle.clone());
        Ok(handle)
    }

    fn mint_handle(&self) -> io::Result<Arc<str>> {
        for _ in 0..4 {
            let handle = Arc::<str>::from(super::random_handle::random_hex::<HANDLE_BYTES>()?);
            if !self.exports.contains_key(handle.as_ref()) {
                return Ok(handle);
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "kernel CSPRNG repeatedly produced an existing xdg-foreign handle",
        ))
    }

    fn live_handle(&self, handle: &str) -> Option<Arc<str>> {
        let (handle, export) = self.exports.get_key_value(handle)?;
        export.surface.is_alive().then(|| handle.clone())
    }

    fn register_import(&mut self, handle: &Arc<str>, resource: &ZxdgImportedV2) -> bool {
        if self.import_count >= MAX_IMPORTS {
            return false;
        }
        let Some(export) = self.exports.get_mut(handle.as_ref()) else {
            return false;
        };
        export.importers.insert(resource.id(), resource.downgrade());
        self.import_count += 1;
        true
    }

    fn imported_parent(&self, handle: &str, imported: &ObjectId) -> Option<WlSurface> {
        let export = self.exports.get(handle)?;
        export.importers.contains_key(imported).then_some(())?;
        export.surface.upgrade().ok()
    }

    fn remove_import(&mut self, handle: &str, imported: &ObjectId) {
        if self
            .exports
            .get_mut(handle)
            .is_some_and(|export| export.importers.remove(imported).is_some())
        {
            self.import_count -= 1;
        }
    }

    fn install_relation(
        &mut self,
        imported: ObjectId,
        child: &WlSurface,
        parent: &WlSurface,
    ) -> Option<Relation> {
        let previous = self.take_relation(&imported);
        let child_id = child.id();
        if let Some(previous_owner) = self
            .relation_owners
            .insert(child_id.clone(), imported.clone())
            && previous_owner != imported
        {
            self.relations.remove(&previous_owner);
        }
        self.relations.insert(
            imported,
            Relation {
                child: child.downgrade(),
                parent: parent.id(),
            },
        );
        previous
    }

    fn take_relation(&mut self, imported: &ObjectId) -> Option<Relation> {
        let relation = self.relations.remove(imported)?;
        let child = relation.child.id();
        if self.relation_owners.get(&child) == Some(imported) {
            self.relation_owners.remove(&child);
        }
        Some(relation)
    }

    fn remove_child_relation(&mut self, child: &ObjectId) {
        if let Some(imported) = self.relation_owners.remove(child) {
            self.relations.remove(&imported);
        }
    }

    fn take_export(&mut self, handle: &str) -> Option<Export> {
        let (handle, export) = self.exports.remove_entry(handle)?;
        self.remove_surface_handle(&export.surface.id(), &handle);
        self.import_count -= export.importers.len();
        Some(export)
    }

    fn take_surface_exports(&mut self, surface: &ObjectId) -> Vec<Export> {
        let Some(handles) = self.exports_by_surface.remove(surface) else {
            return Vec::new();
        };
        handles
            .into_iter()
            .filter_map(|handle| {
                let export = self.exports.remove(handle.as_ref())?;
                self.import_count -= export.importers.len();
                Some(export)
            })
            .collect()
    }

    fn remove_surface_handle(&mut self, surface: &ObjectId, handle: &Arc<str>) {
        let remove_entry = if let Some(handles) = self.exports_by_surface.get_mut(surface) {
            handles.retain(|candidate| candidate != handle);
            handles.is_empty()
        } else {
            false
        };
        if remove_entry {
            self.exports_by_surface.remove(surface);
        }
    }

    #[cfg(test)]
    pub(crate) fn counts(&self) -> (usize, usize, usize) {
        (self.exports.len(), self.import_count, self.relations.len())
    }
}

pub(in crate::protocol) struct ExporterGlobalData;
pub(in crate::protocol) struct ImporterGlobalData;
pub(in crate::protocol) struct ExporterData;
pub(in crate::protocol) struct ImporterData;

pub(in crate::protocol) struct ExportedData {
    handle: Option<Arc<str>>,
}

pub(in crate::protocol) struct ImportedData {
    handle: Option<Arc<str>>,
}

impl GlobalDispatchDelegate<ZxdgExporterV2, RuntimeState> for ExporterGlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<ZxdgExporterV2>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, ExporterData);
    }
}

impl GlobalDispatchDelegate<ZxdgImporterV2, RuntimeState> for ImporterGlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<ZxdgImporterV2>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, ImporterData);
    }
}

impl DispatchDelegate<ZxdgExporterV2, RuntimeState> for ExporterData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        exporter: &ZxdgExporterV2,
        request: zxdg_exporter_v2::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zxdg_exporter_v2::Request::ExportToplevel { id, surface } => {
                if !state.protocol_globals.xdg_shell.is_toplevel(&surface) {
                    exporter.post_error(
                        zxdg_exporter_v2::Error::InvalidSurface,
                        "exported surface is not an xdg_toplevel",
                    );
                    return;
                }
                let handle = state.protocol_globals.xdg_foreign.register_export(&surface);
                let exported = data_init.init(
                    id,
                    ExportedData {
                        handle: handle.as_ref().ok().cloned(),
                    },
                );
                match handle {
                    Ok(handle) => exported.handle(handle.to_string()),
                    Err(error) => {
                        warn!(%error, "could not create xdg-foreign export");
                        exported.handle(String::new());
                    }
                }
            }
            zxdg_exporter_v2::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<ZxdgExportedV2, RuntimeState> for ExportedData {
    fn request(
        &self,
        _state: &mut RuntimeState,
        _client: &Client,
        _resource: &ZxdgExportedV2,
        request: zxdg_exported_v2::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zxdg_exported_v2::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, state: &mut RuntimeState, _client: ClientId, _resource: &ZxdgExportedV2) {
        if let Some(handle) = &self.handle {
            state.revoke_xdg_foreign_export(handle);
        }
    }
}

impl DispatchDelegate<ZxdgImporterV2, RuntimeState> for ImporterData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        _importer: &ZxdgImporterV2,
        request: zxdg_importer_v2::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zxdg_importer_v2::Request::ImportToplevel { id, handle } => {
                let handle = state.protocol_globals.xdg_foreign.live_handle(&handle);
                let imported = data_init.init(
                    id,
                    ImportedData {
                        handle: handle.clone(),
                    },
                );
                if !handle.is_some_and(|handle| {
                    state
                        .protocol_globals
                        .xdg_foreign
                        .register_import(&handle, &imported)
                }) {
                    imported.destroyed();
                }
            }
            zxdg_importer_v2::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<ZxdgImportedV2, RuntimeState> for ImportedData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        resource: &ZxdgImportedV2,
        request: zxdg_imported_v2::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zxdg_imported_v2::Request::SetParentOf { surface } => {
                let Some(handle) = &self.handle else {
                    return;
                };
                state.set_xdg_foreign_parent(handle, resource, surface);
            }
            zxdg_imported_v2::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, state: &mut RuntimeState, _client: ClientId, resource: &ZxdgImportedV2) {
        if let Some(handle) = &self.handle {
            state.destroy_xdg_foreign_import(handle, &resource.id());
        }
    }
}

impl RuntimeState {
    fn revoke_xdg_foreign_export(&mut self, handle: &str) {
        let Some(export) = self.protocol_globals.xdg_foreign.take_export(handle) else {
            return;
        };
        self.finish_revoked_xdg_foreign_export(export);
    }

    fn finish_revoked_xdg_foreign_export(&mut self, export: Export) {
        for (imported, resource) in export.importers {
            if let Ok(resource) = resource.upgrade() {
                resource.destroyed();
            }
            if let Some(relation) = self.protocol_globals.xdg_foreign.take_relation(&imported) {
                self.clear_xdg_foreign_relation(relation);
            }
        }
    }

    pub(crate) fn remove_xdg_foreign_surface(&mut self, surface: &WlSurface) {
        self.protocol_globals
            .xdg_foreign
            .remove_child_relation(&surface.id());
        let exports = self
            .protocol_globals
            .xdg_foreign
            .take_surface_exports(&surface.id());
        for export in exports {
            self.finish_revoked_xdg_foreign_export(export);
        }
    }

    fn destroy_xdg_foreign_import(&mut self, handle: &str, imported: &ObjectId) {
        self.protocol_globals
            .xdg_foreign
            .remove_import(handle, imported);
        if let Some(relation) = self.protocol_globals.xdg_foreign.take_relation(imported) {
            self.clear_xdg_foreign_relation(relation);
        }
    }

    fn set_xdg_foreign_parent(
        &mut self,
        handle: &str,
        imported: &ZxdgImportedV2,
        child: WlSurface,
    ) {
        let Some(parent) = self
            .protocol_globals
            .xdg_foreign
            .imported_parent(handle, &imported.id())
        else {
            return;
        };
        let Some(child_toplevel) = self.protocol_globals.xdg_shell.toplevel_for_surface(&child)
        else {
            imported.post_error(
                zxdg_imported_v2::Error::InvalidSurface,
                "xdg-foreign child is not a live xdg_toplevel",
            );
            return;
        };
        let Some(parent_toplevel) = self
            .protocol_globals
            .xdg_shell
            .toplevel_for_surface(&parent)
        else {
            imported.post_error(
                zxdg_imported_v2::Error::InvalidSurface,
                "xdg-foreign parent is not a live xdg_toplevel",
            );
            return;
        };
        if !child_toplevel.set_parent(Some(parent_toplevel)) {
            imported.post_error(
                zxdg_imported_v2::Error::InvalidSurface,
                "xdg-foreign parent relationship is cyclic",
            );
            return;
        }

        let previous =
            self.protocol_globals
                .xdg_foreign
                .install_relation(imported.id(), &child, &parent);
        if let Some(previous) = previous
            && previous.child.id() != child.id()
        {
            self.clear_xdg_foreign_relation(previous);
        }
    }

    fn clear_xdg_foreign_relation(&mut self, relation: Relation) {
        let Ok(child) = relation.child.upgrade() else {
            return;
        };
        let Some(toplevel) = self.protocol_globals.xdg_shell.toplevel_for_surface(&child) else {
            return;
        };
        if toplevel.parent_surface().as_ref().map(Resource::id) == Some(relation.parent) {
            toplevel.set_parent(None);
        }
    }
}

delegate_global_dispatch!(RuntimeState, ZxdgExporterV2, ExporterGlobalData);
delegate_global_dispatch!(RuntimeState, ZxdgImporterV2, ImporterGlobalData);
delegate_dispatch!(RuntimeState, ZxdgExporterV2, ExporterData);
delegate_dispatch!(RuntimeState, ZxdgImporterV2, ImporterData);
delegate_dispatch!(RuntimeState, ZxdgExportedV2, ExportedData);
delegate_dispatch!(RuntimeState, ZxdgImportedV2, ImportedData);
