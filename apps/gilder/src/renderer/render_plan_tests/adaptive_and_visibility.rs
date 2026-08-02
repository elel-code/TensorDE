    #[test]
    fn adaptive_pause_dynamic_removes_slideshow_from_render_plan() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/slideshow-demo.gwpdir".to_owned());
        config.adaptive.enabled = true;
        config.adaptive.action = crate::config::AdaptiveAction::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };
        let adaptive = adaptive_cpu_pressure_snapshot();

        let sync = static_render_sync_plan_with_config_and_adaptive(
            &config,
            &desktop,
            &state,
            std::env::temp_dir(),
            &adaptive,
        );

        assert!(sync.plans.is_empty());
        assert!(sync.slideshow_plans.is_empty());
        assert_eq!(sync.removals, vec!["eDP-1"]);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Adaptive
        );
    }

    #[test]
    fn adaptive_pause_dynamic_keeps_static_wallpaper_renderable() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/static-demo.gwpdir".to_owned());
        config.adaptive.enabled = true;
        config.adaptive.action = crate::config::AdaptiveAction::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };
        let adaptive = adaptive_cpu_pressure_snapshot();

        let sync = static_render_sync_plan_with_config_and_adaptive(
            &config,
            &desktop,
            &state,
            std::env::temp_dir(),
            &adaptive,
        );

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.removals.is_empty());
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Active);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Interactive
        );
    }

    #[test]
    fn battery_pause_dynamic_removes_slideshow_from_render_plan() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/slideshow-demo.gwpdir".to_owned());
        config.performance.battery = PowerPolicy::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            power: PowerState::Battery,
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert!(sync.plans.is_empty());
        assert!(sync.video_plans.is_empty());
        assert!(sync.slideshow_plans.is_empty());
        assert_eq!(sync.removals, vec!["eDP-1"]);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Battery
        );
    }

    #[test]
    fn battery_pause_dynamic_keeps_static_wallpaper_renderable() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/static-demo.gwpdir".to_owned());
        config.performance.battery = PowerPolicy::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            power: PowerState::Battery,
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.removals.is_empty());
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Active);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Interactive
        );
    }

    #[test]
    fn hidden_pause_dynamic_removes_slideshow_from_render_plan() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/slideshow-demo.gwpdir".to_owned());
        config.performance.hidden = DynamicPausePolicy::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                visible: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert!(sync.plans.is_empty());
        assert!(sync.video_plans.is_empty());
        assert!(sync.slideshow_plans.is_empty());
        assert_eq!(sync.removals, vec!["eDP-1"]);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::OutputHidden
        );
    }

    #[test]
    fn hidden_pause_dynamic_keeps_static_wallpaper_renderable() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/static-demo.gwpdir".to_owned());
        config.performance.hidden = DynamicPausePolicy::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                visible: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.removals.is_empty());
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Active);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Interactive
        );
    }

    #[test]
    fn session_pause_dynamic_removes_slideshow_from_render_plan() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/slideshow-demo.gwpdir".to_owned());
        config.performance.session = DynamicPausePolicy::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            session_active: false,
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert!(sync.plans.is_empty());
        assert!(sync.slideshow_plans.is_empty());
        assert_eq!(sync.removals, vec!["eDP-1"]);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::SessionInactive
        );
    }

    #[test]
    fn session_pause_dynamic_keeps_static_wallpaper_renderable() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/static-demo.gwpdir".to_owned());
        config.performance.session = DynamicPausePolicy::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            session_locked: true,
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.removals.is_empty());
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Active);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Interactive
        );
    }

    #[test]
    fn unfocused_pause_dynamic_removes_slideshow_from_render_plan() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/slideshow-demo.gwpdir".to_owned());
        config.performance.unfocused = crate::config::ThrottlePolicy::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                focused: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert!(sync.plans.is_empty());
        assert!(sync.slideshow_plans.is_empty());
        assert_eq!(sync.removals, vec!["eDP-1"]);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Unfocused
        );
    }

    #[test]
    fn unfocused_pause_dynamic_keeps_static_wallpaper_renderable() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/static-demo.gwpdir".to_owned());
        config.performance.unfocused = crate::config::ThrottlePolicy::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                focused: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.removals.is_empty());
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Active);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Interactive
        );
    }

    #[test]
    fn config_output_wallpaper_adds_named_output_without_state() {
        let mut config = GilderConfig::default();
        config.outputs.insert(
            "DP-1".to_owned(),
            OutputConfig {
                wallpaper: Some("examples/wallpapers/static-demo.gwpdir".to_owned()),
                ..OutputConfig::default()
            },
        );
        let state = AppState::default();
        let desktop = DesktopSnapshot::default();

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.errors.is_empty());
        assert_eq!(sync.plans[0].output_name, "DP-1");
        assert_eq!(
            sync.decisions[0].wallpaper.as_deref(),
            Some("examples/wallpapers/static-demo.gwpdir")
        );
    }

    #[test]
    fn persisted_state_wallpaper_overrides_config_default() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("missing-config-default.gwpdir".to_owned());
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: "examples/wallpapers/static-demo.gwpdir".to_owned(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.errors.is_empty());
        assert_eq!(
            sync.decisions[0].wallpaper.as_deref(),
            Some("examples/wallpapers/static-demo.gwpdir")
        );
    }

    #[test]
    fn fullscreen_pause_policy_removes_output_without_loading_wallpaper() {
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: "missing-wallpaper.gwpdir".to_owned(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                has_fullscreen: true,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_performance(
            &PerformanceConfig::default(),
            &desktop,
            &state,
            std::env::temp_dir(),
        );
        assert!(sync.plans.is_empty());
        assert_eq!(sync.removals, ["eDP-1"]);
        assert!(sync.errors.is_empty());
        assert_eq!(sync.decisions.len(), 1);
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Remove);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Fullscreen
        );
        assert_eq!(
            sync.decisions[0].wallpaper.as_deref(),
            Some("missing-wallpaper.gwpdir")
        );
    }

    #[test]
    fn fullscreen_pause_dynamic_removes_slideshow_after_manifest_load() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/slideshow-demo.gwpdir".to_owned());
        config.performance.fullscreen = crate::config::ThrottlePolicy::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                has_fullscreen: true,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert!(sync.plans.is_empty());
        assert!(sync.slideshow_plans.is_empty());
        assert_eq!(sync.removals, vec!["eDP-1"]);
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Remove);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Fullscreen
        );
    }

    #[test]
    fn fullscreen_pause_dynamic_keeps_static_wallpaper_renderable() {
        let test_dir = TestDir::new("gilder-fullscreen-pause-dynamic-static");
        let package_dir = test_dir.path.join("static-variant.gwpdir");
        write_minimal_static_variant_gwpdir(&package_dir);
        set_runtime_continue_when_fullscreen(&package_dir);
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some(package_dir.display().to_string());
        config.performance.fullscreen = crate::config::ThrottlePolicy::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                has_fullscreen: true,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config(
            &config,
            &desktop,
            &state,
            test_dir.path.join("cache"),
        );

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.slideshow_plans.is_empty());
        assert!(sync.removals.is_empty());
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Render);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Active);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Interactive
        );
    }

    #[test]
    fn throttled_policy_keeps_static_plan_with_decision() {
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: "examples/wallpapers/static-demo.gwpdir".to_owned(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                focused: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_performance(
            &PerformanceConfig::default(),
            &desktop,
            &state,
            std::env::temp_dir(),
        );
        assert_eq!(sync.plans.len(), 1);
        assert!(sync.removals.is_empty());
        assert!(sync.errors.is_empty());
        assert_eq!(sync.decisions.len(), 1);
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Render);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Throttled);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Unfocused
        );
    }

    #[test]
    fn manifest_runtime_policy_can_pause_unfocused_output() {
        let test_dir = TestDir::new("gilder-runtime-unfocused-pause");
        let package_dir = test_dir.path.join("static-variant.gwpdir");
        write_minimal_static_variant_gwpdir(&package_dir);
        set_runtime_pause_when_unfocused(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                focused: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert!(sync.plans.is_empty());
        assert!(sync.video_plans.is_empty());
        assert!(sync.errors.is_empty());
        assert_eq!(sync.removals, ["eDP-1"]);
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Remove);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Unfocused
        );
    }

    #[test]
    fn builds_video_sync_plan_with_effective_fps() {
        let test_dir = TestDir::new("gilder-video-plan");
        let package_dir = test_dir.path.join("video-demo.gwpdir");
        write_minimal_video_gwpdir(&package_dir);
        let mut config = PerformanceConfig::default();
        config.background_max_fps = 15;
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                focused: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_performance(
            &config,
            &desktop,
            &state,
            test_dir.path.join("cache"),
        );

        assert!(sync.plans.is_empty());
        assert_eq!(sync.video_plans.len(), 1);
        assert!(sync.removals.is_empty());
        assert!(sync.errors.is_empty());
        let plan = &sync.video_plans[0];
        assert_eq!(plan.output_name, "eDP-1");
        assert!(plan.source.ends_with("assets/loop.webm"));
        assert!(
            plan.poster
                .as_ref()
                .unwrap()
                .ends_with("previews/poster.jpg")
        );
        assert_eq!(plan.fit, FitMode::Contain);
        assert!(!plan.loop_playback);
        assert!(plan.muted);
        assert_eq!(plan.manifest_max_fps, Some(60));
        assert_eq!(plan.target_max_fps, Some(15));
        assert_eq!(plan.start_offset_ms, 1200);
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Render);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Throttled);
        assert_eq!(sync.decisions[0].performance.max_fps, Some(15));
    }

    #[test]
    fn video_plan_keeps_audio_unmuted_when_runtime_allows_audio() {
        let test_dir = TestDir::new("gilder-video-runtime-audio");
        let package_dir = test_dir.path.join("video-demo.gwpdir");
        write_minimal_video_gwpdir(&package_dir);
        set_runtime_allow_audio(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert_eq!(sync.video_plans.len(), 1);
        assert!(!sync.video_plans[0].muted);
    }

    #[test]
    fn output_performance_override_sets_video_target_fps() {
        let test_dir = TestDir::new("gilder-output-performance-plan");
        let package_dir = test_dir.path.join("video-demo.gwpdir");
        write_minimal_video_gwpdir(&package_dir);
        let mut config = GilderConfig::default();
        config.video.decoder = VideoDecoderPolicy::Software;
        config.outputs.insert(
            "eDP-1".to_owned(),
            OutputConfig {
                performance: OutputPerformanceConfig {
                    background_max_fps: Some(12),
                    ..OutputPerformanceConfig::default()
                },
                ..OutputConfig::default()
            },
        );
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                focused: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config(
            &config,
            &desktop,
            &state,
            test_dir.path.join("cache"),
        );

        assert_eq!(sync.video_plans.len(), 1);
        assert_eq!(sync.video_plans[0].target_max_fps, Some(12));
        assert_eq!(
            sync.video_plans[0].decoder_policy,
            VideoDecoderPolicy::Software
        );
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Throttled);
        assert_eq!(sync.decisions[0].performance.max_fps, Some(12));
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Unfocused
        );
    }

    #[test]
    fn output_fit_override_sets_video_and_poster_fit() {
        let test_dir = TestDir::new("gilder-output-fit-plan");
        let package_dir = test_dir.path.join("video-demo.gwpdir");
        write_minimal_video_gwpdir(&package_dir);
        let mut config = GilderConfig::default();
        config.outputs.insert(
            "eDP-1".to_owned(),
            OutputConfig {
                fit: Some(FitMode::Stretch),
                ..OutputConfig::default()
            },
        );
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config(
            &config,
            &desktop,
            &state,
            test_dir.path.join("cache"),
        );

        assert!(sync.plans.is_empty());
        assert_eq!(sync.video_plans.len(), 1);
        assert_eq!(sync.video_plans[0].fit, FitMode::Stretch);
    }

    #[test]
    fn video_plan_uses_requested_variant_source() {
        let test_dir = TestDir::new("gilder-video-variant-plan");
        let package_dir = test_dir.path.join("video-demo.gwpdir");
        write_minimal_video_gwpdir(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: Some("mobile".to_owned()),
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert_eq!(sync.video_plans.len(), 1);
        assert!(
            sync.video_plans[0]
                .source
                .ends_with("assets/loop-mobile.webm")
        );
        assert_eq!(
            sync.decisions[0].wallpaper.as_deref(),
            Some(package_dir.display().to_string().as_str())
        );
    }

    #[test]
    fn video_plan_auto_selects_portrait_variant_source() {
        let test_dir = TestDir::new("gilder-video-auto-variant-plan");
        let package_dir = test_dir.path.join("video-demo.gwpdir");
        write_minimal_video_gwpdir(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                width: Some(1080),
                height: Some(1920),
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert_eq!(sync.video_plans.len(), 1);
        assert!(sync.errors.is_empty());
        assert!(
            sync.video_plans[0]
                .source
                .ends_with("assets/loop-mobile.webm")
        );
    }

    #[test]
    fn video_plan_uses_preview_poster_when_entry_poster_is_missing() {
        let test_dir = TestDir::new("gilder-video-preview-poster");
        let package_dir = test_dir.path.join("video-demo.gwpdir");
        write_minimal_video_gwpdir(&package_dir);
        remove_entry_poster(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert_eq!(sync.video_plans.len(), 1);
        assert!(sync.plans.is_empty());
        assert!(
            sync.video_plans[0]
                .poster
                .as_ref()
                .unwrap()
                .ends_with("previews/poster.jpg")
        );
    }
    #[test]
    fn web_fallback_builds_static_plan() {
        let test_dir = TestDir::new("gilder-web-fallback-plan");
        let package_dir = test_dir.path.join("web-demo.gwpdir");
        write_minimal_web_gwpdir(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.video_plans.is_empty());
        assert!(sync.slideshow_plans.is_empty());
        assert!(sync.errors.is_empty());
        assert!(sync.plans[0].source.ends_with("previews/poster.svg"));
        assert_eq!(sync.plans[0].fit, FitMode::Cover);
        assert_eq!(sync.plans[0].background.as_deref(), Some("#000000"));
        assert_eq!(sync.cache.planned_static_image_resources, 1);
        assert_eq!(sync.cache.planned_image_resource_references, 1);
    }

    #[test]
    fn web_without_fallback_reports_unsupported_entry() {
        let test_dir = TestDir::new("gilder-web-without-fallback-plan");
        let package_dir = test_dir.path.join("web-demo.gwpdir");
        write_minimal_web_gwpdir(&package_dir);
        remove_entry_fallback(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert!(sync.plans.is_empty());
        assert_eq!(sync.errors.len(), 1);
        assert_eq!(sync.errors[0].message, "web entries are not supported here");
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Error);
    }

    include!("adaptive_and_visibility/fallback_entries.rs");
