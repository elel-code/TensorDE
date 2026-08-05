//! Completion-gated direct `wp_drm_lease_v1` owner.

use std::{
    os::fd::{AsFd, OwnedFd},
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use tensor_drm::{LeaseConnector, LeaseError, LeaseRevocation, LeaseToken, MAX_LEASE_CONNECTORS};
use tensor_host::ConnectorId;
use wayland_protocols::wp::drm_lease::v1::server::{
    wp_drm_lease_connector_v1::{self, WpDrmLeaseConnectorV1},
    wp_drm_lease_device_v1::{self, WpDrmLeaseDeviceV1},
    wp_drm_lease_request_v1::{self, WpDrmLeaseRequestV1},
    wp_drm_lease_v1::{self, WpDrmLeaseV1},
};
use wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource, Weak,
    backend::{ClientId, GlobalId},
};

use crate::{
    backend::{DrmLeaseDeviceSnapshot, DrmLeaseError},
    protocol::{
        dispatch::{
            DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
        },
        state::{RuntimeState, WaylandClientState},
    },
};

const VERSION: u32 = 1;

#[derive(Debug)]
struct OfferedConnector {
    descriptor: LeaseConnector,
    resources: Vec<Weak<WpDrmLeaseConnectorV1>>,
}

#[derive(Debug)]
struct ActiveLeaseResource {
    token: LeaseToken,
    resource: Weak<WpDrmLeaseV1>,
}

#[derive(Debug)]
struct InstalledDevice {
    device_id: u64,
    path: PathBuf,
    global: GlobalId,
    resources: Vec<Weak<WpDrmLeaseDeviceV1>>,
    connectors: Vec<OfferedConnector>,
    leases: Vec<ActiveLeaseResource>,
    test_fd: bool,
}

#[derive(Debug, Default)]
pub(crate) struct DrmLeaseProtocol {
    device: Option<InstalledDevice>,
}

impl DrmLeaseProtocol {
    pub(crate) fn update(
        &mut self,
        display: &DisplayHandle,
        snapshot: Option<DrmLeaseDeviceSnapshot>,
        revocations: Vec<LeaseRevocation>,
    ) {
        self.finish_revocations(&revocations);
        let Some(snapshot) = snapshot else {
            self.close_device(display);
            return;
        };
        let same_device = self.device.as_ref().is_some_and(|device| {
            device.device_id == snapshot.device_id && device.path == snapshot.path
        });
        if !same_device {
            self.close_device(display);
            if open_non_master_fd(&snapshot.path).is_err() {
                return;
            }
            let data = DrmLeaseGlobalData {
                device_id: snapshot.device_id,
                path: snapshot.path.clone(),
            };
            let global =
                display.create_global::<RuntimeState, WpDrmLeaseDeviceV1, _>(VERSION, data);
            self.device = Some(InstalledDevice {
                device_id: snapshot.device_id,
                path: snapshot.path.clone(),
                global,
                resources: Vec::new(),
                connectors: Vec::with_capacity(MAX_LEASE_CONNECTORS),
                leases: Vec::with_capacity(tensor_drm::MAX_ACTIVE_LEASES),
                test_fd: false,
            });
        }
        self.reconcile_connectors(display, snapshot.connectors);
    }

    pub(crate) fn advertised(&self) -> bool {
        self.device.is_some()
    }

    #[cfg(test)]
    fn install_for_test(
        &mut self,
        display: &DisplayHandle,
        device_id: u64,
        connectors: Vec<LeaseConnector>,
    ) {
        self.close_device(display);
        let path = PathBuf::from("/dev/null");
        let global = display.create_global::<RuntimeState, WpDrmLeaseDeviceV1, _>(
            VERSION,
            DrmLeaseGlobalData {
                device_id,
                path: path.clone(),
            },
        );
        self.device = Some(InstalledDevice {
            device_id,
            path,
            global,
            resources: Vec::new(),
            connectors: connectors
                .into_iter()
                .map(|descriptor| OfferedConnector {
                    descriptor,
                    resources: Vec::new(),
                })
                .collect(),
            leases: Vec::with_capacity(tensor_drm::MAX_ACTIVE_LEASES),
            test_fd: true,
        });
    }

