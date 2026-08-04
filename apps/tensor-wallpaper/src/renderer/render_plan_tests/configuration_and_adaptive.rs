    // Config-default and adaptive render-plan decisions.

    #[test]
    fn config_default_wallpaper_builds_plan_for_desktop_output() {
        let mut config = TensorWallpaperConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/static-demo.gwpdir".to_owned());
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.errors.is_empty());
        assert_eq!(sync.plans[0].output_name, "eDP-1");
        assert_eq!(
            sync.decisions[0].wallpaper.as_deref(),
            Some("examples/wallpapers/static-demo.gwpdir")
        );
    }

    #[test]
    fn adaptive_snapshot_throttles_render_sync_decision() {
        let mut config = TensorWallpaperConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/static-demo.gwpdir".to_owned());
        config.adaptive.enabled = true;
        config.adaptive.throttle_max_fps = 15;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };
        let adaptive = crate::adaptive::AdaptiveSnapshot {
            monitoring_enabled: true,
            active_triggers: vec![crate::adaptive::AdaptiveTrigger {
                metric: crate::adaptive::AdaptiveMetric::CpuPressureSomeAvg10,
                value_x100: 9_000,
                threshold_x100: 7_500,
            }],
            ..crate::adaptive::AdaptiveSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config_and_adaptive(
            &config,
            &desktop,
            &state,
            std::env::temp_dir(),
            &adaptive,
        );

        assert_eq!(sync.plans.len(), 1);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Throttled);
        assert_eq!(sync.decisions[0].performance.max_fps, Some(15));
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Adaptive
        );
    }

    #[test]
    fn adaptive_pause_unfocused_removes_unfocused_output_from_render_plan() {
        let mut config = TensorWallpaperConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/static-demo.gwpdir".to_owned());
        config.adaptive.enabled = true;
        config.adaptive.action = crate::config::AdaptiveAction::PauseUnfocused;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                focused: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };
        let adaptive = crate::adaptive::AdaptiveSnapshot {
            monitoring_enabled: true,
            active_triggers: vec![crate::adaptive::AdaptiveTrigger {
                metric: crate::adaptive::AdaptiveMetric::CpuPressureSomeAvg10,
                value_x100: 9_000,
                threshold_x100: 7_500,
            }],
            ..crate::adaptive::AdaptiveSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config_and_adaptive(
            &config,
            &desktop,
            &state,
            std::env::temp_dir(),
            &adaptive,
        );

        assert!(sync.plans.is_empty());
        assert_eq!(sync.removals, vec!["eDP-1"]);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Adaptive
        );
    }
