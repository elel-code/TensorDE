use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

use wayland_client_runtime::{Event, IdleNotifyEvent, IdleNotifyKind, Runtime, RuntimeError};

use crate::{IdleAction, IdlePlan, IdleStage};

mod output_power;

use output_power::OutputPowerSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IdleTransition {
    pub after_ms: u32,
    pub action: IdleAction,
    pub idle: bool,
}

pub struct IdleMonitorRuntime {
    wayland: Runtime,
    monitors: IdleMonitorSet,
    post_lock: PostLockMonitor,
    output_power: OutputPowerSet,
    events: Vec<Event>,
}

impl IdleMonitorRuntime {
    pub fn connect(plan: &IdlePlan) -> Result<Self, IdleRuntimeError> {
        let mut wayland = Runtime::connect()?;
        let mut monitors = IdleMonitorSet::register(&mut wayland, plan)?;
        let output_power = match OutputPowerSet::register(&mut wayland, plan) {
            Ok(output_power) => output_power,
            Err(error) => {
                monitors.unregister(&mut wayland);
                return Err(error);
            }
        };
        if let Err(error) = wayland.flush() {
            let mut output_power = output_power;
            output_power.unregister(&mut wayland);
            monitors.unregister(&mut wayland);
            return Err(error.into());
        }
        Ok(Self {
            wayland,
            monitors,
            post_lock: PostLockMonitor::default(),
            output_power,
            events: Vec::with_capacity(32),
        })
    }

    pub fn monitor_count(&self) -> usize {
        self.monitors.bindings.len()
    }

    pub fn output_power_count(&self) -> usize {
        self.output_power.len()
    }

    pub fn wake_handle(&self) -> wayland_client_runtime::WakeHandle {
        self.wayland.wake_handle()
    }

    /// Replace every idle deadline after a live policy change.
    ///
    /// New notifications are registered before the old set is destroyed. If
    /// registration fails, the active policy remains intact.
    pub fn reconfigure(&mut self, plan: &IdlePlan) -> Result<(), IdleRuntimeError> {
        let mut replacement = IdleMonitorSet::register(&mut self.wayland, plan)?;
        let mut post_lock_replacement =
            match self
                .post_lock
                .replacement(&mut self.wayland, plan, Instant::now())
            {
                Ok(replacement) => replacement,
                Err(error) => {
                    replacement.unregister(&mut self.wayland);
                    return Err(error);
                }
            };
        if let Err(error) = self.output_power.reconfigure(&mut self.wayland, plan) {
            post_lock_replacement.unregister(&mut self.wayland);
            replacement.unregister(&mut self.wayland);
            return Err(error);
        }
        self.monitors.unregister(&mut self.wayland);
        self.post_lock.unregister(&mut self.wayland);
        self.monitors = replacement;
        self.post_lock = post_lock_replacement;
        self.wayland.flush()?;
        Ok(())
    }

    /// Start the post-lock monitor-off deadline from a completed logind lock
    /// request. Registration failure leaves the current lock cycle intact.
    pub fn rebase_post_lock_monitor(&mut self, plan: &IdlePlan) -> Result<(), IdleRuntimeError> {
        let now = Instant::now();
        let mut replacement = PostLockMonitor::for_lock(&mut self.wayland, plan, now)?;
        self.post_lock.unregister(&mut self.wayland);
        std::mem::swap(&mut self.post_lock, &mut replacement);
        self.wayland.flush()?;
        Ok(())
    }

    pub fn dispatch_into(
        &mut self,
        timeout: Option<Duration>,
        transitions: &mut Vec<IdleTransition>,
    ) -> Result<(), IdleRuntimeError> {
        self.wayland.dispatch(timeout)?;
        self.events.clear();
        self.wayland.drain_events_into(&mut self.events);
        self.output_power
            .apply_batch(&mut self.wayland, self.events.iter())?;
        let canceled_post_lock = apply_idle_event_batch(
            &mut self.wayland,
            &mut self.monitors,
            &mut self.post_lock,
            self.events.iter().filter_map(idle_event),
            transitions,
        );
        if canceled_post_lock {
            self.wayland.flush()?;
        }
        Ok(())
    }

