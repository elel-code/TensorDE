//! Bounded value-only DRM lease policy and lifecycle.
//!
//! Kernel ioctls and Wayland resources stay in Tensorland's compositor-thread
//! owner. This module decides which connector identities may be reserved and
//! which active kernel leases must be revoked after topology/session changes.

use std::{
    fmt,
    num::{NonZeroU32, NonZeroU64},
};

use tensor_host::ConnectorId;

pub const MAX_LEASE_CONNECTORS: usize = 32;
pub const MAX_CONNECTORS_PER_LEASE: usize = 8;
pub const MAX_ACTIVE_LEASES: usize = 16;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeaseConnector {
    pub id: ConnectorId,
    pub name: String,
    pub description: String,
    pub crtc_id: u32,
    pub primary_plane_id: u32,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LeaseToken(NonZeroU64);

impl LeaseToken {
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct KernelLeaseId(NonZeroU32);

impl KernelLeaseId {
    pub const fn new(id: NonZeroU32) -> Self {
        Self(id)
    }

    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeaseReservation {
    pub token: LeaseToken,
    pub connectors: Vec<LeaseConnector>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActiveLease {
    pub token: LeaseToken,
    pub kernel_id: KernelLeaseId,
    pub connectors: Vec<ConnectorId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeaseRevocation {
    pub token: LeaseToken,
    pub kernel_id: Option<KernelLeaseId>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LeaseOfferChanges {
    pub offered: Vec<LeaseConnector>,
    pub withdrawn: Vec<ConnectorId>,
    pub revoked: Vec<LeaseRevocation>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LeasePhase {
    Reserved,
    Active(KernelLeaseId),
}

#[derive(Debug)]
struct ConnectorRecord {
    descriptor: LeaseConnector,
    owner: Option<LeaseToken>,
}

#[derive(Debug)]
struct LeaseRecord {
    token: LeaseToken,
    connectors: Vec<ConnectorId>,
    phase: LeasePhase,
}

#[derive(Debug)]
pub struct LeaseRegistry {
    session_active: bool,
    connectors: Vec<ConnectorRecord>,
    leases: Vec<LeaseRecord>,
    next_token: NonZeroU64,
}

impl Default for LeaseRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl LeaseRegistry {
    pub fn new() -> Self {
        Self {
            session_active: true,
            connectors: Vec::with_capacity(MAX_LEASE_CONNECTORS),
            leases: Vec::with_capacity(MAX_ACTIVE_LEASES),
            next_token: NonZeroU64::MIN,
        }
    }

    pub fn session_active(&self) -> bool {
        self.session_active
    }

    pub fn available(&self) -> impl Iterator<Item = &LeaseConnector> {
        self.connectors
            .iter()
            .filter(|connector| self.session_active && connector.owner.is_none())
            .map(|connector| &connector.descriptor)
    }

    pub fn catalog(&self) -> impl Iterator<Item = &LeaseConnector> {
        self.connectors
            .iter()
            .map(|connector| &connector.descriptor)
    }

    pub fn reconcile_connectors(
        &mut self,
        mut connectors: Vec<LeaseConnector>,
    ) -> Result<LeaseOfferChanges, LeaseError> {
        validate_connector_catalog(&connectors)?;
        connectors.sort_by_key(|connector| connector.id);
        let previously_available = self.available().cloned().collect::<Vec<_>>();
        let changed_or_removed = self
            .connectors
            .iter()
            .filter(|old| !connectors.contains(&old.descriptor))
            .map(|old| old.descriptor.id)
            .collect::<Vec<_>>();
        let affected = self
            .leases
            .iter()
            .filter(|lease| {
                lease
                    .connectors
                    .iter()
                    .any(|connector| changed_or_removed.contains(connector))
            })
            .map(|lease| lease.token)
            .collect::<Vec<_>>();
        let mut revoked = Vec::with_capacity(affected.len());
        for token in affected {
            if let Some(revocation) = self.revoke(token) {
                revoked.push(revocation);
            }
        }

        let old_owners = self
            .connectors
            .iter()
            .filter_map(|old| old.owner.map(|owner| (old.descriptor.id, owner)))
            .collect::<Vec<_>>();
        self.connectors.clear();
        for descriptor in connectors {
            let owner = old_owners
                .iter()
                .find_map(|(id, owner)| (*id == descriptor.id).then_some(*owner));
            self.connectors.push(ConnectorRecord { descriptor, owner });
        }

        let currently_available = self.available().cloned().collect::<Vec<_>>();
        Ok(LeaseOfferChanges {
            offered: currently_available
                .iter()
                .filter(|new| !previously_available.contains(new))
                .cloned()
                .collect(),
            withdrawn: previously_available
                .iter()
                .filter(|old| !currently_available.contains(old))
                .map(|old| old.id)
                .collect(),
            revoked,
        })
    }

    pub fn set_session_active(&mut self, active: bool) -> LeaseOfferChanges {
        if self.session_active == active {
            return LeaseOfferChanges::default();
        }
        let previously_available = self.available().cloned().collect::<Vec<_>>();
        self.session_active = active;
        let tokens = if active {
            Vec::new()
        } else {
            self.leases.iter().map(|lease| lease.token).collect()
        };
        let mut revoked = Vec::with_capacity(tokens.len());
        for token in tokens {
            if let Some(revocation) = self.revoke(token) {
                revoked.push(revocation);
            }
        }
        let currently_available = self.available().cloned().collect::<Vec<_>>();
        LeaseOfferChanges {
            offered: currently_available,
            withdrawn: previously_available
                .into_iter()
                .map(|connector| connector.id)
                .collect(),
            revoked,
        }
    }

    pub fn reserve(
        &mut self,
        device_id: u64,
        requested: &[ConnectorId],
    ) -> Result<LeaseReservation, LeaseError> {
        if !self.session_active {
            return Err(LeaseError::SessionInactive);
        }
        if requested.is_empty() {
            return Err(LeaseError::EmptyLease);
        }
        if requested.len() > MAX_CONNECTORS_PER_LEASE {
            return Err(LeaseError::TooManyRequestedConnectors {
                count: requested.len(),
                max: MAX_CONNECTORS_PER_LEASE,
            });
        }
        if self.leases.len() == MAX_ACTIVE_LEASES {
            return Err(LeaseError::LeaseCapacity);
        }
        for (index, connector) in requested.iter().enumerate() {
            if connector.device_id != device_id {
                return Err(LeaseError::WrongDevice(*connector));
            }
            if requested[..index].contains(connector) {
                return Err(LeaseError::DuplicateConnector(*connector));
            }
            let Some(record) = self
                .connectors
                .iter()
                .find(|record| record.descriptor.id == *connector)
            else {
                return Err(LeaseError::UnavailableConnector(*connector));
            };
            if record.owner.is_some() {
                return Err(LeaseError::UnavailableConnector(*connector));
            }
        }

        let token = LeaseToken(self.next_token);
        self.next_token =
            NonZeroU64::new(self.next_token.get().wrapping_add(1)).unwrap_or(NonZeroU64::MIN);
        let mut reserved = Vec::with_capacity(requested.len());
        for id in requested {
            let record = self
                .connectors
                .iter_mut()
                .find(|record| record.descriptor.id == *id)
                .expect("request validation found every connector");
            record.owner = Some(token);
            reserved.push(record.descriptor.clone());
        }
        self.leases.push(LeaseRecord {
            token,
            connectors: requested.to_vec(),
            phase: LeasePhase::Reserved,
        });
        Ok(LeaseReservation {
            token,
            connectors: reserved,
        })
    }

    pub fn activate(
        &mut self,
        token: LeaseToken,
        kernel_id: KernelLeaseId,
    ) -> Result<ActiveLease, LeaseError> {
        let lease = self
            .leases
            .iter_mut()
            .find(|lease| lease.token == token)
            .ok_or(LeaseError::UnknownLease(token))?;
        if lease.phase != LeasePhase::Reserved {
            return Err(LeaseError::AlreadyActivated(token));
        }
        lease.phase = LeasePhase::Active(kernel_id);
        Ok(ActiveLease {
            token,
            kernel_id,
            connectors: lease.connectors.clone(),
        })
    }

    pub fn revoke(&mut self, token: LeaseToken) -> Option<LeaseRevocation> {
        let index = self.leases.iter().position(|lease| lease.token == token)?;
        let lease = self.leases.swap_remove(index);
        for connector in &mut self.connectors {
            if connector.owner == Some(token) {
                connector.owner = None;
            }
        }
        Some(LeaseRevocation {
            token,
            kernel_id: match lease.phase {
                LeasePhase::Reserved => None,
                LeasePhase::Active(kernel_id) => Some(kernel_id),
            },
        })
    }
}

fn validate_connector_catalog(connectors: &[LeaseConnector]) -> Result<(), LeaseError> {
    if connectors.len() > MAX_LEASE_CONNECTORS {
        return Err(LeaseError::ConnectorCapacity {
            count: connectors.len(),
            max: MAX_LEASE_CONNECTORS,
        });
    }
    for (index, connector) in connectors.iter().enumerate() {
        if connectors[..index]
            .iter()
            .any(|previous| previous.id == connector.id)
        {
            return Err(LeaseError::DuplicateCatalogConnector(connector.id));
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LeaseError {
    SessionInactive,
    EmptyLease,
    WrongDevice(ConnectorId),
    DuplicateConnector(ConnectorId),
    UnavailableConnector(ConnectorId),
    DuplicateCatalogConnector(ConnectorId),
    ConnectorCapacity { count: usize, max: usize },
    TooManyRequestedConnectors { count: usize, max: usize },
    LeaseCapacity,
    UnknownLease(LeaseToken),
    AlreadyActivated(LeaseToken),
}

impl fmt::Display for LeaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SessionInactive => formatter.write_str("DRM lease session is inactive"),
            Self::EmptyLease => formatter.write_str("DRM lease request contains no connectors"),
            Self::WrongDevice(id) => {
                write!(formatter, "connector {id:?} belongs to another DRM device")
            }
            Self::DuplicateConnector(id) => {
                write!(formatter, "connector {id:?} was requested more than once")
            }
            Self::UnavailableConnector(id) => {
                write!(formatter, "connector {id:?} is not available for leasing")
            }
            Self::DuplicateCatalogConnector(id) => write!(
                formatter,
                "connector {id:?} appears more than once in the lease catalog"
            ),
            Self::ConnectorCapacity { count, max } => write!(
                formatter,
                "lease connector catalog has {count} entries, maximum is {max}"
            ),
            Self::TooManyRequestedConnectors { count, max } => write!(
                formatter,
                "lease request has {count} connectors, maximum is {max}"
            ),
            Self::LeaseCapacity => formatter.write_str("active DRM lease capacity is exhausted"),
            Self::UnknownLease(token) => write!(formatter, "unknown DRM lease token {token:?}"),
            Self::AlreadyActivated(token) => {
                write!(formatter, "DRM lease token {token:?} was already activated")
            }
        }
    }
}

impl std::error::Error for LeaseError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn connector(device: u64, id: u32) -> LeaseConnector {
        LeaseConnector {
            id: ConnectorId::new(device, id),
            name: format!("card-{device}-connector-{id}"),
            description: format!("lease connector {id}"),
            crtc_id: id + 100,
            primary_plane_id: id + 200,
        }
    }

    fn kernel_id(id: u32) -> KernelLeaseId {
        KernelLeaseId::new(NonZeroU32::new(id).unwrap())
    }

    #[test]
    fn request_validation_is_atomic_and_device_scoped() {
        let mut registry = LeaseRegistry::new();
        registry
            .reconcile_connectors(vec![connector(1, 1), connector(1, 2), connector(2, 3)])
            .unwrap();
        assert_eq!(registry.reserve(1, &[]), Err(LeaseError::EmptyLease));
        assert_eq!(
            registry.reserve(1, &[ConnectorId::new(2, 3)]),
            Err(LeaseError::WrongDevice(ConnectorId::new(2, 3)))
        );
        assert_eq!(
            registry.reserve(1, &[ConnectorId::new(1, 1), ConnectorId::new(1, 1)]),
            Err(LeaseError::DuplicateConnector(ConnectorId::new(1, 1)))
        );
        assert_eq!(registry.available().count(), 3);
    }

    #[test]
    fn reservation_excludes_connectors_until_rejected() {
        let mut registry = LeaseRegistry::new();
        registry
            .reconcile_connectors(vec![connector(1, 1), connector(1, 2)])
            .unwrap();
        let reservation = registry.reserve(1, &[ConnectorId::new(1, 1)]).unwrap();
        assert_eq!(
            registry
                .available()
                .map(|connector| connector.id)
                .collect::<Vec<_>>(),
            vec![ConnectorId::new(1, 2)]
        );
        assert_eq!(
            registry.reserve(1, &[ConnectorId::new(1, 1)]),
            Err(LeaseError::UnavailableConnector(ConnectorId::new(1, 1)))
        );
        assert_eq!(registry.revoke(reservation.token).unwrap().kernel_id, None);
        assert_eq!(registry.available().count(), 2);
    }

    #[test]
    fn session_pause_withdraws_offers_and_revokes_kernel_leases() {
        let mut registry = LeaseRegistry::new();
        registry
            .reconcile_connectors(vec![connector(1, 1), connector(1, 2)])
            .unwrap();
        let reservation = registry.reserve(1, &[ConnectorId::new(1, 1)]).unwrap();
        registry.activate(reservation.token, kernel_id(9)).unwrap();

        let paused = registry.set_session_active(false);
        assert_eq!(paused.withdrawn, vec![ConnectorId::new(1, 2)]);
        assert_eq!(
            paused.revoked,
            vec![LeaseRevocation {
                token: reservation.token,
                kernel_id: Some(kernel_id(9)),
            }]
        );
        assert_eq!(registry.available().count(), 0);

        let resumed = registry.set_session_active(true);
        assert_eq!(resumed.offered.len(), 2);
        assert_eq!(registry.available().count(), 2);
    }

    #[test]
    fn hot_unplug_revokes_affected_lease_only() {
        let mut registry = LeaseRegistry::new();
        registry
            .reconcile_connectors(vec![connector(1, 1), connector(1, 2)])
            .unwrap();
        let first = registry.reserve(1, &[ConnectorId::new(1, 1)]).unwrap();
        let second = registry.reserve(1, &[ConnectorId::new(1, 2)]).unwrap();
        registry.activate(first.token, kernel_id(10)).unwrap();
        registry.activate(second.token, kernel_id(11)).unwrap();

        let changed = registry
            .reconcile_connectors(vec![connector(1, 2)])
            .unwrap();
        assert_eq!(
            changed.revoked,
            vec![LeaseRevocation {
                token: first.token,
                kernel_id: Some(kernel_id(10)),
            }]
        );
        assert!(registry.revoke(second.token).is_some());
        assert_eq!(registry.available().count(), 1);
    }

    #[test]
    fn resource_reassignment_revokes_and_reoffers_connector() {
        let mut registry = LeaseRegistry::new();
        registry
            .reconcile_connectors(vec![connector(1, 1)])
            .unwrap();
        let reservation = registry.reserve(1, &[ConnectorId::new(1, 1)]).unwrap();
        registry.activate(reservation.token, kernel_id(12)).unwrap();
        let mut reassigned = connector(1, 1);
        reassigned.primary_plane_id = 999;

        let changed = registry
            .reconcile_connectors(vec![reassigned.clone()])
            .unwrap();
        assert_eq!(
            changed.revoked,
            vec![LeaseRevocation {
                token: reservation.token,
                kernel_id: Some(kernel_id(12)),
            }]
        );
        assert_eq!(changed.offered, vec![reassigned]);
    }
}
