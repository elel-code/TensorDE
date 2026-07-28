fn persist_or_error(id: &serde_json::Value, context: &DaemonContext) -> Option<String> {
    gilder::state::save_state(&context.paths.state_file, &context.state)
        .err()
        .map(|err| gilder::ipc::error_response(Some(id), "internal_error", &err.to_string()))
}

fn refresh_desktop(context: &mut DaemonContext) {
    context.desktop = gilder::desktop::adapters::read_desktop_snapshot(&context.config.adapters);
    mark_desktop_refreshed(context);
}

fn refresh_desktop_if_stale(context: &mut DaemonContext) {
    let interval = desktop_refresh_interval(&context.config.performance);
    let is_stale = context
        .last_desktop_refresh
        .map(|last_refresh| last_refresh.elapsed() >= interval)
        .unwrap_or(true);
    if is_stale {
        refresh_desktop(context);
    } else {
        context.telemetry.desktop_refresh_skips += 1;
    }
}

fn refresh_adaptive_if_stale(context: &mut DaemonContext) {
    let interval = adaptive_refresh_interval(&context.config.adaptive);
    if context.adaptive_monitor.should_refresh(interval) {
        context.adaptive_snapshot = context.adaptive_monitor.refresh(&context.config);
        context.telemetry.adaptive_refreshes += 1;
    } else {
        context.telemetry.adaptive_refresh_skips += 1;
    }
}

fn mark_desktop_refreshed(context: &mut DaemonContext) {
    context.last_desktop_refresh = Some(Instant::now());
    context.telemetry.desktop_refreshes += 1;
}

fn output_reports(
    context: &DaemonContext,
    render_sync: Option<&StaticRenderSyncPlan>,
) -> Vec<serde_json::Value> {
    let mut names: Vec<String> = context
        .desktop
        .outputs
        .iter()
        .map(|output| output.name.clone())
        .chain(context.state.outputs.keys().cloned())
        .chain(context.config.outputs.keys().cloned())
        .collect();
    names.sort();
    names.dedup();

    names
        .into_iter()
        .map(|name| {
            let desktop_output = context.desktop.output(&name);
            let state = context
                .state
                .outputs
                .get(&name)
                .cloned()
                .unwrap_or_default();
            let performance_config = context.config.performance_for_output(&name);
            let performance = render_sync
                .and_then(|render_sync| {
                    render_sync
                        .decisions
                        .iter()
                        .find(|decision| decision.output_name == name)
                        .map(|decision| decision.performance.clone())
                })
                .unwrap_or_else(|| {
                    let performance = gilder::policy::decide_performance(
                        &performance_config,
                        &context.desktop,
                        desktop_output,
                        &state,
                    );
                    gilder::policy::apply_adaptive_policy(
                        performance,
                        &context.config,
                        &name,
                        desktop_output,
                        &context.adaptive_snapshot,
                    )
                });
            json!({
                "name": name,
                "desktop": desktop_output,
                "state": state,
                "performance": performance,
            })
        })
        .collect()
}

fn snapshot_event(
    context: &mut DaemonContext,
    runtime_telemetry: RuntimeTelemetrySnapshot,
    renderer_runtime: RendererRuntimeSnapshot,
) -> Value {
    let render_sync = current_render_sync(context);
    let outputs = output_reports(context, Some(&render_sync));
    json!({
        "desktop": context.desktop,
        "outputs": outputs,
        "persisted_state": context.state,
        "render_sync": render_sync,
        "renderer": renderer_name(),
        "renderer_capabilities": renderer_capabilities(),
        "renderer_runtime": renderer_runtime_report(&renderer_runtime),
        "telemetry": telemetry_report(context, runtime_telemetry, &renderer_runtime),
    })
}

fn state_changed_event(
    action: &str,
    output: Option<&str>,
    context: &DaemonContext,
    render_sync: &StaticRenderSyncPlan,
    runtime_telemetry: RuntimeTelemetrySnapshot,
    renderer_runtime: RendererRuntimeSnapshot,
) -> Value {
    json!({
        "action": action,
        "output": output,
        "desktop": context.desktop,
        "outputs": output_reports(context, Some(render_sync)),
        "persisted_state": context.state,
        "render_sync": render_sync,
        "renderer_capabilities": renderer_capabilities(),
        "renderer_runtime": renderer_runtime_report(&renderer_runtime),
        "telemetry": telemetry_report(context, runtime_telemetry, &renderer_runtime),
    })
}

