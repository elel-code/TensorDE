//! Performance policy decisions derived from desktop state.

use crate::config::{PerformanceConfig, PowerPolicy, ThrottlePolicy};
use crate::core::RuntimePolicy;
use crate::desktop::{DesktopOutput, DesktopSnapshot};
use crate::state::OutputState;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerformanceDecision {
    pub mode: RenderMode,
    pub max_fps: Option<u32>,
    pub reason: DecisionReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RenderMode {
    Active,
    Throttled,
    Paused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DecisionReason {
    Interactive,
    UserPaused,
    SessionInactive,
    OutputHidden,
    Fullscreen,
    Unfocused,
    Battery,
}

pub fn decide_performance(
    config: &PerformanceConfig,
    desktop: &DesktopSnapshot,
    output: Option<&DesktopOutput>,
    state: &OutputState,
) -> PerformanceDecision {
    let mut decision = active(config.interactive_max_fps, DecisionReason::Interactive);
    if state.paused {
        decision = select_more_conservative(decision, paused(DecisionReason::UserPaused));
    }
    if !desktop.session_active {
        decision = select_more_conservative(decision, paused(DecisionReason::SessionInactive));
    }
    if let Some(output) = output {
        if !output.visible {
            decision = select_more_conservative(decision, paused(DecisionReason::OutputHidden));
        }
        if output.has_fullscreen {
            decision = select_more_conservative(
                decision,
                apply_throttle(
                    config.fullscreen,
                    config.interactive_max_fps,
                    config.background_max_fps,
                    DecisionReason::Fullscreen,
                ),
            );
        }
        if !output.focused {
            decision = select_more_conservative(
                decision,
                apply_throttle(
                    config.unfocused,
                    config.interactive_max_fps,
                    config.background_max_fps,
                    DecisionReason::Unfocused,
                ),
            );
        }
    }
    if desktop.power.is_battery() {
        decision = select_more_conservative(
            decision,
            match config.battery {
                PowerPolicy::Continue => {
                    active(config.interactive_max_fps, DecisionReason::Interactive)
                }
                PowerPolicy::Throttle => throttled(config.battery_max_fps, DecisionReason::Battery),
                PowerPolicy::Pause => paused(DecisionReason::Battery),
            },
        );
    }
    decision
}

pub fn apply_runtime_policy(
    mut decision: PerformanceDecision,
    runtime: &RuntimePolicy,
    output: Option<&DesktopOutput>,
) -> PerformanceDecision {
    let Some(output) = output else {
        return decision;
    };

    if output.has_fullscreen && runtime.pause_when_fullscreen {
        decision = select_more_conservative(decision, paused(DecisionReason::Fullscreen));
    }
    if !output.focused && runtime.pause_when_unfocused {
        decision = select_more_conservative(decision, paused(DecisionReason::Unfocused));
    }
    decision
}

fn select_more_conservative(
    current: PerformanceDecision,
    candidate: PerformanceDecision,
) -> PerformanceDecision {
    if decision_rank(&candidate) > decision_rank(&current) {
        candidate
    } else {
        current
    }
}

fn decision_rank(decision: &PerformanceDecision) -> (u8, std::cmp::Reverse<u32>) {
    match decision.mode {
        RenderMode::Paused => (2, std::cmp::Reverse(0)),
        RenderMode::Throttled => (1, std::cmp::Reverse(decision.max_fps.unwrap_or(u32::MAX))),
        RenderMode::Active => (0, std::cmp::Reverse(decision.max_fps.unwrap_or(u32::MAX))),
    }
}

fn apply_throttle(
    policy: ThrottlePolicy,
    active_fps: u32,
    throttle_fps: u32,
    reason: DecisionReason,
) -> PerformanceDecision {
    match policy {
        ThrottlePolicy::Continue => active(active_fps, DecisionReason::Interactive),
        ThrottlePolicy::Throttle => throttled(throttle_fps, reason),
        ThrottlePolicy::Pause => paused(reason),
    }
}

fn active(max_fps: u32, reason: DecisionReason) -> PerformanceDecision {
    PerformanceDecision {
        mode: RenderMode::Active,
        max_fps: Some(max_fps),
        reason,
    }
}

fn throttled(max_fps: u32, reason: DecisionReason) -> PerformanceDecision {
    PerformanceDecision {
        mode: RenderMode::Throttled,
        max_fps: Some(max_fps),
        reason,
    }
}

fn paused(reason: DecisionReason) -> PerformanceDecision {
    PerformanceDecision {
        mode: RenderMode::Paused,
        max_fps: None,
        reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::{DesktopOutput, PowerState};

    #[test]
    fn pauses_when_user_paused() {
        let config = PerformanceConfig::default();
        let desktop = DesktopSnapshot::default();
        let state = OutputState {
            paused: true,
            ..OutputState::default()
        };
        let decision = decide_performance(&config, &desktop, None, &state);
        assert_eq!(decision.mode, RenderMode::Paused);
        assert_eq!(decision.reason, DecisionReason::UserPaused);
    }

    #[test]
    fn pauses_for_fullscreen_by_default() {
        let config = PerformanceConfig::default();
        let desktop = DesktopSnapshot::default();
        let output = DesktopOutput {
            has_fullscreen: true,
            ..DesktopOutput::virtual_output("eDP-1")
        };
        let decision =
            decide_performance(&config, &desktop, Some(&output), &OutputState::default());
        assert_eq!(decision.mode, RenderMode::Paused);
        assert_eq!(decision.reason, DecisionReason::Fullscreen);
    }

    #[test]
    fn pauses_when_session_is_inactive() {
        let config = PerformanceConfig::default();
        let desktop = DesktopSnapshot {
            session_active: false,
            ..DesktopSnapshot::default()
        };
        let decision = decide_performance(&config, &desktop, None, &OutputState::default());
        assert_eq!(decision.mode, RenderMode::Paused);
        assert_eq!(decision.reason, DecisionReason::SessionInactive);
    }

    #[test]
    fn throttles_on_battery() {
        let config = PerformanceConfig::default();
        let desktop = DesktopSnapshot {
            power: PowerState::Battery,
            ..DesktopSnapshot::default()
        };
        let decision = decide_performance(&config, &desktop, None, &OutputState::default());
        assert_eq!(decision.mode, RenderMode::Throttled);
        assert_eq!(decision.max_fps, Some(config.battery_max_fps));
    }

    #[test]
    fn battery_throttle_uses_lower_fps_than_unfocused_throttle() {
        let config = PerformanceConfig {
            background_max_fps: 30,
            battery_max_fps: 12,
            ..PerformanceConfig::default()
        };
        let desktop = DesktopSnapshot {
            power: PowerState::Battery,
            ..DesktopSnapshot::default()
        };
        let output = DesktopOutput {
            focused: false,
            ..DesktopOutput::virtual_output("eDP-1")
        };

        let decision =
            decide_performance(&config, &desktop, Some(&output), &OutputState::default());

        assert_eq!(decision.mode, RenderMode::Throttled);
        assert_eq!(decision.max_fps, Some(12));
        assert_eq!(decision.reason, DecisionReason::Battery);
    }

    #[test]
    fn unfocused_throttle_can_remain_stricter_than_battery_throttle() {
        let config = PerformanceConfig {
            background_max_fps: 10,
            battery_max_fps: 24,
            ..PerformanceConfig::default()
        };
        let desktop = DesktopSnapshot {
            power: PowerState::Battery,
            ..DesktopSnapshot::default()
        };
        let output = DesktopOutput {
            focused: false,
            ..DesktopOutput::virtual_output("eDP-1")
        };

        let decision =
            decide_performance(&config, &desktop, Some(&output), &OutputState::default());

        assert_eq!(decision.mode, RenderMode::Throttled);
        assert_eq!(decision.max_fps, Some(10));
        assert_eq!(decision.reason, DecisionReason::Unfocused);
    }

    #[test]
    fn battery_pause_overrides_unfocused_throttle() {
        let config = PerformanceConfig {
            battery: PowerPolicy::Pause,
            ..PerformanceConfig::default()
        };
        let desktop = DesktopSnapshot {
            power: PowerState::Battery,
            ..DesktopSnapshot::default()
        };
        let output = DesktopOutput {
            focused: false,
            ..DesktopOutput::virtual_output("eDP-1")
        };

        let decision =
            decide_performance(&config, &desktop, Some(&output), &OutputState::default());

        assert_eq!(decision.mode, RenderMode::Paused);
        assert_eq!(decision.reason, DecisionReason::Battery);
    }

    #[test]
    fn equal_pause_keeps_earlier_higher_priority_reason() {
        let config = PerformanceConfig {
            battery: PowerPolicy::Pause,
            ..PerformanceConfig::default()
        };
        let desktop = DesktopSnapshot {
            power: PowerState::Battery,
            ..DesktopSnapshot::default()
        };
        let output = DesktopOutput {
            has_fullscreen: true,
            ..DesktopOutput::virtual_output("eDP-1")
        };

        let decision =
            decide_performance(&config, &desktop, Some(&output), &OutputState::default());

        assert_eq!(decision.mode, RenderMode::Paused);
        assert_eq!(decision.reason, DecisionReason::Fullscreen);
    }

    #[test]
    fn runtime_policy_can_pause_unfocused_wallpaper() {
        let config = PerformanceConfig {
            unfocused: ThrottlePolicy::Continue,
            ..PerformanceConfig::default()
        };
        let output = DesktopOutput {
            focused: false,
            ..DesktopOutput::virtual_output("eDP-1")
        };
        let base = decide_performance(
            &config,
            &DesktopSnapshot::default(),
            Some(&output),
            &OutputState::default(),
        );

        let decision = apply_runtime_policy(
            base,
            &RuntimePolicy {
                pause_when_unfocused: true,
                ..RuntimePolicy::default()
            },
            Some(&output),
        );

        assert_eq!(decision.mode, RenderMode::Paused);
        assert_eq!(decision.reason, DecisionReason::Unfocused);
    }

    #[test]
    fn runtime_policy_does_not_make_user_pause_less_conservative() {
        let output = DesktopOutput {
            focused: false,
            ..DesktopOutput::virtual_output("eDP-1")
        };
        let base = PerformanceDecision {
            mode: RenderMode::Paused,
            max_fps: None,
            reason: DecisionReason::UserPaused,
        };

        let decision = apply_runtime_policy(base, &RuntimePolicy::default(), Some(&output));

        assert_eq!(decision.mode, RenderMode::Paused);
        assert_eq!(decision.reason, DecisionReason::UserPaused);
    }
}
