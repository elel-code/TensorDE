use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use serde::Deserialize;

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        println!("{}", gilder::ipc::help_text());
        return Ok(());
    }

    let invocation = parse_invocation(&args)?;
    let command = invocation.command.clone();
    if let Some(response_file) = invocation.response_file {
        let response = fs::read_to_string(&response_file)
            .map_err(|err| format!("failed to read {}: {err}", response_file.display()))?;
        print_response(&response, invocation.format)?;
        return Ok(());
    }

    let socket = env::var_os("GILDER_SOCKET")
        .map(PathBuf::from)
        .or_else(gilder::ipc::runtime_socket_path)
        .ok_or_else(|| {
            "XDG_RUNTIME_DIR is not set; pass GILDER_SOCKET=/path/to/socket".to_owned()
        })?;

    let mut stream = UnixStream::connect(&socket)
        .map_err(|err| format!("failed to connect to {}: {err}", socket.display()))?;

    let request = command.to_json_line();
    stream
        .write_all(request.as_bytes())
        .and_then(|_| stream.write_all(b"\n"))
        .map_err(|err| format!("failed to send request: {err}"))?;

    if matches!(command, gilder::ipc::ClientCommand::Watch) {
        let mut stdout = std::io::stdout().lock();
        let reader = BufReader::new(stream);
        for line in reader.lines() {
            let line = line.map_err(|err| format!("failed to read response: {err}"))?;
            stdout
                .write_all(line.as_bytes())
                .and_then(|_| stdout.write_all(b"\n"))
                .and_then(|_| stdout.flush())
                .map_err(|err| format!("failed to write response: {err}"))?;
        }
        return Ok(());
    }

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|err| format!("failed to read response: {err}"))?;
    print_response(&response, invocation.format)?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
struct Invocation {
    command: gilder::ipc::ClientCommand,
    format: ResponseFormat,
    response_file: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResponseFormat {
    Json,
    DecisionsCsv,
    TelemetryCsv,
}

fn parse_invocation(args: &[String]) -> Result<Invocation, String> {
    match args {
        [cmd, format] if cmd == "status" && format == "--decisions-csv" => Ok(Invocation {
            command: gilder::ipc::ClientCommand::Status,
            format: ResponseFormat::DecisionsCsv,
            response_file: None,
        }),
        [cmd, format, from_file, path]
            if cmd == "status" && format == "--decisions-csv" && from_file == "--from-file" =>
        {
            Ok(Invocation {
                command: gilder::ipc::ClientCommand::Status,
                format: ResponseFormat::DecisionsCsv,
                response_file: Some(PathBuf::from(path)),
            })
        }
        [cmd, format] if cmd == "status" && format == "--telemetry-csv" => Ok(Invocation {
            command: gilder::ipc::ClientCommand::Status,
            format: ResponseFormat::TelemetryCsv,
            response_file: None,
        }),
        [cmd, format, from_file, path]
            if cmd == "status" && format == "--telemetry-csv" && from_file == "--from-file" =>
        {
            Ok(Invocation {
                command: gilder::ipc::ClientCommand::Status,
                format: ResponseFormat::TelemetryCsv,
                response_file: Some(PathBuf::from(path)),
            })
        }
        [cmd, from_file, path] if cmd == "status" && from_file == "--from-file" => Ok(Invocation {
            command: gilder::ipc::ClientCommand::Status,
            format: ResponseFormat::Json,
            response_file: Some(PathBuf::from(path)),
        }),
        _ => Ok(Invocation {
            command: gilder::ipc::parse_client_args(args)?,
            format: ResponseFormat::Json,
            response_file: None,
        }),
    }
}

fn print_response(response: &str, format: ResponseFormat) -> Result<(), String> {
    match format {
        ResponseFormat::Json => {
            print!("{response}");
            Ok(())
        }
        ResponseFormat::DecisionsCsv => {
            print!("{}", render_decisions_csv(response)?);
            Ok(())
        }
        ResponseFormat::TelemetryCsv => {
            print!("{}", render_telemetry_csv(response)?);
            Ok(())
        }
    }
}

fn render_decisions_csv(response: &str) -> Result<String, String> {
    let response: StatusResponse =
        serde_json::from_str(response).map_err(|err| format!("failed to parse response: {err}"))?;
    if let Some(error) = response.error {
        return Err(format!("daemon returned error: {error}"));
    }
    let result = response
        .result
        .ok_or_else(|| "status response did not contain result".to_owned())?;

    let sync = result.render_sync;
    let plan_details = render_plan_details(&sync);
    let mut csv = String::from(
        "output_name,action,mode,reason,max_fps,wallpaper,plan_kind,source,fit,target_max_fps,muted\n",
    );
    for decision in &sync.decisions {
        let details = plan_details.get(decision.output_name.as_str());
        let row = [
            csv_cell(&decision.output_name),
            csv_cell(&decision.action),
            csv_cell(&decision.performance.mode_name),
            csv_cell(&decision.performance.reason),
            csv_cell(
                &decision
                    .performance
                    .max_fps
                    .map(|max_fps| max_fps.to_string())
                    .unwrap_or_default(),
            ),
            csv_cell(decision.wallpaper.as_deref().unwrap_or_default()),
            csv_cell(details.map(|details| details.kind).unwrap_or_default()),
            csv_cell(details.map(|details| details.source).unwrap_or_default()),
            csv_cell(details.map(|details| details.fit).unwrap_or_default()),
            csv_cell(
                &details
                    .and_then(|details| details.target_max_fps)
                    .map(|max_fps| max_fps.to_string())
                    .unwrap_or_default(),
            ),
            csv_cell(
                details
                    .and_then(|details| details.muted)
                    .map(|muted| if muted { "true" } else { "false" })
                    .unwrap_or_default(),
            ),
        ];
        csv.push_str(&row.join(","));
        csv.push('\n');
    }
    Ok(csv)
}

fn render_telemetry_csv(response: &str) -> Result<String, String> {
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

fn render_plan_details(sync: &RenderSync) -> BTreeMap<&str, PlanCsvDetails<'_>> {
    let mut details = BTreeMap::new();
    for plan in &sync.plans {
        details.insert(
            plan.output_name.as_str(),
            PlanCsvDetails {
                kind: "static-image",
                source: plan.source.as_str(),
                fit: plan.fit.as_str(),
                target_max_fps: None,
                muted: None,
            },
        );
    }
    for plan in &sync.video_plans {
        details.insert(
            plan.output_name.as_str(),
            PlanCsvDetails {
                kind: "video",
                source: plan.source.as_str(),
                fit: plan.fit.as_str(),
                target_max_fps: plan.target_max_fps,
                muted: Some(plan.muted),
            },
        );
    }
    for plan in &sync.slideshow_plans {
        details.insert(
            plan.output_name.as_str(),
            PlanCsvDetails {
                kind: "slideshow",
                source: plan.sources.first().map(String::as_str).unwrap_or_default(),
                fit: plan.fit.as_str(),
                target_max_fps: plan.target_max_fps,
                muted: None,
            },
        );
    }
    for plan in &sync.scene_plans {
        details.insert(
            plan.output_name.as_str(),
            PlanCsvDetails {
                kind: "scene",
                source: plan.csv_source(),
                fit: plan.csv_fit(),
                target_max_fps: plan.target_max_fps,
                muted: None,
            },
        );
    }
    details
}

#[derive(Debug, Clone, Copy)]
struct PlanCsvDetails<'a> {
    kind: &'static str,
    source: &'a str,
    fit: &'a str,
    target_max_fps: Option<u32>,
    muted: Option<bool>,
}