fn renderer_action_response(
    id: &serde_json::Value,
    accepted_method: &str,
    accepted_params: serde_json::Value,
    render_sync: &StaticRenderSyncPlan,
) -> String {
    let mut result = json!({
        "accepted": true,
        "method": accepted_method,
        "params": accepted_params,
        "renderer": renderer_name(),
        "renderer_capabilities": renderer_capabilities(),
        "render_sync": render_sync,
    });
    if !cfg!(any(
        feature = "native-vulkan-renderer",
        feature = "native-wayland-renderer"
    )) {
        result["note"] = json!("renderer was built without native renderer features");
    }
    gilder::ipc::success_response(id, result)
}

fn renderer_name() -> &'static str {
    match (
        cfg!(feature = "native-vulkan-renderer"),
        cfg!(feature = "native-wayland-renderer"),
    ) {
        (true, true) => "native-vulkan+native-wayland-host",
        (true, false) => "native-vulkan",
        (false, true) => "native-wayland-host",
        (false, false) => "not-implemented",
    }
}

fn renderer_capabilities() -> Value {
    json!({
        "gtk": null,
        "native_wayland": native_wayland_renderer_capabilities(),
        "native_vulkan": native_vulkan_renderer_capabilities(),
        "video": video_renderer_capabilities(),
    })
}

fn telemetry_report(
    context: &DaemonContext,
    runtime_telemetry: RuntimeTelemetrySnapshot,
    renderer_runtime: &RendererRuntimeSnapshot,
) -> Value {
    json!({
        "desktop": {
            "refreshes": context.telemetry.desktop_refreshes,
            "refresh_skips": context.telemetry.desktop_refresh_skips,
            "changes": context.telemetry.desktop_changes,
            "last_refresh_age_ms": context.last_desktop_refresh.map(elapsed_millis_u64),
        },
        "adaptive": {
            "refreshes": context.telemetry.adaptive_refreshes,
            "refresh_skips": context.telemetry.adaptive_refresh_skips,
            "snapshot": context.adaptive_snapshot,
            "action": adaptive_action_report(context),
        },
        "render_sync": render_sync_telemetry_report(context, runtime_telemetry),
        "renderer": renderer_telemetry_report(renderer_runtime),
    })
}

