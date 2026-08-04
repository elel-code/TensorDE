use super::*;

#[test]
fn adaptive_policy_can_pause_unfocused_output_when_configured() {
    let mut config = crate::config::TensorWallpaperConfig::default();
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
    let mut config = crate::config::TensorWallpaperConfig::default();
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
    let mut config = crate::config::TensorWallpaperConfig::default();
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
    let mut config = crate::config::TensorWallpaperConfig::default();
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
    let mut config = crate::config::TensorWallpaperConfig::default();
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
    let mut config = crate::config::TensorWallpaperConfig::default();
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
