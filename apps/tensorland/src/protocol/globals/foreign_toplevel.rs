//! Tensor-owned `ext-foreign-toplevel-list-v1` wire and stable handles.

use std::sync::{
    Arc, Mutex, Weak as ArcWeak,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use wayland_protocols::ext::foreign_toplevel_list::v1::server::{
    ext_foreign_toplevel_handle_v1::{self, ExtForeignToplevelHandleV1},
    ext_foreign_toplevel_list_v1::{self, ExtForeignToplevelListV1},
};
use wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, New, Resource, Weak,
    backend::{ClientId, GlobalId},
};

use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    state::{ObjectKey, RuntimeState},
};

const VERSION: u32 = 1;
static NEXT_IDENTIFIER: AtomicU64 = AtomicU64::new(1);

pub(crate) struct ForeignToplevelListState {
    display: DisplayHandle,
    _global: GlobalId,
    lists: Vec<ExtForeignToplevelListV1>,
    toplevels: Vec<ForeignToplevelWeakHandle>,
}

#[derive(Debug)]
pub(in crate::protocol) struct ForeignToplevelGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct ForeignToplevelListData;

struct ForeignToplevelInner {
    identifier: String,
    title: String,
    app_id: String,
    instances: Vec<Weak<ExtForeignToplevelHandleV1>>,
}

struct ForeignToplevelShared {
    closed: AtomicBool,
    inner: Mutex<ForeignToplevelInner>,
}

/// Compositor-owned identity for one mapped toplevel.
#[derive(Clone)]
pub(crate) struct ForeignToplevelHandle {
    key: ObjectKey,
    shared: Arc<ForeignToplevelShared>,
}

impl std::fmt::Debug for ForeignToplevelHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ForeignToplevelHandle")
            .field("key", &self.key)
            .field("closed", &self.shared.closed.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug)]
pub(super) struct ForeignToplevelWeakHandle {
    key: ObjectKey,
    shared: ArcWeak<ForeignToplevelShared>,
}

impl ForeignToplevelWeakHandle {
    pub(super) fn live_key(&self) -> Option<ObjectKey> {
        let shared = self.shared.upgrade()?;
        (!shared.closed.load(Ordering::Acquire)).then_some(self.key)
    }

    fn upgrade(&self) -> Option<ForeignToplevelHandle> {
        Some(ForeignToplevelHandle {
            key: self.key,
            shared: self.shared.upgrade()?,
        })
    }
}

#[derive(Debug)]
pub(in crate::protocol) struct ForeignToplevelResourceData {
    handle: ForeignToplevelHandle,
}

impl ForeignToplevelHandle {
    fn new(key: ObjectKey, title: String, app_id: String, instance_capacity: usize) -> Self {
        let generation = NEXT_IDENTIFIER.fetch_add(1, Ordering::Relaxed);
        assert_ne!(generation, 0, "foreign-toplevel identifier space exhausted");
        Self {
            key,
            shared: Arc::new(ForeignToplevelShared {
                closed: AtomicBool::new(false),
                inner: Mutex::new(ForeignToplevelInner {
                    identifier: format!("tensor-{generation:016x}"),
                    title,
                    app_id,
                    instances: Vec::with_capacity(instance_capacity),
                }),
            }),
        }
    }

    fn downgrade(&self) -> ForeignToplevelWeakHandle {
        ForeignToplevelWeakHandle {
            key: self.key,
            shared: Arc::downgrade(&self.shared),
        }
    }

    fn init_instance(&self, resource: ExtForeignToplevelHandleV1) {
        if self.shared.closed.load(Ordering::Acquire) {
            resource.closed();
            return;
        }
        let mut inner = self.shared.inner.lock().unwrap();
        resource.identifier(inner.identifier.clone());
        resource.title(inner.title.clone());
        resource.app_id(inner.app_id.clone());
        resource.done();
        inner.instances.push(resource.downgrade());
    }

    fn remove_instance(&self, resource: &ExtForeignToplevelHandleV1) {
        self.shared
            .inner
            .lock()
            .unwrap()
            .instances
            .retain(|instance| instance != resource);
    }