fn render_sync_telemetry_report(
    context: &DaemonContext,
    runtime_telemetry: RuntimeTelemetrySnapshot,
) -> Value {
    let render_sync_cache = context
        .render_sync_cache
        .as_ref()
        .map(|cache| cache.render_sync.cache)
        .unwrap_or_default();
    let mut object = Map::new();
    macro_rules! insert {
        ($key:literal, $value:expr) => {
            object.insert($key.to_owned(), json!($value));
        };
    }

    insert!("cache_hits", context.telemetry.render_sync_cache_hits);
    insert!("cache_misses", context.telemetry.render_sync_cache_misses);
    insert!(
        "updates_queued",
        runtime_telemetry.render_sync_updates_queued
    );
    insert!(
        "updates_skipped",
        runtime_telemetry.render_sync_updates_skipped
    );
    insert!(
        "package_cache_entries",
        render_sync_cache.package_cache_entries
    );
    insert!(
        "package_cache_max_entries",
        render_sync_cache.package_cache_max_entries
    );
    insert!(
        "package_cache_max_retained_unique_resource_bytes",
        render_sync_cache.package_cache_max_retained_unique_resource_bytes
    );
    insert!("package_cache_hits", render_sync_cache.package_cache_hits);
    insert!(
        "package_cache_misses",
        render_sync_cache.package_cache_misses
    );
    insert!(
        "package_cache_evictions",
        render_sync_cache.package_cache_evictions
    );
    insert!(
        "package_cache_retained_resource_references",
        render_sync_cache.package_cache_retained_resource_references
    );
    insert!(
        "package_cache_retained_unique_resources",
        render_sync_cache.package_cache_retained_unique_resources
    );
    insert!(
        "package_cache_retained_resource_bytes",
        render_sync_cache.package_cache_retained_resource_bytes
    );
    insert!(
        "package_cache_retained_unique_resource_bytes",
        render_sync_cache.package_cache_retained_unique_resource_bytes
    );
    insert!(
        "package_cache_retained_preview_resource_references",
        render_sync_cache.package_cache_retained_preview_resource_references
    );
    insert!(
        "package_cache_retained_unique_preview_resources",
        render_sync_cache.package_cache_retained_unique_preview_resources
    );
    insert!(
        "package_cache_retained_preview_resource_bytes",
        render_sync_cache.package_cache_retained_preview_resource_bytes
    );
    insert!(
        "package_cache_retained_unique_preview_resource_bytes",
        render_sync_cache.package_cache_retained_unique_preview_resource_bytes
    );
    insert!(
        "archive_cache_entries",
        render_sync_cache.archive_cache_entries
    );
    insert!(
        "archive_cache_max_entries",
        render_sync_cache.archive_cache_max_entries
    );
    insert!(
        "archive_cache_reuses",
        render_sync_cache.archive_cache_reuses
    );
    insert!(
        "archive_cache_extractions",
        render_sync_cache.archive_cache_extractions
    );
    insert!(
        "archive_cache_evictions",
        context.telemetry.render_archive_cache_evictions
    );
    insert!(
        "archive_cache_evictions_latest",
        render_sync_cache.archive_cache_evictions
    );
    insert!(
        "archive_cache_eviction_errors",
        context.telemetry.render_archive_cache_eviction_errors
    );
    insert!(
        "archive_cache_eviction_errors_latest",
        render_sync_cache.archive_cache_eviction_errors
    );
    insert!(
        "static_image_cache_entries",
        render_sync_cache.static_image_cache_entries
    );
    insert!(
        "static_image_cache_max_entries",
        render_sync_cache.static_image_cache_max_entries
    );
    insert!(
        "static_image_cache_bytes",
        render_sync_cache.static_image_cache_bytes
    );
    insert!(
        "static_image_cache_max_bytes",
        render_sync_cache.static_image_cache_max_bytes
    );
    insert!(
        "static_image_cache_generations",
        render_sync_cache.static_image_cache_generations
    );
    insert!(
        "static_image_cache_reuses",
        render_sync_cache.static_image_cache_reuses
    );
    insert!(
        "static_image_cache_generation_errors",
        render_sync_cache.static_image_cache_generation_errors
    );
    insert!(
        "static_image_cache_evictions",
        render_sync_cache.static_image_cache_evictions
    );
    insert!(
        "static_image_cache_eviction_errors",
        render_sync_cache.static_image_cache_eviction_errors
    );
    insert!(
        "planned_video_source_references",
        render_sync_cache.planned_video_source_references
    );
    insert!(
        "planned_unique_video_sources",
        render_sync_cache.planned_unique_video_sources
    );
    insert!(
        "planned_duplicate_video_source_references",
        render_sync_cache.planned_duplicate_video_source_references
    );
    insert!(
        "planned_max_video_source_outputs",
        render_sync_cache.planned_max_video_source_outputs
    );
    insert!(
        "planned_video_source_reference_bytes",
        render_sync_cache.planned_video_source_reference_bytes
    );
    insert!(
        "planned_unique_video_source_bytes",
        render_sync_cache.planned_unique_video_source_bytes
    );
    insert!(
        "planned_static_image_resources",
        render_sync_cache.planned_static_image_resources
    );
    insert!(
        "planned_video_poster_resources",
        render_sync_cache.planned_video_poster_resources
    );
    insert!(
        "planned_slideshow_image_resources",
        render_sync_cache.planned_slideshow_image_resources
    );
    insert!(
        "planned_scene_image_resources",
        render_sync_cache.planned_scene_image_resources
    );
    insert!(
        "planned_image_resource_references",
        render_sync_cache.planned_image_resource_references
    );
    insert!(
        "planned_unique_image_resources",
        render_sync_cache.planned_unique_image_resources
    );
    insert!(
        "planned_static_image_resource_bytes",
        render_sync_cache.planned_static_image_resource_bytes
    );
    insert!(
        "planned_video_poster_resource_bytes",
        render_sync_cache.planned_video_poster_resource_bytes
    );
    insert!(
        "planned_slideshow_image_resource_bytes",
        render_sync_cache.planned_slideshow_image_resource_bytes
    );
    insert!(
        "planned_scene_image_resource_bytes",
        render_sync_cache.planned_scene_image_resource_bytes
    );
    insert!(
        "planned_image_resource_reference_bytes",
        render_sync_cache.planned_image_resource_reference_bytes
    );
    insert!(
        "planned_unique_image_resource_bytes",
        render_sync_cache.planned_unique_image_resource_bytes
    );

    Value::Object(object)
}

