use super::*;

#[test]
fn current_render_sync_cache_invalidates_when_manifest_changes() {
    let package_dir = TestDir::new("gilder-render-sync-cache-package");
    write_static_package_manifest(package_dir.path(), "#101418");

    let mut context = test_context();
    context.paths.cache_dir = package_dir.path().join("cache");
    context.desktop.outputs = vec![gilder::desktop::DesktopOutput::virtual_output("eDP-1")];
    context
        .state
        .set_wallpaper(None, package_dir.path().to_string_lossy());

    let first = current_render_sync(&mut context);
    assert_eq!(first.plans[0].background.as_deref(), Some("#101418"));
    assert!(context.render_sync_cache.is_some());

    let second = current_render_sync(&mut context);
    assert_eq!(second, first);
    assert_eq!(context.telemetry.render_sync_cache_hits, 1);
    assert_eq!(context.telemetry.render_sync_cache_misses, 1);

    write_static_package_manifest(package_dir.path(), "#203040ff");
    let third = current_render_sync(&mut context);
    assert_eq!(third.plans[0].background.as_deref(), Some("#203040ff"));
    assert_eq!(context.telemetry.render_sync_cache_hits, 1);
    assert_eq!(context.telemetry.render_sync_cache_misses, 2);
}

#[test]
fn current_render_sync_cache_ignores_existing_output_properties() {
    let package_dir = TestDir::new("gilder-render-sync-property-cache-package");
    write_static_package_manifest(package_dir.path(), "#101418");

    let mut context = test_context();
    context.paths.cache_dir = package_dir.path().join("cache");
    context.desktop.outputs = vec![gilder::desktop::DesktopOutput::virtual_output("eDP-1")];
    context
        .state
        .set_wallpaper(Some("eDP-1"), package_dir.path().to_string_lossy());

    let cached = StaticRenderSyncPlan {
        removals: vec!["cached-plan".to_owned()],
        ..empty_render_sync()
    };
    context.render_sync_cache = Some(RenderSyncCache {
        key: render_sync_cache_key(&context),
        render_sync: cached.clone(),
    });

    context
        .state
        .set_property(Some("eDP-1"), "speed", json!(0.5));
    assert_eq!(current_render_sync(&mut context), cached);

    context.state.pause(Some("eDP-1"), true);
    let paused = current_render_sync(&mut context);
    assert_ne!(paused, cached);
    assert_eq!(paused.removals, vec!["eDP-1"]);
}

#[test]
fn current_render_sync_cache_ignores_non_render_config() {
    let package_dir = TestDir::new("gilder-render-sync-config-cache-package");
    write_static_package_manifest(package_dir.path(), "#101418");

    let mut context = test_context();
    context.paths.cache_dir = package_dir.path().join("cache");
    context.desktop.outputs = vec![gilder::desktop::DesktopOutput::virtual_output("eDP-1")];
    context
        .state
        .set_wallpaper(Some("eDP-1"), package_dir.path().to_string_lossy());

    let cached = StaticRenderSyncPlan {
        removals: vec!["cached-plan".to_owned()],
        ..empty_render_sync()
    };
    context.render_sync_cache = Some(RenderSyncCache {
        key: render_sync_cache_key(&context),
        render_sync: cached.clone(),
    });

    context.config.adapters.niri = false;
    context.config.performance.desktop_refresh_interval_ms = 7_500;
    assert_eq!(current_render_sync(&mut context), cached);

    context.config.outputs.insert(
        "eDP-1".to_owned(),
        OutputConfig {
            fit: Some(gilder::core::FitMode::Contain),
            ..OutputConfig::default()
        },
    );
    let updated = current_render_sync(&mut context);
    assert_ne!(updated, cached);
    assert_eq!(updated.plans[0].fit, gilder::core::FitMode::Contain);
}

#[test]
fn current_render_sync_cache_invalidates_when_cache_policy_changes() {
    let package_dir = TestDir::new("gilder-render-sync-cache-config-package");
    write_static_package_manifest(package_dir.path(), "#101418");

    let mut context = test_context();
    context.paths.cache_dir = package_dir.path().join("cache");
    context.desktop.outputs = vec![gilder::desktop::DesktopOutput::virtual_output("eDP-1")];
    context
        .state
        .set_wallpaper(Some("eDP-1"), package_dir.path().to_string_lossy());

    let cached = StaticRenderSyncPlan {
        removals: vec!["cached-plan".to_owned()],
        ..empty_render_sync()
    };
    context.render_sync_cache = Some(RenderSyncCache {
        key: render_sync_cache_key(&context),
        render_sync: cached.clone(),
    });

    context.config.cache.render_cache_max_entries = 1;
    assert_ne!(current_render_sync(&mut context), cached);
}
