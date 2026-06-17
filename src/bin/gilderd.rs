use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

use gilder::ipc::RequestMethod;
use serde_json::json;

fn main() {
    if let Err(err) = run() {
        eprintln!("gilderd: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let socket = gilder::ipc::runtime_socket_path().ok_or_else(|| {
        "XDG_RUNTIME_DIR is not set; cannot create Wayland-session IPC".to_owned()
    })?;

    prepare_socket_parent(&socket)?;
    if socket.exists() {
        fs::remove_file(&socket)
            .map_err(|err| format!("failed to replace stale socket {}: {err}", socket.display()))?;
    }

    let listener = UnixListener::bind(&socket)
        .map_err(|err| format!("failed to bind {}: {err}", socket.display()))?;
    eprintln!("gilderd: listening on {}", socket.display());

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => handle_client(stream)?,
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
        .map_err(|err| format!("failed to create {}: {err}", parent.display()))
}

fn handle_client(mut stream: UnixStream) -> Result<(), String> {
    let mut request = String::new();
    {
        let mut reader = BufReader::new(&stream);
        reader
            .read_line(&mut request)
            .map_err(|err| format!("failed to read IPC request: {err}"))?;
    }

    let response = match gilder::ipc::parse_request(&request) {
        Ok(request) => handle_ipc_request(request),
        Err(err) => gilder::ipc::error_response(err.id.as_ref(), err.code, &err.message),
    };

    stream
        .write_all(response.as_bytes())
        .and_then(|_| stream.write_all(b"\n"))
        .map_err(|err| format!("failed to write IPC response: {err}"))
}

fn handle_ipc_request(request: gilder::ipc::IpcRequest) -> String {
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
                "outputs": [],
                "renderer": "not-implemented",
            }),
        ),
        RequestMethod::Set { wallpaper, output } => renderer_placeholder_response(
            &request.id,
            "set",
            json!({
                "wallpaper": wallpaper,
                "output": output,
            }),
        ),
        RequestMethod::Pause { output } => renderer_placeholder_response(
            &request.id,
            "pause",
            json!({
                "output": output,
            }),
        ),
        RequestMethod::Resume { output } => renderer_placeholder_response(
            &request.id,
            "resume",
            json!({
                "output": output,
            }),
        ),
        RequestMethod::Stop { output } => renderer_placeholder_response(
            &request.id,
            "stop",
            json!({
                "output": output,
            }),
        ),
    }
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
