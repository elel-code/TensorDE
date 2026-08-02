
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_sync_dedup_tracks_last_queued_plan() {
        let runtime = test_runtime(test_context(), Vec::new());
        let first = empty_render_sync();
        let second = StaticRenderSyncPlan {
            removals: vec!["eDP-1".to_owned()],
            ..empty_render_sync()
        };

        assert!(runtime.queue_render_sync_if_changed(first.clone()));
        assert!(!runtime.queue_render_sync_if_changed(first));
        assert!(runtime.queue_render_sync_if_changed(second));
        let telemetry = runtime.telemetry_snapshot();
        assert_eq!(telemetry.render_sync_updates_queued, 2);
        assert_eq!(telemetry.render_sync_updates_skipped, 1);
    }

    #[test]
    fn render_sync_dedup_suppresses_repeated_renderer_updates() {
        let (sender, receiver) = mpsc::channel();
        let runtime = test_runtime(test_context(), vec![sender]);
        let first = empty_render_sync();
        let second = StaticRenderSyncPlan {
            removals: vec!["eDP-1".to_owned()],
            ..empty_render_sync()
        };

        runtime.store_last_render_sync(first.clone());
        assert!(!runtime.queue_render_sync_if_changed(first.clone()));
        assert!(receiver.try_recv().is_err());

        assert!(runtime.queue_render_sync_if_changed(second.clone()));
        assert_eq!(receiver.try_recv().ok(), Some(second.clone()));
        assert!(!runtime.queue_render_sync_if_changed(second));
    }

    #[test]
    fn clamps_desktop_refresh_interval() {
        let config = PerformanceConfig {
            desktop_refresh_interval_ms: 0,
            ..PerformanceConfig::default()
        };
        assert_eq!(
            desktop_refresh_interval(&config),
            Duration::from_millis(250)
        );

        let config = PerformanceConfig {
            desktop_refresh_interval_ms: 1250,
            ..PerformanceConfig::default()
        };
        assert_eq!(
            desktop_refresh_interval(&config),
            Duration::from_millis(1250)
        );
    }

    #[test]
    fn read_requests_refresh_desktop_only_after_interval() {
        let mut context = test_context();
        context.config.adapters = gilder::config::AdapterConfig {
            generic_wayland: false,
            hyprland: false,
            niri: false,
        };
        context.config.performance.desktop_refresh_interval_ms = 1_000;
        context.desktop.outputs = vec![gilder::desktop::DesktopOutput::virtual_output("eDP-1")];
        context.last_desktop_refresh = Some(Instant::now());

        refresh_desktop_if_stale(&mut context);
        assert_eq!(context.desktop.outputs.len(), 1);
        assert_eq!(context.telemetry.desktop_refresh_skips, 1);
        assert_eq!(context.telemetry.desktop_refreshes, 0);

        context.last_desktop_refresh = Some(Instant::now() - Duration::from_millis(1_500));
        refresh_desktop_if_stale(&mut context);
        assert!(context.desktop.outputs.is_empty());
        assert!(context.last_desktop_refresh.is_some());
        assert_eq!(context.telemetry.desktop_refresh_skips, 1);
        assert_eq!(context.telemetry.desktop_refreshes, 1);
    }

    #[test]
    fn status_response_reports_daemon_telemetry() {
        let mut context = test_context();
        let request = gilder::ipc::IpcRequest {
            id: json!(1),
            method: RequestMethod::Status,
        };

        let renderer_runtime = RendererRuntimeSnapshot {
            output_windows: 3,
            static_surfaces: 2,
            static_picture_surfaces: 1,
            static_css_surfaces: 1,
            static_color_surfaces: 0,
            slideshow_surfaces: 1,
            video_surfaces: 2,
            static_surface_resource_references: 2,
            static_surface_resource_bytes: 4096,
            static_surface_unique_resources: 1,
            static_surface_unique_resource_bytes: 2048,
            static_surface_estimated_decoded_bytes: 8_294_400,
            slideshow_resource_references: 3,
            slideshow_resource_bytes: 8192,
            slideshow_unique_resources: 2,
            slideshow_unique_resource_bytes: 6144,
            video_shared_runtimes: 1,
            video_pipeline_source_references: 3,
            video_pipeline_source_reference_bytes: 18_000,
            video_pipeline_unique_sources: 2,
            video_pipeline_unique_source_bytes: 12_000,
            video_pipelines: vec![
                json!({
                    "output_name": "eDP-1",
                    "actual_decoders": ["dav1ddec"],
                    "frame_stats": {
                        "qos_messages": 3,
                        "qos_dropped_max": 2,
                    },
                }),
                json!({
                    "output_name": "HDMI-A-1",
                    "actual_decoders": ["vaav1dec"],
                    "frame_stats": {
                        "qos_messages": 4,
                        "qos_dropped_max": 3,
                    },
                }),
            ],
        };
        let outcome = handle_ipc_request(
            request,
            &mut context,
            RuntimeTelemetrySnapshot::default(),
            renderer_runtime,
        );
        let response: serde_json::Value =
            serde_json::from_str(&outcome.response).expect("status response should be JSON");

        assert_eq!(
            response["result"]["telemetry"]["desktop"]["refresh_skips"],
            json!(1)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["cache_misses"],
            json!(1)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["updates_queued"],
            json!(0)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["package_cache_entries"],
            json!(0)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["package_cache_max_entries"],
            json!(16)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["package_cache_max_retained_unique_resource_bytes"],
            json!(536_870_912)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["package_cache_evictions"],
            json!(0)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["package_cache_retained_resource_references"],
            json!(0)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["package_cache_retained_unique_resources"],
            json!(0)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["package_cache_retained_resource_bytes"],
            json!(0)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["package_cache_retained_unique_resource_bytes"],
            json!(0)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["archive_cache_entries"],
            json!(0)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["archive_cache_evictions"],
            json!(0)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["archive_cache_eviction_errors"],
            json!(0)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["static_image_cache_entries"],
            json!(0)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["static_image_cache_max_entries"],
            json!(32)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["static_image_cache_bytes"],
            json!(0)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["static_image_cache_max_bytes"],
            json!(536_870_912)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["planned_static_image_resources"],
            json!(0)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["planned_video_poster_resources"],
            json!(0)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["planned_slideshow_image_resources"],
            json!(0)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["planned_scene_image_resources"],
            json!(0)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["planned_image_resource_references"],
            json!(0)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["planned_unique_image_resources"],
            json!(0)
        );
        assert!(
            response["result"]["telemetry"]["desktop"]["last_refresh_age_ms"]
                .as_u64()
                .is_some()
        );
        assert_eq!(
            response["result"]["renderer_runtime"]["video_pipelines"][0]["actual_decoders"],
            json!(["dav1ddec"])
        );
        assert_eq!(
            response["result"]["renderer_runtime"]["output_windows"],
            json!(3)
        );
        assert_eq!(
            response["result"]["renderer_runtime"]["static_picture_surfaces"],
            json!(1)
        );
        assert_eq!(
            response["result"]["renderer_runtime"]["static_css_surfaces"],
            json!(1)
        );
        assert_eq!(
            response["result"]["renderer_runtime"]["static_color_surfaces"],
            json!(0)
        );
        assert_eq!(
            response["result"]["renderer_runtime"]["static_surface_resource_bytes"],
            json!(4096)
        );
        assert_eq!(
            response["result"]["renderer_runtime"]["static_surface_estimated_decoded_bytes"],
            json!(8294400)
        );
        assert_eq!(
            response["result"]["renderer_runtime"]["static_surface_unique_resources"],
            json!(1)
        );
        assert_eq!(
            response["result"]["renderer_runtime"]["slideshow_unique_resource_bytes"],
            json!(6144)
        );
        assert_eq!(
            response["result"]["renderer_runtime"]["video_shared_runtimes"],
            json!(1)
        );
        assert_eq!(
            response["result"]["renderer_runtime"]["video_pipeline_source_references"],
            json!(3)
        );
        assert_eq!(
            response["result"]["renderer_runtime"]["video_pipeline_unique_source_bytes"],
            json!(12000)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["output_windows"],
            json!(3)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["static_surfaces"],
            json!(2)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["static_picture_surfaces"],
            json!(1)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["static_css_surfaces"],
            json!(1)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["static_color_surfaces"],
            json!(0)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["slideshow_surfaces"],
            json!(1)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["video_surfaces"],
            json!(2)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["video_shared_runtimes"],
            json!(1)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["static_surface_resource_references"],
            json!(2)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["static_surface_resource_bytes"],
            json!(4096)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["static_surface_unique_resources"],
            json!(1)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["static_surface_unique_resource_bytes"],
            json!(2048)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["static_surface_estimated_decoded_bytes"],
            json!(8294400)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["slideshow_resource_references"],
            json!(3)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["slideshow_resource_bytes"],
            json!(8192)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["slideshow_unique_resources"],
            json!(2)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["slideshow_unique_resource_bytes"],
            json!(6144)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["video_pipeline_source_references"],
            json!(3)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["video_pipeline_source_reference_bytes"],
            json!(18000)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["video_pipeline_unique_sources"],
            json!(2)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["video_pipeline_unique_source_bytes"],
            json!(12000)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["video_pipelines"],
            json!(2)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["video_qos_messages"],
            json!(7)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["video_qos_dropped_max"],
            json!(3)
        );
    }

    #[test]
    fn status_response_reports_planned_image_resource_telemetry() {
        let mut context = test_context();
        context.config.default_wallpaper = Some("examples/wallpapers/slideshow-demo.gwpdir".into());
        context.desktop.outputs = vec![gilder::desktop::DesktopOutput::virtual_output("eDP-1")];
        let request = gilder::ipc::IpcRequest {
            id: json!(1),
            method: RequestMethod::Status,
        };

        let outcome = handle_ipc_request(
            request,
            &mut context,
            RuntimeTelemetrySnapshot::default(),
            RendererRuntimeSnapshot::default(),
        );
        let response: serde_json::Value =
            serde_json::from_str(&outcome.response).expect("status response should be JSON");

        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["planned_static_image_resources"],
            json!(0)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["planned_video_poster_resources"],
            json!(0)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["planned_slideshow_image_resources"],
            json!(2)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["planned_scene_image_resources"],
            json!(0)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["planned_image_resource_references"],
            json!(2)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["planned_unique_image_resources"],
            json!(2)
        );
        let planned_bytes =
            std::fs::metadata("examples/wallpapers/slideshow-demo.gwpdir/assets/slide-a.svg")
                .unwrap()
                .len()
                + std::fs::metadata("examples/wallpapers/slideshow-demo.gwpdir/assets/slide-b.svg")
                    .unwrap()
                    .len();
        let retained_reference_bytes =
            std::fs::metadata("examples/wallpapers/slideshow-demo.gwpdir/previews/thumbnail.svg")
                .unwrap()
                .len()
                + std::fs::metadata("examples/wallpapers/slideshow-demo.gwpdir/assets/slide-a.svg")
                    .unwrap()
                    .len()
                    * 2
                + std::fs::metadata("examples/wallpapers/slideshow-demo.gwpdir/assets/slide-b.svg")
                    .unwrap()
                    .len();
        let retained_unique_bytes =
            std::fs::metadata("examples/wallpapers/slideshow-demo.gwpdir/previews/thumbnail.svg")
                .unwrap()
                .len()
                + planned_bytes;
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["package_cache_retained_resource_references"],
            json!(4)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["package_cache_retained_unique_resources"],
            json!(3)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["package_cache_retained_resource_bytes"],
            json!(retained_reference_bytes)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["package_cache_retained_unique_resource_bytes"],
            json!(retained_unique_bytes)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["package_cache_retained_preview_resource_references"],
            json!(2)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["package_cache_retained_unique_preview_resources"],
            json!(2)
        );
        let retained_preview_bytes =
            std::fs::metadata("examples/wallpapers/slideshow-demo.gwpdir/previews/thumbnail.svg")
                .unwrap()
                .len()
                + std::fs::metadata("examples/wallpapers/slideshow-demo.gwpdir/assets/slide-a.svg")
                    .unwrap()
                    .len();
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["package_cache_retained_preview_resource_bytes"],
            json!(retained_preview_bytes)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["package_cache_retained_unique_preview_resource_bytes"],
            json!(retained_preview_bytes)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["planned_static_image_resource_bytes"],
            json!(0)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["planned_video_poster_resource_bytes"],
            json!(0)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["planned_slideshow_image_resource_bytes"],
            json!(planned_bytes)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["planned_scene_image_resource_bytes"],
            json!(0)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["planned_image_resource_reference_bytes"],
            json!(planned_bytes)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["planned_unique_image_resource_bytes"],
            json!(planned_bytes)
        );
    }

    #[test]
    fn output_reports_apply_output_performance_override() {
        let mut context = test_context();
        context.config.outputs.insert(
            "eDP-1".to_owned(),
            gilder::config::OutputConfig {
                performance: gilder::config::OutputPerformanceConfig {
                    interactive_max_fps: Some(42),
                    ..gilder::config::OutputPerformanceConfig::default()
                },
                ..gilder::config::OutputConfig::default()
            },
        );
        let reports = output_reports(&context, None);

        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0]["name"], json!("eDP-1"));
        assert_eq!(reports[0]["performance"]["mode"], json!("active"));
        assert_eq!(reports[0]["performance"]["max_fps"], json!(42));
        assert_eq!(reports[0]["performance"]["reason"], json!("interactive"));
    }

    #[test]
    fn output_reports_apply_adaptive_throttle() {
        let mut context = test_context();
        context.desktop.outputs = vec![gilder::desktop::DesktopOutput::virtual_output("eDP-1")];
        context.config.adaptive.enabled = true;
        context.config.adaptive.throttle_max_fps = 15;
        context.adaptive_snapshot = gilder::adaptive::AdaptiveSnapshot {
            monitoring_enabled: true,
            active_triggers: vec![gilder::adaptive::AdaptiveTrigger {
                metric: gilder::adaptive::AdaptiveMetric::CpuPressureSomeAvg10,
                value_x100: 9_000,
                threshold_x100: 7_500,
            }],
            ..gilder::adaptive::AdaptiveSnapshot::default()
        };
        let reports = output_reports(&context, None);

        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0]["performance"]["mode"], json!("throttled"));
        assert_eq!(reports[0]["performance"]["max_fps"], json!(15));
        assert_eq!(reports[0]["performance"]["reason"], json!("adaptive"));
    }

    #[test]
    fn output_reports_apply_adaptive_pause_unfocused() {
        let mut context = test_context();
        context.desktop.outputs = vec![gilder::desktop::DesktopOutput {
            focused: false,
            ..gilder::desktop::DesktopOutput::virtual_output("eDP-1")
        }];
        context.config.adaptive.enabled = true;
        context.config.adaptive.action = gilder::config::AdaptiveAction::PauseUnfocused;
        context.adaptive_snapshot = gilder::adaptive::AdaptiveSnapshot {
            monitoring_enabled: true,
            active_triggers: vec![gilder::adaptive::AdaptiveTrigger {
                metric: gilder::adaptive::AdaptiveMetric::CpuPressureSomeAvg10,
                value_x100: 9_000,
                threshold_x100: 7_500,
            }],
            ..gilder::adaptive::AdaptiveSnapshot::default()
        };
        let reports = output_reports(&context, None);
        let actions = adaptive_action_report(&context);

        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0]["performance"]["mode"], json!("paused"));
        assert_eq!(reports[0]["performance"]["max_fps"], Value::Null);
        assert_eq!(reports[0]["performance"]["reason"], json!("adaptive"));
        assert_eq!(actions[0]["type"], json!("pause-unfocused"));
        assert_eq!(actions[0]["max_fps"], Value::Null);
    }

    #[test]
    fn output_reports_prefer_render_sync_final_performance_decision() {
        let test_dir = TestDir::new("gilder-output-final-performance");
        let mut context = test_context();
        context.paths.cache_dir = test_dir.path().join("cache");
        context.config.default_wallpaper =
            Some("examples/wallpapers/slideshow-demo.gwpdir".to_owned());
        context.config.performance.battery = gilder::config::PowerPolicy::PauseDynamic;
        context.desktop = gilder::desktop::DesktopSnapshot {
            power: gilder::desktop::PowerState::Battery,
            outputs: vec![gilder::desktop::DesktopOutput::virtual_output("eDP-1")],
            ..gilder::desktop::DesktopSnapshot::default()
        };

        let render_sync = current_render_sync(&mut context);
        let reports = output_reports(&context, Some(&render_sync));

        assert_eq!(render_sync.removals, vec!["eDP-1"]);
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0]["performance"]["mode"], json!("paused"));
        assert_eq!(reports[0]["performance"]["max_fps"], Value::Null);
        assert_eq!(reports[0]["performance"]["reason"], json!("battery"));
    }

    #[test]
    fn adaptive_action_report_reports_pause_dynamic_scope() {
        let mut context = test_context();
        context.desktop.outputs = vec![gilder::desktop::DesktopOutput::virtual_output("eDP-1")];
        context.config.adaptive.enabled = true;
        context.config.adaptive.action = gilder::config::AdaptiveAction::PauseDynamic;
        context.adaptive_snapshot = gilder::adaptive::AdaptiveSnapshot {
            monitoring_enabled: true,
            active_triggers: vec![gilder::adaptive::AdaptiveTrigger {
                metric: gilder::adaptive::AdaptiveMetric::CpuPressureSomeAvg10,
                value_x100: 9_000,
                threshold_x100: 7_500,
            }],
            ..gilder::adaptive::AdaptiveSnapshot::default()
        };

        let actions = adaptive_action_report(&context);

        assert_eq!(actions[0]["type"], json!("pause-dynamic"));
        assert_eq!(actions[0]["scope"], json!("dynamic-wallpapers"));
        assert_eq!(actions[0]["max_fps"], Value::Null);
    }

    #[path = "render_sync_cache.rs"]
    mod render_sync_cache;

    fn test_context() -> DaemonContext {
        DaemonContext {
            paths: ApplicationPaths {
                config_file: PathBuf::from("/tmp/gilder-test/config.toml"),
                state_file: PathBuf::from("/tmp/gilder-test/state.json"),
                cache_dir: PathBuf::from("/tmp/gilder-test/cache"),
                data_dir: PathBuf::from("/tmp/gilder-test/data"),
            },
            config: GilderConfig::default(),
            state: AppState::default(),
            desktop: gilder::desktop::DesktopSnapshot::default(),
            adaptive_monitor: gilder::adaptive::AdaptiveMonitor::default(),
            adaptive_snapshot: gilder::adaptive::AdaptiveSnapshot::default(),
            last_desktop_refresh: Some(Instant::now()),
            render_sync_cache: None,
            telemetry: DaemonTelemetry::default(),
        }
    }

    fn test_runtime(
        context: DaemonContext,
        renderer_updates: Vec<mpsc::Sender<StaticRenderSyncPlan>>,
    ) -> DaemonRuntime {
        DaemonRuntime::new(
            context,
            renderer_updates,
            Arc::new(Mutex::new(RendererRuntimeSnapshot::default())),
        )
    }

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(name: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .expect("test clock is before Unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!("{name}-{}-{unique}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("failed to create test directory");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn write_static_package_manifest(root: &Path, background: &str) {
        let assets = root.join("assets");
        std::fs::create_dir_all(&assets).expect("failed to create package assets");
        std::fs::write(
            assets.join("wallpaper.svg"),
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="9"><rect width="16" height="9" fill="#101418"/></svg>"##,
        )
        .expect("failed to write package asset");
        std::fs::write(
            root.join(gilder::core::MANIFEST_FILE),
            format!(
                r#"{{
  "format": "gilder.wallpaper",
  "format_version": 1,
  "id": "io.github.elelcode.gilder.cache-test",
  "version": "0.1.0",
  "title": "Cache Test",
  "kind": "static-image",
  "entry": {{
    "type": "static-image",
    "source": "assets/wallpaper.svg",
    "fit": "cover",
    "background": "{background}"
  }}
}}
"#
            ),
        )
        .expect("failed to write package manifest");
    }

    fn empty_render_sync() -> StaticRenderSyncPlan {
        StaticRenderSyncPlan {
            plans: Vec::new(),
            video_plans: Vec::new(),
            slideshow_plans: Vec::new(),
            scene_plans: Vec::new(),
            removals: Vec::new(),
            errors: Vec::new(),
            decisions: Vec::new(),
            playlist_clock_dependency: gilder::renderer::PlaylistClockDependency::None,
            cache: Default::default(),
        }
    }
}
