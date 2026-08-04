//! Tensor-owned FIFO and commit-timing protocol state.

mod commit_timer;
mod fifo;

use std::{
    cell::RefCell,
    cmp::Reverse,
    collections::{BinaryHeap, HashMap},
    io,
    os::fd::OwnedFd,
};

use super::compositor::{Barrier, with_states};
use rustix::time::{
    ClockId, Itimerspec, TimerfdClockId, TimerfdFlags, TimerfdTimerFlags, Timespec, clock_gettime,
    timerfd_create, timerfd_settime,
};
use tracing::warn;
use wayland_protocols::wp::{
    commit_timing::v1::server::{
        wp_commit_timer_v1::WpCommitTimerV1, wp_commit_timing_manager_v1::WpCommitTimingManagerV1,
    },
    fifo::v1::server::{wp_fifo_manager_v1::WpFifoManagerV1, wp_fifo_v1::WpFifoV1},
};
use wayland_server::{
    Client, DisplayHandle, Resource, Weak,
    backend::{GlobalId, ObjectId},
    protocol::wl_surface::WlSurface,
};

use crate::protocol::state::RuntimeState;

use self::{
    commit_timer::CommitTimingGlobalData,
    fifo::{FifoBarrierCachedState, FifoGlobalData},
};

pub(crate) struct SurfaceTimingProtocol {
    _fifo_global: GlobalId,
    _commit_timing_global: Option<GlobalId>,
    surfaces: RefCell<HashMap<ObjectId, SurfaceTimingState>>,
    active_fifo: RefCell<HashMap<ObjectId, SurfaceBarrier>>,
    commit_scheduler: Option<RefCell<CommitScheduler>>,
}

