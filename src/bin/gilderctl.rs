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
    VideoRuntimeCsv,
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
        [cmd, format] if cmd == "status" && format == "--video-runtime-csv" => Ok(Invocation {
            command: gilder::ipc::ClientCommand::Status,
            format: ResponseFormat::VideoRuntimeCsv,
            response_file: None,
        }),
        [cmd, format, from_file, path]
            if cmd == "status" && format == "--video-runtime-csv" && from_file == "--from-file" =>
        {
            Ok(Invocation {
                command: gilder::ipc::ClientCommand::Status,
                format: ResponseFormat::VideoRuntimeCsv,
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
        ResponseFormat::VideoRuntimeCsv => {
            print!("{}", render_video_runtime_csv(response)?);
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
        "desktop_refreshes,desktop_refresh_skips,desktop_changes,last_desktop_refresh_age_ms,render_sync_cache_hits,render_sync_cache_misses,render_sync_updates_queued,render_sync_updates_skipped,render_sync_package_cache_entries,render_sync_package_cache_max_entries,render_sync_package_cache_hits,render_sync_package_cache_misses,render_sync_package_cache_evictions,render_sync_archive_cache_entries,render_sync_archive_cache_max_entries,render_sync_archive_cache_reuses,render_sync_archive_cache_extractions,render_sync_archive_cache_evictions,render_sync_archive_cache_evictions_latest,render_sync_archive_cache_eviction_errors,render_sync_archive_cache_eviction_errors_latest,render_sync_planned_static_image_resources,render_sync_planned_video_poster_resources,render_sync_planned_slideshow_image_resources,render_sync_planned_image_resource_references,render_sync_planned_unique_image_resources,adaptive_refreshes,adaptive_refresh_skips,adaptive_active_triggers,cpu_pressure_some_avg10_x100,memory_pressure_some_avg10_x100,temperature_max_millicelsius,power_external_online,power_system_battery_present,power_battery_discharging,power_battery_capacity_percent,power_battery_power_microwatts,gpu_busy_percent_avg,gpu_busy_percent_max,gpu_busy_sources,adaptive_action_types,adaptive_action_scopes,adaptive_action_configured_actions,adaptive_action_max_fps,renderer_output_windows,renderer_static_surfaces,renderer_static_picture_surfaces,renderer_static_css_surfaces,renderer_static_color_surfaces,renderer_slideshow_surfaces,renderer_video_surfaces,renderer_video_shared_runtimes,renderer_video_pipelines,renderer_video_qos_messages,renderer_video_qos_dropped_max,renderer_video_gtk_frame_clock_ticks,renderer_video_gtk_frame_clock_interval_us_max,renderer_video_gtk_frame_clock_fps_x1000_max,renderer_video_gtk_frame_timings_complete,renderer_video_gtk_frame_timings_presentation_interval_us_max,renderer_video_gtk_frame_timings_presentation_time_us_max,renderer_video_gtk_frame_clock_before_paint_ticks,renderer_video_gtk_frame_clock_update_ticks,renderer_video_gtk_frame_clock_layout_ticks,renderer_video_gtk_frame_clock_paint_ticks,renderer_video_gtk_frame_clock_after_paint_ticks,render_sync_planned_static_image_resource_bytes,render_sync_planned_video_poster_resource_bytes,render_sync_planned_slideshow_image_resource_bytes,render_sync_planned_image_resource_reference_bytes,render_sync_planned_unique_image_resource_bytes,render_sync_package_cache_retained_resource_references,render_sync_package_cache_retained_unique_resources,render_sync_package_cache_retained_resource_bytes,render_sync_package_cache_retained_unique_resource_bytes,renderer_static_surface_resource_references,renderer_static_surface_resource_bytes,renderer_slideshow_resource_references,renderer_slideshow_resource_bytes,renderer_static_surface_unique_resources,renderer_static_surface_unique_resource_bytes,renderer_static_surface_estimated_decoded_bytes,renderer_slideshow_unique_resources,renderer_slideshow_unique_resource_bytes,render_sync_static_image_cache_entries,render_sync_static_image_cache_max_entries,render_sync_static_image_cache_generations,render_sync_static_image_cache_reuses,render_sync_static_image_cache_generation_errors,render_sync_static_image_cache_evictions,render_sync_static_image_cache_eviction_errors,render_sync_planned_video_source_references,render_sync_planned_unique_video_sources,render_sync_planned_duplicate_video_source_references,render_sync_planned_max_video_source_outputs,render_sync_planned_video_source_reference_bytes,render_sync_planned_unique_video_source_bytes,renderer_video_pipeline_source_references,renderer_video_pipeline_source_reference_bytes,renderer_video_pipeline_unique_sources,renderer_video_pipeline_unique_source_bytes,render_sync_package_cache_max_retained_unique_resource_bytes,render_sync_static_image_cache_bytes,render_sync_static_image_cache_max_bytes,render_sync_package_cache_retained_preview_resource_references,render_sync_package_cache_retained_unique_preview_resources,render_sync_package_cache_retained_preview_resource_bytes,render_sync_package_cache_retained_unique_preview_resource_bytes\n",
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
        telemetry.renderer.video_gtk_frame_clock_ticks.to_string(),
        telemetry
            .renderer
            .video_gtk_frame_clock_interval_us_max
            .map(|value| value.to_string())
            .unwrap_or_default(),
        telemetry
            .renderer
            .video_gtk_frame_clock_fps_x1000_max
            .map(|value| value.to_string())
            .unwrap_or_default(),
        telemetry
            .renderer
            .video_gtk_frame_timings_complete
            .to_string(),
        telemetry
            .renderer
            .video_gtk_frame_timings_presentation_interval_us_max
            .map(|value| value.to_string())
            .unwrap_or_default(),
        telemetry
            .renderer
            .video_gtk_frame_timings_presentation_time_us_max
            .map(|value| value.to_string())
            .unwrap_or_default(),
        telemetry
            .renderer
            .video_gtk_frame_clock_before_paint_ticks
            .to_string(),
        telemetry
            .renderer
            .video_gtk_frame_clock_update_ticks
            .to_string(),
        telemetry
            .renderer
            .video_gtk_frame_clock_layout_ticks
            .to_string(),
        telemetry
            .renderer
            .video_gtk_frame_clock_paint_ticks
            .to_string(),
        telemetry
            .renderer
            .video_gtk_frame_clock_after_paint_ticks
            .to_string(),
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

fn render_video_runtime_csv(response: &str) -> Result<String, String> {
    let response: StatusResponse =
        serde_json::from_str(response).map_err(|err| format!("failed to parse response: {err}"))?;
    if let Some(error) = response.error {
        return Err(format!("daemon returned error: {error}"));
    }
    let result = response
        .result
        .ok_or_else(|| "status response did not contain result".to_owned())?;

    let mut csv = String::from(
        "output_name,mode,gst_state,decoder_policy,decoder_policy_status,actual_decoders,decoder_classes,caps_report_count,memory_features,sink_memory_features,zero_copy_evidence_level,zero_copy_evidence_notes,memory_path_level,memory_path_notes,memory_path_segments,allocation_report_count,allocation_pools,allocation_allocators,media_types,caps_paths,position_ms,duration_ms,frame_limiter_enabled,frame_limiter_max_fps,qos_messages,qos_processed_max,qos_dropped_max,qos_stats_format,qos_jitter_ns_latest,qos_jitter_ns_abs_max,qos_proportion_x1000_latest,gtk_frame_clock_ticks,gtk_frame_clock_counter_latest,gtk_frame_clock_time_us_latest,gtk_frame_clock_interval_us_latest,gtk_frame_clock_interval_us_max,gtk_frame_clock_fps_x1000_latest,gtk_frame_clock_refresh_interval_us_latest,gtk_frame_clock_predicted_presentation_time_us_latest,gtk_frame_timings_observed,gtk_frame_timings_complete,gtk_frame_timings_counter_latest,gtk_frame_timings_complete_counter_latest,gtk_frame_timings_frame_time_us_latest,gtk_frame_timings_predicted_presentation_time_us_latest,gtk_frame_timings_presentation_time_us_latest,gtk_frame_timings_presentation_interval_us_latest,gtk_frame_timings_presentation_interval_us_max,gtk_frame_timings_refresh_interval_us_latest,source,gtk_frame_clock_before_paint_ticks,gtk_frame_clock_update_ticks,gtk_frame_clock_layout_ticks,gtk_frame_clock_paint_ticks,gtk_frame_clock_after_paint_ticks,sink_element,sink_async_enabled,sink_last_sample_enabled,sink_qos_enabled,sink_max_lateness_ns,sink_render_delay_ns,sink_processing_deadline_ns,sink_preroll_frame_enabled,memory_retention_level,memory_retention_notes,memory_retention_estimated_min_pool_bytes,memory_retention_estimated_max_pool_bytes,memory_retention_pool_reports,memory_retention_system_memory_pool_reports,memory_retention_gpu_memory_pool_reports,memory_retention_dmabuf_pool_reports,memory_retention_other_memory_pool_reports,memory_retention_sink_frame_retention\n",
    );
    for pipeline in &result.renderer_runtime.video_pipelines {
        let actual_decoders = if pipeline.actual_decoders.is_empty() {
            pipeline
                .actual_decoder_reports
                .iter()
                .map(|report| report.element.clone())
                .collect()
        } else {
            pipeline.actual_decoders.clone()
        };
        let decoder_classes = pipeline
            .actual_decoder_reports
            .iter()
            .map(|report| report.class.clone())
            .collect::<Vec<_>>();
        let memory_features = collect_memory_features(&pipeline.caps_reports, false);
        let sink_memory_features = collect_memory_features(&pipeline.caps_reports, true);
        let media_types = collect_media_types(&pipeline.caps_reports);
        let caps_paths = collect_caps_paths(&pipeline.caps_reports);
        let memory_path_segments = collect_memory_path_segments(&pipeline.memory_path);
        let allocation_pools = collect_allocation_pools(&pipeline.allocation_reports);
        let allocation_allocators = collect_allocation_allocators(&pipeline.allocation_reports);

        let row = [
            csv_cell(&pipeline.output_name),
            csv_cell(&pipeline.mode),
            csv_cell(&pipeline.gst_state),
            csv_cell(&pipeline.decoder_policy),
            csv_cell(&pipeline.decoder_policy_status),
            csv_cell(&pipe_join(actual_decoders)),
            csv_cell(&pipe_join(decoder_classes)),
            pipeline.caps_reports.len().to_string(),
            csv_cell(&pipe_join(memory_features)),
            csv_cell(&pipe_join(sink_memory_features)),
            csv_cell(&pipeline.zero_copy_evidence.level),
            csv_cell(&pipe_join(pipeline.zero_copy_evidence.notes.clone())),
            csv_cell(&pipeline.memory_path.level),
            csv_cell(&pipe_join(pipeline.memory_path.notes.clone())),
            csv_cell(&pipe_join(memory_path_segments)),
            pipeline.allocation_reports.len().to_string(),
            csv_cell(&pipe_join(allocation_pools)),
            csv_cell(&pipe_join(allocation_allocators)),
            csv_cell(&pipe_join(media_types)),
            csv_cell(&pipe_join(caps_paths)),
            pipeline
                .position_ms
                .map(|value| value.to_string())
                .unwrap_or_default(),
            pipeline
                .duration_ms
                .map(|value| value.to_string())
                .unwrap_or_default(),
            bool_csv(pipeline.frame_limiter_enabled),
            pipeline
                .frame_limiter_max_fps
                .map(|value| value.to_string())
                .unwrap_or_default(),
            pipeline.frame_stats.qos_messages.to_string(),
            pipeline
                .frame_stats
                .qos_processed_max
                .map(|value| value.to_string())
                .unwrap_or_default(),
            pipeline
                .frame_stats
                .qos_dropped_max
                .map(|value| value.to_string())
                .unwrap_or_default(),
            csv_cell(
                pipeline
                    .frame_stats
                    .qos_stats_format
                    .as_deref()
                    .unwrap_or_default(),
            ),
            pipeline
                .frame_stats
                .qos_jitter_ns_latest
                .map(|value| value.to_string())
                .unwrap_or_default(),
            pipeline
                .frame_stats
                .qos_jitter_ns_abs_max
                .map(|value| value.to_string())
                .unwrap_or_default(),
            pipeline
                .frame_stats
                .qos_proportion_x1000_latest
                .map(|value| value.to_string())
                .unwrap_or_default(),
            pipeline.frame_stats.gtk_frame_clock_ticks.to_string(),
            pipeline
                .frame_stats
                .gtk_frame_clock_counter_latest
                .map(|value| value.to_string())
                .unwrap_or_default(),
            pipeline
                .frame_stats
                .gtk_frame_clock_time_us_latest
                .map(|value| value.to_string())
                .unwrap_or_default(),
            pipeline
                .frame_stats
                .gtk_frame_clock_interval_us_latest
                .map(|value| value.to_string())
                .unwrap_or_default(),
            pipeline
                .frame_stats
                .gtk_frame_clock_interval_us_max
                .map(|value| value.to_string())
                .unwrap_or_default(),
            pipeline
                .frame_stats
                .gtk_frame_clock_fps_x1000_latest
                .map(|value| value.to_string())
                .unwrap_or_default(),
            pipeline
                .frame_stats
                .gtk_frame_clock_refresh_interval_us_latest
                .map(|value| value.to_string())
                .unwrap_or_default(),
            pipeline
                .frame_stats
                .gtk_frame_clock_predicted_presentation_time_us_latest
                .map(|value| value.to_string())
                .unwrap_or_default(),
            pipeline.frame_stats.gtk_frame_timings_observed.to_string(),
            pipeline.frame_stats.gtk_frame_timings_complete.to_string(),
            pipeline
                .frame_stats
                .gtk_frame_timings_counter_latest
                .map(|value| value.to_string())
                .unwrap_or_default(),
            pipeline
                .frame_stats
                .gtk_frame_timings_complete_counter_latest
                .map(|value| value.to_string())
                .unwrap_or_default(),
            pipeline
                .frame_stats
                .gtk_frame_timings_frame_time_us_latest
                .map(|value| value.to_string())
                .unwrap_or_default(),
            pipeline
                .frame_stats
                .gtk_frame_timings_predicted_presentation_time_us_latest
                .map(|value| value.to_string())
                .unwrap_or_default(),
            pipeline
                .frame_stats
                .gtk_frame_timings_presentation_time_us_latest
                .map(|value| value.to_string())
                .unwrap_or_default(),
            pipeline
                .frame_stats
                .gtk_frame_timings_presentation_interval_us_latest
                .map(|value| value.to_string())
                .unwrap_or_default(),
            pipeline
                .frame_stats
                .gtk_frame_timings_presentation_interval_us_max
                .map(|value| value.to_string())
                .unwrap_or_default(),
            pipeline
                .frame_stats
                .gtk_frame_timings_refresh_interval_us_latest
                .map(|value| value.to_string())
                .unwrap_or_default(),
            csv_cell(&pipeline.source),
            pipeline
                .frame_stats
                .gtk_frame_clock_before_paint_ticks
                .to_string(),
            pipeline
                .frame_stats
                .gtk_frame_clock_update_ticks
                .to_string(),
            pipeline
                .frame_stats
                .gtk_frame_clock_layout_ticks
                .to_string(),
            pipeline.frame_stats.gtk_frame_clock_paint_ticks.to_string(),
            pipeline
                .frame_stats
                .gtk_frame_clock_after_paint_ticks
                .to_string(),
            csv_cell(
                pipeline
                    .sink_tuning
                    .sink_element
                    .as_deref()
                    .unwrap_or_default(),
            ),
            optional_bool_csv(pipeline.sink_tuning.async_enabled),
            optional_bool_csv(pipeline.sink_tuning.last_sample_enabled),
            optional_bool_csv(pipeline.sink_tuning.qos_enabled),
            pipeline
                .sink_tuning
                .max_lateness_ns
                .map(|value| value.to_string())
                .unwrap_or_default(),
            pipeline
                .sink_tuning
                .render_delay_ns
                .map(|value| value.to_string())
                .unwrap_or_default(),
            pipeline
                .sink_tuning
                .processing_deadline_ns
                .map(|value| value.to_string())
                .unwrap_or_default(),
            optional_bool_csv(pipeline.sink_tuning.preroll_frame_enabled),
            csv_cell(&pipeline.retention_report.level),
            csv_cell(&pipe_join(pipeline.retention_report.notes.clone())),
            pipeline
                .retention_report
                .estimated_min_pool_bytes
                .to_string(),
            pipeline
                .retention_report
                .estimated_max_pool_bytes
                .map(|value| value.to_string())
                .unwrap_or_default(),
            pipeline.retention_report.pool_reports.to_string(),
            pipeline
                .retention_report
                .system_memory_pool_reports
                .to_string(),
            pipeline
                .retention_report
                .gpu_memory_pool_reports
                .to_string(),
            pipeline.retention_report.dmabuf_pool_reports.to_string(),
            pipeline
                .retention_report
                .other_memory_pool_reports
                .to_string(),
            csv_cell(&pipeline.retention_report.sink_frame_retention),
        ];
        csv.push_str(&row.join(","));
        csv.push('\n');
    }
    Ok(csv)
}

fn collect_memory_features(caps_reports: &[VideoCapsReport], sink_only: bool) -> Vec<String> {
    caps_reports
        .iter()
        .filter(|report| !sink_only || report.direction == "sink")
        .flat_map(|report| report.memory_features.iter().cloned())
        .collect()
}

fn collect_media_types(caps_reports: &[VideoCapsReport]) -> Vec<String> {
    caps_reports
        .iter()
        .flat_map(|report| report.structures.iter())
        .map(|structure| structure.media_type.clone())
        .collect()
}

fn collect_caps_paths(caps_reports: &[VideoCapsReport]) -> Vec<String> {
    caps_reports
        .iter()
        .map(|report| format!("{}:{}:{}", report.element, report.pad, report.direction))
        .collect()
}

fn collect_memory_path_segments(path: &VideoMemoryPathReport) -> Vec<String> {
    path.segments
        .iter()
        .map(|segment| {
            let mut value = format!(
                "{}:{}:{}:{}:{}",
                segment.element,
                segment.pad,
                segment.direction,
                segment.media_type,
                segment.memory_class
            );
            if !segment.memory_features.is_empty() {
                value.push(':');
                value.push_str(&segment.memory_features.join("+"));
            }
            value
        })
        .collect()
}

fn collect_allocation_pools(reports: &[VideoAllocationReport]) -> Vec<String> {
    reports
        .iter()
        .flat_map(|report| {
            report.pools.iter().map(|pool| {
                format!(
                    "{}:{}:{}:{}:{}:{}:{}",
                    report.element,
                    report.pad,
                    report.query_scope,
                    pool.pool,
                    pool.size,
                    pool.min_buffers,
                    pool.max_buffers
                )
            })
        })
        .collect()
}

fn collect_allocation_allocators(reports: &[VideoAllocationReport]) -> Vec<String> {
    reports
        .iter()
        .flat_map(|report| {
            report.params.iter().map(|param| {
                format!(
                    "{}:{}:{}:{}:{}:{}:{}:{}",
                    report.element,
                    report.pad,
                    report.query_scope,
                    param.allocator,
                    param.flags,
                    param.align,
                    param.prefix,
                    param.padding
                )
            })
        })
        .collect()
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
    for plan in &sync.scene_lite_plans {
        details.insert(
            plan.output_name.as_str(),
            PlanCsvDetails {
                kind: "scene-lite",
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

fn optional_bool_csv(value: Option<bool>) -> String {
    value.map(bool_csv).unwrap_or_default()
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
    #[serde(default)]
    renderer_runtime: RendererRuntime,
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
    scene_lite_plans: Vec<SceneLitePlan>,
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
struct SceneLitePlan {
    output_name: String,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    target_max_fps: Option<u32>,
    #[serde(default)]
    display: Option<SceneLiteDisplay>,
}

impl SceneLitePlan {
    fn csv_source(&self) -> &str {
        match &self.display {
            Some(SceneLiteDisplay::Image { source, .. }) => source.as_str(),
            Some(SceneLiteDisplay::Color { color }) => color.as_str(),
            None => self.source.as_deref().unwrap_or_default(),
        }
    }

    fn csv_fit(&self) -> &str {
        match &self.display {
            Some(SceneLiteDisplay::Image { fit, .. }) => fit.as_str(),
            Some(SceneLiteDisplay::Color { .. }) | None => "",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum SceneLiteDisplay {
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
    #[serde(default)]
    video_gtk_frame_clock_ticks: u64,
    #[serde(default)]
    video_gtk_frame_clock_before_paint_ticks: u64,
    #[serde(default)]
    video_gtk_frame_clock_update_ticks: u64,
    #[serde(default)]
    video_gtk_frame_clock_layout_ticks: u64,
    #[serde(default)]
    video_gtk_frame_clock_paint_ticks: u64,
    #[serde(default)]
    video_gtk_frame_clock_after_paint_ticks: u64,
    #[serde(default)]
    video_gtk_frame_clock_interval_us_max: Option<u64>,
    #[serde(default)]
    video_gtk_frame_clock_fps_x1000_max: Option<u64>,
    #[serde(default)]
    video_gtk_frame_timings_complete: u64,
    #[serde(default)]
    video_gtk_frame_timings_presentation_interval_us_max: Option<u64>,
    #[serde(default)]
    video_gtk_frame_timings_presentation_time_us_max: Option<u64>,
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

#[derive(Debug, Default, Deserialize)]
struct RendererRuntime {
    #[serde(default)]
    video_pipelines: Vec<VideoRuntimePipeline>,
}

#[derive(Debug, Default, Deserialize)]
struct VideoRuntimePipeline {
    #[serde(default)]
    output_name: String,
    #[serde(default)]
    source: String,
    #[serde(default)]
    mode: String,
    #[serde(default)]
    gst_state: String,
    #[serde(default)]
    decoder_policy: String,
    #[serde(default)]
    decoder_policy_status: String,
    #[serde(default)]
    actual_decoders: Vec<String>,
    #[serde(default)]
    actual_decoder_reports: Vec<VideoDecoderReport>,
    #[serde(default)]
    caps_reports: Vec<VideoCapsReport>,
    #[serde(default)]
    allocation_reports: Vec<VideoAllocationReport>,
    #[serde(default)]
    zero_copy_evidence: VideoZeroCopyEvidence,
    #[serde(default)]
    memory_path: VideoMemoryPathReport,
    #[serde(default)]
    retention_report: VideoMemoryRetentionReport,
    #[serde(default)]
    position_ms: Option<u64>,
    #[serde(default)]
    duration_ms: Option<u64>,
    #[serde(default)]
    frame_limiter_enabled: bool,
    #[serde(default)]
    frame_limiter_max_fps: Option<u32>,
    #[serde(default)]
    sink_tuning: VideoSinkTuningReport,
    #[serde(default)]
    frame_stats: VideoFrameStats,
}

#[derive(Debug, Default, Deserialize)]
struct VideoSinkTuningReport {
    #[serde(default)]
    sink_element: Option<String>,
    #[serde(default)]
    async_enabled: Option<bool>,
    #[serde(default)]
    last_sample_enabled: Option<bool>,
    #[serde(default)]
    qos_enabled: Option<bool>,
    #[serde(default)]
    max_lateness_ns: Option<i64>,
    #[serde(default)]
    render_delay_ns: Option<u64>,
    #[serde(default)]
    processing_deadline_ns: Option<u64>,
    #[serde(default)]
    preroll_frame_enabled: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
struct VideoFrameStats {
    #[serde(default)]
    qos_messages: u64,
    #[serde(default)]
    qos_stats_format: Option<String>,
    #[serde(default)]
    qos_processed_max: Option<u64>,
    #[serde(default)]
    qos_dropped_max: Option<u64>,
    #[serde(default)]
    qos_jitter_ns_latest: Option<i64>,
    #[serde(default)]
    qos_jitter_ns_abs_max: Option<u64>,
    #[serde(default)]
    qos_proportion_x1000_latest: Option<u32>,
    #[serde(default)]
    gtk_frame_clock_ticks: u64,
    #[serde(default)]
    gtk_frame_clock_before_paint_ticks: u64,
    #[serde(default)]
    gtk_frame_clock_update_ticks: u64,
    #[serde(default)]
    gtk_frame_clock_layout_ticks: u64,
    #[serde(default)]
    gtk_frame_clock_paint_ticks: u64,
    #[serde(default)]
    gtk_frame_clock_after_paint_ticks: u64,
    #[serde(default)]
    gtk_frame_clock_counter_latest: Option<u64>,
    #[serde(default)]
    gtk_frame_clock_time_us_latest: Option<u64>,
    #[serde(default)]
    gtk_frame_clock_interval_us_latest: Option<u64>,
    #[serde(default)]
    gtk_frame_clock_interval_us_max: Option<u64>,
    #[serde(default)]
    gtk_frame_clock_fps_x1000_latest: Option<u32>,
    #[serde(default)]
    gtk_frame_clock_refresh_interval_us_latest: Option<u64>,
    #[serde(default)]
    gtk_frame_clock_predicted_presentation_time_us_latest: Option<u64>,
    #[serde(default)]
    gtk_frame_timings_observed: u64,
    #[serde(default)]
    gtk_frame_timings_complete: u64,
    #[serde(default)]
    gtk_frame_timings_counter_latest: Option<u64>,
    #[serde(default)]
    gtk_frame_timings_complete_counter_latest: Option<u64>,
    #[serde(default)]
    gtk_frame_timings_frame_time_us_latest: Option<u64>,
    #[serde(default)]
    gtk_frame_timings_predicted_presentation_time_us_latest: Option<u64>,
    #[serde(default)]
    gtk_frame_timings_presentation_time_us_latest: Option<u64>,
    #[serde(default)]
    gtk_frame_timings_presentation_interval_us_latest: Option<u64>,
    #[serde(default)]
    gtk_frame_timings_presentation_interval_us_max: Option<u64>,
    #[serde(default)]
    gtk_frame_timings_refresh_interval_us_latest: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
struct VideoDecoderReport {
    #[serde(default)]
    element: String,
    #[serde(default)]
    class: String,
}

#[derive(Debug, Default, Deserialize)]
struct VideoCapsReport {
    #[serde(default)]
    element: String,
    #[serde(default)]
    pad: String,
    #[serde(default)]
    direction: String,
    #[serde(default)]
    memory_features: Vec<String>,
    #[serde(default)]
    structures: Vec<VideoCapsStructureReport>,
}

#[derive(Debug, Default, Deserialize)]
struct VideoCapsStructureReport {
    #[serde(default)]
    media_type: String,
}

#[derive(Debug, Default, Deserialize)]
struct VideoZeroCopyEvidence {
    #[serde(default)]
    level: String,
    #[serde(default)]
    notes: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct VideoAllocationReport {
    #[serde(default)]
    element: String,
    #[serde(default)]
    pad: String,
    #[serde(default)]
    query_scope: String,
    #[serde(default)]
    pools: Vec<VideoAllocationPoolReport>,
    #[serde(default)]
    params: Vec<VideoAllocationParamReport>,
}

#[derive(Debug, Default, Deserialize)]
struct VideoAllocationPoolReport {
    #[serde(default)]
    pool: String,
    #[serde(default)]
    size: u32,
    #[serde(default)]
    min_buffers: u32,
    #[serde(default)]
    max_buffers: u32,
}

#[derive(Debug, Default, Deserialize)]
struct VideoAllocationParamReport {
    #[serde(default)]
    allocator: String,
    #[serde(default)]
    flags: String,
    #[serde(default)]
    align: u64,
    #[serde(default)]
    prefix: u64,
    #[serde(default)]
    padding: u64,
}

#[derive(Debug, Default, Deserialize)]
struct VideoMemoryPathReport {
    #[serde(default)]
    level: String,
    #[serde(default)]
    notes: Vec<String>,
    #[serde(default)]
    segments: Vec<VideoMemoryPathSegment>,
}

#[derive(Debug, Default, Deserialize)]
struct VideoMemoryPathSegment {
    #[serde(default)]
    element: String,
    #[serde(default)]
    pad: String,
    #[serde(default)]
    direction: String,
    #[serde(default)]
    media_type: String,
    #[serde(default)]
    memory_features: Vec<String>,
    #[serde(default)]
    memory_class: String,
}

#[derive(Debug, Default, Deserialize)]
struct VideoMemoryRetentionReport {
    #[serde(default)]
    level: String,
    #[serde(default)]
    estimated_min_pool_bytes: u64,
    #[serde(default)]
    estimated_max_pool_bytes: Option<u64>,
    #[serde(default)]
    pool_reports: usize,
    #[serde(default)]
    system_memory_pool_reports: usize,
    #[serde(default)]
    gpu_memory_pool_reports: usize,
    #[serde(default)]
    dmabuf_pool_reports: usize,
    #[serde(default)]
    other_memory_pool_reports: usize,
    #[serde(default)]
    sink_frame_retention: String,
    #[serde(default)]
    notes: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_render_decisions_as_csv() {
        let response = r##"{"jsonrpc":"2.0","id":1,"result":{"render_sync":{"plans":[{"output_name":"HDMI-A-1","source":"/tmp/poster.jpg","fit":"contain","background":"#000000"}],"video_plans":[{"output_name":"eDP-1","source":"/tmp/loop.webm","poster":"/tmp/poster.jpg","fit":"cover","loop_playback":true,"muted":true,"manifest_max_fps":60,"target_max_fps":24,"start_offset_ms":0}],"slideshow_plans":[{"output_name":"DP-1","sources":["/tmp/a.jpg","/tmp/b.jpg"],"interval_ms":300000,"transition":"none","fit":"cover","target_max_fps":12}],"scene_lite_plans":[{"output_name":"DP-2","source":"/tmp/scene-lite.json","target_max_fps":30,"display":{"type":"image","source":"/tmp/scene-poster.jpg","fit":"cover","background":"#000000"}}],"decisions":[{"output_name":"eDP-1","action":"render","performance":{"mode":"throttled","max_fps":24,"reason":"battery"},"wallpaper":"/tmp/wall.gwpdir"},{"output_name":"HDMI-A-1","action":"remove","performance":{"mode":"paused","max_fps":null,"reason":"fullscreen"},"wallpaper":null},{"output_name":"DP-1","action":"render","performance":{"mode":"throttled","max_fps":12,"reason":"unfocused"},"wallpaper":"/tmp/slides.gwpdir"},{"output_name":"DP-2","action":"render","performance":{"mode":"throttled","max_fps":30,"reason":"adaptive"},"wallpaper":"/tmp/scene.gwpdir"}]}}}"##;

        let csv = render_decisions_csv(response).unwrap();

        assert_eq!(
            csv,
            "output_name,action,mode,reason,max_fps,wallpaper,plan_kind,source,fit,target_max_fps,muted\n\
             eDP-1,render,throttled,battery,24,/tmp/wall.gwpdir,video,/tmp/loop.webm,cover,24,true\n\
             HDMI-A-1,remove,paused,fullscreen,,,static-image,/tmp/poster.jpg,contain,,\n\
             DP-1,render,throttled,unfocused,12,/tmp/slides.gwpdir,slideshow,/tmp/a.jpg,cover,12,\n\
             DP-2,render,throttled,adaptive,30,/tmp/scene.gwpdir,scene-lite,/tmp/scene-poster.jpg,cover,30,\n"
        );
    }

    #[test]
    fn escapes_csv_cells() {
        let response = r##"{"jsonrpc":"2.0","id":1,"result":{"render_sync":{"plans":[{"output_name":"DP,1","source":"/tmp/a,b.png","fit":"cover","background":null}],"decisions":[{"output_name":"DP,1","action":"render","performance":{"mode":"active","max_fps":60,"reason":"interactive"},"wallpaper":"/tmp/a\"b.gwpdir"}]}}}"##;

        let csv = render_decisions_csv(response).unwrap();

        assert_eq!(
            csv,
            "output_name,action,mode,reason,max_fps,wallpaper,plan_kind,source,fit,target_max_fps,muted\n\
             \"DP,1\",render,active,interactive,60,\"/tmp/a\"\"b.gwpdir\",static-image,\"/tmp/a,b.png\",cover,,\n"
        );
    }

    #[test]
    fn formats_daemon_telemetry_as_csv() {
        let response = r##"{"jsonrpc":"2.0","id":1,"result":{"render_sync":{"plans":[],"video_plans":[],"decisions":[]},"telemetry":{"desktop":{"refreshes":7,"refresh_skips":11,"changes":2,"last_refresh_age_ms":42},"render_sync":{"cache_hits":23,"cache_misses":5,"updates_queued":3,"updates_skipped":2,"package_cache_entries":2,"package_cache_max_entries":5,"package_cache_max_retained_unique_resource_bytes":1048576,"package_cache_hits":4,"package_cache_misses":3,"package_cache_evictions":1,"archive_cache_entries":8,"archive_cache_max_entries":32,"archive_cache_reuses":6,"archive_cache_extractions":1,"archive_cache_evictions":9,"archive_cache_evictions_latest":2,"archive_cache_eviction_errors":1,"archive_cache_eviction_errors_latest":1,"static_image_cache_entries":2,"static_image_cache_max_entries":32,"static_image_cache_bytes":5120,"static_image_cache_max_bytes":1048576,"static_image_cache_generations":1,"static_image_cache_reuses":4,"static_image_cache_generation_errors":0,"static_image_cache_evictions":3,"static_image_cache_eviction_errors":0,"planned_video_source_references":3,"planned_unique_video_sources":2,"planned_duplicate_video_source_references":1,"planned_max_video_source_outputs":2,"planned_video_source_reference_bytes":9000,"planned_unique_video_source_bytes":6000,"planned_static_image_resources":2,"planned_video_poster_resources":1,"planned_slideshow_image_resources":3,"planned_image_resource_references":6,"planned_unique_image_resources":5,"planned_static_image_resource_bytes":2048,"planned_video_poster_resource_bytes":512,"planned_slideshow_image_resource_bytes":4096,"planned_image_resource_reference_bytes":6656,"planned_unique_image_resource_bytes":6400,"package_cache_retained_resource_references":9,"package_cache_retained_unique_resources":7,"package_cache_retained_resource_bytes":12345,"package_cache_retained_unique_resource_bytes":12000,"package_cache_retained_preview_resource_references":4,"package_cache_retained_unique_preview_resources":3,"package_cache_retained_preview_resource_bytes":7000,"package_cache_retained_unique_preview_resource_bytes":6500},"adaptive":{"refreshes":5,"refresh_skips":6,"snapshot":{"sample":{"cpu_pressure_some_avg10_x100":123,"memory_pressure_some_avg10_x100":45,"temperature_max_millicelsius":73500,"power_external_online":true,"power_system_battery_present":true,"power_battery_discharging":false,"power_battery_capacity_percent":88,"power_battery_power_microwatts":12000000,"gpu_busy_percent_avg":37,"gpu_busy_percent_max":72,"gpu_busy_sources":["renderD128","card0"]},"active_triggers":[{"metric":"temperature-max-celsius","value_x100":7350,"threshold_x100":7000}]},"action":[{"output_name":"eDP-1","type":"throttle","configured_action":"pause-unfocused","max_fps":15},{"output_name":"HDMI-A-1","type":"pause-dynamic","scope":"dynamic-wallpapers"}]},"renderer":{"output_windows":3,"static_surfaces":2,"static_picture_surfaces":1,"static_css_surfaces":1,"static_color_surfaces":0,"slideshow_surfaces":1,"video_surfaces":2,"video_shared_runtimes":1,"static_surface_resource_references":2,"static_surface_resource_bytes":2048,"static_surface_unique_resources":1,"static_surface_unique_resource_bytes":1024,"static_surface_estimated_decoded_bytes":8294400,"slideshow_resource_references":4,"slideshow_resource_bytes":8192,"slideshow_unique_resources":3,"slideshow_unique_resource_bytes":6144,"video_pipeline_source_references":3,"video_pipeline_source_reference_bytes":18000,"video_pipeline_unique_sources":2,"video_pipeline_unique_source_bytes":12000,"video_pipelines":2,"video_qos_messages":7,"video_qos_dropped_max":3,"video_gtk_frame_clock_ticks":40,"video_gtk_frame_clock_interval_us_max":22000,"video_gtk_frame_clock_fps_x1000_max":60000,"video_gtk_frame_timings_complete":12,"video_gtk_frame_timings_presentation_interval_us_max":21000,"video_gtk_frame_timings_presentation_time_us_max":150000,"video_gtk_frame_clock_before_paint_ticks":36,"video_gtk_frame_clock_update_ticks":34,"video_gtk_frame_clock_layout_ticks":33,"video_gtk_frame_clock_paint_ticks":32,"video_gtk_frame_clock_after_paint_ticks":40}}}}"##;

        let csv = render_telemetry_csv(response).unwrap();

        assert_eq!(
            csv,
            "desktop_refreshes,desktop_refresh_skips,desktop_changes,last_desktop_refresh_age_ms,render_sync_cache_hits,render_sync_cache_misses,render_sync_updates_queued,render_sync_updates_skipped,render_sync_package_cache_entries,render_sync_package_cache_max_entries,render_sync_package_cache_hits,render_sync_package_cache_misses,render_sync_package_cache_evictions,render_sync_archive_cache_entries,render_sync_archive_cache_max_entries,render_sync_archive_cache_reuses,render_sync_archive_cache_extractions,render_sync_archive_cache_evictions,render_sync_archive_cache_evictions_latest,render_sync_archive_cache_eviction_errors,render_sync_archive_cache_eviction_errors_latest,render_sync_planned_static_image_resources,render_sync_planned_video_poster_resources,render_sync_planned_slideshow_image_resources,render_sync_planned_image_resource_references,render_sync_planned_unique_image_resources,adaptive_refreshes,adaptive_refresh_skips,adaptive_active_triggers,cpu_pressure_some_avg10_x100,memory_pressure_some_avg10_x100,temperature_max_millicelsius,power_external_online,power_system_battery_present,power_battery_discharging,power_battery_capacity_percent,power_battery_power_microwatts,gpu_busy_percent_avg,gpu_busy_percent_max,gpu_busy_sources,adaptive_action_types,adaptive_action_scopes,adaptive_action_configured_actions,adaptive_action_max_fps,renderer_output_windows,renderer_static_surfaces,renderer_static_picture_surfaces,renderer_static_css_surfaces,renderer_static_color_surfaces,renderer_slideshow_surfaces,renderer_video_surfaces,renderer_video_shared_runtimes,renderer_video_pipelines,renderer_video_qos_messages,renderer_video_qos_dropped_max,renderer_video_gtk_frame_clock_ticks,renderer_video_gtk_frame_clock_interval_us_max,renderer_video_gtk_frame_clock_fps_x1000_max,renderer_video_gtk_frame_timings_complete,renderer_video_gtk_frame_timings_presentation_interval_us_max,renderer_video_gtk_frame_timings_presentation_time_us_max,renderer_video_gtk_frame_clock_before_paint_ticks,renderer_video_gtk_frame_clock_update_ticks,renderer_video_gtk_frame_clock_layout_ticks,renderer_video_gtk_frame_clock_paint_ticks,renderer_video_gtk_frame_clock_after_paint_ticks,render_sync_planned_static_image_resource_bytes,render_sync_planned_video_poster_resource_bytes,render_sync_planned_slideshow_image_resource_bytes,render_sync_planned_image_resource_reference_bytes,render_sync_planned_unique_image_resource_bytes,render_sync_package_cache_retained_resource_references,render_sync_package_cache_retained_unique_resources,render_sync_package_cache_retained_resource_bytes,render_sync_package_cache_retained_unique_resource_bytes,renderer_static_surface_resource_references,renderer_static_surface_resource_bytes,renderer_slideshow_resource_references,renderer_slideshow_resource_bytes,renderer_static_surface_unique_resources,renderer_static_surface_unique_resource_bytes,renderer_static_surface_estimated_decoded_bytes,renderer_slideshow_unique_resources,renderer_slideshow_unique_resource_bytes,render_sync_static_image_cache_entries,render_sync_static_image_cache_max_entries,render_sync_static_image_cache_generations,render_sync_static_image_cache_reuses,render_sync_static_image_cache_generation_errors,render_sync_static_image_cache_evictions,render_sync_static_image_cache_eviction_errors,render_sync_planned_video_source_references,render_sync_planned_unique_video_sources,render_sync_planned_duplicate_video_source_references,render_sync_planned_max_video_source_outputs,render_sync_planned_video_source_reference_bytes,render_sync_planned_unique_video_source_bytes,renderer_video_pipeline_source_references,renderer_video_pipeline_source_reference_bytes,renderer_video_pipeline_unique_sources,renderer_video_pipeline_unique_source_bytes,render_sync_package_cache_max_retained_unique_resource_bytes,render_sync_static_image_cache_bytes,render_sync_static_image_cache_max_bytes,render_sync_package_cache_retained_preview_resource_references,render_sync_package_cache_retained_unique_preview_resources,render_sync_package_cache_retained_preview_resource_bytes,render_sync_package_cache_retained_unique_preview_resource_bytes\n\
             7,11,2,42,23,5,3,2,2,5,4,3,1,8,32,6,1,9,2,1,1,2,1,3,6,5,5,6,1,123,45,73500,true,true,false,88,12000000,37,72,card0|renderD128,pause-dynamic|throttle,dynamic-wallpapers,pause-unfocused,15,3,2,1,1,0,1,2,1,2,7,3,40,22000,60000,12,21000,150000,36,34,33,32,40,2048,512,4096,6656,6400,9,7,12345,12000,2,2048,4,8192,1,1024,8294400,3,6144,2,32,1,4,0,3,0,3,2,1,2,9000,6000,3,18000,2,12000,1048576,5120,1048576,4,3,7000,6500\n"
        );
    }

    #[test]
    fn formats_video_runtime_as_csv() {
        let response = r##"{"jsonrpc":"2.0","id":1,"result":{"render_sync":{"plans":[],"video_plans":[],"decisions":[]},"renderer_runtime":{"video_pipelines":[{"output_name":"eDP-1","source":"/tmp/loop.mp4","mode":"active","gst_state":"Playing","loop_playback":true,"muted":true,"target_max_fps":24,"frame_limiter_enabled":true,"frame_limiter_max_fps":24,"sink_tuning":{"sink_element":"glsinkbin+gtk4paintablesink","async_enabled":false,"last_sample_enabled":false,"qos_enabled":true,"max_lateness_ns":41666666,"render_delay_ns":0,"processing_deadline_ns":0,"preroll_frame_enabled":false},"position_ms":1234,"duration_ms":60000,"frame_stats":{"qos_messages":3,"qos_stats_format":"buffers","qos_processed_max":120,"qos_dropped_max":2,"qos_jitter_ns_latest":-2000,"qos_jitter_ns_abs_max":7000,"qos_proportion_x1000_latest":995,"gtk_frame_clock_ticks":9,"gtk_frame_clock_before_paint_ticks":8,"gtk_frame_clock_update_ticks":7,"gtk_frame_clock_layout_ticks":6,"gtk_frame_clock_paint_ticks":5,"gtk_frame_clock_after_paint_ticks":9,"gtk_frame_clock_counter_latest":300,"gtk_frame_clock_time_us_latest":5000000,"gtk_frame_clock_interval_us_latest":16667,"gtk_frame_clock_interval_us_max":20000,"gtk_frame_clock_fps_x1000_latest":59940,"gtk_frame_clock_refresh_interval_us_latest":16667,"gtk_frame_clock_predicted_presentation_time_us_latest":5016667,"gtk_frame_timings_observed":8,"gtk_frame_timings_complete":7,"gtk_frame_timings_counter_latest":300,"gtk_frame_timings_complete_counter_latest":299,"gtk_frame_timings_frame_time_us_latest":5000000,"gtk_frame_timings_predicted_presentation_time_us_latest":5016667,"gtk_frame_timings_presentation_time_us_latest":5017000,"gtk_frame_timings_presentation_interval_us_latest":16667,"gtk_frame_timings_presentation_interval_us_max":20000,"gtk_frame_timings_refresh_interval_us_latest":16667},"decoder_policy":"hardware-preferred","decoder_policy_status":"software-fallback","actual_decoders":["dav1ddec"],"actual_decoder_reports":[{"element":"dav1ddec","class":"software"}],"caps_reports":[{"element":"gtk4paintablesink0","pad":"sink","direction":"sink","caps":"video/x-raw(memory:DMABuf)","memory_features":["memory:DMABuf"],"structures":[{"media_type":"video/x-raw","features":["memory:DMABuf"]}]},{"element":"videoconvert0","pad":"src","direction":"src","caps":"video/x-raw","memory_features":[],"structures":[{"media_type":"video/x-raw","features":[]}]}],"allocation_reports":[{"element":"videoconvert0","pad":"src","direction":"src","query_scope":"peer","caps":"video/x-raw(memory:DMABuf)","need_pool":true,"pools":[{"pool":"GstVideoBufferPool","size":4096,"min_buffers":2,"max_buffers":4}],"params":[{"allocator":"dmabufallocator0","flags":"MemoryFlags(0x0)","align":0,"prefix":0,"padding":0}],"metas":["GstVideoMeta"]}],"zero_copy_evidence":{"level":"sink-dmabuf-caps","notes":["sink-side DMABuf caps observed"]},"memory_path":{"level":"sink-dmabuf","notes":["sink-side DMABuf caps observed"],"segments":[{"element":"gtk4paintablesink0","pad":"sink","direction":"sink","media_type":"video/x-raw","memory_features":["memory:DMABuf"],"memory_class":"dmabuf"}]},"retention_report":{"level":"medium","estimated_min_pool_bytes":8192,"estimated_max_pool_bytes":16384,"pool_reports":1,"system_memory_pool_reports":0,"gpu_memory_pool_reports":0,"dmabuf_pool_reports":1,"other_memory_pool_reports":0,"sink_frame_retention":"disabled","notes":["sink last-sample and preroll frame retention are disabled","allocation pools report at least 8192 bytes of minimum buffer capacity"]}}]}}}"##;

        let csv = render_video_runtime_csv(response).unwrap();

        assert_eq!(
            csv,
            "output_name,mode,gst_state,decoder_policy,decoder_policy_status,actual_decoders,decoder_classes,caps_report_count,memory_features,sink_memory_features,zero_copy_evidence_level,zero_copy_evidence_notes,memory_path_level,memory_path_notes,memory_path_segments,allocation_report_count,allocation_pools,allocation_allocators,media_types,caps_paths,position_ms,duration_ms,frame_limiter_enabled,frame_limiter_max_fps,qos_messages,qos_processed_max,qos_dropped_max,qos_stats_format,qos_jitter_ns_latest,qos_jitter_ns_abs_max,qos_proportion_x1000_latest,gtk_frame_clock_ticks,gtk_frame_clock_counter_latest,gtk_frame_clock_time_us_latest,gtk_frame_clock_interval_us_latest,gtk_frame_clock_interval_us_max,gtk_frame_clock_fps_x1000_latest,gtk_frame_clock_refresh_interval_us_latest,gtk_frame_clock_predicted_presentation_time_us_latest,gtk_frame_timings_observed,gtk_frame_timings_complete,gtk_frame_timings_counter_latest,gtk_frame_timings_complete_counter_latest,gtk_frame_timings_frame_time_us_latest,gtk_frame_timings_predicted_presentation_time_us_latest,gtk_frame_timings_presentation_time_us_latest,gtk_frame_timings_presentation_interval_us_latest,gtk_frame_timings_presentation_interval_us_max,gtk_frame_timings_refresh_interval_us_latest,source,gtk_frame_clock_before_paint_ticks,gtk_frame_clock_update_ticks,gtk_frame_clock_layout_ticks,gtk_frame_clock_paint_ticks,gtk_frame_clock_after_paint_ticks,sink_element,sink_async_enabled,sink_last_sample_enabled,sink_qos_enabled,sink_max_lateness_ns,sink_render_delay_ns,sink_processing_deadline_ns,sink_preroll_frame_enabled,memory_retention_level,memory_retention_notes,memory_retention_estimated_min_pool_bytes,memory_retention_estimated_max_pool_bytes,memory_retention_pool_reports,memory_retention_system_memory_pool_reports,memory_retention_gpu_memory_pool_reports,memory_retention_dmabuf_pool_reports,memory_retention_other_memory_pool_reports,memory_retention_sink_frame_retention\n\
             eDP-1,active,Playing,hardware-preferred,software-fallback,dav1ddec,software,2,memory:DMABuf,memory:DMABuf,sink-dmabuf-caps,sink-side DMABuf caps observed,sink-dmabuf,sink-side DMABuf caps observed,gtk4paintablesink0:sink:sink:video/x-raw:dmabuf:memory:DMABuf,1,videoconvert0:src:peer:GstVideoBufferPool:4096:2:4,videoconvert0:src:peer:dmabufallocator0:MemoryFlags(0x0):0:0:0,video/x-raw,gtk4paintablesink0:sink:sink|videoconvert0:src:src,1234,60000,true,24,3,120,2,buffers,-2000,7000,995,9,300,5000000,16667,20000,59940,16667,5016667,8,7,300,299,5000000,5016667,5017000,16667,20000,16667,/tmp/loop.mp4,8,7,6,5,9,glsinkbin+gtk4paintablesink,false,false,true,41666666,0,0,false,medium,allocation pools report at least 8192 bytes of minimum buffer capacity|sink last-sample and preroll frame retention are disabled,8192,16384,1,0,0,1,0,disabled\n"
        );
    }

    #[test]
    fn parses_status_file_invocation() {
        let args = vec![
            "status".to_owned(),
            "--decisions-csv".to_owned(),
            "--from-file".to_owned(),
            "status.json".to_owned(),
        ];

        assert_eq!(
            parse_invocation(&args).unwrap(),
            Invocation {
                command: gilder::ipc::ClientCommand::Status,
                format: ResponseFormat::DecisionsCsv,
                response_file: Some(PathBuf::from("status.json")),
            }
        );
    }

    #[test]
    fn parses_status_video_runtime_file_invocation() {
        let args = vec![
            "status".to_owned(),
            "--video-runtime-csv".to_owned(),
            "--from-file".to_owned(),
            "status.json".to_owned(),
        ];

        assert_eq!(
            parse_invocation(&args).unwrap(),
            Invocation {
                command: gilder::ipc::ClientCommand::Status,
                format: ResponseFormat::VideoRuntimeCsv,
                response_file: Some(PathBuf::from("status.json")),
            }
        );
    }

    #[test]
    fn parses_status_telemetry_file_invocation() {
        let args = vec![
            "status".to_owned(),
            "--telemetry-csv".to_owned(),
            "--from-file".to_owned(),
            "status.json".to_owned(),
        ];

        assert_eq!(
            parse_invocation(&args).unwrap(),
            Invocation {
                command: gilder::ipc::ClientCommand::Status,
                format: ResponseFormat::TelemetryCsv,
                response_file: Some(PathBuf::from("status.json")),
            }
        );
    }
}
