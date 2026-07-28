use serde::Deserialize;

use super::{bool_csv, csv_cell};

#[derive(Debug, Deserialize)]
struct StatusResponse {
    #[serde(default)]
    result: Option<StatusResult>,
    #[serde(default)]
    error: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct StatusResult {
    #[serde(default)]
    telemetry: Telemetry,
}

pub(super) fn render_telemetry_csv(response: &str) -> Result<String, String> {
    let response: StatusResponse =
        serde_json::from_str(response).map_err(|err| format!("failed to parse response: {err}"))?;
    if let Some(error) = response.error {
        return Err(format!("daemon returned error: {error}"));
    }
    let result = response
        .result
        .ok_or_else(|| "status response did not contain result".to_owned())?;

    let telemetry = result.telemetry;
    let mut csv = String::from(
        "desktop_refreshes,desktop_refresh_skips,desktop_changes,last_desktop_refresh_age_ms,render_sync_cache_hits,render_sync_cache_misses,render_sync_updates_queued,render_sync_updates_skipped,render_sync_package_cache_entries,render_sync_package_cache_max_entries,render_sync_package_cache_hits,render_sync_package_cache_misses,render_sync_package_cache_evictions,render_sync_archive_cache_entries,render_sync_archive_cache_max_entries,render_sync_archive_cache_reuses,render_sync_archive_cache_extractions,render_sync_archive_cache_evictions,render_sync_archive_cache_evictions_latest,render_sync_archive_cache_eviction_errors,render_sync_archive_cache_eviction_errors_latest,render_sync_planned_static_image_resources,render_sync_planned_video_poster_resources,render_sync_planned_slideshow_image_resources,render_sync_planned_image_resource_references,render_sync_planned_unique_image_resources,adaptive_refreshes,adaptive_refresh_skips,adaptive_active_triggers,cpu_pressure_some_avg10_x100,memory_pressure_some_avg10_x100,temperature_max_millicelsius,power_external_online,power_system_battery_present,power_battery_discharging,power_battery_capacity_percent,power_battery_power_microwatts,gpu_busy_percent_avg,gpu_busy_percent_max,gpu_busy_sources,adaptive_action_types,adaptive_action_scopes,adaptive_action_configured_actions,adaptive_action_max_fps,renderer_output_windows,renderer_static_surfaces,renderer_static_picture_surfaces,renderer_static_css_surfaces,renderer_static_color_surfaces,renderer_slideshow_surfaces,renderer_video_surfaces,renderer_video_shared_runtimes,renderer_video_pipelines,renderer_video_qos_messages,renderer_video_qos_dropped_max,render_sync_planned_static_image_resource_bytes,render_sync_planned_video_poster_resource_bytes,render_sync_planned_slideshow_image_resource_bytes,render_sync_planned_image_resource_reference_bytes,render_sync_planned_unique_image_resource_bytes,render_sync_package_cache_retained_resource_references,render_sync_package_cache_retained_unique_resources,render_sync_package_cache_retained_resource_bytes,render_sync_package_cache_retained_unique_resource_bytes,renderer_static_surface_resource_references,renderer_static_surface_resource_bytes,renderer_slideshow_resource_references,renderer_slideshow_resource_bytes,renderer_static_surface_unique_resources,renderer_static_surface_unique_resource_bytes,renderer_static_surface_estimated_decoded_bytes,renderer_slideshow_unique_resources,renderer_slideshow_unique_resource_bytes,render_sync_static_image_cache_entries,render_sync_static_image_cache_max_entries,render_sync_static_image_cache_generations,render_sync_static_image_cache_reuses,render_sync_static_image_cache_generation_errors,render_sync_static_image_cache_evictions,render_sync_static_image_cache_eviction_errors,render_sync_planned_video_source_references,render_sync_planned_unique_video_sources,render_sync_planned_duplicate_video_source_references,render_sync_planned_max_video_source_outputs,render_sync_planned_video_source_reference_bytes,render_sync_planned_unique_video_source_bytes,renderer_video_pipeline_source_references,renderer_video_pipeline_source_reference_bytes,renderer_video_pipeline_unique_sources,renderer_video_pipeline_unique_source_bytes,render_sync_package_cache_max_retained_unique_resource_bytes,render_sync_static_image_cache_bytes,render_sync_static_image_cache_max_bytes,render_sync_package_cache_retained_preview_resource_references,render_sync_package_cache_retained_unique_preview_resources,render_sync_package_cache_retained_preview_resource_bytes,render_sync_package_cache_retained_unique_preview_resource_bytes\n",
    );
    let adaptive_sample = telemetry.adaptive.snapshot.sample.as_ref();
    let adaptive_actions = telemetry.adaptive.action.as_deref();
    let row = [
        telemetry.desktop.refreshes.to_string(),
        telemetry.desktop.refresh_skips.to_string(),
        telemetry.desktop.changes.to_string(),
        telemetry
            .desktop
            .last_refresh_age_ms
            .map(|age| age.to_string())
            .unwrap_or_default(),
        telemetry.render_sync.cache_hits.to_string(),
        telemetry.render_sync.cache_misses.to_string(),
        telemetry.render_sync.updates_queued.to_string(),
        telemetry.render_sync.updates_skipped.to_string(),
        telemetry.render_sync.package_cache_entries.to_string(),
        telemetry.render_sync.package_cache_max_entries.to_string(),
        telemetry.render_sync.package_cache_hits.to_string(),
        telemetry.render_sync.package_cache_misses.to_string(),
        telemetry.render_sync.package_cache_evictions.to_string(),
        telemetry.render_sync.archive_cache_entries.to_string(),
        telemetry.render_sync.archive_cache_max_entries.to_string(),
        telemetry.render_sync.archive_cache_reuses.to_string(),
        telemetry.render_sync.archive_cache_extractions.to_string(),
        telemetry.render_sync.archive_cache_evictions.to_string(),
        telemetry
            .render_sync
            .archive_cache_evictions_latest
            .to_string(),
        telemetry
            .render_sync
            .archive_cache_eviction_errors
            .to_string(),
        telemetry
            .render_sync
            .archive_cache_eviction_errors_latest
            .to_string(),
        telemetry
            .render_sync
            .planned_static_image_resources
            .to_string(),
        telemetry
            .render_sync
            .planned_video_poster_resources
            .to_string(),
        telemetry
            .render_sync
            .planned_slideshow_image_resources
            .to_string(),
        telemetry
            .render_sync
            .planned_image_resource_references
            .to_string(),
        telemetry
            .render_sync
            .planned_unique_image_resources
            .to_string(),
        telemetry.adaptive.refreshes.to_string(),
        telemetry.adaptive.refresh_skips.to_string(),
        telemetry
            .adaptive
            .snapshot
            .active_triggers
            .len()
            .to_string(),
        telemetry
            .adaptive
            .snapshot
            .sample
            .as_ref()
            .and_then(|sample| sample.cpu_pressure_some_avg10_x100)
            .map(|value| value.to_string())
            .unwrap_or_default(),
        adaptive_sample
            .and_then(|sample| sample.memory_pressure_some_avg10_x100)
            .map(|value| value.to_string())
            .unwrap_or_default(),
        adaptive_sample
            .and_then(|sample| sample.temperature_max_millicelsius)
            .map(|value| value.to_string())
            .unwrap_or_default(),
        adaptive_sample
            .and_then(|sample| sample.power_external_online)
            .map(bool_csv)
            .unwrap_or_default(),
        adaptive_sample
            .and_then(|sample| sample.power_system_battery_present)
            .map(bool_csv)
            .unwrap_or_default(),
        adaptive_sample
            .and_then(|sample| sample.power_battery_discharging)
            .map(bool_csv)
            .unwrap_or_default(),
        adaptive_sample
            .and_then(|sample| sample.power_battery_capacity_percent)
            .map(|value| value.to_string())
            .unwrap_or_default(),
        adaptive_sample
            .and_then(|sample| sample.power_battery_power_microwatts)
            .map(|value| value.to_string())
            .unwrap_or_default(),
        adaptive_sample
            .and_then(|sample| sample.gpu_busy_percent_avg)
            .map(|value| value.to_string())
            .unwrap_or_default(),
        adaptive_sample
            .and_then(|sample| sample.gpu_busy_percent_max)
            .map(|value| value.to_string())
            .unwrap_or_default(),
        adaptive_sample
            .map(|sample| csv_cell(&pipe_join(sample.gpu_busy_sources.clone())))
            .unwrap_or_default(),
        csv_cell(&adaptive_action_values(adaptive_actions, |action| {
            Some(action.kind.clone())
        })),
        csv_cell(&adaptive_action_values(adaptive_actions, |action| {
            action.scope.clone()
        })),
        csv_cell(&adaptive_action_values(adaptive_actions, |action| {
            action.configured_action.clone()
        })),
        csv_cell(&adaptive_action_values(adaptive_actions, |action| {
            action.max_fps.map(|max_fps| max_fps.to_string())
        })),
        telemetry.renderer.output_windows.to_string(),
        telemetry.renderer.static_surfaces.to_string(),
        telemetry.renderer.static_picture_surfaces.to_string(),
        telemetry.renderer.static_css_surfaces.to_string(),
        telemetry.renderer.static_color_surfaces.to_string(),
        telemetry.renderer.slideshow_surfaces.to_string(),
        telemetry.renderer.video_surfaces.to_string(),
        telemetry.renderer.video_shared_runtimes.to_string(),
        telemetry.renderer.video_pipelines.to_string(),
        telemetry.renderer.video_qos_messages.to_string(),
        telemetry
            .renderer
            .video_qos_dropped_max
            .map(|value| value.to_string())
            .unwrap_or_default(),
        telemetry
            .render_sync
            .planned_static_image_resource_bytes
            .to_string(),
        telemetry
            .render_sync
            .planned_video_poster_resource_bytes
            .to_string(),
        telemetry
            .render_sync
            .planned_slideshow_image_resource_bytes
            .to_string(),
        telemetry
            .render_sync
            .planned_image_resource_reference_bytes
            .to_string(),
        telemetry
            .render_sync
            .planned_unique_image_resource_bytes
            .to_string(),
        telemetry
            .render_sync
            .package_cache_retained_resource_references
            .to_string(),
        telemetry
            .render_sync
            .package_cache_retained_unique_resources
            .to_string(),
        telemetry
            .render_sync
            .package_cache_retained_resource_bytes
            .to_string(),
        telemetry
            .render_sync
            .package_cache_retained_unique_resource_bytes
            .to_string(),
        telemetry
            .renderer
            .static_surface_resource_references
            .to_string(),
        telemetry.renderer.static_surface_resource_bytes.to_string(),
        telemetry.renderer.slideshow_resource_references.to_string(),
        telemetry.renderer.slideshow_resource_bytes.to_string(),
        telemetry
            .renderer
            .static_surface_unique_resources
            .to_string(),
        telemetry
            .renderer
            .static_surface_unique_resource_bytes
            .to_string(),
        telemetry
            .renderer
            .static_surface_estimated_decoded_bytes
            .to_string(),
        telemetry.renderer.slideshow_unique_resources.to_string(),
        telemetry
            .renderer
            .slideshow_unique_resource_bytes
            .to_string(),
        telemetry.render_sync.static_image_cache_entries.to_string(),
        telemetry
            .render_sync
            .static_image_cache_max_entries
            .to_string(),
        telemetry
            .render_sync
            .static_image_cache_generations
            .to_string(),
        telemetry.render_sync.static_image_cache_reuses.to_string(),
        telemetry
            .render_sync
            .static_image_cache_generation_errors
            .to_string(),
        telemetry
            .render_sync
            .static_image_cache_evictions
            .to_string(),
        telemetry
            .render_sync
            .static_image_cache_eviction_errors
            .to_string(),
        telemetry
            .render_sync
            .planned_video_source_references
            .to_string(),
        telemetry
            .render_sync
            .planned_unique_video_sources
            .to_string(),
        telemetry
            .render_sync
            .planned_duplicate_video_source_references
            .to_string(),
        telemetry
            .render_sync
            .planned_max_video_source_outputs
            .to_string(),
        telemetry
            .render_sync
            .planned_video_source_reference_bytes
            .to_string(),
        telemetry
            .render_sync
            .planned_unique_video_source_bytes
            .to_string(),
        telemetry
            .renderer
            .video_pipeline_source_references
            .to_string(),
        telemetry
            .renderer
            .video_pipeline_source_reference_bytes
            .to_string(),
        telemetry.renderer.video_pipeline_unique_sources.to_string(),
        telemetry
            .renderer
            .video_pipeline_unique_source_bytes
            .to_string(),
        telemetry
            .render_sync
            .package_cache_max_retained_unique_resource_bytes
            .to_string(),
        telemetry.render_sync.static_image_cache_bytes.to_string(),
        telemetry
            .render_sync
            .static_image_cache_max_bytes
            .to_string(),
        telemetry
            .render_sync
            .package_cache_retained_preview_resource_references
            .to_string(),
        telemetry
            .render_sync
            .package_cache_retained_unique_preview_resources
            .to_string(),
        telemetry
            .render_sync
            .package_cache_retained_preview_resource_bytes
            .to_string(),
        telemetry
            .render_sync
            .package_cache_retained_unique_preview_resource_bytes
            .to_string(),
    ];
    csv.push_str(&row.join(","));
    csv.push('\n');
    Ok(csv)
}

fn adaptive_action_values(
    actions: Option<&[AdaptiveActionReport]>,
    value: impl Fn(&AdaptiveActionReport) -> Option<String>,
) -> String {
    pipe_join(
        actions
            .unwrap_or_default()
            .iter()
            .filter_map(value)
            .collect::<Vec<_>>(),
    )
}

fn pipe_join(mut values: Vec<String>) -> String {
    values.retain(|value| !value.is_empty());
    values.sort();
    values.dedup();
    values.join("|")
}

#[derive(Debug, Default, Deserialize)]
struct Telemetry {
    #[serde(default)]
    desktop: DesktopTelemetry,
    #[serde(default)]
    render_sync: RenderSyncTelemetry,
    #[serde(default)]
    adaptive: AdaptiveTelemetry,
    #[serde(default)]
    renderer: RendererTelemetry,
}

#[derive(Debug, Default, Deserialize)]
struct DesktopTelemetry {
    #[serde(default)]
    refreshes: u64,
    #[serde(default)]
    refresh_skips: u64,
    #[serde(default)]
    changes: u64,
    #[serde(default)]
    last_refresh_age_ms: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
struct RenderSyncTelemetry {
    #[serde(default)]
    cache_hits: u64,
    #[serde(default)]
    cache_misses: u64,
    #[serde(default)]
    updates_queued: u64,
    #[serde(default)]
    updates_skipped: u64,
    #[serde(default)]
    package_cache_entries: u64,
    #[serde(default)]
    package_cache_max_entries: u64,
    #[serde(default)]
    package_cache_max_retained_unique_resource_bytes: u64,
    #[serde(default)]
    package_cache_hits: u64,
    #[serde(default)]
    package_cache_misses: u64,
    #[serde(default)]
    package_cache_evictions: u64,
    #[serde(default)]
    package_cache_retained_resource_references: u64,
    #[serde(default)]
    package_cache_retained_unique_resources: u64,
    #[serde(default)]
    package_cache_retained_resource_bytes: u64,
    #[serde(default)]
    package_cache_retained_unique_resource_bytes: u64,
    #[serde(default)]
    package_cache_retained_preview_resource_references: u64,
    #[serde(default)]
    package_cache_retained_unique_preview_resources: u64,
    #[serde(default)]
    package_cache_retained_preview_resource_bytes: u64,
    #[serde(default)]
    package_cache_retained_unique_preview_resource_bytes: u64,
    #[serde(default)]
    archive_cache_entries: u64,
    #[serde(default)]
    archive_cache_max_entries: u64,
    #[serde(default)]
    archive_cache_reuses: u64,
    #[serde(default)]
    archive_cache_extractions: u64,
    #[serde(default)]
    archive_cache_evictions: u64,
    #[serde(default)]
    archive_cache_evictions_latest: u64,
    #[serde(default)]
    archive_cache_eviction_errors: u64,
    #[serde(default)]
    archive_cache_eviction_errors_latest: u64,
    #[serde(default)]
    static_image_cache_entries: u64,
    #[serde(default)]
    static_image_cache_max_entries: u64,
    #[serde(default)]
    static_image_cache_bytes: u64,
    #[serde(default)]
    static_image_cache_max_bytes: u64,
    #[serde(default)]
    static_image_cache_generations: u64,
    #[serde(default)]
    static_image_cache_reuses: u64,
    #[serde(default)]
    static_image_cache_generation_errors: u64,
    #[serde(default)]
    static_image_cache_evictions: u64,
    #[serde(default)]
    static_image_cache_eviction_errors: u64,
    #[serde(default)]
    planned_video_source_references: u64,
    #[serde(default)]
    planned_unique_video_sources: u64,
    #[serde(default)]
    planned_duplicate_video_source_references: u64,
    #[serde(default)]
    planned_max_video_source_outputs: u64,
    #[serde(default)]
    planned_video_source_reference_bytes: u64,
    #[serde(default)]
    planned_unique_video_source_bytes: u64,
    #[serde(default)]
    planned_static_image_resources: u64,
    #[serde(default)]
    planned_video_poster_resources: u64,
    #[serde(default)]
    planned_slideshow_image_resources: u64,
    #[serde(default)]
    planned_image_resource_references: u64,
    #[serde(default)]
    planned_unique_image_resources: u64,
    #[serde(default)]
    planned_static_image_resource_bytes: u64,
    #[serde(default)]
    planned_video_poster_resource_bytes: u64,
    #[serde(default)]
    planned_slideshow_image_resource_bytes: u64,
    #[serde(default)]
    planned_image_resource_reference_bytes: u64,
    #[serde(default)]
    planned_unique_image_resource_bytes: u64,
}

#[derive(Debug, Default, Deserialize)]
struct AdaptiveTelemetry {
    #[serde(default)]
    refreshes: u64,
    #[serde(default)]
    refresh_skips: u64,
    #[serde(default)]
    snapshot: AdaptiveSnapshot,
    #[serde(default)]
    action: Option<Vec<AdaptiveActionReport>>,
}

#[derive(Debug, Deserialize)]
struct AdaptiveActionReport {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    configured_action: Option<String>,
    #[serde(default)]
    max_fps: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
struct RendererTelemetry {
    #[serde(default)]
    output_windows: u64,
    #[serde(default)]
    static_surfaces: u64,
    #[serde(default)]
    static_picture_surfaces: u64,
    #[serde(default)]
    static_css_surfaces: u64,
    #[serde(default)]
    static_color_surfaces: u64,
    #[serde(default)]
    slideshow_surfaces: u64,
    #[serde(default)]
    video_surfaces: u64,
    #[serde(default)]
    video_shared_runtimes: u64,
    #[serde(default)]
    static_surface_resource_references: u64,
    #[serde(default)]
    static_surface_resource_bytes: u64,
    #[serde(default)]
    static_surface_unique_resources: u64,
    #[serde(default)]
    static_surface_unique_resource_bytes: u64,
    #[serde(default)]
    static_surface_estimated_decoded_bytes: u64,
    #[serde(default)]
    slideshow_resource_references: u64,
    #[serde(default)]
    slideshow_resource_bytes: u64,
    #[serde(default)]
    slideshow_unique_resources: u64,
    #[serde(default)]
    slideshow_unique_resource_bytes: u64,
    #[serde(default)]
    video_pipeline_source_references: u64,
    #[serde(default)]
    video_pipeline_source_reference_bytes: u64,
    #[serde(default)]
    video_pipeline_unique_sources: u64,
    #[serde(default)]
    video_pipeline_unique_source_bytes: u64,
    #[serde(default)]
    video_pipelines: u64,
    #[serde(default)]
    video_qos_messages: u64,
    #[serde(default)]
    video_qos_dropped_max: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
struct AdaptiveSnapshot {
    #[serde(default)]
    sample: Option<AdaptiveSample>,
    #[serde(default)]
    active_triggers: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct AdaptiveSample {
    #[serde(default)]
    cpu_pressure_some_avg10_x100: Option<u32>,
    #[serde(default)]
    memory_pressure_some_avg10_x100: Option<u32>,
    #[serde(default)]
    temperature_max_millicelsius: Option<i32>,
    #[serde(default)]
    power_external_online: Option<bool>,
    #[serde(default)]
    power_system_battery_present: Option<bool>,
    #[serde(default)]
    power_battery_discharging: Option<bool>,
    #[serde(default)]
    power_battery_capacity_percent: Option<u32>,
    #[serde(default)]
    power_battery_power_microwatts: Option<u64>,
    #[serde(default)]
    gpu_busy_percent_avg: Option<u32>,
    #[serde(default)]
    gpu_busy_percent_max: Option<u32>,
    #[serde(default)]
    gpu_busy_sources: Vec<String>,
}
