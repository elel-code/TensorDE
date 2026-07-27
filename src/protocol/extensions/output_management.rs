//! `zwlr_output_management_v1` (community tier-4) protocol adapter.
//!
//! Performance: only mutates value-only [`OutputRule`] tables and emits
//! topology events. No modeset / buffer reallocation on the page-flip path;
//! apply runs from protocol dispatch then replan (same as IPC).
//!
//! Supported: enable/disable, position, scale. Mode switching deferred until
//! live KMS target replacement is safe.

use std::collections::HashMap;

use tracing::{debug, warn};
use wayland_protocols_wlr::output_management::v1::server::{
    zwlr_output_configuration_head_v1::{self, ZwlrOutputConfigurationHeadV1},
    zwlr_output_configuration_v1::{self, ZwlrOutputConfigurationV1},
    zwlr_output_head_v1::ZwlrOutputHeadV1,
    zwlr_output_manager_v1::{self, ZwlrOutputManagerV1},
    zwlr_output_mode_v1::ZwlrOutputModeV1,
};
use wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource, backend::ClientId,
    protocol::wl_output::Transform as WlTransform,
};

pub use tensor_protocol::{OutputHeadSnapshot as HeadSnapshot, OutputHeadUpdate};

use crate::protocol::dispatch::{
    DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
};
use crate::protocol::state::RuntimeState;

const VERSION: u32 = 2;

#[derive(Debug, Default)]
pub struct ManagerUserData;

#[derive(Clone, Debug)]
pub struct HeadUserData {
    pub name: String,
}

#[derive(Debug)]
pub struct ConfigurationUserData {
    pub serial: u32,
}

#[derive(Debug)]
pub struct ConfigurationHeadUserData {
    pub name: String,
}

#[derive(Debug, Default)]
pub struct ModeUserData;

pub struct OutputManagementState {
    display: DisplayHandle,
    serial: u32,
    /// Last advertised heads: name → geometry snapshot.
    heads: HashMap<String, HeadSnapshot>,
    managers: HashMap<ClientId, ZwlrOutputManagerV1>,
    /// Per configuration resource: head name → pending.
    pending: HashMap<ZwlrOutputConfigurationV1, HashMap<String, OutputHeadUpdate>>,
    /// Live head objects per client.
    client_heads: HashMap<ClientId, HashMap<String, ZwlrOutputHeadV1>>,
}

pub struct OutputManagementGlobalData {
    filter: Box<dyn for<'c> Fn(&'c Client) -> bool + Send + Sync>,
}

/// Partial head mutation from a configuration object (only set fields apply).
pub trait OutputManagementHandler: 'static {
    fn output_management_state(&mut self) -> &mut OutputManagementState;
    /// Apply one complete configuration as a single topology transaction.
    fn apply_output_configuration(
        &mut self,
        updates: Vec<(String, OutputHeadUpdate)>,
    ) -> Result<(), String>;
    fn current_output_heads(&self) -> Vec<HeadSnapshot>;
}

impl OutputManagementState {
    pub fn new<D, F>(display: &DisplayHandle, filter: F) -> Self
    where
        D: GlobalDispatch<ZwlrOutputManagerV1, OutputManagementGlobalData>,
        D: Dispatch<ZwlrOutputManagerV1, ManagerUserData>,
        D: OutputManagementHandler,
        D: 'static,
        F: for<'c> Fn(&'c Client) -> bool + Send + Sync + 'static,
    {
        display.create_global::<D, ZwlrOutputManagerV1, _>(
            VERSION,
            OutputManagementGlobalData {
                filter: Box::new(filter),
            },
        );
        Self {
            display: display.clone(),
            serial: 1,
            heads: HashMap::new(),
            managers: HashMap::new(),
            pending: HashMap::new(),
            client_heads: HashMap::new(),
        }
    }

    /// Push current topology to all bound managers (call after output events).
    pub fn notify_heads<D>(&mut self, heads: Vec<HeadSnapshot>)
    where
        D: Dispatch<ZwlrOutputHeadV1, HeadUserData>,
        D: Dispatch<ZwlrOutputModeV1, ModeUserData>,
        D: 'static,
    {
        let mut map = HashMap::new();
        for head in heads {
            map.insert(head.name.clone(), head);
        }
        if map == self.heads {
            return;
        }
        self.heads = map;
        self.serial = self.serial.wrapping_add(1).max(1);
        // Full re-advertise is O(heads × clients) but only at topology rate.
        for (client_id, manager) in self.managers.clone() {
            if let Ok(client) = self.display.get_client(manager.id()) {
                self.advertise_to_client::<D>(&client, client_id, &manager);
            }
        }
        debug!(
            serial = self.serial,
            heads = self.heads.len(),
            "output-management notified"
        );
    }

    fn advertise_to_client<D>(
        &mut self,
        client: &Client,
        client_id: ClientId,
        manager: &ZwlrOutputManagerV1,
    ) where
        D: Dispatch<ZwlrOutputHeadV1, HeadUserData>,
        D: Dispatch<ZwlrOutputModeV1, ModeUserData>,
        D: 'static,
    {
        if let Some(old) = self.client_heads.remove(&client_id) {
            for head in old.values() {
                head.finished();
            }
        }
        let mut heads = HashMap::new();
        for snap in self.heads.values() {
            let Ok(head) = client.create_resource::<ZwlrOutputHeadV1, _, D>(
                &self.display,
                manager.version(),
                HeadUserData {
                    name: snap.name.clone(),
                },
            ) else {
                continue;
            };
            manager.head(&head);
            head.name(snap.name.clone());
            head.description(snap.name.clone());
            head.physical_size(0, 0);
            head.enabled(i32::from(snap.enabled));
            head.position(snap.x, snap.y);
            head.transform(WlTransform::Normal);
            head.scale(snap.scale.as_f64());
            if let Ok(mode) = client.create_resource::<ZwlrOutputModeV1, _, D>(
                &self.display,
                manager.version(),
                ModeUserData,
            ) {
                head.mode(&mode);
                mode.size(snap.mode_width, snap.mode_height);
                mode.refresh(snap.refresh_millihertz);
                mode.preferred();
                head.current_mode(&mode);
            }
            heads.insert(snap.name.clone(), head);
        }
        self.client_heads.insert(client_id, heads);
        manager.done(self.serial);
    }
}