    /// Apply only monitor-power transitions; lock and suspend remain owned by
    /// their dedicated integrations.
    pub fn apply_monitor_power_transition(
        &mut self,
        transition: IdleTransition,
    ) -> Result<bool, IdleRuntimeError> {
        if transition.action != IdleAction::MonitorOff {
            return Ok(false);
        }
        self.output_power
            .set_idle(&mut self.wayland, transition.idle)?;
        Ok(true)
    }
}

impl Drop for IdleMonitorRuntime {
    fn drop(&mut self) {
        self.post_lock.unregister(&mut self.wayland);
        self.monitors.unregister(&mut self.wayland);
        self.output_power.unregister(&mut self.wayland);
        let _ = self.wayland.flush();
    }
}

#[derive(Clone, Copy, Debug)]
struct MonitorBinding {
    stage: IdleStage,
    idle: bool,
}

#[derive(Default)]
struct IdleMonitorSet {
    bindings: BTreeMap<u64, MonitorBinding>,
}

impl IdleMonitorSet {
    fn register(
        protocol: &mut impl IdleProtocol,
        plan: &IdlePlan,
    ) -> Result<Self, IdleRuntimeError> {
        Self::register_stages(
            protocol,
            plan.enabled,
            plan.respect_inhibitors,
            plan.stages
                .iter()
                .copied()
                .map(|stage| (stage, stage.after_ms)),
        )
    }

    fn register_stages(
        protocol: &mut impl IdleProtocol,
        enabled: bool,
        respect_inhibitors: bool,
        stages: impl IntoIterator<Item = (IdleStage, u32)>,
    ) -> Result<Self, IdleRuntimeError> {
        if !enabled {
            return Ok(Self::default());
        }
        let stages = stages.into_iter();
        if stages.size_hint().1 == Some(0) {
            return Ok(Self::default());
        }
        let kind = if respect_inhibitors {
            if !protocol.has_idle_notify() {
                return Err(IdleRuntimeError::MissingProtocol("ext_idle_notifier_v1"));
            }
            IdleNotifyKind::WithInhibitors
        } else {
            if !protocol.has_idle_notify_input() {
                return Err(IdleRuntimeError::MissingProtocol(
                    "ext_idle_notifier_v1 v2 input-only notifications",
                ));
            }
            IdleNotifyKind::InputOnly
        };

        let mut set = Self::default();
        for (stage, timeout_ms) in stages {
            match protocol.create_idle_notification(timeout_ms, kind) {
                Ok(id) => {
                    set.bindings
                        .insert(id, MonitorBinding { stage, idle: false });
                }
                Err(source) => {
                    set.unregister(protocol);
                    return Err(IdleRuntimeError::Register {
                        action: stage.action,
                        after_ms: stage.after_ms,
                        source,
                    });
                }
            }
        }
        Ok(set)
    }

    fn apply_batch(
        &mut self,
        events: impl IntoIterator<Item = IdleNotifyEvent>,
        transitions: &mut Vec<IdleTransition>,
    ) {
        let first = transitions.len();
        for event in events {
            let Some(binding) = self.bindings.get_mut(&event.id) else {
                continue;
            };
            if binding.idle == event.idle {
                continue;
            }
            binding.idle = event.idle;
            transitions.push(IdleTransition {
                after_ms: binding.stage.after_ms,
                action: binding.stage.action,
                idle: event.idle,
            });
        }
        transitions[first..].sort_unstable_by_key(|transition| {
            (transition.idle, transition.after_ms, transition.action)
        });
    }

    fn unregister(&mut self, protocol: &mut impl IdleProtocol) {
        for id in std::mem::take(&mut self.bindings).into_keys() {
            let _ = protocol.destroy_idle_notification(id);
        }
    }
}

#[derive(Default)]
struct PostLockMonitor {
    monitors: IdleMonitorSet,
    started_at: Option<Instant>,
}

