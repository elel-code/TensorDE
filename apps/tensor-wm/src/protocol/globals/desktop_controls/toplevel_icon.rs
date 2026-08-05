use std::sync::{Arc, Mutex, OnceLock};

use super::super::compositor;
use wayland_protocols::xdg::toplevel_icon::v1::server::{
    xdg_toplevel_icon_manager_v1::{self, XdgToplevelIconManagerV1},
    xdg_toplevel_icon_v1::{self, XdgToplevelIconV1},
};
use wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource,
    backend::{ClientId, ObjectId},
};

use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    globals::shm::{ShmBufferLease, lease_shm_buffer},
    state::RuntimeState,
};

#[derive(Debug)]
pub(super) struct ToplevelIconGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct ToplevelIconManagerData;

#[derive(Debug, Default)]
pub(super) struct ToplevelIconData {
    builder: Mutex<IconBuilder>,
    frozen: OnceLock<Arc<IconSnapshot>>,
}

impl ToplevelIconData {
    fn is_immutable(&self) -> bool {
        self.frozen.get().is_some()
    }

    fn set_name(&self, name: String) -> bool {
        if self.is_immutable() {
            return false;
        }
        self.builder.lock().unwrap().name = Some(name);
        true
    }

    fn add_buffer(&self, buffer: IconBuffer) -> Option<ObjectId> {
        self.builder.lock().unwrap().add_buffer(buffer)
    }

    pub(super) fn freeze(&self) -> Arc<IconSnapshot> {
        self.frozen
            .get_or_init(|| {
                let builder = std::mem::take(&mut *self.builder.lock().unwrap());
                Arc::new(builder.freeze())
            })
            .clone()
    }

    pub(super) fn for_each_buffer(&self, mut callback: impl FnMut(&ObjectId)) {
        if let Some(snapshot) = self.frozen.get() {
            snapshot
                .buffers
                .iter()
                .for_each(|entry| callback(&entry.resource));
        } else {
            self.builder
                .lock()
                .unwrap()
                .buffers
                .iter()
                .for_each(|entry| callback(&entry.resource));
        }
    }
}

#[derive(Debug, Default)]
struct IconBuilder {
    name: Option<String>,
    buffers: Vec<IconBuffer>,
}

impl IconBuilder {
    fn add_buffer(&mut self, buffer: IconBuffer) -> Option<ObjectId> {
        let metadata = buffer.lease.metadata();
        if let Some(existing) = self.buffers.iter_mut().find(|existing| {
            let existing_metadata = existing.lease.metadata();
            existing_metadata.width == metadata.width
                && existing_metadata.height == metadata.height
                && existing.scale == buffer.scale
        }) {
            let replaced = std::mem::replace(existing, buffer).resource;
            (!self.references_buffer(&replaced)).then_some(replaced)
        } else {
            self.buffers.push(buffer);
            None
        }
    }

    fn references_buffer(&self, buffer: &ObjectId) -> bool {
        self.buffers.iter().any(|entry| &entry.resource == buffer)
    }

    fn freeze(self) -> IconSnapshot {
        IconSnapshot {
            name: self.name,
            buffers: self.buffers,
        }
    }
}

#[derive(Debug)]
struct IconBuffer {
    resource: ObjectId,
    lease: ShmBufferLease,
    scale: i32,
}

#[derive(Debug)]
pub(super) struct IconSnapshot {
    name: Option<String>,
    buffers: Vec<IconBuffer>,
}

impl IconSnapshot {
    pub(super) fn is_empty(&self) -> bool {
        self.name.is_none() && self.buffers.is_empty()
    }

    #[cfg(test)]
    pub(super) fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    #[cfg(test)]
    #[allow(unsafe_code)]
    pub(super) fn first_buffer_sample(&self) -> Option<(i32, i32, i32, [u8; 4])> {
        let buffer = self.buffers.first()?;
        let metadata = buffer.lease.metadata();
        let sample = buffer
            .lease
            .with_contents(|ptr, len, _| {
                (len >= 4).then(|| unsafe {
                    [
                        ptr.read(),
                        ptr.add(1).read(),
                        ptr.add(2).read(),
                        ptr.add(3).read(),
                    ]
                })
            })
            .ok()??;
        Some((metadata.width, metadata.height, buffer.scale, sample))
    }
}

