use std::collections::BTreeMap;

use wayland_client_runtime::{
    Event, OutputEvent, OutputId, OutputPowerEvent, OutputPowerMode, Runtime, RuntimeError,
};

use crate::{IdleAction, IdlePlan, IdleRuntimeError};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct OutputPowerState {
    mode: Option<OutputPowerMode>,
    desired: Option<OutputPowerMode>,
}

#[derive(Default)]
pub(super) struct OutputPowerSet {
    enabled: bool,
    idle: bool,
    controls: BTreeMap<OutputId, OutputPowerState>,
}

impl OutputPowerSet {
    pub(super) fn register(
        protocol: &mut impl OutputPowerProtocol,
        plan: &IdlePlan,
    ) -> Result<Self, IdleRuntimeError> {
        let enabled = plan.enabled
            && plan
                .stages
                .iter()
                .any(|stage| stage.action == IdleAction::MonitorOff);
        if !enabled {
            return Ok(Self::default());
        }
        if !protocol.has_output_power() {
            return Err(IdleRuntimeError::MissingOutputPower(
                "zwlr_output_power_manager_v1",
            ));
        }

        let mut set = Self {
            enabled: true,
            ..Self::default()
        };
        for output in protocol.output_ids() {
            if let Err(source) = protocol.create_output_power(output) {
                set.unregister(protocol);
                return Err(IdleRuntimeError::RegisterOutputPower { output, source });
            }
            set.controls.insert(output, OutputPowerState::default());
        }
        Ok(set)
    }

    pub(super) fn len(&self) -> usize {
        self.controls.len()
    }