impl<D> GlobalDispatchDelegate<ZwlrOutputManagerV1, D> for OutputManagementGlobalData
where
    D: Dispatch<ZwlrOutputManagerV1, ManagerUserData>,
    D: Dispatch<ZwlrOutputHeadV1, HeadUserData>,
    D: Dispatch<ZwlrOutputModeV1, ModeUserData>,
    D: OutputManagementHandler,
    D: 'static,
{
    fn bind(
        &self,
        state: &mut D,
        _handle: &DisplayHandle,
        client: &Client,
        resource: New<ZwlrOutputManagerV1>,
        data_init: &mut DataInit<'_, D>,
    ) {
        let manager = data_init.init(resource, ManagerUserData);
        let heads = state.current_output_heads();
        let protocol = state.output_management_state();
        protocol.heads = heads.into_iter().map(|h| (h.name.clone(), h)).collect();
        protocol.managers.insert(client.id(), manager.clone());
        protocol.advertise_to_client::<D>(client, client.id(), &manager);
    }

    fn can_view(&self, client: &Client) -> bool {
        (self.filter)(client)
    }
}

impl<D> DispatchDelegate<ZwlrOutputManagerV1, D> for ManagerUserData
where
    D: Dispatch<ZwlrOutputConfigurationV1, ConfigurationUserData>,
    D: OutputManagementHandler,
    D: 'static,
{
    fn request(
        &self,
        state: &mut D,
        _client: &Client,
        resource: &ZwlrOutputManagerV1,
        request: <ZwlrOutputManagerV1 as Resource>::Request,
        _dhandle: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            zwlr_output_manager_v1::Request::CreateConfiguration { id, serial } => {
                let protocol = state.output_management_state();
                let conf = data_init.init(id, ConfigurationUserData { serial });
                if serial != protocol.serial {
                    conf.cancelled();
                    return;
                }
                protocol.pending.insert(conf, HashMap::new());
            }
            zwlr_output_manager_v1::Request::Stop => {
                resource.finished();
                if let Ok(client) = resource.client().ok_or(()) {
                    state
                        .output_management_state()
                        .managers
                        .remove(&client.id());
                }
            }
            _ => {}
        }
    }

    fn destroyed(&self, state: &mut D, client: ClientId, _resource: &ZwlrOutputManagerV1) {
        let protocol = state.output_management_state();
        protocol.managers.remove(&client);
        protocol.client_heads.remove(&client);
    }
}

