use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

use gilder::config::{ApplicationPaths, GilderConfig};
use gilder::desktop::DesktopSnapshot;
use gilder::ipc::RequestMethod;
use gilder::state::AppState;
use serde_json::json;

fn main() {
    if let Err(err) = run() {
        eprintln!("gilderd: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let paths = ApplicationPaths::from_env().map_err(|err| err.to_string())?;
    let config = GilderConfig::load(&paths.config_file)
        .map_err(|err| format!("failed to load {}: {err}", paths.config_file.display()))?;
    let state = gilder::state::load_state(&paths.state_file)
        .map_err(|err| format!("failed to load {}: {err}", paths.state_file.display()))?;
    let mut context = DaemonContext {
        paths,
        config,
        state,
        desktop: DesktopSnapshot::placeholder(),
    };

    let socket = gilder::ipc::runtime_socket_path().ok_or_else(|| {
        "XDG_RUNTIME_DIR is not set; cannot create Wayland-session IPC".to_owned()
    })?;

    prepare_socket_parent(&socket)?;
    if socket.exists() {
        if UnixStream::connect(&socket).is_ok() {
            return Err(format!(
                "another gilderd instance is already listening on {}",
                socket.display()
            ));
        }
        fs::remove_file(&socket)
            .map_err(|err| format!("failed to remove stale socket {}: {err}", socket.display()))?;
    }

    let listener = UnixListener::bind(&socket)
        .map_err(|err| format!("failed to bind {}: {err}", socket.display()))?;
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o600)).map_err(|err| {
        format!(
            "failed to set socket permissions {}: {err}",
            socket.display()
        )
    })?;
    eprintln!("gilderd: listening on {}", socket.display());

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => handle_client(stream, &mut context)?,
            Err(err) => eprintln!("gilderd: failed to accept client: {err}"),
        }
    }

    Ok(())
}

fn prepare_socket_parent(socket: &PathBuf) -> Result<(), String> {
    let parent = socket
        .parent()
        .ok_or_else(|| format!("invalid socket path {}", socket.display()))?;
    fs::create_dir_all(parent)
        .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
        .map_err(|err| format!("failed to set permissions on {}: {err}", parent.display()))
}

fn handle_client(mut stream: UnixStream, context: &mut DaemonContext) -> Result<(), String> {
    let mut request = String::new();
    {
        let mut reader = BufReader::new(&stream);
        reader
            .read_line(&mut request)
            .map_err(|err| format!("failed to read IPC request: {err}"))?;
    }

    let response = match gilder::ipc::parse_request(&request) {
        Ok(request) => handle_ipc_request(request, context),
        Err(err) => gilder::ipc::error_response(err.id.as_ref(), err.code, &err.message),
    };

    stream
        .write_all(response.as_bytes())
        .and_then(|_| stream.write_all(b"\n"))
        .map_err(|err| format!("failed to write IPC response: {err}"))
}

struct DaemonContext {
    paths: ApplicationPaths,
    config: GilderConfig,
    state: AppState,
    desktop: DesktopSnapshot,
}

fn handle_ipc_request(request: gilder::ipc::IpcRequest, context: &mut DaemonContext) -> String {
    match request.method {
        RequestMethod::Ping { protocol } => gilder::ipc::success_response(
            &request.id,
            json!({
                "ok": true,
                "daemon": "gilderd",
                "protocol": gilder::ipc::PROTOCOL_VERSION,
                "client_protocol": protocol,
            }),
        ),
        RequestMethod::Status => gilder::ipc::success_response(
            &request.id,
            json!({
                "state": "idle",
                "config_file": context.paths.config_file,
                "state_file": context.paths.state_file,
                "outputs": output_reports(context),
                "persisted_state": context.state,
                "renderer": "not-implemented",
            }),
        ),
        RequestMethod::Outputs => gilder::ipc::success_response(
            &request.id,
            json!({ "outputs": output_reports(context) }),
        ),
        RequestMethod::PropertiesGet { output, key } => {
            let result = match key {
                Some(key) => {
                    let value = context.state.get_property(output.as_deref(), &key);
                    json!({
                        "output": output,
                        "key": key,
                        "found": value.is_some(),
                        "value": value,
                    })
                }
                None => json!({
                    "output": output,
                    "properties": context.state.properties(output.as_deref()),
                }),
            };
            gilder::ipc::success_response(&request.id, result)
        }
        RequestMethod::PropertiesSet { output, key, value } => {
            context
                .state
                .set_property(output.as_deref(), key.clone(), value.clone());
            persist_or_error(&request.id, context).unwrap_or_else(|| {
                gilder::ipc::success_response(
                    &request.id,
                    json!({
                        "accepted": true,
                        "method": "properties.set",
                        "output": output,
                        "key": key,
                        "value": value,
                    }),
                )
            })
        }
        RequestMethod::PropertiesUnset { output, key } => {
            let removed = context.state.unset_property(output.as_deref(), &key);
            persist_or_error(&request.id, context).unwrap_or_else(|| {
                gilder::ipc::success_response(
                    &request.id,
                    json!({
                        "accepted": true,
                        "method": "properties.unset",
                        "output": output,
                        "key": key,
                        "removed": removed,
                    }),
                )
            })
        }
        RequestMethod::Set { wallpaper, output } => {
            context
                .state
                .set_wallpaper(output.as_deref(), wallpaper.clone());
            persist_or_error(&request.id, context).unwrap_or_else(|| {
                renderer_placeholder_response(
                    &request.id,
                    "set",
                    json!({
                        "wallpaper": wallpaper,
                        "output": output,
                    }),
                )
            })
        }
        RequestMethod::Pause { output } => {
            context.state.pause(output.as_deref(), true);
            persist_or_error(&request.id, context).unwrap_or_else(|| {
                renderer_placeholder_response(
                    &request.id,
                    "pause",
                    json!({
                        "output": output,
                    }),
                )
            })
        }
        RequestMethod::Resume { output } => {
            context.state.pause(output.as_deref(), false);
            persist_or_error(&request.id, context).unwrap_or_else(|| {
                renderer_placeholder_response(
                    &request.id,
                    "resume",
                    json!({
                        "output": output,
                    }),
                )
            })
        }
        RequestMethod::Stop { output } => {
            context.state.stop(output.as_deref());
            persist_or_error(&request.id, context).unwrap_or_else(|| {
                renderer_placeholder_response(
                    &request.id,
                    "stop",
                    json!({
                        "output": output,
                    }),
                )
            })
        }
    }
}

fn persist_or_error(id: &serde_json::Value, context: &DaemonContext) -> Option<String> {
    gilder::state::save_state(&context.paths.state_file, &context.state)
        .err()
        .map(|err| gilder::ipc::error_response(Some(id), "internal_error", &err.to_string()))
}

fn output_reports(context: &DaemonContext) -> Vec<serde_json::Value> {
    let mut names: Vec<String> = context
        .desktop
        .outputs
        .iter()
        .map(|output| output.name.clone())
        .chain(context.state.outputs.keys().cloned())
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
            let performance = gilder::policy::decide_performance(
                &context.config.performance,
                &context.desktop,
                desktop_output,
                &state,
            );
            json!({
                "name": name,
                "desktop": desktop_output,
                "state": state,
                "performance": performance,
            })
        })
        .collect()
}

fn renderer_placeholder_response(
    id: &serde_json::Value,
    accepted_method: &str,
    accepted_params: serde_json::Value,
) -> String {
    gilder::ipc::success_response(
        id,
        json!({
            "accepted": true,
            "method": accepted_method,
            "params": accepted_params,
            "note": "renderer is not implemented yet",
        }),
    )
}