impl SurfaceTimingProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        let fifo_global =
            display.create_global::<RuntimeState, WpFifoManagerV1, _>(1, FifoGlobalData);
        let (commit_timing_global, commit_scheduler) = match CommitScheduler::new() {
            Ok(scheduler) => (
                Some(
                    display.create_global::<RuntimeState, WpCommitTimingManagerV1, _>(
                        1,
                        CommitTimingGlobalData,
                    ),
                ),
                Some(RefCell::new(scheduler)),
            ),
            Err(error) => {
                warn!(%error, "commit-timing timerfd is unavailable; global will not be advertised");
                (None, None)
            }
        };
        Self {
            _fifo_global: fifo_global,
            _commit_timing_global: commit_timing_global,
            surfaces: RefCell::new(HashMap::new()),
            active_fifo: RefCell::new(HashMap::new()),
            commit_scheduler,
        }
    }

    pub(crate) fn commit_timing_advertised(&self) -> bool {
        self._commit_timing_global.is_some()
    }

    pub(crate) fn duplicate_commit_timer_fd(&self) -> io::Result<Option<OwnedFd>> {
        self.commit_scheduler
            .as_ref()
            .map(|scheduler| scheduler.borrow().duplicate_fd())
            .transpose()
    }

    fn attach_fifo(&self, surface: &WlSurface, resource: &WpFifoV1) -> AttachResult {
        let mut surfaces = self.surfaces.borrow_mut();
        let timing = surfaces.entry(surface.id()).or_default();
        if timing.fifo_resource.is_some() {
            return AttachResult::AlreadyExists;
        }
        timing.fifo_resource = Some(resource.id());
        let install_hooks = !timing.fifo_hooks_installed;
        timing.fifo_hooks_installed = true;
        AttachResult::Attached { install_hooks }
    }

    fn detach_fifo(&self, surface: &WlSurface, resource: &WpFifoV1) {
        let mut surfaces = self.surfaces.borrow_mut();
        let Some(timing) = surfaces.get_mut(&surface.id()) else {
            return;
        };
        if timing.fifo_resource.as_ref() == Some(&resource.id()) {
            timing.fifo_resource = None;
        }
    }

    fn attach_commit_timer(&self, surface: &WlSurface, resource: &WpCommitTimerV1) -> AttachResult {
        let mut surfaces = self.surfaces.borrow_mut();
        let timing = surfaces.entry(surface.id()).or_default();
        if timing.commit_timer_resource.is_some() {
            return AttachResult::AlreadyExists;
        }
        timing.commit_timer_resource = Some(resource.id());
        let install_hooks = !timing.commit_timer_hook_installed;
        timing.commit_timer_hook_installed = true;
        AttachResult::Attached { install_hooks }
    }

    fn detach_commit_timer(&self, surface: &WlSurface, resource: &WpCommitTimerV1) {
        let mut surfaces = self.surfaces.borrow_mut();
        let Some(timing) = surfaces.get_mut(&surface.id()) else {
            return;
        };
        if timing.commit_timer_resource.as_ref() == Some(&resource.id()) {
            timing.commit_timer_resource = None;
        }
    }

    fn set_pending_timestamp(&self, surface: &WlSurface, deadline: Deadline) -> bool {
        let mut surfaces = self.surfaces.borrow_mut();
        let timing = surfaces.entry(surface.id()).or_default();
        if timing.pending_timestamp.is_some() {
            return false;
        }
        timing.pending_timestamp = Some(deadline);
        true
    }

    fn take_pending_timestamp(&self, surface: &WlSurface) -> Option<Deadline> {
        self.surfaces
            .borrow_mut()
            .get_mut(&surface.id())?
            .pending_timestamp
            .take()
    }

    fn register_deadline(&self, surface: &WlSurface, deadline: Deadline) -> Registration {
        let barrier = SurfaceBarrier::new(surface);
        let Some(scheduler) = &self.commit_scheduler else {
            barrier.signal();
            return Registration {
                blocker: barrier.barrier.clone(),
                released: vec![barrier],
            };
        };
        scheduler.borrow_mut().register(deadline, barrier)
    }

    fn activate_fifo(&self, surface: &WlSurface) -> FifoActivation {
        #[cfg(test)]
        if let Some(timing) = self.surfaces.borrow_mut().get_mut(&surface.id()) {
            timing.applied_fifo_commits = timing.applied_fifo_commits.saturating_add(1);
        }
        let current = with_states(surface, |states| {
            states
                .cached_state
                .get::<FifoBarrierCachedState>()
                .current()
                .barrier
                .clone()
        });
        let surface_id = surface.id();
        let mut active = self.active_fifo.borrow_mut();
        let unchanged = active.get(&surface_id).map(|entry| &entry.barrier) == current.as_ref();
        if unchanged {
            return FifoActivation::default();
        }

        let released = active.remove(&surface_id).into_iter().collect();
        let activated = current.is_some_and(|barrier| {
            if barrier.is_signaled() {
                false
            } else {
                active.insert(
                    surface_id.clone(),
                    SurfaceBarrier {
                        surface_id,
                        surface: surface.downgrade(),
                        barrier,
                    },
                );
                true
            }
        });
        if activated && let Some(timing) = self.surfaces.borrow_mut().get_mut(&surface.id()) {
            timing.fifo_activation_pending = true;
        }
        FifoActivation { released }
    }

    pub(crate) fn take_fifo_activation(&self, surface: &WlSurface) -> bool {
        let mut surfaces = self.surfaces.borrow_mut();
        let Some(timing) = surfaces.get_mut(&surface.id()) else {
            return false;
        };
        std::mem::take(&mut timing.fifo_activation_pending)
    }

    pub(in crate::protocol) fn capture_fifo_barriers(
        &self,
        mut submitted: impl FnMut(&ObjectId) -> bool,
    ) -> Vec<SurfaceBarrier> {
        let active = self.active_fifo.borrow();
        active
            .iter()
            .filter(|(surface, _)| submitted(surface))
            .map(|(_, barrier)| barrier.clone())
            .collect()
    }

    pub(super) fn finish_fifo_barriers(&self, barriers: &[SurfaceBarrier]) {
        if barriers.is_empty() {
            return;
        }
        let mut active = self.active_fifo.borrow_mut();
        for barrier in barriers {
            if active
                .get(&barrier.surface_id)
                .is_some_and(|current| current.barrier == barrier.barrier)
            {
                active.remove(&barrier.surface_id);
            }
        }
    }

    pub(crate) fn take_unlatched_fifo_barriers(&self) -> Vec<SurfaceBarrier> {
        let barriers = self
            .active_fifo
            .borrow_mut()
            .drain()
            .map(|(_, b)| b)
            .collect::<Vec<_>>();
        let mut surfaces = self.surfaces.borrow_mut();
        for barrier in &barriers {
            if let Some(timing) = surfaces.get_mut(&barrier.surface_id) {
                timing.fifo_activation_pending = false;
            }
        }
        barriers
    }

    pub(crate) fn complete_commit_timer(&self) -> TimerCompletion {
        let Some(scheduler) = &self.commit_scheduler else {
            return TimerCompletion::failed(Vec::new());
        };
        scheduler.borrow_mut().complete()
    }

    pub(crate) fn fail_commit_timer(&self) -> Vec<SurfaceBarrier> {
        self.commit_scheduler
            .as_ref()
            .map(|scheduler| scheduler.borrow_mut().fail_open())
            .unwrap_or_default()
    }

    pub(crate) fn remove_surface(&self, surface: &WlSurface) -> Vec<SurfaceBarrier> {
        self.surfaces.borrow_mut().remove(&surface.id());
        let mut released = self
            .active_fifo
            .borrow_mut()
            .remove(&surface.id())
            .into_iter()
            .collect::<Vec<_>>();

        with_states(surface, |states| {
            if states.cached_state.has::<FifoBarrierCachedState>() {
                let mut cached = states.cached_state.get::<FifoBarrierCachedState>();
                if let Some(barrier) = cached.current().barrier.take() {
                    released.push(SurfaceBarrier::with_barrier(surface, barrier));
                }
                if let Some(barrier) = cached.pending().barrier.take() {
                    released.push(SurfaceBarrier::with_barrier(surface, barrier));
                }
            }
        });
        if let Some(scheduler) = &self.commit_scheduler {
            released.extend(scheduler.borrow_mut().remove_surface(&surface.id()));
        }
        released
    }

    #[cfg(test)]
    pub(in crate::protocol) fn active_fifo_barrier_count(&self) -> usize {
        self.active_fifo.borrow().len()
    }

    #[cfg(test)]
    pub(in crate::protocol) fn applied_fifo_commit_count(&self) -> u64 {
        self.surfaces
            .borrow()
            .values()
            .map(|timing| timing.applied_fifo_commits)
            .sum()
    }

    #[cfg(test)]
    pub(in crate::protocol) fn scheduled_commit_timer_count(&self) -> usize {
        self.commit_scheduler
            .as_ref()
            .map_or(0, |scheduler| scheduler.borrow().deadlines.len())
    }

    #[cfg(test)]
    fn note_applied_timed_commit(&self, surface: &WlSurface) {
        if let Some(timing) = self.surfaces.borrow_mut().get_mut(&surface.id()) {
            timing.applied_timed_commits = timing.applied_timed_commits.saturating_add(1);
        }
    }

    #[cfg(test)]
    pub(in crate::protocol) fn applied_timed_commit_count(&self) -> u64 {
        self.surfaces
            .borrow()
            .values()
            .map(|timing| timing.applied_timed_commits)
            .sum()
    }
}

