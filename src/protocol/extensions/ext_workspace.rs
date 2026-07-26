//! `ext-workspace-v1` (staging / tier-2) on Dispatch2.
//!
//! Mapping (Niri-inspired, simplified for Tensor):
//! - One workspace group for the session (all heads share the pool).
//! - Workspaces are a fixed vertical pool (`WorkspaceHost`).
//! - Coordinates: `[0, index]` (2D so clients treat them as ordered).
//! - Activate is supported; assign/create/remove are no-ops for now.

use std::collections::{HashMap, HashSet};

use smithay::{
    reexports::{
        wayland_protocols::ext::workspace::v1::server::{
            ext_workspace_group_handle_v1::{self, ExtWorkspaceGroupHandleV1},
            ext_workspace_handle_v1::{self, ExtWorkspaceHandleV1},
            ext_workspace_manager_v1::{self, ExtWorkspaceManagerV1},
        },
        wayland_server::{
            Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
            backend::ClientId,
        },
    },
    wayland::{Dispatch2, GlobalDispatch2},
};
use tracing::trace;

use crate::ecs::WorkspaceId;

const VERSION: u32 = 1;

/// User data for the manager object (local type for Dispatch2).
#[derive(Debug, Default)]
pub struct ManagerUserData;

/// User data for a workspace handle — stores its id.
#[derive(Clone, Copy, Debug)]
pub struct WorkspaceUserData {
    pub id: WorkspaceId,
}

/// User data for a group handle.
#[derive(Debug, Default)]
pub struct GroupUserData;

pub struct ExtWorkspaceManagerState {
    display: DisplayHandle,
    managers: HashSet<ExtWorkspaceManagerV1>,
    pending: HashMap<ExtWorkspaceManagerV1, Vec<WorkspaceId>>,
    workspaces: HashMap<WorkspaceId, WorkspaceData>,
    groups: Vec<ExtWorkspaceGroupHandleV1>,
}

struct WorkspaceData {
    name: String,
    index: u32,
    active: bool,
    handles: Vec<ExtWorkspaceHandleV1>,
}

pub struct ExtWorkspaceGlobalData {
    filter: Box<dyn for<'c> Fn(&'c Client) -> bool + Send + Sync>,
}

pub trait ExtWorkspaceHandler: 'static {
    fn ext_workspace_manager_state(&mut self) -> &mut ExtWorkspaceManagerState;
    fn activate_workspace_id(&mut self, id: WorkspaceId);
    fn workspace_snapshot(&self) -> WorkspaceProtocolSnapshot;
}

#[derive(Clone, Debug)]
pub struct WorkspaceProtocolSnapshot {
    pub active: WorkspaceId,
    pub count: u32,
}

impl ExtWorkspaceManagerState {
    pub fn new<D, F>(display: &DisplayHandle, filter: F) -> Self
    where
        D: GlobalDispatch<ExtWorkspaceManagerV1, ExtWorkspaceGlobalData>,
        D: Dispatch<ExtWorkspaceManagerV1, ManagerUserData>,
        D: ExtWorkspaceHandler,
        D: 'static,
        F: for<'c> Fn(&'c Client) -> bool + Send + Sync + 'static,
    {
        display.create_global::<D, ExtWorkspaceManagerV1, _>(
            VERSION,
            ExtWorkspaceGlobalData {
                filter: Box::new(filter),
            },
        );
        Self {
            display: display.clone(),
            managers: HashSet::new(),
            pending: HashMap::new(),
            workspaces: HashMap::new(),
            groups: Vec::new(),
        }
    }

    pub fn refresh(&mut self, snapshot: &WorkspaceProtocolSnapshot) {
        for index in 0..snapshot.count {
            let id = WorkspaceId::new(index);
            let active = id == snapshot.active;
            let name = (index + 1).to_string();
            let entry = self.workspaces.entry(id).or_insert_with(|| WorkspaceData {
                name: name.clone(),
                index,
                active,
                handles: Vec::new(),
            });
            let mut changed = false;
            if entry.name != name {
                entry.name = name;
                changed = true;
            }
            if entry.index != index {
                entry.index = index;
                changed = true;
            }
            if entry.active != active {
                entry.active = active;
                changed = true;
            }
            if changed {
                for handle in &entry.handles {
                    handle.name(entry.name.clone());
                    handle.coordinates(coords_bytes(entry.index));
                    handle.state(workspace_state(entry.active));
                }
            }
        }
        self.workspaces.retain(|id, data| {
            if id.get() < snapshot.count {
                true
            } else {
                for h in &data.handles {
                    h.removed();
                }
                false
            }
        });
        for manager in &self.managers {
            manager.done();
        }
        trace!(
            active = snapshot.active.get(),
            count = snapshot.count,
            "ext-workspace refreshed"
        );
    }

    fn ensure_initial_objects<D>(
        &mut self,
        client: &Client,
        manager: &ExtWorkspaceManagerV1,
        snapshot: &WorkspaceProtocolSnapshot,
    ) where
        D: Dispatch<ExtWorkspaceHandleV1, WorkspaceUserData>,
        D: Dispatch<ExtWorkspaceGroupHandleV1, GroupUserData>,
        D: 'static,
    {
        let Ok(group) = client.create_resource::<ExtWorkspaceGroupHandleV1, _, D>(
            &self.display,
            manager.version(),
            GroupUserData,
        ) else {
            return;
        };
        manager.workspace_group(&group);
        group.capabilities(ext_workspace_group_handle_v1::GroupCapabilities::empty());
        self.groups.push(group.clone());

        for index in 0..snapshot.count {
            let id = WorkspaceId::new(index);
            let active = id == snapshot.active;
            let name = (index + 1).to_string();
            let data = self.workspaces.entry(id).or_insert_with(|| WorkspaceData {
                name: name.clone(),
                index,
                active,
                handles: Vec::new(),
            });
            let Ok(ws) = client.create_resource::<ExtWorkspaceHandleV1, _, D>(
                &self.display,
                manager.version(),
                WorkspaceUserData { id },
            ) else {
                continue;
            };
            manager.workspace(&ws);
            ws.name(data.name.clone());
            ws.coordinates(coords_bytes(data.index));
            ws.state(workspace_state(data.active));
            ws.capabilities(ext_workspace_handle_v1::WorkspaceCapabilities::Activate);
            group.workspace_enter(&ws);
            data.handles.push(ws);
        }
        manager.done();
    }
}