impl PostLockMonitor {
    fn for_lock(
        protocol: &mut impl IdleProtocol,
        plan: &IdlePlan,
        now: Instant,
    ) -> Result<Self, IdleRuntimeError> {
        Self::register(protocol, plan, Some(now), now)
    }

    fn replacement(
        &self,
        protocol: &mut impl IdleProtocol,
        plan: &IdlePlan,
        now: Instant,
    ) -> Result<Self, IdleRuntimeError> {
        Self::register(protocol, plan, self.started_at, now)
    }

    fn register(
        protocol: &mut impl IdleProtocol,
        plan: &IdlePlan,
        started_at: Option<Instant>,
        now: Instant,
    ) -> Result<Self, IdleRuntimeError> {
        let mut monitor = Self {
            monitors: IdleMonitorSet::default(),
            started_at,
        };
        let (Some(started_at), Some(after_ms)) = (started_at, plan.post_lock_monitor_off_after_ms)
        else {
            return Ok(monitor);
        };
        let elapsed_ms = now
            .saturating_duration_since(started_at)
            .as_millis()
            .min(u128::from(u32::MAX)) as u32;
        let stage = IdleStage {
            after_ms,
            action: IdleAction::MonitorOff,
        };
        monitor.monitors = IdleMonitorSet::register_stages(
            protocol,
            plan.enabled,
            plan.respect_inhibitors,
            [(stage, after_ms.saturating_sub(elapsed_ms))],
        )?;
        Ok(monitor)
    }

    fn unregister(&mut self, protocol: &mut impl IdleProtocol) {
        self.monitors.unregister(protocol);
        self.started_at = None;
    }
}

fn idle_event(event: &Event) -> Option<IdleNotifyEvent> {
    match event {
        Event::IdleNotify(event) => Some(*event),
        _ => None,
    }
}

fn apply_idle_event_batch(
    protocol: &mut impl IdleProtocol,
    monitors: &mut IdleMonitorSet,
    post_lock: &mut PostLockMonitor,
    events: impl Clone + IntoIterator<Item = IdleNotifyEvent>,
    transitions: &mut Vec<IdleTransition>,
) -> bool {
    let first = transitions.len();
    monitors.apply_batch(events.clone(), transitions);
    let resumed_from_lock = transitions[first..]
        .iter()
        .any(|transition| transition.action == IdleAction::Lock && !transition.idle);
    if resumed_from_lock {
        post_lock.unregister(protocol);
    } else {
        post_lock.monitors.apply_batch(events, transitions);
    }
    transitions[first..].sort_unstable_by_key(|transition| {
        (transition.idle, transition.after_ms, transition.action)
    });
    resumed_from_lock
}

trait IdleProtocol {
    fn has_idle_notify(&self) -> bool;
    fn has_idle_notify_input(&self) -> bool;
    fn create_idle_notification(
        &mut self,
        timeout_ms: u32,
        kind: IdleNotifyKind,
    ) -> Result<u64, RuntimeError>;
    fn destroy_idle_notification(&mut self, id: u64) -> Result<(), RuntimeError>;
}

impl IdleProtocol for Runtime {
    fn has_idle_notify(&self) -> bool {
        self.has_idle_notify()
    }

    fn has_idle_notify_input(&self) -> bool {
        self.has_idle_notify_input()
    }

    fn create_idle_notification(
        &mut self,
        timeout_ms: u32,
        kind: IdleNotifyKind,
    ) -> Result<u64, RuntimeError> {
        self.create_idle_notification(timeout_ms, None, kind)
    }

