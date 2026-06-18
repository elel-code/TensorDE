//! Performance policy decisions derived from desktop state.

use crate::adaptive::AdaptiveSnapshot;
use crate::config::{
    AdaptiveAction, DynamicPausePolicy, PerformanceConfig, PowerPolicy, ThrottlePolicy,
};
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
    SessionLocked,
    OutputHidden,
    Fullscreen,
    Unfocused,
    Battery,
    Adaptive,
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
        decision = select_more_conservative(
            decision,
            apply_dynamic_pause(
                config.session,
                config.interactive_max_fps,
                DecisionReason::SessionInactive,
            ),
        );
    }
    if desktop.session_locked {
        decision = select_more_conservative(
            decision,
            apply_dynamic_pause(
                config.session,
                config.interactive_max_fps,
                DecisionReason::SessionLocked,
            ),
        );
    }
    if let Some(output) = output {
        if !output.visible {
            decision = select_more_conservative(
                decision,
                apply_dynamic_pause(
                    config.hidden,
                    config.interactive_max_fps,
                    DecisionReason::OutputHidden,
                ),
            );
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
                PowerPolicy::Continue | PowerPolicy::PauseDynamic => {
                    active(config.interactive_max_fps, DecisionReason::Interactive)
                }
                PowerPolicy::Throttle => throttled(config.battery_max_fps, DecisionReason::Battery),
                PowerPolicy::Pause => paused(DecisionReason::Battery),
            },
        );
    }
    decision
}

pub fn apply_power_dynamic_policy(
    decision: PerformanceDecision,
    config: &PerformanceConfig,
    desktop: &DesktopSnapshot,
    dynamic_wallpaper: bool,
) -> PerformanceDecision {
    if !dynamic_wallpaper
        || !desktop.power.is_battery()
        || config.battery != PowerPolicy::PauseDynamic
    {
        return decision;
    }

    select_more_conservative(decision, paused(DecisionReason::Battery))
}

pub fn apply_desktop_dynamic_policy(
    decision: PerformanceDecision,
    config: &PerformanceConfig,
    desktop: &DesktopSnapshot,
    output: Option<&DesktopOutput>,
    dynamic_wallpaper: bool,
) -> PerformanceDecision {
    if !dynamic_wallpaper {
        return decision;
    }

    let mut dynamic_decision = decision.clone();
    if !desktop.session_active && config.session == DynamicPausePolicy::PauseDynamic {
        dynamic_decision =
            select_more_conservative(dynamic_decision, paused(DecisionReason::SessionInactive));
    }
    if desktop.session_locked && config.session == DynamicPausePolicy::PauseDynamic {
        dynamic_decision =
            select_more_conservative(dynamic_decision, paused(DecisionReason::SessionLocked));
    }
    if output.is_some_and(|output| !output.visible)
        && config.hidden == DynamicPausePolicy::PauseDynamic
    {
        dynamic_decision =
            select_more_conservative(dynamic_decision, paused(DecisionReason::OutputHidden));
    }
    if output.is_some_and(|output| output.has_fullscreen)
        && config.fullscreen == ThrottlePolicy::PauseDynamic
    {
        dynamic_decision =
            select_more_conservative(dynamic_decision, paused(DecisionReason::Fullscreen));
    }
    if output.is_some_and(|output| !output.focused)
        && config.unfocused == ThrottlePolicy::PauseDynamic
    {
        dynamic_decision =
            select_more_conservative(dynamic_decision, paused(DecisionReason::Unfocused));
    }
    dynamic_decision
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

pub fn apply_adaptive_policy(
    decision: PerformanceDecision,
    config: &crate::config::GilderConfig,
    output_name: &str,
    output: Option<&DesktopOutput>,
    snapshot: &AdaptiveSnapshot,
) -> PerformanceDecision {
    if !snapshot.affects_render_plan() || !crate::adaptive::output_enabled(config, output_name) {
        return decision;
    }

    let candidate = match crate::adaptive::output_action(config, output_name) {
        AdaptiveAction::Throttle => throttled(
            crate::adaptive::output_throttle_max_fps(config, output_name),
            DecisionReason::Adaptive,
        ),
        AdaptiveAction::PauseUnfocused if output.is_some_and(|output| !output.focused) => {
            paused(DecisionReason::Adaptive)
        }
        AdaptiveAction::PauseUnfocused => throttled(
            crate::adaptive::output_throttle_max_fps(config, output_name),
            DecisionReason::Adaptive,
        ),
        AdaptiveAction::PauseDynamic => decision.clone(),
    };

    select_more_conservative(decision, candidate)
}

pub fn apply_adaptive_dynamic_policy(
    decision: PerformanceDecision,
    config: &crate::config::GilderConfig,
    output_name: &str,
    snapshot: &AdaptiveSnapshot,
    dynamic_wallpaper: bool,
) -> PerformanceDecision {
    if !dynamic_wallpaper
        || !snapshot.affects_render_plan()
        || !crate::adaptive::output_enabled(config, output_name)
        || crate::adaptive::output_action(config, output_name) != AdaptiveAction::PauseDynamic
    {
        return decision;
    }

    select_more_conservative(decision, paused(DecisionReason::Adaptive))
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
        ThrottlePolicy::Continue | ThrottlePolicy::PauseDynamic => {
            active(active_fps, DecisionReason::Interactive)
        }
        ThrottlePolicy::Throttle => throttled(throttle_fps, reason),
        ThrottlePolicy::Pause => paused(reason),
    }
}

