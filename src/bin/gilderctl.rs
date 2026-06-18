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
        "desktop_refreshes,desktop_refresh_skips,desktop_changes,last_desktop_refresh_age_ms,render_sync_cache_hits,render_sync_cache_misses,render_sync_updates_queued,render_sync_updates_skipped,adaptive_refreshes,adaptive_refresh_skips,adaptive_active_triggers,cpu_pressure_some_avg10_x100,memory_pressure_some_avg10_x100,temperature_max_millicelsius\n",
    );
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
        telemetry
            .adaptive
            .snapshot
            .sample
            .as_ref()
            .and_then(|sample| sample.memory_pressure_some_avg10_x100)
            .map(|value| value.to_string())
            .unwrap_or_default(),
        telemetry
            .adaptive
            .snapshot
            .sample
            .as_ref()
            .and_then(|sample| sample.temperature_max_millicelsius)
            .map(|value| value.to_string())
            .unwrap_or_default(),
    ];
    csv.push_str(&row.join(","));
    csv.push('\n');
    Ok(csv)
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
}

#[derive(Debug, Default, Deserialize)]
struct AdaptiveTelemetry {
    #[serde(default)]
    refreshes: u64,
    #[serde(default)]
    refresh_skips: u64,
    #[serde(default)]
    snapshot: AdaptiveSnapshot,
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
        let response = r##"{"jsonrpc":"2.0","id":1,"result":{"render_sync":{"plans":[],"video_plans":[],"decisions":[]},"telemetry":{"desktop":{"refreshes":7,"refresh_skips":11,"changes":2,"last_refresh_age_ms":42},"render_sync":{"cache_hits":23,"cache_misses":5,"updates_queued":3,"updates_skipped":2},"adaptive":{"refreshes":5,"refresh_skips":6,"snapshot":{"sample":{"cpu_pressure_some_avg10_x100":123,"memory_pressure_some_avg10_x100":45,"temperature_max_millicelsius":73500},"active_triggers":[{"metric":"temperature-max-celsius","value_x100":7350,"threshold_x100":7000}]}}}}}"##;

        let csv = render_telemetry_csv(response).unwrap();

        assert_eq!(
            csv,
            "desktop_refreshes,desktop_refresh_skips,desktop_changes,last_desktop_refresh_age_ms,render_sync_cache_hits,render_sync_cache_misses,render_sync_updates_queued,render_sync_updates_skipped,adaptive_refreshes,adaptive_refresh_skips,adaptive_active_triggers,cpu_pressure_some_avg10_x100,memory_pressure_some_avg10_x100,temperature_max_millicelsius\n\
             7,11,2,42,23,5,3,2,5,6,1,123,45,73500\n"
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
