//! Tensor-owned ext-idle-notify state driven by one completion-model timerfd.

mod completion;
mod wire;

use std::{
    cmp::Reverse,
    collections::{BTreeMap, BinaryHeap, HashMap, HashSet},
    io,
    os::fd::OwnedFd,
    time::Duration,
};

use rustix::time::{
    ClockId, Itimerspec, TimerfdClockId, TimerfdFlags, TimerfdTimerFlags, Timespec, clock_gettime,
    timerfd_create, timerfd_settime,
};
use tracing::warn;
use wayland_protocols::ext::idle_notify::v1::server::{
    ext_idle_notification_v1::ExtIdleNotificationV1, ext_idle_notifier_v1::ExtIdleNotifierV1,
};
use wayland_server::{
    DisplayHandle, Resource,
    backend::{GlobalId, ObjectId},
};

use crate::protocol::state::RuntimeState;

use self::{completion::IdleTimerCompletion, wire::IdleNotifyGlobalData};

pub(crate) struct IdleNotifyProtocol {
    _global: Option<GlobalId>,
    scheduler: Option<IdleScheduler>,
}

impl IdleNotifyProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        match IdleScheduler::new() {
            Ok(scheduler) => {
                Self {
                    _global: Some(display.create_global::<RuntimeState, ExtIdleNotifierV1, _>(
                        2,
                        IdleNotifyGlobalData,
                    )),
                    scheduler: Some(scheduler),
                }
            }
            Err(error) => {
                warn!(%error, "idle-notify timerfd is unavailable; global will not be advertised");
                Self {
                    _global: None,
                    scheduler: None,
                }
            }
        }
    }

    pub(crate) fn advertised(&self) -> bool {
        self._global.is_some()
    }

    fn duplicate_timer_fd(&self) -> io::Result<Option<OwnedFd>> {
        self.scheduler
            .as_ref()
            .map(IdleScheduler::duplicate_fd)
            .transpose()
    }

    fn register(
        &mut self,
        notification: ExtIdleNotificationV1,
        timeout_ms: u32,
        kind: NotificationKind,
    ) {
        let Some(scheduler) = &mut self.scheduler else {
            return;
        };
        if let Err(error) = scheduler.register(notification, timeout_ms, kind) {
            warn!(%error, "idle-notify timerfd could not be armed");
            scheduler.fail();
        }
    }

    fn remove(&mut self, notification: &ExtIdleNotificationV1) {
        if let Some(scheduler) = &mut self.scheduler {
            scheduler.remove(&notification.id());
        }
    }

    pub(crate) fn notify_activity(&mut self) {
        let Some(scheduler) = &mut self.scheduler else {
            return;
        };
        if let Err(error) = scheduler.notify_activity() {
            warn!(%error, "idle-notify timerfd could not be armed after input activity");
            scheduler.fail();
        }
    }

    pub(crate) fn set_inhibited(&mut self, inhibited: bool) {
        let Some(scheduler) = &mut self.scheduler else {
            return;
        };
        if let Err(error) = scheduler.set_inhibited(inhibited) {
            warn!(%error, "idle-notify timerfd could not be armed after inhibitor change");
            scheduler.fail();
        }
    }

    fn complete_timer(&mut self) -> IdleTimerCompletion {
        self.scheduler
            .as_mut()
            .map_or_else(IdleTimerCompletion::failed, IdleScheduler::complete)
    }

    fn fail_timer(&mut self) {
        if let Some(scheduler) = &mut self.scheduler {
            scheduler.fail();
        }
    }

    #[cfg(test)]
    pub(crate) fn notification_count(&self) -> usize {
        self.scheduler
            .as_ref()
            .map_or(0, |scheduler| scheduler.notifications.len())
    }

    #[cfg(test)]
    pub(crate) fn idle_count(&self) -> usize {
        self.scheduler.as_ref().map_or(0, |scheduler| {
            scheduler.inhibitable_idle.len() + scheduler.input_only_idle.len()
        })
    }

    #[cfg(test)]
    pub(crate) fn armed_deadline(&self) -> Option<Duration> {
        self.scheduler
            .as_ref()
            .and_then(|scheduler| scheduler.armed)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum NotificationKind {
    Inhibitable,
    InputOnly,
}

struct Notification {
    resource: ExtIdleNotificationV1,
    timeout: Duration,
    kind: NotificationKind,
    token: u64,
    fresh_generation: Option<u64>,
    idle: bool,
}

#[derive(Default)]
struct TimeoutBucket {
    inhibitable: HashSet<ObjectId>,
    input_only: HashSet<ObjectId>,
    inhibitable_active: usize,
    input_only_active: usize,
    inhibitable_fresh: FreshCount,
    input_only_fresh: FreshCount,
}

impl TimeoutBucket {
    fn ids(&self, kind: NotificationKind) -> &HashSet<ObjectId> {
        match kind {
            NotificationKind::Inhibitable => &self.inhibitable,
            NotificationKind::InputOnly => &self.input_only,
        }
    }

    fn ids_mut(&mut self, kind: NotificationKind) -> &mut HashSet<ObjectId> {
        match kind {
            NotificationKind::Inhibitable => &mut self.inhibitable,
            NotificationKind::InputOnly => &mut self.input_only,
        }
    }

    fn active(&self, kind: NotificationKind) -> usize {
        match kind {
            NotificationKind::Inhibitable => self.inhibitable_active,
            NotificationKind::InputOnly => self.input_only_active,
        }
    }

    fn increment_active(&mut self, kind: NotificationKind) {
        match kind {
            NotificationKind::Inhibitable => {
                self.inhibitable_active = self.inhibitable_active.saturating_add(1);
            }
            NotificationKind::InputOnly => {
                self.input_only_active = self.input_only_active.saturating_add(1);
            }
        }
    }

    fn decrement_active(&mut self, kind: NotificationKind) {
        let active = match kind {
            NotificationKind::Inhibitable => &mut self.inhibitable_active,
            NotificationKind::InputOnly => &mut self.input_only_active,
        };
        *active = active.saturating_sub(1);
    }

    fn fresh(&self, kind: NotificationKind) -> &FreshCount {
        match kind {
            NotificationKind::Inhibitable => &self.inhibitable_fresh,
            NotificationKind::InputOnly => &self.input_only_fresh,
        }
    }

    fn fresh_mut(&mut self, kind: NotificationKind) -> &mut FreshCount {
        match kind {
            NotificationKind::Inhibitable => &mut self.inhibitable_fresh,
            NotificationKind::InputOnly => &mut self.input_only_fresh,
        }
    }

    fn is_empty(&self) -> bool {
        self.inhibitable.is_empty() && self.input_only.is_empty()
    }
}

#[derive(Default)]
struct FreshCount {
    generation: u64,
    count: usize,
}

impl FreshCount {
    fn current(&self, generation: u64) -> usize {
        if self.generation == generation {
            self.count
        } else {
            0
        }
    }

    fn increment(&mut self, generation: u64) {
        if self.generation != generation {
            self.generation = generation;
            self.count = 0;
        }
        self.count = self.count.saturating_add(1);
    }

    fn decrement(&mut self, generation: u64) {
        if self.generation == generation {
            self.count = self.count.saturating_sub(1);
        }
    }
}

#[derive(Clone, Debug)]
struct FreshDeadline {
    deadline: Duration,
    token: u64,
    notification: ObjectId,
}

impl PartialEq for FreshDeadline {
    fn eq(&self, other: &Self) -> bool {
        (self.deadline, self.token) == (other.deadline, other.token)
    }
}

impl Eq for FreshDeadline {}

impl PartialOrd for FreshDeadline {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FreshDeadline {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.deadline, self.token).cmp(&(other.deadline, other.token))
    }
}

