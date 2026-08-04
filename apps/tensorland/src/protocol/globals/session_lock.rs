//! Tensor-owned `ext-session-lock-v1` wire state and presentation gate.

use std::collections::HashMap;

use tensor_host::ConnectorId;
use wayland_protocols::ext::session_lock::v1::server::{
    ext_session_lock_manager_v1::ExtSessionLockManagerV1,
    ext_session_lock_surface_v1::ExtSessionLockSurfaceV1, ext_session_lock_v1::ExtSessionLockV1,
};
use wayland_server::{
    DisplayHandle, Resource, Weak,
    backend::{GlobalId, ObjectId},
    protocol::wl_surface::WlSurface,
};

use crate::protocol::{globals::output::OutputInstanceId, state::RuntimeState};

const MAX_PENDING_CONFIGURES: usize = 16;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::protocol) struct LockToken(u64);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::protocol) struct LockSurfaceToken(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LockStatus {
    Pending,
    Locked,
    Finished,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OutputGate {
    NeedsSubmit,
    WaitingForVBlank(u64),
}

#[derive(Debug)]
enum SessionPhase {
    Unlocked,
    Pending {
        lock: LockToken,
        outputs: HashMap<ConnectorId, OutputGate>,
    },
    Locked {
        controller: Option<LockToken>,
    },
}

#[derive(Debug)]
struct LockRecord {
    resource: Option<Weak<ExtSessionLockV1>>,
    status: LockStatus,
    surfaces: HashMap<OutputInstanceId, LockSurfaceToken>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LockConfigure {
    serial: u32,
    size: (u32, u32),
}

#[derive(Debug)]
struct ConfigureQueue {
    pending: [Option<LockConfigure>; MAX_PENDING_CONFIGURES],
    len: usize,
    last_acked: Option<LockConfigure>,
    deferred: Option<(u32, u32)>,
}

impl ConfigureQueue {
    fn new() -> Self {
        Self {
            pending: [None; MAX_PENDING_CONFIGURES],
            len: 0,
            last_acked: None,
            deferred: None,
        }
    }

    fn latest_size(&self) -> Option<(u32, u32)> {
        self.len
            .checked_sub(1)
            .and_then(|index| self.pending[index])
            .map(|configure| configure.size)
            .or(self.last_acked.map(|configure| configure.size))
    }

    fn push(&mut self, serial: u32, size: (u32, u32)) -> Option<LockConfigure> {
        if self.latest_size() == Some(size) {
            self.deferred = None;
            return None;
        }
        if self.len == MAX_PENDING_CONFIGURES {
            self.deferred = Some(size);
            return None;
        }
        let configure = LockConfigure { serial, size };
        self.pending[self.len] = Some(configure);
        self.len += 1;
        Some(configure)
    }

    fn ack(&mut self, serial: u32) -> Result<Option<(u32, u32)>, ()> {
        let index = self.pending[..self.len]
            .iter()
            .position(|configure| configure.is_some_and(|configure| configure.serial == serial))
            .ok_or(())?;
        let configure = self.pending[index].expect("the matching configure is present");
        let consumed = index + 1;
        self.pending.copy_within(consumed..self.len, 0);
        self.len -= consumed;
        self.pending[self.len..].fill(None);
        self.last_acked = Some(configure);
        let deferred = self.deferred.take();
        Ok(deferred.filter(|size| self.latest_size() != Some(*size)))
    }
}

#[derive(Debug)]
struct LockSurfaceRecord {
    lock: LockToken,
    output_instance: OutputInstanceId,
    output: ConnectorId,
    resource: Weak<ExtSessionLockSurfaceV1>,
    surface: WlSurface,
    configures: ConfigureQueue,
}

struct LockSurfaceRegistration {
    token: LockSurfaceToken,
    lock: LockToken,
    output_instance: OutputInstanceId,
    output: ConnectorId,
    output_is_live: bool,
    resource: Weak<ExtSessionLockSurfaceV1>,
    surface: WlSurface,
}

pub(crate) struct SessionLockProtocol {
    _global: GlobalId,
    phase: SessionPhase,
    locks: HashMap<LockToken, LockRecord>,
    surfaces: HashMap<LockSurfaceToken, LockSurfaceRecord>,
    surface_index: HashMap<ObjectId, LockSurfaceToken>,
    active_output_index: HashMap<ConnectorId, LockSurfaceToken>,
    next_lock: u64,
    next_surface: u64,
    next_serial: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BeginLock {
    Pending,
    Locked,
    Finished,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DestroyLock {
    Allowed,
    Cancelled,
    Invalid,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DropLock {
    None,
    Cancelled,
    Orphaned,
}

#[derive(Debug)]
struct RemovedSurface {
    surface: WlSurface,
    active: bool,
}

#[derive(Debug)]
struct RemovedOutput {
    surface: Option<WlSurface>,
    confirmed: Option<ExtSessionLockV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CommitError {
    BeforeFirstAck,
    NullBuffer,
    DimensionsMismatch,
}

#[derive(Debug)]
struct CommitFailure {
    resource: ExtSessionLockSurfaceV1,
    error: CommitError,
}

impl SessionLockProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        Self {
            _global: display.create_global::<RuntimeState, ExtSessionLockManagerV1, _>(
                1,
                SessionLockGlobalData,
            ),
            phase: SessionPhase::Unlocked,
            locks: HashMap::new(),
            surfaces: HashMap::new(),
            surface_index: HashMap::new(),
            active_output_index: HashMap::new(),
            next_lock: 1,
            next_surface: 1,
            next_serial: 1,
        }
    }

    fn allocate_lock(&mut self) -> LockToken {
        let token = LockToken(self.next_lock);
        self.next_lock = self
            .next_lock
            .checked_add(1)
            .expect("session lock token exhausted");
        token
    }

    fn allocate_surface(&mut self) -> LockSurfaceToken {
        let token = LockSurfaceToken(self.next_surface);
        self.next_surface = self
            .next_surface
            .checked_add(1)
            .expect("session lock surface token exhausted");
        token
    }

    fn next_serial(&mut self) -> u32 {
        let serial = self.next_serial;
        self.next_serial = self.next_serial.wrapping_add(1).max(1);
        serial
    }

    fn register_lock(
        &mut self,
        token: LockToken,
        resource: &ExtSessionLockV1,
        outputs: impl IntoIterator<Item = ConnectorId>,
    ) -> BeginLock {
        let (status, result) = if matches!(self.phase, SessionPhase::Unlocked) {
            let outputs = outputs
                .into_iter()
                .map(|output| (output, OutputGate::NeedsSubmit))
                .collect::<HashMap<_, _>>();
            if outputs.is_empty() {
                self.phase = SessionPhase::Locked {
                    controller: Some(token),
                };
                (LockStatus::Locked, BeginLock::Locked)
            } else {
                self.phase = SessionPhase::Pending {
                    lock: token,
                    outputs,
                };
                (LockStatus::Pending, BeginLock::Pending)
            }
        } else {
            (LockStatus::Finished, BeginLock::Finished)
        };
        self.locks.insert(
            token,
            LockRecord {
                resource: Some(resource.downgrade()),
                status,
                surfaces: HashMap::new(),
            },
        );
        result
    }

    pub(in crate::protocol) fn is_locked(&self) -> bool {
        !matches!(self.phase, SessionPhase::Unlocked)
    }

    fn active_lock(&self) -> Option<LockToken> {
        match self.phase {
            SessionPhase::Pending { lock, .. } => Some(lock),
            SessionPhase::Locked { controller } => controller,
            SessionPhase::Unlocked => None,
        }
    }

    fn can_add_surface(&self, lock: LockToken, output: OutputInstanceId) -> bool {
        self.locks
            .get(&lock)
            .is_some_and(|record| !record.surfaces.contains_key(&output))
    }

    fn insert_surface(&mut self, registration: LockSurfaceRegistration) -> bool {
        let LockSurfaceRegistration {
            token,
            lock,
            output_instance,
            output,
            output_is_live,
            resource,
            surface,
        } = registration;
        let active = output_is_live && self.active_lock() == Some(lock);
        let Some(lock_record) = self.locks.get_mut(&lock) else {
            return false;
        };
        lock_record.surfaces.insert(output_instance, token);
        self.surface_index.insert(surface.id(), token);
        if active {
            let previous = self.active_output_index.insert(output, token);
            debug_assert!(previous.is_none(), "active lock output must be unique");
        }
        self.surfaces.insert(
            token,
            LockSurfaceRecord {
                lock,
                output_instance,
                output,
                resource,
                surface,
                configures: ConfigureQueue::new(),
            },
        );
        active
    }

    fn configure_surface(
        &mut self,
        token: LockSurfaceToken,
        size: (u32, u32),
    ) -> Option<(ExtSessionLockSurfaceV1, LockConfigure)> {
        let serial = self.next_serial();
        let record = self.surfaces.get_mut(&token)?;
        let configure = record.configures.push(serial, size)?;
        Some((record.resource.upgrade().ok()?, configure))
    }

    fn ack_configure(
        &mut self,
        token: LockSurfaceToken,
        serial: u32,
    ) -> Result<Option<(ExtSessionLockSurfaceV1, LockConfigure)>, ()> {
        let deferred = self
            .surfaces
            .get_mut(&token)
            .ok_or(())?
            .configures
            .ack(serial)?;
        let Some(size) = deferred else {
            return Ok(None);
        };
        Ok(self.configure_surface(token, size))
    }

    fn remove_surface_token(&mut self, token: LockSurfaceToken) -> Option<RemovedSurface> {
        let record = self.surfaces.remove(&token)?;
        if self.surface_index.get(&record.surface.id()) == Some(&token) {
            self.surface_index.remove(&record.surface.id());
        }
        if self.active_output_index.get(&record.output) == Some(&token) {
            self.active_output_index.remove(&record.output);
        }
        if let Some(lock) = self.locks.get_mut(&record.lock)
            && lock.surfaces.get(&record.output_instance) == Some(&token)
        {
            lock.surfaces.remove(&record.output_instance);
        }
        let active = self.active_lock() == Some(record.lock);
        self.remove_unused_lock(record.lock);
        Some(RemovedSurface {
            surface: record.surface,
            active,
        })
    }

    fn remove_surface_resource(&mut self, token: LockSurfaceToken) -> Option<RemovedSurface> {
        self.remove_surface_token(token)
    }

    fn remove_wl_surface(&mut self, surface: &WlSurface) -> Option<RemovedSurface> {
        let token = self.surface_index.get(&surface.id()).copied()?;
        self.remove_surface_token(token)
    }

    fn remove_unused_lock(&mut self, token: LockToken) {
        let removable = self
            .locks
            .get(&token)
            .is_some_and(|record| record.resource.is_none() && record.surfaces.is_empty());
        if removable {
            self.locks.remove(&token);
        }
    }

    fn deactivate_lock_surfaces(&mut self, token: LockToken) {
        let Some(record) = self.locks.get(&token) else {
            return;
        };
        for surface in record.surfaces.values() {
            let Some(surface_record) = self.surfaces.get(surface) else {
                continue;
            };
            if self.active_output_index.get(&surface_record.output) == Some(surface) {
                self.active_output_index.remove(&surface_record.output);
            }
        }
    }

    fn request_destroy(&mut self, token: LockToken) -> DestroyLock {
        let Some(status) = self.locks.get(&token).map(|record| record.status) else {
            return DestroyLock::Allowed;
        };
        match status {
            LockStatus::Locked => DestroyLock::Invalid,
            LockStatus::Pending => {
                self.locks
                    .get_mut(&token)
                    .expect("lock status was read above")
                    .status = LockStatus::Finished;
                if matches!(self.phase, SessionPhase::Pending { lock, .. } if lock == token) {
                    self.phase = SessionPhase::Unlocked;
                    self.deactivate_lock_surfaces(token);
                    DestroyLock::Cancelled
                } else {
                    DestroyLock::Allowed
                }
            }
            LockStatus::Finished => DestroyLock::Allowed,
        }
    }

    fn request_unlock(&mut self, token: LockToken) -> bool {
        if self.locks.get(&token).map(|record| record.status) != Some(LockStatus::Locked)
            || !matches!(self.phase, SessionPhase::Locked { controller: Some(lock) } if lock == token)
        {
            return false;
        }
        self.locks
            .get_mut(&token)
            .expect("lock status was read above")
            .status = LockStatus::Finished;
        self.phase = SessionPhase::Unlocked;
        self.deactivate_lock_surfaces(token);
        true
    }

    fn drop_lock_resource(&mut self, token: LockToken) -> DropLock {
        let status = {
            let Some(record) = self.locks.get_mut(&token) else {
                return DropLock::None;
            };
            record.resource = None;
            record.status
        };
        let dropped = match status {
            LockStatus::Pending if matches!(self.phase, SessionPhase::Pending { lock, .. } if lock == token) =>
            {
                self.locks
                    .get_mut(&token)
                    .expect("lock resource was cleared above")
                    .status = LockStatus::Finished;
                self.phase = SessionPhase::Unlocked;
                DropLock::Cancelled
            }
            LockStatus::Locked if matches!(self.phase, SessionPhase::Locked { controller: Some(lock) } if lock == token) =>
            {
                self.phase = SessionPhase::Locked { controller: None };
                DropLock::Orphaned
            }
            _ => DropLock::None,
        };
        if dropped != DropLock::None {
            self.deactivate_lock_surfaces(token);
        }
        self.remove_unused_lock(token);
        dropped
    }

    pub(in crate::protocol) fn frame_submitted(&mut self, output: ConnectorId, timeline: u64) {
        let SessionPhase::Pending { outputs, .. } = &mut self.phase else {
            return;
        };
        if let Some(gate @ OutputGate::NeedsSubmit) = outputs.get_mut(&output) {
            *gate = OutputGate::WaitingForVBlank(timeline);
        }
    }

    pub(in crate::protocol) fn frame_completed(
        &mut self,
        output: ConnectorId,
        timeline: u64,
    ) -> Option<ExtSessionLockV1> {
        let SessionPhase::Pending { lock, outputs } = &mut self.phase else {
            return None;
        };
        if outputs.get(&output) != Some(&OutputGate::WaitingForVBlank(timeline)) {
            return None;
        }
        outputs.remove(&output);
        if !outputs.is_empty() {
            return None;
        }
        let lock = *lock;
        self.phase = SessionPhase::Locked {
            controller: Some(lock),
        };
        let record = self.locks.get_mut(&lock)?;
        record.status = LockStatus::Locked;
        record.resource.as_ref()?.upgrade().ok()
    }

    pub(in crate::protocol) fn output_added(&mut self, output: ConnectorId) {
        if let SessionPhase::Pending { outputs, .. } = &mut self.phase {
            outputs.insert(output, OutputGate::NeedsSubmit);
        }
    }

    pub(in crate::protocol) fn configure_output(&mut self, output: ConnectorId, size: (u32, u32)) {
        let Some(token) = self.active_output_index.get(&output).copied() else {
            return;
        };
        let serial = self.next_serial();
        let Some(record) = self.surfaces.get_mut(&token) else {
            self.active_output_index.remove(&output);
            return;
        };
        if let Some(configure) = record.configures.push(serial, size)
            && let Ok(resource) = record.resource.upgrade()
        {
            resource.configure(configure.serial, configure.size.0, configure.size.1);
        }
    }

    fn output_removed(&mut self, output: ConnectorId) -> RemovedOutput {
        let surface = self.active_output_index.remove(&output).and_then(|token| {
            let surface = self.surfaces.get(&token)?;
            if let Some(lock) = self.locks.get_mut(&surface.lock)
                && lock.surfaces.get(&surface.output_instance) == Some(&token)
            {
                lock.surfaces.remove(&surface.output_instance);
            }
            Some(surface.surface.clone())
        });
        let confirmed = if let SessionPhase::Pending { lock, outputs } = &mut self.phase {
            outputs.remove(&output);
            if outputs.is_empty() {
                let lock = *lock;
                self.phase = SessionPhase::Locked {
                    controller: Some(lock),
                };
                self.locks.get_mut(&lock).and_then(|record| {
                    record.status = LockStatus::Locked;
                    record.resource.as_ref()?.upgrade().ok()
                })
            } else {
                None
            }
        } else {
            None
        };
        RemovedOutput { surface, confirmed }
    }

    pub(in crate::protocol) fn surface_for_output(
        &self,
        output: ConnectorId,
    ) -> Option<&WlSurface> {
        let lock = self.active_lock()?;
        let token = self.active_output_index.get(&output)?;
        let record = self.surfaces.get(token)?;
        (record.lock == lock).then_some(&record.surface)
    }

    pub(in crate::protocol) fn contains_active_surface(&self, surface: &WlSurface) -> bool {
        let Some(token) = self.surface_index.get(&surface.id()) else {
            return false;
        };
        self.surfaces.get(token).is_some_and(|record| {
            self.active_lock() == Some(record.lock)
                && self.active_output_index.get(&record.output) == Some(token)
        })
    }

    pub(in crate::protocol) fn first_active_surface(&self) -> Option<WlSurface> {
        let lock = self.active_lock()?;
        let token = self
            .active_output_index
            .iter()
            .filter(|(_, token)| {
                self.surfaces
                    .get(token)
                    .is_some_and(|record| record.lock == lock)
            })
            .min_by_key(|(output, _)| **output)
            .map(|(_, token)| token)?;
        self.surfaces
            .get(token)
            .map(|record| record.surface.clone())
    }

    fn validate_commit(
        &mut self,
        surface: &WlSurface,
        change: PendingBufferChange,
    ) -> Result<bool, CommitFailure> {
        let Some(token) = self.surface_index.get(&surface.id()).copied() else {
            return Ok(false);
        };
        let Some(record) = self.surfaces.get_mut(&token) else {
            return Ok(false);
        };
        let resource = || {
            record
                .resource
                .upgrade()
                .expect("live lock surface commit retains its protocol object")
        };
        let Some(configure) = record.configures.last_acked else {
            return Err(CommitFailure {
                resource: resource(),
                error: CommitError::BeforeFirstAck,
            });
        };
        match change {
            PendingBufferChange::New(Some(size)) if size != configure.size => {
                return Err(CommitFailure {
                    resource: resource(),
                    error: CommitError::DimensionsMismatch,
                });
            }
            PendingBufferChange::New(_) => {}
            PendingBufferChange::Removed => {
                return Err(CommitFailure {
                    resource: resource(),
                    error: CommitError::NullBuffer,
                });
            }
            PendingBufferChange::None => {}
        }
        Ok(true)
    }

    #[cfg(test)]
    pub(in crate::protocol) fn counts(&self) -> (usize, usize) {
        (self.locks.len(), self.surfaces.len())
    }

    #[cfg(test)]
    pub(in crate::protocol) fn active_output_count(&self) -> usize {
        self.active_output_index.len()
    }
}

#[derive(Clone, Copy, Debug)]
enum PendingBufferChange {
    New(Option<(u32, u32)>),
    Removed,
    None,
}

mod wire;

use wire::SessionLockGlobalData;

#[cfg(test)]
mod tests;