    pub(super) fn apply_batch<'a>(
        &mut self,
        protocol: &mut impl OutputPowerProtocol,
        events: impl IntoIterator<Item = &'a Event>,
    ) -> Result<(), IdleRuntimeError> {
        if !self.enabled {
            return Ok(());
        }
        for event in events {
            match event {
                Event::Output(OutputEvent::Added(info) | OutputEvent::Updated(info)) => {
                    self.add_output(protocol, info.id)?;
                }
                Event::Output(OutputEvent::Removed(output)) => {
                    self.controls.remove(output);
                }
                Event::OutputPower(OutputPowerEvent::Mode { output, mode }) => {
                    if let Some(state) = self.controls.get_mut(output) {
                        state.mode = Some(*mode);
                    }
                }
                Event::OutputPower(OutputPowerEvent::Failed { output })
                    if self.controls.remove(output).is_some() =>
                {
                    let _ = protocol.destroy_output_power(*output);
                    self.restore_outputs_after_failure(protocol, *output)?;
                    return Err(IdleRuntimeError::OutputPowerFailed { output: *output });
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub(super) fn set_idle(
        &mut self,
        protocol: &mut impl OutputPowerProtocol,
        idle: bool,
    ) -> Result<(), IdleRuntimeError> {
        if !self.enabled || self.idle == idle {
            return Ok(());
        }
        let mode = if idle {
            OutputPowerMode::Off
        } else {
            OutputPowerMode::On
        };
        let outputs = self.controls.keys().copied().collect::<Vec<_>>();
        for output in outputs {
            let state = self.controls.get_mut(&output).expect("stable output key");
            if state.desired == Some(mode) {
                continue;
            }
            if let Err(source) = protocol.set_output_power(output, mode) {
                if idle {
                    let _ = self.restore_outputs(protocol);
                }
                return Err(IdleRuntimeError::SetOutputPower {
                    output,
                    mode,
                    source,
                });
            }
            state.desired = Some(mode);
        }
        self.idle = idle;
        Ok(())
    }

    pub(super) fn unregister(&mut self, protocol: &mut impl OutputPowerProtocol) {
        for output in std::mem::take(&mut self.controls).into_keys() {
            let _ = protocol.destroy_output_power(output);
        }
        self.idle = false;
    }

    fn add_output(
        &mut self,
        protocol: &mut impl OutputPowerProtocol,
        output: OutputId,
    ) -> Result<(), IdleRuntimeError> {
        if self.controls.contains_key(&output) {
            return Ok(());
        }
        protocol
            .create_output_power(output)
            .map_err(|source| IdleRuntimeError::RegisterOutputPower { output, source })?;
        let mut state = OutputPowerState::default();
        if self.idle {
            protocol
                .set_output_power(output, OutputPowerMode::Off)
                .map_err(|source| IdleRuntimeError::SetOutputPower {
                    output,
                    mode: OutputPowerMode::Off,
                    source,
                })?;
            state.desired = Some(OutputPowerMode::Off);
        }
        self.controls.insert(output, state);
        Ok(())
    }

    fn restore_outputs_after_failure(
        &mut self,
        protocol: &mut impl OutputPowerProtocol,
        failed: OutputId,
    ) -> Result<(), IdleRuntimeError> {
        if !self.idle {
            return Ok(());
        }
        self.restore_outputs(protocol)
            .map_err(|source| IdleRuntimeError::SetOutputPower {
                output: failed,
                mode: OutputPowerMode::On,
                source,
            })
    }

    fn restore_outputs(
        &mut self,
        protocol: &mut impl OutputPowerProtocol,
    ) -> Result<(), RuntimeError> {
        let outputs = self.controls.keys().copied().collect::<Vec<_>>();
        let mut first_error = None;
        for output in outputs {
            match protocol.set_output_power(output, OutputPowerMode::On) {
                Ok(()) => {
                    if let Some(state) = self.controls.get_mut(&output) {
                        state.desired = Some(OutputPowerMode::On);
                    }
                }
                Err(error) if first_error.is_none() => first_error = Some(error),
                Err(_) => {}
            }
        }
        self.idle = false;
        first_error.map_or(Ok(()), Err)
    }
}

pub(super) trait OutputPowerProtocol {
    fn has_output_power(&self) -> bool;
    fn output_ids(&self) -> Vec<OutputId>;
    fn create_output_power(&mut self, output: OutputId) -> Result<(), RuntimeError>;
    fn set_output_power(
        &mut self,
        output: OutputId,
        mode: OutputPowerMode,
    ) -> Result<(), RuntimeError>;
    fn destroy_output_power(&mut self, output: OutputId) -> Result<(), RuntimeError>;
}

impl OutputPowerProtocol for Runtime {
    fn has_output_power(&self) -> bool {
        self.has_output_power()
    }

    fn output_ids(&self) -> Vec<OutputId> {
        self.outputs().into_iter().map(|info| info.id).collect()
    }

    fn create_output_power(&mut self, output: OutputId) -> Result<(), RuntimeError> {
        self.create_output_power(output)
    }

    fn set_output_power(
        &mut self,
        output: OutputId,
        mode: OutputPowerMode,
    ) -> Result<(), RuntimeError> {
        self.set_output_power(output, mode)
    }

    fn destroy_output_power(&mut self, output: OutputId) -> Result<(), RuntimeError> {
        self.destroy_output_power(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wayland_client_runtime::{LogicalPosition, LogicalSize, OutputInfo};

    #[derive(Default)]
    struct FakeProtocol {
        available: bool,
        outputs: Vec<OutputId>,
        fail_create: Option<OutputId>,
        fail_set: Option<(OutputId, OutputPowerMode)>,
        created: Vec<OutputId>,
        set: Vec<(OutputId, OutputPowerMode)>,
        destroyed: Vec<OutputId>,
    }

    impl OutputPowerProtocol for FakeProtocol {
        fn has_output_power(&self) -> bool {
            self.available
        }

        fn output_ids(&self) -> Vec<OutputId> {
            self.outputs.clone()
        }

        fn create_output_power(&mut self, output: OutputId) -> Result<(), RuntimeError> {
            if self.fail_create == Some(output) {
                return Err(RuntimeError::Protocol("injected create failure".into()));
            }
            self.created.push(output);
            Ok(())
        }

        fn set_output_power(
            &mut self,
            output: OutputId,
            mode: OutputPowerMode,
        ) -> Result<(), RuntimeError> {
            if self.fail_set == Some((output, mode)) {
                return Err(RuntimeError::Protocol("injected set failure".into()));
            }
            self.set.push((output, mode));
            Ok(())
        }

        fn destroy_output_power(&mut self, output: OutputId) -> Result<(), RuntimeError> {
            self.destroyed.push(output);
            Ok(())
        }
    }

    fn plan() -> IdlePlan {
        IdlePlan::compile(&crate::IdleConfig::default(), crate::PowerSource::Ac)
    }

    fn output(raw: u32) -> OutputId {
        OutputId::from_raw(raw)
    }

    fn updated(raw: u32) -> Event {
        Event::Output(OutputEvent::Updated(OutputInfo {
            id: output(raw),
            name: Some(format!("output-{raw}")),
            description: None,
            make: String::new(),
            model: String::new(),
            logical_position: Some(LogicalPosition::ZERO),
            logical_size: Some(LogicalSize::new(1920, 1080)),
            scale_factor: 1,
            refresh_mhz: Some(60_000),
        }))
    }

    #[test]
    fn partial_registration_rolls_back_created_controls() {
        let mut protocol = FakeProtocol {
            available: true,
            outputs: vec![output(1), output(2), output(3)],
            fail_create: Some(output(2)),
            ..FakeProtocol::default()
        };
        assert!(matches!(
            OutputPowerSet::register(&mut protocol, &plan()),
            Err(IdleRuntimeError::RegisterOutputPower { output: id, .. }) if id == output(2)
        ));
        assert_eq!(protocol.created, [output(1)]);
        assert_eq!(protocol.destroyed, [output(1)]);
    }

    #[test]
    fn duplicate_idle_events_do_not_repeat_requests_and_resume_restores_all() {
        let mut protocol = FakeProtocol {
            available: true,
            outputs: vec![output(1), output(2)],
            ..FakeProtocol::default()
        };
        let mut set = OutputPowerSet::register(&mut protocol, &plan()).unwrap();

        set.set_idle(&mut protocol, true).unwrap();
        set.set_idle(&mut protocol, true).unwrap();
        set.set_idle(&mut protocol, false).unwrap();

        assert_eq!(
            protocol.set,
            [
                (output(1), OutputPowerMode::Off),
                (output(2), OutputPowerMode::Off),
                (output(1), OutputPowerMode::On),
                (output(2), OutputPowerMode::On),
            ]
        );
    }

    #[test]
    fn hotplugged_output_inherits_active_monitor_off_policy_once() {
        let mut protocol = FakeProtocol {
            available: true,
            outputs: vec![output(1)],
            ..FakeProtocol::default()
        };
        let mut set = OutputPowerSet::register(&mut protocol, &plan()).unwrap();
        set.set_idle(&mut protocol, true).unwrap();
        let events = [updated(2), updated(2)];
        set.apply_batch(&mut protocol, events.iter()).unwrap();

        assert_eq!(protocol.created, [output(1), output(2)]);
        assert_eq!(
            protocol.set,
            [
                (output(1), OutputPowerMode::Off),
                (output(2), OutputPowerMode::Off),
            ]
        );
    }

    #[test]
    fn failed_control_restores_every_remaining_output() {
        let mut protocol = FakeProtocol {
            available: true,
            outputs: vec![output(1), output(2), output(3)],
            ..FakeProtocol::default()
        };
        let mut set = OutputPowerSet::register(&mut protocol, &plan()).unwrap();
        set.set_idle(&mut protocol, true).unwrap();
        protocol.set.clear();
        let events = [Event::OutputPower(OutputPowerEvent::Failed {
            output: output(2),
        })];

        assert!(matches!(
            set.apply_batch(&mut protocol, events.iter()),
            Err(IdleRuntimeError::OutputPowerFailed { output: id }) if id == output(2)
        ));
        assert_eq!(protocol.destroyed, [output(2)]);
        assert_eq!(
            protocol.set,
            [
                (output(1), OutputPowerMode::On),
                (output(3), OutputPowerMode::On),
            ]
        );
    }

    #[test]
    fn output_removal_relies_on_runtime_cleanup_and_is_not_destroyed_twice() {
        let mut protocol = FakeProtocol {
            available: true,
            outputs: vec![output(1), output(2)],
            ..FakeProtocol::default()
        };
        let mut set = OutputPowerSet::register(&mut protocol, &plan()).unwrap();
        let events = [Event::Output(OutputEvent::Removed(output(1)))];
        set.apply_batch(&mut protocol, events.iter()).unwrap();
        set.unregister(&mut protocol);

        assert_eq!(protocol.destroyed, [output(2)]);
    }
}