struct IdleScheduler {
    timer: OwnedFd,
    notifications: HashMap<ObjectId, Notification>,
    buckets: BTreeMap<Duration, TimeoutBucket>,
    inhibitable_fresh: BinaryHeap<Reverse<FreshDeadline>>,
    input_only_fresh: BinaryHeap<Reverse<FreshDeadline>>,
    inhibitable_idle: HashSet<ObjectId>,
    input_only_idle: HashSet<ObjectId>,
    inhibitable_anchor: Duration,
    input_only_anchor: Duration,
    inhibitable_generation: u64,
    input_only_generation: u64,
    next_token: u64,
    inhibitable_count: usize,
    input_only_count: usize,
    armed: Option<Duration>,
    inhibited: bool,
    failed: bool,
}

impl IdleScheduler {
    fn new() -> io::Result<Self> {
        let timer = timerfd_create(
            TimerfdClockId::Monotonic,
            TimerfdFlags::CLOEXEC | TimerfdFlags::NONBLOCK,
        )?;
        let now = monotonic_now();
        Ok(Self {
            timer,
            notifications: HashMap::new(),
            buckets: BTreeMap::new(),
            inhibitable_fresh: BinaryHeap::new(),
            input_only_fresh: BinaryHeap::new(),
            inhibitable_idle: HashSet::new(),
            input_only_idle: HashSet::new(),
            inhibitable_anchor: now,
            input_only_anchor: now,
            inhibitable_generation: 1,
            input_only_generation: 1,
            next_token: 1,
            inhibitable_count: 0,
            input_only_count: 0,
            armed: None,
            inhibited: false,
            failed: false,
        })
    }