fn adaptive_action_report(context: &DaemonContext) -> Value {
    if !context.adaptive_snapshot.affects_render_plan() {
        return Value::Null;
    }

    let mut names: Vec<String> = context
        .desktop
        .outputs
        .iter()
        .map(|output| output.name.clone())
        .chain(context.state.outputs.keys().cloned())
        .chain(context.config.outputs.keys().cloned())
        .collect();
    names.sort();
    names.dedup();

    let actions = names
        .into_iter()
        .filter(|name| gilder::adaptive::output_enabled(&context.config, name))
        .map(|name| {
            let desktop_output = context.desktop.output(&name);
            match gilder::adaptive::output_action(&context.config, &name) {
                gilder::config::AdaptiveAction::PauseUnfocused
                    if desktop_output.is_some_and(|output| !output.focused) =>
                {
                    json!({
                        "output_name": name,
                        "type": "pause-unfocused",
                    })
                }
                gilder::config::AdaptiveAction::PauseDynamic => {
                    json!({
                        "output_name": name,
                        "type": "pause-dynamic",
                        "scope": "dynamic-wallpapers",
                    })
                }
                action => {
                    let max_fps = gilder::adaptive::output_throttle_max_fps(&context.config, &name);
                    json!({
                        "output_name": name,
                        "type": "throttle",
                        "configured_action": action,
                        "max_fps": max_fps,
                    })
                }
            }
        })
        .collect::<Vec<_>>();
    json!(actions)
}

fn elapsed_millis_u64(instant: Instant) -> u64 {
    instant.elapsed().as_millis().min(u64::MAX as u128) as u64
}

fn video_renderer_capabilities() -> Value {
    json!({
        "built": cfg!(feature = "native-vulkan-video"),
        "headless_worker": false,
        "visible_surface_path": if cfg!(feature = "native-vulkan-video") {
            Some("native-vulkan-video")
        } else {
            None::<&str>
        },
    })
}

#[cfg(feature = "native-wayland-renderer")]
fn native_wayland_renderer_capabilities() -> Value {
    json!(gilder::renderer::native_wayland::capabilities())
}

#[cfg(not(feature = "native-wayland-renderer"))]
fn native_wayland_renderer_capabilities() -> Value {
    json!({
        "built": false,
        "experimental": false,
        "owns_wlr_layer_shell_surface": false,
        "exports_raw_wayland_handles": false,
        "raw_wayland_handles_planned": false,
        "supports_fractional_scale_protocol": false,
        "supports_viewporter_protocol": false,
        "consumes_render_sync": false,
        "unsafe_policy": "unsafe is not used by this build",
    })
}

#[cfg(feature = "native-vulkan-renderer")]
fn native_vulkan_renderer_capabilities() -> Value {
    json!({
        "capabilities": gilder::renderer::native_vulkan::capabilities(),
        "backend_contract": gilder::renderer::native_vulkan::backend_contract(),
    })
}

#[cfg(not(feature = "native-vulkan-renderer"))]
fn native_vulkan_renderer_capabilities() -> Value {
    json!({
        "capabilities": {
            "built": false,
            "experimental": false,
            "default_enabled": false,
            "reuses_native_wayland_host": false,
            "owns_layer_shell_surface_now": false,
            "owns_vulkan_instance_now": false,
            "owns_vulkan_device_now": false,
            "owns_wayland_vulkan_surface_now": false,
            "owns_swapchain_now": false,
            "renders_frames_now": false,
            "consumes_render_sync": false,
            "direct_video_memory_status": "not built",
            "unsafe_policy": "unsafe is not used by this build",
        },
        "backend_contract": null,
    })
}

fn current_render_sync(context: &mut DaemonContext) -> StaticRenderSyncPlan {
    refresh_adaptive_if_stale(context);
    let key = render_sync_cache_key(context);
    if let Some(cache) = &context.render_sync_cache
        && cache.key == key
    {
        context.telemetry.render_sync_cache_hits += 1;
        return cache.render_sync.clone();
    }

    context.telemetry.render_sync_cache_misses += 1;
    let render_sync = gilder::renderer::static_render_sync_plan_with_config_and_adaptive(
        &context.config,
        &context.desktop,
        &context.state,
        &context.paths.cache_dir,
        &context.adaptive_snapshot,
    );
    context.telemetry.render_archive_cache_evictions += render_sync.cache.archive_cache_evictions;
    context.telemetry.render_archive_cache_eviction_errors +=
        render_sync.cache.archive_cache_eviction_errors;
    let key = render_sync_cache_key_for_plan(context, Some(&render_sync));
    context.render_sync_cache = Some(RenderSyncCache {
        key,
        render_sync: render_sync.clone(),
    });
    render_sync
}