fn apply_dynamic_pause(
    policy: DynamicPausePolicy,
    active_fps: u32,
    reason: DecisionReason,
) -> PerformanceDecision {
    match policy {
        DynamicPausePolicy::Continue | DynamicPausePolicy::PauseDynamic => {
            active(active_fps, DecisionReason::Interactive)
        }
        DynamicPausePolicy::Pause => paused(reason),
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
    fn pauses_when_session_is_locked() {
        let config = PerformanceConfig::default();
        let desktop = DesktopSnapshot {
            session_locked: true,
            ..DesktopSnapshot::default()
        };
        let decision = decide_performance(&config, &desktop, None, &OutputState::default());
        assert_eq!(decision.mode, RenderMode::Paused);
        assert_eq!(decision.reason, DecisionReason::SessionLocked);
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
    fn battery_pause_dynamic_waits_until_wallpaper_type_is_known() {
        let config = PerformanceConfig {
            battery: PowerPolicy::PauseDynamic,
            ..PerformanceConfig::default()
        };
        let desktop = DesktopSnapshot {
            power: PowerState::Battery,
            ..DesktopSnapshot::default()
        };

        let decision = decide_performance(&config, &desktop, None, &OutputState::default());

        assert_eq!(decision.mode, RenderMode::Active);
        assert_eq!(decision.reason, DecisionReason::Interactive);
    }

    #[test]
    fn battery_pause_dynamic_pauses_dynamic_wallpaper_after_manifest_load() {
        let config = PerformanceConfig {
            battery: PowerPolicy::PauseDynamic,
            ..PerformanceConfig::default()
        };
        let desktop = DesktopSnapshot {
            power: PowerState::Battery,
            ..DesktopSnapshot::default()
        };
        let base = active(60, DecisionReason::Interactive);

        let decision = apply_power_dynamic_policy(base, &config, &desktop, true);

        assert_eq!(decision.mode, RenderMode::Paused);
        assert_eq!(decision.reason, DecisionReason::Battery);
    }

    #[test]
    fn battery_pause_dynamic_leaves_static_wallpaper_policy_unchanged() {
        let config = PerformanceConfig {
            battery: PowerPolicy::PauseDynamic,
            ..PerformanceConfig::default()
        };
        let desktop = DesktopSnapshot {
            power: PowerState::Battery,
            ..DesktopSnapshot::default()
        };
        let base = active(60, DecisionReason::Interactive);

        let decision = apply_power_dynamic_policy(base, &config, &desktop, false);

        assert_eq!(decision.mode, RenderMode::Active);
        assert_eq!(decision.reason, DecisionReason::Interactive);
    }

    #[test]
    fn hidden_pause_dynamic_waits_until_wallpaper_type_is_known() {
        let config = PerformanceConfig {
            hidden: DynamicPausePolicy::PauseDynamic,
            ..PerformanceConfig::default()
        };
        let output = DesktopOutput {
            visible: false,
            ..DesktopOutput::virtual_output("eDP-1")
        };

        let decision = decide_performance(
            &config,
            &DesktopSnapshot::default(),
            Some(&output),
            &OutputState::default(),
        );

        assert_eq!(decision.mode, RenderMode::Active);
        assert_eq!(decision.reason, DecisionReason::Interactive);
    }

    #[test]
    fn fullscreen_pause_dynamic_waits_until_wallpaper_type_is_known() {
        let config = PerformanceConfig {
            fullscreen: ThrottlePolicy::PauseDynamic,
            ..PerformanceConfig::default()
        };
        let output = DesktopOutput {
            has_fullscreen: true,
            ..DesktopOutput::virtual_output("eDP-1")
        };

        let decision = decide_performance(
            &config,
            &DesktopSnapshot::default(),
            Some(&output),
            &OutputState::default(),
        );

        assert_eq!(decision.mode, RenderMode::Active);
        assert_eq!(decision.reason, DecisionReason::Interactive);
    }

    #[test]
    fn fullscreen_pause_dynamic_pauses_dynamic_wallpaper_after_manifest_load() {
        let config = PerformanceConfig {
            fullscreen: ThrottlePolicy::PauseDynamic,
            ..PerformanceConfig::default()
        };
        let output = DesktopOutput {
            has_fullscreen: true,
            ..DesktopOutput::virtual_output("eDP-1")
        };
        let base = active(60, DecisionReason::Interactive);

        let decision = apply_desktop_dynamic_policy(
            base,
            &config,
            &DesktopSnapshot::default(),
            Some(&output),
            true,
        );

        assert_eq!(decision.mode, RenderMode::Paused);
        assert_eq!(decision.reason, DecisionReason::Fullscreen);
    }

    #[test]
    fn unfocused_pause_dynamic_waits_until_wallpaper_type_is_known() {
        let config = PerformanceConfig {
            unfocused: ThrottlePolicy::PauseDynamic,
            ..PerformanceConfig::default()
        };
        let output = DesktopOutput {
            focused: false,
            ..DesktopOutput::virtual_output("eDP-1")
        };

        let decision = decide_performance(
            &config,
            &DesktopSnapshot::default(),
            Some(&output),
            &OutputState::default(),
        );

        assert_eq!(decision.mode, RenderMode::Active);
        assert_eq!(decision.reason, DecisionReason::Interactive);
    }

    #[test]
    fn unfocused_pause_dynamic_pauses_dynamic_wallpaper_after_manifest_load() {
        let config = PerformanceConfig {
            unfocused: ThrottlePolicy::PauseDynamic,
            ..PerformanceConfig::default()
        };
        let output = DesktopOutput {
            focused: false,
            ..DesktopOutput::virtual_output("eDP-1")
        };
        let base = active(60, DecisionReason::Interactive);

        let decision = apply_desktop_dynamic_policy(
            base,
            &config,
            &DesktopSnapshot::default(),
            Some(&output),
            true,
        );

        assert_eq!(decision.mode, RenderMode::Paused);
        assert_eq!(decision.reason, DecisionReason::Unfocused);
    }

    #[test]
    fn hidden_pause_dynamic_pauses_dynamic_wallpaper_after_manifest_load() {
        let config = PerformanceConfig {
            hidden: DynamicPausePolicy::PauseDynamic,
            ..PerformanceConfig::default()
        };
        let output = DesktopOutput {
            visible: false,
            ..DesktopOutput::virtual_output("eDP-1")
        };
        let base = active(60, DecisionReason::Interactive);

        let decision = apply_desktop_dynamic_policy(
            base,
            &config,
            &DesktopSnapshot::default(),
            Some(&output),
            true,
        );

        assert_eq!(decision.mode, RenderMode::Paused);
        assert_eq!(decision.reason, DecisionReason::OutputHidden);
    }

    #[test]
    fn hidden_pause_dynamic_leaves_static_wallpaper_policy_unchanged() {
        let config = PerformanceConfig {
            hidden: DynamicPausePolicy::PauseDynamic,
            ..PerformanceConfig::default()
        };
        let output = DesktopOutput {
            visible: false,
            ..DesktopOutput::virtual_output("eDP-1")
        };
        let base = active(60, DecisionReason::Interactive);

        let decision = apply_desktop_dynamic_policy(
            base,
            &config,
            &DesktopSnapshot::default(),
            Some(&output),
            false,
        );

        assert_eq!(decision.mode, RenderMode::Active);
        assert_eq!(decision.reason, DecisionReason::Interactive);
    }

    #[test]
    fn session_pause_dynamic_pauses_dynamic_wallpaper_after_manifest_load() {
        let config = PerformanceConfig {
            session: DynamicPausePolicy::PauseDynamic,
            ..PerformanceConfig::default()
        };
        let desktop = DesktopSnapshot {
            session_active: false,
            ..DesktopSnapshot::default()
        };
        let base = active(60, DecisionReason::Interactive);

        let decision = apply_desktop_dynamic_policy(base, &config, &desktop, None, true);

        assert_eq!(decision.mode, RenderMode::Paused);
        assert_eq!(decision.reason, DecisionReason::SessionInactive);
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

    #[test]
    fn adaptive_policy_throttles_when_enabled_and_triggered() {
        let mut config = crate::config::GilderConfig::default();
        config.adaptive.enabled = true;
        config.adaptive.throttle_max_fps = 15;
        let base = active(60, DecisionReason::Interactive);
        let snapshot = AdaptiveSnapshot {
            monitoring_enabled: true,
            active_triggers: vec![crate::adaptive::AdaptiveTrigger {
                metric: crate::adaptive::AdaptiveMetric::CpuPressureSomeAvg10,
                value_x100: 9_000,
                threshold_x100: 7_500,
            }],
            ..AdaptiveSnapshot::default()
        };

        let decision = apply_adaptive_policy(base, &config, "eDP-1", None, &snapshot);

        assert_eq!(decision.mode, RenderMode::Throttled);
        assert_eq!(decision.max_fps, Some(15));
        assert_eq!(decision.reason, DecisionReason::Adaptive);
    }

    #[test]
    fn adaptive_policy_cannot_override_stronger_pause() {
        let mut config = crate::config::GilderConfig::default();
        config.adaptive.enabled = true;
        let snapshot = AdaptiveSnapshot {
            monitoring_enabled: true,
            active_triggers: vec![crate::adaptive::AdaptiveTrigger {
                metric: crate::adaptive::AdaptiveMetric::MemoryPressureSomeAvg10,
                value_x100: 2_500,
                threshold_x100: 2_000,
            }],
            ..AdaptiveSnapshot::default()
        };
        let base = paused(DecisionReason::UserPaused);

        let decision = apply_adaptive_policy(base, &config, "eDP-1", None, &snapshot);

        assert_eq!(decision.mode, RenderMode::Paused);
        assert_eq!(decision.reason, DecisionReason::UserPaused);
    }

    #[test]
    fn adaptive_policy_can_pause_unfocused_output_when_configured() {
        let mut config = crate::config::GilderConfig::default();
        config.adaptive.enabled = true;
        config.adaptive.action = AdaptiveAction::PauseUnfocused;
        let snapshot = AdaptiveSnapshot {
            monitoring_enabled: true,
            active_triggers: vec![crate::adaptive::AdaptiveTrigger {
                metric: crate::adaptive::AdaptiveMetric::CpuPressureSomeAvg10,
                value_x100: 9_000,
                threshold_x100: 7_500,
            }],
            ..AdaptiveSnapshot::default()
        };
        let base = active(60, DecisionReason::Interactive);
        let output = DesktopOutput {
            focused: false,
            ..DesktopOutput::virtual_output("eDP-1")
        };

        let decision = apply_adaptive_policy(base, &config, "eDP-1", Some(&output), &snapshot);

        assert_eq!(decision.mode, RenderMode::Paused);
        assert_eq!(decision.reason, DecisionReason::Adaptive);
    }

    #[test]
    fn adaptive_pause_unfocused_falls_back_to_throttle_for_focused_output() {
        let mut config = crate::config::GilderConfig::default();
        config.adaptive.enabled = true;
        config.adaptive.action = AdaptiveAction::PauseUnfocused;
        config.adaptive.throttle_max_fps = 12;
        let snapshot = AdaptiveSnapshot {
            monitoring_enabled: true,
            active_triggers: vec![crate::adaptive::AdaptiveTrigger {
                metric: crate::adaptive::AdaptiveMetric::CpuPressureSomeAvg10,
                value_x100: 9_000,
                threshold_x100: 7_500,
            }],
            ..AdaptiveSnapshot::default()
        };
        let base = active(60, DecisionReason::Interactive);
        let output = DesktopOutput::virtual_output("eDP-1");

        let decision = apply_adaptive_policy(base, &config, "eDP-1", Some(&output), &snapshot);

        assert_eq!(decision.mode, RenderMode::Throttled);
        assert_eq!(decision.max_fps, Some(12));
        assert_eq!(decision.reason, DecisionReason::Adaptive);
    }

    #[test]
    fn adaptive_pause_dynamic_does_not_change_generic_policy_before_manifest_load() {
        let mut config = crate::config::GilderConfig::default();
        config.adaptive.enabled = true;
        config.adaptive.action = AdaptiveAction::PauseDynamic;
        let snapshot = AdaptiveSnapshot {
            monitoring_enabled: true,
            active_triggers: vec![crate::adaptive::AdaptiveTrigger {
                metric: crate::adaptive::AdaptiveMetric::CpuPressureSomeAvg10,
                value_x100: 9_000,
                threshold_x100: 7_500,
            }],
            ..AdaptiveSnapshot::default()
        };
        let base = active(60, DecisionReason::Interactive);

        let decision = apply_adaptive_policy(base, &config, "eDP-1", None, &snapshot);

        assert_eq!(decision.mode, RenderMode::Active);
        assert_eq!(decision.reason, DecisionReason::Interactive);
    }

    #[test]
    fn adaptive_pause_dynamic_pauses_dynamic_wallpaper_after_manifest_load() {
        let mut config = crate::config::GilderConfig::default();
        config.adaptive.enabled = true;
        config.adaptive.action = AdaptiveAction::PauseDynamic;
        let snapshot = AdaptiveSnapshot {
            monitoring_enabled: true,
            active_triggers: vec![crate::adaptive::AdaptiveTrigger {
                metric: crate::adaptive::AdaptiveMetric::CpuPressureSomeAvg10,
                value_x100: 9_000,
                threshold_x100: 7_500,
            }],
            ..AdaptiveSnapshot::default()
        };
        let base = active(60, DecisionReason::Interactive);

        let decision = apply_adaptive_dynamic_policy(base, &config, "eDP-1", &snapshot, true);

        assert_eq!(decision.mode, RenderMode::Paused);
        assert_eq!(decision.reason, DecisionReason::Adaptive);
    }

    #[test]
    fn adaptive_pause_dynamic_leaves_static_wallpaper_policy_unchanged() {
        let mut config = crate::config::GilderConfig::default();
        config.adaptive.enabled = true;
        config.adaptive.action = AdaptiveAction::PauseDynamic;
        let snapshot = AdaptiveSnapshot {
            monitoring_enabled: true,
            active_triggers: vec![crate::adaptive::AdaptiveTrigger {
                metric: crate::adaptive::AdaptiveMetric::CpuPressureSomeAvg10,
                value_x100: 9_000,
                threshold_x100: 7_500,
            }],
            ..AdaptiveSnapshot::default()
        };
        let base = active(60, DecisionReason::Interactive);

        let decision = apply_adaptive_dynamic_policy(base, &config, "eDP-1", &snapshot, false);

        assert_eq!(decision.mode, RenderMode::Active);
        assert_eq!(decision.reason, DecisionReason::Interactive);
    }

    #[test]
    fn adaptive_policy_can_be_disabled_per_output() {
        let mut config = crate::config::GilderConfig::default();
        config.adaptive.enabled = true;
        config.outputs.insert(
            "eDP-1".to_owned(),
            crate::config::OutputConfig {
                adaptive: crate::config::OutputAdaptiveConfig {
                    enabled: Some(false),
                    throttle_max_fps: None,
                    action: None,
                },
                ..crate::config::OutputConfig::default()
            },
        );
        let snapshot = AdaptiveSnapshot {
            monitoring_enabled: true,
            active_triggers: vec![crate::adaptive::AdaptiveTrigger {
                metric: crate::adaptive::AdaptiveMetric::CpuPressureSomeAvg10,
                value_x100: 9_000,
                threshold_x100: 7_500,
            }],
            ..AdaptiveSnapshot::default()
        };
        let base = active(60, DecisionReason::Interactive);

        let decision = apply_adaptive_policy(base, &config, "eDP-1", None, &snapshot);

        assert_eq!(decision.mode, RenderMode::Active);
        assert_eq!(decision.reason, DecisionReason::Interactive);
    }
}
