//! xdg-foreign-unstable-v2 dispatch.

use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::xdg::foreign::zv2::client::{
    zxdg_exported_v2, zxdg_exporter_v2, zxdg_imported_v2, zxdg_importer_v2,
};

use super::types::{NativeShellEvent, NativeShellState};

impl Dispatch<zxdg_exporter_v2::ZxdgExporterV2, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &zxdg_exporter_v2::ZxdgExporterV2,
        _: zxdg_exporter_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zxdg_importer_v2::ZxdgImporterV2, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &zxdg_importer_v2::ZxdgImporterV2,
        _: zxdg_importer_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zxdg_exported_v2::ZxdgExportedV2, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        exported: &zxdg_exported_v2::ZxdgExportedV2,
        event: zxdg_exported_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(surface) = state
            .foreign_export_objects
            .get(&exported.id().protocol_id())
            .copied()
        else {
            return;
        };
        if let zxdg_exported_v2::Event::Handle { handle } = event {
            state.push(NativeShellEvent::ForeignExported { surface, handle });
        }
    }
}

impl Dispatch<zxdg_imported_v2::ZxdgImportedV2, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        imported: &zxdg_imported_v2::ZxdgImportedV2,
        event: zxdg_imported_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(id) = state
            .foreign_import_objects
            .get(&imported.id().protocol_id())
            .copied()
        else {
            return;
        };
        if let zxdg_imported_v2::Event::Destroyed = event {
            state.foreign_imports.remove(&id);
            state
                .foreign_import_objects
                .remove(&imported.id().protocol_id());
            state.push(NativeShellEvent::ForeignImportedDestroyed { id });
        }
    }
}