    fn bind(
        &mut self,
        display: &DisplayHandle,
        client: &Client,
        resource: New<WpDrmLeaseDeviceV1>,
        global: &DrmLeaseGlobalData,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        let device_resource = data_init.init(
            resource,
            DrmLeaseDeviceData {
                device_id: global.device_id,
            },
        );
        let Some(device) = self
            .device
            .as_mut()
            .filter(|device| device.device_id == global.device_id && device.path == global.path)
        else {
            device_resource.released();
            return;
        };
        let Ok(fd) = open_bound_fd(&device.path, device.test_fd) else {
            device_resource.released();
            return;
        };
        device_resource.drm_fd(fd.as_fd());
        for connector in &mut device.connectors {
            send_connector(display, client, &device_resource, connector);
        }
        device_resource.done();
        device.resources.push(device_resource.downgrade());
    }

    fn reconcile_connectors(&mut self, display: &DisplayHandle, connectors: Vec<LeaseConnector>) {
        let Some(device) = self.device.as_mut() else {
            return;
        };
        let mut changed = false;
        device.connectors.retain_mut(|offered| {
            if connectors.contains(&offered.descriptor) {
                return true;
            }
            for resource in offered.resources.drain(..) {
                if let Ok(resource) = resource.upgrade() {
                    resource.withdrawn();
                }
            }
            changed = true;
            false
        });
        for descriptor in connectors {
            if device
                .connectors
                .iter()
                .any(|offered| offered.descriptor == descriptor)
            {
                continue;
            }
            let mut offered = OfferedConnector {
                descriptor,
                resources: Vec::new(),
            };
            device
                .resources
                .retain(|resource| resource.upgrade().is_ok());
            for resource in &device.resources {
                let Ok(device_resource) = resource.upgrade() else {
                    continue;
                };
                let Some(client) = device_resource.client() else {
                    continue;
                };
                send_connector(display, &client, &device_resource, &mut offered);
            }
            device.connectors.push(offered);
            changed = true;
        }
        if changed {
            for resource in &device.resources {
                if let Ok(resource) = resource.upgrade() {
                    resource.done();
                }
            }
        }
    }

    fn register_lease(&mut self, token: LeaseToken, resource: &WpDrmLeaseV1) {
        if let Some(device) = self.device.as_mut() {
            device
                .leases
                .retain(|lease| lease.resource.upgrade().is_ok());
            device.leases.push(ActiveLeaseResource {
                token,
                resource: resource.downgrade(),
            });
        }
    }

    fn forget_lease(&mut self, token: LeaseToken) {
        if let Some(device) = self.device.as_mut() {
            device.leases.retain(|lease| lease.token != token);
        }
    }

    fn finish_revocations(&mut self, revocations: &[LeaseRevocation]) {
        let Some(device) = self.device.as_mut() else {
            return;
        };
        device.leases.retain(|lease| {
            if !revocations
                .iter()
                .any(|revocation| revocation.token == lease.token)
            {
                return lease.resource.upgrade().is_ok();
            }
            if let Ok(resource) = lease.resource.upgrade()
                && let Some(data) = resource.data::<DrmLeaseData>()
            {
                data.token.lock().unwrap().take();
                if !data.finished.swap(true, Ordering::AcqRel) {
                    resource.finished();
                }
            }
            false
        });
    }

    fn close_device(&mut self, display: &DisplayHandle) {
        let Some(mut device) = self.device.take() else {
            return;
        };
        for connector in &mut device.connectors {
            for resource in connector.resources.drain(..) {
                if let Ok(resource) = resource.upgrade() {
                    resource.withdrawn();
                }
            }
        }
        for lease in device.leases.drain(..) {
            if let Ok(resource) = lease.resource.upgrade()
                && let Some(data) = resource.data::<DrmLeaseData>()
            {
                data.token.lock().unwrap().take();
                if !data.finished.swap(true, Ordering::AcqRel) {
                    resource.finished();
                }
            }
        }
        for resource in device.resources.drain(..) {
            if let Ok(resource) = resource.upgrade() {
                resource.released();
            }
        }
        display.disable_global::<RuntimeState>(device.global.clone());
        display.remove_global::<RuntimeState>(device.global);
    }
}

fn send_connector(
    display: &DisplayHandle,
    client: &Client,
    device_resource: &WpDrmLeaseDeviceV1,
    offered: &mut OfferedConnector,
) {
    let Ok(resource) = client.create_resource::<WpDrmLeaseConnectorV1, _, RuntimeState>(
        display,
        VERSION,
        DrmLeaseConnectorData {
            device_id: offered.descriptor.id.device_id,
            connector: offered.descriptor.id,
        },
    ) else {
        return;
    };
    device_resource.connector(&resource);
    resource.name(offered.descriptor.name.clone());
    resource.description(offered.descriptor.description.clone());
    resource.connector_id(offered.descriptor.id.connector_id);
    resource.done();
    offered.resources.push(resource.downgrade());
}

