//! DRM lease catalog and real kernel lease lifecycle.
//!
//! The shared `tensor-drm` crate owns bounded value policy. This module owns
//! compositor-thread DRM ioctls and translates the selected primary device's
//! non-desktop topology into that policy.

use std::{num::NonZeroU32, os::fd::OwnedFd, path::PathBuf};

use drm::control::{Device as ControlDevice, RawResourceHandle, crtc};
use rustix::fs::OFlags;
use tensor_drm::{KernelLeaseId, LeaseConnector, LeaseError, LeaseRevocation, LeaseToken};
use tensor_host::{ConnectorId, ConnectorState};
use thiserror::Error;
use tracing::warn;

use super::{BackendError, OpenDevice, TtyBackend, kms, node_path};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DrmLeaseDeviceSnapshot {
    pub(crate) device_id: u64,
    pub(crate) path: PathBuf,
    pub(crate) connectors: Vec<LeaseConnector>,
}

#[derive(Debug)]
pub(crate) struct CreatedDrmLease {
    pub(crate) token: LeaseToken,
    pub(crate) fd: OwnedFd,
}

#[derive(Debug, Error)]
pub(crate) enum DrmLeaseError {
    #[error(transparent)]
    Policy(#[from] LeaseError),
    #[error("authoritative DRM lease device is unavailable")]
    DeviceUnavailable,
    #[error("failed to create DRM lease: {0}")]
    Create(std::io::Error),
    #[error("failed to revoke DRM lease: {0}")]
    Revoke(std::io::Error),
}

impl TtyBackend {
    pub(crate) fn drm_lease_device(&self) -> Option<DrmLeaseDeviceSnapshot> {
        if !self.session.is_active() {
            return None;
        }
        let device_id = self.primary_node.dev_id();
        let device = self
            .devices
            .get(&device_id)
            .filter(|device| device.drm.is_active())?;
        let lease_capable = device.lease_registry.catalog().next().is_some();
        let connectors = device
            .lease_registry
            .available()
            .cloned()
            .collect::<Vec<_>>();
        lease_capable.then(|| DrmLeaseDeviceSnapshot {
            device_id,
            path: node_path(self.primary_node),
            connectors,
        })
    }

    pub(crate) fn create_drm_lease(
        &mut self,
        requested: &[ConnectorId],
    ) -> Result<CreatedDrmLease, DrmLeaseError> {
        let device_id = self.primary_node.dev_id();
        let device = self
            .devices
            .get_mut(&device_id)
            .filter(|device| device.drm.is_active())
            .ok_or(DrmLeaseError::DeviceUnavailable)?;
        let reservation = device.lease_registry.reserve(device_id, requested)?;
        let mut objects = Vec::with_capacity(reservation.connectors.len() * 3);
        for connector in &reservation.connectors {
            objects.push(raw_handle(connector.id.connector_id));
            objects.push(raw_handle(connector.crtc_id));
            objects.push(raw_handle(connector.primary_plane_id));
        }
        let (kernel_id, fd) = match device.drm.create_lease(&objects, OFlags::CLOEXEC.bits()) {
            Ok(created) => created,
            Err(error) => {
                let _ = device.lease_registry.revoke(reservation.token);
                return Err(DrmLeaseError::Create(error));
            }
        };
        if let Err(error) = device
            .lease_registry
            .activate(reservation.token, KernelLeaseId::new(kernel_id))
        {
            let _ = device.drm.revoke_lease(kernel_id);
            let _ = device.lease_registry.revoke(reservation.token);
            return Err(DrmLeaseError::Policy(error));
        }
        Ok(CreatedDrmLease {
            token: reservation.token,
            fd,
        })
    }

    pub(crate) fn revoke_drm_lease(&mut self, token: LeaseToken) -> Result<bool, DrmLeaseError> {
        let device_id = self.primary_node.dev_id();
        let Some(device) = self.devices.get_mut(&device_id) else {
            return Ok(false);
        };
        let Some(revocation) = device.lease_registry.revoke(token) else {
            return Ok(false);
        };
        if let Some(kernel_id) = revocation.kernel_id {
            device
                .drm
                .revoke_lease(raw_lease_id(kernel_id))
                .map_err(DrmLeaseError::Revoke)?;
        }
        Ok(true)
    }

    pub(crate) fn take_drm_lease_revocations(&mut self) -> Vec<LeaseRevocation> {
        std::mem::take(&mut self.pending_lease_revocations)
    }
}

pub(super) fn reconcile_catalog(
    device_id: u64,
    device: &mut OpenDevice,
    connectors: &std::collections::BTreeMap<ConnectorId, super::ConnectorSnapshot>,
) -> Result<Vec<LeaseRevocation>, BackendError> {
    let mut claimed_planes = Vec::new();
    let mut lease_connectors = Vec::new();
    for connector in connectors.values().filter(|connector| {
        connector.non_desktop
            && connector.state == ConnectorState::Connected
            && connector.mapped_crtc.is_some()
    }) {
        let crtc_id = connector
            .mapped_crtc
            .expect("filtered lease connector has a CRTC");
        let crtc = crtc::Handle::from(raw_handle(crtc_id));
        let primary_plane =
            match kms::select_lease_primary_plane(&device.drm, crtc, &claimed_planes) {
                Ok(plane) => plane,
                Err(error) => {
                    warn!(
                        device_id,
                        connector = %connector.name,
                        %error,
                        "non-desktop connector has no leasable primary plane"
                    );
                    continue;
                }
            };
        claimed_planes.push(primary_plane);
        lease_connectors.push(LeaseConnector {
            id: connector.id,
            name: connector.name.clone(),
            description: format!("{} non-desktop connector", connector.name),
            crtc_id,
            primary_plane_id: u32::from(primary_plane),
        });
    }
    let changes = device
        .lease_registry
        .reconcile_connectors(lease_connectors)
        .map_err(|error| BackendError::LeaseTopology {
            device_id,
            message: error.to_string(),
        })?;
    revoke_kernel_leases(device_id, device, &changes.revoked);
    Ok(changes.revoked)
}

pub(super) fn suspend(device_id: u64, device: &mut OpenDevice) -> Vec<LeaseRevocation> {
    let changes = device.lease_registry.set_session_active(false);
    revoke_kernel_leases(device_id, device, &changes.revoked);
    changes.revoked
}

pub(super) fn resume(device: &mut OpenDevice) {
    let _ = device.lease_registry.set_session_active(true);
}

fn revoke_kernel_leases(device_id: u64, device: &OpenDevice, revocations: &[LeaseRevocation]) {
    for revocation in revocations {
        let Some(kernel_id) = revocation.kernel_id else {
            continue;
        };
        if let Err(error) = device.drm.revoke_lease(raw_lease_id(kernel_id)) {
            warn!(
                device_id,
                lease_id = kernel_id.get(),
                %error,
                "failed to revoke kernel DRM lease"
            );
        }
    }
}

fn raw_handle(id: u32) -> RawResourceHandle {
    NonZeroU32::new(id).expect("DRM object handles are non-zero")
}

fn raw_lease_id(id: KernelLeaseId) -> drm::control::LeaseId {
    NonZeroU32::new(id.get()).expect("kernel lease IDs are non-zero")
}