fn csv_cell(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

fn bool_csv(value: bool) -> String {
    if value {
        "true".to_owned()
    } else {
        "false".to_owned()
    }
}

#[derive(Debug, Deserialize)]
struct StatusResponse {
    #[serde(default)]
    result: Option<StatusResult>,
    #[serde(default)]
    error: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct StatusResult {
    render_sync: RenderSync,
    #[serde(default)]
    telemetry: Telemetry,
}

#[derive(Debug, Deserialize)]
struct RenderSync {
    #[serde(default)]
    plans: Vec<StaticPlan>,
    #[serde(default)]
    video_plans: Vec<VideoPlan>,
    #[serde(default)]
    slideshow_plans: Vec<SlideshowPlan>,
    #[serde(default)]
    scene_plans: Vec<ScenePlan>,
    #[serde(default)]
    decisions: Vec<RenderDecision>,
}

#[derive(Debug, Deserialize)]
struct StaticPlan {
    output_name: String,
    source: String,
    fit: String,
}

#[derive(Debug, Deserialize)]
struct VideoPlan {
    output_name: String,
    source: String,
    fit: String,
    #[serde(default)]
    target_max_fps: Option<u32>,
    muted: bool,
}

#[derive(Debug, Deserialize)]
struct SlideshowPlan {
    output_name: String,
    sources: Vec<String>,
    fit: String,
    #[serde(default)]
    target_max_fps: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct ScenePlan {
    output_name: String,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    target_max_fps: Option<u32>,
    #[serde(default)]
    display: Option<SceneDisplay>,
}

impl ScenePlan {
    fn csv_source(&self) -> &str {
        match &self.display {
            Some(SceneDisplay::Image { source, .. }) => source.as_str(),
            Some(SceneDisplay::Color { color }) => color.as_str(),
            None => self.source.as_deref().unwrap_or_default(),
        }
    }

    fn csv_fit(&self) -> &str {
        match &self.display {
            Some(SceneDisplay::Image { fit, .. }) => fit.as_str(),
            Some(SceneDisplay::Color { .. }) | None => "",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum SceneDisplay {
    Image { source: String, fit: String },
    Color { color: String },
}

#[derive(Debug, Deserialize)]
struct RenderDecision {
    output_name: String,
    action: String,
    performance: DecisionPerformance,
    #[serde(default)]
    wallpaper: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DecisionPerformance {
    #[serde(rename = "mode")]
    mode_name: String,
    #[serde(default)]
    max_fps: Option<u32>,
    reason: String,
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

#[cfg(test)]
#[path = "gilderctl/tests.rs"]
mod tests;
