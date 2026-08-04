    // Unsupported shader/web fallback and release-policy behavior.

    #[test]
    fn shader_fallback_builds_static_plan() {
        let test_dir = TestDir::new("tensor-wallpaper-shader-fallback-plan");
        let package_dir = test_dir.path.join("shader-demo.gwpdir");
        write_minimal_shader_gwpdir(&package_dir);
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
        assert!(sync.scene_plans.is_empty());
        assert!(sync.errors.is_empty());
        assert!(sync.plans[0].source.ends_with("previews/poster.svg"));
        assert_eq!(sync.plans[0].fit, FitMode::Cover);
        assert_eq!(sync.plans[0].background.as_deref(), Some("#000000"));
        assert_eq!(sync.cache.planned_static_image_resources, 1);
        assert_eq!(sync.cache.planned_image_resource_references, 1);
    }

    #[test]
    fn shader_without_fallback_reports_unsupported_entry() {
        let test_dir = TestDir::new("tensor-wallpaper-shader-without-fallback-plan");
        let package_dir = test_dir.path.join("shader-demo.gwpdir");
        write_minimal_shader_gwpdir(&package_dir);
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
        assert_eq!(
            sync.errors[0].message,
            "shader entries are not supported here"
        );
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Error);
    }

    #[test]
    fn pause_dynamic_releases_shader_wallpaper_after_manifest_load() {
        let test_dir = TestDir::new("tensor-wallpaper-shader-pause-dynamic");
        let package_dir = test_dir.path.join("shader-demo.gwpdir");
        write_minimal_shader_gwpdir(&package_dir);
        let mut config = TensorWallpaperConfig::default();
        config.performance.hidden = DynamicPausePolicy::PauseDynamic;
        config.default_wallpaper = Some(package_dir.display().to_string());
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                visible: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config(
            &config,
            &desktop,
            &AppState::default(),
            test_dir.path.join("cache"),
        );

        assert!(sync.plans.is_empty());
        assert!(sync.video_plans.is_empty());
        assert!(sync.slideshow_plans.is_empty());
        assert!(sync.scene_plans.is_empty());
        assert!(sync.errors.is_empty());
        assert_eq!(sync.removals, vec!["eDP-1"]);
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Remove);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            crate::policy::DecisionReason::OutputHidden
        );
    }

    #[test]
    fn pause_dynamic_releases_web_wallpaper_after_manifest_load() {
        let test_dir = TestDir::new("tensor-wallpaper-web-pause-dynamic");
        let package_dir = test_dir.path.join("web-demo.gwpdir");
        write_minimal_web_gwpdir(&package_dir);
        let mut config = TensorWallpaperConfig::default();
        config.performance.hidden = DynamicPausePolicy::PauseDynamic;
        config.default_wallpaper = Some(package_dir.display().to_string());
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                visible: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config(
            &config,
            &desktop,
            &AppState::default(),
            test_dir.path.join("cache"),
        );

        assert!(sync.plans.is_empty());
        assert!(sync.video_plans.is_empty());
        assert!(sync.slideshow_plans.is_empty());
        assert!(sync.errors.is_empty());
        assert_eq!(sync.removals, vec!["eDP-1"]);
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Remove);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            crate::policy::DecisionReason::OutputHidden
        );
    }