#[derive(Debug, Default)]
struct SurfaceTimingState {
    fifo_resource: Option<ObjectId>,
    commit_timer_resource: Option<ObjectId>,
    pending_timestamp: Option<Deadline>,
    fifo_hooks_installed: bool,
    commit_timer_hook_installed: bool,
    fifo_activation_pending: bool,
    #[cfg(test)]
    applied_fifo_commits: u64,
    #[cfg(test)]
    applied_timed_commits: u64,
}

#[derive(Clone, Debug)]
pub(in crate::protocol) struct SurfaceBarrier {
    surface_id: ObjectId,
    surface: Weak<WlSurface>,
    barrier: Barrier,
}

impl SurfaceBarrier {
    fn new(surface: &WlSurface) -> Self {
        Self::with_barrier(surface, Barrier::new(false))
    }

    fn with_barrier(surface: &WlSurface, barrier: Barrier) -> Self {
        Self {
            surface_id: surface.id(),
            surface: surface.downgrade(),
            barrier,
        }
    }

    pub(super) fn signal(&self) {
        self.barrier.signal();
    }

    pub(super) fn surface(&self) -> &Weak<WlSurface> {
        &self.surface
    }
}

#[derive(Default)]
struct FifoActivation {
    released: Vec<SurfaceBarrier>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Deadline {
    seconds: u64,
    nanoseconds: u32,
}

impl Deadline {
    fn from_wire(tv_sec_hi: u32, tv_sec_lo: u32, tv_nsec: u32) -> Option<Self> {
        (tv_nsec < 1_000_000_000).then_some(Self {
            seconds: (u64::from(tv_sec_hi) << 32) | u64::from(tv_sec_lo),
            nanoseconds: tv_nsec,
        })
    }

