
    #[test]
    fn pause_dynamic_releases_scene_wallpaper_after_manifest_load() {
        let test_dir = TestDir::new("tensor-wallpaper-scene-pause-dynamic");
        let package_dir = test_dir.path.join("scene-demo.gwpdir");
        write_minimal_scene_gwpdir(&package_dir);
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
    fn scene_engine_binary_builds_scene_render_plan() {
        let test_dir = TestDir::new("tensor-wallpaper-scene-engine-binary-plan");
        let package_dir = test_dir.path.join("scene-demo.gwpdir");
        write_minimal_scene_gwpdir(&package_dir);
        let mut config = TensorWallpaperConfig::default();
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
        let test_dir = TestDir::new("tensor-wallpaper-slideshow-plan");
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
        let test_dir = TestDir::new("tensor-wallpaper-render-resource-footprint");
        let static_package = test_dir.path.join("static-demo.gwpdir");
        let video_package = test_dir.path.join("video-demo.gwpdir");
        let slideshow_package = test_dir.path.join("slideshow-demo.gwpdir");
        write_minimal_static_variant_gwpdir(&static_package);
        write_minimal_video_gwpdir(&video_package);
        write_minimal_slideshow_gwpdir(&slideshow_package);
        let mut config = TensorWallpaperConfig::default();
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
        let test_dir = TestDir::new("tensor-wallpaper-video-source-sharing");
        let video_package = test_dir.path.join("video-demo.gwpdir");
        write_minimal_video_gwpdir(&video_package);
        let mut config = TensorWallpaperConfig::default();
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
        let test_dir = TestDir::new("tensor-wallpaper-render-archive");
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
        let test_dir = TestDir::new("tensor-wallpaper-render-package-cache");
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
        let test_dir = TestDir::new("tensor-wallpaper-render-package-cache-limit");
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
        let test_dir = TestDir::new("tensor-wallpaper-render-package-cache-zero-limit");
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
        let test_dir = TestDir::new("tensor-wallpaper-render-package-cache-byte-limit");
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
        let mut config = TensorWallpaperConfig::default();
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
        let mut config = TensorWallpaperConfig::default();
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
        let test_dir = TestDir::new("tensor-wallpaper-render-cache-prune");
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
        let test_dir = TestDir::new("tensor-wallpaper-render-cache-zero-limit");
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
        let test_dir = TestDir::new("tensor-wallpaper-static-cache-byte-limit");
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
        let test_dir = TestDir::new("tensor-wallpaper-static-cache-byte-limit-protected");
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
        let test_dir = TestDir::new("tensor-wallpaper-render-sync-static-cache-byte-limit");
        let cache_dir = test_dir.path.join("cache");
        let static_cache_dir = cache_dir.join("static-image-cache");
        fs::create_dir_all(&static_cache_dir).unwrap();
        let old = static_cache_dir.join("a-old.png");
        let current = static_cache_dir.join("b-current.png");
        fs::write(&old, b"12345").unwrap();
        fs::write(&current, b"67890").unwrap();
        let mut config = TensorWallpaperConfig::default();
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
        let test_dir = TestDir::new("tensor-wallpaper-render-sync-cache-prune");
        let archive = test_dir.path.join("static-demo.gwp");
        let cache_dir = test_dir.path.join("cache");
        let render_cache_dir = cache_dir.join("render-cache");
        let old_a = render_cache_dir.join("a-old.gwpdir");
        let old_b = render_cache_dir.join("b-old.gwpdir");
        fs::create_dir_all(&old_a).unwrap();
        fs::create_dir_all(&old_b).unwrap();
        pack_gwp("examples/wallpapers/static-demo.gwpdir", &archive).unwrap();
        let mut config = TensorWallpaperConfig::default();
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

include!("cache_policy_tests/package_eviction.rs");