fn open_non_master_fd(path: &Path) -> Result<OwnedFd, String> {
    let fd = rustix::fs::open(
        path,
        rustix::fs::OFlags::RDWR | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .map_err(|error| format!("failed to open DRM lease node {}: {error}", path.display()))?;
    let client = drm_ffi::get_client(fd.as_fd(), 0)
        .map_err(|error| format!("failed to inspect DRM lease fd: {error}"))?;
    if client.auth == 1 {
        drm_ffi::auth::release_master(fd.as_fd())
            .map_err(|error| format!("failed to drop DRM master on lease fd: {error}"))?;
    }
    Ok(fd)
}

fn open_bound_fd(path: &Path, test_fd: bool) -> Result<OwnedFd, String> {
    #[cfg(test)]
    if test_fd {
        return rustix::fs::open(
            path,
            rustix::fs::OFlags::RDWR | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )
        .map_err(|error| format!("failed to open test lease fd: {error}"));
    }
    let _ = test_fd;
    open_non_master_fd(path)
}

#[derive(Clone, Debug)]
pub(in crate::protocol) struct DrmLeaseGlobalData {
    device_id: u64,
    path: PathBuf,
}

#[derive(Debug)]
pub(in crate::protocol) struct DrmLeaseDeviceData {
    device_id: u64,
}

#[derive(Debug)]
pub(in crate::protocol) struct DrmLeaseConnectorData {
    device_id: u64,
    connector: ConnectorId,
}

#[derive(Debug)]
pub(in crate::protocol) struct DrmLeaseRequestData {
    device_id: u64,
    connectors: Mutex<Vec<ConnectorId>>,
    overflowed: AtomicBool,
}

#[derive(Debug)]
pub(in crate::protocol) struct DrmLeaseData {
    token: Mutex<Option<LeaseToken>>,
    finished: AtomicBool,
}

impl GlobalDispatchDelegate<WpDrmLeaseDeviceV1, RuntimeState> for DrmLeaseGlobalData {
    fn bind(
        &self,
        state: &mut RuntimeState,
        display: &DisplayHandle,
        client: &Client,
        resource: New<WpDrmLeaseDeviceV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        state
            .protocol_globals
            .drm_lease
            .bind(display, client, resource, self, data_init);
    }

    fn can_view(&self, client: &Client) -> bool {
        client
            .get_data::<WaylandClientState>()
            .is_none_or(|data| data.security_context.is_none())
    }
}

impl DispatchDelegate<WpDrmLeaseDeviceV1, RuntimeState> for DrmLeaseDeviceData {
    fn request(
        &self,
        _state: &mut RuntimeState,
        _client: &Client,
        resource: &WpDrmLeaseDeviceV1,
        request: wp_drm_lease_device_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wp_drm_lease_device_v1::Request::CreateLeaseRequest { id } => {
                data_init.init(
                    id,
                    DrmLeaseRequestData {
                        device_id: self.device_id,
                        connectors: Mutex::new(Vec::with_capacity(MAX_LEASE_CONNECTORS)),
                        overflowed: AtomicBool::new(false),
                    },
                );
            }
            wp_drm_lease_device_v1::Request::Release => resource.released(),
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<WpDrmLeaseConnectorV1, RuntimeState> for DrmLeaseConnectorData {
    fn request(
        &self,
        _state: &mut RuntimeState,
        _client: &Client,
        _resource: &WpDrmLeaseConnectorV1,
        request: wp_drm_lease_connector_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wp_drm_lease_connector_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<WpDrmLeaseRequestV1, RuntimeState> for DrmLeaseRequestData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        resource: &WpDrmLeaseRequestV1,
        request: wp_drm_lease_request_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wp_drm_lease_request_v1::Request::RequestConnector { connector } => {
                let Some(connector_data) = connector.data::<DrmLeaseConnectorData>() else {
                    resource.post_error(
                        wp_drm_lease_request_v1::Error::WrongDevice,
                        "connector has no Tensorland DRM lease identity".to_owned(),
                    );
                    return;
                };
                if connector_data.device_id != self.device_id {
                    resource.post_error(
                        wp_drm_lease_request_v1::Error::WrongDevice,
                        "connector belongs to a different DRM lease device".to_owned(),
                    );
                    return;
                }
                let mut connectors = self.connectors.lock().unwrap();
                if connectors.contains(&connector_data.connector) {
                    resource.post_error(
                        wp_drm_lease_request_v1::Error::DuplicateConnector,
                        "connector was requested more than once".to_owned(),
                    );
                    return;
                }
                if connectors.len() == MAX_LEASE_CONNECTORS {
                    self.overflowed.store(true, Ordering::Release);
                    return;
                }
                connectors.push(connector_data.connector);
            }
            wp_drm_lease_request_v1::Request::Submit { id } => {
                let connectors = self.connectors.lock().unwrap();
                if connectors.is_empty() && !self.overflowed.load(Ordering::Acquire) {
                    resource.post_error(
                        wp_drm_lease_request_v1::Error::EmptyLease,
                        "DRM lease request contains no connectors".to_owned(),
                    );
                    return;
                }
                let lease = data_init.init(
                    id,
                    DrmLeaseData {
                        token: Mutex::new(None),
                        finished: AtomicBool::new(false),
                    },
                );
                let created = (!self.overflowed.load(Ordering::Acquire))
                    .then(|| {
                        state
                            .backend
                            .as_mut()
                            .ok_or(DrmLeaseError::DeviceUnavailable)?
                            .create_drm_lease(&connectors)
                    })
                    .transpose()
                    .and_then(|created| {
                        created.ok_or(DrmLeaseError::Policy(
                            LeaseError::TooManyRequestedConnectors {
                                count: connectors.len(),
                                max: tensor_drm::MAX_CONNECTORS_PER_LEASE,
                            },
                        ))
                    });
                match created {
                    Ok(created) => {
                        lease
                            .data::<DrmLeaseData>()
                            .expect("lease was initialized with Tensor data")
                            .token
                            .lock()
                            .unwrap()
                            .replace(created.token);
                        lease.lease_fd(created.fd.as_fd());
                        state
                            .protocol_globals
                            .drm_lease
                            .register_lease(created.token, &lease);
                        state.refresh_drm_lease_device();
                    }
                    Err(error) => {
                        tracing::warn!(%error, "DRM lease request rejected");
                        lease
                            .data::<DrmLeaseData>()
                            .expect("lease was initialized with Tensor data")
                            .finished
                            .store(true, Ordering::Release);
                        lease.finished();
                    }
                }
            }
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<WpDrmLeaseV1, RuntimeState> for DrmLeaseData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        _resource: &WpDrmLeaseV1,
        request: wp_drm_lease_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wp_drm_lease_v1::Request::Destroy => revoke_resource_lease(state, self),
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, state: &mut RuntimeState, _client: ClientId, _resource: &WpDrmLeaseV1) {
        revoke_resource_lease(state, self);
    }
}

fn revoke_resource_lease(state: &mut RuntimeState, data: &DrmLeaseData) {
    let Some(token) = data.token.lock().unwrap().take() else {
        return;
    };
    if let Some(backend) = state.backend.as_mut()
        && let Err(error) = backend.revoke_drm_lease(token)
    {
        tracing::warn!(%error, "failed to revoke destroyed DRM lease");
    }
    state.protocol_globals.drm_lease.forget_lease(token);
    state.refresh_drm_lease_device();
}

impl RuntimeState {
    pub(crate) fn refresh_drm_lease_device(&mut self) {
        let (device, revocations) = match self.backend.as_mut() {
            Some(backend) => {
                let revocations = backend.take_drm_lease_revocations();
                (backend.drm_lease_device(), revocations)
            }
            None => (None, Vec::new()),
        };
        self.protocol_globals
            .update_drm_lease(&self.display_handle, device, revocations);
    }
}

delegate_global_dispatch!(RuntimeState, WpDrmLeaseDeviceV1, DrmLeaseGlobalData);
delegate_dispatch!(RuntimeState, WpDrmLeaseDeviceV1, DrmLeaseDeviceData);
delegate_dispatch!(RuntimeState, WpDrmLeaseConnectorV1, DrmLeaseConnectorData);
delegate_dispatch!(RuntimeState, WpDrmLeaseRequestV1, DrmLeaseRequestData);
delegate_dispatch!(RuntimeState, WpDrmLeaseV1, DrmLeaseData);

#[cfg(test)]
mod tests;
