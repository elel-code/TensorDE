
    #[test]
    fn pause_dynamic_releases_scene_wallpaper_after_manifest_load() {
        let test_dir = TestDir::new("gilder-scene-pause-dynamic");
        let package_dir = test_dir.path.join("scene-demo.gwpdir");
        write_minimal_scene_gwpdir(&package_dir);
        let mut config = GilderConfig::default();
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
    fn scene_engine_binary_builds_scene_render_plan() {
        let test_dir = TestDir::new("gilder-scene-engine-binary-plan");
        let package_dir = test_dir.path.join("scene-demo.gwpdir");
        write_minimal_scene_gwpdir(&package_dir);
        let mut config = GilderConfig::default();
        config.performance.background_max_fps = 30;
        config.default_wallpaper = Some(package_dir.display().to_string());
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                focused: false,
                cursor_parallax: Some(DesktopCursorParallax { x: 0.5, y: 0.25 }),
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

        assert!(sync.errors.is_empty());
        assert_eq!(sync.scene_plans.len(), 1);
        let plan = &sync.scene_plans[0];
        assert!(
            plan.source
                .as_ref()
                .is_some_and(|source| source.ends_with("assets/scene.gscene"))
        );
        assert_eq!(plan.manifest_max_fps, Some(60));
        assert_eq!(plan.target_max_fps, Some(30));
        assert_eq!(
            plan.scene_size,
            Some(SceneSize {
                width: 1920,
                height: 1080
            })
        );
        assert_eq!(plan.scene_material_graph_count, 1);
        assert_eq!(plan.scene_material_graph_resource_count, 1);
        assert_eq!(plan.scene_effect_graph_count, 1);
        assert_eq!(plan.scene_mesh_count, 1);
        assert_eq!(plan.scene_mesh_vertex_count, 4);
        assert_eq!(plan.scene_mesh_index_count, 6);
        let scene_engine = plan.scene_engine.clone().expect("scene engine render plan");
        assert_eq!(scene_engine.renderer_scene_render.mesh_count, 1);
        assert_eq!(scene_engine.renderer_scene_render.mesh_vertex_count, 4);
        assert_eq!(scene_engine.renderer_scene_render.mesh_index_count, 6);
        assert_eq!(
            scene_engine
                .renderer_scene_render
                .descriptor_heap_resource_count,
            1
        );
        assert_eq!(
            scene_engine
                .renderer_scene_render
                .descriptor_heap_sampled_image_count,
            0
        );
        assert!(
            scene_engine
                .renderer_scene_render
                .fifo_latest_ready_present_required
        );
        assert_eq!(scene_engine.rendering_device_graph.mesh_draws.len(), 1);
        assert_eq!(scene_engine.rendering_device_graph.pass_nodes.len(), 1);
        assert_eq!(
            plan.scene_systems.shader_material_graph,
            SceneSystemStatus::Ready
        );
        assert!(plan.cursor_parallax_input_ready);
        assert!(plan.layers.is_empty());
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Render);
    }

    #[test]
    fn builds_slideshow_sync_plan_with_effective_fps() {
        let test_dir = TestDir::new("gilder-slideshow-plan");
        let package_dir = test_dir.path.join("slideshow-demo.gwpdir");
        write_minimal_slideshow_gwpdir(&package_dir);
        let mut config = PerformanceConfig::default();
        config.background_max_fps = 10;
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
        assert!(sync.video_plans.is_empty());
        assert_eq!(sync.slideshow_plans.len(), 1);
        assert!(sync.errors.is_empty());
        let plan = &sync.slideshow_plans[0];
        assert_eq!(plan.output_name, "eDP-1");
        assert_eq!(plan.sources.len(), 2);
        assert!(plan.sources[0].ends_with("assets/a.svg"));
        assert!(plan.sources[1].ends_with("assets/b.svg"));
        assert_eq!(plan.interval_ms, 1_500);
        assert_eq!(plan.transition, Transition::Crossfade);
        assert_eq!(plan.fit, FitMode::Contain);
        assert_eq!(plan.target_max_fps, Some(10));
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Render);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Throttled);
    }

    #[test]
    fn render_sync_reports_planned_image_resource_footprint() {
        let test_dir = TestDir::new("gilder-render-resource-footprint");
        let static_package = test_dir.path.join("static-demo.gwpdir");
        let video_package = test_dir.path.join("video-demo.gwpdir");
        let slideshow_package = test_dir.path.join("slideshow-demo.gwpdir");
        write_minimal_static_variant_gwpdir(&static_package);
        write_minimal_video_gwpdir(&video_package);
        write_minimal_slideshow_gwpdir(&slideshow_package);
        let mut config = GilderConfig::default();
        config.outputs.insert(
            "eDP-1".to_owned(),
            OutputConfig {
                wallpaper: Some(static_package.display().to_string()),
                ..OutputConfig::default()
            },
        );
        config.outputs.insert(
            "HDMI-A-1".to_owned(),
            OutputConfig {
                wallpaper: Some(video_package.display().to_string()),
                ..OutputConfig::default()
            },
        );
        config.outputs.insert(
            "DP-1".to_owned(),
            OutputConfig {
                wallpaper: Some(slideshow_package.display().to_string()),
                ..OutputConfig::default()
            },
        );
        let desktop = DesktopSnapshot {
            outputs: vec![
                DesktopOutput::virtual_output("eDP-1"),
                DesktopOutput::virtual_output("HDMI-A-1"),
                DesktopOutput::virtual_output("DP-1"),
            ],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config(
            &config,
            &desktop,
            &AppState::default(),
            test_dir.path.join("cache"),
        );

        assert!(sync.errors.is_empty());
        assert_eq!(sync.plans.len(), 1);
        assert_eq!(sync.video_plans.len(), 1);
        assert_eq!(sync.slideshow_plans.len(), 1);
        assert!(sync.scene_plans.is_empty());
        assert_eq!(sync.cache.planned_static_image_resources, 1);
        assert_eq!(sync.cache.planned_video_poster_resources, 1);
        assert_eq!(sync.cache.planned_slideshow_image_resources, 2);
        assert_eq!(sync.cache.planned_scene_image_resources, 0);
        let expected_image_resource_count = 3;
        assert_eq!(
            sync.cache.planned_image_resource_references,
            expected_image_resource_count
        );
        assert_eq!(
            sync.cache.planned_unique_image_resources,
            expected_image_resource_count
        );
        let static_bytes = fs::metadata(static_package.join("assets/wallpaper.svg"))
            .unwrap()
            .len();
        let poster_bytes = fs::metadata(video_package.join("previews/poster.jpg"))
            .unwrap()
            .len();
        let slideshow_bytes = fs::metadata(slideshow_package.join("assets/a.svg"))
            .unwrap()
            .len()
            + fs::metadata(slideshow_package.join("assets/b.svg"))
                .unwrap()
                .len();
        assert_eq!(sync.cache.planned_static_image_resource_bytes, static_bytes);
        assert_eq!(sync.cache.planned_video_poster_resource_bytes, poster_bytes);
        assert_eq!(
            sync.cache.planned_slideshow_image_resource_bytes,
            slideshow_bytes
        );
        assert_eq!(sync.cache.planned_scene_image_resource_bytes, 0);
        let expected_image_resource_bytes = static_bytes + slideshow_bytes;
        assert_eq!(
            sync.cache.planned_image_resource_reference_bytes,
            expected_image_resource_bytes
        );
        assert_eq!(
            sync.cache.planned_unique_image_resource_bytes,
            expected_image_resource_bytes
        );
    }

    #[test]
    fn render_sync_reports_duplicate_video_source_candidates() {
        let test_dir = TestDir::new("gilder-video-source-sharing");
        let video_package = test_dir.path.join("video-demo.gwpdir");
        write_minimal_video_gwpdir(&video_package);
        let mut config = GilderConfig::default();
        for output_name in ["eDP-1", "HDMI-A-1"] {
            config.outputs.insert(
                output_name.to_owned(),
                OutputConfig {
                    wallpaper: Some(video_package.display().to_string()),
                    ..OutputConfig::default()
                },
            );
        }
        let desktop = DesktopSnapshot {
            outputs: vec![
                DesktopOutput::virtual_output("eDP-1"),
                DesktopOutput::virtual_output("HDMI-A-1"),
            ],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config(
            &config,
            &desktop,
            &AppState::default(),
            test_dir.path.join("cache"),
        );

        assert!(sync.errors.is_empty());
        assert_eq!(sync.video_plans.len(), 2);
        assert_eq!(sync.cache.planned_video_source_references, 2);
        assert_eq!(sync.cache.planned_unique_video_sources, 1);
        assert_eq!(sync.cache.planned_duplicate_video_source_references, 1);
        assert_eq!(sync.cache.planned_max_video_source_outputs, 2);
        let video_bytes = fs::metadata(video_package.join("assets/loop.webm"))
            .unwrap()
            .len();
        assert_eq!(
            sync.cache.planned_video_source_reference_bytes,
            video_bytes * 2
        );
        assert_eq!(sync.cache.planned_unique_video_source_bytes, video_bytes);
    }

    #[test]
    fn builds_plan_from_gwp_archive() {
        let test_dir = TestDir::new("gilder-render-archive");
        let archive = test_dir.path.join("static-demo.gwp");
        let cache = test_dir.path.join("cache");
        pack_gwp("examples/wallpapers/static-demo.gwpdir", &archive).unwrap();
        let assignment = WallpaperAssignment {
            path: archive.display().to_string(),
            variant: None,
        };

        let plan = static_wallpaper_plan_for_assignment("eDP-1", &assignment, &cache).unwrap();
        assert_eq!(plan.output_name, "eDP-1");
        assert!(plan.source.ends_with("assets/wallpaper.svg"));
        assert!(cache.join("render-cache").exists());
    }

    #[test]
    fn render_package_cache_reuses_loaded_package() {
        let test_dir = TestDir::new("gilder-render-package-cache");
        let package_dir = test_dir.path.join("static-variant.gwpdir");
        write_minimal_static_variant_gwpdir(&package_dir);
        let assignment = WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        };
        let mut cache = RenderPackageCache::new(&test_dir.path, 16, u64::MAX);

        let first = cache.package(&assignment).unwrap();
        let first_id = first.manifest.id.clone();
        fs::remove_file(package_dir.join(crate::core::MANIFEST_FILE)).unwrap();
        let second = cache.package(&assignment).unwrap();
        let second_id = second.manifest.id.clone();

        assert_eq!(first_id, "org.example.static-variant");
        assert_eq!(second_id, first_id);
        assert!(Rc::ptr_eq(&first, &second));
        assert_eq!(cache.packages.len(), 1);
        assert_eq!(cache.stats.package_cache_misses, 1);
        assert_eq!(cache.stats.package_cache_hits, 1);
    }

    #[test]
    fn render_package_cache_evicts_old_entries_at_limit() {
        let test_dir = TestDir::new("gilder-render-package-cache-limit");
        let package_a = test_dir.path.join("a.gwpdir");
        let package_b = test_dir.path.join("b.gwpdir");
        write_minimal_static_variant_gwpdir(&package_a);
        write_minimal_static_variant_gwpdir(&package_b);
        let assignment_a = WallpaperAssignment {
            path: package_a.display().to_string(),
            variant: None,
        };
        let assignment_b = WallpaperAssignment {
            path: package_b.display().to_string(),
            variant: None,
        };
        let mut cache = RenderPackageCache::new(&test_dir.path, 1, u64::MAX);

        cache.package(&assignment_a).unwrap();
        cache.package(&assignment_b).unwrap();
        fs::remove_file(package_a.join(crate::core::MANIFEST_FILE)).unwrap();
        let err = cache.package(&assignment_a).unwrap_err();

        assert!(err.to_string().contains("manifest"));
        assert!(
            err.to_string().contains(
                &package_a
                    .join(crate::core::MANIFEST_FILE)
                    .display()
                    .to_string()
            )
        );
        assert_eq!(cache.packages.len(), 1);
        assert_eq!(cache.stats.package_cache_hits, 0);
        assert_eq!(cache.stats.package_cache_misses, 3);
        assert_eq!(cache.stats.package_cache_evictions, 2);
    }

    #[test]
    fn zero_package_cache_limit_disables_package_retention() {
        let test_dir = TestDir::new("gilder-render-package-cache-zero-limit");
        let package_dir = test_dir.path.join("static-variant.gwpdir");
        write_minimal_static_variant_gwpdir(&package_dir);
        let assignment = WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        };
        let mut cache = RenderPackageCache::new(&test_dir.path, 0, u64::MAX);

        cache.package(&assignment).unwrap();
        fs::remove_file(package_dir.join(crate::core::MANIFEST_FILE)).unwrap();
        assert!(cache.package(&assignment).is_err());

        assert!(cache.packages.is_empty());
        assert_eq!(cache.stats.package_cache_hits, 0);
        assert_eq!(cache.stats.package_cache_misses, 2);
        assert_eq!(cache.stats.package_cache_evictions, 0);
    }

    #[test]
    fn render_package_cache_evicts_old_entries_at_retained_resource_byte_limit() {
        let test_dir = TestDir::new("gilder-render-package-cache-byte-limit");
        let package_a = test_dir.path.join("a.gwpdir");
        let package_b = test_dir.path.join("b.gwpdir");
        write_minimal_static_variant_gwpdir(&package_a);
        write_minimal_static_variant_gwpdir(&package_b);
        let assignment_a = WallpaperAssignment {
            path: package_a.display().to_string(),
            variant: None,
        };
        let assignment_b = WallpaperAssignment {
            path: package_b.display().to_string(),
            variant: None,
        };
        let package_resource_bytes = source_tree_size(&package_a.join("assets/wallpaper.svg"))
            + source_tree_size(&package_a.join("assets/wide.svg"));
        let mut cache = RenderPackageCache::new(&test_dir.path, 16, package_resource_bytes);

        cache.package(&assignment_a).unwrap();
        cache.package(&assignment_b).unwrap();
        cache.update_retained_resource_footprint();

        assert_eq!(cache.packages.len(), 1);
        assert!(cache.packages.contains_key(&assignment_b.path));
        assert_eq!(cache.stats.package_cache_evictions, 1);
        assert_eq!(
            cache.stats.package_cache_retained_unique_resource_bytes,
            package_resource_bytes
        );
    }

    #[test]
    fn render_sync_reports_package_cache_retained_resource_footprint() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/static-demo.gwpdir".to_owned());
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config(
            &config,
            &desktop,
            &AppState::default(),
            std::env::temp_dir(),
        );

        let retained_bytes = [
            "previews/thumbnail.svg",
            "previews/poster.svg",
            "assets/wallpaper.svg",
        ]
        .iter()
        .map(|path| {
            fs::metadata(Path::new("examples/wallpapers/static-demo.gwpdir").join(path))
                .unwrap()
                .len()
        })
        .sum::<u64>();
        let retained_preview_bytes = ["previews/thumbnail.svg", "previews/poster.svg"]
            .iter()
            .map(|path| {
                fs::metadata(Path::new("examples/wallpapers/static-demo.gwpdir").join(path))
                    .unwrap()
                    .len()
            })
            .sum::<u64>();
        assert_eq!(sync.cache.package_cache_entries, 1);
        assert_eq!(sync.cache.package_cache_retained_resource_references, 3);
        assert_eq!(sync.cache.package_cache_retained_unique_resources, 3);
        assert_eq!(
            sync.cache.package_cache_retained_resource_bytes,
            retained_bytes
        );
        assert_eq!(
            sync.cache.package_cache_retained_unique_resource_bytes,
            retained_bytes
        );
        assert_eq!(
            sync.cache
                .package_cache_retained_preview_resource_references,
            2
        );
        assert_eq!(
            sync.cache.package_cache_retained_unique_preview_resources,
            2
        );
        assert_eq!(
            sync.cache.package_cache_retained_preview_resource_bytes,
            retained_preview_bytes
        );
        assert_eq!(
            sync.cache
                .package_cache_retained_unique_preview_resource_bytes,
            retained_preview_bytes
        );
    }

    #[test]
    fn zero_package_cache_limit_reports_no_retained_resource_footprint() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/static-demo.gwpdir".to_owned());
        config.cache.package_cache_max_entries = 0;
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config(
            &config,
            &desktop,
            &AppState::default(),
            std::env::temp_dir(),
        );

        assert_eq!(sync.cache.package_cache_entries, 0);
        assert_eq!(sync.cache.package_cache_retained_resource_references, 0);
        assert_eq!(sync.cache.package_cache_retained_unique_resources, 0);
        assert_eq!(sync.cache.package_cache_retained_resource_bytes, 0);
        assert_eq!(sync.cache.package_cache_retained_unique_resource_bytes, 0);
        assert_eq!(
            sync.cache
                .package_cache_retained_preview_resource_references,
            0
        );
        assert_eq!(
            sync.cache.package_cache_retained_unique_preview_resources,
            0
        );
        assert_eq!(sync.cache.package_cache_retained_preview_resource_bytes, 0);
        assert_eq!(
            sync.cache
                .package_cache_retained_unique_preview_resource_bytes,
            0
        );
    }

    #[test]
    fn prunes_unprotected_archive_cache_entries() {
        let test_dir = TestDir::new("gilder-render-cache-prune");
        let cache_dir = test_dir.path.join("cache");
        let render_cache_dir = cache_dir.join("render-cache");
        let old = render_cache_dir.join("a-old.gwpdir");
        let current = render_cache_dir.join("z-current.gwpdir");
        fs::create_dir_all(&old).unwrap();
        fs::create_dir_all(&current).unwrap();
        let mut protected = BTreeSet::new();
        protected.insert(current.clone());

        let report = prune_render_cache(&cache_dir, 1, &protected);

        assert_eq!(report.evictions, 1);
        assert_eq!(report.errors, 0);
        assert_eq!(report.entries_after, 1);
        assert!(!old.exists());
        assert!(current.exists());
    }

    #[test]
    fn zero_archive_cache_limit_keeps_only_protected_entries() {
        let test_dir = TestDir::new("gilder-render-cache-zero-limit");
        let cache_dir = test_dir.path.join("cache");
        let render_cache_dir = cache_dir.join("render-cache");
        let old_a = render_cache_dir.join("a-old.gwpdir");
        let old_b = render_cache_dir.join("b-old.gwpdir");
        let current = render_cache_dir.join("z-current.gwpdir");
        fs::create_dir_all(&old_a).unwrap();
        fs::create_dir_all(&old_b).unwrap();
        fs::create_dir_all(&current).unwrap();
        let mut protected = BTreeSet::new();
        protected.insert(current.clone());

        let report = prune_render_cache(&cache_dir, 0, &protected);

        assert_eq!(report.evictions, 2);
        assert_eq!(report.entries_after, 1);
        assert!(!old_a.exists());
        assert!(!old_b.exists());
        assert!(current.exists());
    }

    #[test]
    fn prunes_static_image_cache_entries_by_total_bytes() {
        let test_dir = TestDir::new("gilder-static-cache-byte-limit");
        let cache_dir = test_dir.path.join("cache");
        let static_cache_dir = cache_dir.join("static-image-cache");
        fs::create_dir_all(&static_cache_dir).unwrap();
        let old = static_cache_dir.join("a-old.png");
        let current = static_cache_dir.join("b-current.png");
        fs::write(&old, b"12345").unwrap();
        fs::write(&current, b"67890").unwrap();
        let protected = BTreeSet::new();

        let report = prune_static_image_cache(&cache_dir, 32, 5, &protected);

        assert_eq!(report.evictions, 1);
        assert_eq!(report.errors, 0);
        assert_eq!(report.entries_after, 1);
        assert_eq!(report.bytes_after, 5);
        assert!(!old.exists());
        assert!(current.exists());
    }

    #[test]
    fn static_image_cache_byte_limit_keeps_protected_files() {
        let test_dir = TestDir::new("gilder-static-cache-byte-limit-protected");
        let cache_dir = test_dir.path.join("cache");
        let static_cache_dir = cache_dir.join("static-image-cache");
        fs::create_dir_all(&static_cache_dir).unwrap();
        let old = static_cache_dir.join("a-old.png");
        let current = static_cache_dir.join("b-current.png");
        fs::write(&old, b"12345").unwrap();
        fs::write(&current, b"67890").unwrap();
        let mut protected = BTreeSet::new();
        protected.insert(current.clone());

        let report = prune_static_image_cache(&cache_dir, 32, 1, &protected);

        assert_eq!(report.evictions, 1);
        assert_eq!(report.errors, 0);
        assert_eq!(report.entries_after, 1);
        assert_eq!(report.bytes_after, 5);
        assert!(!old.exists());
        assert!(current.exists());
    }

    #[test]
    fn render_sync_reports_static_image_cache_bytes_after_prune() {
        let test_dir = TestDir::new("gilder-render-sync-static-cache-byte-limit");
        let cache_dir = test_dir.path.join("cache");
        let static_cache_dir = cache_dir.join("static-image-cache");
        fs::create_dir_all(&static_cache_dir).unwrap();
        let old = static_cache_dir.join("a-old.png");
        let current = static_cache_dir.join("b-current.png");
        fs::write(&old, b"12345").unwrap();
        fs::write(&current, b"67890").unwrap();
        let mut config = GilderConfig::default();
        config.cache.static_image_cache_max_bytes = 5;

        let sync = static_render_sync_plan_with_config(
            &config,
            &DesktopSnapshot::default(),
            &AppState::default(),
            &cache_dir,
        );

        assert_eq!(sync.cache.static_image_cache_entries, 1);
        assert_eq!(sync.cache.static_image_cache_bytes, 5);
        assert_eq!(sync.cache.static_image_cache_max_bytes, 5);
        assert_eq!(sync.cache.static_image_cache_evictions, 1);
        assert!(!old.exists());
        assert!(current.exists());
    }

    #[test]
    fn render_sync_prunes_stale_archive_cache_and_reports_stats() {
        let test_dir = TestDir::new("gilder-render-sync-cache-prune");
        let archive = test_dir.path.join("static-demo.gwp");
        let cache_dir = test_dir.path.join("cache");
        let render_cache_dir = cache_dir.join("render-cache");
        let old_a = render_cache_dir.join("a-old.gwpdir");
        let old_b = render_cache_dir.join("b-old.gwpdir");
        fs::create_dir_all(&old_a).unwrap();
        fs::create_dir_all(&old_b).unwrap();
        pack_gwp("examples/wallpapers/static-demo.gwpdir", &archive).unwrap();
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some(archive.display().to_string());
        config.cache.render_cache_max_entries = 1;
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config(
            &config,
            &desktop,
            &AppState::default(),
            &cache_dir,
        );

        let extract_dir = archive_extract_dir(&cache_dir, &archive);
        assert!(sync.errors.is_empty());
        assert_eq!(sync.plans.len(), 1);
        assert_eq!(sync.cache.package_cache_entries, 1);
        assert_eq!(sync.cache.package_cache_misses, 1);
        assert_eq!(sync.cache.archive_cache_extractions, 1);
        assert_eq!(sync.cache.archive_cache_evictions, 2);
        assert_eq!(sync.cache.archive_cache_entries, 1);
        assert_eq!(sync.cache.archive_cache_max_entries, 1);
        assert!(!old_a.exists());
        assert!(!old_b.exists());
        assert!(extract_dir.exists());
    }

    #[test]
    fn render_sync_reports_package_cache_limit_and_evictions() {
        let test_dir = TestDir::new("gilder-render-sync-package-cache-limit");
        let package_a = test_dir.path.join("a.gwpdir");
        let package_b = test_dir.path.join("b.gwpdir");
        write_minimal_static_variant_gwpdir(&package_a);
        write_minimal_static_variant_gwpdir(&package_b);
        let mut config = GilderConfig::default();
        config.cache.package_cache_max_entries = 1;
        config.outputs.insert(
            "eDP-1".to_owned(),
            OutputConfig {
                wallpaper: Some(package_a.display().to_string()),
                ..OutputConfig::default()
            },
        );
        config.outputs.insert(
            "HDMI-A-1".to_owned(),
            OutputConfig {
                wallpaper: Some(package_b.display().to_string()),
                ..OutputConfig::default()
            },
        );
        let desktop = DesktopSnapshot {
            outputs: vec![
                DesktopOutput::virtual_output("eDP-1"),
                DesktopOutput::virtual_output("HDMI-A-1"),
            ],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config(
            &config,
            &desktop,
            &AppState::default(),
            test_dir.path.join("cache"),
        );

        assert!(sync.errors.is_empty());
        assert_eq!(sync.plans.len(), 2);
        assert_eq!(sync.cache.package_cache_entries, 1);
        assert_eq!(sync.cache.package_cache_max_entries, 1);
        assert_eq!(sync.cache.package_cache_misses, 2);
        assert_eq!(sync.cache.package_cache_evictions, 1);
    }

    fn adaptive_cpu_pressure_snapshot() -> crate::adaptive::AdaptiveSnapshot {
        crate::adaptive::AdaptiveSnapshot {
            monitoring_enabled: true,
            active_triggers: vec![crate::adaptive::AdaptiveTrigger {
                metric: crate::adaptive::AdaptiveMetric::CpuPressureSomeAvg10,
                value_x100: 9_000,
                threshold_x100: 7_500,
            }],
            ..crate::adaptive::AdaptiveSnapshot::default()
        }
    }

    fn write_minimal_video_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::create_dir_all(path.join("previews")).unwrap();
        fs::write(path.join("assets/loop.webm"), b"not a real video").unwrap();
        fs::write(path.join("assets/loop-mobile.webm"), b"not a real video").unwrap();
        fs::write(path.join("previews/poster.jpg"), b"not a real image").unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.video-demo",
            "version": "1.0.0",
            "title": "Video Demo",
            "kind": "video",
            "preview": {
                "poster": "previews/poster.jpg"
            },
            "entry": {
                "type": "video",
                "source": "assets/loop.webm",
                "poster": "previews/poster.jpg",
                "loop": false,
                "muted": false,
                "fit": "contain",
                "max_fps": 60,
                "start_offset_ms": 1200
            },
            "variants": [
                {
                    "id": "mobile",
                    "source": "assets/loop-mobile.webm",
                    "width": 1080,
                    "height": 1920
                }
            ]
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_minimal_static_variant_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::write(path.join("assets/wallpaper.svg"), b"<svg/>").unwrap();
        fs::write(path.join("assets/wide.svg"), b"<svg/>").unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.static-variant",
            "version": "1.0.0",
            "title": "Static Variant Demo",
            "kind": "static-image",
            "entry": {
                "type": "static-image",
                "source": "assets/wallpaper.svg",
                "fit": "cover"
            },
            "variants": [
                {
                    "id": "wide",
                    "source": "assets/wide.svg",
                    "width": 2560,
                    "height": 1080
                }
            ]
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_minimal_slideshow_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::write(path.join("assets/a.svg"), b"<svg/>").unwrap();
        fs::write(path.join("assets/b.svg"), b"<svg/>").unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.slideshow-demo",
            "version": "1.0.0",
            "title": "Slideshow Demo",
            "kind": "slideshow",
            "entry": {
                "type": "slideshow",
                "sources": ["assets/a.svg", "assets/b.svg"],
                "interval_ms": 1500,
                "transition": "crossfade",
                "fit": "contain"
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_minimal_playlist_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::write(path.join("assets/battery.svg"), b"<svg/>").unwrap();
        fs::write(path.join("assets/loop.webm"), b"not a real video").unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.playlist-demo",
            "version": "1.0.0",
            "title": "Playlist Demo",
            "kind": "playlist",
            "entry": {
                "type": "playlist",
                "items": [
                    {
                        "id": "battery-static",
                        "conditions": {
                            "power": "battery"
                        },
                        "entry": {
                            "type": "static-image",
                            "source": "assets/battery.svg",
                            "fit": "cover"
                        }
                    },
                    {
                        "id": "default-video",
                        "entry": {
                            "type": "video",
                            "source": "assets/loop.webm",
                            "loop": true,
                            "muted": true,
                            "fit": "cover",
                            "max_fps": 60
                        }
                    }
                ]
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_playlist_no_match_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::write(path.join("assets/loop.webm"), b"not a real video").unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.playlist-no-match",
            "version": "1.0.0",
            "title": "Playlist No Match",
            "kind": "playlist",
            "entry": {
                "type": "playlist",
                "items": [
                    {
                        "id": "dp-only-video",
                        "conditions": {
                            "outputs": ["DP-1"]
                        },
                        "entry": {
                            "type": "video",
                            "source": "assets/loop.webm",
                            "loop": true,
                            "muted": true,
                            "fit": "cover"
                        }
                    }
                ]
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_static_auto_variant_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::write(path.join("assets/wallpaper.svg"), b"<svg/>").unwrap();
        fs::write(path.join("assets/small.svg"), b"<svg/>").unwrap();
        fs::write(path.join("assets/hd.svg"), b"<svg/>").unwrap();
        fs::write(path.join("assets/uhd.svg"), b"<svg/>").unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.static-auto-variant",
            "version": "1.0.0",
            "title": "Static Auto Variant Demo",
            "kind": "static-image",
            "entry": {
                "type": "static-image",
                "source": "assets/wallpaper.svg",
                "fit": "cover"
            },
            "variants": [
                {
                    "id": "small",
                    "source": "assets/small.svg",
                    "width": 1280,
                    "height": 720
                },
                {
                    "id": "hd",
                    "source": "assets/hd.svg",
                    "width": 1920,
                    "height": 1080
                },
                {
                    "id": "uhd",
                    "source": "assets/uhd.svg",
                    "width": 3840,
                    "height": 2160
                }
            ]
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_static_large_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::write(path.join("assets/wallpaper.png"), b"original-large-image").unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.static-large",
            "version": "1.0.0",
            "title": "Static Large Demo",
            "kind": "static-image",
            "entry": {
                "type": "static-image",
                "source": "assets/wallpaper.png",
                "fit": "cover",
                "width": 7680,
                "height": 4320
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_minimal_web_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets/web")).unwrap();
        fs::create_dir_all(path.join("previews")).unwrap();
        fs::write(
            path.join("assets/web/index.html"),
            b"<main>web wallpaper</main>",
        )
        .unwrap();
        fs::write(
            path.join("assets/web/gilder-bridge.js"),
            b"window.gilder = {};",
        )
        .unwrap();
        fs::write(path.join("previews/poster.svg"), b"<svg/>").unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.web-demo",
            "version": "1.0.0",
            "title": "Web Demo",
            "kind": "web",
            "preview": {
                "poster": "previews/poster.svg"
            },
            "entry": {
                "type": "web",
                "root": "assets/web",
                "index": "index.html",
                "fallback": "previews/poster.svg",
                "max_fps": 30
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_minimal_shader_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("shaders")).unwrap();
        fs::create_dir_all(path.join("previews")).unwrap();
        fs::write(
            path.join("shaders/main.frag"),
            br##"
uniform float u_time;
uniform vec2 u_resolution;
uniform float u_intensity;
void main() {}
"##,
        )
        .unwrap();
        fs::write(path.join("previews/poster.svg"), b"<svg/>").unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.shader-demo",
            "version": "1.0.0",
            "title": "Shader Demo",
            "kind": "shader",
            "preview": {
                "poster": "previews/poster.svg"
            },
            "entry": {
                "type": "shader",
                "source": "shaders/main.frag",
                "fallback": "previews/poster.svg",
                "language": "glsl",
                "max_fps": 60,
                "uniforms": [
                    { "name": "u_time", "source": "time" },
                    { "name": "u_resolution", "source": "resolution" },
                    { "name": "u_intensity", "source": "property", "property": "intensity" }
                ]
            },
            "properties": {
                "intensity": {
                    "type": "range",
                    "min": 0.0,
                    "max": 1.0,
                    "default": 0.5
                }
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_minimal_scene_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::create_dir_all(path.join("previews")).unwrap();
        let scene_binary = minimal_scene_engine_binary();
        let mut scene_file = fs::File::create(path.join("assets/scene.gscene")).unwrap();
        crate::engine::scene::write_scene_binary(&scene_binary, &mut scene_file).unwrap();
        fs::write(path.join("previews/poster.svg"), b"<svg/>").unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.scene-demo",
            "version": "1.0.0",
            "title": "Scene Demo",
            "kind": "scene",
            "preview": {
                "poster": "previews/poster.svg"
            },
            "entry": {
                "type": "scene",
                "source": "assets/scene.gscene",
                "max_fps": 60
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn minimal_scene_engine_binary() -> crate::engine::scene::SceneBinaryDocument {
        use crate::engine::scene::{
            SCENE_DEFAULT_FEATURE_FLAGS, SceneBinaryDocument, SceneMaterialHandle,
            SceneMaterialRecord, SceneMeshRecord, SceneMeshVertexRecord, SceneObjectHandle,
            SceneObjectKind, SceneObjectRecord, SceneProjectRecord, SceneRenderGraphRecord,
            SceneRenderPassKind, SceneRenderPassRecord, SceneRenderTargetKind, SceneResourceId,
            SceneResourceKind, SceneResourceRecord, SceneShaderContractRecord, SceneStringId,
            SceneVec3,
        };

        SceneBinaryDocument {
            feature_flags: SCENE_DEFAULT_FEATURE_FLAGS,
            strings: vec![
                "Scene Demo".to_owned(),
                "scene".to_owned(),
                "scene.json".to_owned(),
                "materials/layer.json".to_owned(),
                "genericimage4".to_owned(),
                "genericimage4|blend=normal".to_owned(),
                "loose-file".to_owned(),
            ],
            project: SceneProjectRecord {
                title: SceneStringId(0),
                wallpaper_type: SceneStringId(1),
                scene_file: SceneStringId(2),
                preview: SceneStringId::NONE,
                properties_json: SceneStringId::NONE,
                logical_width: 1920,
                logical_height: 1080,
                clear_color: [0.0, 0.0, 0.0, 1.0],
                ambient_color: [0.3, 0.3, 0.3, 1.0],
                skylight_color: [0.3, 0.3, 0.3, 1.0],
                camera_eye: SceneVec3::default(),
                camera_center: SceneVec3 {
                    x: 0.0,
                    y: 0.0,
                    z: -1.0,
                },
                camera_up: SceneVec3 {
                    x: 0.0,
                    y: 1.0,
                    z: 0.0,
                },
            },
            resources: vec![SceneResourceRecord {
                id: SceneResourceId(0),
                kind: SceneResourceKind::MaterialJson,
                path: SceneStringId(3),
                source: SceneStringId(6),
                payload_offset: 0,
                payload_len: 2,
            }],
            resource_payload: b"{}".to_vec(),
            objects: vec![SceneObjectRecord {
                id: SceneObjectHandle(0),
                we_id: 7,
                name: SceneStringId::NONE,
                kind: SceneObjectKind::Image,
                resource: SceneResourceId::NONE,
                material: SceneMaterialHandle(0),
                parent_we_id: crate::engine::scene::INVALID_OBJECT_ID,
                attachment: SceneStringId::NONE,
                origin: SceneVec3::default(),
                angles: SceneVec3::default(),
                scale: SceneVec3 {
                    x: 1.0,
                    y: 1.0,
                    z: 1.0,
                },
                visible: true,
                color_blend_mode: 0,
                sort_order: 0,
                effect_start: u32::MAX,
                effect_count: 0,
                render_graph: 0,
            }],
            materials: vec![SceneMaterialRecord {
                id: SceneMaterialHandle(0),
                resource: SceneResourceId(0),
                pass_start: 0,
                pass_count: 0,
            }],
            meshes: vec![SceneMeshRecord {
                object: SceneObjectHandle(0),
                material: SceneMaterialHandle(0),
                vertex_start: 0,
                vertex_count: 4,
                index_start: 0,
                index_count: 6,
                width: 64.0,
                height: 32.0,
                bounds_min: SceneVec3 {
                    x: -32.0,
                    y: -16.0,
                    z: 0.0,
                },
                bounds_max: SceneVec3 {
                    x: 32.0,
                    y: 16.0,
                    z: 0.0,
                },
            }],
            mesh_vertices: vec![
                SceneMeshVertexRecord {
                    position: SceneVec3 {
                        x: -32.0,
                        y: -16.0,
                        z: 0.0,
                    },
                    uv: [0.0, 1.0],
                },
                SceneMeshVertexRecord {
                    position: SceneVec3 {
                        x: 32.0,
                        y: -16.0,
                        z: 0.0,
                    },
                    uv: [1.0, 1.0],
                },
                SceneMeshVertexRecord {
                    position: SceneVec3 {
                        x: 32.0,
                        y: 16.0,
                        z: 0.0,
                    },
                    uv: [1.0, 0.0],
                },
                SceneMeshVertexRecord {
                    position: SceneVec3 {
                        x: -32.0,
                        y: 16.0,
                        z: 0.0,
                    },
                    uv: [0.0, 0.0],
                },
            ],
            mesh_indices: vec![0, 1, 2, 0, 2, 3],
            render_graphs: vec![SceneRenderGraphRecord {
                object: SceneObjectHandle(0),
                pass_start: 0,
                pass_count: 1,
                unsupported_start: 0,
                unsupported_count: 0,
            }],
            render_passes: vec![SceneRenderPassRecord {
                id: 0,
                role: SceneRenderPassKind::BaseMaterial,
                object: SceneObjectHandle(0),
                pass_index: 0,
                shader_key: SceneStringId(4),
                target: SceneRenderTargetKind::SceneColor,
                target_name: SceneStringId::NONE,
                binding_start: 0,
                binding_count: 0,
                pipeline_blend: crate::engine::scene::ScenePipelineBlend::Normal,
                depth_test: crate::engine::scene::SceneDepthTest::Disabled,
                depth_write: false,
                cull_mode: crate::engine::scene::SceneCullMode::None,
            }],
            shader_contracts: vec![SceneShaderContractRecord {
                shader_key: SceneStringId(4),
                pipeline_key: SceneStringId(5),
                texture_slot_mask: 0,
                constant_start: 0,
                constant_count: 0,
                resource_heap_count: 1,
                sampler_heap_count: 0,
            }],
            ..SceneBinaryDocument::default()
        }
    }

    fn remove_entry_poster(path: &Path) {
        let manifest_path = path.join(crate::core::MANIFEST_FILE);
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest
            .get_mut("entry")
            .and_then(|entry| entry.as_object_mut())
            .unwrap()
            .remove("poster");
        fs::write(manifest_path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    }

    fn remove_entry_fallback(path: &Path) {
        let manifest_path = path.join(crate::core::MANIFEST_FILE);
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest
            .get_mut("entry")
            .and_then(|entry| entry.as_object_mut())
            .unwrap()
            .remove("fallback");
        fs::write(manifest_path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    }

    fn set_runtime_pause_when_unfocused(path: &Path) {
        let manifest_path = path.join(crate::core::MANIFEST_FILE);
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest["runtime"] = json!({
            "pause_when_unfocused": true
        });
        fs::write(manifest_path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    }

    fn set_runtime_continue_when_fullscreen(path: &Path) {
        let manifest_path = path.join(crate::core::MANIFEST_FILE);
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest["runtime"] = json!({
            "pause_when_fullscreen": false
        });
        fs::write(manifest_path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    }

    fn set_runtime_allow_audio(path: &Path) {
        let manifest_path = path.join(crate::core::MANIFEST_FILE);
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest["runtime"] = json!({
            "allow_audio": true
        });
        fs::write(manifest_path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    }

    fn active_performance_decision() -> PerformanceDecision {
        PerformanceDecision {
            mode: RenderMode::Active,
            max_fps: Some(60),
            reason: DecisionReason::Interactive,
        }
    }

    fn write_executable_script(path: &Path, contents: &str) {
        fs::write(path, contents).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(path).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(path, permissions).unwrap();
        }
    }

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(prefix: &str) -> Self {
            let pid = std::process::id();
            let sequence = TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("{prefix}-{pid}-{sequence}-{nanos}"));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
