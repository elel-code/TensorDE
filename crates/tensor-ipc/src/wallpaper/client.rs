use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use serde::Deserialize;

#[path = "client/telemetry_csv.rs"]
mod telemetry_csv;

use telemetry_csv::render_telemetry_csv;

pub fn run(args: Vec<String>) -> Result<(), String> {
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        println!("{}", super::help_text());
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

    let socket = env::var_os("TENSOR_WALLPAPER_SOCKET")
        .map(PathBuf::from)
        .or_else(super::runtime_socket_path)
        .ok_or_else(|| {
            "XDG_RUNTIME_DIR is not set; pass TENSOR_WALLPAPER_SOCKET=/path/to/socket".to_owned()
        })?;

    let mut stream = UnixStream::connect(&socket)
        .map_err(|err| format!("failed to connect to {}: {err}", socket.display()))?;

    let request = command.to_json_line();
    stream
        .write_all(request.as_bytes())
        .and_then(|_| stream.write_all(b"\n"))
        .map_err(|err| format!("failed to send request: {err}"))?;

    if matches!(command, super::ClientCommand::Watch) {
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
    command: super::ClientCommand,
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
            command: super::ClientCommand::Status,
            format: ResponseFormat::DecisionsCsv,
            response_file: None,
        }),
        [cmd, format, from_file, path]
            if cmd == "status" && format == "--decisions-csv" && from_file == "--from-file" =>
        {
            Ok(Invocation {
                command: super::ClientCommand::Status,
                format: ResponseFormat::DecisionsCsv,
                response_file: Some(PathBuf::from(path)),
            })
        }
        [cmd, format] if cmd == "status" && format == "--telemetry-csv" => Ok(Invocation {
            command: super::ClientCommand::Status,
            format: ResponseFormat::TelemetryCsv,
            response_file: None,
        }),
        [cmd, format, from_file, path]
            if cmd == "status" && format == "--telemetry-csv" && from_file == "--from-file" =>
        {
            Ok(Invocation {
                command: super::ClientCommand::Status,
                format: ResponseFormat::TelemetryCsv,
                response_file: Some(PathBuf::from(path)),
            })
        }
        [cmd, from_file, path] if cmd == "status" && from_file == "--from-file" => Ok(Invocation {
            command: super::ClientCommand::Status,
            format: ResponseFormat::Json,
            response_file: Some(PathBuf::from(path)),
        }),
        _ => Ok(Invocation {
            command: super::parse_client_args(args)?,
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

#[cfg(test)]
#[path = "client/tests.rs"]
mod tests;