    fn now() -> Self {
        let now = clock_gettime(ClockId::Monotonic);
        Self {
            seconds: u64::try_from(now.tv_sec).unwrap_or(0),
            nanoseconds: u32::try_from(now.tv_nsec).unwrap_or(0),
        }
    }

    fn timespec(self) -> Option<Timespec> {
        Some(Timespec {
            tv_sec: i64::try_from(self.seconds).ok()?,
            tv_nsec: i64::from(self.nanoseconds),
        })
    }
}

#[derive(Debug)]
struct TimedBarrier {
    deadline: Deadline,
    sequence: u64,
    barrier: SurfaceBarrier,
}

impl PartialEq for TimedBarrier {
    fn eq(&self, other: &Self) -> bool {
        (self.deadline, self.sequence) == (other.deadline, other.sequence)
    }
}

impl Eq for TimedBarrier {}

impl PartialOrd for TimedBarrier {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TimedBarrier {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.deadline, self.sequence).cmp(&(other.deadline, other.sequence))
    }
}

struct Registration {
    blocker: Barrier,
    released: Vec<SurfaceBarrier>,
}

pub(crate) struct TimerCompletion {
    pub(crate) released: Vec<SurfaceBarrier>,
    pub(crate) rearm: bool,
    pub(crate) error: Option<String>,
}

impl TimerCompletion {
    fn healthy(released: Vec<SurfaceBarrier>) -> Self {
        Self {
            released,
            rearm: true,
            error: None,
        }
    }

    fn failed(released: Vec<SurfaceBarrier>) -> Self {
        Self {
            released,
            rearm: false,
            error: None,
        }
    }

    fn with_error(released: Vec<SurfaceBarrier>, error: String) -> Self {
        Self {
            released,
            rearm: false,
            error: Some(error),
        }
    }
}

struct CommitScheduler {
    timer: OwnedFd,
    deadlines: BinaryHeap<Reverse<TimedBarrier>>,
    sequence: u64,
    armed: Option<Deadline>,
    failed: bool,
}

impl CommitScheduler {
    fn new() -> io::Result<Self> {
        let timer = timerfd_create(
            TimerfdClockId::Monotonic,
            TimerfdFlags::CLOEXEC | TimerfdFlags::NONBLOCK,
        )?;
        Ok(Self {
            timer,
            deadlines: BinaryHeap::new(),
            sequence: 0,
            armed: None,
            failed: false,
        })
    }

    fn duplicate_fd(&self) -> io::Result<OwnedFd> {
        rustix::io::fcntl_dupfd_cloexec(&self.timer, 0).map_err(io::Error::from)
    }

    fn register(&mut self, deadline: Deadline, barrier: SurfaceBarrier) -> Registration {
        if self.failed || deadline <= Deadline::now() {
            barrier.signal();
            return Registration {
                blocker: barrier.barrier.clone(),
                released: vec![barrier],
            };
        }
        let blocker = barrier.barrier.clone();
        let sequence = self.sequence;
        self.sequence = self.sequence.wrapping_add(1);
        self.deadlines.push(Reverse(TimedBarrier {
            deadline,
            sequence,
            barrier,
        }));
        let released = match self.arm_earliest() {
            Ok(()) => Vec::new(),
            Err(error) => {
                warn!(%error, "commit-timing timerfd could not be armed; releasing constraints");
                self.fail_open()
            }
        };
        Registration { blocker, released }
    }

    fn complete(&mut self) -> TimerCompletion {
        if self.failed {
            return TimerCompletion::failed(Vec::new());
        }
        let mut expirations = [0_u8; 8];
        if let Err(error) = rustix::io::read(&self.timer, &mut expirations[..]).and_then(|read| {
            (read == expirations.len())
                .then_some(())
                .ok_or(rustix::io::Errno::IO)
        }) {
            let released = self.fail_open();
            return TimerCompletion::with_error(released, error.to_string());
        }
        self.armed = None;
        let now = Deadline::now();
        let mut released = Vec::new();
        while self
            .deadlines
            .peek()
            .is_some_and(|entry| entry.0.deadline <= now)
        {
            let entry = self.deadlines.pop().expect("peeked deadline").0;
            released.push(entry.barrier);
        }
        match self.arm_earliest() {
            Ok(()) => TimerCompletion::healthy(released),
            Err(error) => {
                released.extend(self.fail_open());
                TimerCompletion::with_error(released, error.to_string())
            }
        }
    }

