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

    let mut csv = String::from("output_name,action,mode,reason,max_fps,wallpaper\n");
    for decision in result.render_sync.decisions {
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
        ];
        csv.push_str(&row.join(","));
        csv.push('\n');
    }
    Ok(csv)
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
}

#[derive(Debug, Deserialize)]
struct RenderSync {
    #[serde(default)]
    decisions: Vec<RenderDecision>,
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
mod tests {
    use super::*;

    #[test]
    fn formats_render_decisions_as_csv() {
        let response = r##"{"jsonrpc":"2.0","id":1,"result":{"render_sync":{"decisions":[{"output_name":"eDP-1","action":"render","performance":{"mode":"throttled","max_fps":24,"reason":"battery"},"wallpaper":"/tmp/wall.gwpdir"},{"output_name":"HDMI-A-1","action":"remove","performance":{"mode":"paused","max_fps":null,"reason":"fullscreen"},"wallpaper":null}]}}}"##;

        let csv = render_decisions_csv(response).unwrap();

        assert_eq!(
            csv,
            "output_name,action,mode,reason,max_fps,wallpaper\n\
             eDP-1,render,throttled,battery,24,/tmp/wall.gwpdir\n\
             HDMI-A-1,remove,paused,fullscreen,,\n"
        );
    }

    #[test]
    fn escapes_csv_cells() {
        let response = r##"{"jsonrpc":"2.0","id":1,"result":{"render_sync":{"decisions":[{"output_name":"DP,1","action":"render","performance":{"mode":"active","max_fps":60,"reason":"interactive"},"wallpaper":"/tmp/a\"b.gwpdir"}]}}}"##;

        let csv = render_decisions_csv(response).unwrap();

        assert_eq!(
            csv,
            "output_name,action,mode,reason,max_fps,wallpaper\n\
             \"DP,1\",render,active,interactive,60,\"/tmp/a\"\"b.gwpdir\"\n"
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
}
