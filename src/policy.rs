//! Performance policy decisions derived from desktop state.

use crate::config::{PerformanceConfig, PowerPolicy, ThrottlePolicy};
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
    if state.paused {
        return paused(DecisionReason::UserPaused);
    }
    if !desktop.session_active {
        return paused(DecisionReason::SessionInactive);
    }
    if let Some(output) = output {
        if !output.visible {
            return paused(DecisionReason::OutputHidden);
        }
        if output.has_fullscreen {
            return apply_throttle(
                config.fullscreen,
                config.interactive_max_fps,
                config.background_max_fps,
                DecisionReason::Fullscreen,
            );
        }
        if !output.focused {
            return apply_throttle(
                config.unfocused,
                config.interactive_max_fps,
                config.background_max_fps,
                DecisionReason::Unfocused,
            );
        }
    }
    if desktop.power.is_battery() {
        return match config.battery {
            PowerPolicy::Continue => {
                active(config.interactive_max_fps, DecisionReason::Interactive)
            }
            PowerPolicy::Throttle => throttled(config.battery_max_fps, DecisionReason::Battery),
            PowerPolicy::Pause => paused(DecisionReason::Battery),
        };
    }
    active(config.interactive_max_fps, DecisionReason::Interactive)
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
}
