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
        "desktop_refreshes,desktop_refresh_skips,desktop_changes,last_desktop_refresh_age_ms,render_sync_cache_hits,render_sync_cache_misses,render_sync_updates_queued,render_sync_updates_skipped,render_sync_package_cache_entries,render_sync_package_cache_max_entries,render_sync_package_cache_hits,render_sync_package_cache_misses,render_sync_package_cache_evictions,render_sync_archive_cache_entries,render_sync_archive_cache_max_entries,render_sync_archive_cache_reuses,render_sync_archive_cache_extractions,render_sync_archive_cache_evictions,render_sync_archive_cache_evictions_latest,render_sync_archive_cache_eviction_errors,render_sync_archive_cache_eviction_errors_latest,render_sync_planned_static_image_resources,render_sync_planned_video_poster_resources,render_sync_planned_slideshow_image_resources,render_sync_planned_image_resource_references,render_sync_planned_unique_image_resources,adaptive_refreshes,adaptive_refresh_skips,adaptive_active_triggers,cpu_pressure_some_avg10_x100,memory_pressure_some_avg10_x100,temperature_max_millicelsius,power_external_online,power_system_battery_present,power_battery_discharging,power_battery_capacity_percent,power_battery_power_microwatts,gpu_busy_percent_avg,gpu_busy_percent_max,gpu_busy_sources,adaptive_action_types,adaptive_action_scopes,adaptive_action_configured_actions,adaptive_action_max_fps,renderer_output_windows,renderer_static_surfaces,renderer_slideshow_surfaces,renderer_video_surfaces,renderer_video_pipelines,renderer_video_qos_messages,renderer_video_qos_dropped_max,renderer_video_gtk_frame_clock_ticks,renderer_video_gtk_frame_clock_interval_us_max,renderer_video_gtk_frame_clock_fps_x1000_max,renderer_video_gtk_frame_timings_complete,renderer_video_gtk_frame_timings_presentation_interval_us_max,renderer_video_gtk_frame_timings_presentation_time_us_max,renderer_video_gtk_frame_clock_before_paint_ticks,renderer_video_gtk_frame_clock_update_ticks,renderer_video_gtk_frame_clock_layout_ticks,renderer_video_gtk_frame_clock_paint_ticks,renderer_video_gtk_frame_clock_after_paint_ticks\n",
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
        telemetry.renderer.slideshow_surfaces.to_string(),
        telemetry.renderer.video_surfaces.to_string(),
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
        "output_name,mode,gst_state,decoder_policy,decoder_policy_status,actual_decoders,decoder_classes,caps_report_count,memory_features,sink_memory_features,zero_copy_evidence_level,zero_copy_evidence_notes,media_types,caps_paths,position_ms,duration_ms,frame_limiter_enabled,frame_limiter_max_fps,qos_messages,qos_processed_max,qos_dropped_max,qos_stats_format,qos_jitter_ns_latest,qos_jitter_ns_abs_max,qos_proportion_x1000_latest,gtk_frame_clock_ticks,gtk_frame_clock_counter_latest,gtk_frame_clock_time_us_latest,gtk_frame_clock_interval_us_latest,gtk_frame_clock_interval_us_max,gtk_frame_clock_fps_x1000_latest,gtk_frame_clock_refresh_interval_us_latest,gtk_frame_clock_predicted_presentation_time_us_latest,gtk_frame_timings_observed,gtk_frame_timings_complete,gtk_frame_timings_counter_latest,gtk_frame_timings_complete_counter_latest,gtk_frame_timings_frame_time_us_latest,gtk_frame_timings_predicted_presentation_time_us_latest,gtk_frame_timings_presentation_time_us_latest,gtk_frame_timings_presentation_interval_us_latest,gtk_frame_timings_presentation_interval_us_max,gtk_frame_timings_refresh_interval_us_latest,source,gtk_frame_clock_before_paint_ticks,gtk_frame_clock_update_ticks,gtk_frame_clock_layout_ticks,gtk_frame_clock_paint_ticks,gtk_frame_clock_after_paint_ticks\n",
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
    package_cache_hits: u64,
    #[serde(default)]
    package_cache_misses: u64,
    #[serde(default)]
    package_cache_evictions: u64,
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
    planned_static_image_resources: u64,
    #[serde(default)]
    planned_video_poster_resources: u64,
    #[serde(default)]
    planned_slideshow_image_resources: u64,
    #[serde(default)]
    planned_image_resource_references: u64,
    #[serde(default)]
    planned_unique_image_resources: u64,
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
    slideshow_surfaces: u64,
    #[serde(default)]
    video_surfaces: u64,
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
    zero_copy_evidence: VideoZeroCopyEvidence,
    #[serde(default)]
    position_ms: Option<u64>,
    #[serde(default)]
    duration_ms: Option<u64>,
    #[serde(default)]
    frame_limiter_enabled: bool,
    #[serde(default)]
    frame_limiter_max_fps: Option<u32>,
    #[serde(default)]
    frame_stats: VideoFrameStats,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_render_decisions_as_csv() {
        let response = r##"{"jsonrpc":"2.0","id":1,"result":{"render_sync":{"plans":[{"output_name":"HDMI-A-1","source":"/tmp/poster.jpg","fit":"contain","background":"#000000"}],"video_plans":[{"output_name":"eDP-1","source":"/tmp/loop.webm","poster":"/tmp/poster.jpg","fit":"cover","loop_playback":true,"muted":true,"manifest_max_fps":60,"target_max_fps":24,"start_offset_ms":0}],"slideshow_plans":[{"output_name":"DP-1","sources":["/tmp/a.jpg","/tmp/b.jpg"],"interval_ms":300000,"transition":"none","fit":"cover","target_max_fps":12}],"decisions":[{"output_name":"eDP-1","action":"render","performance":{"mode":"throttled","max_fps":24,"reason":"battery"},"wallpaper":"/tmp/wall.gwpdir"},{"output_name":"HDMI-A-1","action":"remove","performance":{"mode":"paused","max_fps":null,"reason":"fullscreen"},"wallpaper":null},{"output_name":"DP-1","action":"render","performance":{"mode":"throttled","max_fps":12,"reason":"unfocused"},"wallpaper":"/tmp/slides.gwpdir"}]}}}"##;

        let csv = render_decisions_csv(response).unwrap();

        assert_eq!(
            csv,
            "output_name,action,mode,reason,max_fps,wallpaper,plan_kind,source,fit,target_max_fps,muted\n\
             eDP-1,render,throttled,battery,24,/tmp/wall.gwpdir,video,/tmp/loop.webm,cover,24,true\n\
             HDMI-A-1,remove,paused,fullscreen,,,static-image,/tmp/poster.jpg,contain,,\n\
             DP-1,render,throttled,unfocused,12,/tmp/slides.gwpdir,slideshow,/tmp/a.jpg,cover,12,\n"
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
        let response = r##"{"jsonrpc":"2.0","id":1,"result":{"render_sync":{"plans":[],"video_plans":[],"decisions":[]},"telemetry":{"desktop":{"refreshes":7,"refresh_skips":11,"changes":2,"last_refresh_age_ms":42},"render_sync":{"cache_hits":23,"cache_misses":5,"updates_queued":3,"updates_skipped":2,"package_cache_entries":2,"package_cache_max_entries":5,"package_cache_hits":4,"package_cache_misses":3,"package_cache_evictions":1,"archive_cache_entries":8,"archive_cache_max_entries":32,"archive_cache_reuses":6,"archive_cache_extractions":1,"archive_cache_evictions":9,"archive_cache_evictions_latest":2,"archive_cache_eviction_errors":1,"archive_cache_eviction_errors_latest":1,"planned_static_image_resources":2,"planned_video_poster_resources":1,"planned_slideshow_image_resources":3,"planned_image_resource_references":6,"planned_unique_image_resources":5},"adaptive":{"refreshes":5,"refresh_skips":6,"snapshot":{"sample":{"cpu_pressure_some_avg10_x100":123,"memory_pressure_some_avg10_x100":45,"temperature_max_millicelsius":73500,"power_external_online":true,"power_system_battery_present":true,"power_battery_discharging":false,"power_battery_capacity_percent":88,"power_battery_power_microwatts":12000000,"gpu_busy_percent_avg":37,"gpu_busy_percent_max":72,"gpu_busy_sources":["renderD128","card0"]},"active_triggers":[{"metric":"temperature-max-celsius","value_x100":7350,"threshold_x100":7000}]},"action":[{"output_name":"eDP-1","type":"throttle","configured_action":"pause-unfocused","max_fps":15},{"output_name":"HDMI-A-1","type":"pause-dynamic","scope":"dynamic-wallpapers"}]},"renderer":{"output_windows":3,"static_surfaces":2,"slideshow_surfaces":1,"video_surfaces":2,"video_pipelines":2,"video_qos_messages":7,"video_qos_dropped_max":3,"video_gtk_frame_clock_ticks":40,"video_gtk_frame_clock_interval_us_max":22000,"video_gtk_frame_clock_fps_x1000_max":60000,"video_gtk_frame_timings_complete":12,"video_gtk_frame_timings_presentation_interval_us_max":21000,"video_gtk_frame_timings_presentation_time_us_max":150000,"video_gtk_frame_clock_before_paint_ticks":36,"video_gtk_frame_clock_update_ticks":34,"video_gtk_frame_clock_layout_ticks":33,"video_gtk_frame_clock_paint_ticks":32,"video_gtk_frame_clock_after_paint_ticks":40}}}}"##;

        let csv = render_telemetry_csv(response).unwrap();

        assert_eq!(
            csv,
            "desktop_refreshes,desktop_refresh_skips,desktop_changes,last_desktop_refresh_age_ms,render_sync_cache_hits,render_sync_cache_misses,render_sync_updates_queued,render_sync_updates_skipped,render_sync_package_cache_entries,render_sync_package_cache_max_entries,render_sync_package_cache_hits,render_sync_package_cache_misses,render_sync_package_cache_evictions,render_sync_archive_cache_entries,render_sync_archive_cache_max_entries,render_sync_archive_cache_reuses,render_sync_archive_cache_extractions,render_sync_archive_cache_evictions,render_sync_archive_cache_evictions_latest,render_sync_archive_cache_eviction_errors,render_sync_archive_cache_eviction_errors_latest,render_sync_planned_static_image_resources,render_sync_planned_video_poster_resources,render_sync_planned_slideshow_image_resources,render_sync_planned_image_resource_references,render_sync_planned_unique_image_resources,adaptive_refreshes,adaptive_refresh_skips,adaptive_active_triggers,cpu_pressure_some_avg10_x100,memory_pressure_some_avg10_x100,temperature_max_millicelsius,power_external_online,power_system_battery_present,power_battery_discharging,power_battery_capacity_percent,power_battery_power_microwatts,gpu_busy_percent_avg,gpu_busy_percent_max,gpu_busy_sources,adaptive_action_types,adaptive_action_scopes,adaptive_action_configured_actions,adaptive_action_max_fps,renderer_output_windows,renderer_static_surfaces,renderer_slideshow_surfaces,renderer_video_surfaces,renderer_video_pipelines,renderer_video_qos_messages,renderer_video_qos_dropped_max,renderer_video_gtk_frame_clock_ticks,renderer_video_gtk_frame_clock_interval_us_max,renderer_video_gtk_frame_clock_fps_x1000_max,renderer_video_gtk_frame_timings_complete,renderer_video_gtk_frame_timings_presentation_interval_us_max,renderer_video_gtk_frame_timings_presentation_time_us_max,renderer_video_gtk_frame_clock_before_paint_ticks,renderer_video_gtk_frame_clock_update_ticks,renderer_video_gtk_frame_clock_layout_ticks,renderer_video_gtk_frame_clock_paint_ticks,renderer_video_gtk_frame_clock_after_paint_ticks\n\
             7,11,2,42,23,5,3,2,2,5,4,3,1,8,32,6,1,9,2,1,1,2,1,3,6,5,5,6,1,123,45,73500,true,true,false,88,12000000,37,72,card0|renderD128,pause-dynamic|throttle,dynamic-wallpapers,pause-unfocused,15,3,2,1,2,2,7,3,40,22000,60000,12,21000,150000,36,34,33,32,40\n"
        );
    }

    #[test]
    fn formats_video_runtime_as_csv() {
        let response = r##"{"jsonrpc":"2.0","id":1,"result":{"render_sync":{"plans":[],"video_plans":[],"decisions":[]},"renderer_runtime":{"video_pipelines":[{"output_name":"eDP-1","source":"/tmp/loop.mp4","mode":"active","gst_state":"Playing","loop_playback":true,"muted":true,"target_max_fps":24,"frame_limiter_enabled":true,"frame_limiter_max_fps":24,"position_ms":1234,"duration_ms":60000,"frame_stats":{"qos_messages":3,"qos_stats_format":"buffers","qos_processed_max":120,"qos_dropped_max":2,"qos_jitter_ns_latest":-2000,"qos_jitter_ns_abs_max":7000,"qos_proportion_x1000_latest":995,"gtk_frame_clock_ticks":9,"gtk_frame_clock_before_paint_ticks":8,"gtk_frame_clock_update_ticks":7,"gtk_frame_clock_layout_ticks":6,"gtk_frame_clock_paint_ticks":5,"gtk_frame_clock_after_paint_ticks":9,"gtk_frame_clock_counter_latest":300,"gtk_frame_clock_time_us_latest":5000000,"gtk_frame_clock_interval_us_latest":16667,"gtk_frame_clock_interval_us_max":20000,"gtk_frame_clock_fps_x1000_latest":59940,"gtk_frame_clock_refresh_interval_us_latest":16667,"gtk_frame_clock_predicted_presentation_time_us_latest":5016667,"gtk_frame_timings_observed":8,"gtk_frame_timings_complete":7,"gtk_frame_timings_counter_latest":300,"gtk_frame_timings_complete_counter_latest":299,"gtk_frame_timings_frame_time_us_latest":5000000,"gtk_frame_timings_predicted_presentation_time_us_latest":5016667,"gtk_frame_timings_presentation_time_us_latest":5017000,"gtk_frame_timings_presentation_interval_us_latest":16667,"gtk_frame_timings_presentation_interval_us_max":20000,"gtk_frame_timings_refresh_interval_us_latest":16667},"decoder_policy":"hardware-preferred","decoder_policy_status":"software-fallback","actual_decoders":["dav1ddec"],"actual_decoder_reports":[{"element":"dav1ddec","class":"software"}],"caps_reports":[{"element":"gtk4paintablesink0","pad":"sink","direction":"sink","caps":"video/x-raw(memory:DMABuf)","memory_features":["memory:DMABuf"],"structures":[{"media_type":"video/x-raw","features":["memory:DMABuf"]}]},{"element":"videoconvert0","pad":"src","direction":"src","caps":"video/x-raw","memory_features":[],"structures":[{"media_type":"video/x-raw","features":[]}]}],"zero_copy_evidence":{"level":"sink-dmabuf-caps","notes":["sink-side DMABuf caps observed"]}}]}}}"##;

        let csv = render_video_runtime_csv(response).unwrap();

        assert_eq!(
            csv,
            "output_name,mode,gst_state,decoder_policy,decoder_policy_status,actual_decoders,decoder_classes,caps_report_count,memory_features,sink_memory_features,zero_copy_evidence_level,zero_copy_evidence_notes,media_types,caps_paths,position_ms,duration_ms,frame_limiter_enabled,frame_limiter_max_fps,qos_messages,qos_processed_max,qos_dropped_max,qos_stats_format,qos_jitter_ns_latest,qos_jitter_ns_abs_max,qos_proportion_x1000_latest,gtk_frame_clock_ticks,gtk_frame_clock_counter_latest,gtk_frame_clock_time_us_latest,gtk_frame_clock_interval_us_latest,gtk_frame_clock_interval_us_max,gtk_frame_clock_fps_x1000_latest,gtk_frame_clock_refresh_interval_us_latest,gtk_frame_clock_predicted_presentation_time_us_latest,gtk_frame_timings_observed,gtk_frame_timings_complete,gtk_frame_timings_counter_latest,gtk_frame_timings_complete_counter_latest,gtk_frame_timings_frame_time_us_latest,gtk_frame_timings_predicted_presentation_time_us_latest,gtk_frame_timings_presentation_time_us_latest,gtk_frame_timings_presentation_interval_us_latest,gtk_frame_timings_presentation_interval_us_max,gtk_frame_timings_refresh_interval_us_latest,source,gtk_frame_clock_before_paint_ticks,gtk_frame_clock_update_ticks,gtk_frame_clock_layout_ticks,gtk_frame_clock_paint_ticks,gtk_frame_clock_after_paint_ticks\n\
             eDP-1,active,Playing,hardware-preferred,software-fallback,dav1ddec,software,2,memory:DMABuf,memory:DMABuf,sink-dmabuf-caps,sink-side DMABuf caps observed,video/x-raw,gtk4paintablesink0:sink:sink|videoconvert0:src:src,1234,60000,true,24,3,120,2,buffers,-2000,7000,995,9,300,5000000,16667,20000,59940,16667,5016667,8,7,300,299,5000000,5016667,5017000,16667,20000,16667,/tmp/loop.mp4,8,7,6,5,9\n"
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
