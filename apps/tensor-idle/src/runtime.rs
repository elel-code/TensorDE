use std::{collections::BTreeMap, time::Duration};

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
        self.monitors.apply_batch(
            self.events.iter().filter_map(|event| match event {
                Event::IdleNotify(event) => Some(*event),
                _ => None,
            }),
            transitions,
        );
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
        if !plan.enabled || plan.stages.is_empty() {
            return Ok(Self::default());
        }
        let kind = if plan.respect_inhibitors {
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
        for &stage in &plan.stages {
            match protocol.create_idle_notification(stage.after_ms, kind) {
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
}