    fn remove_surface(&mut self, surface: &ObjectId) -> Vec<SurfaceBarrier> {
        let mut released = Vec::new();
        self.deadlines.retain(|entry| {
            if &entry.0.barrier.surface_id == surface {
                released.push(entry.0.barrier.clone());
                false
            } else {
                true
            }
        });
        if !self.failed
            && let Err(error) = self.arm_earliest()
        {
            warn!(%error, "commit-timing timerfd could not be rearmed after surface removal");
            released.extend(self.fail_open());
        }
        released
    }

    fn arm_earliest(&mut self) -> io::Result<()> {
        let earliest = self.deadlines.peek().map(|entry| entry.0.deadline);
        if self.armed == earliest {
            return Ok(());
        }
        let value = earliest.and_then(Deadline::timespec).unwrap_or_default();
        timerfd_settime(
            &self.timer,
            TimerfdTimerFlags::ABSTIME,
            &Itimerspec {
                it_interval: Timespec::default(),
                it_value: value,
            },
        )?;
        self.armed = earliest;
        Ok(())
    }

    fn fail_open(&mut self) -> Vec<SurfaceBarrier> {
        self.failed = true;
        self.armed = None;
        let _ = timerfd_settime(
            &self.timer,
            TimerfdTimerFlags::empty(),
            &Itimerspec {
                it_interval: Timespec::default(),
                it_value: Timespec::default(),
            },
        );
        self.deadlines
            .drain()
            .map(|entry| entry.0.barrier)
            .collect()
    }
}

impl RuntimeState {
    pub(crate) fn duplicate_commit_timer_fd(&self) -> io::Result<Option<OwnedFd>> {
        self.protocol_globals
            .surface_timing
            .duplicate_commit_timer_fd()
    }

    pub(crate) fn complete_commit_timer(&mut self) -> bool {
        let completion = self.protocol_globals.surface_timing.complete_commit_timer();
        if let Some(error) = completion.error {
            warn!(%error, "commit-timing completion failed; releasing all constraints");
        }
        self.release_surface_barriers(completion.released);
        completion.rearm
    }

    pub(crate) fn commit_timer_completion_failed(&mut self, error: &io::Error) {
        warn!(%error, "commit-timing io_uring operation failed; releasing all constraints");
        let released = self.protocol_globals.surface_timing.fail_commit_timer();
        self.release_surface_barriers(released);
    }

    pub(crate) fn release_unlatched_fifo_barriers(&mut self) {
        let barriers = self
            .protocol_globals
            .surface_timing
            .take_unlatched_fifo_barriers();
        self.release_surface_barriers(barriers);
    }

    pub(in crate::protocol) fn release_captured_fifo_barriers(
        &mut self,
        barriers: Vec<SurfaceBarrier>,
    ) {
        self.protocol_globals
            .surface_timing
            .finish_fifo_barriers(&barriers);
        self.release_surface_barriers(barriers);
    }

    pub(in crate::protocol) fn release_surface_barriers(&mut self, barriers: Vec<SurfaceBarrier>) {
        if barriers.is_empty() {
            return;
        }
        let mut clients = Vec::<Client>::new();
        for barrier in barriers {
            barrier.signal();
            let Ok(surface) = barrier.surface().upgrade() else {
                continue;
            };
            let Some(client) = surface.client() else {
                continue;
            };
            if clients.iter().all(|known| known.id() != client.id()) {
                clients.push(client);
            }
        }
        for client in clients {
            self.compositor_blocker_cleared(&client);
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AttachResult {
    AlreadyExists,
    Attached { install_hooks: bool },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_validation_preserves_full_unsigned_seconds() {
        assert!(Deadline::from_wire(0, 0, 1_000_000_000).is_none());
        let deadline = Deadline::from_wire(u32::MAX, u32::MAX, 999_999_999).unwrap();
        assert_eq!(deadline.seconds, u64::MAX);
        assert_eq!(deadline.nanoseconds, 999_999_999);
        assert!(deadline.timespec().is_none());
    }
}