    /// Send only changed metadata and one atomic `done` event.
    pub(in crate::protocol) fn send_metadata(&self, title: Option<&str>, app_id: Option<&str>) {
        if self.shared.closed.load(Ordering::Acquire) {
            return;
        }
        let mut inner = self.shared.inner.lock().unwrap();
        let title_changed = title.is_some_and(|title| inner.title != title);
        let app_id_changed = app_id.is_some_and(|app_id| inner.app_id != app_id);
        if !title_changed && !app_id_changed {
            return;
        }
        inner.instances.retain(|weak| {
            let Ok(resource) = weak.upgrade() else {
                return false;
            };
            if let Some(title) = title.filter(|_| title_changed) {
                resource.title(title.to_owned());
            }
            if let Some(app_id) = app_id.filter(|_| app_id_changed) {
                resource.app_id(app_id.to_owned());
            }
            resource.done();
            true
        });
        if let Some(title) = title.filter(|_| title_changed) {
            inner.title.clear();
            inner.title.push_str(title);
        }
        if let Some(app_id) = app_id.filter(|_| app_id_changed) {
            inner.app_id.clear();
            inner.app_id.push_str(app_id);
        }
    }

    pub(in crate::protocol) fn send_closed(&self) {
        if self.shared.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        for weak in self.shared.inner.lock().unwrap().instances.drain(..) {
            if let Ok(resource) = weak.upgrade() {
                resource.closed();
            }
        }
    }
}

pub(super) fn weak_handle_from_resource(
    resource: &ExtForeignToplevelHandleV1,
) -> Option<ForeignToplevelWeakHandle> {
    resource
        .data::<ForeignToplevelResourceData>()
        .map(|data| data.handle.downgrade())
}

pub(in crate::protocol) trait ForeignToplevelListHandler: 'static {
    fn foreign_toplevel_list_state(&mut self) -> &mut ForeignToplevelListState;
}

impl ForeignToplevelListHandler for RuntimeState {
    fn foreign_toplevel_list_state(&mut self) -> &mut ForeignToplevelListState {
        self.protocol_globals.foreign_toplevel_list()
    }
}

impl ForeignToplevelListState {
    pub(in crate::protocol) fn new<D>(display: &DisplayHandle) -> Self
    where
        D: ForeignToplevelListHandler,
        D: wayland_server::GlobalDispatch<ExtForeignToplevelListV1, ForeignToplevelGlobalData>,
    {
        let global = display
            .create_global::<D, ExtForeignToplevelListV1, _>(VERSION, ForeignToplevelGlobalData);
        Self {
            display: display.clone(),
            _global: global,
            lists: Vec::new(),
            toplevels: Vec::new(),
        }
    }

    pub(in crate::protocol) fn new_toplevel<D>(
        &mut self,
        key: ObjectKey,
        title: String,
        app_id: String,
    ) -> ForeignToplevelHandle
    where
        D: Dispatch<ExtForeignToplevelHandleV1, ForeignToplevelResourceData> + 'static,
    {
        self.lists.retain(Resource::is_alive);
        let handle = ForeignToplevelHandle::new(key, title, app_id, self.lists.len());
        for list in &self.lists {
            let Ok(client) = self.display.get_client(list.id()) else {
                continue;
            };
            let Ok(resource) = client.create_resource::<ExtForeignToplevelHandleV1, _, D>(
                &self.display,
                list.version(),
                ForeignToplevelResourceData {
                    handle: handle.clone(),
                },
            ) else {
                continue;
            };
            list.toplevel(&resource);
            handle.init_instance(resource);
        }
        self.toplevels
            .retain(|toplevel| toplevel.live_key().is_some());
        self.toplevels.push(handle.downgrade());
        handle
    }