impl GlobalDispatchDelegate<XdgToplevelIconManagerV1, RuntimeState> for ToplevelIconGlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<XdgToplevelIconManagerV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        let manager = data_init.init(resource, ToplevelIconManagerData);
        manager.done();
    }
}

impl DispatchDelegate<XdgToplevelIconManagerV1, RuntimeState> for ToplevelIconManagerData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        _manager: &XdgToplevelIconManagerV1,
        request: xdg_toplevel_icon_manager_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            xdg_toplevel_icon_manager_v1::Request::CreateIcon { id } => {
                data_init.init(id, ToplevelIconData::default());
            }
            xdg_toplevel_icon_manager_v1::Request::SetIcon { toplevel, icon } => {
                let Some(surface) = super::toplevel_surface(state, &toplevel) else {
                    return;
                };
                let snapshot = icon.map(|icon| {
                    icon.data::<ToplevelIconData>()
                        .expect("Tensor-created icon carries Tensor icon data")
                        .freeze()
                });
                let install_hook = state
                    .protocol_globals
                    .desktop_controls
                    .set_pending_icon(&surface, snapshot);
                if install_hook {
                    compositor::add_post_commit_hook::<RuntimeState, _>(&surface, icon_post_commit);
                }
            }
            xdg_toplevel_icon_manager_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<XdgToplevelIconV1, RuntimeState> for ToplevelIconData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        icon: &XdgToplevelIconV1,
        request: xdg_toplevel_icon_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            xdg_toplevel_icon_v1::Request::SetName { icon_name } => {
                if !self.set_name(icon_name) {
                    icon.post_error(
                        xdg_toplevel_icon_v1::Error::Immutable,
                        "the icon is immutable after its first toplevel assignment",
                    );
                }
            }
            xdg_toplevel_icon_v1::Request::AddBuffer { buffer, scale } => {
                if self.is_immutable() {
                    icon.post_error(
                        xdg_toplevel_icon_v1::Error::Immutable,
                        "the icon is immutable after its first toplevel assignment",
                    );
                    return;
                }
                let Some(lease) = lease_shm_buffer(&buffer) else {
                    icon.post_error(
                        xdg_toplevel_icon_v1::Error::InvalidBuffer,
                        "toplevel icons require a wl_shm buffer",
                    );
                    return;
                };
                let metadata = lease.metadata();
                if metadata.width != metadata.height {
                    icon.post_error(
                        xdg_toplevel_icon_v1::Error::InvalidBuffer,
                        "toplevel icon buffers must be square",
                    );
                    return;
                }
                let buffer_id = buffer.id();
                let replaced = self.add_buffer(IconBuffer {
                    resource: buffer_id.clone(),
                    lease,
                    scale,
                });
                state
                    .protocol_globals
                    .desktop_controls
                    .replace_icon_buffer(icon, buffer_id, replaced);
            }
            xdg_toplevel_icon_v1::Request::Destroy => {
                state
                    .protocol_globals
                    .desktop_controls
                    .unregister_icon(icon, self);
            }
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, state: &mut RuntimeState, _client: ClientId, icon: &XdgToplevelIconV1) {
        state
            .protocol_globals
            .desktop_controls
            .unregister_icon(icon, self);
    }
}

fn icon_post_commit(
    state: &mut RuntimeState,
    _display: &DisplayHandle,
    surface: &wayland_server::protocol::wl_surface::WlSurface,
) {
    state.protocol_globals.desktop_controls.commit_icon(surface);
}

delegate_global_dispatch!(
    RuntimeState,
    XdgToplevelIconManagerV1,
    ToplevelIconGlobalData
);
delegate_dispatch!(
    RuntimeState,
    XdgToplevelIconManagerV1,
    ToplevelIconManagerData
);
delegate_dispatch!(RuntimeState, XdgToplevelIconV1, ToplevelIconData);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_assignment_freezes_once_without_snapshot_copies() {
        let data = ToplevelIconData::default();
        assert!(data.set_name("org.tensor.Settings".to_owned()));

        let first = data.freeze();
        let second = data.freeze();
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.name(), Some("org.tensor.Settings"));
        assert!(!data.set_name("changed".to_owned()));
    }
}