fn render_sync_cache_key(context: &DaemonContext) -> RenderSyncCacheKey {
    render_sync_cache_key_for_plan(
        context,
        context
            .render_sync_cache
            .as_ref()
            .map(|cache| &cache.render_sync),
    )
}

fn render_sync_cache_key_for_plan(
    context: &DaemonContext,
    render_sync: Option<&StaticRenderSyncPlan>,
) -> RenderSyncCacheKey {
    RenderSyncCacheKey {
        config: render_sync_config_key(&context.config),
        state: render_sync_state_key(&context.state),
        desktop: context.desktop.clone(),
        adaptive_affects_render_plan: context.adaptive_snapshot.affects_render_plan(),
        playlist_clock: render_sync.and_then(|render_sync| {
            gilder::renderer::current_playlist_clock_cache_key(
                render_sync.playlist_clock_dependency,
            )
        }),
        cache_dir: context.paths.cache_dir.clone(),
        packages: wallpaper_package_fingerprints(context),
        bound_properties: render_sync_bound_property_key(&context.state, render_sync),
    }
}

fn render_sync_config_key(config: &GilderConfig) -> RenderSyncConfigKey {
    RenderSyncConfigKey {
        default_wallpaper: config.default_wallpaper.clone(),
        outputs: config.outputs.clone(),
        adaptive: config.adaptive.clone(),
        video_decoder: config.video.decoder,
        cache: config.cache,
        performance: RenderSyncPerformanceKey {
            interactive_max_fps: config.performance.interactive_max_fps,
            background_max_fps: config.performance.background_max_fps,
            battery_max_fps: config.performance.battery_max_fps,
            fullscreen: config.performance.fullscreen,
            hidden: config.performance.hidden,
            session: config.performance.session,
            unfocused: config.performance.unfocused,
            battery: config.performance.battery,
        },
    }
}

fn render_sync_state_key(state: &AppState) -> RenderSyncStateKey {
    RenderSyncStateKey {
        default_wallpaper: state.default_wallpaper.clone(),
        outputs: state
            .outputs
            .iter()
            .map(|(name, state)| {
                (
                    name.clone(),
                    OutputRenderStateKey {
                        wallpaper: state.wallpaper.clone(),
                        paused: state.paused,
                    },
                )
            })
            .collect(),
    }
}

fn render_sync_bound_property_key(
    state: &AppState,
    render_sync: Option<&StaticRenderSyncPlan>,
) -> Vec<RenderSyncBoundPropertyKey> {
    let Some(render_sync) = render_sync else {
        return Vec::new();
    };
    let mut properties = render_sync
        .scene_plans
        .iter()
        .flat_map(|plan| {
            plan.bound_properties
                .iter()
                .map(|property| RenderSyncBoundPropertyKey {
                    output_name: plan.output_name.clone(),
                    property: property.clone(),
                    value: effective_render_property_key_value(state, &plan.output_name, property),
                })
        })
        .collect::<Vec<_>>();
    properties.sort();
    properties.dedup();
    properties
}

fn effective_render_property_key_value(
    state: &AppState,
    output_name: &str,
    property: &str,
) -> Option<String> {
    let value = state
        .outputs
        .get(output_name)
        .and_then(|output| output.properties.get(property))
        .or_else(|| state.properties.get(property))?;
    serde_json::to_string(value).ok()
}

fn wallpaper_package_fingerprints(context: &DaemonContext) -> Vec<PackageInputFingerprint> {
    let mut paths = Vec::new();
    if let Some(assignment) = &context.state.default_wallpaper {
        paths.push(assignment.path.clone());
    }
    paths.extend(context.state.outputs.values().filter_map(|state| {
        state
            .wallpaper
            .as_ref()
            .map(|assignment| assignment.path.clone())
    }));
    if let Some(path) = &context.config.default_wallpaper {
        paths.push(path.clone());
    }
    paths.extend(
        context
            .config
            .outputs
            .values()
            .filter_map(|output| output.wallpaper.clone()),
    );
    paths.sort();
    paths.dedup();

    paths
        .into_iter()
        .map(|path| PackageInputFingerprint::new(path))
        .collect()
}

