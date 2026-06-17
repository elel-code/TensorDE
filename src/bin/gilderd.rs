use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use gilder::config::{ApplicationPaths, GilderConfig};
use gilder::ipc::RequestMethod;
use gilder::state::AppState;
use serde_json::{Value, json};

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
    let desktop = gilder::desktop::adapters::read_desktop_snapshot(&config.adapters);
    let context = DaemonContext {
        paths,
        config,
        state,
        desktop,
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

    let runtime = Arc::new(DaemonRuntime::new(context));
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let runtime = Arc::clone(&runtime);
                thread::spawn(move || {
                    if let Err(err) = handle_client(stream, runtime) {
                        eprintln!("gilderd: client error: {err}");
                    }
                });
            }
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

fn handle_client(mut stream: UnixStream, runtime: Arc<DaemonRuntime>) -> Result<(), String> {
    let mut request = String::new();
    {
        let mut reader = BufReader::new(&stream);
        reader
            .read_line(&mut request)
            .map_err(|err| format!("failed to read IPC request: {err}"))?;
    }

    let request = match gilder::ipc::parse_request(&request) {
        Ok(request) => request,
        Err(err) => {
            let response = gilder::ipc::error_response(err.id.as_ref(), err.code, &err.message);
            return write_line(&mut stream, &response);
        }
    };

    match request.method {
        RequestMethod::Watch { include_snapshot } => {
            handle_watch_client(stream, request.id, include_snapshot, runtime)
        }
        method => {
            let outcome = {
                let mut context = runtime.lock_context()?;
                handle_ipc_request(
                    gilder::ipc::IpcRequest {
                        id: request.id,
                        method,
                    },
                    &mut context,
                )
            };
            write_line(&mut stream, &outcome.response)?;
            if let Some(event) = outcome.event {
                runtime.watchers.broadcast("state.changed", event);
            }
            Ok(())
        }
    }
}

fn handle_watch_client(
    mut stream: UnixStream,
    id: Value,
    include_snapshot: bool,
    runtime: Arc<DaemonRuntime>,
) -> Result<(), String> {
    let receiver = runtime.watchers.subscribe()?;
    let response = gilder::ipc::success_response(
        &id,
        json!({
            "subscribed": true,
            "protocol": gilder::ipc::PROTOCOL_VERSION,
            "events": ["snapshot", "state.changed"],
        }),
    );
    write_line(&mut stream, &response)?;

    if include_snapshot {
        let event = {
            let context = runtime.lock_context()?;
            snapshot_event(&context)
        };
        let line = runtime.watchers.event_line("snapshot", event);
        write_line(&mut stream, &line)?;
    }

    for line in receiver {
        if write_line(&mut stream, &line).is_err() {
            break;
        }
    }
    Ok(())
}

fn write_line(stream: &mut UnixStream, line: &str) -> Result<(), String> {
    stream
        .write_all(line.as_bytes())
        .and_then(|_| stream.write_all(b"\n"))
        .map_err(|err| format!("failed to write IPC response: {err}"))
}

struct DaemonRuntime {
    context: Mutex<DaemonContext>,
    watchers: WatchHub,
}

impl DaemonRuntime {
    fn new(context: DaemonContext) -> Self {
        Self {
            context: Mutex::new(context),
            watchers: WatchHub::new(),
        }
    }

    fn lock_context(&self) -> Result<std::sync::MutexGuard<'_, DaemonContext>, String> {
        self.context
            .lock()
            .map_err(|_| "daemon context lock poisoned".to_owned())
    }
}

struct WatchHub {
    next_sequence: AtomicU64,
    subscribers: Mutex<Vec<mpsc::Sender<String>>>,
}

impl WatchHub {
    fn new() -> Self {
        Self {
            next_sequence: AtomicU64::new(1),
            subscribers: Mutex::new(Vec::new()),
        }
    }

    fn subscribe(&self) -> Result<mpsc::Receiver<String>, String> {
        let (sender, receiver) = mpsc::channel();
        self.subscribers
            .lock()
            .map_err(|_| "watch subscriber lock poisoned".to_owned())?
            .push(sender);
        Ok(receiver)
    }

    fn broadcast(&self, event_type: &str, payload: Value) {
        let line = self.event_line(event_type, payload);
        let Ok(mut subscribers) = self.subscribers.lock() else {
            eprintln!("gilderd: watch subscriber lock poisoned");
            return;
        };
        subscribers.retain(|subscriber| subscriber.send(line.clone()).is_ok());
    }

    fn event_line(&self, event_type: &str, payload: Value) -> String {
        let sequence = self.next_sequence.fetch_add(1, Ordering::Relaxed);
        gilder::ipc::event_notification(sequence, event_type, payload)
    }
}

struct DaemonContext {
    paths: ApplicationPaths,
    config: GilderConfig,
    state: AppState,
    desktop: gilder::desktop::DesktopSnapshot,
}

struct IpcOutcome {
    response: String,
    event: Option<Value>,
}

impl IpcOutcome {
    fn response(response: String) -> Self {
        Self {
            response,
            event: None,
        }
    }

    fn with_event(response: String, event: Value) -> Self {
        Self {
            response,
            event: Some(event),
        }
    }
}