    fn duplicate_fd(&self) -> io::Result<OwnedFd> {
        rustix::io::fcntl_dupfd_cloexec(&self.timer, 0).map_err(io::Error::from)
    }

    fn register(
        &mut self,
        resource: ExtIdleNotificationV1,
        timeout_ms: u32,
        kind: NotificationKind,
    ) -> io::Result<()> {
        let now = monotonic_now();
        let timeout = Duration::from_millis(u64::from(timeout_ms));
        let token = self.next_token;
        self.next_token = self.next_token.wrapping_add(1).max(1);
        let scheduled = kind == NotificationKind::InputOnly || !self.inhibited;
        let generation = scheduled.then(|| self.generation(kind));
        let id = resource.id();

        let bucket = self.buckets.entry(timeout).or_default();
        bucket.ids_mut(kind).insert(id.clone());
        bucket.increment_active(kind);
        if let Some(generation) = generation {
            bucket.fresh_mut(kind).increment(generation);
        }
        let (idle, total) = match kind {
            NotificationKind::Inhibitable => {
                self.inhibitable_count = self.inhibitable_count.saturating_add(1);
                (&mut self.inhibitable_idle, self.inhibitable_count)
            }
            NotificationKind::InputOnly => {
                self.input_only_count = self.input_only_count.saturating_add(1);
                (&mut self.input_only_idle, self.input_only_count)
            }
        };
        idle.reserve(total.saturating_sub(idle.len()));
        self.notifications.insert(
            id.clone(),
            Notification {
                resource,
                timeout,
                kind,
                token,
                fresh_generation: generation,
                idle: false,
            },
        );

        if scheduled {
            let deadline = now.saturating_add(timeout);
            self.fresh_heap_mut(kind).push(Reverse(FreshDeadline {
                deadline,
                token,
                notification: id,
            }));
            self.arm_if_earlier(deadline)?;
        }
        Ok(())
    }

    fn remove(&mut self, id: &ObjectId) {
        let Some(notification) = self.notifications.remove(id) else {
            return;
        };
        match notification.kind {
            NotificationKind::Inhibitable => {
                self.inhibitable_count = self.inhibitable_count.saturating_sub(1);
            }
            NotificationKind::InputOnly => {
                self.input_only_count = self.input_only_count.saturating_sub(1);
            }
        }
        self.idle_set_mut(notification.kind).remove(id);
        self.fresh_heap_mut(notification.kind)
            .retain(|entry| entry.0.token != notification.token);

        let generation = self.generation(notification.kind);
        let mut remove_bucket = false;
        if let Some(bucket) = self.buckets.get_mut(&notification.timeout) {
            bucket.ids_mut(notification.kind).remove(id);
            if !notification.idle {
                bucket.decrement_active(notification.kind);
                if notification.fresh_generation == Some(generation) {
                    bucket.fresh_mut(notification.kind).decrement(generation);
                }
            }
            remove_bucket = bucket.is_empty();
        }
        if remove_bucket {
            self.buckets.remove(&notification.timeout);
        }
    }

    fn notify_activity(&mut self) -> io::Result<()> {
        let track_input_only = self.input_only_count != 0;
        let track_inhibitable = !self.inhibited && self.inhibitable_count != 0;
        if !track_input_only && !track_inhibitable {
            return Ok(());
        }
        let now = monotonic_now();
        let mut introduced_deadline = false;
        if track_input_only {
            self.input_only_generation = next_generation(self.input_only_generation);
            self.input_only_anchor = now;
            introduced_deadline = self.resume_idle(NotificationKind::InputOnly);
        }
        if track_inhibitable {
            self.inhibitable_generation = next_generation(self.inhibitable_generation);
            self.inhibitable_anchor = now;
            introduced_deadline |= self.resume_idle(NotificationKind::Inhibitable);
        }
        if self.armed.is_none() || introduced_deadline {
            self.ensure_armed()
        } else {
            Ok(())
        }
    }