impl<D> DispatchDelegate<ZwlrOutputConfigurationV1, D> for ConfigurationUserData
where
    D: Dispatch<ZwlrOutputConfigurationHeadV1, ConfigurationHeadUserData>,
    D: OutputManagementHandler,
    D: 'static,
{
    fn request(
        &self,
        state: &mut D,
        _client: &Client,
        resource: &ZwlrOutputConfigurationV1,
        request: <ZwlrOutputConfigurationV1 as Resource>::Request,
        _dhandle: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        // Serial check and pending mutations need only a short exclusive borrow.
        let serial = state.output_management_state().serial;
        if self.serial != serial {
            match request {
                zwlr_output_configuration_v1::Request::Apply
                | zwlr_output_configuration_v1::Request::Test => {
                    resource.cancelled();
                }
                _ => {}
            }
            return;
        }
        match request {
            zwlr_output_configuration_v1::Request::EnableHead { id, head } => {
                let Some(data) = head.data::<HeadUserData>() else {
                    return;
                };
                let name = data.name.clone();
                let conf_head =
                    data_init.init(id, ConfigurationHeadUserData { name: name.clone() });
                let _ = conf_head;
                state
                    .output_management_state()
                    .pending
                    .entry(resource.clone())
                    .or_default()
                    .entry(name)
                    .or_default()
                    .enabled = Some(true);
            }
            zwlr_output_configuration_v1::Request::DisableHead { head } => {
                let Some(data) = head.data::<HeadUserData>() else {
                    return;
                };
                state
                    .output_management_state()
                    .pending
                    .entry(resource.clone())
                    .or_default()
                    .entry(data.name.clone())
                    .or_default()
                    .enabled = Some(false);
            }
            zwlr_output_configuration_v1::Request::Apply => {
                let Some(pending) = state.output_management_state().pending.remove(resource) else {
                    resource.failed();
                    return;
                };
                let keeps_head = {
                    let protocol = state.output_management_state();
                    tensor_protocol::configuration_keeps_head_enabled(&protocol.heads, &pending)
                };
                if !keeps_head {
                    resource.failed();
                    return;
                }
                match state.apply_output_configuration(pending.into_iter().collect()) {
                    Ok(()) => resource.succeeded(),
                    Err(error) => {
                        warn!(%error, "output-management apply failed");
                        resource.failed();
                    }
                }
            }
            zwlr_output_configuration_v1::Request::Test => {
                let protocol = state.output_management_state();
                let any_on = protocol.pending.get(resource).is_some_and(|pending| {
                    tensor_protocol::configuration_keeps_head_enabled(&protocol.heads, pending)
                });
                if any_on {
                    resource.succeeded();
                } else {
                    resource.failed();
                }
            }
            zwlr_output_configuration_v1::Request::Destroy => {
                state.output_management_state().pending.remove(resource);
            }
            _ => {}
        }
    }

    fn destroyed(&self, state: &mut D, _client: ClientId, resource: &ZwlrOutputConfigurationV1) {
        state.output_management_state().pending.remove(resource);
    }
}

impl<D> DispatchDelegate<ZwlrOutputConfigurationHeadV1, D> for ConfigurationHeadUserData
where
    D: OutputManagementHandler,
    D: 'static,
{
    fn request(
        &self,
        state: &mut D,
        _client: &Client,
        resource: &ZwlrOutputConfigurationHeadV1,
        request: <ZwlrOutputConfigurationHeadV1 as Resource>::Request,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        // Find owning configuration via pending scan (small map at topology rate).
        let protocol = state.output_management_state();
        let Some((conf, pending)) = protocol
            .pending
            .iter_mut()
            .find(|(_, heads)| heads.contains_key(&self.name))
        else {
            return;
        };
        match request {
            zwlr_output_configuration_head_v1::Request::SetMode { mode: _ } => {
                // Mode switch not yet supported without live KMS replacement.
                let _ = conf;
                resource.post_error(
                    zwlr_output_configuration_head_v1::Error::InvalidMode,
                    "mode switching is not yet supported",
                );
            }
            zwlr_output_configuration_head_v1::Request::SetCustomMode { .. } => {
                let _ = conf;
                resource.post_error(
                    zwlr_output_configuration_head_v1::Error::InvalidCustomMode,
                    "custom modes are not supported",
                );
            }
            zwlr_output_configuration_head_v1::Request::SetPosition { x, y } => {
                pending.entry(self.name.clone()).or_default().position = Some((x, y));
            }
            zwlr_output_configuration_head_v1::Request::SetTransform { transform } => {
                let _ = transform;
                // Transform not wired; ignore rather than fail the whole conf.
            }
            zwlr_output_configuration_head_v1::Request::SetScale { scale } => {
                let Some(scale) = tensor_util::OutputScale::from_f64(scale) else {
                    resource.post_error(
                        zwlr_output_configuration_head_v1::Error::InvalidScale,
                        "scale is outside the supported range",
                    );
                    return;
                };
                pending.entry(self.name.clone()).or_default().scale = Some(scale);
            }
            zwlr_output_configuration_head_v1::Request::SetAdaptiveSync { state: sync } => {
                let _ = sync;
            }
            _ => {}
        }
        let _ = resource;
    }
}

impl<D> DispatchDelegate<ZwlrOutputHeadV1, D> for HeadUserData
where
    D: 'static,
{
    fn request(
        &self,
        _state: &mut D,
        _client: &Client,
        _resource: &ZwlrOutputHeadV1,
        request: <ZwlrOutputHeadV1 as Resource>::Request,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        let _ = request;
    }
}

impl<D> DispatchDelegate<ZwlrOutputModeV1, D> for ModeUserData
where
    D: 'static,
{
    fn request(
        &self,
        _state: &mut D,
        _client: &Client,
        _resource: &ZwlrOutputModeV1,
        request: <ZwlrOutputModeV1 as Resource>::Request,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        let _ = request;
    }
}

delegate_global_dispatch!(
    RuntimeState,
    ZwlrOutputManagerV1,
    OutputManagementGlobalData
);
delegate_dispatch!(RuntimeState, ZwlrOutputManagerV1, ManagerUserData);
delegate_dispatch!(
    RuntimeState,
    ZwlrOutputConfigurationV1,
    ConfigurationUserData
);
delegate_dispatch!(
    RuntimeState,
    ZwlrOutputConfigurationHeadV1,
    ConfigurationHeadUserData
);
delegate_dispatch!(RuntimeState, ZwlrOutputHeadV1, HeadUserData);
delegate_dispatch!(RuntimeState, ZwlrOutputModeV1, ModeUserData);