    fn destroy_idle_notification(&mut self, id: u64) -> Result<(), RuntimeError> {
        self.destroy_idle_notification(id)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IdleRuntimeError {
    #[error(transparent)]
    Wayland(#[from] RuntimeError),
    #[error("required idle protocol is unavailable: {0}")]
    MissingProtocol(&'static str),
    #[error("failed to register {action:?} idle monitor at {after_ms} ms: {source}")]
    Register {
        action: IdleAction,
        after_ms: u32,
        source: RuntimeError,
    },
    #[error("required output-power protocol is unavailable: {0}")]
    MissingOutputPower(&'static str),
    #[error("failed to create output power control for {output:?}: {source}")]
    RegisterOutputPower {
        output: wayland_client_runtime::OutputId,
        source: RuntimeError,
    },
    #[error("failed to set {output:?} power mode to {mode:?}: {source}")]
    SetOutputPower {
        output: wayland_client_runtime::OutputId,
        mode: wayland_client_runtime::OutputPowerMode,
        source: RuntimeError,
    },
    #[error("failed to restore output power while changing idle policy: {source}")]
    RestoreOutputPower { source: RuntimeError },
    #[error("output power control failed for {output:?}")]
    OutputPowerFailed {
        output: wayland_client_runtime::OutputId,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{IdleConfig, PowerSource};

    #[derive(Default)]
    struct FakeProtocol {
        idle_notify: bool,
        idle_notify_input: bool,
        fail_create_at: Option<usize>,
        created: Vec<(u32, IdleNotifyKind)>,
        destroyed: Vec<u64>,
    }

    impl IdleProtocol for FakeProtocol {
        fn has_idle_notify(&self) -> bool {
            self.idle_notify
        }

        fn has_idle_notify_input(&self) -> bool {
            self.idle_notify_input
        }

        fn create_idle_notification(
            &mut self,
            timeout_ms: u32,
            kind: IdleNotifyKind,
        ) -> Result<u64, RuntimeError> {
            if self.fail_create_at == Some(self.created.len()) {
                return Err(RuntimeError::Protocol("injected failure".to_owned()));
            }
            self.created.push((timeout_ms, kind));
            Ok(self.created.len() as u64)
        }

        fn destroy_idle_notification(&mut self, id: u64) -> Result<(), RuntimeError> {
            self.destroyed.push(id);
            Ok(())
        }
    }

    #[test]
    fn registers_every_stage_with_the_selected_inhibitor_semantics() {
        let plan = IdlePlan::compile(&IdleConfig::default(), PowerSource::Ac);
        let mut protocol = FakeProtocol {
            idle_notify: true,
            ..FakeProtocol::default()
        };
        let monitors = IdleMonitorSet::register(&mut protocol, &plan).unwrap();
        assert_eq!(monitors.bindings.len(), 3);
        assert!(
            protocol
                .created
                .iter()
                .all(|(_, kind)| *kind == IdleNotifyKind::WithInhibitors)
        );
    }

    #[test]
    fn input_only_policy_requires_protocol_version_two() {
        let config = IdleConfig {
            respect_inhibitors: false,
            ..IdleConfig::default()
        };
        let plan = IdlePlan::compile(&config, PowerSource::Ac);
        let mut protocol = FakeProtocol {
            idle_notify: true,
            ..FakeProtocol::default()
        };
        assert!(matches!(
            IdleMonitorSet::register(&mut protocol, &plan),
            Err(IdleRuntimeError::MissingProtocol(_))
        ));
        assert!(protocol.created.is_empty());
    }

    #[test]
    fn partial_registration_is_rolled_back() {
        let plan = IdlePlan::compile(&IdleConfig::default(), PowerSource::Ac);
        let mut protocol = FakeProtocol {
            idle_notify: true,
            fail_create_at: Some(1),
            ..FakeProtocol::default()
        };
        assert!(matches!(
            IdleMonitorSet::register(&mut protocol, &plan),
            Err(IdleRuntimeError::Register { .. })
        ));
        assert_eq!(protocol.destroyed, [1]);
    }

    #[test]
    fn event_batch_is_deduplicated_and_sorted_by_policy_not_wire_order() {
        let plan = IdlePlan::compile(&IdleConfig::default(), PowerSource::Ac);
        let mut protocol = FakeProtocol {
            idle_notify: true,
            ..FakeProtocol::default()
        };
        let mut monitors = IdleMonitorSet::register(&mut protocol, &plan).unwrap();
        let mut transitions = Vec::new();
        monitors.apply_batch(
            [
                IdleNotifyEvent { id: 3, idle: true },
                IdleNotifyEvent { id: 1, idle: true },
                IdleNotifyEvent { id: 2, idle: true },
                IdleNotifyEvent { id: 2, idle: true },
            ],
            &mut transitions,
        );
        assert_eq!(transitions.len(), 3);
        assert!(transitions.windows(2).all(|pair| {
            (pair[0].after_ms, pair[0].action) <= (pair[1].after_ms, pair[1].action)
        }));
    }

    #[test]
    fn replacement_registration_failure_keeps_the_active_monitor_set() {
        let plan = IdlePlan::compile(&IdleConfig::default(), PowerSource::Ac);
        let mut protocol = FakeProtocol {
            idle_notify: true,
            ..FakeProtocol::default()
        };
        let mut monitors = IdleMonitorSet::register(&mut protocol, &plan).unwrap();
        let active = monitors.bindings.keys().copied().collect::<Vec<_>>();
        protocol.fail_create_at = Some(protocol.created.len() + 1);

        let result = IdleMonitorSet::register(&mut protocol, &plan);

        assert!(matches!(result, Err(IdleRuntimeError::Register { .. })));
        assert_eq!(
            monitors.bindings.keys().copied().collect::<Vec<_>>(),
            active
        );
        monitors.unregister(&mut protocol);
    }

    #[test]
    fn post_lock_registration_starts_at_lock_completion_and_rebases_remaining_time() {
        let plan = IdlePlan::compile(&IdleConfig::default(), PowerSource::Ac);
        let mut protocol = FakeProtocol {
            idle_notify: true,
            ..FakeProtocol::default()
        };
        let started_at = Instant::now() - Duration::from_secs(1);
        let current = PostLockMonitor {
            monitors: IdleMonitorSet::default(),
            started_at: Some(started_at),
        };
        let replacement = current
            .replacement(&mut protocol, &plan, Instant::now())
            .unwrap();
        let timeout = protocol.created[0].0;
        assert!(timeout < plan.post_lock_monitor_off_after_ms.unwrap());
        assert!(timeout > 0);
        assert_eq!(replacement.started_at, Some(started_at));
    }

    #[test]
    fn lock_resume_cancels_a_post_lock_event_from_the_same_batch() {
        let plan = IdlePlan::compile(&IdleConfig::default(), PowerSource::Ac);
        let mut protocol = FakeProtocol {
            idle_notify: true,
            ..FakeProtocol::default()
        };
        let mut monitors = IdleMonitorSet::register(&mut protocol, &plan).unwrap();
        let mut post_lock =
            PostLockMonitor::for_lock(&mut protocol, &plan, Instant::now()).unwrap();
        let lock_id = monitors
            .bindings
            .iter()
            .find_map(|(id, binding)| (binding.stage.action == IdleAction::Lock).then_some(*id))
            .unwrap();
        let post_lock_id = *post_lock.monitors.bindings.keys().next().unwrap();
        let mut transitions = Vec::new();
        apply_idle_event_batch(
            &mut protocol,
            &mut monitors,
            &mut post_lock,
            [IdleNotifyEvent {
                id: lock_id,
                idle: true,
            }],
            &mut transitions,
        );
        transitions.clear();

        assert!(apply_idle_event_batch(
            &mut protocol,
            &mut monitors,
            &mut post_lock,
            [
                IdleNotifyEvent {
                    id: lock_id,
                    idle: false,
                },
                IdleNotifyEvent {
                    id: post_lock_id,
                    idle: true,
                },
            ],
            &mut transitions,
        ));
        assert!(post_lock.monitors.bindings.is_empty());
        assert_eq!(
            transitions,
            [IdleTransition {
                after_ms: plan.stages[1].after_ms,
                action: IdleAction::Lock,
                idle: false,
            }]
        );
    }
}