    fn set_inhibited(&mut self, inhibited: bool) -> io::Result<()> {
        if self.inhibited == inhibited {
            return Ok(());
        }
        self.inhibited = inhibited;
        self.inhibitable_generation = next_generation(self.inhibitable_generation);
        if inhibited {
            self.resume_idle(NotificationKind::Inhibitable);
            return Ok(());
        }
        if self.inhibitable_count == 0 {
            return Ok(());
        }
        self.inhibitable_anchor = monotonic_now();
        self.ensure_armed()
    }

    fn resume_idle(&mut self, kind: NotificationKind) -> bool {
        let Self {
            notifications,
            buckets,
            inhibitable_idle,
            input_only_idle,
            ..
        } = self;
        let idle = match kind {
            NotificationKind::Inhibitable => inhibitable_idle,
            NotificationKind::InputOnly => input_only_idle,
        };
        let mut resumed = false;
        for id in idle.drain() {
            let Some(notification) = notifications.get_mut(&id) else {
                continue;
            };
            if !notification.idle || notification.kind != kind {
                continue;
            }
            notification.idle = false;
            resumed = true;
            if let Some(bucket) = buckets.get_mut(&notification.timeout) {
                bucket.increment_active(kind);
            }
            notification.resource.resumed();
        }
        resumed
    }

    fn complete(&mut self) -> IdleTimerCompletion {
        if self.failed {
            return IdleTimerCompletion::failed();
        }
        let mut expirations = [0_u8; 8];
        if let Err(error) = rustix::io::read(&self.timer, &mut expirations[..]).and_then(|read| {
            (read == expirations.len())
                .then_some(())
                .ok_or(rustix::io::Errno::IO)
        }) {
            self.fail();
            return IdleTimerCompletion::with_error(error.to_string());
        }
        self.armed = None;
        let now = monotonic_now();
        self.fire_established(now, NotificationKind::InputOnly);
        if !self.inhibited {
            self.fire_established(now, NotificationKind::Inhibitable);
        }
        self.fire_fresh(now, NotificationKind::InputOnly);
        if !self.inhibited {
            self.fire_fresh(now, NotificationKind::Inhibitable);
        }
        match self.ensure_armed() {
            Ok(()) => IdleTimerCompletion::healthy(),
            Err(error) => {
                self.fail();
                IdleTimerCompletion::with_error(error.to_string())
            }
        }
    }

    fn fire_established(&mut self, now: Duration, kind: NotificationKind) {
        let elapsed = now.saturating_sub(self.anchor(kind));
        let generation = self.generation(kind);
        let Self {
            notifications,
            buckets,
            inhibitable_idle,
            input_only_idle,
            ..
        } = self;
        let idle = match kind {
            NotificationKind::Inhibitable => inhibitable_idle,
            NotificationKind::InputOnly => input_only_idle,
        };
        for (_, bucket) in buckets.range_mut(..=elapsed) {
            if bucket.active(kind) <= bucket.fresh(kind).current(generation) {
                continue;
            }
            let mut fired = 0_usize;
            for id in bucket.ids(kind) {
                let Some(notification) = notifications.get_mut(id) else {
                    continue;
                };
                if notification.kind != kind
                    || notification.idle
                    || notification.fresh_generation == Some(generation)
                {
                    continue;
                }
                notification.idle = true;
                fired = fired.saturating_add(1);
                idle.insert(id.clone());
                notification.resource.idled();
            }
            for _ in 0..fired {
                bucket.decrement_active(kind);
            }
        }
    }

    fn fire_fresh(&mut self, now: Duration, kind: NotificationKind) {
        loop {
            self.discard_stale_fresh(kind);
            let due = self
                .fresh_heap(kind)
                .peek()
                .is_some_and(|entry| entry.0.deadline <= now);
            if !due {
                return;
            }
            let entry = self
                .fresh_heap_mut(kind)
                .pop()
                .expect("peeked fresh deadline")
                .0;
            let generation = self.generation(kind);
            let Some(notification) = self.notifications.get_mut(&entry.notification) else {
                continue;
            };
            if notification.token != entry.token
                || notification.kind != kind
                || notification.idle
                || notification.fresh_generation != Some(generation)
            {
                continue;
            }
            notification.idle = true;
            let timeout = notification.timeout;
            notification.resource.idled();
            if let Some(bucket) = self.buckets.get_mut(&timeout) {
                bucket.decrement_active(kind);
                bucket.fresh_mut(kind).decrement(generation);
            }
            self.idle_set_mut(kind).insert(entry.notification);
        }
    }