    fn bind<D>(&mut self, client: &Client, list: ExtForeignToplevelListV1)
    where
        D: Dispatch<ExtForeignToplevelHandleV1, ForeignToplevelResourceData> + 'static,
    {
        self.toplevels.retain(|weak| {
            let Some(handle) = weak.upgrade() else {
                return false;
            };
            if handle.shared.closed.load(Ordering::Acquire) {
                return false;
            }
            if let Ok(resource) = client.create_resource::<ExtForeignToplevelHandleV1, _, D>(
                &self.display,
                list.version(),
                ForeignToplevelResourceData {
                    handle: handle.clone(),
                },
            ) {
                list.toplevel(&resource);
                handle.init_instance(resource);
            }
            true
        });
        self.lists.push(list);
    }

    fn remove_list(&mut self, list: &ExtForeignToplevelListV1) {
        self.lists.retain(|instance| instance != list);
    }
}

impl<D> GlobalDispatchDelegate<ExtForeignToplevelListV1, D> for ForeignToplevelGlobalData
where
    D: ForeignToplevelListHandler,
    D: Dispatch<ExtForeignToplevelListV1, ForeignToplevelListData>,
    D: Dispatch<ExtForeignToplevelHandleV1, ForeignToplevelResourceData>,
    D: 'static,
{
    fn bind(
        &self,
        state: &mut D,
        _display: &DisplayHandle,
        client: &Client,
        resource: New<ExtForeignToplevelListV1>,
        data_init: &mut DataInit<'_, D>,
    ) {
        let list = data_init.init(resource, ForeignToplevelListData);
        state.foreign_toplevel_list_state().bind::<D>(client, list);
    }
}

impl<D> DispatchDelegate<ExtForeignToplevelListV1, D> for ForeignToplevelListData
where
    D: ForeignToplevelListHandler,
    D: 'static,
{
    fn request(
        &self,
        state: &mut D,
        _client: &Client,
        list: &ExtForeignToplevelListV1,
        request: ext_foreign_toplevel_list_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            ext_foreign_toplevel_list_v1::Request::Stop => {
                state.foreign_toplevel_list_state().remove_list(list);
                list.finished();
            }
            ext_foreign_toplevel_list_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, state: &mut D, _client: ClientId, list: &ExtForeignToplevelListV1) {
        state.foreign_toplevel_list_state().remove_list(list);
    }
}

impl<D> DispatchDelegate<ExtForeignToplevelHandleV1, D> for ForeignToplevelResourceData
where
    D: 'static,
{
    fn request(
        &self,
        _state: &mut D,
        _client: &Client,
        _handle: &ExtForeignToplevelHandleV1,
        request: ext_foreign_toplevel_handle_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            ext_foreign_toplevel_handle_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, _state: &mut D, _client: ClientId, resource: &ExtForeignToplevelHandleV1) {
        self.handle.remove_instance(resource);
    }
}

delegate_global_dispatch!(
    RuntimeState,
    ExtForeignToplevelListV1,
    ForeignToplevelGlobalData
);
delegate_dispatch!(
    RuntimeState,
    ExtForeignToplevelListV1,
    ForeignToplevelListData
);
delegate_dispatch!(
    RuntimeState,
    ExtForeignToplevelHandleV1,
    ForeignToplevelResourceData
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_unique_printable_and_bounded() {
        let first =
            ForeignToplevelHandle::new(ObjectKey::from_protocol_id(1), "a".into(), "b".into(), 0);
        let second =
            ForeignToplevelHandle::new(ObjectKey::from_protocol_id(2), "a".into(), "b".into(), 0);
        let first = first.shared.inner.lock().unwrap().identifier.clone();
        let second = second.shared.inner.lock().unwrap().identifier.clone();
        assert_ne!(first, second);
        assert!(!first.is_empty() && first.len() <= 32);
        assert!(first.bytes().all(|byte| byte.is_ascii_graphic()));
    }

    #[test]
    fn closed_toplevel_invalidates_capture_identity() {
        let handle = ForeignToplevelHandle::new(
            ObjectKey::from_protocol_id(7),
            "title".into(),
            "app".into(),
            0,
        );
        let weak = handle.downgrade();
        assert_eq!(weak.live_key(), Some(ObjectKey::from_protocol_id(7)));
        handle.send_closed();
        assert_eq!(weak.live_key(), None);
    }
}