impl PackageInputFingerprint {
    fn new(path: String) -> Self {
        let package_path = Path::new(&path);
        let package = metadata_fingerprint(package_path);
        let manifest = if package_path.is_dir()
            || package_path
                .extension()
                .and_then(|extension| extension.to_str())
                == Some("gwpdir")
        {
            Some(PackageManifestFingerprint {
                json: metadata_fingerprint(&package_path.join(gilder::core::MANIFEST_FILE)),
                toml: metadata_fingerprint(&package_path.join(gilder::core::MANIFEST_TOML_FILE)),
            })
        } else {
            None
        };
        Self {
            path,
            package,
            manifest,
        }
    }
}

fn metadata_fingerprint(path: &Path) -> MetadataFingerprint {
    match fs::metadata(path) {
        Ok(metadata) => MetadataFingerprint::Available {
            is_dir: metadata.is_dir(),
            is_file: metadata.is_file(),
            len: metadata.len(),
            modified: metadata.modified().ok(),
        },
        Err(err) => MetadataFingerprint::Unavailable(err.kind().to_string()),
    }
}

fn refreshed_render_sync(runtime: &DaemonRuntime) -> Result<StaticRenderSyncPlan, String> {
    let mut context = runtime.lock_context()?;
    refresh_desktop(&mut context);
    Ok(current_render_sync(&mut context))
}

fn refresh_runtime_desktop_if_changed(runtime: &DaemonRuntime) -> Result<(), String> {
    let Some((event_type, event, render_sync)) = ({
        let mut context = runtime.lock_context()?;
        let previous_desktop = context.desktop.clone();
        let previous_adaptive_affects_render_plan = context.adaptive_snapshot.affects_render_plan();
        refresh_desktop(&mut context);
        refresh_adaptive_if_stale(&mut context);
        let desktop_changed = context.desktop != previous_desktop;
        let adaptive_affects_render_plan_changed = context.adaptive_snapshot.affects_render_plan()
            != previous_adaptive_affects_render_plan;

        if !desktop_changed && !adaptive_affects_render_plan_changed {
            None
        } else {
            if desktop_changed {
                context.telemetry.desktop_changes += 1;
            }
            let render_sync = current_render_sync(&mut context);
            let event = runtime_changed_event(
                &context,
                &render_sync,
                runtime.telemetry_snapshot(),
                runtime.renderer_runtime_snapshot(),
            );
            let event_type = if desktop_changed {
                "desktop.changed"
            } else {
                "adaptive.changed"
            };
            Some((event_type, event, render_sync))
        }
    }) else {
        return Ok(());
    };

    runtime.queue_render_sync_if_changed(render_sync);
    runtime.watchers.broadcast(event_type, event);
    Ok(())
}

fn runtime_changed_event(
    context: &DaemonContext,
    render_sync: &StaticRenderSyncPlan,
    runtime_telemetry: RuntimeTelemetrySnapshot,
    renderer_runtime: RendererRuntimeSnapshot,
) -> Value {
    json!({
        "desktop": context.desktop,
        "outputs": output_reports(context, Some(render_sync)),
        "persisted_state": context.state,
        "render_sync": render_sync,
        "renderer": renderer_name(),
        "renderer_capabilities": renderer_capabilities(),
        "renderer_runtime": renderer_runtime_report(&renderer_runtime),
        "telemetry": telemetry_report(context, runtime_telemetry, &renderer_runtime),
    })
}

fn runtime_desktop_refresh_interval(runtime: &DaemonRuntime) -> Duration {
    match runtime.lock_context() {
        Ok(context) => desktop_refresh_interval(&context.config.performance),
        Err(err) => {
            eprintln!("gilderd: failed to read desktop refresh interval: {err}");
            desktop_refresh_interval(&PerformanceConfig::default())
        }
    }
}

fn adaptive_refresh_interval(config: &gilder::config::AdaptiveConfig) -> Duration {
    Duration::from_millis(config.refresh_interval_ms.max(250))
}

fn desktop_refresh_interval(config: &PerformanceConfig) -> Duration {
    Duration::from_millis(config.desktop_refresh_interval_ms.max(250))
}