    fn discard_stale_fresh(&mut self, kind: NotificationKind) {
        loop {
            let Some(entry) = self.fresh_heap(kind).peek() else {
                return;
            };
            let entry = &entry.0;
            let generation = self.generation(kind);
            let valid = self
                .notifications
                .get(&entry.notification)
                .is_some_and(|notification| {
                    notification.token == entry.token
                        && notification.kind == kind
                        && !notification.idle
                        && notification.fresh_generation == Some(generation)
                });
            if valid {
                return;
            }
            self.fresh_heap_mut(kind).pop();
        }
    }

    fn ensure_armed(&mut self) -> io::Result<()> {
        if self.failed {
            return Ok(());
        }
        if let Some(deadline) = self.next_deadline() {
            self.arm_if_earlier(deadline)?;
        }
        Ok(())
    }

    fn arm_if_earlier(&mut self, deadline: Duration) -> io::Result<()> {
        if self.failed || self.armed.is_some_and(|armed| armed <= deadline) {
            return Ok(());
        }
        self.arm(deadline)
    }

    fn arm(&mut self, deadline: Duration) -> io::Result<()> {
        let seconds = i64::try_from(deadline.as_secs()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "idle deadline exceeds time_t")
        })?;
        timerfd_settime(
            &self.timer,
            TimerfdTimerFlags::ABSTIME,
            &Itimerspec {
                it_interval: Timespec::default(),
                it_value: Timespec {
                    tv_sec: seconds,
                    tv_nsec: i64::from(deadline.subsec_nanos()),
                },
            },
        )?;
        self.armed = Some(deadline);
        Ok(())
    }

    fn next_deadline(&mut self) -> Option<Duration> {
        let input = self.next_deadline_for(NotificationKind::InputOnly);
        let inhibitable = (!self.inhibited)
            .then(|| self.next_deadline_for(NotificationKind::Inhibitable))
            .flatten();
        input.into_iter().chain(inhibitable).min()
    }

    fn next_deadline_for(&mut self, kind: NotificationKind) -> Option<Duration> {
        self.discard_stale_fresh(kind);
        let fresh = self.fresh_heap(kind).peek().map(|entry| entry.0.deadline);
        let generation = self.generation(kind);
        let established = self.buckets.iter().find_map(|(timeout, bucket)| {
            (bucket.active(kind) > bucket.fresh(kind).current(generation))
                .then(|| self.anchor(kind).saturating_add(*timeout))
        });
        fresh.into_iter().chain(established).min()
    }

    fn fail(&mut self) {
        if self.failed {
            return;
        }
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
    }

    fn generation(&self, kind: NotificationKind) -> u64 {
        match kind {
            NotificationKind::Inhibitable => self.inhibitable_generation,
            NotificationKind::InputOnly => self.input_only_generation,
        }
    }

    fn anchor(&self, kind: NotificationKind) -> Duration {
        match kind {
            NotificationKind::Inhibitable => self.inhibitable_anchor,
            NotificationKind::InputOnly => self.input_only_anchor,
        }
    }

    fn fresh_heap(&self, kind: NotificationKind) -> &BinaryHeap<Reverse<FreshDeadline>> {
        match kind {
            NotificationKind::Inhibitable => &self.inhibitable_fresh,
            NotificationKind::InputOnly => &self.input_only_fresh,
        }
    }

    fn fresh_heap_mut(
        &mut self,
        kind: NotificationKind,
    ) -> &mut BinaryHeap<Reverse<FreshDeadline>> {
        match kind {
            NotificationKind::Inhibitable => &mut self.inhibitable_fresh,
            NotificationKind::InputOnly => &mut self.input_only_fresh,
        }
    }

    fn idle_set_mut(&mut self, kind: NotificationKind) -> &mut HashSet<ObjectId> {
        match kind {
            NotificationKind::Inhibitable => &mut self.inhibitable_idle,
            NotificationKind::InputOnly => &mut self.input_only_idle,
        }
    }
}

fn monotonic_now() -> Duration {
    let now = clock_gettime(ClockId::Monotonic);
    Duration::new(
        u64::try_from(now.tv_sec).unwrap_or(0),
        u32::try_from(now.tv_nsec).unwrap_or(0),
    )
}

fn next_generation(generation: u64) -> u64 {
    generation.wrapping_add(1).max(1)
}