fn handle_ipc_request(request: gilder::ipc::IpcRequest, context: &mut DaemonContext) -> IpcOutcome {
    match request.method {
        RequestMethod::Ping { protocol } => IpcOutcome::response(gilder::ipc::success_response(
            &request.id,
            json!({
                "ok": true,
                "daemon": "gilderd",
                "protocol": gilder::ipc::PROTOCOL_VERSION,
                "client_protocol": protocol,
            }),
        )),
        RequestMethod::Status => {
            refresh_desktop(context);
            IpcOutcome::response(gilder::ipc::success_response(
                &request.id,
                json!({
                    "state": "idle",
                    "config_file": context.paths.config_file,
                    "state_file": context.paths.state_file,
                    "desktop": context.desktop,
                    "outputs": output_reports(context),
                    "persisted_state": context.state,
                    "renderer": renderer_name(),
                }),
            ))
        }
        RequestMethod::Outputs => {
            refresh_desktop(context);
            IpcOutcome::response(gilder::ipc::success_response(
                &request.id,
                json!({ "desktop": context.desktop, "outputs": output_reports(context) }),
            ))
        }
        RequestMethod::Watch { .. } => IpcOutcome::response(gilder::ipc::error_response(
            Some(&request.id),
            "bad_request",
            "watch must be handled as a streaming request",
        )),
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
            IpcOutcome::response(gilder::ipc::success_response(&request.id, result))
        }
        RequestMethod::PropertiesSet { output, key, value } => {
            context
                .state
                .set_property(output.as_deref(), key.clone(), value.clone());
            if let Some(response) = persist_or_error(&request.id, context) {
                IpcOutcome::response(response)
            } else {
                refresh_desktop(context);
                let response = gilder::ipc::success_response(
                    &request.id,
                    json!({
                        "accepted": true,
                        "method": "properties.set",
                        "output": output,
                        "key": key,
                        "value": value,
                    }),
                );
                let event = state_changed_event("properties.set", output.as_deref(), context);
                IpcOutcome::with_event(response, event)
            }
        }
        RequestMethod::PropertiesUnset { output, key } => {
            let removed = context.state.unset_property(output.as_deref(), &key);
            if let Some(response) = persist_or_error(&request.id, context) {
                IpcOutcome::response(response)
            } else {
                refresh_desktop(context);
                let response = gilder::ipc::success_response(
                    &request.id,
                    json!({
                        "accepted": true,
                        "method": "properties.unset",
                        "output": output,
                        "key": key,
                        "removed": removed,
                    }),
                );
                let event = state_changed_event("properties.unset", output.as_deref(), context);
                IpcOutcome::with_event(response, event)
            }
        }
        RequestMethod::Set { wallpaper, output } => {
            context
                .state
                .set_wallpaper(output.as_deref(), wallpaper.clone());
            if let Some(response) = persist_or_error(&request.id, context) {
                IpcOutcome::response(response)
            } else {
                refresh_desktop(context);
                let response = renderer_placeholder_response(
                    &request.id,
                    "set",
                    json!({
                        "wallpaper": wallpaper,
                        "output": output,
                    }),
                );
                let event = state_changed_event("set", output.as_deref(), context);
                IpcOutcome::with_event(response, event)
            }
        }
        RequestMethod::Pause { output } => {
            context.state.pause(output.as_deref(), true);
            if let Some(response) = persist_or_error(&request.id, context) {
                IpcOutcome::response(response)
            } else {
                refresh_desktop(context);
                let response = renderer_placeholder_response(
                    &request.id,
                    "pause",
                    json!({
                        "output": output,
                    }),
                );
                let event = state_changed_event("pause", output.as_deref(), context);
                IpcOutcome::with_event(response, event)
            }
        }
        RequestMethod::Resume { output } => {
            context.state.pause(output.as_deref(), false);
            if let Some(response) = persist_or_error(&request.id, context) {
                IpcOutcome::response(response)
            } else {
                refresh_desktop(context);
                let response = renderer_placeholder_response(
                    &request.id,
                    "resume",
                    json!({
                        "output": output,
                    }),
                );
                let event = state_changed_event("resume", output.as_deref(), context);
                IpcOutcome::with_event(response, event)
            }
        }
        RequestMethod::Stop { output } => {
            context.state.stop(output.as_deref());
            if let Some(response) = persist_or_error(&request.id, context) {
                IpcOutcome::response(response)
            } else {
                refresh_desktop(context);
                let response = renderer_placeholder_response(
                    &request.id,
                    "stop",
                    json!({
                        "output": output,
                    }),
                );
                let event = state_changed_event("stop", output.as_deref(), context);
                IpcOutcome::with_event(response, event)
            }
        }
    }
}

fn persist_or_error(id: &serde_json::Value, context: &DaemonContext) -> Option<String> {
    gilder::state::save_state(&context.paths.state_file, &context.state)
        .err()
        .map(|err| gilder::ipc::error_response(Some(id), "internal_error", &err.to_string()))
}

fn refresh_desktop(context: &mut DaemonContext) {
    context.desktop = gilder::desktop::adapters::read_desktop_snapshot(&context.config.adapters);
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

fn snapshot_event(context: &DaemonContext) -> Value {
    json!({
        "outputs": output_reports(context),
        "persisted_state": context.state,
        "renderer": renderer_name(),
    })
}

fn state_changed_event(action: &str, output: Option<&str>, context: &DaemonContext) -> Value {
    json!({
        "action": action,
        "output": output,
        "outputs": output_reports(context),
        "persisted_state": context.state,
    })
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

fn renderer_name() -> &'static str {
    if cfg!(feature = "gtk-renderer") {
        "gtk-layer-shell-static"
    } else {
        "not-implemented"
    }
}
