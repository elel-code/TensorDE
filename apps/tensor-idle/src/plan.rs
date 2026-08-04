use crate::IdleConfig;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PowerSource {
    Ac,
    Battery,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum IdleAction {
    MonitorOff,
    Lock,
    Suspend,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IdleStage {
    pub after_ms: u32,
    pub action: IdleAction,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdlePlan {
    pub enabled: bool,
    pub respect_inhibitors: bool,
    pub stages: Vec<IdleStage>,
    pub post_lock_monitor_off_after_ms: Option<u32>,
}

impl IdlePlan {
    pub fn compile(config: &IdleConfig, source: PowerSource) -> Self {
        let policy = match source {
            PowerSource::Ac => config.ac,
            PowerSource::Battery => config.battery,
        };
        let mut stages = Vec::with_capacity(3);
        push_stage(
            &mut stages,
            policy.monitor_off_after_ms,
            IdleAction::MonitorOff,
        );
        push_stage(&mut stages, policy.lock_after_ms, IdleAction::Lock);
        push_stage(&mut stages, policy.suspend_after_ms, IdleAction::Suspend);
        stages.sort_unstable_by_key(|stage| (stage.after_ms, stage.action));
        Self {
            enabled: config.enabled,
            respect_inhibitors: config.respect_inhibitors,
            stages,
            post_lock_monitor_off_after_ms: policy.post_lock_monitor_off_after_ms,
        }
    }
}

fn push_stage(stages: &mut Vec<IdleStage>, after_ms: Option<u32>, action: IdleAction) {
    if let Some(after_ms) = after_ms {
        stages.push(IdleStage { after_ms, action });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PowerPolicy;

    #[test]
    fn plan_is_sorted_and_power_source_specific() {
        let config = IdleConfig {
            battery: PowerPolicy {
                monitor_off_after_ms: Some(300_000),
                lock_after_ms: Some(120_000),
                suspend_after_ms: None,
                post_lock_monitor_off_after_ms: Some(5_000),
            },
            ..IdleConfig::default()
        };
        let plan = IdlePlan::compile(&config, PowerSource::Battery);
        assert_eq!(
            plan.stages,
            [
                IdleStage {
                    after_ms: 120_000,
                    action: IdleAction::Lock,
                },
                IdleStage {
                    after_ms: 300_000,
                    action: IdleAction::MonitorOff,
                },
            ]
        );
        assert_eq!(plan.post_lock_monitor_off_after_ms, Some(5_000));
    }

    #[test]
    fn disabled_policy_retains_validated_stages_for_live_enable() {
        let config = IdleConfig {
            enabled: false,
            ..IdleConfig::default()
        };
        let plan = IdlePlan::compile(&config, PowerSource::Ac);
        assert!(!plan.enabled);
        assert_eq!(plan.stages.len(), 3);
    }
}