fn coords_bytes(index: u32) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(8);
    bytes.extend_from_slice(&0u32.to_ne_bytes());
    bytes.extend_from_slice(&index.to_ne_bytes());
    bytes
}

fn workspace_state(active: bool) -> ext_workspace_handle_v1::State {
    if active {
        ext_workspace_handle_v1::State::Active
    } else {
        ext_workspace_handle_v1::State::empty()
    }
}

impl<D> GlobalDispatch2<ExtWorkspaceManagerV1, D> for ExtWorkspaceGlobalData
where
    D: Dispatch<ExtWorkspaceManagerV1, ManagerUserData>,
    D: Dispatch<ExtWorkspaceHandleV1, WorkspaceUserData>,
    D: Dispatch<ExtWorkspaceGroupHandleV1, GroupUserData>,
    D: ExtWorkspaceHandler,
    D: 'static,
{
    fn bind(
        &self,
        state: &mut D,
        _handle: &DisplayHandle,
        client: &Client,
        resource: New<ExtWorkspaceManagerV1>,
        data_init: &mut DataInit<'_, D>,
    ) {
        let manager = data_init.init(resource, ManagerUserData);
        let snapshot = state.workspace_snapshot();
        let protocol = state.ext_workspace_manager_state();
        protocol.pending.insert(manager.clone(), Vec::new());
        protocol.managers.insert(manager.clone());
        protocol.ensure_initial_objects::<D>(client, &manager, &snapshot);
    }

    fn can_view(&self, client: &Client) -> bool {
        (self.filter)(client)
    }
}

impl<D> Dispatch2<ExtWorkspaceManagerV1, D> for ManagerUserData
where
    D: ExtWorkspaceHandler,
    D: 'static,
{
    fn request(
        &self,
        state: &mut D,
        _client: &Client,
        resource: &ExtWorkspaceManagerV1,
        request: <ExtWorkspaceManagerV1 as Resource>::Request,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            ext_workspace_manager_v1::Request::Commit => {
                let actions = state
                    .ext_workspace_manager_state()
                    .pending
                    .get_mut(resource)
                    .map(std::mem::take)
                    .unwrap_or_default();
                for id in actions {
                    state.activate_workspace_id(id);
                }
            }
            ext_workspace_manager_v1::Request::Stop => {
                resource.finished();
                let protocol = state.ext_workspace_manager_state();
                protocol.managers.remove(resource);
                protocol.pending.remove(resource);
            }
            _ => {}
        }
    }

    fn destroyed(&self, state: &mut D, _client: ClientId, resource: &ExtWorkspaceManagerV1) {
        let protocol = state.ext_workspace_manager_state();
        protocol.managers.remove(resource);
        protocol.pending.remove(resource);
    }
}

impl<D> Dispatch2<ExtWorkspaceHandleV1, D> for WorkspaceUserData
where
    D: ExtWorkspaceHandler,
    D: 'static,
{
    fn request(
        &self,
        state: &mut D,
        _client: &Client,
        _resource: &ExtWorkspaceHandleV1,
        request: <ExtWorkspaceHandleV1 as Resource>::Request,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        if !matches!(request, ext_workspace_handle_v1::Request::Activate) {
            return;
        }
        let id = self.id;
        let protocol = state.ext_workspace_manager_state();
        let managers: Vec<_> = protocol.managers.iter().cloned().collect();
        for manager in managers {
            protocol.pending.entry(manager).or_default().push(id);
        }
    }

    fn destroyed(&self, state: &mut D, _client: ClientId, resource: &ExtWorkspaceHandleV1) {
        let protocol = state.ext_workspace_manager_state();
        for data in protocol.workspaces.values_mut() {
            data.handles.retain(|h| h != resource);
        }
    }
}

impl<D> Dispatch2<ExtWorkspaceGroupHandleV1, D> for GroupUserData
where
    D: ExtWorkspaceHandler,
    D: 'static,
{
    fn request(
        &self,
        _state: &mut D,
        _client: &Client,
        _resource: &ExtWorkspaceGroupHandleV1,
        request: <ExtWorkspaceGroupHandleV1 as Resource>::Request,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        let _ = request;
    }

    fn destroyed(&self, state: &mut D, _client: ClientId, resource: &ExtWorkspaceGroupHandleV1) {
        state
            .ext_workspace_manager_state()
            .groups
            .retain(|g| g != resource);
    }
}
